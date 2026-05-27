//! The nine MCP tools exposed by genATP v0.1.0.
//!
//! Each tool consists of:
//!   1. A stable name + description.
//!   2. A JSON-Schema for its arguments (rendered into the
//!      `tools/list` response so MCP clients can prompt the LLM).
//!   3. A dispatch function that takes the parsed arguments + the
//!      bound [`McpSession`] and returns a tool result.
//!
//! Tools are **stateless** — all mutable state lives on [`McpSession`].

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{McpError, Result};
use crate::protocol::{ToolContent, ToolDescriptor, ToolsCallResult};
use crate::session::McpSession;

/// Return the canonical list of tool descriptors served in
/// `tools/list`. Order is stable across calls.
#[must_use]
pub fn descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "navigate".into(),
            description: "Navigate the active browser tab to a URL. Returns the final URL after redirects. The agent should call `snapshot` afterwards before issuing any element-targeting actions.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["url"],
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute URL (must start with http:// or https://).",
                        "format": "uri"
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "snapshot".into(),
            description: "Capture a compressed accessibility snapshot of the current page. Returns JSON listing interactive nodes with stable `@eN` refs that subsequent `click` / `fill` calls use to identify elements.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        ToolDescriptor {
            name: "click".into(),
            description: "Click an element identified by a perception ref (`@eN`) that appeared in the most recent snapshot.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["ref"],
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "An element ref like `@e3`, as emitted by `snapshot`.",
                        "pattern": "^@e[0-9]+$"
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "fill".into(),
            description: "Type a value into a text input / textarea identified by a perception ref. Fires `input` and `change` events so JS frameworks observe the update.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["ref", "value"],
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "Input ref like `@e5`, as emitted by `snapshot`.",
                        "pattern": "^@e[0-9]+$"
                    },
                    "value": {
                        "type": "string",
                        "description": "The new value to set."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "extract".into(),
            description: "Read visible text from the most recent snapshot. Pass `query` to filter (case-insensitive substring); omit it for the full page text.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional case-insensitive substring filter."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "scroll".into(),
            description: "Scroll the viewport by relative pixel deltas (positive = right/down). Useful for revealing off-screen elements before snapshotting.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "dx": {
                        "type": "integer",
                        "default": 0,
                        "description": "Horizontal scroll delta in CSS pixels."
                    },
                    "dy": {
                        "type": "integer",
                        "default": 0,
                        "description": "Vertical scroll delta in CSS pixels."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "screenshot".into(),
            description: "Capture a PNG screenshot of the current viewport. Returned as base64-encoded `image/png` content for the LLM to inspect visually.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        ToolDescriptor {
            name: "get_url".into(),
            description: "Return the current URL of the active tab.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        ToolDescriptor {
            name: "go_back".into(),
            description: "Navigate one step back in the browser history. Returns `true` if the navigation actually happened, `false` if there was nothing to go back to.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Argument parsers — one per tool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NavigateArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
struct RefArgs {
    r#ref: String,
}

#[derive(Debug, Deserialize)]
struct FillArgs {
    r#ref: String,
    value: String,
}

#[derive(Debug, Deserialize, Default)]
struct ScrollArgs {
    #[serde(default)]
    dx: i32,
    #[serde(default)]
    dy: i32,
}

#[derive(Debug, Deserialize, Default)]
struct ExtractArgs {
    #[serde(default)]
    query: Option<String>,
}

fn parse_args<T: for<'de> Deserialize<'de>>(args: &Value, tool: &str) -> Result<T> {
    serde_json::from_value(args.clone())
        .map_err(|e| McpError::InvalidParams(format!("invalid arguments for `{tool}`: {e}")))
}

/// Render an OK text result.
fn text_ok(text: impl Into<String>) -> ToolsCallResult {
    ToolsCallResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: false,
    }
}

/// Render an error text result (`isError: true`). Used for domain
/// failures (e.g. ref not in scope) so the LLM can see + recover.
fn text_err(text: impl Into<String>) -> ToolsCallResult {
    ToolsCallResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: true,
    }
}

