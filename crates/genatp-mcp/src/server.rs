//! Transport-agnostic message dispatcher.
//!
//! Wraps an [`McpSession`] and turns parsed [`IncomingMessage`]s into
//! response strings ready for the wire. Transports (stdio, HTTP) handle
//! framing; the server handles semantics.

use serde_json::{Value, json};
use tracing::warn;

use crate::error::{McpError, Result};
use crate::protocol::{
    IncomingMessage, InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse,
    PROTOCOL_VERSION, ResourceContent, ResourceDescriptor, ResourcesCapability,
    ResourcesListResult, ResourcesReadParams, ResourcesReadResult, ServerCapabilities, ServerInfo,
    ToolsCallParams, ToolsCallResult, ToolsCapability, ToolsListResult,
};
use crate::session::McpSession;
use crate::tools;

/// The MCP server core. Holds the session and routes JSON-RPC method
/// calls. Transport-independent.
#[derive(Clone, Debug)]
pub struct McpServer {
    session: McpSession,
    /// Server identifier emitted in `initialize.serverInfo`.
    server_name: &'static str,
    /// Server version emitted in `initialize.serverInfo`.
    server_version: &'static str,
}

impl McpServer {
    /// Build a server bound to the given session.
    #[must_use]
    pub fn new(session: McpSession) -> Self {
        Self {
            session,
            server_name: "genatp",
            server_version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// Borrow the underlying session (handy for tests).
    #[must_use]
    pub fn session(&self) -> &McpSession {
        &self.session
    }

    /// Handle one raw incoming message (already de-framed by the
    /// transport) and produce a serialised JSON-RPC response.
    ///
    /// Returns `None` for notifications (no reply expected). On parse
    /// errors, returns a JSON-RPC error with `id: null`.
    pub async fn handle_bytes(&self, raw: &[u8]) -> Option<String> {
        let parsed = match IncomingMessage::from_slice(raw) {
            Ok(m) => m,
            Err(err) => {
                let resp = JsonRpcResponse::error(None, err);
                return Some(serde_json::to_string(&resp).unwrap_or_else(|_| {
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal serialization failure"}}"#.into()
                }));
            }
        };

        match parsed {
            IncomingMessage::Request(req) => Some(self.handle_request(req).await),
            IncomingMessage::Notification(note) => {
                // We do not emit a response to notifications. Log
                // unknown / ignored notifications at debug level so
                // operators can audit chatter.
                if note.method == "notifications/initialized" {
                    tracing::info!("client signalled `notifications/initialized`");
                } else {
                    tracing::debug!(method = %note.method, "ignoring notification");
                }
                None
            }
        }
    }

    /// Handle a parsed JSON-RPC request and return a serialised
    /// response string.
    pub async fn handle_request(&self, req: JsonRpcRequest) -> String {
        let id = req.id.clone();
        let result = self.dispatch(req).await;
        let resp = match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(err) => {
                warn!(error = %err, "request failed");
                JsonRpcResponse::error(Some(id), err.to_jsonrpc())
            }
        };
        serde_json::to_string(&resp).unwrap_or_else(|_| Self::fallback_internal_error())
    }

    fn fallback_internal_error() -> String {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal serialization failure"}}"#.into()
    }

    /// Method-name dispatcher.
    async fn dispatch(&self, req: JsonRpcRequest) -> Result<Value> {
        match req.method.as_str() {
            "initialize" => self.handle_initialize(req.params).await,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.handle_tools_list()),
            "tools/call" => self.handle_tools_call(req.params).await,
            "resources/list" => Ok(self.handle_resources_list()),
            "resources/read" => self.handle_resources_read(req.params).await,
            // We do not implement `prompts/*`, `sampling/*`, `logging/*`,
            // `completion/*` for v0.1.0. Clients that probe these get a
            // proper -32601 rather than a generic error.
            other => Err(McpError::MethodNotFound(other.into())),
        }
    }

