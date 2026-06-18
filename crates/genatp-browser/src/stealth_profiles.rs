//! Extended fingerprint profiles for Phase 11 stealth engine.
//!
//! This module adds more profiles to reach the ≥25 requirement from the
//! Phase 11 acceptance criteria. Each profile represents a realistic
//! browser/OS/hardware combination.

use crate::stealth::{FingerprintProfile, PluginMimeType};

/// Return additional fingerprint profiles (profiles 11–25).
///
/// Combined with the 10 built-in profiles in [`builtin_profiles()`],
/// this brings the total to ≥25.
#[must_use]
pub fn extended_profiles() -> Vec<FingerprintProfile> {
    vec![
        // Profile 11: Chrome 131 on Windows 11, Korean locale, 4K display
        FingerprintProfile {
            name: "chrome-131-win11-ko".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["ko-KR".into(), "ko".into(), "en".into()],
            hardware_concurrency: 12,
            device_memory: 32,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 3840,
            screen_height: 2160,
            screen_avail_width: 3840,
            screen_avail_height: 2100,
            screen_color_depth: 30,
            screen_pixel_depth: 30,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3080 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Asia/Seoul".into(),
            locale: "ko-KR".into(),
            fonts: vec![
                "Arial".into(), "Batang".into(), "BatangChe".into(), "Calibri".into(),
                "Consolas".into(), "Courier New".into(), "Dotum".into(), "DotumChe".into(),
                "Georgia".into(), "Gulim".into(), "GulimChe".into(), "Gungsuh".into(),
                "GungsuhChe".into(), "Malgun Gothic".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 12: Chrome 130 on macOS 13, Chinese locale, 2880x1800
        FingerprintProfile {
            name: "chrome-130-macos-zh".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["zh-CN".into(), "zh".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 2880,
            screen_height: 1800,
            screen_avail_width: 2880,
            screen_avail_height: 1740,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M1 Pro, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Asia/Shanghai".into(),
            locale: "zh-CN".into(),
            fonts: vec![
                "Arial".into(), "Courier New".into(), "Georgia".into(),
                "Heiti SC".into(), "Hiragino Sans GB".into(), "Microsoft YaHei".into(),
                "PingFang SC".into(), "SimHei".into(), "SimSun".into(),
                "Times New Roman".into(), "Verdana".into(),
            ],
        },

        // Profile 13: Chrome 129 on Windows 11, Russian locale, 2560x1440
        FingerprintProfile {
            name: "chrome-129-win11-ru".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["ru-RU".into(), "ru".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 2560,
            screen_height: 1440,
            screen_avail_width: 2560,
            screen_avail_height: 1390,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (AMD)".into(),
            webgl_renderer: "ANGLE (AMD, AMD Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Moscow".into(),
            locale: "ru-RU".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 14: Chrome 128 on Windows 11, Spanish locale, 1366x768 laptop
        FingerprintProfile {
            name: "chrome-128-win11-es".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["es-ES".into(), "es".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1366,
            screen_height: 768,
            screen_avail_width: 1366,
            screen_avail_height: 728,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 620, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Madrid".into(),
            locale: "es-ES".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 15: Chrome 131 on macOS 14, Australian locale, high-DPI laptop
        FingerprintProfile {
            name: "chrome-131-macos-au".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["en-AU".into(), "en".into()],
            hardware_concurrency: 10,
            device_memory: 16,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 2560,
            screen_height: 1600,
            screen_avail_width: 2560,
            screen_avail_height: 1540,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M2 Max, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Australia/Sydney".into(),
            locale: "en-AU".into(),
            fonts: vec![
                "American Typewriter".into(), "Apple Color Emoji".into(), "Arial".into(),
                "Avenir".into(), "Baskerville".into(), "Courier New".into(),
                "Georgia".into(), "Helvetica".into(), "Palatino".into(),
                "Times New Roman".into(), "Verdana".into(),
            ],
        },

        // Profile 16: Chrome 127 on Windows 10, French locale, 1680x1050
        FingerprintProfile {
            name: "chrome-127-win10-fr".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["fr-FR".into(), "fr".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1680,
            screen_height: 1050,
            screen_avail_width: 1680,
            screen_avail_height: 1000,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GT 730 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Paris".into(),
            locale: "fr-FR".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 17: Chrome 132 on Windows 11, Italian locale, compact laptop
        FingerprintProfile {
            name: "chrome-132-win11-it".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["it-IT".into(), "it".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1280,
            screen_height: 800,
            screen_avail_width: 1280,
            screen_avail_height: 760,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) HD Graphics 520, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Rome".into(),
            locale: "it-IT".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Segoe UI".into(),
                "Tahoma".into(), "Times New Roman".into(), "Verdana".into(),
            ],
        },

        // Profile 18: Chrome 128 on Ubuntu 22.04, developer workstation
        FingerprintProfile {
            name: "chrome-128-ubuntu-dev".into(),
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 16,
            device_memory: 32,
            platform: "Linux x86_64".into(),
            max_touch_points: 0,
            screen_width: 2560,
            screen_height: 1440,
            screen_avail_width: 2560,
            screen_avail_height: 1400,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3090, OpenGL 4.5)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "US/Pacific".into(),
            locale: "en-US".into(),
            fonts: vec![
                "DejaVu Sans".into(), "DejaVu Sans Mono".into(), "DejaVu Serif".into(),
                "Liberation Mono".into(), "Liberation Sans".into(), "Liberation Serif".into(),
                "Noto Color Emoji".into(), "Noto Sans".into(), "Noto Serif".into(),
                "Ubuntu".into(), "Ubuntu Mono".into(),
            ],
        },

        // Profile 19: Chrome 131 on macOS 13, ARM only, 3000x2000
        FingerprintProfile {
            name: "chrome-131-macos-arm-highdpi".into(),
            user_agent: "Mozilla/5.0 (Macintosh; ARM Mac OS X 13_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["en-GB".into(), "en".into()],
            hardware_concurrency: 10,
            device_memory: 16,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 3000,
            screen_height: 2000,
            screen_avail_width: 3000,
            screen_avail_height: 1940,
            screen_color_depth: 30,
            screen_pixel_depth: 30,
            webgl_vendor: "Google Inc. (Apple)".into(),
            webgl_renderer: "ANGLE (Apple, Apple M2, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/London".into(),
            locale: "en-GB".into(),
            fonts: vec![
                "American Typewriter".into(), "Apple Color Emoji".into(), "Arial".into(),
                "Avenir".into(), "Baskerville".into(), "Courier New".into(),
                "Georgia".into(), "Helvetica".into(), "Noteworthy".into(),
                "Palatino".into(), "Times New Roman".into(), "Verdana".into(),
            ],
        },

        // Profile 20: Chrome 127 on Windows 11, Brazilian Portuguese, 1920x1080
        FingerprintProfile {
            name: "chrome-127-win11-pt".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["pt-BR".into(), "pt".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1040,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 2060 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "America/Sao_Paulo".into(),
            locale: "pt-BR".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 21: Chrome 132 on macOS 15, Dutch locale
        FingerprintProfile {
            name: "chrome-132-macos-nl".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["nl-NL".into(), "nl".into(), "en".into()],
            hardware_concurrency: 8,
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
            webgl_renderer: "ANGLE (Apple, Apple M3, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/Amsterdam".into(),
            locale: "nl-NL".into(),
            fonts: vec![
                "Arial".into(), "Apple Color Emoji".into(), "Avenir".into(),
                "Baskerville".into(), "Courier New".into(), "Georgia".into(),
                "Helvetica Neue".into(), "Palatino".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 22: Chrome 127 on Fedora Linux, Swedish locale
        FingerprintProfile {
            name: "chrome-127-fedora-sv".into(),
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["sv-SE".into(), "sv".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Linux x86_64".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1050,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 770, OpenGL 4.6)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/Stockholm".into(),
            locale: "sv-SE".into(),
            fonts: vec![
                "DejaVu Sans".into(), "DejaVu Sans Mono".into(), "DejaVu Serif".into(),
                "Liberation Mono".into(), "Liberation Sans".into(), "Liberation Serif".into(),
                "Noto Color Emoji".into(), "Noto Sans".into(), "Noto Serif".into(),
            ],
        },

        // Profile 23: Chrome 130 on Windows 11, Czech locale, 1440x900
        FingerprintProfile {
            name: "chrome-130-win11-cs".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["cs-CZ".into(), "cs".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1440,
            screen_height: 900,
            screen_avail_width: 1440,
            screen_avail_height: 860,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) HD Graphics 620, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Europe/Prague".into(),
            locale: "cs-CZ".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Segoe UI".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },

        // Profile 24: Chrome 131 on Windows 11, Hindi locale
        FingerprintProfile {
            name: "chrome-131-win11-hi".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["hi-IN".into(), "hi".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 16,
            platform: "Win32".into(),
            max_touch_points: 0,
            screen_width: 1920,
            screen_height: 1080,
            screen_avail_width: 1920,
            screen_avail_height: 1040,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (NVIDIA)".into(),
            webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0)".into(),
            timezone: "Asia/Kolkata".into(),
            locale: "hi-IN".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Microsoft Sans Serif".into(),
                "Nirmala UI".into(), "Segoe UI".into(), "Tahoma".into(),
                "Times New Roman".into(), "Verdana".into(),
            ],
        },

        // Profile 25: Chrome 128 on macOS 12, Polish locale, older Intel Mac
        FingerprintProfile {
            name: "chrome-128-macos-pl".into(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 12_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36".into(),
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
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/x-google-chrome-pdf".into())],
                vec![PluginMimeType::new("Portable Document Format".into(), "pdf".into(), "application/pdf".into())],
                vec![PluginMimeType::new("Native Client Executable".into(), "".into(), "application/x-nacl".into())],
            ],
            languages: vec!["pl-PL".into(), "pl".into(), "en".into()],
            hardware_concurrency: 4,
            device_memory: 8,
            platform: "MacIntel".into(),
            max_touch_points: 0,
            screen_width: 1440,
            screen_height: 900,
            screen_avail_width: 1440,
            screen_avail_height: 870,
            screen_color_depth: 24,
            screen_pixel_depth: 24,
            webgl_vendor: "Google Inc. (Intel)".into(),
            webgl_renderer: "ANGLE (Intel, Intel(R) Iris(TM) Graphics 540, OpenGL 4.1)Google".into(),
            webgl_version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".into(),
            timezone: "Europe/Warsaw".into(),
            locale: "pl-PL".into(),
            fonts: vec![
                "Arial".into(), "Calibri".into(), "Cambria".into(), "Consolas".into(),
                "Courier New".into(), "Georgia".into(), "Helvetica Neue".into(),
                "Microsoft Sans Serif".into(), "Tahoma".into(), "Times New Roman".into(),
                "Verdana".into(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_profiles_have_15_entries() {
        let profiles = extended_profiles();
        assert_eq!(
            profiles.len(),
            15,
            "extended_profiles must have exactly 15 entries"
        );
    }

    #[test]
    fn combined_with_builtin_reaches_25_profiles() {
        let builtin = crate::stealth::builtin_profiles();
        let extended = extended_profiles();
        assert!(
            builtin.len() + extended.len() >= 25,
            "combined profiles must be ≥ 25, got {}",
            builtin.len() + extended.len()
        );
    }
}
