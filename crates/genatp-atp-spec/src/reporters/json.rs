//! JSON reporter — pretty-prints [`SpecResult`] as a formatted JSON document.
//!
//! Useful for programmatic consumption, custom dashboards, or inspection.

use super::Reporter;
use crate::ir::SpecResult;

/// JSON reporter.
pub struct JsonReporter;

impl Reporter for JsonReporter {
    type Output = String;

    fn report(&self, result: &SpecResult) -> String {
        serde_json::to_string_pretty(result)
            .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{SpecResult, StepResult};

    #[test]
    fn json_output_is_valid_and_contains_key_fields() {
        let result = SpecResult {
            name: "JSON Test".into(),
            success: true,
            duration_ms: 500,
            passed: 1,
            failed: 0,
            total: 1,
            steps: vec![StepResult {
                seq: 0,
                action: "navigate".into(),
                success: true,
                message: Some("OK".into()),
                duration_ms: 500,
                assertion_passed: None,
            }],
            spec_path: Some("test.yml".into()),
        };
        let reporter = JsonReporter;
        let json = reporter.report(&result);
        assert!(json.contains("\"name\": \"JSON Test\""));
        assert!(json.contains("\"success\": true"));
        assert!(json.contains("\"navigate\""));
        assert!(json.contains("test.yml"));
        // Verify it's valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"].as_str(), Some("JSON Test"));
    }

    #[test]
    fn json_output_with_failure() {
        let result = SpecResult {
            name: "Failure Test".into(),
            success: false,
            duration_ms: 300,
            passed: 0,
            failed: 1,
            total: 1,
            steps: vec![StepResult {
                seq: 0,
                action: "assert".into(),
                success: false,
                message: Some("element not found".into()),
                duration_ms: 300,
                assertion_passed: Some(false),
            }],
            spec_path: None,
        };
        let reporter = JsonReporter;
        let json = reporter.report(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["failed"], 1);
        assert_eq!(
            parsed["steps"][0]["assertion_passed"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn json_pretty_print_has_newlines() {
        let result = SpecResult {
            name: "pretty".into(),
            success: true,
            duration_ms: 0,
            passed: 0,
            failed: 0,
            total: 0,
            steps: vec![],
            spec_path: None,
        };
        let reporter = JsonReporter;
        let json = reporter.report(&result);
        assert!(json.contains('\n'), "pretty JSON should have newlines");
    }
}
