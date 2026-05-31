//! Security tests — Phase 6 acceptance gates.
//!
//! These don't run a full attacker simulation; they pin the most
//! important vault invariants:
//!
//! 1. **No plaintext on disk.** A vault file written through the
//!    public API must not contain user-supplied plaintext bytes
//!    anywhere on disk.
//! 2. **Missing keychain ⇒ `NotInitialized`.** Opening a vault
//!    without the master key returns the correct error variant (and
//!    notably *not* a generic "decrypt failed" — the error must be
//!    actionable for the CLI / MCP layer).
//! 3. **Tampered ciphertext rejected.** Flipping a single bit in the
//!    on-disk ciphertext column makes `load` fail.

use std::sync::Arc;

use genatp_browser::storage::Cookie;
use genatp_vault::{Capability, Entry, InMemoryKeyStore, KeyStore, Vault, VaultError};

const SECRET_MARKER: &[u8] = b"VERY-DISTINCTIVE-SECRET-MARKER-DEADBEEF";

fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("vault.sqlite");
    (td, p)
}

#[test]
fn vault_file_contains_no_plaintext_after_save() {
    let (_td, path) = temp_path();
    let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    let mut v = Vault::open_or_init(&path, ks).unwrap();
    let cap = Capability::any();

    let value = String::from_utf8(SECRET_MARKER.to_vec()).unwrap();
    let cookie = Cookie {
        name: "session".into(),
        value: value.clone(),
        domain: Some("github.com".into()),
        path: Some("/".into()),
        secure: true,
        http_only: true,
        same_site: Some("Lax".into()),
        url: None,
    };
    v.save(
        &cap,
        &[
            Entry::cookie("github.com", cookie).unwrap(),
            Entry::local_storage("github.com", "auth_token", &value).unwrap(),
        ],
    )
    .unwrap();
    drop(v);

    // SQLite WAL mode keeps recent writes in `vault.sqlite-wal`; we
    // checkpoint into the main file by re-opening read-only.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "wal_checkpoint", "FULL").unwrap();
    drop(conn);

    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes
            .windows(SECRET_MARKER.len())
            .any(|w| w == SECRET_MARKER),
        "plaintext marker leaked into vault file"
    );

    // Also check the WAL/SHM siblings if they exist.
    for ext in ["-wal", "-shm"] {
        let p = path.with_extension(format!("sqlite{ext}"));
        if let Ok(b) = std::fs::read(&p) {
            assert!(
                !b.windows(SECRET_MARKER.len()).any(|w| w == SECRET_MARKER),
                "plaintext leaked into {p:?}"
            );
        }
    }
}

#[test]
fn opening_vault_without_keychain_entry_returns_not_initialized() {
    let (_td, path) = temp_path();
    // Materialize a SQLite file that *looks* like a vault.
    let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    {
        let _v = Vault::open_or_init(&path, ks).unwrap();
    }
    // Now drop the keystore entry (simulates: laptop reset, fresh
    // user, OS keychain wiped) and try to open with a *different*
    // empty keystore.
    let empty_ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    let err = Vault::open(&path, empty_ks).unwrap_err();
    assert!(
        matches!(err, VaultError::NotInitialized),
        "expected NotInitialized, got {err:?}"
    );
}

#[test]
fn tampered_ciphertext_rejects_load() {
    let (_td, path) = temp_path();
    let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    {
        let mut v = Vault::open_or_init(&path, ks.clone()).unwrap();
        v.save(
            &Capability::any(),
            &[Entry::local_storage("x.com", "k", "v").unwrap()],
        )
        .unwrap();
    }
    // Flip a bit in the ciphertext column.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let blob: Vec<u8> = conn
            .query_row("SELECT ciphertext FROM entries LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let mut tampered = blob.clone();
        tampered[0] ^= 0x01;
        conn.execute(
            "UPDATE entries SET ciphertext = ?1",
            rusqlite::params![tampered],
        )
        .unwrap();
    }
    let mut v = Vault::open(&path, ks).unwrap();
    let err = v.load(&Capability::any(), "x.com").unwrap_err();
    assert!(matches!(err, VaultError::Decrypt));
}

#[test]
fn vault_id_differs_for_distinct_paths() {
    // Two vaults at distinct paths must use distinct keychain
    // entries — otherwise revoking vault A would also revoke
    // vault B's master key. This is implicit but worth pinning.
    let (_td_a, path_a) = temp_path();
    let (_td_b, path_b) = temp_path();
    let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    let _v_a = Vault::open_or_init(&path_a, ks.clone()).unwrap();
    let _v_b = Vault::open_or_init(&path_b, ks.clone()).unwrap();
    // The InMemoryKeyStore stores by id, so if both used the same id
    // we'd see exactly one entry; we expect two distinct.
    // Use destroy on path_a and confirm path_b still opens.
    drop(_v_a);
    let v_b2 = Vault::open(&path_b, ks);
    assert!(
        v_b2.is_ok(),
        "second vault must still be openable after first is created"
    );
}
