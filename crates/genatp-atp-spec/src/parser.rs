//! YAML parser for ATP Spec files (`.atp.yml` / `.atp.yaml`).
//!
//! Parses raw YAML into the [`ir::AtpSpec`] IR and runs basic structural
//! validation (required fields, version check). Semantic validation
//! (budget ranges, referential integrity) lives in [`crate::validator`].

use std::fs;
use std::path::Path;

use serde_norway::Value;

use crate::error::{AtpSpecError, Result};
use crate::ir::{self, AtpSpec, Step};

/// Parse an ATP Spec file from disk.
///
/// # Errors
/// Returns [`AtpSpecError::ReadError`] if the file cannot be read,
/// [`AtpSpecError::YamlParse`] if the YAML is malformed,
/// [`AtpSpecError::UnsupportedVersion`] if the version is unsupported.
pub fn parse_file(path: &Path) -> Result<AtpSpec> {
    let contents = fs::read_to_string(path).map_err(|e| AtpSpecError::ReadError {
        path: path.to_path_buf(),
        source: e,
    })?;
    parse_str(&contents, path)
}

/// Parse an ATP Spec from a string.
///
/// # Errors
/// As for [`parse_file`].
pub fn parse_str(contents: &str, source: &Path) -> Result<AtpSpec> {
    // 1) Parse raw YAML to Value.
    let raw: Value = serde_norway::from_str(contents).map_err(|e| AtpSpecError::YamlParse {
        path: source.to_path_buf(),
        detail: e.to_string(),
    })?;

    // 2) Check it's a mapping at the top level.
    if !raw.is_mapping() {
        return Err(AtpSpecError::Validation(
            "spec must be a YAML mapping (key/value pairs) at the top level".into(),
        ));
    }

    // 3) Extract scalar fields using Value::get() which takes &str.
    let version = get_u32(&raw, "version", source)?
        .ok_or_else(|| AtpSpecError::Validation("missing required field `version`".into()))?;

    if version != ir::SCHEMA_VERSION {
        return Err(AtpSpecError::UnsupportedVersion {
            found: version,
            supported: ir::SCHEMA_VERSION,
        });
    }

    let name = get_string(&raw, "name", source)?
        .ok_or_else(|| AtpSpecError::Validation("missing required field `name`".into()))?;
    if name.trim().is_empty() {
        return Err(AtpSpecError::Validation(
            "`name` must be a non-empty string".into(),
        ));
    }

    let description = get_string(&raw, "description", source)?;
    let base_url = get_string(&raw, "base_url", source)?;
    let auth = get_string_list(&raw, "auth", source)?;

    // 4) Parse budget.
    let budget = match raw.get("budget") {
        Some(b) if b.is_mapping() => Some(parse_budget(b, source)?),
        _ => None,
    };

    // 5) Parse env.
    let env = match raw.get("env") {
        Some(e) if e.is_mapping() => {
            let mut vars = Vec::new();
            if let Some(env_map) = e.as_mapping() {
                for (k, v) in env_map {
                    let key_str = key_as_str(k, "env key", source)?;
                    let val_str = value_as_str(v, "env value", source)?;
                    vars.push(ir::EnvVar {
                        name: key_str.to_string(),
                        value: val_str.to_string(),
                    });
                }
            }
            vars
        }
        _ => Vec::new(),
    };

    // 6) Parse on_change policy.
    let on_change = match raw.get("on_change") {
        Some(c) if c.is_mapping() => Some(parse_change_policy(c, source)?),
        _ => None,
    };

    // 7) Parse steps.
    let steps = parse_steps(&raw, source)?;
    if steps.is_empty() {
        return Err(AtpSpecError::Validation(
            "spec must contain at least one step".into(),
        ));
    }

    Ok(AtpSpec {
        version,
        name,
        description,
        budget,
        auth,
        base_url,
        env,
        steps,
        on_change,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Parse the `steps` list.
fn parse_steps(raw: &Value, source: &Path) -> Result<Vec<Step>> {
    let steps_val = match raw.get("steps") {
        Some(v) => v,
        None => {
            return Err(AtpSpecError::Validation(
                "missing required field `steps`".into(),
            ));
        }
    };

    let steps_seq = match steps_val.as_sequence() {
        Some(s) => s,
        _ => {
            return Err(AtpSpecError::Validation(
                "`steps` must be a list of action objects".into(),
            ));
        }
    };

    let mut steps = Vec::with_capacity(steps_seq.len());
    for (idx, item) in steps_seq.iter().enumerate() {
        let step = parse_single_step(item, idx, source)?;
        steps.push(step);
    }
    Ok(steps)
}

/// Parse a single step entry.
fn parse_single_step(item: &Value, idx: usize, source: &Path) -> Result<Step> {
    let mapping = match item.as_mapping() {
        Some(m) => m,
        None => {
            return Err(AtpSpecError::YamlParse {
                path: source.to_path_buf(),
                detail: format!(
                    "step {} must be a mapping (key/value pair), got scalar",
                    idx
                ),
            });
        }
    };

    if mapping.len() != 1 {
        return Err(AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!(
                "step {} must have exactly one key (the action type), got {} keys",
                idx,
                mapping.len()
            ),
        });
    }

    let (action_key, action_val) = mapping.iter().next().unwrap();
    let action_name = key_as_str(action_key, "step action key", source)?;

    match action_name {
        "navigate" => {
            let url = value_as_str(action_val, "navigate url", source)?;
            Ok(Step::Navigate {
                url: url.to_string(),
            })
        }
        "click" => {
            let (selector, timeout) = parse_click_args(action_val, idx, source)?;
            Ok(Step::Click {
                selector,
                timeout_ms: timeout,
            })
        }
        "fill" => {
            let (selector, value) = parse_fill_args(action_val, idx, source)?;
            Ok(Step::Fill { selector, value })
        }
        "select" => {
            let (selector, value) = parse_fill_args(action_val, idx, source)?;
            Ok(Step::Select { selector, value })
        }
        "press" => {
            let key = value_as_str(action_val, "press key", source)?;
            Ok(Step::Press {
                key: key.to_string(),
            })
        }
        "scroll" => {
            let (dx, dy) = parse_scroll_args(action_val, source)?;
            Ok(Step::Scroll { dx, dy })
        }
        "wait" => {
            let ms = parse_wait_arg(action_val, idx, source)?;
            Ok(Step::Wait { ms })
        }
        "assert" => {
            let text = value_as_str(action_val, "assert text", source)?;
            Ok(Step::Assert {
                text: text.to_string(),
            })
        }
        "assert_not" => {
            let text = value_as_str(action_val, "assert_not text", source)?;
            Ok(Step::AssertNot {
                text: text.to_string(),
            })
        }
        "assert_url" => parse_assert_url(action_val, source),
        "screenshot" => {
            let name = match action_val {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                v if v.is_mapping() => v
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                _ => None,
            };
            Ok(Step::Screenshot { name })
        }
        "extract" => {
            let (query, into) = parse_extract_args(action_val, source)?;
            Ok(Step::Extract { query, into })
        }
        "script" => {
            let code = value_as_str(action_val, "script code", source)?;
            Ok(Step::Script {
                code: code.to_string(),
            })
        }
        other => Err(AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!(
                "step {}: unknown action `{other}` — expected one of: \
                 navigate, click, fill, select, press, scroll, wait, \
                 assert, assert_not, assert_url, screenshot, extract, script",
                idx
            ),
        }),
    }
}

/// Parse `click` action value — can be a string or mapping.
fn parse_click_args(
    val: &Value,
    idx: usize,
    _source: &Path,
) -> Result<(ir::ElementSelector, Option<u32>)> {
    match val {
        Value::String(s) => Ok((ir::ElementSelector::Text(s.clone()), None)),
        v if v.is_mapping() => {
            let selector = parse_selector(v)?;
            let timeout = v.get("timeout").and_then(|v| v.as_u64()).map(|n| n as u32);
            Ok((selector, timeout))
        }
        _ => Err(AtpSpecError::Validation(format!(
            "step {}: click value must be a string (element text/ref) or mapping",
            idx
        ))),
    }
}

/// Parse `fill` action value.
fn parse_fill_args(
    val: &Value,
    idx: usize,
    source: &Path,
) -> Result<(ir::ElementSelector, String)> {
    match val {
        Value::String(_) => Err(AtpSpecError::Validation(format!(
            "step {}: fill value must be a mapping with `selector`/`ref`/`label` and `value`/`from` fields",
            idx
        ))),
        v if v.is_mapping() => {
            let selector = parse_selector(v)?;
            let value = if let Some(v) = v.get("value") {
                value_as_str(v, "fill value", source)?.to_string()
            } else if let Some(f) = v.get("from") {
                // secret:// reference or env var
                value_as_str(f, "fill from", source)?.to_string()
            } else {
                return Err(AtpSpecError::Validation(format!(
                    "step {}: fill must have `value` or `from` field",
                    idx
                )));
            };
            Ok((selector, value))
        }
        _ => Err(AtpSpecError::Validation(format!(
            "step {}: fill value must be a mapping",
            idx
        ))),
    }
}

/// Parse a selector from a Value that is a mapping.
fn parse_selector(v: &Value) -> Result<ir::ElementSelector> {
    // Try ref first.
    if let Some(r) = v.get("ref")
        && let Some(s) = r.as_str()
    {
        return Ok(ir::ElementSelector::Structured {
            label: v
                .get("label")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            r#ref: Some(s.to_string()),
            css: v.get("css").and_then(|v| v.as_str()).map(|s| s.to_string()),
        });
    }
    // Try label.
    if let Some(l) = v.get("label")
        && let Some(s) = l.as_str()
    {
        return Ok(ir::ElementSelector::Structured {
            label: Some(s.to_string()),
            r#ref: None,
            css: v.get("css").and_then(|v| v.as_str()).map(|s| s.to_string()),
        });
    }
    // Fallback: if a "selector" field exists, treat as text.
    if let Some(sel) = v.get("selector")
        && let Some(s) = sel.as_str()
    {
        return Ok(ir::ElementSelector::Text(s.to_string()));
    }
    Err(AtpSpecError::Validation(
        "element selector must have `ref`, `label`, or `selector` field".into(),
    ))
}

/// Parse `scroll` action value.
fn parse_scroll_args(val: &Value, _source: &Path) -> Result<(i32, i32)> {
    match val {
        v if v.is_mapping() => {
            let dx = v.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dy = v.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            Ok((dx, dy))
        }
        Value::String(s) => {
            // "0,200" format
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() == 2 {
                let dx = parts[0].trim().parse::<i32>().unwrap_or(0);
                let dy = parts[1].trim().parse::<i32>().unwrap_or(0);
                Ok((dx, dy))
            } else {
                Ok((0, 0))
            }
        }
        _ => Ok((0, 0)),
    }
}

/// Parse `wait` action value — can be number or string "1000".
fn parse_wait_arg(val: &Value, idx: usize, _source: &Path) -> Result<u32> {
    match val {
        Value::Number(n) => n.as_u64().map(|u| u as u32).ok_or_else(|| {
            AtpSpecError::Validation(format!("step {}: wait value out of range", idx))
        }),
        Value::String(s) => s.parse::<u32>().map_err(|_| {
            AtpSpecError::Validation(format!("step {}: wait value must be a valid number", idx))
        }),
        _ => Err(AtpSpecError::Validation(format!(
            "step {}: wait value must be a number or string",
            idx
        ))),
    }
}

/// Parse `assert_url` action.
fn parse_assert_url(val: &Value, _source: &Path) -> Result<Step> {
    match val {
        Value::String(s) => Ok(Step::AssertUrl {
            url: s.clone(),
            contains: false,
        }),
        v if v.is_mapping() => {
            let url = v
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    AtpSpecError::Validation("assert_url must have a `url` field".into())
                })?;
            let contains = v.get("contains").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(Step::AssertUrl { url, contains })
        }
        _ => Err(AtpSpecError::Validation(
            "assert_url value must be a string or mapping".into(),
        )),
    }
}