/// Dispatch a parsed `tools/call`. Returns a [`ToolsCallResult`] —
/// even soft failures (bad ref, navigation timeout) come back here
/// with `isError: true` rather than as JSON-RPC errors, so the LLM
/// can recover.
///
/// Hard JSON-RPC errors (unknown tool, malformed arguments) escape
/// via the [`McpError`] return.
///
/// # Errors
/// - [`McpError::UnknownTool`] if `name` isn't one of the nine.
/// - [`McpError::InvalidParams`] if `arguments` fails schema validation.
pub async fn dispatch(
    session: &McpSession,
    name: &str,
    arguments: &Value,
) -> Result<ToolsCallResult> {
    match name {
        "navigate" => {
            let a: NavigateArgs = parse_args(arguments, name)?;
            match session.navigate(&a.url).await {
                Ok(final_url) => Ok(text_ok(format!("navigated to {final_url}"))),
                Err(McpError::Browser(e)) => Ok(text_err(format!("navigation failed: {e}"))),
                Err(e) => Err(e),
            }
        }
        "snapshot" => match session.snapshot().await {
            Ok(json) => Ok(text_ok(json)),
            Err(McpError::Browser(e)) => Ok(text_err(format!("snapshot failed: {e}"))),
            Err(e) => Err(e),
        },
        "click" => {
            let a: RefArgs = parse_args(arguments, name)?;
            match session.click(&a.r#ref).await {
                Ok(()) => Ok(text_ok(format!("clicked {}", a.r#ref))),
                Err(McpError::ToolFailed(m)) => Ok(text_err(m)),
                Err(McpError::Browser(e)) => Ok(text_err(format!("click failed: {e}"))),
                Err(e) => Err(e),
            }
        }
        "fill" => {
            let a: FillArgs = parse_args(arguments, name)?;
            match session.fill(&a.r#ref, &a.value).await {
                Ok(()) => Ok(text_ok(format!(
                    "filled {} with {} chars",
                    a.r#ref,
                    a.value.chars().count()
                ))),
                Err(McpError::ToolFailed(m)) => Ok(text_err(m)),
                Err(McpError::Browser(e)) => Ok(text_err(format!("fill failed: {e}"))),
                Err(e) => Err(e),
            }
        }
        "extract" => {
            let a: ExtractArgs = parse_args(arguments, name).unwrap_or_default();
            let text = session.extract(a.query.as_deref()).await?;
            Ok(text_ok(text))
        }
        "scroll" => {
            let a: ScrollArgs = parse_args(arguments, name).unwrap_or_default();
            match session.scroll(a.dx, a.dy).await {
                Ok(()) => Ok(text_ok(format!("scrolled by ({}, {})", a.dx, a.dy))),
                Err(McpError::Browser(e)) => Ok(text_err(format!("scroll failed: {e}"))),
                Err(e) => Err(e),
            }
        }
        "screenshot" => match session.screenshot_png().await {
            Ok(bytes) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(ToolsCallResult {
                    content: vec![ToolContent::Image {
                        data: b64,
                        mime_type: "image/png".into(),
                    }],
                    is_error: false,
                })
            }
            Err(McpError::Browser(e)) => Ok(text_err(format!("screenshot failed: {e}"))),
            Err(e) => Err(e),
        },
        "get_url" => match session.current_url().await {
            Ok(url) => Ok(text_ok(url)),
            Err(McpError::Browser(e)) => Ok(text_err(format!("get_url failed: {e}"))),
            Err(e) => Err(e),
        },
        "go_back" => match session.go_back().await {
            Ok(true) => Ok(text_ok("navigated back")),
            Ok(false) => Ok(text_ok("no prior page in history")),
            Err(McpError::Browser(e)) => Ok(text_err(format!("go_back failed: {e}"))),
            Err(e) => Err(e),
        },
        other => Err(McpError::UnknownTool(other.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_emit_nine_stable_tools() {
        let d = descriptors();
        assert_eq!(d.len(), 9);
        let names: Vec<&str> = d.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "navigate",
                "snapshot",
                "click",
                "fill",
                "extract",
                "scroll",
                "screenshot",
                "get_url",
                "go_back",
            ]
        );
    }

    #[test]
    fn each_input_schema_is_a_valid_json_schema_object() {
        for t in descriptors() {
            assert_eq!(
                t.input_schema.get("type").and_then(Value::as_str),
                Some("object"),
                "tool {} schema is not a JSON object",
                t.name
            );
            assert!(
                t.input_schema.get("properties").is_some(),
                "tool {} schema missing `properties`",
                t.name
            );
        }
    }

    #[test]
    fn navigate_schema_requires_url() {
        let nav = descriptors()
            .into_iter()
            .find(|t| t.name == "navigate")
            .unwrap();
        let required = nav
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .unwrap();
        assert!(required.iter().any(|v| v == "url"));
    }

    #[test]
    fn click_schema_pattern_rejects_garbage() {
        let click = descriptors()
            .into_iter()
            .find(|t| t.name == "click")
            .unwrap();
        let pat = click
            .input_schema
            .pointer("/properties/ref/pattern")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(pat, "^@e[0-9]+$");
    }
}
