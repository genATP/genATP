//! Phase 1 demo: launch Chrome, navigate, fetch a11y tree, close.
//!
//! Acceptance: `genatp_browser_demo --url <URL>` completes in < 1 s on
//! developer laptop (per `docs/DEVELOPMENT_PLAN.md` § Phase 1).
//!
//! Usage:
//!   genatp_browser_demo                       # navigates about:blank
//!   genatp_browser_demo --url https://example.com
//!   genatp_browser_demo --url file:///path/to/hello.html

#![forbid(unsafe_code)]

use std::env;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use genatp_browser::{LaunchOptions, launch};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .compact()
        .init();

    let url = parse_url_arg();
    let target = url.as_deref().unwrap_or("about:blank");
    info!(target, "starting genatp_browser_demo");

    match run(target).await {
        Ok(report) => {
            println!(
                "OK: nodes={} interactive={} elapsed={:?}",
                report.node_count, report.interactive_count, report.elapsed
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            error!(?err, "demo failed");
            ExitCode::FAILURE
        }
    }
}

struct DemoReport {
    node_count: usize,
    interactive_count: usize,
    elapsed: Duration,
}

async fn run(url: &str) -> genatp_browser::Result<DemoReport> {
    let start = Instant::now();

    let handle = launch(LaunchOptions::default()).await?;
    info!(launch_ms = ?start.elapsed(), "browser launched");

    let page = handle.new_page().await?;
    page.navigate(url).await?;
    info!(navigate_ms = ?start.elapsed(), "navigated");

    let tree = page.accessibility_tree().await?;
    info!(
        a11y_ms = ?start.elapsed(),
        nodes = tree.len(),
        interactive = tree.interactive_count(),
        "a11y tree fetched"
    );

    page.close().await?;
    handle.shutdown(Duration::from_secs(5)).await?;
    let elapsed = start.elapsed();
    info!(?elapsed, "shutdown complete");

    Ok(DemoReport {
        node_count: tree.len(),
        interactive_count: tree.interactive_count(),
        elapsed,
    })
}

fn parse_url_arg() -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" | "-u" => return args.next(),
            other if other.starts_with("--url=") => {
                return Some(other.trim_start_matches("--url=").to_string());
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: genatp_browser_demo [--url <URL>]\n\
                     Defaults to about:blank if --url is omitted."
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    None
}
