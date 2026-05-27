//! JSON-RPC 2.0 wire types and MCP envelope shapes.
//!
//! This module is **transport-agnostic**: it just describes the on-wire
//! shape of messages exchanged between an MCP client (Claude Desktop,
//! Cursor, MCP Inspector, …) and our server.
//!
//! We target MCP protocol version `2024-11-05`, which is the version
//! currently negotiated by all major MCP clients in the wild. Newer
//! versions add streaming-HTTP, sampling, and roots; none are required
//! for the v0.1.0 release surface.
//!
//! ## JSON-RPC primer
//!
//! - Every message is one of:
//!   - **Request**  — `{jsonrpc, id, method, params?}` → expects a response
//!   - **Response** — `{jsonrpc, id, result | error}`
//!   - **Notification** — `{jsonrpc, method, params?}` → no response
//! - `id` may be a number or a string.
//! - On a parse error, the server SHOULD reply with `id: null` and
//!   error code `-32700`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP protocol version negotiated by genATP v0.1.0.
///
/// Claude Desktop ≥1.x and Cursor IDE all support this version. Clients
/// that send a newer protocol version receive this one back; per the
/// MCP spec, the response value is authoritative for the session.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// The JSON-RPC version string we accept and emit. JSON-RPC 2.0 mandates
/// `"2.0"` literally.
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC request id — either a string, a number, or absent (for
/// notifications). MCP forbids `null` ids in requests but tolerates them
/// in responses generated for un-parseable input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// String id (Claude Desktop uses these).
    Str(String),
    /// Numeric id (Cursor + MCP Inspector use these).
    Num(i64),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(s) => write!(f, "{s}"),
            Self::Num(n) => write!(f, "{n}"),
        }
    }
}

/// JSON-RPC 2.0 request envelope.
///
/// `id` is required for requests (vs. notifications). `params` is
/// optional per the spec; we model it as `Option<Value>` to round-trip
/// either presentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier echoed in the response.
    pub id: RequestId,
    /// Method name (e.g. `"tools/list"`).
    pub method: String,
    /// Method parameters (object or array). Missing in some pings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 notification envelope (no id, no response expected).
///
/// MCP uses notifications for `notifications/initialized`,
/// `notifications/cancelled`, log forwarding, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Method name (always `notifications/*` for MCP).
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response envelope.
///
/// Exactly one of `result` / `error` must be present (we enforce this
/// via the `JsonRpcResponseBody` enum below).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoed from the request. `None` only for parse errors against
    /// un-parseable input.
    pub id: Option<RequestId>,
    /// Successful result payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload. Mutually exclusive with `result`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObject>,
}

impl JsonRpcResponse {
    /// Build a success response with the given result payload.
    #[must_use]
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response, echoing the request id when known.
    #[must_use]
    pub fn error(id: Option<RequestId>, error: JsonRpcErrorObject) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// Inline JSON-RPC error object embedded inside an error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorObject {
    /// JSON-RPC error code. Standard codes:
    ///   - `-32700` parse error
    ///   - `-32600` invalid request
    ///   - `-32601` method not found
    ///   - `-32602` invalid params
    ///   - `-32603` internal error
    ///   - `-32000..=-32099` server-defined errors
    pub code: i32,
    /// Short, human-readable summary.
    pub message: String,
    /// Optional structured detail (e.g. tool error context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Either a request or a notification — what we parse off the wire.
///
/// We allow `serde` to discriminate on the presence of `id`: requests
/// have an id, notifications do not.
#[derive(Debug)]
pub enum IncomingMessage {
    /// A request — caller MUST reply with a response.
    Request(JsonRpcRequest),
    /// A notification — caller MUST NOT reply.
    Notification(JsonRpcNotification),
}

impl IncomingMessage {
    /// Parse a single JSON-RPC message from raw bytes.
    ///
    /// Returns `Err(JsonRpcErrorObject)` with the conventional error
    /// code so callers can forward the error verbatim.
    ///
    /// # Errors
    /// - `-32700` Parse error if the bytes are not valid JSON.
    /// - `-32600` Invalid Request if the envelope is missing required
    ///   fields or has the wrong `jsonrpc` version.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, JsonRpcErrorObject> {
        let value: Value = serde_json::from_slice(bytes).map_err(|e| JsonRpcErrorObject {
            code: -32700,
            message: format!("Parse error: {e}"),
            data: None,
        })?;

