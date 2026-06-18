//! Report formatters for ATP Spec results.
//!
//! Each formatter converts a [`SpecResult`](crate::ir::SpecResult) into a
//! specific output format (JUnit XML, JSON, etc.).

pub mod json;
pub mod junit;

pub use json::JsonReporter;
pub use junit::JunitXmlReporter;

use crate::ir::SpecResult;

/// Trait for spec result reporters.
pub trait Reporter {
    /// Format the result.
    type Output;
    /// Generate a report from a spec result.
    fn report(&self, result: &SpecResult) -> Self::Output;
}
