//! `genatp-browser` — CDP-native browser substrate via `chromiumoxide`.
//!
//! Implementation timeline: **Phase 1** (Week 1–3).
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
