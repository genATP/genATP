//! Intermediate Representation (IR) for ATP Spec documents.
//!
//! These types represent a fully parsed and validated `.atp.yml` spec in
//! a form the runner can execute. The IR is provider- and engine-agnostic
//! — it only describes *what* to do, not *how* to do it.
//!
//! ## Design decisions
//!
//! - **Step actions are enums** so pattern matching is exhaustive at
//!   compile time — adding a new action type must be handled everywhere.
//! - **Assertions are first-class steps** rather than post-conditions,
//!   because the spec is executed linearly and each assertion fires
//!   immediately after the preceding action.
//! - **Text labels vs refs**: `click("Submit")` is resolved at run time
//!   by scanning the accessibility tree for a node whose name or role
//!   matches. `click("@e3")` is a direct CDP ref. The IR stores both
//!   cases as a single `ElementSelector` enum.

use serde::{Deserialize, Serialize};

/// Current schema version supported by this parser.
pub const SCHEMA_VERSION: u32 = 1;

/// A complete, validated ATP Spec document ready for execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtpSpec {
    /// Schema version (must be [`SCHEMA_VERSION`]).
    pub version: u32,
    /// Human-readable name for the spec (shown in reports).
    pub name: String,
    /// One-paragraph description (optional).
    #[serde(default)]
    pub description: Option<String>,
    /// Optional timeout budget.
    #[serde(default)]
    pub budget: Option<BudgetDef>,
    /// Domains that need auth vault access.
    #[serde(default)]
    pub auth: Vec<String>,
    /// Base URL for relative navigations.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Environment variables / secrets used by the spec.
    #[serde(default)]
    pub env: Vec<EnvVar>,
    /// Ordered list of steps.
    pub steps: Vec<Step>,
    /// Policy for what happens when the page changes between runs.
    #[serde(default)]
    pub on_change: Option<ChangePolicy>,
}

/// Budget constraints for a spec run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BudgetDef {
    /// Maximum total token usage (prompt + completion). 0 = unlimited.
    #[serde(default)]
    pub tokens: u32,
    /// Maximum wall-clock seconds. 0 = unlimited.
    #[serde(default)]
    pub seconds: u32,
    /// Maximum USD cost. 0.0 = unlimited.
    #[serde(default)]
    pub usd: f64,
}

impl BudgetDef {
    /// `true` if every dimension is unlimited.
    #[must_use]
    pub fn is_unlimited(self) -> bool {
        self.tokens == 0 && self.seconds == 0 && self.usd <= 0.0
    }
}

/// An environment variable or secret reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvVar {
    /// Variable or secret name.
    pub name: String,
    /// Value or `secret://<domain>/<key>` reference.
    pub value: String,
}

impl EnvVar {
    /// `true` if the value is a vault secret reference.
    #[must_use]
    pub fn is_secret(&self) -> bool {
        self.value.starts_with("secret://")
    }

    /// Parse the vault domain and key from a secret reference.
    /// Returns `None` if the value is not a secret reference.
    #[must_use]
    pub fn parse_secret(&self) -> Option<(&str, &str)> {
        let rest = self.value.strip_prefix("secret://")?;
        let (domain, key) = rest.split_once('/')?;
        Some((domain, key))
    }
}

