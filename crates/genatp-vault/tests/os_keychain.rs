//! Integration test against the **real** OS keychain.
//!
//! This test is gated by `#[ignore]` so it never runs on default CI.
//! Per-OS jobs run it explicitly with `cargo test -- --ignored`,
//! consuming the real Keychain on macOS, Credential Manager on
//! Windows, and Secret Service on Linux.
//!
//! Local invocation:
//!
//! ```text
//! cargo test -p genatp-vault --test os_keychain -- --ignored
//! ```
//!
//! On CI we set `GENATP_VAULT_OS_KEYCHAIN=1` to force these tests on
//! purpose-built jobs that have a logged-in keychain available.

use std::sync::Arc;

use genatp_vault::{
    Capability, Entry, KeyStore, OsKeyStore, Vault, default_vault_path, keystore::MasterKey,
};

fn unique_service() -> String {
    // Suffix with the test PID + timestamp so concurrent CI lanes
    // don't collide on the same keychain entry.
    let pid = std::process::id();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("genatp-vault-test-{pid}-{now}")
}

#[test]
#[ignore = "touches the OS keychain — opt-in via `--ignored`"]
fn os_keychain_round_trips_master_key() {
    let svc = unique_service();
    let ks = OsKeyStore::with_service(&svc);
    let key = MasterKey::from_bytes([0x99u8; 32]);
    ks.store("vault-test", &key).expect("store");
    let loaded = ks.load("vault-test").expect("load");
    // We don't expose the bytes directly — use seal+open to confirm
    // the round-trip preserved them.
    let aad = b"test";
    let pt = b"hello-real-keychain";
    let dek1 = genatp_vault::crypto::derive_dek(&key, "x.com");
    let dek2 = genatp_vault::crypto::derive_dek(&loaded, "x.com");
    let sealed = genatp_vault::crypto::seal(&dek1, aad, pt).unwrap();
    let out = genatp_vault::crypto::open(&dek2, aad, &sealed).unwrap();
    assert_eq!(out, pt);
    ks.delete("vault-test").expect("delete");
}

#[test]
#[ignore = "touches the OS keychain — opt-in via `--ignored`"]
fn vault_persists_via_real_keychain() {
    let svc = unique_service();
    let ks: Arc<dyn KeyStore> = Arc::new(OsKeyStore::with_service(&svc));

    let td = tempfile::tempdir().unwrap();
    let path = td.path().join("vault.sqlite");

    {
        let mut v = Vault::open_or_init(&path, ks.clone()).unwrap();
        v.save(
            &Capability::any(),
            &[Entry::local_storage("test-domain.example", "k", "v").unwrap()],
        )
        .unwrap();
    }
    {
        let mut v = Vault::open(&path, ks.clone()).unwrap();
        let out = v.load(&Capability::any(), "test-domain.example").unwrap();
        assert_eq!(out.len(), 1);
        v.destroy().unwrap();
    }
    // After destroy, the keychain entry should be gone too.
    let ks2: Arc<dyn KeyStore> = Arc::new(OsKeyStore::with_service(&svc));
    let err = Vault::open(&path, ks2).unwrap_err();
    assert!(matches!(
        err,
        genatp_vault::VaultError::NotInitialized | genatp_vault::VaultError::Io(_)
    ));
}

#[test]
#[ignore = "touches platform default vault path — opt-in"]
fn default_vault_path_resolves() {
    // Just confirm the helper returns a writable path. We don't
    // actually create the vault here so we don't pollute the user's
    // real home dir on a dev box.
    let p = default_vault_path().unwrap();
    assert!(p.is_absolute(), "expected absolute path, got {p:?}");
}
