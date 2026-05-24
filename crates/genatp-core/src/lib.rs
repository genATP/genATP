//! `genatp-core` — orchestrator: plan loop, budget guard, action dispatcher.
//!
//! Implementation timeline: **Phase 3** (Week 5–7).
//! This crate is currently a Phase 0 stub. See [`docs/DEVELOPMENT_PLAN.md`].
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]

/// Returns this crate's package version (compile-time `CARGO_PKG_VERSION`).
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_non_empty() {
        assert!(
            !version().is_empty(),
            "CARGO_PKG_VERSION must be set at build time"
        );
    }
}
