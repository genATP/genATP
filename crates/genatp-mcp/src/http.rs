//! HTTP transport — JSON-RPC over POST.
//!
//! This is a **secondary** transport for `genatp serve --port`,
//! useful for:
//!   - Browser-side MCP clients (the MCP TypeScript SDK ships an
//!     HTTP client.)
//!   - Curl / integration-test drivers that don't want to spawn a
//!     subprocess.
//!
//! ## Framing
//!
//! - Each HTTP request body is **one** JSON-RPC message.
//! - The response body is **one** JSON-RPC response.
//! - Notifications POST with no body in the response (HTTP 204).
//!
//! ## Endpoint
//!
//! - `POST /` — single JSON-RPC message in/out.
//! - `GET  /healthz` — liveness probe; returns `200 OK`.
//!
//! ## Security model (v0.1.0)
//!
//! Binds to `127.0.0.1:<port>` by default. We do NOT enable CORS for
//! arbitrary origins; the only client is local. Anyone with network
//! access to the port can drive the browser, so do not expose this
//! to the public internet.

#![cfg(feature = "http")]

use std::net::SocketAddr;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::error::Result;
use crate::server::McpServer;

/// Build the axum router that hosts the MCP HTTP transport. Exposed
/// so callers can compose with their own middleware in tests.
pub fn router(server: McpServer) -> Router {
    Router::new()
        .route("/", post(handle_rpc))
        .route("/healthz", get(healthz))
        .with_state(server)
}

/// Bind to `addr` and serve the MCP transport until the future is
/// dropped or the listener errors out.
///
/// # Errors
/// Returns [`crate::error::McpError::Io`] if the listener fails.
pub async fn serve(server: McpServer, addr: SocketAddr) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "MCP HTTP transport listening");
    axum::serve(listener, router(server))
        .await
        .map_err(crate::error::McpError::Io)?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn handle_rpc(State(server): State<McpServer>, body: Bytes) -> Response {
    match server.handle_bytes(&body).await {
        Some(resp) => {
            (StatusCode::OK, [("content-type", "application/json")], resp).into_response()
        }
        // Notifications — 204 No Content per JSON-RPC convention.
        None => StatusCode::NO_CONTENT.into_response(),
    }
}
