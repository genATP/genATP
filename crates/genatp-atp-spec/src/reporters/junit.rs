//! JUnit XML reporter for CI integration.
//!
//! Produces a JUnit-compatible XML document conforming to the schema
//! understood by Jenkins, GitHub Actions, GitLab CI, CircleCI, etc.
//!
//! ## Example output
//!
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! <testsuites name="genatp-specs" tests="3" failures="1" errors="0" time="1.234">
//!   <testsuite name="Login Flow" tests="3" failures="1" errors="0" time="1.234">
//!     <testcase name="[0] navigate" classname="Login Flow" time="0.500"/>
//!     <testcase name="[1] click" classname="Login Flow" time="0.300"/>
//!     <testcase name="[2] assert" classname="Login Flow" time="0.434">
//!       <failure message="assertion failed">
//!         expected page to contain `Welcome`, but it was not found
//!       </failure>
//!     </testcase>
//!   </testsuite>
//! </testsuites>
//! ```

use std::fmt::Write as _;

use super::Reporter;
use crate::ir::{SpecResult, StepResult};

/// JUnit XML reporter.
pub struct JunitXmlReporter;

impl Reporter for JunitXmlReporter {
    type Output = String;

    fn report(&self, result: &SpecResult) -> String {
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push('\n');

        let time_secs = result.duration_ms as f64 / 1000.0;
        let _ = write!(
            xml,
            r#"<testsuites name="genatp-specs" tests="{}" failures="{}" errors="0" time="{:.3}">"#,
            result.total, result.failed, time_secs
        );
        xml.push('\n');

        let _ = write!(
            xml,
            r#"  <testsuite name="{}" tests="{}" failures="{}" errors="0" time="{:.3}">"#,
            escape_xml(&result.name),
            result.total,
            result.failed,
            time_secs
        );
        xml.push('\n');

        for step in &result.steps {
            format_step_xml(&mut xml, result, step);
        }

        xml.push_str("  </testsuite>\n");
        xml.push_str("</testsuites>\n");
        xml
    }
}

fn format_step_xml(xml: &mut String, result: &SpecResult, step: &StepResult) {
    let time_secs = step.duration_ms as f64 / 1000.0;
    let classname = escape_xml(&result.name);
    let name = escape_xml(&format!("[{}] {}", step.seq, step.action));

    let _ = write!(
        xml,
        r#"    <testcase name="{}" classname="{}" time="{:.3}">"#,
        name, classname, time_secs
    );

    if !step.success {
        let msg = step.message.as_deref().unwrap_or("step failed");
        let body = escape_xml(msg);
        xml.push('\n');
        let _ = write!(
            xml,
            r#"      <failure message="step {}: {}">{}</failure>"#,
            step.seq,
            escape_xml(&step.action),
            body
        );
        xml.push('\n');
        xml.push_str("    </testcase>\n");
    } else {
        xml.push_str("</testcase>\n");
    }
}

/// Escape special XML characters in a string.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{SpecResult, StepResult};

    fn sample_result() -> SpecResult {
        SpecResult {
            name: "Login Flow".into(),
            success: false,
            duration_ms: 1234,
            passed: 2,
            failed: 1,
            total: 3,
            steps: vec![
                StepResult {
                    seq: 0,
                    action: "navigate".into(),
                    success: true,
                    message: Some("navigated".into()),
                    duration_ms: 500,
                    assertion_passed: None,
                },
                StepResult {
                    seq: 1,
                    action: "click".into(),
                    success: true,
                    message: Some("clicked".into()),
                    duration_ms: 300,
                    assertion_passed: None,
                },
                StepResult {
                    seq: 2,
                    action: "assert".into(),
                    success: false,
                    message: Some(
                        "expected page to contain `Welcome`, but it was not found".into(),
                    ),
                    duration_ms: 434,
                    assertion_passed: Some(false),
                },
            ],
            spec_path: Some("tests/login.atp.yml".into()),
        }
    }

    #[test]
    fn junit_output_contains_test_suites() {
        let reporter = JunitXmlReporter;
        let xml = reporter.report(&sample_result());
        assert!(xml.contains("<testsuites"));
        assert!(xml.contains("</testsuites>"));
        assert!(xml.contains("<testsuite"));
        assert!(xml.contains("</testsuite>"));
        assert!(xml.contains("<testcase"));
        assert!(xml.contains("</testcase>"));
    }

    #[test]
    fn junit_output_shows_failure() {
        let reporter = JunitXmlReporter;
        let xml = reporter.report(&sample_result());
        assert!(
            xml.contains("<failure"),
            "should have failure element: {xml}"
        );
        assert!(
            xml.contains("expected page to contain"),
            "should show the failure message: {xml}"
        );
        assert!(
            xml.contains("Welcome"),
            "should contain the expected text: {xml}"
        );
        assert!(
            xml.contains("step 2: assert"),
            "should identify the failed step: {xml}"
        );
    }

    #[test]
    fn junit_output_lists_correct_counts() {
        let reporter = JunitXmlReporter;
        let xml = reporter.report(&sample_result());
        assert!(xml.contains("tests=\"3\""));
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("errors=\"0\""));
    }

    #[test]
    fn junit_output_escapes_xml_special_chars() {
        let result = SpecResult {
            name: "Test < & > \"'".into(),
            success: true,
            duration_ms: 100,
            passed: 1,
            failed: 0,
            total: 1,
            steps: vec![StepResult {
                seq: 0,
                action: "assert".into(),
                success: true,
                message: Some("text with < & > \"'".into()),
                duration_ms: 50,
                assertion_passed: Some(true),
            }],
            spec_path: None,
        };
        let reporter = JunitXmlReporter;
        let xml = reporter.report(&result);
        assert!(xml.contains("&amp;"), "`&` should be escaped to `&amp;`");
        assert!(xml.contains("&lt;"), "`<` should be escaped to `&lt;`");
        assert!(xml.contains("&gt;"), "`>` should be escaped to `&gt;`");
    }

    #[test]
    fn junit_output_has_correct_xml_preamble() {
        let result = sample_result();
        let reporter = JunitXmlReporter;
        let xml = reporter.report(&result);
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
    }
}
