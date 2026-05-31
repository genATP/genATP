//! Top-level [`Vault`] type.
//!
//! Owns:
//!   - the SQLite handle for the encrypted on-disk file,
//!   - a [`crate::keystore::KeyStore`] for fetching the master key,
//!   - lazy-derived per-domain DEKs (cached for the lifetime of the
//!     vault handle).
//!
//! Concurrency: `Vault` is **not** internally synchronised. SQLite
//! itself supports concurrent readers via `BUSY_TIMEOUT`, but every
//! mutation flows through a single in-process handle. Wrap in
//! `Arc<Mutex<Vault>>` if you need to share across threads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use rusqlite::Connection;

use crate::capability::Capability;
use crate::crypto::{
    Dek, Sealed, derive_dek, generate_master_key, open as gcm_open, seal as gcm_seal,
};
use crate::entry::{Entry, KIND_COOKIE, KIND_LOCAL_STORAGE, canonical_domain};
use crate::error::{Result, VaultError};
#[cfg(test)]
use crate::keystore::InMemoryKeyStore;
use crate::keystore::{KeyStore, MasterKey, OsKeyStore};
use crate::schema::{SCHEMA_CURRENT, migrate};

/// Default file name under the user's data dir.
pub const DEFAULT_FILE_NAME: &str = "vault.sqlite";

/// Build the on-disk vault path for the current user, creating the
/// parent directory if needed.
///
/// Resolution order:
///   1. `$GENATP_VAULT_PATH` (full file path).
///   2. `$GENATP_HOME/vault.sqlite`.
///   3. Platform data dir (`directories::ProjectDirs`) — e.g.
///      `~/Library/Application Support/genatp/vault.sqlite` on macOS,
///      `%APPDATA%\genatp\vault.sqlite` on Windows,
///      `~/.local/share/genatp/vault.sqlite` on Linux.
///
/// # Errors
/// Returns [`VaultError::Io`] if the parent directory cannot be created.
pub fn default_vault_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("GENATP_VAULT_PATH") {
        let p = PathBuf::from(p);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        return Ok(p);
    }
    if let Some(home) = std::env::var_os("GENATP_HOME") {
        let dir = PathBuf::from(home);
        std::fs::create_dir_all(&dir)?;
        return Ok(dir.join(DEFAULT_FILE_NAME));
    }
    let proj = directories::ProjectDirs::from("dev", "genatp", "genatp")
        .ok_or_else(|| VaultError::Keychain("could not resolve user data directory".into()))?;
    let dir = proj.data_dir();
    std::fs::create_dir_all(dir)?;
    Ok(dir.join(DEFAULT_FILE_NAME))
}

/// Stable identifier for a vault file. We use the SHA-256 of the
/// canonicalised absolute path (hex). Distinct paths therefore use
/// distinct keychain entries — handy for tests, dev/prod separation,
/// etc.
fn vault_id_for(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy();
    let mut hasher = blake3::Hasher::new();
    hasher.update(s.as_bytes());
    let hash = hasher.finalize();
    hash.to_hex().as_str().to_string()
}

/// Summary returned by [`Vault::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSummary {
    /// Canonical domain.
    pub domain: String,
    /// Number of entries stored under this domain.
    pub entry_count: usize,
    /// Last-update timestamp in milliseconds since the Unix epoch.
    pub updated_at_ms: i64,
}

/// Encrypted credential store.
pub struct Vault {
    conn: Connection,
    path: PathBuf,
    /// Held for the vault lifetime; dropped (zeroed) when the vault is
    /// closed.
    master_key: MasterKey,
    /// Cached per-domain DEKs. Cleared on `Vault::drop`.
    dek_cache: HashMap<String, Dek>,
    /// Origin keystore — kept for future operations like `rekey`.
    keystore: Arc<dyn KeyStore>,
    /// Stable id used to name the keychain entry.
    vault_id: String,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("vault_id", &self.vault_id)
            .field("dek_cache.len", &self.dek_cache.len())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// Initialise a brand-new vault: generate a fresh master key, store
    /// it in `keystore`, create the SQLite file, run all migrations.
    ///
    /// Returns an opened, unlocked [`Vault`] ready for entry writes.
    ///
    /// **Idempotency:** if the SQLite file already exists, this
    /// preserves any existing rows but **overwrites** the keychain
    /// entry with a fresh KEK — which renders existing rows
    /// unreadable. Catastrophic by design for `initialize`; use
    /// [`Vault::open`] for normal startup.
    ///
    /// # Errors
    /// - [`VaultError::Io`] if the file cannot be created.
    /// - [`VaultError::Keychain`] if the keystore rejects the write.
    /// - [`VaultError::Database`] on schema setup failures.
    pub fn initialize(path: impl AsRef<Path>, keystore: Arc<dyn KeyStore>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;

        let key = generate_master_key();
        let vault_id = vault_id_for(&path);
        keystore.store(&vault_id, &key)?;

        Ok(Self {
            conn,
            path,
            master_key: key,
            dek_cache: HashMap::new(),
            keystore,
            vault_id,
        })
    }

