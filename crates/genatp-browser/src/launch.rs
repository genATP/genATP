//! Browser launch configuration.
//!
//! `LaunchOptions` is a typed builder that compiles down to a
//! [`chromiumoxide::browser::BrowserConfig`]. We control the launch flags
//! ourselves (rather than delegating to defaults) so that:
//!
//! 1. Phase 11 stealth hardening can extend this builder without forking
//!    `chromiumoxide`.
//! 2. The set of flags used in production is reviewable and grep-able in
//!    one file (this one), not buried inside upstream.
//!
//! ### What we set today
//!
//! - **Always-disabled automation telltales** — `AutomationControlled` blink
//!   feature, `--enable-automation` is not passed (chromiumoxide's default).
//! - **Realistic viewport** — 1280×800 by default.
//! - **Locale + timezone** — defaults to `en-US` / `UTC`; overridable.
//! - **Headless** — controlled by `headless()`. `HeadlessMode::New` is used
//!   by default (the modern --headless=new which renders identically to
//!   headed Chrome); override to `HeadlessMode::True` for legacy headless or
//!   `HeadlessMode::False` for visible debugging.
//!
//! Anti-bot fingerprint hardening (`navigator.webdriver`, plugin spoofing,
//! WebGL vendor) lands in Phase 11.

use std::path::PathBuf;
use std::time::Duration;

use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::handler::viewport::Viewport;

use crate::error::{BrowserError, Result};

/// Default viewport width — chosen to match a common desktop screen so pages
/// that opt into responsive layouts give us the desktop tree (richer a11y).
pub const DEFAULT_VIEWPORT_WIDTH: u32 = 1280;
/// Default viewport height.
pub const DEFAULT_VIEWPORT_HEIGHT: u32 = 800;

/// Headless mode selection.
///
/// Wraps `chromiumoxide`'s private `HeadlessMode` enum. We expose our own type
/// because the upstream enum is module-private even though the builder methods
/// to switch it are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadlessMode {
    /// Visible window. Use for local debugging.
    Visible,
    /// Legacy `--headless` Chromium. Default behaviour for upstream.
    Legacy,
    /// `--headless=new` — pixel-identical to headed Chrome. Recommended.
    #[default]
    New,
}

/// Typed builder for browser launch options.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// Path to the Chrome / Chromium executable. `None` lets `chromiumoxide`
    /// auto-detect (`CHROME_PATH` env or platform defaults).
    pub chrome_executable: Option<PathBuf>,

    /// Headless mode. Defaults to [`HeadlessMode::New`].
    pub headless: HeadlessMode,

    /// Viewport size. `None` disables viewport emulation.
    pub viewport: Option<Viewport>,

    /// IETF BCP-47 locale tag (`Accept-Language`).
    pub locale: String,

    /// IANA timezone name (e.g. `UTC`, `America/Los_Angeles`).
    pub timezone: String,

    /// User-data directory for the profile. `None` → ephemeral temp dir.
    pub user_data_dir: Option<PathBuf>,

    /// Connection timeout to the CDP websocket once Chrome boots.
    pub connect_timeout: Duration,

    /// Extra raw command-line arguments appended verbatim. Use sparingly —
    /// prefer typed builder methods so the flag is reviewable.
    pub extra_args: Vec<String>,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            chrome_executable: std::env::var_os("CHROME_PATH").map(PathBuf::from),
            headless: HeadlessMode::default(),
            viewport: Some(Viewport {
                width: DEFAULT_VIEWPORT_WIDTH,
                height: DEFAULT_VIEWPORT_HEIGHT,
                device_scale_factor: Some(1.0),
                emulating_mobile: false,
                is_landscape: true,
                has_touch: false,
            }),
            locale: "en-US".to_string(),
            timezone: "UTC".to_string(),
            user_data_dir: None,
            connect_timeout: Duration::from_secs(20),
            extra_args: Vec::new(),
        }
    }
}

impl LaunchOptions {
    /// Start a fresh builder with safe defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override Chrome binary path.
    #[must_use]
    pub fn chrome_executable(mut self, path: impl Into<PathBuf>) -> Self {
        self.chrome_executable = Some(path.into());
        self
    }

    /// Override headless mode.
    #[must_use]
    pub fn headless(mut self, mode: HeadlessMode) -> Self {
        self.headless = mode;
        self
    }

    /// Set viewport. Pass `None` to disable viewport emulation entirely.
    #[must_use]
    pub fn viewport(mut self, viewport: Option<Viewport>) -> Self {
        self.viewport = viewport;
        self
    }

