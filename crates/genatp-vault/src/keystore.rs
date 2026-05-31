//! KEK custodian abstraction.
//!
//! The vault stores ciphertexts on disk. The **master key** that
//! protects every per-domain DEK never lives in the same place — it's
//! sealed by the OS keychain (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service) and only retrieved when the user
//! actively unlocks the session.
//!
//! [`KeyStore`] is the abstraction: production code wires it to
//! `keyring`, tests wire it to [`InMemoryKeyStore`] so the unit suite
//! never touches the real OS keychain (which would require interactive
//! prompts and platform CI gating).

use crate::error::Result;

/// A 32-byte master key (KEK). Never logged, never printed; the
/// `Debug` impl elides the bytes.
#[derive(Clone)]
pub struct MasterKey(pub(crate) [u8; 32]);

impl MasterKey {
    /// Construct from raw bytes.
    ///
    /// Public so integration tests, benches, and out-of-process
    /// migration scripts can construct fixtures. Production code goes
    /// through [`crate::crypto::generate_master_key`] (OS RNG) — this
    /// constructor exists only to make tests deterministic.
    #[must_use]
    pub fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    /// Borrow the raw bytes — only the crypto layer in this crate
    /// should ever call this.
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey([REDACTED])")
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        // Best-effort zeroize. Crate-level `forbid(unsafe_code)` rules
        // out manual memory clobbering; we rely on `zeroize::Zeroize`
        // through a stack copy to make the compiler observe the write.
        use zeroize::Zeroize as _;
        self.0.zeroize();
    }
}

/// Custodian for the master key. Implementations must NOT log key
/// material and must surface user cancellations as
/// [`crate::error::VaultError::Keychain`].
pub trait KeyStore: Send + Sync {
    /// Fetch the master key for `vault_id`. Returns
    /// [`crate::error::VaultError::NotInitialized`] when the key has
    /// never been provisioned.
    ///
    /// `vault_id` is a stable identifier (the SHA-256 of the absolute
    /// vault file path, hex-encoded) so tests can isolate concurrent
    /// vaults without conflicting with production credentials.
    ///
    /// # Errors
    /// - [`crate::error::VaultError::NotInitialized`] if no entry exists.
    /// - [`crate::error::VaultError::Keychain`] for any other backend
    ///   failure (locked keychain, permission denied, etc.).
    fn load(&self, vault_id: &str) -> Result<MasterKey>;

    /// Store `key` under `vault_id`. Overwrites any existing entry.
    ///
    /// # Errors
    /// Returns [`crate::error::VaultError::Keychain`] on backend failures.
    fn store(&self, vault_id: &str, key: &MasterKey) -> Result<()>;

    /// Delete the entry for `vault_id`. Idempotent — deleting a
    /// non-existent entry returns `Ok(())`.
    ///
    /// # Errors
    /// As for [`KeyStore::store`].
    fn delete(&self, vault_id: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// OS keychain (production)
// ---------------------------------------------------------------------------

/// Production [`KeyStore`] backed by the OS keychain via the
/// [`keyring`] crate.
///
/// One entry is stored per vault: service name `"genatp-vault"`,
/// account `vault_id`. The value is a base64 of the 32-byte KEK.
#[derive(Debug, Default, Clone)]
pub struct OsKeyStore {
    /// Service identifier registered in the OS keychain. Defaulting to
    /// `"genatp-vault"` keeps all genATP entries grouped under one app
    /// in OS UI.
    pub service: String,
}

impl OsKeyStore {
    /// Construct with a custom service identifier (mainly for tests
    /// that want to namespace a real-keychain-backed run).
    #[must_use]
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// Construct with the default service name `"genatp-vault"`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_service("genatp-vault")
    }

    fn entry(&self, vault_id: &str) -> Result<keyring::Entry> {
        let svc = if self.service.is_empty() {
            "genatp-vault"
        } else {
            self.service.as_str()
        };
        Ok(keyring::Entry::new(svc, vault_id)?)
    }
}

impl KeyStore for OsKeyStore {
    fn load(&self, vault_id: &str) -> Result<MasterKey> {
        use base64::Engine as _;
        let entry = self.entry(vault_id)?;
        let secret = match entry.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => {
                return Err(crate::error::VaultError::NotInitialized);
            }
            Err(e) => return Err(e.into()),
        };
        let raw = base64::engine::general_purpose::STANDARD
            .decode(secret.trim())
            .map_err(|_| crate::error::VaultError::Keychain("malformed keychain entry".into()))?;
        if raw.len() != 32 {
            return Err(crate::error::VaultError::Keychain(
                "keychain entry has unexpected length".into(),
            ));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&raw);
        Ok(MasterKey::from_bytes(buf))
    }