    async fn handle_initialize(&self, params: Option<Value>) -> Result<Value> {
        let params: InitializeParams = match params {
            Some(v) => serde_json::from_value(v)
                .map_err(|e| McpError::InvalidParams(format!("initialize: {e}")))?,
            None => {
                return Err(McpError::InvalidParams("initialize requires params".into()));
            }
        };
        if let Some(info) = &params.client_info {
            tracing::info!(client = %info.name, version = %info.version, protocol = %params.protocol_version, "initialize");
        }
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.into(),
            capabilities: ServerCapabilities {
                tools: ToolsCapability { list_changed: false },
                resources: ResourcesCapability {
                    subscribe: false,
                    list_changed: false,
                },
            },
            server_info: ServerInfo {
                name: self.server_name.into(),
                version: self.server_version.into(),
            },
            instructions: Some(
                "genATP browser-control server. Workflow: navigate(url) → snapshot() → click/fill (via `@eN` refs) → repeat. Always snapshot before targeting elements.".into(),
            ),
        };
        Ok(serde_json::to_value(result)?)
    }

    fn handle_tools_list(&self) -> Value {
        let list = ToolsListResult {
            tools: tools::descriptors(),
        };
        serde_json::to_value(list).expect("tools list serializes")
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value> {
        let params: ToolsCallParams = match params {
            Some(v) => serde_json::from_value(v)
                .map_err(|e| McpError::InvalidParams(format!("tools/call: {e}")))?,
            None => {
                return Err(McpError::InvalidParams("tools/call requires params".into()));
            }
        };
        let result: ToolsCallResult =
            tools::dispatch(&self.session, &params.name, &params.arguments).await?;
        Ok(serde_json::to_value(result)?)
    }

    fn handle_resources_list(&self) -> Value {
        let list = ResourcesListResult {
            resources: vec![ResourceDescriptor {
                uri: "genatp://snapshot/current".into(),
                name: "Current page snapshot".into(),
                description: Some(
                    "Most recent compressed accessibility snapshot. Captured by the `snapshot` tool.".into(),
                ),
                mime_type: Some("application/json".into()),
            }],
        };
        serde_json::to_value(list).expect("resources list serializes")
    }

    async fn handle_resources_read(&self, params: Option<Value>) -> Result<Value> {
        let params: ResourcesReadParams = match params {
            Some(v) => serde_json::from_value(v)
                .map_err(|e| McpError::InvalidParams(format!("resources/read: {e}")))?,
            None => {
                return Err(McpError::InvalidParams(
                    "resources/read requires params".into(),
                ));
            }
        };
        if params.uri != "genatp://snapshot/current" {
            return Err(McpError::UnknownResource(params.uri));
        }
        let body = self
            .session
            .last_snapshot_json()
            .await
            .unwrap_or_else(|| "{}".into());
        let result = ResourcesReadResult {
            contents: vec![ResourceContent {
                uri: params.uri,
                mime_type: Some("application/json".into()),
                text: Some(body),
                blob: None,
            }],
        };
        Ok(serde_json::to_value(result)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: i64, method: &str, params: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .unwrap()
    }

    /// Build a server with a dummy session that will never connect to
    /// Chrome (we only exercise the protocol surface that doesn't need
    /// a page). The browser is allocated lazily so this is safe.
    async fn fake_server() -> Option<McpServer> {
        // Try to construct a headless session. CI without Chrome will
        // skip — those tests are covered by integration tests instead.
        let cfg = crate::session::SessionConfig::default();
        let session = McpSession::new(cfg).await.ok()?;
        Some(McpServer::new(session))
    }

    #[tokio::test]
    async fn unknown_method_returns_minus_32601() {
        let Some(server) = fake_server().await else {
            return; // no Chrome available
        };
        let raw = request(1, "wat", json!({}));
        let resp = server.handle_bytes(&raw).await.unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn parse_error_returns_minus_32700_with_null_id() {
        let Some(server) = fake_server().await else {
            return;
        };
        let resp = server.handle_bytes(b"{not json").await.unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32700);
        assert!(v.get("id").map(Value::is_null).unwrap_or(false));
    }

    #[tokio::test]
    async fn initialize_negotiates_protocol_version() {
        let Some(server) = fake_server().await else {
            return;
        };
        let raw = request(
            1,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }),
        );
        let resp = server.handle_bytes(&raw).await.unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(v["result"]["serverInfo"]["name"], "genatp");
    }

    #[tokio::test]
    async fn tools_list_returns_nine_tools() {
        let Some(server) = fake_server().await else {
            return;
        };
        let raw = request(1, "tools/list", json!({}));
        let resp = server.handle_bytes(&raw).await.unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["tools"].as_array().unwrap().len(), 9);
    }

    #[tokio::test]
    async fn notifications_produce_no_response() {
        let Some(server) = fake_server().await else {
            return;
        };
        let raw = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .unwrap();
        assert!(server.handle_bytes(&raw).await.is_none());
    }
}
