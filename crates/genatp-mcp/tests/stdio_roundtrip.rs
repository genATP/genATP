//! Phase 4 integration test — drive every tool through the stdio
//! transport against a real Chrome.
//!
//! Skipped automatically when Chrome is not installed (so CI machines
//! without a graphical browser don't break).

mod common;

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex};

use common::{FixtureServer, chrome_available, chrome_test_guard};
use genatp_mcp::{McpServer, McpSession, SessionConfig};

/// Helper: write one request, await one response. Panics on transport
/// errors — the tests want to fail loudly.
async fn rpc<W, R>(writer: &mut W, reader: &mut R, id: i64, method: &str, params: Value) -> Value
where
    W: tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncBufRead + Unpin,
{
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let line = serde_json::to_string(&req).unwrap();
    writer.write_all(line.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut response_line = String::new();
    reader.read_line(&mut response_line).await.unwrap();
    serde_json::from_str(&response_line).expect("valid JSON-RPC response")
}

async fn notify<W>(writer: &mut W, method: &str)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let n = json!({
        "jsonrpc": "2.0",
        "method": method,
    });
    let line = serde_json::to_string(&n).unwrap();
    writer.write_all(line.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_drives_every_tool_against_real_chrome() {
    if !chrome_available() {
        eprintln!("Chrome not available — skipping stdio integration test");
        return;
    }
    let _chrome_lock = chrome_test_guard().await;

    let fixtures = FixtureServer::start().await;
    let form_url = fixtures.url("form.html");

    let session = McpSession::new(SessionConfig::default())
        .await
        .expect("session");
    let server = McpServer::new(session);

    // Wire stdin/stdout as in-memory duplex pipes. The "server side"
    // reads from `srv_in` and writes to `srv_out`. The "client side"
    // (this test) writes to `cli_in` and reads from `cli_out`.
    let (cli_in, srv_in) = duplex(64 * 1024);
    let (srv_out, cli_out) = duplex(8 * 1024 * 1024); // big to fit a screenshot
    let mut cli_in = cli_in;
    let mut cli_reader = BufReader::new(cli_out);

    // Spawn the transport loop driving the server with these pipes.
    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let _ = genatp_mcp::stdio::serve_async(server, srv_in, srv_out).await;
        }
    });

    // ---- initialize -------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "stdio-test", "version": "0"}
        }),
    )
    .await;
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "genatp");
    notify(&mut cli_in, "notifications/initialized").await;

    // ---- tools/list -------------------------------------------------
    let resp = rpc(&mut cli_in, &mut cli_reader, 2, "tools/list", json!({})).await;
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 12);

    // ---- navigate ---------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        3,
        "tools/call",
        json!({ "name": "navigate", "arguments": { "url": &form_url } }),
    )
    .await;
    assert_eq!(
        resp["result"]["isError"], false,
        "navigate response: {resp:?}"
    );

    // Some fixtures fire async DOM work; wait a beat so first snapshot
    // sees the steady tree.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ---- snapshot ---------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        4,
        "tools/call",
        json!({ "name": "snapshot", "arguments": {} }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);
    let snap_text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("text payload");
    let snap: Value = serde_json::from_str(snap_text).expect("snapshot json");
    assert!(snap.get("nodes").is_some(), "snapshot lacks `nodes`");

    // Pull the first textbox ref so we can drive a `fill`. The form
    // fixture has an `input[type=text][name=name]` near the top.
    let nodes = snap["nodes"].as_array().unwrap();
    let textbox = nodes
        .iter()
        .find(|n| n["role"].as_str() == Some("textbox"))
        .expect("at least one textbox ref in form fixture");
    let textbox_ref = textbox["ref"].as_str().unwrap().to_string();

    // ---- fill -------------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        5,
        "tools/call",
        json!({ "name": "fill", "arguments": { "ref": &textbox_ref, "value": "Ada Lovelace" } }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false, "fill: {resp:?}");

    // ---- extract (no query) ----------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        6,
        "tools/call",
        json!({ "name": "extract", "arguments": {} }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);

    // ---- scroll -----------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        7,
        "tools/call",
        json!({ "name": "scroll", "arguments": { "dx": 0, "dy": 100 } }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);

    // ---- get_url ----------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        8,
        "tools/call",
        json!({ "name": "get_url", "arguments": {} }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);
    let url = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(url.contains("form.html"), "unexpected url: {url}");

    // ---- screenshot -------------------------------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        9,
        "tools/call",
        json!({ "name": "screenshot", "arguments": {} }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);
    let mime = resp["result"]["content"][0]["mimeType"].as_str().unwrap();
    assert_eq!(mime, "image/png");
    let data = resp["result"]["content"][0]["data"].as_str().unwrap();
    assert!(data.len() > 100, "screenshot data improbably small");

    // ---- click ------------------------------------------------------
    // Find the submit button.
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        10,
        "tools/call",
        json!({ "name": "snapshot", "arguments": {} }),
    )
    .await;
    let snap: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let button_ref = snap["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["role"].as_str() == Some("button")
                && n["name"].as_str().is_some_and(|s| {
                    let s = s.to_lowercase();
                    s.contains("submit") || s.contains("send")
                })
        })
        .map(|n| n["ref"].as_str().unwrap().to_string());

    if let Some(r) = button_ref {
        let resp = rpc(
            &mut cli_in,
            &mut cli_reader,
            11,
            "tools/call",
            json!({ "name": "click", "arguments": { "ref": &r } }),
        )
        .await;
        assert_eq!(resp["result"]["isError"], false, "click: {resp:?}");
    }

    // ---- go_back ----------------------------------------------------
    // We've only navigated once, so there might not be history — both
    // outcomes are fine.
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        12,
        "tools/call",
        json!({ "name": "go_back", "arguments": {} }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], false);

    // ---- resources/list + resources/read ---------------------------
    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        13,
        "resources/list",
        json!({}),
    )
    .await;
    let resources = resp["result"]["resources"].as_array().expect("resources");
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0]["uri"].as_str().unwrap(),
        "genatp://snapshot/current"
    );

    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        14,
        "resources/read",
        json!({ "uri": "genatp://snapshot/current" }),
    )
    .await;
    let body = resp["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(body.contains("\"nodes\""));

    // Close stdin → server loop exits naturally.
    drop(cli_in);
    let _ = tokio::time::timeout(Duration::from_secs(5), server_task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_unknown_tool_returns_json_rpc_error() {
    if !chrome_available() {
        eprintln!("Chrome not available — skipping");
        return;
    }
    let _chrome_lock = chrome_test_guard().await;
    let session = McpSession::new(SessionConfig::default()).await.unwrap();
    let server = McpServer::new(session);

    let (cli_in, srv_in) = duplex(8 * 1024);
    let (srv_out, cli_out) = duplex(8 * 1024);
    let mut cli_in = cli_in;
    let mut cli_reader = BufReader::new(cli_out);

    let task = tokio::spawn(async move {
        let _ = genatp_mcp::stdio::serve_async(server, srv_in, srv_out).await;
    });

    let resp = rpc(
        &mut cli_in,
        &mut cli_reader,
        1,
        "tools/call",
        json!({ "name": "wat", "arguments": {} }),
    )
    .await;
    assert!(
        resp.get("error").is_some(),
        "expected JSON-RPC error: {resp}"
    );
    assert_eq!(resp["error"]["code"], -32602);

    drop(cli_in);
    let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
}
