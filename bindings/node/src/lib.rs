//! `genatp-node` — TypeScript/Node SDK bindings via NAPI-RS.
//!
//! Implementation timeline: **Phase 10** (Week 17–19).
//! Phase 0 keeps this as a plain Rust library so the workspace builds without
//! NAPI tooling in CI. Phase 10 swaps `crate-type` to `cdylib` and wires up
//! `#[napi]` exports.
//!
//! See [`docs/DEVELOPMENT_PLAN.md`].
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]

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