/// One step in a spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    /// Navigate to a URL (absolute or relative to `base_url`).
    Navigate {
        /// Target URL. If relative, resolved against `base_url`.
        url: String,
    },
    /// Click an element by text label or ref.
    Click {
        /// How to identify the element.
        selector: ElementSelector,
        /// Optional timeout in ms to wait for the element.
        #[serde(default)]
        timeout_ms: Option<u32>,
    },
    /// Type a value into an element.
    Fill {
        /// How to identify the element.
        selector: ElementSelector,
        /// Value to type. Supports `secret://` references.
        value: String,
    },
    /// Select an option from a dropdown / listbox.
    Select {
        /// How to identify the element.
        selector: ElementSelector,
        /// Option value or label to select.
        value: String,
    },
    /// Press a keyboard key.
    Press {
        /// Key identifier (e.g. `"Enter"`, `"Tab"`, `"Escape"`).
        key: String,
    },
    /// Scroll the viewport by relative pixels.
    Scroll {
        /// Horizontal scroll delta in CSS pixels.
        #[serde(default)]
        dx: i32,
        /// Vertical scroll delta in CSS pixels.
        #[serde(default)]
        dy: i32,
    },
    /// Wait for a fixed number of milliseconds.
    Wait {
        /// Milliseconds to wait.
        ms: u32,
    },
    /// Assert that `text` is present on the page.
    Assert {
        /// The text to search for (case-insensitive).
        text: String,
    },
    /// Assert that `text` is NOT present on the page.
    AssertNot {
        /// The text that must be absent (case-insensitive).
        text: String,
    },
    /// Assert the current URL matches a pattern.
    AssertUrl {
        /// URL or URL substring.
        url: String,
        /// If `true`, the assertion passes if the URL contains `url`.
        /// If `false`, an exact match is required.
        #[serde(default)]
        contains: bool,
    },
    /// Take a screenshot (useful for debugging / reports).
    Screenshot {
        /// Optional label for the screenshot.
        #[serde(default)]
        name: Option<String>,
    },
    /// Extract text from the page and optionally store in a variable.
    Extract {
        /// Optional substring filter (case-insensitive).
        #[serde(default)]
        query: Option<String>,
        /// Variable name to store extracted text into (e.g. `$PRICE`).
        #[serde(default)]
        into: Option<String>,
    },
    /// Execute arbitrary JavaScript in the page context.
    Script {
        /// JavaScript code to execute.
        code: String,
    },
}

impl Step {
    /// Human-readable action name for error messages.
    #[must_use]
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::Navigate { .. } => "navigate",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Select { .. } => "select",
            Self::Press { .. } => "press",
            Self::Scroll { .. } => "scroll",
            Self::Wait { .. } => "wait",
            Self::Assert { .. } => "assert",
            Self::AssertNot { .. } => "assert_not",
            Self::AssertUrl { .. } => "assert_url",
            Self::Screenshot { .. } => "screenshot",
            Self::Extract { .. } => "extract",
            Self::Script { .. } => "script",
        }
    }

    /// Whether this step is an assertion.
    #[must_use]
    pub fn is_assertion(&self) -> bool {
        matches!(
            self,
            Self::Assert { .. } | Self::AssertNot { .. } | Self::AssertUrl { .. }
        )
    }
}

/// How to identify a DOM element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElementSelector {
    /// A bare string is treated as a text label or `@eN` ref.
    Text(String),
    /// Structured selector (future use: CSS, XPath, etc.).
    Structured {
        /// Text label to search for.
        label: Option<String>,
        /// `@eN` ref from a snapshot.
        r#ref: Option<String>,
        /// CSS selector (fallback).
        #[serde(default)]
        css: Option<String>,
    },
}

impl ElementSelector {
    /// Extract the text label, if this selector identifies by label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Structured { label, .. } => label.as_deref(),
        }
    }

    /// Extract the `@eN` ref, if this selector identifies by ref.
    #[must_use]
    pub fn r#ref(&self) -> Option<&str> {
        match self {
            Self::Text(s) => {
                if s.starts_with("@e") {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            Self::Structured { r#ref, .. } => r#ref.as_deref(),
        }
    }

    /// `true` if this is a direct `@eN` ref (no text resolution needed).
    #[must_use]
    pub fn is_ref(&self) -> bool {
        self.r#ref().is_some()
    }

    /// `true` if this is a text-label selector (needs run-time resolution).
    #[must_use]
    pub fn is_text_label(&self) -> bool {
        self.label().is_some() && !self.is_ref()
    }
}

/// Policy for handling page drift between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ChangeStrategy {
    /// Attempt to heal using LLM (default).
    #[default]
    Heal,
    /// Fail immediately on drift.
    Fail,
    /// Re-record the cassette on drift.
    Record,
}

/// Notification channel configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NotifyTarget {
    /// Slack webhook URL.
    Slack { url: String },
    /// Generic webhook URL.
    Webhook { url: String },
    /// Log-only (default if not specified).
    Log,
}

/// Change handling policy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangePolicy {
    /// Strategy for handling drift.
    #[serde(default)]
    pub strategy: ChangeStrategy,
    /// Notification channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<NotifyTarget>,
}

