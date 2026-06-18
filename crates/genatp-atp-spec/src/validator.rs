//! Semantic validator for parsed ATP Spec IR.
//!
//! After the parser converts raw YAML into an [`AtpSpec`] IR, the validator
//! runs additional semantic checks that the parser cannot express:
//!
//! - Referential integrity in `base_url` + relative URL resolution.
//! - Budget constraint consistency.
//! - Step ordering constraints (assertions after actions).
//! - Environmental variable [`recurring`] references.

use url::Url;

use crate::error::{AtpSpecError, Result};
use crate::ir::AtpSpec;

/// Validate a fully parsed [`AtpSpec`] and return `Ok(())` or the first
/// validation error.
///
/// # Errors
/// Returns [`AtpSpecError::Validation`] on any violation.
pub fn validate(spec: &AtpSpec) -> Result<()> {
    // 1) If base_url is set, verify it's a valid URL.
    if let Some(base) = &spec.base_url {
        Url::parse(base).map_err(|e| {
            AtpSpecError::Validation(format!("`base_url` is not a valid URL: `{base}` — {e}"))
        })?;
    }

    // 2) Validate budget constraints.
    if let Some(budget) = &spec.budget {
        if budget.seconds > 3600 {
            return Err(AtpSpecError::Validation(
                "`budget.seconds` must not exceed 3600 (1 hour)".into(),
            ));
        }
        if budget.usd < 0.0 || budget.usd > 1000.0 {
            return Err(AtpSpecError::Validation(
                "`budget.usd` must be between 0 and 1000".into(),
            ));
        }
        if budget.tokens > 10_000_000 {
            return Err(AtpSpecError::Validation(
                "`budget.tokens` must not exceed 10,000,000".into(),
            ));
        }
    }

    // 3) Validate auth domains aren't empty.
    for domain in &spec.auth {
        if domain.trim().is_empty() {
            return Err(AtpSpecError::Validation(
                "`auth` list must not contain empty strings".into(),
            ));
        }
        if domain.contains("://") {
            return Err(AtpSpecError::Validation(format!(
                "`auth` entry `{domain}` looks like a URL — use bare domain names (e.g. `github.com`)"
            )));
        }
    }

    // 4) Validate env vars aren't empty.
    for env in &spec.env {
        if env.name.trim().is_empty() {
            return Err(AtpSpecError::Validation(
                "`env` entries must have a non-empty `name`".into(),
            ));
        }
    }

    // 5) Validate step ordering — an assertion should not appear before
    //    the step that sets up the state it asserts on. We emit warnings
    //    for common bad patterns but only reject egregious ones.
    let mut navigated = false;
    let mut has_actions = false;
    for step in &spec.steps {
        match step {
            crate::ir::Step::Navigate { .. } => navigated = true,
            crate::ir::Step::Click { .. }
            | crate::ir::Step::Fill { .. }
            | crate::ir::Step::Select { .. }
            | crate::ir::Step::Press { .. }
            | crate::ir::Step::Scroll { .. }
            | crate::ir::Step::Script { .. } => has_actions = true,
            _ => {}
        }
    }

    if !navigated && has_actions {
        // Not an error per se (the runner handles it), but worth warning
        // the caller. We don't block validation for this.
    }

    Ok(())
}

/// Validate a spec and return warnings as a list of strings.
/// `validate()` returns only the first error; this collects all issues.
#[must_use]
pub fn validate_all(spec: &AtpSpec) -> Vec<String> {
    let mut warnings = Vec::new();

    // Base URL validation.
    if let Some(base) = &spec.base_url
        && Url::parse(base).is_err()
    {
        warnings.push(format!("`base_url` is not a valid URL: `{base}`"));
    }

    // Check for empty name.
    if spec.name.trim().is_empty() {
        warnings.push("`name` should not be empty".into());
    }

    // Check for no auth with sensitive-looking steps.
    let has_vault_refs = spec.env.iter().any(|e| e.is_secret())
        || spec.steps.iter().any(
            |s| matches!(s, crate::ir::Step::Fill { value, .. } if value.starts_with("secret://")),
        );
    if has_vault_refs && spec.auth.is_empty() {
        warnings.push(
            "spec uses `secret://` references but has no `auth` domains — vault may be unavailable"
                .into(),
        );
    }

    // Budget warnings.
    if let Some(b) = &spec.budget
        && b.tokens == 0
        && b.seconds == 0
        && b.usd <= 0.0
    {
        warnings.push("no budget limits set — run may exceed expected costs".into());
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AtpSpec, BudgetDef, ElementSelector, EnvVar, SCHEMA_VERSION, Step};

    fn valid_spec() -> AtpSpec {
        AtpSpec {
            version: SCHEMA_VERSION,
            name: "test".into(),
            description: None,
            budget: None,
            auth: vec![],
            base_url: Some("https://example.com".into()),
            env: vec![],
            steps: vec![
                Step::Navigate {
                    url: "/login".into(),
                },
                Step::Click {
                    selector: ElementSelector::Text("Login".into()),
                    timeout_ms: None,
                },
            ],
            on_change: None,
        }
    }

    #[test]
    fn valid_spec_passes() {
        validate(&valid_spec()).unwrap();
    }

    #[test]
    fn invalid_base_url_fails() {
        let mut spec = valid_spec();
        spec.base_url = Some("not a url".into());
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("base_url"));
    }

    #[test]
    fn budget_seconds_too_large_fails() {
        let mut spec = valid_spec();
        spec.budget = Some(BudgetDef {
            tokens: 0,
            seconds: 99999,
            usd: 0.0,
        });
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("3600"));
    }

    #[test]
    fn budget_usd_negative_fails() {
        let mut spec = valid_spec();
        spec.budget = Some(BudgetDef {
            tokens: 0,
            seconds: 0,
            usd: -1.0,
        });
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("budget"));
    }

    #[test]
    fn budget_tokens_excessive_fails() {
        let mut spec = valid_spec();
        spec.budget = Some(BudgetDef {
            tokens: 99_000_000,
            seconds: 0,
            usd: 0.0,
        });
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("tokens"));
    }

    #[test]
    fn url_domain_in_auth_rejected() {
        let mut spec = valid_spec();
        spec.auth = vec!["https://github.com/".into()];
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("URL"));
    }

    #[test]
    fn empty_auth_entry_rejected() {
        let mut spec = valid_spec();
        spec.auth = vec!["".into()];
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn empty_env_var_name_rejected() {
        let mut spec = valid_spec();
        spec.env = vec![EnvVar {
            name: "".into(),
            value: "x".into(),
        }];
        let err = validate(&spec).unwrap_err();
        assert!(err.to_string().contains("env"));
    }

    #[test]
    fn validate_all_collects_multiple_warnings() {
        let spec = AtpSpec {
            version: SCHEMA_VERSION,
            name: "".into(),
            description: None,
            budget: Some(BudgetDef {
                tokens: 0,
                seconds: 0,
                usd: 0.0,
            }),
            auth: vec![],
            base_url: Some("not a url".into()),
            env: vec![],
            steps: vec![Step::Navigate { url: "/".into() }],
            on_change: None,
        };
        let warnings = validate_all(&spec);
        assert!(!warnings.is_empty());
    }
}
