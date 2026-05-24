//! `genatp-python` — Python SDK bindings via PyO3 + maturin.
//!
//! Implementation timeline: **Phase 5** (Week 8–10).
//! Phase 0 keeps this as a plain Rust library so the workspace builds without
//! Python headers in CI. Phase 5 swaps `crate-type` to `cdylib` and wires up
//! `#[pymodule]` / `#[pyclass]`.
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
