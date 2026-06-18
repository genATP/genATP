//! Phase 4 integration test — call MCP tools through the HTTP
//! transport using a real Chrome.
//!
//! Auto-skips when Chrome is not installed.

#![cfg(feature = "http")]

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use serde_json::{Value, json};

use common::{FixtureServer, chrome_available, chrome_test_guard};
use genatp_mcp::{McpServer, McpSession, SessionConfig};

async fn rpc_post(client: &reqwest::Client, base: &str, body: Value) -> Value {
    let resp = client
        .post(base)
        .json(&body)
        .send()
        .await
        .expect("POST succeeds");
    assert!(resp.status().is_success() || resp.status() == reqwest::StatusCode::NO_CONTENT);
    if resp.status() == reqwest::StatusCode::NO_CONTENT {
        Value::Null
    } else {
        resp.json().await.expect("JSON body")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_transport_serves_initialize_and_tools_call() {
    if !chrome_available() {
        eprintln!("Chrome not available — skipping HTTP integration test");
        return;
    }
    let _chrome_lock = chrome_test_guard().await;

    let fixtures = FixtureServer::start().await;
    let url = fixtures.url("form.html");

    let session = McpSession::new(SessionConfig::default()).await.unwrap();
    let server = McpServer::new(session);

    // Bind on an ephemeral port and remember it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let app = genatp_mcp::http::router(server);
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // Wait until the port is reachable (CI race tolerance).
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    for _ in 0..30 {
        if client.get(format!("{base}/healthz")).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let init = rpc_post(
        &client,
        &base,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "http-test", "version": "0"}
            }
        }),
    )
    .await;
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");

    // notifications produce 204 — confirm the route exists and silent.
    rpc_post(
        &client,
        &base,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }),
    )
    .await;

    let list = rpc_post(
        &client,
        &base,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    )
    .await;
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 13);

    let nav = rpc_post(
        &client,
        &base,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "navigate", "arguments": {"url": url}}
        }),
    )
    .await;
    assert_eq!(nav["result"]["isError"], false, "navigate: {nav:?}");

    let snap = rpc_post(
        &client,
        &base,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "snapshot", "arguments": {}}
        }),
    )
    .await;
    assert_eq!(snap["result"]["isError"], false);
    let snap_text = snap["result"]["content"][0]["text"].as_str().unwrap();
    assert!(snap_text.contains("\"nodes\""));

    server_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_healthz_returns_ok() {
    // This one doesn't need Chrome — pure HTTP smoke test.
    let session = match McpSession::new(SessionConfig::default()).await {
        Ok(s) => s,
        Err(_) => {
            eprintln!("session unavailable (no Chrome) — skipping HTTP healthz test");
            return;
        }
    };
    let server = McpServer::new(session);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let app = genatp_mcp::http::router(server);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let mut healthy = false;
    for _ in 0..20 {
        if let Ok(r) = client.get(format!("http://{addr}/healthz")).send().await
            && r.status().is_success()
        {
            healthy = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(healthy, "healthz never responded");

    task.abort();
}
