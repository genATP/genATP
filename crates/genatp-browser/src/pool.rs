//! Bounded-concurrency pool of `BrowserHandle` instances.
//!
//! Phase 1 ships a single shared Chrome with bounded *page* concurrency
//! (one Chrome process, N pages) — this is the most resource-efficient
//! pattern when each cassette/agent step is short-lived. Phase 6+ adds an
//! optional one-Chrome-per-context mode for vault isolation.
//!
//! Concurrency is enforced by a `tokio::sync::Semaphore`. `acquire` returns
//! a permit + page; dropping the [`PageGuard`] returns the permit and
//! schedules the page for close.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Semaphore};

use crate::cdp::Page;
use crate::error::{BrowserError, Result};
use crate::launch::LaunchOptions;
use crate::{BrowserHandle, launch};

type SharedSemaphore = Arc<Semaphore>;

/// Defaults chosen for a developer-laptop ergonomic profile.
const DEFAULT_MAX_CONCURRENCY: usize = 4;
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Configuration for a [`BrowserPool`].
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of concurrent pages.
    pub max_concurrency: usize,
    /// How long [`BrowserPool::acquire`] waits before returning
    /// [`BrowserError::PoolExhausted`].
    pub acquire_timeout: Duration,
    /// How long [`BrowserPool::shutdown`] waits for Chrome to exit cleanly.
    pub shutdown_grace: Duration,
    /// Launch options used the first time a Chrome instance is needed.
    pub launch: LaunchOptions,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
            shutdown_grace: DEFAULT_SHUTDOWN_GRACE,
            launch: LaunchOptions::default(),
        }
    }
}

impl PoolConfig {
    /// Override max concurrency.
    #[must_use]
    pub fn max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.max(1);
        self
    }

    /// Override the launch options used for the underlying Chrome instance.
    #[must_use]
    pub fn launch(mut self, launch: LaunchOptions) -> Self {
        self.launch = launch;
        self
    }
}

/// A pool of browser pages backed by a single Chrome instance.
///
/// Cheap to clone — internally `Arc`-shared.
#[derive(Clone)]
pub struct BrowserPool {
    inner: Arc<Inner>,
}

struct Inner {
    config: PoolConfig,
    permits: SharedSemaphore,
    /// Chrome is launched lazily on first `acquire` so the pool can be
    /// constructed cheaply (e.g. in tests that may never call acquire).
    handle: Mutex<Option<BrowserHandle>>,
}

impl BrowserPool {
    /// Construct a pool. Chrome is **not** launched until first acquire.
    #[must_use]
    pub fn new(config: PoolConfig) -> Self {
        let permits = Arc::new(Semaphore::new(config.max_concurrency));
        Self {
            inner: Arc::new(Inner {
                config,
                permits,
                handle: Mutex::new(None),
            }),
        }
    }

    /// Number of permits currently available (i.e. unused capacity).
    #[must_use]
    pub fn available(&self) -> usize {
        self.inner.permits.available_permits()
    }

    /// Pool capacity (`max_concurrency`).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.config.max_concurrency
    }

    /// Acquire a [`PageGuard`]. Blocks until a permit is free or until the
    /// configured `acquire_timeout` elapses.
    ///
    /// On the first call, this lazily launches Chrome.
    ///
    /// # Errors
    /// - [`BrowserError::PoolExhausted`] when the timeout elapses.
    /// - [`BrowserError::Launch`] if Chrome cannot start.
    /// - [`BrowserError::Cdp`] if opening a tab fails.
    pub async fn acquire(&self) -> Result<PageGuard> {
        let permit = match tokio::time::timeout(
            self.inner.config.acquire_timeout,
            Arc::clone(&self.inner.permits).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                // Semaphore closed — pool is being shut down.
                return Err(BrowserError::PoolExhausted {
                    active: self.capacity(),
                    max: self.capacity(),
                });
            }
            Err(_) => {
                return Err(BrowserError::PoolExhausted {
                    active: self
                        .capacity()
                        .saturating_sub(self.inner.permits.available_permits()),
                    max: self.capacity(),
                });
            }
        };

        let page = self.ensure_handle_and_open_page().await?;

        Ok(PageGuard {
            page: Some(page),
            _permit: permit,
        })
    }

    async fn ensure_handle_and_open_page(&self) -> Result<Page> {
        let mut guard = self.inner.handle.lock().await;
        if guard.is_none() {
            *guard = Some(launch(self.inner.config.launch.clone()).await?);
        }
        let handle = guard.as_ref().expect("handle was just set if it was None");
        handle.new_page().await
    }

    /// Force-kill the current Chrome instance and re-launch a fresh one.
    /// Any existing [`PageGuard`] acquired from the old Chrome is now
    /// stale — callers MUST discard it and call [`acquire`] again.
    ///
    /// This is used when the CDP WebSocket drops (e.g. Chrome 148+
    /// closing the WS after navigation) to recover without restarting
    /// the entire server process.
    ///
    /// # Errors
    /// Returns [`BrowserError::Launch`] if the new Chrome instance
    /// cannot start.
    pub async fn reconnect(&self) -> Result<()> {
        let mut guard = self.inner.handle.lock().await;
        // Shut down the old Chrome (best-effort — ignore errors).
        if let Some(handle) = guard.take() {
            let _ = handle.shutdown(self.inner.config.shutdown_grace).await;
        }
        // Launch a fresh instance.
        *guard = Some(launch(self.inner.config.launch.clone()).await?);
        tracing::info!(target = "genatp::browser", "reconnected Chrome instance");
        Ok(())
    }

    /// Shut the pool down, draining outstanding permits and closing Chrome.
    /// Idempotent — calling twice is a no-op the second time.
    pub async fn shutdown(self) -> Result<()> {
        // Stop new acquires; outstanding `PageGuard`s still drop normally.
        self.inner.permits.close();

        let handle = {
            let mut guard = self.inner.handle.lock().await;
            guard.take()
        };

        if let Some(h) = handle {
            h.shutdown(self.inner.config.shutdown_grace).await?;
        }
        Ok(())
    }
}

/// RAII page guard. Dropping returns the permit to the pool. Callers should
/// prefer [`PageGuard::close`] for deterministic page close.
pub struct PageGuard {
    page: Option<Page>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl std::fmt::Debug for PageGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageGuard")
            .field("active", &self.page.is_some())
            .finish()
    }
}

impl PageGuard {
    /// Borrow the underlying [`Page`].
    #[must_use]
    pub fn page(&self) -> &Page {
        self.page.as_ref().expect("page is held until close()")
    }

    /// Explicitly close the page and release the permit.
    ///
    /// # Errors
    /// Surfaces any CDP failure from `Page.close`.
    pub async fn close(mut self) -> Result<()> {
        if let Some(page) = self.page.take() {
            page.close().await?;
        }
        Ok(())
    }
}

impl Drop for PageGuard {
    fn drop(&mut self) {
        // Best-effort close on drop. Spawning is unavoidable here since Drop
        // is sync; we rely on the runtime being alive for graceful close.
        if let Some(page) = self.page.take() {
            tokio::spawn(async move {
                let _ = page.close().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_config_max_concurrency_clamps_to_one() {
        let cfg = PoolConfig::default().max_concurrency(0);
        assert_eq!(cfg.max_concurrency, 1);
    }

    #[test]
    fn pool_starts_with_full_capacity_available() {
        let pool = BrowserPool::new(PoolConfig::default().max_concurrency(4));
        assert_eq!(pool.capacity(), 4);
        assert_eq!(pool.available(), 4);
    }
}