/// Result of a single step execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    /// 0-based step index.
    pub seq: u32,
    /// Action name.
    pub action: String,
    /// Whether the step succeeded.
    pub success: bool,
    /// Human-readable message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// For assertions: whether the assertion passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_passed: Option<bool>,
}

/// Result of a full spec execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecResult {
    /// Spec name.
    pub name: String,
    /// Whether every step succeeded.
    pub success: bool,
    /// Total wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Count of passed steps.
    pub passed: u32,
    /// Count of failed steps.
    pub failed: u32,
    /// Count of steps (including assertions).
    pub total: u32,
    /// Per-step results.
    pub steps: Vec<StepResult>,
    /// Spec file path (for reports).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_unlimited_by_default() {
        let b = BudgetDef {
            tokens: 0,
            seconds: 0,
            usd: 0.0,
        };
        assert!(b.is_unlimited());
    }

    #[test]
    fn budget_not_unlimited_when_set() {
        let b = BudgetDef {
            tokens: 1000,
            seconds: 30,
            usd: 0.05,
        };
        assert!(!b.is_unlimited());
    }

    #[test]
    fn env_var_secret_detection() {
        let plain = EnvVar {
            name: "KEY".into(),
            value: "plain_value".into(),
        };
        assert!(!plain.is_secret());

        let secret = EnvVar {
            name: "API_KEY".into(),
            value: "secret://stripe.com/sk_live".into(),
        };
        assert!(secret.is_secret());
        assert_eq!(secret.parse_secret(), Some(("stripe.com", "sk_live")));
    }

    #[test]
    fn element_selector_text_label() {
        let s = ElementSelector::Text("Submit".into());
        assert_eq!(s.label(), Some("Submit"));
        assert!(s.is_text_label());
        assert!(!s.is_ref());
    }

    #[test]
    fn element_selector_ref() {
        let s = ElementSelector::Text("@e3".into());
        assert_eq!(s.r#ref(), Some("@e3"));
        assert!(s.is_ref());
    }

    #[test]
    fn element_selector_structured() {
        let s = ElementSelector::Structured {
            label: Some("OK".into()),
            r#ref: None,
            css: None,
        };
        assert_eq!(s.label(), Some("OK"));
        assert!(s.is_text_label());

        let s2 = ElementSelector::Structured {
            label: None,
            r#ref: Some("@e5".into()),
            css: None,
        };
        assert_eq!(s2.r#ref(), Some("@e5"));
        assert!(s2.is_ref());
    }

    #[test]
    fn step_action_names() {
        assert_eq!(Step::Navigate { url: "".into() }.action_name(), "navigate");
        assert_eq!(
            Step::Click {
                selector: ElementSelector::Text("".into()),
                timeout_ms: None,
            }
            .action_name(),
            "click"
        );
        assert_eq!(Step::Assert { text: "".into() }.action_name(), "assert");
        assert_eq!(
            Step::AssertNot { text: "".into() }.action_name(),
            "assert_not"
        );
    }

    #[test]
    fn step_is_assertion_classification() {
        assert!(Step::Assert { text: "x".into() }.is_assertion());
        assert!(Step::AssertNot { text: "x".into() }.is_assertion());
        assert!(
            Step::AssertUrl {
                url: "".into(),
                contains: false,
            }
            .is_assertion()
        );
        assert!(!Step::Navigate { url: "".into() }.is_assertion());
        assert!(!Step::Wait { ms: 100 }.is_assertion());
    }

    #[test]
    fn serde_roundtrip_spec_result() {
        let r = SpecResult {
            name: "test".into(),
            success: true,
            duration_ms: 1234,
            passed: 2,
            failed: 0,
            total: 2,
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
                    action: "assert".into(),
                    success: true,
                    message: Some("found text".into()),
                    duration_ms: 100,
                    assertion_passed: Some(true),
                },
            ],
            spec_path: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: SpecResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn change_strategy_default_is_heal() {
        assert_eq!(ChangeStrategy::default(), ChangeStrategy::Heal);
    }

    #[test]
    fn change_policy_defaults() {
        let p = ChangePolicy::default();
        assert_eq!(p.strategy, ChangeStrategy::Heal);
        assert!(p.notify.is_none());
    }
}
