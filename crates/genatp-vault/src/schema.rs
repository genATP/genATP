//! SQLite schema and migrations for the encrypted vault.
//!
//! On-disk layout:
//!
//! ```sql
//! CREATE TABLE meta (
//!   key TEXT PRIMARY KEY,
//!   value TEXT NOT NULL
//! );  -- holds vault_schema_version, created_at_ms
//!
//! CREATE TABLE domains (
//!   domain TEXT PRIMARY KEY,            -- canonicalized
//!   created_at_ms INTEGER NOT NULL,
//!   updated_at_ms INTEGER NOT NULL
//! );
//!
//! CREATE TABLE entries (
//!   id INTEGER PRIMARY KEY AUTOINCREMENT,
//!   domain TEXT NOT NULL REFERENCES domains(domain) ON DELETE CASCADE,
//!   kind TEXT NOT NULL,                 -- 'cookie' | 'local_storage' | ...
//!   sub_key TEXT NOT NULL,              -- e.g. cookie name, lS key
//!   nonce BLOB NOT NULL,                -- 12 bytes
//!   ciphertext BLOB NOT NULL,           -- AES-256-GCM ct + tag
//!   created_at_ms INTEGER NOT NULL,
//!   UNIQUE (domain, kind, sub_key)
//! );
//! ```
//!
//! ## Migrations
//!
//! Migrations are append-only `static` SQL strings indexed by their
//! target version. The vault opens, reads `vault_schema_version`, and
//! applies every migration whose target is greater than the current
//! version, inside a single transaction. If the on-disk version is
//! *higher* than [`SCHEMA_CURRENT`], we abort with
//! [`crate::error::VaultError::SchemaTooNew`] — never silently
//! downgrade.

use rusqlite::Connection;

use crate::error::{Result, VaultError};

/// Highest schema version this binary understands.
pub const SCHEMA_CURRENT: i64 = 1;

/// Ordered migrations. Each tuple is `(target_version, sql)`.
const MIGRATIONS: &[(i64, &str)] = &[(
    1,
    r"
    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS domains (
        domain        TEXT PRIMARY KEY,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS entries (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        domain        TEXT NOT NULL REFERENCES domains(domain) ON DELETE CASCADE,
        kind          TEXT NOT NULL,
        sub_key       TEXT NOT NULL,
        nonce         BLOB NOT NULL,
        ciphertext    BLOB NOT NULL,
        created_at_ms INTEGER NOT NULL,
        UNIQUE (domain, kind, sub_key)
    );

    CREATE INDEX IF NOT EXISTS idx_entries_domain ON entries(domain);
    ",
)];

/// Read the current `vault_schema_version`, defaulting to `0` if the
/// `meta` table is empty / absent.
fn read_version(conn: &Connection) -> Result<i64> {
    let exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if exists == 0 {
        return Ok(0);
    }
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'vault_schema_version'",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(v.and_then(|s| s.parse::<i64>().ok()).unwrap_or(0))
}

/// Apply any pending migrations up to [`SCHEMA_CURRENT`]. No-op if
/// already current.
///
/// # Errors
/// - [`VaultError::SchemaTooNew`] if on-disk version > `SCHEMA_CURRENT`.
/// - [`VaultError::Database`] on SQLite failures.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current = read_version(conn)?;
    if current > SCHEMA_CURRENT {
        return Err(VaultError::SchemaTooNew {
            found: current,
            supported: SCHEMA_CURRENT,
        });
    }

    let tx = conn.transaction()?;
    for (target, sql) in MIGRATIONS {
        if *target > current {
            tracing::info!(from = current, to = *target, "applying vault migration");
            tx.execute_batch(sql)?;
        }
    }
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('vault_schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![SCHEMA_CURRENT.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn migrate_from_empty_lands_at_current() {
        let mut c = open_mem();
        migrate(&mut c).unwrap();
        let v = read_version(&c).unwrap();
        assert_eq!(v, SCHEMA_CURRENT);
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut c = open_mem();
        migrate(&mut c).unwrap();
        migrate(&mut c).unwrap();
        migrate(&mut c).unwrap();
        let v = read_version(&c).unwrap();
        assert_eq!(v, SCHEMA_CURRENT);
    }

    #[test]
    fn schema_too_new_aborts() {
        let mut c = open_mem();
        migrate(&mut c).unwrap();
        c.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'vault_schema_version'",
            rusqlite::params![(SCHEMA_CURRENT + 1).to_string()],
        )
        .unwrap();
        let err = migrate(&mut c).unwrap_err();
        assert!(matches!(err, VaultError::SchemaTooNew { .. }));
    }

    #[test]
    fn entries_table_enforces_unique_triplet() {
        let mut c = open_mem();
        migrate(&mut c).unwrap();
        c.execute(
            "INSERT INTO domains(domain, created_at_ms, updated_at_ms) VALUES('x.com', 0, 0)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO entries(domain, kind, sub_key, nonce, ciphertext, created_at_ms)
             VALUES('x.com', 'cookie', 'sid', x'000102030405060708090A0B', x'00', 0)",
            [],
        )
        .unwrap();
        let err = c.execute(
            "INSERT INTO entries(domain, kind, sub_key, nonce, ciphertext, created_at_ms)
             VALUES('x.com', 'cookie', 'sid', x'000102030405060708090A0B', x'00', 0)",
            [],
        );
        assert!(
            err.is_err(),
            "duplicate (domain, kind, sub_key) should fail"
        );
    }
}