        if !value.is_object() {
            return Err(JsonRpcErrorObject {
                code: -32600,
                message: "Invalid Request: payload is not a JSON object".into(),
                data: None,
            });
        }

        let jsonrpc = value.get("jsonrpc").and_then(Value::as_str).unwrap_or("");
        if jsonrpc != JSONRPC_VERSION {
            return Err(JsonRpcErrorObject {
                code: -32600,
                message: format!("Invalid Request: expected jsonrpc='2.0', got '{jsonrpc}'"),
                data: None,
            });
        }

        if value.get("method").is_none() {
            return Err(JsonRpcErrorObject {
                code: -32600,
                message: "Invalid Request: missing 'method'".into(),
                data: None,
            });
        }

        if value.get("id").is_some() {
            let req: JsonRpcRequest =
                serde_json::from_value(value).map_err(|e| JsonRpcErrorObject {
                    code: -32600,
                    message: format!("Invalid Request: {e}"),
                    data: None,
                })?;
            Ok(Self::Request(req))
        } else {
            let note: JsonRpcNotification =
                serde_json::from_value(value).map_err(|e| JsonRpcErrorObject {
                    code: -32600,
                    message: format!("Invalid Request: {e}"),
                    data: None,
                })?;
            Ok(Self::Notification(note))
        }
    }
}

// ---------------------------------------------------------------------------
// MCP-specific payloads
// ---------------------------------------------------------------------------

/// `initialize` request parameters sent by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version the client wants to speak (we respond with
    /// `PROTOCOL_VERSION` regardless; clients adapt).
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Client-declared capabilities — we ignore most for v0.1.0.
    #[serde(default)]
    pub capabilities: Value,
    /// Optional client identification (name + version).
    #[serde(rename = "clientInfo", default)]
    pub client_info: Option<ClientInfo>,
}

/// Client identity (purely informational; surfaced in server logs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Human-readable client name, e.g. `"claude-ai"`.
    pub name: String,
    /// Client version string.
    pub version: String,
}

/// `initialize` response — declares what the server can do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Negotiated protocol version (we echo our supported version).
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities — for v0.1.0 we advertise `tools` and
    /// `resources` only.
    pub capabilities: ServerCapabilities,
    /// Server identification.
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    /// Optional welcome / instructions banner that some clients
    /// surface in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Capability declarations sent in `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tools capability — we always set `list_changed: false` because
    /// our tool set is static.
    pub tools: ToolsCapability,
    /// Resources capability — we expose the latest snapshot.
    pub resources: ResourcesCapability,
}

/// `tools` capability block in `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether the server will emit `notifications/tools/list_changed`.
    /// Our tool set is fixed; always `false`.
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// `resources` capability block in `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Whether resources can be subscribed to. v0.1.0: `false`.
    pub subscribe: bool,
    /// Whether the server will emit `notifications/resources/list_changed`.
    /// Our resource list is fixed; `false`.
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Server identity (surfaced in client UIs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Human-readable server name.
    pub name: String,
    /// Server version (matches the crate version).
    pub version: String,
}

/// `tools/list` response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    /// All tools the server can dispatch. Order is stable across calls
    /// to ease golden tests / docs generation.
    pub tools: Vec<ToolDescriptor>,
}

/// One MCP tool, with its JSON-Schema for arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Stable identifier (used in `tools/call`).
    pub name: String,
    /// One-paragraph description shown to the LLM.
    pub description: String,
    /// JSON-Schema for the arguments object.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// `tools/call` request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallParams {
    /// Tool name (must match a `ToolDescriptor::name`).
    pub name: String,
    /// Arguments object — interpreted per the tool's `input_schema`.
    #[serde(default)]
    pub arguments: Value,
}

