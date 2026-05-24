//! Phase 1 integration tests: launch real Chrome, navigate the fixture site,
//! read the accessibility tree, and validate the pool's concurrency contract.
//!
//! These tests skip cleanly when Chrome is not installed so
//! `cargo test --workspace` stays green on contributor machines without a
//! browser. CI installs Chrome via `browser-actions/setup-chrome` and exports
//! `CHROME_PATH`, so the suite runs on every PR.

mod common;

use std::time::Duration;

use genatp_browser::{BrowserPool, PoolConfig, launch};

use crate::common::{FixtureServer, chrome_available, chrome_test_guard, test_launch_options};

macro_rules! skip_without_chrome {
    () => {
        if !chrome_available() {
            eprintln!(
                "[skip] {}: Chrome not found (set CHROME_PATH or install Chrome stable)",
                module_path!()
            );
            return;
        }
    };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigate_hello_returns_a11y_tree_with_interactive_elements() {
    skip_without_chrome!();
    let _chrome = chrome_test_guard().await;

    let server = FixtureServer::start().await;
    let handle = launch(test_launch_options()).await.expect("launch chrome");
    let page = handle.new_page().await.expect("new page");

    page.navigate(&server.url("hello.html"))
        .await
        .expect("navigate fixture");

    let tree = page.accessibility_tree().await.expect("a11y tree");
    assert!(!tree.is_empty(), "a11y tree must contain at least one node");
    assert!(
        tree.interactive_count() >= 1,
        "hello.html ships ≥ 1 button/input — got {} interactive of {} nodes",
        tree.interactive_count(),
        tree.len()
    );

    page.close().await.expect("page close");
    handle
        .shutdown(Duration::from_secs(5))
        .await
        .expect("clean shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pool_supports_four_concurrent_pages() {
    skip_without_chrome!();
    let _chrome = chrome_test_guard().await;

    let server = FixtureServer::start().await;
    let pool = BrowserPool::new(
        PoolConfig::default()
            .max_concurrency(4)
            .launch(test_launch_options()),
    );

    let mut joins = Vec::new();
    for _ in 0..4 {
        let pool = pool.clone();
        let url = server.url("hello.html");
        joins.push(tokio::spawn(async move {
            let guard = pool.acquire().await.expect("acquire");
            guard.page().navigate(&url).await.expect("navigate");
            let tree = guard.page().accessibility_tree().await.expect("a11y");
            assert!(!tree.is_empty());
            guard.close().await.expect("close");
        }));
    }

    for j in joins {
        j.await.expect("task");
    }

    pool.shutdown().await.expect("pool shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pool_acquire_times_out_when_capacity_exhausted() {
    skip_without_chrome!();
    let _chrome = chrome_test_guard().await;

    let server = FixtureServer::start().await;
    let mut config = PoolConfig::default()
        .max_concurrency(1)
        .launch(test_launch_options());
    config.acquire_timeout = Duration::from_millis(200);
    let pool = BrowserPool::new(config);

    let holder = pool.acquire().await.expect("first acquire");
    holder
        .page()
        .navigate(&server.url("hello.html"))
        .await
        .expect("navigate");

    // Second acquire should fail with PoolExhausted because `holder` above
    // is still alive (we haven't dropped/closed it).
    let started = std::time::Instant::now();
    let err = pool.acquire().await.expect_err("second acquire times out");
    assert!(matches!(
        err,
        genatp_browser::BrowserError::PoolExhausted { .. }
    ));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "timeout should fire near the 200 ms budget, not block forever"
    );

    holder.close().await.expect("close holder");
    pool.shutdown().await.expect("shutdown");
}
