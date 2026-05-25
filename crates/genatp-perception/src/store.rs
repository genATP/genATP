//! Content-addressed snapshot store backed by SQLite + zstd.
//!
//! Every blob is keyed by its [BLAKE3] hash, which doubles as both the
//! identity check (deduplicates identical snapshots) and the integrity
//! check (a corrupt row is detected on read by re-hashing).
//!
//! Storage shape:
//!
//! ```sql
//! CREATE TABLE snapshots (
//!   hash       BLOB PRIMARY KEY,   -- 32 bytes BLAKE3
//!   payload    BLOB NOT NULL,      -- zstd-compressed JSON
//!   bytes      INTEGER NOT NULL,   -- compressed size (for analytics)
//!   raw_bytes  INTEGER NOT NULL,   -- decompressed size
//!   ref_count  INTEGER NOT NULL,   -- how many cassettes reference this row
//!   created_at INTEGER NOT NULL    -- unix millis
//! );
//! ```
//!
//! [BLAKE3]: https://github.com/BLAKE3-team/BLAKE3-specs

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{PerceptionError, Result};
use crate::snapshot::CompressedSnapshot;

/// 32-byte BLAKE3 content hash of a JSON-serialised snapshot.
///
/// `Display` produces lowercase hex, which is what the cassette format and
/// CLI use as the canonical text form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Hash a serialisable snapshot. Uses canonical JSON (sorted keys via
    /// `serde_json` default; we don't promise full JCS yet — Phase 7 brings
    /// that in for cassettes).
    ///
    /// # Errors
    /// Surfaces serialisation failures.
    pub fn of(snapshot: &CompressedSnapshot) -> Result<Self> {
        let bytes = serde_json::to_vec(snapshot)?;
        Ok(Self::of_bytes(&bytes))
    }

    /// Hash a raw byte payload directly. Used after loading a row when we
    /// re-verify integrity.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Lowercase hex (64 chars).
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex (lower or uppercase).
    ///
    /// # Errors
    /// `hex::FromHexError` if the string is not 64 hex chars.
    pub fn from_hex(s: &str) -> std::result::Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(Self(out))
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Minimum + recommended zstd compression levels. Level 3 hits ~3x compression
/// on representative snapshot JSON in benchmarks, with sub-millisecond encode.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// Snapshot CAS handle. Wraps a single SQLite connection — the store is
/// **not** thread-safe; callers that need concurrent access should pool
/// connections or wrap in a `Mutex`.
pub struct SnapshotStore {
    conn: Connection,
    path: PathBuf,
}

