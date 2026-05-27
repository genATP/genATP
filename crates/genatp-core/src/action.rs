//! Actions the LLM may request via tool calls.
//!
//! The orchestrator advertises these as JSON-Schema tool definitions to
//! the model and dispatches each `ToolCall` it returns through the
//! [`crate::dispatcher::ActionDispatcher`]. The enum is closed by design:
//! every action we accept must be modelled here so we can audit / mock /
//! disable it.

use serde::{Deserialize, Serialize};

use genatp_llm::{ToolCall, ToolDefinition};

use crate::error::CoreError;

/// Action variants. Serialised with `tag = "name"` so they look like:
/// `{"name": "click", "ref": "@e3"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Action {
    /// Navigate to a URL.
    Navigate { url: String },
    /// Click an interactive element identified by its `@eN` ref.
    Click { r#ref: String },
    /// Fill a text field by ref.
    Fill { r#ref: String, value: String },
    /// Press a single key on the focused element (or page if nothing focused).
    Press { key: String },
    /// Scroll by relative pixels.
    Scroll { dx: i32, dy: i32 },
    /// Wait for `ms` milliseconds (rare; usually a smell).
    Wait { ms: u32 },
    /// Signal that the task is complete with a final answer / summary.
    Finish { summary: String },
}

impl Action {
    /// Tool name as the model sees it.
    #[must_use]
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Navigate { .. } => "navigate",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Press { .. } => "press",
            Self::Scroll { .. } => "scroll",
            Self::Wait { .. } => "wait",
            Self::Finish { .. } => "finish",
        }
    }

    /// Parse from an LLM-emitted [`ToolCall`].
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidAction`] if the tool name is unknown or
    /// the arguments do not match the expected schema.
    pub fn from_tool_call(call: &ToolCall) -> Result<Self, CoreError> {
        let mut value =
            serde_json::Value::Object(call.arguments.as_object().cloned().unwrap_or_default());
        let obj = value
            .as_object_mut()
            .expect("we just built an object above");
        obj.insert("name".into(), serde_json::Value::String(call.name.clone()));

        serde_json::from_value::<Action>(value).map_err(|e| CoreError::InvalidAction {
            name: call.name.clone(),
            reason: e.to_string(),
        })
    }

    /// Standard JSON-Schema tool definitions advertised to the model.
    #[must_use]
    pub fn default_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new(
                "navigate",
                "Navigate the browser to a fully-qualified URL.",
                serde_json::json!({
                    "type": "object",
                    "properties": { "url": { "type": "string" } },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "click",
                "Click an element identified by the @eN ref in the current snapshot.",
                serde_json::json!({
                    "type": "object",
                    "properties": { "ref": { "type": "string", "pattern": "^@e\\d+$" } },
                    "required": ["ref"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "fill",
                "Type `value` into a textbox / input element by its @eN ref.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "ref": { "type": "string", "pattern": "^@e\\d+$" },
                        "value": { "type": "string" }
                    },
                    "required": ["ref", "value"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "press",
                "Press a single key (e.g. \"Enter\", \"Tab\", \"Escape\").",
                serde_json::json!({
                    "type": "object",
                    "properties": { "key": { "type": "string" } },
                    "required": ["key"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "scroll",
                "Scroll the viewport by (dx, dy) pixels.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dx": { "type": "integer" },
                        "dy": { "type": "integer" }
                    },
                    "required": ["dx", "dy"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "wait",
                "Pause execution for `ms` milliseconds. Avoid except when explicitly needed.",
                serde_json::json!({
                    "type": "object",
                    "properties": { "ms": { "type": "integer", "minimum": 0 } },
                    "required": ["ms"],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "finish",
                "Signal that the task is complete. Provide a final summary string.",
                serde_json::json!({
                    "type": "object",
                    "properties": { "summary": { "type": "string" } },
                    "required": ["summary"],
                    "additionalProperties": false
                }),
            ),
        ]
    }
}

/// Outcome of dispatching an [`Action`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    pub action: Action,
    pub success: bool,
    /// Human-readable status message (`"navigated"`, `"clicked"`, …).
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigate_round_trips_via_tool_call() {
        let call = ToolCall {
            id: "1".into(),
            name: "navigate".into(),
            arguments: serde_json::json!({ "url": "https://example.com" }),
        };
        let action = Action::from_tool_call(&call).unwrap();
        assert_eq!(
            action,
            Action::Navigate {
                url: "https://example.com".into()
            }
        );
    }

    #[test]
    fn click_round_trips_via_tool_call() {
        let call = ToolCall {
            id: "2".into(),
            name: "click".into(),
            arguments: serde_json::json!({ "ref": "@e3" }),
        };
        let action = Action::from_tool_call(&call).unwrap();
        assert_eq!(
            action,
            Action::Click {
                r#ref: "@e3".into()
            }
        );
    }

    #[test]
    fn unknown_tool_name_errors() {
        let call = ToolCall {
            id: "3".into(),
            name: "teleport".into(),
            arguments: serde_json::json!({}),
        };
        let err = Action::from_tool_call(&call).expect_err("must error");
        assert!(matches!(err, CoreError::InvalidAction { .. }));
    }

    #[test]
    fn default_tool_definitions_cover_every_variant() {
        let defs = Action::default_tool_definitions();
        let names: std::collections::HashSet<_> = defs.iter().map(|d| d.name.as_str()).collect();
        for variant in [
            "navigate", "click", "fill", "press", "scroll", "wait", "finish",
        ] {
            assert!(
                names.contains(variant),
                "missing definition for `{variant}`"
            );
        }
    }
}