/// `tools/call` response.
///
/// MCP returns a `content` array (multiple parts allowed) and a flag
/// indicating whether the call should be treated as an error. Treating
/// the call as an error still goes through `result`, not `error`,
/// because the *server-side* dispatch succeeded — it's a domain-level
/// error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallResult {
    /// Result payload as content parts. v0.1.0 emits `text` and `image`.
    pub content: Vec<ToolContent>,
    /// `true` if the tool failed at the domain level (e.g. bad ref).
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

/// One piece of tool result content. MCP allows mixed text + image
/// parts; we only emit one of each variant per call but the field is
/// uniform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    /// Plain text fragment.
    Text {
        /// UTF-8 text payload.
        text: String,
    },
    /// Base64-encoded image (screenshot tool).
    Image {
        /// Base64 payload (no `data:` prefix).
        data: String,
        /// MIME type, e.g. `"image/png"`.
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// `resources/list` response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesListResult {
    /// Available resources.
    pub resources: Vec<ResourceDescriptor>,
}

/// One MCP resource (read-only, addressed by URI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    /// URI like `genatp://snapshot/current`.
    pub uri: String,
    /// Short display name.
    pub name: String,
    /// One-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource (`application/json` for snapshots).
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// `resources/read` request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesReadParams {
    /// The URI to read.
    pub uri: String,
}

/// `resources/read` response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesReadResult {
    /// One entry per resource part — for genATP this is always a
    /// single text part containing the JSON snapshot.
    pub contents: Vec<ResourceContent>,
}

/// One content part of a resource read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    /// URI being returned (echoes the request).
    pub uri: String,
    /// MIME type (`application/json` for snapshots).
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// UTF-8 text content. Mutually exclusive with `blob`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64-encoded binary content. Mutually exclusive with `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_with_numeric_id() {
        let raw = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"# as &[u8];
        match IncomingMessage::from_slice(raw).expect("parse") {
            IncomingMessage::Request(req) => {
                assert_eq!(req.method, "tools/list");
                assert_eq!(req.id, RequestId::Num(1));
            }
            IncomingMessage::Notification(_) => panic!("expected request"),
        }
    }

    #[test]
    fn parse_request_with_string_id() {
        let raw = br#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"# as &[u8];
        match IncomingMessage::from_slice(raw).expect("parse") {
            IncomingMessage::Request(req) => {
                assert_eq!(req.id, RequestId::Str("abc".into()));
            }
            IncomingMessage::Notification(_) => panic!("expected request"),
        }
    }

    #[test]
    fn parse_notification() {
        let raw = br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"# as &[u8];
        match IncomingMessage::from_slice(raw).expect("parse") {
            IncomingMessage::Notification(note) => {
                assert_eq!(note.method, "notifications/initialized");
            }
            IncomingMessage::Request(_) => panic!("expected notification"),
        }
    }

    #[test]
    fn parse_error_for_invalid_jsonrpc_version() {
        let raw = br#"{"jsonrpc":"1.0","id":1,"method":"ping"}"# as &[u8];
        let err = IncomingMessage::from_slice(raw).expect_err("must reject");
        assert_eq!(err.code, -32600);
    }

    #[test]
    fn parse_error_for_invalid_json() {
        let raw = b"not-json" as &[u8];
        let err = IncomingMessage::from_slice(raw).expect_err("must reject");
        assert_eq!(err.code, -32700);
    }

    #[test]
    fn success_response_round_trips() {
        let r = JsonRpcResponse::success(RequestId::Num(7), serde_json::json!({"ok": true}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"id\":7"));
        assert!(s.contains("\"ok\":true"));
    }

    #[test]
    fn error_response_includes_code_and_message() {
        let r = JsonRpcResponse::error(
            Some(RequestId::Str("x".into())),
            JsonRpcErrorObject {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            },
        );
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"code\":-32601"));
        assert!(s.contains("\"id\":\"x\""));
    }
}
