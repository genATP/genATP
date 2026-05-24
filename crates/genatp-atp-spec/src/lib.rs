//! `genatp-atp-spec` — declarative ATP Spec (`.atp.yml`) parser, IR, and runner.
//!
//! Implementation timeline: **Phase 9** (Week 16–17).
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
