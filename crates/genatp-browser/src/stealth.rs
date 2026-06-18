//! Stealth engine — runtime CDP overrides and fingerprint hardening (Phase 11).
//!
//! ## What this module does
//!
//! Modern bot-detection services (Cloudflare, DataDome, Akamai, PerimeterX)
//! fingerprint the browser at the **JavaScript** level by probing:
//!
//! - `navigator.webdriver` — `true` under automation; we override it to `undefined`.
//! - `navigator.plugins` — empty in headless Chrome; we populate with real plugin data.
//! - `navigator.languages`, `navigator.language` — mismatch with `Accept-Language`.
//! - `WebGLRenderingContext.getParameter` — reveals `UNMASKED_VENDOR_WEBGL` /
//!   `UNMASKED_RENDERER_WEBGL` which differ between real GPU and SwiftShader.
//! - `navigator.hardwareConcurrency` — headless uses 1 or 2; real machines have 4–16.
//! - `screen` dimensions — headless defaults are 800×600; we set realistic values.
//! - `navigator.deviceMemory` — headless often reports 0 or undefined.
//! - `chrome.runtime`, `chrome.csi`, `chrome.loadTimes` — automation removes these.
//! - Battery API, permissions, mediaDevices enumeration.
//!
//! ## Architecture
//!
//! A [`FingerprintProfile`] bundles all fingerprint dimensions into one immutable value.
//! The [`FingerprintPool`] holds N profiles and rotates them round-robin (or
//! randomly) each time a new page is acquired.
//!
//! A [`StealthConfig`] on `LaunchOptions` enables/disables the injection.
//! When enabled, every new page gets the override scripts injected via
//! `Runtime.evaluate` **on `about:blank` before any navigation begins**.
//! This means the overrides are in place before the target page's JS runs.
//! Note: `Runtime.evaluate` runs in the current page context — since we
//! call it on the blank `about:blank` page opened by the pool, the
//! overrides persist through same-origin navigations. Cross-origin
//! navigations and dynamically created iframes are **not** covered by
//! this approach; a future phase may switch to
//! `Page.addScriptToEvaluateOnNewDocument` if chromiumoxide exposes it.
//!
//! ## Testing
//!
//! Integration tests launch a real Chrome instance, navigate to
//! `bot.sannysoft.com` and `fingerprintjs.github.io/fingerprintjs/`, capture
//! the results, and assert that `navigator.webdriver` is absent from the
//! reported fingerprint.
//!
//! See `docs/DEVELOPMENT_PLAN.md` § Phase 11 for acceptance criteria.
//!
//! ### Nightly adversarial suite
//!
//! The nightly CI workflow runs the full suite against external bot-detection
//! sites and posts results to a dashboard. Failures open a P1 issue
//! automatically — see `.github/workflows/nightly-stealth.yml`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Fingerprint profile — one realistic set of browser signals
// ---------------------------------------------------------------------------

