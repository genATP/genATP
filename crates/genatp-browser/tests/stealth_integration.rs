//! Phase 11 stealth integration tests — adversarial bot-detection probes.
//!
//! These tests are `#[ignore]` by default and excluded from `cargo test
//! --workspace`. Run them manually or via the nightly CI workflow:
//!
//! ```bash
//! # Unit tests only (no external sites)
//! cargo test -p genatp-browser -- stealth
//!
//! # Live probes against real bot-detection sites (requires Chrome)
//! GENATP_STEALTH_LIVE=1 cargo test -p genatp-browser -- --ignored --test-threads 1
//! ```
//!
//! ## Acceptance criteria (Phase 11)
//!
//! - bot.sannysoft.com: 100% pass (no detectable automation flags)
//! - fingerprintjs/botd: ≥ 90% score
//! - creepjs: realistic-score band (documented per profile)

mod common;

use std::sync::LazyLock;
use std::time::Duration;

use genatp_browser::stealth::StealthConfig;
use genatp_browser::{BrowserPool, LaunchOptions, PoolConfig};

/// True if `GENATP_STEALTH_LIVE` is set, meaning we should probe real
/// external bot-detection sites.
fn live_mode() -> bool {
    std::env::var_os("GENATP_STEALTH_LIVE").is_some()
}

/// Shared pool for integration tests — created once per test binary
/// invocation to amortize Chrome launch cost across multiple probes.
static SHARED_POOL: LazyLock<Option<genatp_browser::BrowserPool>> = LazyLock::new(|| {
    if !live_mode() {
        return None;
    }
    let launch = LaunchOptions::default();
    let pool_cfg = PoolConfig::default().launch(launch).max_concurrency(1);
    Some(BrowserPool::new(pool_cfg))
});