    /// Open an existing vault. Loads the master key from `keystore`,
    /// runs any pending migrations, and returns the unlocked handle.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`] if the keystore has no entry.
    /// - [`VaultError::SchemaTooNew`] if the file is from a newer
    ///   version of genATP.
    /// - [`VaultError::Database`] on SQLite failures.
    pub fn open(path: impl AsRef<Path>, keystore: Arc<dyn KeyStore>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;

        let vault_id = vault_id_for(&path);
        let key = keystore.load(&vault_id)?;

        Ok(Self {
            conn,
            path,
            master_key: key,
            dek_cache: HashMap::new(),
            keystore,
            vault_id,
        })
    }

    /// Convenience: open or initialise. Used by the MCP / CLI path.
    ///
    /// # Errors
    /// As for [`Vault::open`] / [`Vault::initialize`].
    pub fn open_or_init(path: impl AsRef<Path>, keystore: Arc<dyn KeyStore>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        match Vault::open(&path, keystore.clone()) {
            Ok(v) => Ok(v),
            Err(VaultError::NotInitialized) => Vault::initialize(&path, keystore),
            Err(e) => Err(e),
        }
    }

    /// Convenience constructor backed by the OS keychain.
    ///
    /// # Errors
    /// As for [`Vault::open_or_init`].
    pub fn open_default() -> Result<Self> {
        let path = default_vault_path()?;
        Self::open_or_init(path, Arc::new(OsKeyStore::new()))
    }

    /// Convenience constructor backed by an in-memory keystore. Used
    /// in tests and benches.
    #[cfg(test)]
    fn open_in_memory(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_or_init(path, Arc::new(InMemoryKeyStore::new()))
    }

    /// Returns the on-disk vault path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the schema version held on disk.
    ///
    /// # Errors
    /// Returns [`VaultError::Database`] on SQLite failures.
    pub fn schema_version(&self) -> Result<i64> {
        let v: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'vault_schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(SCHEMA_CURRENT))
    }

    /// Lookup or compute the DEK for `domain`. Cached for the lifetime
    /// of the `Vault` handle; first access derives via HKDF.
    fn dek_for(&mut self, domain: &str) -> &Dek {
        if !self.dek_cache.contains_key(domain) {
            let dek = derive_dek(&self.master_key, domain);
            self.dek_cache.insert(domain.to_string(), dek);
        }
        self.dek_cache
            .get(domain)
            .expect("inserted above; this lookup must succeed")
    }

    fn aad_for(kind: &str, domain: &str, sub_key: &str) -> Vec<u8> {
        format!("{kind}:{domain}:{sub_key}").into_bytes()
    }

