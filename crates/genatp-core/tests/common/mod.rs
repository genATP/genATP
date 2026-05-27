//! Shared helpers for `genatp-core` integration tests.
//!
//! Duplicates a small amount of code from `crates/genatp-browser/tests/common`
//! deliberately — a `genatp-test-support` crate is a Phase 4 cleanup. The
//! duplication is < 100 lines and keeps Phase 3 self-contained.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use genatp_browser::LaunchOptions;

static CHROME_TEST_LOCK: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

pub async fn chrome_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    CHROME_TEST_LOCK.lock().await
}

pub struct FixtureServer {
    pub base_url: String,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct AppState {
    files: Arc<HashMap<String, Vec<u8>>>,
}

impl FixtureServer {
    pub async fn start() -> Self {
        let files = load_fixture_files();
        let state = AppState {
            files: Arc::new(files),
        };
        let app = Router::new()
            .route("/{*path}", get(serve_file))
            .route("/", get(serve_index))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let addr: SocketAddr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self { base_url, handle }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    match state.files.get("index.html") {
        Some(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes.clone(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no index").into_response(),
    }
}

async fn serve_file(Path(path): Path<String>, State(state): State<AppState>) -> impl IntoResponse {
    match state.files.get(&path) {
        Some(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime_for(&path))],
            bytes.clone(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, format!("not found: {path}")).into_response(),
    }
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn workspace_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("fixtures").join("sites"))
        .expect("workspace root resolvable")
}

fn load_fixture_files() -> HashMap<String, Vec<u8>> {
    let dir = workspace_fixtures_dir();
    let mut out = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let (Some(name), Ok(bytes)) = (
                    path.file_name().and_then(|n| n.to_str()),
                    std::fs::read(&path),
                )
            {
                out.insert(name.to_string(), bytes);
            }
        }
    }
    out
}

pub fn test_launch_options() -> LaunchOptions {
    let tmp = std::env::temp_dir().join(format!(
        "genatp-core-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&tmp);
    LaunchOptions::for_integration_tests(tmp)
}

pub fn chrome_available() -> bool {
    if std::env::var_os("CHROME_PATH").is_some() {
        return true;
    }

    #[cfg(target_os = "windows")]
    let candidates: &[&str] = &[
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];
    #[cfg(target_os = "macos")]
    let candidates: &[&str] = &[
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
    #[cfg(target_os = "linux")]
    let candidates: &[&str] = &[
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let candidates: &[&str] = &[];

    candidates.iter().any(|p| std::path::Path::new(p).exists())
}