/// Acquire a page from the shared pool (or skip if Chrome is unavailable).
async fn acquire_page() -> Option<genatp_browser::pool::PageGuard> {
    let pool = SHARED_POOL.as_ref()?;
    match tokio::time::timeout(Duration::from_secs(15), pool.acquire()).await {
        Ok(Ok(guard)) => Some(guard),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Unit tests — verify stealth script structure without a browser
// ---------------------------------------------------------------------------

#[test]
fn stealth_config_injects_webdriver_override() {
    let mut config = StealthConfig::default();
    let script = config.build_script().expect("stealth script must build");
    assert!(
        script.contains("webdriver"),
        "must override navigator.webdriver"
    );
    assert!(script.contains("WebGLRenderingContext"), "must patch WebGL");
    assert!(
        script.contains("hardwareConcurrency"),
        "must override hardwareConcurrency"
    );
}

#[test]
fn fingerprint_pool_has_at_least_25_profiles() {
    use genatp_browser::stealth::FingerprintPool;
    let pool = FingerprintPool::new();
    assert!(
        pool.len() >= 25,
        "Phase 11 acceptance: at least 25 profiles, got {}",
        pool.len()
    );
}

#[test]
fn fingerprint_profiles_have_unique_names() {
    use genatp_browser::stealth::builtin_profiles;
    use genatp_browser::stealth_profiles::extended_profiles;
    let mut names = std::collections::HashSet::new();
    for p in builtin_profiles().iter().chain(extended_profiles().iter()) {
        assert!(names.insert(&p.name), "duplicate profile name: {}", p.name);
    }
}

#[test]
fn disabled_stealth_produces_no_script() {
    let mut config = StealthConfig::disabled();
    assert!(config.build_script().is_none());
}

// ---------------------------------------------------------------------------
// Live browser probes — require Chrome and external network
// ---------------------------------------------------------------------------

/// Navigate to a URL and capture a JSON string of automation flags.
/// Uses the shared pool. Returns `Err` if navigation or evaluation fails.
async fn probe_url(url: &str) -> Result<String, String> {
    let guard = acquire_page()
        .await
        .ok_or("no page available (Chrome not installed)")?;
    let page = guard.page();

    // Navigate with a generous timeout for slow bot-detection sites.
    tokio::time::timeout(Duration::from_secs(30), page.navigate(url))
        .await
        .map_err(|_| "navigation timeout".to_string())?
        .map_err(|e| format!("navigation error: {e}"))?;

    // Wait for the page to settle.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Use the page's evaluate_with_str method, which returns an
    // EvaluateResponse that can be converted to a serde_json::Value.
    use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
    let params = EvaluateParams::builder()
        .expression(
            r#"
            JSON.stringify({
                webdriver: navigator.webdriver,
                pluginsLength: navigator.plugins.length,
                languages: navigator.languages,
                hardwareConcurrency: navigator.hardwareConcurrency,
                deviceMemory: navigator.deviceMemory,
                platform: navigator.platform,
                userAgent: navigator.userAgent
            })
        "#,
        )
        .return_by_value(true)
        .build()
        .map_err(|e| format!("EvaluateParams builder: {e}"))?;

    let result = page
        .raw()
        .execute(params)
        .await
        .map_err(|e| format!("evaluate error: {e}"))?;

    let json_str: String = result
        .result
        .value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    guard.close().await.ok();
    Ok(json_str)
}

#[tokio::test]
#[ignore = "requires live network and Chrome — run with GENATP_STEALTH_LIVE=1"]
async fn stealth_probe_bot_sannysoft() {
    if !live_mode() {
        return;
    }
    let result = probe_url("https://bot.sannysoft.com/").await;
    match result {
        Ok(json) => {
            // Verify navigator.webdriver is not present or is undefined.
            // bot.sannysoft.com displays automation flags in a table;
            // we check the JS output directly.
            assert!(
                json.contains("\"webdriver\":null") || json.contains("\"webdriver\":undefined"),
                "navigator.webdriver should be null or undefined. Got: {json}"
            );
            assert!(
                json.contains("\"pluginsLength\":3")
                    || json.contains("\"pluginsLength\":") && !json.contains("\"pluginsLength\":0"),
                "plugins should be populated. Got: {json}"
            );
        }
        Err(e) => {
            // Non-fatal: site may be down. Log and skip.
            eprintln!("bot.sannysoft.com probe skipped: {e}");
        }
    }
}

#[tokio::test]
#[ignore = "requires live network and Chrome — run with GENATP_STEALTH_LIVE=1"]
async fn stealth_probe_fingerprintjs() {
    if !live_mode() {
        return;
    }
    let result = probe_url("https://fingerprintjs.github.io/fingerprintjs/").await;
    match result {
        Ok(json) => {
            assert!(
                !json.contains("\"webdriver\":true"),
                "navigator.webdriver should not be true. Got: {json}"
            );
        }
        Err(e) => {
            eprintln!("fingerprintjs probe skipped: {e}");
        }
    }
}

#[tokio::test]
#[ignore = "requires live network and Chrome — run with GENATP_STEALTH_LIVE=1"]
async fn stealth_probe_creepjs() {
    if !live_mode() {
        return;
    }
    let result = probe_url("https://abrahamjuliot.github.io/creepjs/").await;
    match result {
        Ok(json) => {
            // Ensure basic stealth properties are intact.
            assert!(
                json.contains("\"webdriver\":null") || json.contains("\"webdriver\":undefined"),
                "navigator.webdriver should be null. Got: {json}"
            );
            assert!(
                json.contains("\"hardwareConcurrency\":") && json.contains("\"deviceMemory\":"),
                "hardware metrics should be present. Got: {json}"
            );
        }
        Err(e) => {
            eprintln!("creepjs probe skipped: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Profile consistency tests
// ---------------------------------------------------------------------------

#[test]
fn all_profiles_have_realistic_viewport() {
    use genatp_browser::stealth::{FingerprintPool, builtin_profiles};
    use genatp_browser::stealth_profiles::extended_profiles;

    for p in builtin_profiles().iter().chain(extended_profiles().iter()) {
        assert!(
            p.screen_width >= 1024,
            "profile {} has too-small screen_width: {}",
            p.name,
            p.screen_width
        );
        assert!(
            p.screen_height >= 768,
            "profile {} has too-small screen_height: {}",
            p.name,
            p.screen_height
        );
        assert!(
            p.hardware_concurrency >= 2 && p.hardware_concurrency <= 64,
            "profile {} has unrealistic hardwareConcurrency: {}",
            p.name,
            p.hardware_concurrency
        );
    }
}

#[test]
fn all_profiles_have_unique_locale_timezone_combinations() {
    use genatp_browser::stealth::{FingerprintPool, builtin_profiles};
    use genatp_browser::stealth_profiles::extended_profiles;

    let mut combos = std::collections::HashSet::new();
    for p in builtin_profiles().iter().chain(extended_profiles().iter()) {
        let key = format!("{}|{}", p.locale, p.timezone);
        assert!(
            combos.insert(key.clone()),
            "duplicate locale+timezone combo: {key} in profile {}",
            p.name
        );
    }
}