/// Parse `extract` action.
fn parse_extract_args(val: &Value, _source: &Path) -> Result<(Option<String>, Option<String>)> {
    match val {
        Value::String(s) => Ok((Some(s.clone()), None)),
        v if v.is_mapping() => {
            let query = v
                .get("query")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let into = v
                .get("into")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            Ok((query, into))
        }
        _ => Ok((None, None)),
    }
}

/// Parse a budget from a Value mapping.
fn parse_budget(v: &Value, _source: &Path) -> Result<ir::BudgetDef> {
    let tokens = v.get("tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let seconds = v.get("seconds").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let usd = v.get("usd").and_then(|v| v.as_f64()).unwrap_or(0.0);

    if seconds > 3600 {
        return Err(AtpSpecError::Validation(
            "budget.seconds must not exceed 3600 (1 hour)".into(),
        ));
    }

    Ok(ir::BudgetDef {
        tokens,
        seconds,
        usd,
    })
}

/// Parse change policy from a Value mapping.
fn parse_change_policy(v: &Value, _source: &Path) -> Result<ir::ChangePolicy> {
    let strategy_str = v.get("strategy").and_then(|v| v.as_str()).unwrap_or("heal");
    let strategy = match strategy_str {
        "heal" => ir::ChangeStrategy::Heal,
        "fail" => ir::ChangeStrategy::Fail,
        "record" => ir::ChangeStrategy::Record,
        other => {
            return Err(AtpSpecError::Validation(format!(
                "on_change.strategy must be one of: heal, fail, record; got `{other}`"
            )));
        }
    };

    let notify = match v.get("notify") {
        Some(Value::String(url)) if url.starts_with("slack://") => Some(ir::NotifyTarget::Slack {
            url: url.to_string(),
        }),
        Some(Value::String(url)) => Some(ir::NotifyTarget::Webhook {
            url: url.to_string(),
        }),
        _ => None,
    };

    Ok(ir::ChangePolicy { strategy, notify })
}

/// Extract a key as a string from a mapping key.
/// In serde_norway (like serde_yaml), mapping keys are `Value` objects.
fn key_as_str<'a>(key: &'a Value, context: &str, source: &Path) -> Result<&'a str> {
    key.as_str()
        .ok_or_else(|| AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!("expected string for mapping key `{context}`, got {:?}", key),
        })
        .and_then(|s| {
            if s.is_empty() {
                Err(AtpSpecError::YamlParse {
                    path: source.to_path_buf(),
                    detail: format!("expected non-empty string for `{context}`"),
                })
            } else {
                Ok(s)
            }
        })
}

