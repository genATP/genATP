//! `genatp-registry` — OCI Distribution Spec v1.1 cassette registry.
//!
//! ## Modules
//!
//! - [`manifest`] — registry manifest types, trust ladder, semver resolution.
//! - [`storage`] — SQLite-backed local registry store.
//! - [`api`] — axum-based OCI-compatible API server.
//! - [`client`] — publish, install, search operations.
//! - [`error`] — error types.
//!
//! ## Quick start
//!
//! ```no_run
//! # use genatp_registry::storage::RegistryStore;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let store = RegistryStore::open("/tmp/genatp-registry.db")?;
//! let slugs = store.list_slugs()?;
//! println!("Available cassettes: {slugs:?}");
//! # Ok(())
//! # }
//! ```
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod api;
pub mod client;
pub mod error;
pub mod manifest;
pub mod storage;

pub use api::{AppState, router, serve};
pub use client::{
    InstalledCassette, format_install_info, format_manifest_info, install, list_publishers,
    publish, search,
};
pub use error::{RegistryError, Result};
pub use manifest::{
    Capability, Publisher, RegistryManifest, Semver, TrustLevel, VersionConstraint,
    parse_constraint, resolve_version,
};
pub use storage::{RegistryStore, default_registry_dir};

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
