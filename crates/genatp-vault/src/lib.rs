//! `genatp-vault` — encrypted per-domain auth store.
//!
//! Phase 6 (v0.3.0) deliverable. Solves "log in once, stay logged in
//! across restarts" by sealing cookies + `localStorage` under a master
//! key that lives in the OS keychain.
//!
//! # Architecture
//!
//! ```text
//!     ┌────────────────┐    ┌──────────────────┐
//!     │  OS keychain   │◀──▶│   KeyStore       │   master key
//!     └────────────────┘    └────────┬─────────┘
//!                                    │
//!                            ┌───────▼────────┐
//!                            │  HKDF-SHA256   │   per-domain DEK
//!                            └───────┬────────┘
//!                                    │
//!                       ┌────────────▼────────────┐
//!                       │   AES-256-GCM seal/open │   per-entry
//!                       └────────────┬────────────┘
//!                                    │
//!                            ┌───────▼────────┐
//!                            │  SQLite file   │
//!                            └────────────────┘
//! ```
//!
//! # Quickstart
//!
//! ```no_run
//! use std::sync::Arc;
//! use genatp_vault::{Vault, Capability, Entry, OsKeyStore};
//! use genatp_browser::storage::Cookie;
//!
//! # fn run() -> genatp_vault::Result<()> {
//! let ks = Arc::new(OsKeyStore::new());
//! let mut v = Vault::open_or_init(genatp_vault::default_vault_path()?, ks)?;
//!
//! let cap = Capability::for_domains(["github.com"])?;
//! v.save(&cap, &[Entry::cookie(
//!     "github.com",
//!     Cookie::for_url("session", "abc", "https://github.com/"),
//! )?])?;
//!
//! let out = v.load(&cap, "github.com")?;
//! assert_eq!(out.len(), 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Cryptographic recipe
//!
//! - **KEK**: 32 bytes from `OsRng`, sealed by the OS keychain.
//! - **DEK**: HKDF-SHA256(KEK, salt = `b"genatp-vault-kdf-v1"`,
//!   info = `b"genatp-vault/dek/<domain>"`).
//! - **Cipher**: AES-256-GCM with 96-bit random nonces; AAD =
//!   `<kind>:<domain>:<sub_key>`.
//! - **AAD-binding**: relocating a ciphertext to a different row
//!   fails authentication.
//!
//! See [`docs/VAULT.md`](../../../docs/VAULT.md) for the threat model
//! and rotation story.

#![forbid(unsafe_code)]

pub mod capability;
pub mod crypto;
pub mod entry;
pub mod error;
pub mod keystore;
pub mod schema;
pub mod vault;

pub use crate::capability::Capability;
pub use crate::entry::{Entry, KIND_COOKIE, KIND_LOCAL_STORAGE, canonical_domain};
pub use crate::error::{Result, VaultError};
pub use crate::keystore::{InMemoryKeyStore, KeyStore, OsKeyStore};
pub use crate::vault::{DEFAULT_FILE_NAME, DomainSummary, Vault, default_vault_path};

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