/// Extract a scalar string from a YAML value.
fn value_as_str<'a>(v: &'a Value, context: &str, source: &Path) -> Result<&'a str> {
    v.as_str().ok_or_else(|| AtpSpecError::YamlParse {
        path: source.to_path_buf(),
        detail: format!("expected string for `{context}`, got {:?}", v),
    })
}

/// Get a required or optional `u32` field from a YAML mapping.
fn get_u32(raw: &Value, key: &str, source: &Path) -> Result<Option<u32>> {
    match raw.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(|u| u as u32)
            .ok_or_else(|| AtpSpecError::YamlParse {
                path: source.to_path_buf(),
                detail: format!("`{key}` value out of range"),
            })
            .map(Some),
        Some(other) => Err(AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!("expected number for `{key}`, got {:?}", other),
        }),
    }
}

/// Get an optional string field from a YAML mapping.
fn get_string(raw: &Value, key: &str, source: &Path) -> Result<Option<String>> {
    match raw.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!("expected string for `{key}`, got {:?}", other),
        }),
    }
}

/// Get a list of strings from a YAML field.
fn get_string_list(raw: &Value, key: &str, source: &Path) -> Result<Vec<String>> {
    match raw.get(key) {
        None => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(vec![s.clone()]),
        Some(Value::Sequence(seq)) => {
            let mut out = Vec::with_capacity(seq.len());
            for item in seq {
                out.push(
                    item.as_str()
                        .ok_or_else(|| AtpSpecError::YamlParse {
                            path: source.to_path_buf(),
                            detail: format!("expected string in `{key}` list, got {:?}", item),
                        })?
                        .to_string(),
                );
            }
            Ok(out)
        }
        Some(other) => Err(AtpSpecError::YamlParse {
            path: source.to_path_buf(),
            detail: format!("expected string or list for `{key}`, got {:?}", other),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_valid_spec() {
        let yaml = r#"
version: 1
name: "Minimal test"
steps:
  - navigate: "https://example.com"
  - assert: "Example Domain"
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(spec.version, 1);
        assert_eq!(spec.name, "Minimal test");
        assert_eq!(spec.steps.len(), 2);
        assert!(matches!(spec.steps[0], Step::Navigate { .. }));
        assert!(matches!(spec.steps[1], Step::Assert { .. }));
    }

    #[test]
    fn parse_spec_with_all_basic_actions() {
        let yaml = r#"
version: 1
name: "Full action test"
base_url: "https://example.com"
auth:
  - github.com
  - stripe.com
budget:
  tokens: 5000
  seconds: 60
  usd: 0.10
steps:
  - navigate: "/login"
  - click: "Login"
  - click:
      ref: "@e3"
      timeout: 5000
  - fill:
      label: "Username"
      value: "testuser"
  - fill:
      ref: "@e7"
      value: "mypassword"
  - select:
      label: "Country"
      value: "US"
  - press: "Enter"
  - scroll:
      dx: 0
      dy: 200
  - wait: 1000
  - assert: "Welcome"
  - assert_not: "Error"
  - assert_url: "https://example.com/dashboard"
  - assert_url:
      url: "dashboard"
      contains: true
  - screenshot: "after-login"
  - extract:
      query: "Price"
      into: "$PRICE"
  - script: "console.log('done')"
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(spec.steps.len(), 16);

        // Check specific steps.
        // Step 1: click "Login"
        assert!(matches!(&spec.steps[1], Step::Click { .. }));
        // Step 3: fill { label: "Username", value: "testuser" }
        assert!(matches!(&spec.steps[3], Step::Fill { .. }));
        // Step 7: scroll { dx: 0, dy: 200 }
        assert!(matches!(&spec.steps[7], Step::Scroll { .. }));
        // Step 8: wait 1000
        assert!(matches!(&spec.steps[8], Step::Wait { .. }));
        // Step 9: assert "Welcome"
        assert!(matches!(&spec.steps[9], Step::Assert { .. }));
        // Step 10: assert_not "Error"
        assert!(matches!(&spec.steps[10], Step::AssertNot { .. }));
        // Step 11: assert_url "https://example.com/dashboard"
        assert!(matches!(&spec.steps[11], Step::AssertUrl { .. }));
        // Step 13: screenshot "after-login"
        assert!(matches!(&spec.steps[13], Step::Screenshot { .. }));

        // Verify auth and budget.
        assert_eq!(spec.auth, vec!["github.com", "stripe.com"]);
        assert_eq!(spec.budget.unwrap().tokens, 5000);
    }

    #[test]
    fn parse_with_env_and_on_change() {
        let yaml = r#"
version: 1
name: "Env test"
env:
  API_KEY: "secret://stripe.com/sk_live"
  USERNAME: "admin"
steps:
  - navigate: "https://example.com"
on_change:
  strategy: heal
  notify: slack://alerts
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(spec.env.len(), 2);
        assert!(spec.env[0].is_secret());
        assert!(!spec.env[1].is_secret());
        assert_eq!(
            spec.on_change.as_ref().unwrap().strategy,
            ir::ChangeStrategy::Heal
        );
        assert!(spec.on_change.as_ref().unwrap().notify.is_some());
    }

    #[test]
    fn parse_with_mapping_click_selector() {
        let yaml = r#"
version: 1
name: "Selector test"
steps:
  - click:
      label: "Submit"
      css: ".btn-primary"
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        let step = &spec.steps[0];
        if let Step::Click { selector, .. } = step {
            assert_eq!(selector.label(), Some("Submit"));
        } else {
            panic!("expected Click step");
        }
    }

    #[test]
    fn parse_with_from_field() {
        let yaml = r#"
version: 1
name: "Fill from secret"
steps:
  - fill:
      ref: "@e5"
      from: "secret://stripe.com/card"
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        if let Step::Fill { value, .. } = &spec.steps[0] {
            assert_eq!(value, "secret://stripe.com/card");
        } else {
            panic!("expected Fill step");
        }
    }

    #[test]
    fn reject_missing_version() {
        let yaml = "name: no version\nsteps:\n  - wait: 100\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("`version`"));
    }

    #[test]
    fn reject_missing_name() {
        let yaml = "version: 1\nsteps:\n  - wait: 100\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("`name`"));
    }

    #[test]
    fn reject_missing_steps() {
        let yaml = "version: 1\nname: no steps\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("`steps`"));
    }

    #[test]
    fn reject_empty_steps() {
        let yaml = "version: 1\nname: empty steps\nsteps: []\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("at least one step"));
    }

    #[test]
    fn reject_unsupported_version() {
        let yaml = "version: 999\nname: too new\nsteps:\n  - wait: 100\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(matches!(err, AtpSpecError::UnsupportedVersion { .. }));
    }

    #[test]
    fn reject_unknown_action() {
        let yaml = "version: 1\nname: bad action\nsteps:\n  - teleport: \"now\"\n";
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("unknown action"));
    }

    #[test]
    fn reject_budget_seconds_too_large() {
        let yaml = r#"
version: 1
name: "bad budget"
budget:
  seconds: 99999
steps:
  - wait: 100
"#;
        let err = parse_str(yaml, Path::new("test.yml")).unwrap_err();
        assert!(err.to_string().contains("budget"));
    }

    #[test]
    fn parse_scroll_as_string() {
        let yaml = r#"
version: 1
name: "scroll test"
steps:
  - scroll: "0, 200"
  - scroll:
      dx: 10
      dy: 20
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(spec.steps.len(), 2);
    }

    #[test]
    fn parse_screenshot_without_name() {
        let yaml = r#"
version: 1
name: "screenshot"
steps:
  - screenshot: ""
  - screenshot: {}
"#;
        let spec = parse_str(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(spec.steps.len(), 2);
    }
}
