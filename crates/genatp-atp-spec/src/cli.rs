//! CLI command handlers for ATP Spec operations.
//!
//! These functions are called by `genatp-cli` when the user invokes
//! `genatp atp <subcommand>`. They implement:
//!
//! - `genatp atp validate <path>` — parse + validate a `.atp.yml` file
//! - `genatp atp dump <path>` — dump the parsed IR as JSON for inspection
//! - `genatp atp list [directory]` — find all `.atp.yml` files under a dir
//! - `genatp atp steps <path>` — list step summaries for a spec

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AtpSpecError, Result as AtpResult};
use crate::ir::AtpSpec;
use crate::parser;
use crate::validator;

/// Validate a single `.atp.yml` file and return the parsed spec.
///
/// # Errors
/// Returns parse and validation errors.
pub fn validate_spec(path: &Path) -> AtpResult<AtpSpec> {
    let spec = parser::parse_file(path)?;
    validator::validate(&spec)?;
    Ok(spec)
}

/// Validate and return a human-readable summary.
///
/// # Errors
/// Returns a formatted error message on failure.
pub fn validate_spec_pretty(path: &Path) -> String {
    match validate_spec(path) {
        Ok(spec) => {
            let mut out = format!(
                "✓ {} — valid ({} steps, {} env vars, {} auth domains)",
                spec.name,
                spec.steps.len(),
                spec.env.len(),
                spec.auth.len(),
            );
            if let Some(ref desc) = spec.description {
                out.push_str(&format!("\n  Description: {desc}"));
            }
            if let Some(ref base) = spec.base_url {
                out.push_str(&format!("\n  Base URL: {base}"));
            }
            if let Some(ref budget) = spec.budget
                && !budget.is_unlimited()
            {
                out.push_str(&format!(
                    "\n  Budget: {} tokens, {}s, ${:.2}",
                    budget.tokens, budget.seconds, budget.usd
                ));
            }
            if !spec.auth.is_empty() {
                out.push_str(&format!("\n  Auth domains: {}", spec.auth.join(", ")));
            }
            // List step names.
            for (i, step) in spec.steps.iter().enumerate() {
                let name = step.action_name();
                out.push_str(&format!("\n  [{i:>2}] {name}"));
            }
            out
        }
        Err(e) => {
            format!("✗ validation failed:\n  {e}")
        }
    }
}

/// List all `.atp.yml` / `.atp.yaml` files under `dir` (non-recursive).
///
/// # Errors
/// Returns I/O errors.
pub fn list_specs(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut specs = Vec::new();
    if !dir.exists() {
        return Ok(specs);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name.ends_with(".atp.yml") || name.ends_with(".atp.yaml"))
        {
            specs.push(path);
        }
    }
    specs.sort();
    Ok(specs)
}

/// Dump the parsed IR of a spec as pretty-printed JSON.
///
/// # Errors
/// Returns parse errors or JSON serialization errors.
pub fn dump_spec_json(path: &Path) -> AtpResult<String> {
    let spec = parser::parse_file(path)?;
    serde_json::to_string_pretty(&spec).map_err(|e| AtpSpecError::Report(e.to_string()))
}

/// Print a step summary for a spec to stdout.
///
/// # Errors
/// Returns parse errors.
pub fn print_steps(path: &Path) -> AtpResult<String> {
    let spec = parser::parse_file(path)?;
    let mut out = String::new();
    out.push_str(&format!("Spec: {}", spec.name));
    out.push('\n');
    for (i, step) in spec.steps.iter().enumerate() {
        let summary = match step {
            crate::ir::Step::Navigate { url } => format!("navigate `{url}`"),
            crate::ir::Step::Click { selector, .. } => {
                let label = selector.label().unwrap_or("?");
                format!("click `{label}`")
            }
            crate::ir::Step::Fill { selector, value } => {
                let label = selector.label().unwrap_or("?");
                let masked = if value.starts_with("secret://") {
                    "***SECRET***"
                } else {
                    value
                };
                format!("fill `{label}` = `{masked}`")
            }
            crate::ir::Step::Select { selector, value } => {
                format!("select `{}` → `{value}`", selector.label().unwrap_or("?"))
            }
            crate::ir::Step::Press { key } => format!("press `{key}`"),
            crate::ir::Step::Scroll { dx, dy } => format!("scroll ({dx}, {dy})"),
            crate::ir::Step::Wait { ms } => format!("wait {ms}ms"),
            crate::ir::Step::Assert { text } => format!("assert `{text}`"),
            crate::ir::Step::AssertNot { text } => format!("assert_not `{text}`"),
            crate::ir::Step::AssertUrl { url, .. } => format!("assert_url `{url}`"),
            crate::ir::Step::Screenshot { name } => {
                format!("screenshot `{}`", name.as_deref().unwrap_or("unnamed"))
            }
            crate::ir::Step::Extract { query, .. } => {
                format!("extract `{}`", query.as_deref().unwrap_or("*"))
            }
            crate::ir::Step::Script { .. } => "script".to_string(),
        };
        out.push_str(&format!("  [{i:>2}] {summary}"));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_spec(path: &Path, yaml: &str) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
    }

    #[test]
    fn validate_valid_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.atp.yml");
        write_spec(
            &path,
            "version: 1\nname: \"Test\"\nsteps:\n  - navigate: \"https://example.com\"\n",
        );
        let result = validate_spec_pretty(&path);
        assert!(result.starts_with('✓'), "got: {result}");
    }

    #[test]
    fn validate_invalid_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.atp.yml");
        write_spec(&path, "version: 1\nname: \"\"\nsteps:\n  - wait: 100\n");
        let result = validate_spec_pretty(&path);
        assert!(result.starts_with('✗'));
    }

    #[test]
    fn list_specs_finds_only_atp_yml_files() {
        let tmp = tempfile::tempdir().unwrap();
        write_spec(
            &tmp.path().join("test.atp.yml"),
            "version: 1\nname: a\nsteps: []",
        );
        write_spec(&tmp.path().join("other.yaml"), "a: 1");
        write_spec(&tmp.path().join("data.json"), "{}");
        write_spec(
            &tmp.path().join("spec.atp.yaml"),
            "version: 1\nname: b\nsteps: []",
        );
        let specs = list_specs(tmp.path()).unwrap();
        assert_eq!(specs.len(), 2);
        let names: Vec<_> = specs
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"test.atp.yml".to_string()));
        assert!(names.contains(&"spec.atp.yaml".to_string()));
    }

    #[test]
    fn dump_spec_json_is_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.atp.yml");
        write_spec(
            &path,
            "version: 1\nname: \"JSON Dump\"\nsteps:\n  - navigate: \"https://example.com\"\n",
        );
        let json = dump_spec_json(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"].as_str(), Some("JSON Dump"));
        assert_eq!(parsed["version"].as_u64(), Some(1));
    }

    #[test]
    fn print_steps_contains_step_details() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.atp.yml");
        write_spec(
            &path,
            r#"
version: 1
name: "Steps Test"
steps:
  - navigate: "https://example.com"
  - click: "Login"
  - wait: 500
"#,
        );
        let out = print_steps(&path).unwrap();
        assert!(out.contains("navigate `https://example.com`"));
        assert!(out.contains("click `Login`"));
        assert!(out.contains("wait 500ms"));
    }
}
