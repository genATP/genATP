//! Unified error type for the MCP server.
//!
//! Errors flow up from sub-crates (`genatp-browser`, `genatp-perception`)
//! and from invalid client requests, and are converted to JSON-RPC
//! error objects at the dispatch boundary.

use crate::protocol::JsonRpcErrorObject;
use thiserror::Error;

/// Result alias for MCP fallible operations.
pub type Result<T> = std::result::Result<T, McpError>;

/// All error variants produced by the MCP server.
///
/// Each variant carries enough context to be rendered as a JSON-RPC
/// error object via [`McpError::to_jsonrpc`].
#[derive(Debug, Error)]
pub enum McpError {
    /// The wire protocol or message shape was invalid. JSON-RPC -32600.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// The client called a method we don't implement. JSON-RPC -32601.
    #[error("method not found: {0}")]
    MethodNotFound(String),

    /// The method exists but its parameters are malformed or missing.
    /// JSON-RPC -32602.
    #[error("invalid params: {0}")]
    InvalidParams(String),

    /// Unhandled server-side error. JSON-RPC -32603.
    #[error("internal error: {0}")]
    Internal(String),

    /// Tool name is not recognised. JSON-RPC -32602.
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    /// Tool dispatch failed at the domain level (e.g. ref not in scope).
    /// Surfaced via `isError: true` on the `tools/call` result; **not**
    /// emitted as a JSON-RPC error. Construct one of these to signal a
    /// soft failure to the client.
    #[error("tool error: {0}")]
    ToolFailed(String),

    /// Resource URI does not match any known resource.
    #[error("unknown resource: {0}")]
    UnknownResource(String),

    /// Underlying browser layer failure.
    #[error("browser error: {0}")]
    Browser(#[from] genatp_browser::BrowserError),

    /// Perception layer failure (snapshot, compression).
    #[error("perception error: {0}")]
    Perception(#[from] genatp_perception::error::PerceptionError),

    /// I/O failure on the transport.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialization failed.
    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Misconfiguration at startup (no port, invalid URL, …).
    #[error("configuration error: {0}")]
    Config(String),
}

impl McpError {
    /// Convert to the JSON-RPC error object surface, using the
    /// conventional codes:
    ///
    /// - `-32600` invalid request
    /// - `-32601` method not found
    /// - `-32602` invalid params (incl. unknown tool/resource)
    /// - `-32603` internal error (catch-all)
    /// - server-defined (`-32000`) for tool failures that escape to the
    ///   JSON-RPC layer (normally these go into the result body
    ///   instead — see [`McpError::ToolFailed`]).
    #[must_use]
    pub fn to_jsonrpc(&self) -> JsonRpcErrorObject {
        let (code, message) = match self {
            Self::InvalidRequest(m) => (-32600, m.clone()),
            Self::MethodNotFound(m) => (-32601, format!("method not found: {m}")),
            Self::InvalidParams(m) => (-32602, m.clone()),
            Self::UnknownTool(m) => (-32602, format!("unknown tool: {m}")),
            Self::UnknownResource(m) => (-32602, format!("unknown resource: {m}")),
            Self::ToolFailed(m) => (-32000, m.clone()),
            Self::Internal(m) => (-32603, m.clone()),
            Self::Browser(_)
            | Self::Perception(_)
            | Self::Io(_)
            | Self::Serde(_)
            | Self::Config(_) => (-32603, self.to_string()),
        };
        JsonRpcErrorObject {
            code,
            message,
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_invalid_params_to_minus_32602() {
        let e = McpError::InvalidParams("missing 'url'".into());
        let j = e.to_jsonrpc();
        assert_eq!(j.code, -32602);
        assert!(j.message.contains("missing 'url'"));
    }

    #[test]
    fn maps_method_not_found_to_minus_32601() {
        let e = McpError::MethodNotFound("foo".into());
        let j = e.to_jsonrpc();
        assert_eq!(j.code, -32601);
    }

    #[test]
    fn maps_internal_for_io() {
        let e = McpError::Io(std::io::Error::other("oops"));
        let j = e.to_jsonrpc();
        assert_eq!(j.code, -32603);
    }
}