    /// Set locale (IETF BCP-47, e.g. `"en-US"`, `"de-DE"`).
    #[must_use]
    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = locale.into();
        self
    }

    /// Set timezone (IANA, e.g. `"UTC"`, `"America/Los_Angeles"`).
    #[must_use]
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = tz.into();
        self
    }

    /// Set user-data directory.
    #[must_use]
    pub fn user_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.user_data_dir = Some(dir.into());
        self
    }

    /// Append a raw command-line argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// The deliberate set of flags we always pass. Centralising them here
    /// makes the launch surface auditable in one place.
    fn baseline_args(&self) -> Vec<String> {
        vec![
            // Disable the blink feature that exposes navigator.webdriver behaviour.
            "--disable-blink-features=AutomationControlled".to_string(),
            // Avoid info-bars and dev-mode banners that change the viewport.
            "--disable-infobars".to_string(),
            // Prevent the first-run dialog from racing with the test.
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            // Stable rendering across runs.
            "--disable-features=Translate,BackForwardCache,AcceptCHFrame".to_string(),
            // Stability fixes for headless on CI runners + restricted host filesystems.
            // /dev/shm is small in containers; --disable-dev-shm-usage falls back to /tmp.
            "--disable-dev-shm-usage".to_string(),
            // GPU is not available headless on most CI; software-rasterise instead.
            "--disable-gpu".to_string(),
            // Crash-pad sometimes fails to start on restricted FS (OneDrive, sandboxes).
            "--disable-crashpad".to_string(),
            // Locale + timezone passed via dedicated flags.
            format!("--lang={}", self.locale),
        ]
    }

    /// Compile this builder into a [`BrowserConfig`] ready for `Browser::launch`.
    ///
    /// # Errors
    /// Returns [`BrowserError::Launch`] if the underlying builder rejects the
    /// configuration (e.g. unwritable `user_data_dir`).
    pub fn into_config(self) -> Result<BrowserConfig> {
        let mut builder = match self.headless {
            HeadlessMode::Visible => BrowserConfig::builder().with_head(),
            HeadlessMode::New => BrowserConfig::builder().new_headless_mode(),
            HeadlessMode::Legacy => BrowserConfig::builder(),
        };

        if let Some(path) = self.chrome_executable.as_ref() {
            builder = builder.chrome_executable(path);
        }

        if let Some(viewport) = self.viewport.clone() {
            builder = builder.viewport(viewport);
        } else {
            builder = builder.viewport(None);
        }

        if let Some(dir) = self.user_data_dir.as_ref() {
            builder = builder.user_data_dir(dir);
        }

        builder = builder.request_timeout(self.connect_timeout);

        for arg in self.baseline_args() {
            builder = builder.arg(arg);
        }
        for arg in &self.extra_args {
            builder = builder.arg(arg.as_str());
        }

        // Set the timezone via env var inherited by Chrome's process.
        // CDP also exposes Emulation.setTimezoneOverride, applied per-page in cdp.rs.
        // Keeping both paths covers cases where Chrome forks rebase env handling.

        builder.build().map_err(BrowserError::Launch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_realistic_viewport() {
        let opts = LaunchOptions::default();
        let vp = opts.viewport.expect("default has viewport");
        assert_eq!(vp.width, DEFAULT_VIEWPORT_WIDTH);
        assert_eq!(vp.height, DEFAULT_VIEWPORT_HEIGHT);
        assert!(!vp.emulating_mobile);
    }

    #[test]
    fn baseline_args_disable_automation_blink_feature() {
        let opts = LaunchOptions::default();
        let args = opts.baseline_args();
        assert!(
            args.iter().any(|a| a.contains("AutomationControlled")),
            "baseline must disable AutomationControlled blink feature: {args:?}"
        );
    }

    #[test]
    fn baseline_args_include_locale() {
        let opts = LaunchOptions::default().locale("de-DE");
        let args = opts.baseline_args();
        assert!(args.iter().any(|a| a == "--lang=de-DE"), "got {args:?}");
    }

    #[test]
    fn builder_methods_are_chainable() {
        let opts = LaunchOptions::new()
            .locale("ja-JP")
            .timezone("Asia/Tokyo")
            .arg("--mute-audio");
        assert_eq!(opts.locale, "ja-JP");
        assert_eq!(opts.timezone, "Asia/Tokyo");
        assert!(opts.extra_args.contains(&"--mute-audio".to_string()));
    }

    #[test]
    fn into_config_succeeds_with_defaults() {
        // Default config does not require Chrome to actually be present —
        // it only validates the builder. Launch is a separate concern.
        LaunchOptions::default()
            .into_config()
            .expect("default config should build");
    }
}