    fn store(&self, vault_id: &str, key: &MasterKey) -> Result<()> {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(key.as_bytes());
        self.entry(vault_id)?.set_password(&encoded)?;
        Ok(())
    }

    fn delete(&self, vault_id: &str) -> Result<()> {
        match self.entry(vault_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory keystore (tests + benches)
// ---------------------------------------------------------------------------

/// `KeyStore` that lives entirely in-process. **Not for production**
/// — anyone with debugger access can read the key. We use it so the
/// unit/property suites are deterministic and don't pollute the real
/// keychain.
#[derive(Debug, Default, Clone)]
pub struct InMemoryKeyStore {
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, [u8; 32]>>>,
}

impl InMemoryKeyStore {
    /// Construct an empty in-memory keystore.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop **all** entries. Tests use this between scenarios to
    /// guarantee a clean keystore.
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.clear();
        }
    }
}

impl KeyStore for InMemoryKeyStore {
    fn load(&self, vault_id: &str) -> Result<MasterKey> {
        let g = self
            .inner
            .lock()
            .map_err(|_| crate::error::VaultError::Keychain("poisoned lock".into()))?;
        match g.get(vault_id) {
            Some(b) => Ok(MasterKey::from_bytes(*b)),
            None => Err(crate::error::VaultError::NotInitialized),
        }
    }

    fn store(&self, vault_id: &str, key: &MasterKey) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| crate::error::VaultError::Keychain("poisoned lock".into()))?;
        g.insert(vault_id.into(), *key.as_bytes());
        Ok(())
    }

    fn delete(&self, vault_id: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| crate::error::VaultError::Keychain("poisoned lock".into()))?;
        g.remove(vault_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_round_trips_master_key() {
        let ks = InMemoryKeyStore::new();
        let key = MasterKey::from_bytes([7u8; 32]);
        ks.store("vault-A", &key).unwrap();
        let loaded = ks.load("vault-A").unwrap();
        assert_eq!(loaded.as_bytes(), key.as_bytes());
    }

    #[test]
    fn in_memory_load_missing_returns_not_initialized() {
        let ks = InMemoryKeyStore::new();
        let err = ks.load("vault-X").unwrap_err();
        assert!(matches!(err, crate::error::VaultError::NotInitialized));
    }

    #[test]
    fn delete_is_idempotent() {
        let ks = InMemoryKeyStore::new();
        ks.delete("vault-A").unwrap();
        ks.store("vault-A", &MasterKey::from_bytes([0u8; 32]))
            .unwrap();
        ks.delete("vault-A").unwrap();
        ks.delete("vault-A").unwrap();
        assert!(matches!(
            ks.load("vault-A").unwrap_err(),
            crate::error::VaultError::NotInitialized
        ));
    }

    #[test]
    fn debug_redacts_master_key_bytes() {
        let key = MasterKey::from_bytes([0xABu8; 32]);
        let s = format!("{key:?}");
        assert!(!s.contains("AB"), "Debug must not print bytes: {s}");
        assert!(s.contains("REDACTED"));
    }
}
