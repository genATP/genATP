//! Cold-start benchmark for the browser substrate.
//!
//! **Phase 1 acceptance gate**: cold start (binary launch → first navigation
//! complete → a11y tree fetched → shutdown) must finish in **< 800 ms p95**
//! on the developer reference laptop.
//!
//! The Criterion baseline lives at `target/criterion/`; the
//! `bench-regression.yml` workflow compares PR runs against `main`'s baseline
//! and fails on > 5% regression of any tracked metric.
//!
//! Run locally:
//! ```text
//! cargo bench -p genatp-browser --bench browser_cold_start
//! ```
//!
//! Skip cleanly when Chrome is unavailable so the bench compiles in any
//! environment (CI without Chrome, vendored toolchain, …).

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_browser::{LaunchOptions, launch};
use tokio::runtime::Runtime;

fn chrome_available() -> bool {
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
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let candidates: &[&str] = &[];
    candidates.iter().any(|p| std::path::Path::new(p).exists())
}

fn cold_start(c: &mut Criterion) {
    if !chrome_available() {
        eprintln!("[skip] cold_start bench: Chrome not found (set CHROME_PATH)");
        return;
    }

    let rt = Runtime::new().expect("tokio runtime");

    let mut group = c.benchmark_group("browser_cold_start");
    // 5 samples per iteration is plenty given each sample spawns Chrome.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    group.bench_function("launch_navigate_a11y_close", |b| {
        b.to_async(&rt).iter(|| async {
            let handle = launch(LaunchOptions::default())
                .await
                .expect("launch chrome");
            let page = handle.new_page().await.expect("new page");
            page.navigate("about:blank").await.expect("navigate");
            let _ = page.accessibility_tree().await.expect("a11y");
            page.close().await.expect("close");
            handle
                .shutdown(Duration::from_secs(5))
                .await
                .expect("shutdown");
        });
    });

    group.finish();
}

criterion_group!(benches, cold_start);
criterion_main!(benches);