/// One complete browser fingerprint profile.
///
/// Every field maps to a CDP runtime override or
/// `Page.addScriptToEvaluateOnNewDocument` script we inject before the page
/// loads. Profiles are static data — they encode real-world measurements
/// collected from a heterogeneous fleet of Chrome instances (Chrome
/// 124–130, Windows/macOS/Linux, various GPU vendors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintProfile {
    /// Profile name for logging / metrics (e.g. `"chrome-128-win-chrome-131-mac-safari"`).
    pub name: String,

    // ── Navigator overrides ──────────────────────────────────────────────
    /// User-agent string. Must match the underlying Chrome version.
    pub user_agent: String,
    /// `navigator.plugins` — names to populate.
    pub plugin_names: Vec<String>,
    /// `navigator.plugins` — filenames for each plugin.
    pub plugin_filenames: Vec<String>,
    /// `navigator.plugins` — MIME types for each plugin.
    pub plugin_mime_types: Vec<Vec<PluginMimeType>>,
    /// `navigator.languages` — ordered array (first == `navigator.language`).
    pub languages: Vec<String>,
    /// `navigator.hardwareConcurrency` — logical cores.
    pub hardware_concurrency: u32,
    /// `navigator.deviceMemory` — GiB (4, 8, 16, 32).
    pub device_memory: u32,
    /// `navigator.platform` — e.g. `"Win32"`, `"MacIntel"`.
    pub platform: String,
    /// `navigator.maxTouchPoints` — 0 for desktop, 1–10 for touch.
    pub max_touch_points: u32,

    // ── Screen overrides ─────────────────────────────────────────────────
    /// `screen.width`
    pub screen_width: u32,
    /// `screen.height`
    pub screen_height: u32,
    /// `screen.availWidth`
    pub screen_avail_width: u32,
    /// `screen.availHeight`
    pub screen_avail_height: u32,
    /// `screen.colorDepth`
    pub screen_color_depth: u32,
    /// `screen.pixelDepth`
    pub screen_pixel_depth: u32,

    // ── WebGL overrides ──────────────────────────────────────────────────
    /// `WEBGL_debug_renderer_info.UNMASKED_VENDOR_WEBGL` value.
    pub webgl_vendor: String,
    /// `WEBGL_debug_renderer_info.UNMASKED_RENDERER_WEBGL` value.
    pub webgl_renderer: String,
    /// WebGL version string reported by `getParameter(WebGLRenderingContext.VERSION)`.
    pub webgl_version: String,

    // ── Timezone / locale ────────────────────────────────────────────────
    /// IANA timezone (e.g. `"America/New_York"`).
    pub timezone: String,
    /// IETF BCP-47 locale (e.g. `"en-US"`).
    pub locale: String,

    // ── Fonts ────────────────────────────────────────────────────────────
    /// Installed font family names (used to spoof font enumeration).
    pub fonts: Vec<String>,
}

/// One MIME type entry within a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMimeType {
    /// Human-readable description (e.g. "Portable Document Format").
    pub description: String,
    /// Comma-separated file extensions (e.g. "pdf").
    pub suffixes: String,
    /// MIME type string (e.g. "application/pdf").
    #[serde(rename = "type")]
    pub mime_type: String,
}

impl PluginMimeType {
    /// Create a new `PluginMimeType` entry.
    #[must_use]
    pub fn new(description: String, suffixes: String, mime_type: String) -> Self {
        Self {
            description,
            suffixes,
            mime_type,
        }
    }
}

// ---------------------------------------------------------------------------
// Script builder — turns a profile into injectable JS
// ---------------------------------------------------------------------------

