//! `genatp-atp-spec` — declarative ATP Spec (`.atp.yml`) parser, IR, and runner.
//!
//! This crate implements **Phase 9** of the genATP roadmap: a declarative spec
//! format for browser-automation tasks that can be parsed, validated, and
//! executed deterministically.
//!
//! ## Modules
//!
//! - [`ir`] — Intermediate Representation (IR) types for parsed ATP Specs.
//! - [`parser`] — YAML parser for `.atp.yml` files.
//! - [`validator`] — Semantic validation of parsed specs.
//! - [`runner`] — Spec runner that executes steps against a browser.
//! - [`reporters`] — Report formatters (JUnit XML, JSON).
//! - [`cli`] — CLI command handlers for `genatp atp` subcommands.
//! - [`error`] — Error types.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::path::Path;
//! use genatp_atp_spec::parser;
//! use genatp_atp_spec::validator;
//!
//! let spec = parser::parse_file(Path::new("my-spec.atp.yml")).unwrap();
//! validator::validate(&spec).unwrap();
//! println!("Spec '{}' is valid with {} steps", spec.name, spec.steps.len());
//! ```

#![forbid(unsafe_code)]

pub mod cli;
pub mod error;
pub mod ir;
pub mod parser;
pub mod reporters;
pub mod runner;
pub mod validator;

// Re-export key types at the crate root for ergonomic `use genatp_atp_spec::*`.
pub use error::{AtpSpecError, Result};
pub use ir::{
    AtpSpec, BudgetDef, ChangePolicy, ChangeStrategy, ElementSelector, EnvVar, SpecResult, Step,
    StepResult,
};
pub use parser::{parse_file, parse_str};
pub use reporters::{JsonReporter, JunitXmlReporter};
pub use runner::{SpecRunConfig, run_spec};
pub use validator::{validate, validate_all};

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
