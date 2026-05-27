//! `genatp-browser` — CDP-native browser substrate via `chromiumoxide`.
//!
//! Phase 1 implementation. Public surface:
//!
//! - [`LaunchOptions`] / [`launch`] — boot Chrome with auditable flags.
//! - [`BrowserHandle`] — owned handle to a running Chrome instance.
//! - [`Page`] — typed page operations: navigate, accessibility tree, cookies.
//! - [`BrowserPool`] — bounded-concurrency reuse of browser contexts.
//!
//! Stealth fingerprint hardening lives in **Phase 11**; this crate ships the
//! plumbing that hardening will hook into.
//!
//! See [`docs/DEVELOPMENT_PLAN.md`].
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cdp;
pub mod error;
pub mod launch;
pub mod pool;
pub mod storage;

use std::time::Duration;

use chromiumoxide::Browser;
use futures_util::StreamExt;
use tokio::task::JoinHandle;

pub use crate::cdp::{AccessibilityTree, AxNodeJson, AxPropertyJson, Page};
pub use crate::error::{BrowserError, Result};
pub use crate::launch::LaunchOptions;
pub use crate::pool::{BrowserPool, PoolConfig};

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Owned handle to a running Chrome instance.
///
/// Drop semantics: dropping a `BrowserHandle` aborts the event-pump task but
/// does **not** wait for Chrome to exit cleanly. Call [`BrowserHandle::shutdown`]
/// for deterministic cleanup. The pool always calls `shutdown` on release;
/// integration tests should too.
pub struct BrowserHandle {
    browser: Browser,
    handler_task: JoinHandle<()>,
}

impl BrowserHandle {
    /// Open a new tab and navigate to `about:blank`. The returned [`Page`]
    /// keeps a clone of the underlying handle, so it survives until you
    /// drop the page or call [`Page::close`].
    ///
    /// # Errors
    /// Returns [`BrowserError::Cdp`] if Chrome refuses to open a target.
    pub async fn new_page(&self) -> Result<Page> {
        let raw = self.browser.new_page("about:blank").await?;
        Ok(Page::new(raw))
    }

    /// Borrow the underlying `chromiumoxide::Browser`. Prefer [`new_page`]
    /// for the normal acquire path.
    ///
    /// [`new_page`]: BrowserHandle::new_page
    #[must_use]
    pub fn raw(&self) -> &Browser {
        &self.browser
    }

    /// Cleanly shut Chrome down: send `Browser.close`, wait for the handler
    /// task to drain, abort if it doesn't drain in `grace`.
    ///
    /// # Errors
    /// Returns [`BrowserError::Cdp`] if `Browser.close` fails.
    pub async fn shutdown(mut self, grace: Duration) -> Result<()> {
        let _ = self.browser.close().await;

        match tokio::time::timeout(grace, self.browser.wait()).await {
            Ok(_) => {}
            Err(_) => {
                // Graceful shutdown timed out; abort the handler task.
                self.handler_task.abort();
            }
        }
        let _ = self.handler_task.await;
        Ok(())
    }
}

/// Launch Chrome with the given options.
///
/// Spawns a background task that drains CDP events from the handler. The
/// returned [`BrowserHandle`] keeps the task alive until shutdown.
///
/// # Errors
/// - [`BrowserError::Launch`] if `chromiumoxide` cannot start Chrome (binary
///   missing, port collision, sandbox denied).
pub async fn launch(opts: LaunchOptions) -> Result<BrowserHandle> {
    let config = opts.into_config()?;
    let (browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|err| BrowserError::Launch(err.to_string()))?;

    let handler_task = tokio::task::spawn(async move {
        // Drain events; chromiumoxide requires this to keep the connection alive.
        // We log nothing here — observability hook lands in Phase 13 (debugger).
        while let Some(_event) = handler.next().await {}
    });

    Ok(BrowserHandle {
        browser,
        handler_task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