impl SnapshotStore {
    /// Open or create a store at `path`. Pass `:memory:` for an in-memory
    /// store (tests).
    ///
    /// # Errors
    /// Surfaces SQLite open / DDL failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|source| PerceptionError::StoreIo {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let conn = Connection::open(&path_buf)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS snapshots (
                hash        BLOB PRIMARY KEY,
                payload     BLOB NOT NULL,
                bytes       INTEGER NOT NULL,
                raw_bytes   INTEGER NOT NULL,
                ref_count   INTEGER NOT NULL DEFAULT 1,
                created_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS snapshots_created_at ON snapshots (created_at);
            ",
        )?;

        Ok(Self {
            conn,
            path: path_buf,
        })
    }

    /// Path the store is bound to (`:memory:` for in-memory).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Insert (or refcount-bump) a snapshot. Idempotent: storing the same
    /// snapshot twice returns the same hash both times and bumps `ref_count`
    /// rather than re-encoding the payload.
    ///
    /// # Errors
    /// Surfaces serialisation, zstd, or SQLite errors.
    pub fn put(&self, snapshot: &CompressedSnapshot) -> Result<ContentHash> {
        let json = serde_json::to_vec(snapshot)?;
        let hash = ContentHash::of_bytes(&json);

        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM snapshots WHERE hash = ?1",
                params![&hash.0[..]],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            self.conn.execute(
                "UPDATE snapshots SET ref_count = ref_count + 1 WHERE hash = ?1",
                params![&hash.0[..]],
            )?;
            return Ok(hash);
        }

        let payload =
            zstd::encode_all(json.as_slice(), DEFAULT_ZSTD_LEVEL).map_err(PerceptionError::Zstd)?;
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(_) => 0,
        };

        self.conn.execute(
            "INSERT INTO snapshots (hash, payload, bytes, raw_bytes, ref_count, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                &hash.0[..],
                &payload,
                payload.len() as i64,
                json.len() as i64,
                now,
            ],
        )?;
        Ok(hash)
    }

    /// Load a snapshot by hash, re-verifying integrity. Returns `Ok(None)`
    /// if the hash is absent.
    ///
    /// # Errors
    /// SQLite, zstd, or JSON errors. A hash mismatch surfaces as
    /// [`PerceptionError::Store`] wrapping `rusqlite::Error::QueryReturnedNoRows`
    /// — we treat tampered rows as "missing" so callers can re-fetch from
    /// authoritative storage.
    pub fn get(&self, hash: &ContentHash) -> Result<Option<CompressedSnapshot>> {
        let row: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT payload FROM snapshots WHERE hash = ?1",
                params![&hash.0[..]],
                |row| row.get(0),
            )
            .optional()?;

        let Some(payload) = row else {
            return Ok(None);
        };

        let json = zstd::decode_all(payload.as_slice()).map_err(PerceptionError::Zstd)?;
        let actual = ContentHash::of_bytes(&json);
        if &actual != hash {
            return Ok(None);
        }

        let snap: CompressedSnapshot = serde_json::from_slice(&json)?;
        Ok(Some(snap))
    }

    /// Decrement `ref_count` for a hash; remove the row when it hits zero.
    ///
    /// # Errors
    /// Surfaces SQLite errors. Returns `Ok(false)` if the hash is unknown.
    pub fn release(&self, hash: &ContentHash) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE snapshots SET ref_count = ref_count - 1 WHERE hash = ?1 AND ref_count > 0",
            params![&hash.0[..]],
        )?;
        if updated == 0 {
            return Ok(false);
        }
        self.conn.execute(
            "DELETE FROM snapshots WHERE hash = ?1 AND ref_count <= 0",
            params![&hash.0[..]],
        )?;
        Ok(true)
    }

    /// Total number of stored rows.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn len(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))?;
        Ok(u64::try_from(n).unwrap_or(u64::MAX))
    }

    /// `true` if the store contains no snapshot rows.
    ///
    /// # Errors
    /// Surfaces `SQLite` errors.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::{CompressOptions, compress};
    use crate::raw::{RawNode, RawTree};
    use crate::refs::RefMap;

    fn sample_snapshot() -> CompressedSnapshot {
        let tree = RawTree::new(
            "https://x.test/a",
            vec![
                RawNode {
                    node_id: "1".into(),
                    parent_id: None,
                    child_ids: vec!["2".into()],
                    role: Some("WebArea".into()),
                    name: Some("Demo".into()),
                    value: None,
                    description: None,
                    properties: vec![],
                    ignored: false,
                },
                RawNode {
                    node_id: "2".into(),
                    parent_id: Some("1".into()),
                    child_ids: vec![],
                    role: Some("button".into()),
                    name: Some("OK".into()),
                    value: None,
                    description: None,
                    properties: vec![],
                    ignored: false,
                },
            ],
        );
        let mut refs = RefMap::new();
        compress(&tree, &mut refs, &CompressOptions::default())
    }

    #[test]
    fn put_get_round_trips() {
        let store = SnapshotStore::open(":memory:").unwrap();
        let s = sample_snapshot();
        let h = store.put(&s).unwrap();
        let back = store.get(&h).unwrap().expect("must exist");
        assert_eq!(back, s);
    }

    #[test]
    fn put_is_idempotent_and_dedups() {
        let store = SnapshotStore::open(":memory:").unwrap();
        let s = sample_snapshot();
        let h1 = store.put(&s).unwrap();
        let h2 = store.put(&s).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn release_removes_only_at_zero_refcount() {
        let store = SnapshotStore::open(":memory:").unwrap();
        let s = sample_snapshot();
        let h = store.put(&s).unwrap();
        store.put(&s).unwrap(); // ref_count == 2
        assert!(store.release(&h).unwrap());
        assert_eq!(store.len().unwrap(), 1);
        assert!(store.release(&h).unwrap());
        assert_eq!(store.len().unwrap(), 0);
        assert!(!store.release(&h).unwrap());
    }

    #[test]
    fn content_hash_hex_round_trip() {
        let s = sample_snapshot();
        let h = ContentHash::of(&s).unwrap();
        let parsed = ContentHash::from_hex(&h.to_hex()).unwrap();
        assert_eq!(h, parsed);
    }
}
