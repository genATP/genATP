//! stdio transport — newline-delimited JSON-RPC over stdin/stdout.
//!
//! This is the **primary** transport mandated by the MCP spec and used
//! by Claude Desktop, Cursor, and the MCP Inspector.
//!
//! ## Framing
//!
//! - Each JSON-RPC message is one UTF-8 line (no `Content-Length`
//!   header — that's LSP, not MCP).
//! - Messages cannot contain raw `\n` inside JSON strings (we rely on
//!   `serde_json` always producing escaped `\n` in output, which it
//!   does by default).
//! - Logs / debug output **must** go to stderr — stdout is reserved
//!   for protocol traffic. The CLI wires `tracing` to stderr to keep
//!   this invariant.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};

use crate::error::Result;
use crate::server::McpServer;

/// Run the stdio transport until stdin closes (EOF) or an unrecoverable
/// I/O error occurs.
///
/// Drives messages serially: read line → dispatch → write response →
/// flush. We could pipeline, but real MCP clients send one request at
/// a time and serial dispatch keeps the session lock contention story
/// trivial.
///
/// # Errors
/// Returns [`crate::error::McpError::Io`] on stdin / stdout errors.
pub async fn serve(server: McpServer) -> Result<()> {
    serve_with_io(server, tokio::io::stdin(), tokio::io::stdout()).await
}

/// Same as [`serve`] but accepts explicit stdin / stdout handles, so
/// the integration tests can drive the loop with pipes.
pub async fn serve_with_io(server: McpServer, stdin: Stdin, mut stdout: Stdout) -> Result<()> {
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            tracing::info!("stdin EOF — exiting stdio transport");
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(resp) = server.handle_bytes(trimmed.as_bytes()).await {
            stdout.write_all(resp.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
}

/// Generic loop driver for transports that supply their own reader /
/// writer pair — used by tests and by the HTTP-transport's optional
/// "echo" connector. Reads NDJSON lines, dispatches, writes responses.
pub async fn serve_async<R, W>(server: McpServer, reader: R, mut writer: W) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(resp) = server.handle_bytes(trimmed.as_bytes()).await {
            writer.write_all(resp.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
}
