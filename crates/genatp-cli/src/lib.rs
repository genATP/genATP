//! `genatp-cli` — clap-based command-line interface.
//!
//! Implementation timeline: structure **Phase 0**; subcommands fill in across
//! Phases 4 (`serve`), 7 (`record`), 8 (`run`), 9 (`test`), 13 (`replay`), 14 (`install`).
//! This crate is currently a Phase 0 stub. See [`docs/DEVELOPMENT_PLAN.md`].
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Phase 0 placeholder: prints a hello banner so the release smoke test has
/// observable output. Replaced with a clap dispatcher in Phase 4.
pub fn run() -> i32 {
    println!(
        "genATP {} — Phase 0 scaffold. See docs/DEVELOPMENT_PLAN.md.",
        version()
    );
    0
}

#[cfg(test)]
mod tests {
    use super::{run, version};

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn run_returns_success_exit_code() {
        assert_eq!(run(), 0);
    }
}