    /// Save `entries` to the vault, atomically. Existing rows for the
    /// same `(domain, kind, sub_key)` triplet are **upserted**
    /// (replaced) — the vault stores one ciphertext per logical entry,
    /// not a history.
    ///
    /// Caller must declare a [`Capability`] that authorises every
    /// domain touched.
    ///
    /// # Errors
    /// - [`VaultError::CapabilityDenied`] if any entry's domain is
    ///   outside `cap`.
    /// - [`VaultError::Database`] / [`VaultError::Decrypt`] on
    ///   underlying failures.
    pub fn save(&mut self, cap: &Capability, entries: &[Entry]) -> Result<()> {
        for e in entries {
            cap.require(e.domain())?;
        }
        let now = Utc::now().timestamp_millis();

        // Encrypt first. We avoid borrowing the SQLite transaction at
        // the same time as the DEK cache so the borrow checker stays
        // happy without `unsafe`. The cost is two passes; per-entry
        // sealing dominates the time budget anyway.
        let mut prepared: Vec<(String, &'static str, String, Sealed)> =
            Vec::with_capacity(entries.len());
        for e in entries {
            let domain = e.domain().to_string();
            let kind = match e {
                Entry::Cookie { .. } => KIND_COOKIE,
                Entry::LocalStorage { .. } => KIND_LOCAL_STORAGE,
            };
            let sub_key = e.sub_key();
            let mut plaintext = serde_json::to_vec(e)?;
            let aad = Self::aad_for(kind, &domain, &sub_key);
            let dek = self.dek_for(&domain);
            let sealed = gcm_seal(dek, &aad, &plaintext)?;
            crate::crypto::zeroize_buf(&mut plaintext);
            prepared.push((domain, kind, sub_key, sealed));
        }

        // Single transaction: domain upsert + entry upsert. Anything
        // that fails inside aborts both, leaving the vault unchanged.
        let tx = self.conn.transaction()?;
        for (domain, kind, sub_key, sealed) in &prepared {
            tx.execute(
                "INSERT INTO domains(domain, created_at_ms, updated_at_ms) VALUES(?1, ?2, ?2)
                 ON CONFLICT(domain) DO UPDATE SET updated_at_ms = excluded.updated_at_ms",
                rusqlite::params![domain, now],
            )?;
            tx.execute(
                "INSERT INTO entries(domain, kind, sub_key, nonce, ciphertext, created_at_ms)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(domain, kind, sub_key) DO UPDATE SET
                   nonce = excluded.nonce,
                   ciphertext = excluded.ciphertext,
                   created_at_ms = excluded.created_at_ms",
                rusqlite::params![
                    domain,
                    *kind,
                    sub_key,
                    sealed.nonce.as_slice(),
                    sealed.ciphertext,
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Load every entry stored under `domain`. Returns an empty vec if
    /// the domain is unknown.
    ///
    /// # Errors
    /// - [`VaultError::CapabilityDenied`] if `cap` does not allow
    ///   `domain`.
    /// - [`VaultError::Decrypt`] if any row fails authentication
    ///   (vault corruption / wrong key).
    /// - [`VaultError::InvalidDomain`] / [`VaultError::Database`].
    pub fn load(&mut self, cap: &Capability, domain: &str) -> Result<Vec<Entry>> {
        let domain = canonical_domain(domain)?;
        cap.require(&domain)?;

        let rows: Vec<(String, String, Vec<u8>, Vec<u8>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT kind, sub_key, nonce, ciphertext FROM entries WHERE domain = ?1
                 ORDER BY id ASC",
            )?;
            stmt.query_map([&domain], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::with_capacity(rows.len());
        for (kind, sub_key, nonce_bytes, ciphertext) in rows {
            if nonce_bytes.len() != Sealed::NONCE_LEN {
                return Err(VaultError::Decrypt);
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&nonce_bytes);
            let sealed = Sealed { nonce, ciphertext };
            let aad = Self::aad_for(&kind, &domain, &sub_key);
            let dek = self.dek_for(&domain);
            let mut plaintext = gcm_open(dek, &aad, &sealed)?;
            let entry: Entry = serde_json::from_slice(&plaintext)?;
            crate::crypto::zeroize_buf(&mut plaintext);
            out.push(entry);
        }
        Ok(out)
    }

    /// Enumerate every domain and its entry count.
    ///
    /// # Errors
    /// Returns [`VaultError::Database`] on SQLite failures.
    pub fn list(&self) -> Result<Vec<DomainSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.domain, COUNT(e.id), d.updated_at_ms
             FROM domains d LEFT JOIN entries e ON e.domain = d.domain
             GROUP BY d.domain ORDER BY d.domain ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DomainSummary {
                    domain: r.get(0)?,
                    entry_count: r.get::<_, i64>(1)? as usize,
                    updated_at_ms: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Revoke (delete) every entry under `domain`. Returns the number
    /// of rows removed. Idempotent — revoking a missing domain is
    /// `Ok(0)`.
    ///
    /// # Errors
    /// Returns [`VaultError::Database`] on SQLite failures.
    pub fn revoke(&mut self, domain: &str) -> Result<usize> {
        let domain = canonical_domain(domain)?;
        // Drop cached DEK so no stale key sits in memory after revoke.
        self.dek_cache.remove(&domain);
        let n = self
            .conn
            .execute("DELETE FROM domains WHERE domain = ?1", [&domain])?;
        Ok(n)
    }

    /// Permanently destroy the vault: delete the on-disk file *and*
    /// remove the keychain entry. After this returns, `Vault::open`
    /// against the same path will fail with `NotInitialized`.
    ///
    /// # Errors
    /// Returns [`VaultError::Io`] / [`VaultError::Keychain`] on the
    /// respective backend failure.
    pub fn destroy(self) -> Result<()> {
        let Vault {
            conn,
            path,
            keystore,
            vault_id,
            ..
        } = self;
        drop(conn);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        keystore.delete(&vault_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> (tempfile::TempDir, PathBuf) {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("vault.sqlite");
        (td, p)
    }

    #[test]
    fn save_and_load_round_trip() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::for_domains(["github.com"]).unwrap();
        let entry = Entry::cookie(
            "github.com",
            genatp_browser::storage::Cookie::for_url("session", "abc", "https://github.com/"),
        )
        .unwrap();
        v.save(&cap, std::slice::from_ref(&entry)).unwrap();
        let out = v.load(&cap, "github.com").unwrap();
        assert_eq!(out, vec![entry]);
    }

    #[test]
    fn load_unknown_domain_returns_empty() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::for_domains(["github.com"]).unwrap();
        let out = v.load(&cap, "github.com").unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn save_denies_unauthorised_domain() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::for_domains(["github.com"]).unwrap();
        let bad = Entry::local_storage("evil.com", "k", "v").unwrap();
        let err = v.save(&cap, &[bad]).unwrap_err();
        assert!(matches!(err, VaultError::CapabilityDenied(_)));
    }

    #[test]
    fn list_reports_distinct_domains() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::any();
        v.save(
            &cap,
            &[
                Entry::local_storage("a.com", "k", "v").unwrap(),
                Entry::local_storage("a.com", "k2", "v").unwrap(),
                Entry::local_storage("b.com", "k", "v").unwrap(),
            ],
        )
        .unwrap();
        let summary = v.list().unwrap();
        assert_eq!(summary.len(), 2);
        let by_domain: HashMap<_, _> = summary
            .into_iter()
            .map(|s| (s.domain.clone(), s.entry_count))
            .collect();
        assert_eq!(by_domain["a.com"], 2);
        assert_eq!(by_domain["b.com"], 1);
    }

    #[test]
    fn revoke_drops_all_rows_for_domain() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::any();
        v.save(
            &cap,
            &[
                Entry::local_storage("a.com", "k", "v").unwrap(),
                Entry::local_storage("a.com", "k2", "v").unwrap(),
                Entry::local_storage("b.com", "k", "v").unwrap(),
            ],
        )
        .unwrap();
        let n = v.revoke("a.com").unwrap();
        assert_eq!(n, 1, "domains row");
        // entries cascade to 0
        let out = v.load(&cap, "a.com").unwrap();
        assert!(out.is_empty());
        let still = v.load(&cap, "b.com").unwrap();
        assert_eq!(still.len(), 1);
    }

    #[test]
    fn upsert_replaces_on_save_of_same_subkey() {
        let (_td, path) = temp_path();
        let mut v = Vault::open_in_memory(&path).unwrap();
        let cap = Capability::for_domains(["x.com"]).unwrap();
        let v1 = Entry::local_storage("x.com", "k", "v1").unwrap();
        v.save(&cap, &[v1]).unwrap();
        let v2 = Entry::local_storage("x.com", "k", "v2").unwrap();
        v.save(&cap, &[v2]).unwrap();
        let out = v.load(&cap, "x.com").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Entry::local_storage("x.com", "k", "v2").unwrap());
    }

    #[test]
    fn vault_persists_across_handles_when_keystore_persists() {
        // Use a shared in-memory keystore across two Vault opens.
        let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let (_td, path) = temp_path();
        {
            let mut v = Vault::open_or_init(&path, ks.clone()).unwrap();
            v.save(
                &Capability::any(),
                &[Entry::local_storage("x.com", "k", "secret-v").unwrap()],
            )
            .unwrap();
        }
        let mut v2 = Vault::open(&path, ks).unwrap();
        let out = v2.load(&Capability::any(), "x.com").unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn missing_keystore_entry_returns_not_initialized() {
        // Pre-create the SQLite file but leave the keystore empty.
        let (_td, path) = temp_path();
        {
            let conn = Connection::open(&path).unwrap();
            // Ensure file is materialized.
            conn.execute_batch("CREATE TABLE IF NOT EXISTS dummy (x INT);")
                .unwrap();
        }
        let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let err = Vault::open(&path, ks).unwrap_err();
        assert!(matches!(err, VaultError::NotInitialized));
    }
}
