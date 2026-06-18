//! `genatp-mcp` — Model Context Protocol server.
//!
//! genATP's MCP server surface (v0.7.1). Exposes 13
//! browser-control tools over JSON-RPC 2.0:
//!
//! | Tool          | Purpose                                                 |
//! | ------------- | ------------------------------------------------------- |
//! | `navigate`          | Drive a tab to a URL                                    |
//! | `click_coordinate`  | Click at viewport-relative coordinates                  |
//! | `snapshot`          | Capture a compressed accessibility tree                 |
//! | `click`             | Click an element by `@eN` ref                           |
//! | `fill`              | Type into a text input by `@eN` ref                     |
//! | `extract`           | Read visible text (optionally filtered)                 |
//! | `scroll`            | Scroll the viewport                                     |
//! | `screenshot`        | Capture a PNG screenshot                                |
//! | `get_url`           | Return the current page URL                             |
//! | `go_back`           | Navigate back in history                                |
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐   ┌────────────────┐
//! │  stdio (NDJSON) │   │  HTTP POST /   │   transports
//! └────────┬────────┘   └────────┬───────┘
//!          │                     │
//!          └───── handle_bytes() ─┘
//!                    │
//!              ┌─────▼──────┐
//!              │ McpServer  │   protocol & dispatch
//!              └─────┬──────┘
//!                    │
//!              ┌─────▼──────┐
//!              │ McpSession │   one tab + RefMap + last snapshot
//!              └─────┬──────┘
//!                    │
//!     genatp-browser · genatp-perception · genatp-core
//! ```
//!
//! # Spec target
//!
//! Implements MCP **2024-11-05**. Any future spec changes are isolated
//! in [`crate::protocol`].
//!
//! # Quickstart (stdio)
//!
//! ```no_run
//! # async fn run() -> genatp_mcp::Result<()> {
//! use genatp_mcp::{McpServer, McpSession, SessionConfig, stdio};
//!
//! let session = McpSession::new(SessionConfig::default()).await?;
//! let server  = McpServer::new(session);
//! stdio::serve(server).await
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod protocol;
pub mod server;
pub mod session;
pub mod stdio;
pub mod tools;

#[cfg(feature = "http")]
pub mod http;

pub use crate::error::{McpError, Result};
pub use crate::server::McpServer;
pub use crate::session::{McpSession, SessionConfig};

/// Returns this crate's package version (matches the workspace
/// version emitted in `serverInfo`).
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