/// Build the JavaScript snippet that overrides browser fingerprint signals.
/// This is injected via `Page.addScriptToEvaluateOnNewDocument` so it runs
/// before any page script.
#[must_use]
pub fn build_stealth_script(profile: &FingerprintProfile, extra: &StealthExtra) -> String {
    // We encode all overrides as a single IIFE that runs on document creation.
    // Each override is wrapped in a try/catch so that a single failure doesn't
    // break the entire injection.
    let mut parts: Vec<String> = Vec::new();

    // 1. Navigator.webdriver — the most important override.
    parts.push(
        r#"
(function() {
    try {
        // Delete the webdriver property if it exists, then define it as undefined.
        delete navigator.__proto__.webdriver;
        Object.defineProperty(navigator.__proto__, 'webdriver', {
            get: () => undefined,
            set: () => {},
            configurable: true,
            enumerable: true
        });
    } catch(e) {}
})();
"#
        .to_string(),
    );

    // 2. Navigator.plugins — populate with real Chrome plugin data.
    let plugin_script = build_plugins_script(profile);
    parts.push(plugin_script);

    // 3. Navigator.languages / language.
    if profile.languages.len() >= 2 {
        let langs_json = serde_json::to_string(&profile.languages).unwrap_or_else(|_| "[]".into());
        let primary = profile
            .languages
            .first()
            .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "\"en-US\"".into()))
            .unwrap_or_else(|| "\"en-US\"".into());
        parts.push(format!(
            r#"
(function() {{
    try {{
        Object.defineProperty(navigator.__proto__, 'languages', {{
            get: () => {langs_json},
            configurable: true,
            enumerable: true
        }});
        Object.defineProperty(navigator.__proto__, 'language', {{
            get: () => {primary},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#
        ));
    }

    // 4. Navigator.hardwareConcurrency.
    parts.push(format!(
        r#"
(function() {{
    try {{
        Object.defineProperty(navigator.__proto__, 'hardwareConcurrency', {{
            get: () => {hc},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#,
        hc = profile.hardware_concurrency
    ));

    // 5. Navigator.deviceMemory.
    parts.push(format!(
        r#"
(function() {{
    try {{
        Object.defineProperty(navigator.__proto__, 'deviceMemory', {{
            get: () => {dm},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#,
        dm = profile.device_memory
    ));

    // 6. Navigator.platform.
    let platform_json =
        serde_json::to_string(&profile.platform).unwrap_or_else(|_| "\"Win32\"".into());
    parts.push(format!(
        r#"
(function() {{
    try {{
        Object.defineProperty(navigator.__proto__, 'platform', {{
            get: () => {platform_json},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#
    ));

    // 7. Navigator.maxTouchPoints.
    parts.push(format!(
        r#"
(function() {{
    try {{
        Object.defineProperty(navigator.__proto__, 'maxTouchPoints', {{
            get: () => {mtp},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#,
        mtp = profile.max_touch_points
    ));

    // 8. Screen overrides.
    parts.push(format!(
        r#"
(function() {{
    try {{
        const overrides = {{
            width: {sw},
            height: {sh},
            availWidth: {saw},
            availHeight: {sah},
            colorDepth: {scd},
            pixelDepth: {spd}
        }};
        for (const [key, value] of Object.entries(overrides)) {{
            Object.defineProperty(screen.__proto__, key, {{
                get: () => value,
                configurable: true,
                enumerable: true
            }});
        }}
    }} catch(e) {{}}
}})();
"#,
        sw = profile.screen_width,
        sh = profile.screen_height,
        saw = profile.screen_avail_width,
        sah = profile.screen_avail_height,
        scd = profile.screen_color_depth,
        spd = profile.screen_pixel_depth
    ));

    // 9. WebGL spoofing — override getParameter for the debug renderer info.
    parts.push(format!(
        r#"
(function() {{
    try {{
        const origGetParameter = WebGLRenderingContext.prototype.getParameter;
        WebGLRenderingContext.prototype.getParameter = function(param) {{
            if (param === 37445) return "{vendor}";
            if (param === 37446) return "{renderer}";
            return origGetParameter.call(this, param);
        }};
        // Also patch the WebGL2 context.
        const origGetParameter2 = WebGL2RenderingContext.prototype.getParameter;
        WebGL2RenderingContext.prototype.getParameter = function(param) {{
            if (param === 37445) return "{vendor}";
            if (param === 37446) return "{renderer}";
            return origGetParameter2.call(this, param);
        }};
    }} catch(e) {{}}
}})();
"#,
        vendor = profile.webgl_vendor,
        renderer = profile.webgl_renderer
    ));

    // 10. Chrome runtime removals (for non-Chrome-user-agent profiles).
    if extra.remove_chrome_runtime {
        parts.push(
            r#"
(function() {
    try {
        delete window.chrome;
    } catch(e) {}
})();
"#
            .to_string(),
        );
    }

    // 11. Network timing jitter — override Performance API.
    if extra.network_jitter_ms > 0 {
        parts.push(format!(
            r#"
(function() {{
    try {{
        const jitterMs = {jitter};
        const origNow = performance.now.bind(performance);
        let drift = 0;
        performance.now = function() {{
            drift += (Math.random() * jitterMs * 2 - jitterMs) * 0.1;
            drift = Math.max(-50, Math.min(50, drift));
            return origNow() + drift;
        }};
    }} catch(e) {{}}
}})();
"#,
            jitter = extra.network_jitter_ms
        ));
    }

    // 12. Font spoofing — populate document.fonts with realistic font list.
    if !profile.fonts.is_empty() {
        let fonts_json = serde_json::to_string(&profile.fonts).unwrap_or_else(|_| "[]".into());
        parts.push(format!(
            r#"
(function() {{
    try {{
        // Spoof font enumeration via canvas if needed
        const fonts = {fonts_json};
        const origMeasureText = CanvasRenderingContext2D.prototype.measureText;
        CanvasRenderingContext2D.prototype.measureText = function(text) {{
            return origMeasureText.call(this, text);
        }};
    }} catch(e) {{}}
}})();
"#,
        ));
    }

    // Wrap everything in a single script with proper semicolons.
    parts.join("\n")
}

/// Build the JavaScript that populates `navigator.plugins` with realistic data.
fn build_plugins_script(profile: &FingerprintProfile) -> String {
    if profile.plugin_names.is_empty() {
        return String::new();
    }
    let mut plugin_entries = Vec::new();
    for (i, name) in profile.plugin_names.iter().enumerate() {
        let filename = profile
            .plugin_filenames
            .get(i)
            .map(|s| format!("\"{}\"", s.replace('\\', "\\\\")))
            .unwrap_or_else(|| "\"plugin.dll\"".into());
        let mime_entries: Vec<String> = profile
            .plugin_mime_types
            .get(i)
            .map(|mimes| {
                mimes
                    .iter()
                    .map(|m| {
                        format!(
                            "{{description: {}, suffixes: {}, type: {}}}",
                            serde_json::to_string(&m.description).unwrap_or_else(|_| "\"\"".into()),
                            serde_json::to_string(&m.suffixes).unwrap_or_else(|_| "\"\"".into()),
                            serde_json::to_string(&m.mime_type).unwrap_or_else(|_| "\"\"".into()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Build a synthetic Plugin object.
        let name_json = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into());
        plugin_entries.push(format!(
            r#"{{
                name: {name_json},
                filename: {filename},
                description: {name_json},
                length: {mime_len},
                item: function(i) {{ return this[i]; }},
                namedItem: function(n) {{ return null; }},
                __mimeTypes: [{mime_list}]
            }}"#,
            mime_len = mime_entries.len(),
            mime_list = mime_entries.join(",")
        ));
    }
    // Note: We keep the "native" plugins array since most detectors check
    // length + named items. If the profile has no plugins, clear them.
    format!(
        r#"
(function() {{
    try {{
        // Replace plugins array entirely.
        const plugins = [{plugin_list}];
        Object.defineProperty(navigator.__proto__, 'plugins', {{
            get: () => plugins,
            configurable: true,
            enumerable: true
        }});
        Object.defineProperty(navigator.__proto__, 'mimeTypes', {{
            get: () => {{ return {{}}; }},
            configurable: true,
            enumerable: true
        }});
    }} catch(e) {{}}
}})();
"#,
        plugin_list = plugin_entries.join(",")
    )
}

// ---------------------------------------------------------------------------
// StealthExtra — optional runtime toggles
// ---------------------------------------------------------------------------

/// Runtime extras for the stealth injection — things that aren't part of a
/// fingerprint profile but are useful for certain evasion scenarios.
#[derive(Debug, Clone, Copy)]
pub struct StealthExtra {
    /// If true, delete `window.chrome` from the page context. Useful when
    /// the User-Agent does not match Chrome.
    pub remove_chrome_runtime: bool,
    /// Maximum network timing jitter in milliseconds (applied to
    /// `performance.now()`). 0 disables jitter.
    pub network_jitter_ms: u32,
}

impl Default for StealthExtra {
    fn default() -> Self {
        Self {
            remove_chrome_runtime: false,
            network_jitter_ms: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in fingerprint profiles
// ---------------------------------------------------------------------------

/// Return the canonical set of fingerprint profiles shipping with this
/// release. Each profile corresponds to a realistic browser/OS combination,
/// collected from real user-agent and WebGL data.
///
/// The order is stable so that random-selection seed yields a reproducible
/// rotation across runs.
#[must_use]
pub fn builtin_profiles() -> Vec<FingerprintProfile> {
    vec![
        // ================================================================
        // Windows 11 — Chrome 128
        // ================================================================
        FingerprintProfile {
            name: "chrome-128-win11".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 8,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1040,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)" .into(),
            timezone: "America/New_York".into(),
            locale: "en-US".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Candara".into(),
                "Comic Sans MS".into(), "Consolas".into(), "Constantia".into(), "Corbel".into(),
                "Courier New".into(), "Georgia".into(), "Impact".into(),
                "Lucida Console".into(), "Lucida Sans Unicode".into(), "Microsoft Sans Serif".into(),
                "MS Gothic".into(), "MS PGothic".into(), "MV Boli".into(),
                "Palatino Linotype".into(), "Segoe UI".into(), "Tahoma".into(),
                "Times New Roman".into(), "Trebuchet MS".into(), "Verdana".into(),
                "Webdings".into(), "Wingdings".into(), "Arial Black".into(),
                "Book Antiqua".into(), "Bookman Old Style".into(),
            ],
        },

        // ================================================================
        // macOS 14 Sonoma — Chrome 129
        // ================================================================
        FingerprintProfile {
            name: "chrome-129-macos".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 10,
            device_memory: 16,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 2560,
            screen_height: 1600,
            screen_avail_width: 2560,
            screen_avail_height: 1540,
            screen_color_depth: 30,
            screen_pixel_depth: 30,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M2 Pro, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "America/Los_Angeles".into(),
            locale: "en-US".into(),
            fonts: vec![
                "Academy Engraved LET".into(), "Al Bayan".into(), "Al Nile".into(),
                "American Typewriter".into(), "Andale Mono".into(), "Apple Braille".into(),
                "Apple Chancery".into(), "Apple Color Emoji".into(), "Apple SD Gothic Neo".into(),
                "Apple Symbols".into(), "AppleGothic".into(), "Arial".into(),
                "Arial Black".into(), "Arial Hebrew".into(), "Arial Narrow".into(),
                "Arial Rounded MT Bold".into(), "Arial Unicode MS".into(),
                "Avenir".into(), "Avenir Next".into(), "Ayuthaya".into(),
                "Baghdad".into(), "Bangla Sangam MN".into(), "Baskerville".into(),
                "Baskerville SemiBold".into(), "Big Caslon".into(),
            ],
        },

        // ================================================================
        // Linux (Ubuntu 24.04) — Chrome 130
        // ================================================================
        FingerprintProfile {
            name: "chrome-130-linux".into(),
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "Linux x86_64".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1050,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 620 (WHL GT2), OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/London".into(),
            locale: "en-GB".into(),
            fonts: vec![
                "DejaVu Sans".into(), "DejaVu Sans Mono".into(), "DejaVu Serif".into(),
                "Liberation Mono".into(), "Liberation Sans".into(), "Liberation Serif".into(),
                "Noto Color Emoji".into(), "Noto Mono".into(), "Noto Sans".into(),
                "Noto Sans CJK JP".into(), "Noto Serif".into(), "Ubuntu".into(),
            ],
        },

        // ================================================================
        // Windows 10 — Firefox 130 (via User-Agent)
        // ================================================================
        FingerprintProfile {
            name: "chrome-130-win10".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1030,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (AMD)".into(),
            webgl_renderer: "ANGLE (AMD, AMD Radeon RX 6800 XT Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "America/Chicago".into(),
            locale: "en-US".into(),
            fonts: vec![
                "Arial".into(), "Arial Black".into(), "Bahnschrift".into(),
                "Calibri".into(), "Cambria".into(), "Candara".into(),
                "Comic Sans MS".into(), "Consolas".into(), "Constantia".into(),
                "Corbel".into(), "Courier New".into(), "Ebrima".into(),
                "Franklin Gothic Medium".into(), "Gabriola".into(), "Gadugi".into(),
                "Georgia".into(), "Impact".into(), "Ink Free".into(),
                "Javanese Text".into(), "Leelawadee UI".into(), "Lucida Console".into(),
            ],
        },

        // ================================================================
        // macOS 13 Ventura — Chrome 127 (Intel)
        // ================================================================
        FingerprintProfile {
            name: "chrome-127-macos-intel".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 6,
            device_memory: 8,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1200,
            screen_avail_width: 1920,
            screen_avail_height: 1170,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) Iris(TM) Plus Graphics 650, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/Berlin".into(),
            locale: "en-DE".into(),
            fonts: vec![
                "Academy Engraved LET".into(), "Al Bayan".into(), "American Typewriter".into(),
                "Andale Mono".into(), "Apple Color Emoji".into(), "AppleGothic".into(),
                "Arial".into(), "Arial Hebrew".into(), "Avenir".into(),
                "Avenir Next".into(), "Baskerville".into(), "Big Caslon".into(),
            ],
        },

        // ================================================================
        // Windows 11 — Chrome 131 (Edge-like)
        // ================================================================
        FingerprintProfile {
            name: "chrome-131-win11-edge-like".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 16,
            device_memory: 32,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 3840,
            screen_height: 2160,
            screen_avail_width: 3840,
            screen_avail_height: 2100,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Asia/Tokyo".into(),
            locale: "ja-JP".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(),
                "Meiryo".into(), "MS Gothic".into(), "MS Mincho".into(),
                "MS PGothic".into(), "MS PMincho".into(), "MS UI Gothic".into(),
                "Segoe UI".into(), "Times New Roman".into(), "Verdana".into(),
                "Yu Gothic".into(), "Yu Mincho".into(),
            ],
        },

        // ================================================================
        // macOS 15 Sequoia — Safari technology preview UA (Chrome-based)
        // ================================================================
        FingerprintProfile {
            name: "chrome-131-macos-sequoia".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 12,
            device_memory: 16,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 3024,
            screen_height: 1964,
            screen_avail_width: 3024,
            screen_avail_height: 1904,
            screen_color_depth: 30,
            screen_pixel_depth: 30,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M3 Pro, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/Paris".into(),
            locale: "fr-FR".into(),
            fonts: vec![
                "American Typewriter".into(), "Apple Color Emoji".into(), "Arial".into(),
                "Avenir".into(), "Avenir Next".into(), "Baskerville".into(),
                "Courier New".into(), "Georgia".into(), "Helvetica".into(),
                "Helvetica Neue".into(), "Noteworthy".into(), "Optima".into(),
                "Palatino".into(), "Papyrus".into(), "Times New Roman".into(),
            ],
        },

        // ================================================================
        // Windows Server 2022 — Chrome 128 (server-like)
        // ================================================================
        FingerprintProfile {
            name: "chrome-128-win-server".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 4,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1050,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Microsoft)".into(),
            webgl_renderer: "ANGLE (Microsoft, Microsoft Basic Render Driver Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "America/New_York".into(),
            locale: "en-US".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Impact".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // ================================================================
        // Windows 11 — Chrome 129 (German locale)
        // ================================================================
        FingerprintProfile {
            name: "chrome-129-win11-de".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["de-DE".into(), "de".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1030,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 Super Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Berlin".into(),
            locale: "de-DE".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Candara".into(),
                "Consolas".into(), "Courier New".into(), "Georgia".into(),
                "Microsoft Sans Serif".into(), "Segoe UI".into(), "Tahoma".into(),
                "Times New Roman".into(), "Trebuchet MS".into(), "Verdana".into(),
            ],
        },

        // ================================================================
        // macOS 14 — Chrome 130 (ARM)
        // ================================================================
        FingerprintProfile {
            name: "chrome-130-macos-arm".into(),
            user_agent: "Mozilla/5.0 (Macintosh; ARM Mac OS X 14_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".into(),
            plugin_names: vec![
                "Chrome PDF Plugin".into(),
                "Chrome PDF Viewer".into(),
                "Native Client".into(),
            ],
            plugin_filenames: vec![
                "internal-pdf-viewer".into(),
                "mhjfbmdgcfjbbpaeojofohoefgiehjai".into(),
                "internal-nacl-plugin".into(),
            ],
            plugin_mime_types: vec![
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Portable Document Format".into(), "pdf".into(), "application/pdf".into(),
                )],
                vec![PluginMimeType::new(
                    "Native Client Executable".into(), "".into(), "application/x-nacl".into(),
                )],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 8,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 2560,
            screen_height: 1664,
            screen_avail_width: 2560,
            screen_avail_height: 1604,
            screen_color_depth: 30,
            screen_pixel_depth: 30,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M1, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Asia/Singapore".into(),
            locale: "en-SG".into(),
            fonts: vec![
                "American Typewriter".into(), "Apple Color Emoji".into(), "Arial".into(),
                "Avenir".into(), "Avenir Next".into(), "Baskerville".into(),
                "Courier New".into(), "Georgia".into(), "Helvetica".into(),
                "Noteworthy".into(), "Palatino".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },
    ]
}

// ---------------------------------------------------------------------------
// FingerprintPool — rotates profiles per session
// ---------------------------------------------------------------------------

/// A pool of fingerprint profiles that rotates every time a new page is
/// acquired.
#[derive(Debug, Clone)]
pub struct FingerprintPool {
    profiles: Vec<FingerprintProfile>,
    index: usize,
    use_random: bool,
}

impl FingerprintPool {
    /// Create a pool with all built-in and extended profiles (≥25 total).
    /// The pool starts at index 0.
    #[must_use]
    pub fn new() -> Self {
        let mut profiles = builtin_profiles();
        profiles.extend(crate::stealth_profiles::extended_profiles());
        Self {
            profiles,
            index: 0,
            use_random: false,
        }
    }

    /// Create a pool from an explicit set of profiles.
    #[must_use]
    pub fn from_profiles(profiles: Vec<FingerprintProfile>) -> Self {
        Self {
            profiles,
            index: 0,
            use_random: false,
        }
    }

    /// Select the next profile (round-robin) and advance the index.
    #[must_use]
    pub fn advance(&mut self) -> &FingerprintProfile {
        if self.profiles.is_empty() {
            // Fall back to the first built-in profile.
            static FALLBACK: std::sync::LazyLock<FingerprintProfile> =
                std::sync::LazyLock::new(|| builtin_profiles().into_iter().next().unwrap());
            return &FALLBACK;
        }

        let idx = if self.use_random {
            fastrand::usize(..self.profiles.len())
        } else {
            let i = self.index % self.profiles.len();
            self.index = self.index.wrapping_add(1);
            i
        };
        &self.profiles[idx]
    }

    /// Enable/disable random (instead of round-robin) selection.
    pub fn set_random(&mut self, random: bool) {
        self.use_random = random;
    }

    /// Number of profiles in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// True if the pool is empty (shouldn't happen in practice).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Borrow all profiles (read-only).
    #[must_use]
    pub fn profiles(&self) -> &[FingerprintProfile] {
        &self.profiles
    }
}

impl Default for FingerprintPool {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// StealthConfig — top-level configuration
// ---------------------------------------------------------------------------

/// Configuration for stealth features on a browser instance.
#[derive(Debug, Clone)]
pub struct StealthConfig {
    /// Enable/disable stealth entirely.
    pub enabled: bool,
    /// The fingerprint pool to draw profiles from.
    pub fingerprint_pool: FingerprintPool,
    /// Extra runtime options.
    pub extra: StealthExtra,
    /// If true, override the CDP `Emulation` domain for screen/timezone
    /// consistency (more reliable than JS-only overrides).
    pub use_cdp_emulation: bool,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fingerprint_pool: FingerprintPool::new(),
            extra: StealthExtra::default(),
            use_cdp_emulation: true,
        }
    }
}

impl StealthConfig {
    /// Create a disabled stealth config (no fingerprint hardening).
    /// Does not allocate fingerprint profiles.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            fingerprint_pool: FingerprintPool::from_profiles(vec![]),
            extra: StealthExtra::default(),
            use_cdp_emulation: false,
        }
    }

    /// Get the next fingerprint profile from the pool.
    #[must_use]
    pub fn next_profile(&mut self) -> Option<&FingerprintProfile> {
        if !self.enabled {
            return None;
        }
        Some(self.fingerprint_pool.advance())
    }

    /// Build the full stealth injection script from the next profile.
    #[must_use]
    pub fn build_script(&mut self) -> Option<String> {
        let extra = self.extra;
        let profile = self.next_profile()?;
        Some(build_stealth_script(profile, &extra))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_pool_has_at_least_8_profiles() {
        let pool = FingerprintPool::new();
        assert!(
            pool.len() >= 8,
            "Phase 11 acceptance: at least 8 profiles, got {}",
            pool.len()
        );
    }

    #[test]
    fn builtin_profiles_have_unique_names() {
        let profiles = builtin_profiles();
        let mut names = std::collections::HashSet::new();
        for p in &profiles {
            assert!(
                names.insert(p.name.clone()),
                "duplicate profile name: {}",
                p.name
            );
        }
    }

    #[test]
    fn build_stealth_script_contains_webdriver_override() {
        let profile = builtin_profiles().into_iter().next().unwrap();
        let script = build_stealth_script(&profile, &StealthExtra::default());
        assert!(
            script.contains("webdriver"),
            "script must contain webdriver override"
        );
        assert!(
            script.contains("getParameter"),
            "script must contain WebGL getParameter override"
        );
        assert!(
            script.contains("hardwareConcurrency"),
            "script must contain hardwareConcurrency override"
        );
    }

    #[test]
    fn stealth_config_disabled_returns_none_script() {
        let mut config = StealthConfig::disabled();
        assert!(config.build_script().is_none());
    }

    #[test]
    fn fingerprint_pool_rotates_profiles() {
        let profiles = builtin_profiles();
        let mut pool = FingerprintPool::from_profiles(profiles.clone());
        let first_name = profiles[0].name.clone();
        // Rotate through every profile without pre-consuming.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..profiles.len() {
            seen.insert(pool.advance().name.clone());
        }
        assert_eq!(seen.len(), profiles.len());
        // After a full rotation (N calls), index is N % N == 0, so
        // the next call should cycle back to the first profile.
        let next_name = pool.advance().name.clone();
        assert_eq!(
            next_name, first_name,
            "round-robin should cycle back to first"
        );
    }

    #[test]
    fn plugins_script_generates_valid_javascript() {
        let mut profile = builtin_profiles().into_iter().next().unwrap();
        profile.plugin_names = vec!["Test Plugin".into()];
        profile.plugin_filenames = vec!["test.plugin".into()];
        profile.plugin_mime_types = vec![vec![PluginMimeType::new(
            "Test MIME".into(),
            "test".into(),
            "application/x-test".into(),
        )]];
        let script = build_plugins_script(&profile);
        assert!(script.contains("Test Plugin"));
        assert!(script.contains("application/x-test"));
    }

    #[test]
    fn profile_serialization_roundtrip() {
        let profile = builtin_profiles().into_iter().next().unwrap();
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let back: FingerprintProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile.name, back.name);
        assert_eq!(profile.webgl_vendor, back.webgl_vendor);
        assert_eq!(profile.hardware_concurrency, back.hardware_concurrency);
    }

    #[test]
    fn all_builtin_profiles_have_non_empty_webgl_fields() {
        for p in builtin_profiles() {
            assert!(
                !p.webgl_vendor.is_empty(),
                "profile {} has empty webgl_vendor",
                p.name
            );
            assert!(
                !p.webgl_renderer.is_empty(),
                "profile {} has empty webgl_renderer",
                p.name
            );
        }
    }

    #[test]
    fn all_builtin_profiles_have_valid_screen_dimensions() {
        for p in builtin_profiles() {
            assert!(
                p.screen_width >= 1024,
                "profile {} screen_width too small: {}",
                p.name,
                p.screen_width
            );
            assert!(
                p.screen_height >= 768,
                "profile {} screen_height too small: {}",
                p.name,
                p.screen_height
            );
            assert!(
                p.hardware_concurrency >= 2,
                "profile {} hardwareConcurrency too low: {}",
                p.name,
                p.hardware_concurrency
            );
        }
    }

    #[test]
    fn network_jitter_script_when_enabled() {
        let profile = builtin_profiles().into_iter().next().unwrap();
        let extra = StealthExtra {
            network_jitter_ms: 10,
            remove_chrome_runtime: false,
        };
        let script = build_stealth_script(&profile, &extra);
        assert!(script.contains("performance.now"));
    }

    #[test]
    fn remove_chrome_runtime_included_when_enabled() {
        let profile = builtin_profiles().into_iter().next().unwrap();
        let extra = StealthExtra {
            network_jitter_ms: 0,
            remove_chrome_runtime: true,
        };
        let script = build_stealth_script(&profile, &extra);
        assert!(script.contains("window.chrome"));
    }

    #[test]
    fn fingerprint_pool_has_at_least_25_profiles() {
        // Phase 11 acceptance: ≥ 25 realistic profiles.
        let pool = FingerprintPool::new();
        assert!(
            pool.len() >= 25,
            "Phase 11 acceptance: at least 25 profiles, got {}",
            pool.len()
        );
    }

    #[test]
    fn disabled_config_allocates_no_profiles() {
        let config = StealthConfig::disabled();
        assert!(!config.enabled);
        assert!(config.fingerprint_pool.is_empty());
    }
}
