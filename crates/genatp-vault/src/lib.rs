//! `genatp-vault` — encrypted per-domain session vault, OS keychain–backed.
//!
//! Implementation timeline: **Phase 6** (Week 10–12).
//! Receives the largest mutation-testing budget in the workspace (≥ 80% mutants killed).
//! This crate is currently a Phase 0 stub. See [`docs/DEVELOPMENT_PLAN.md`].
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
