//! Concrete [`BrowserAdapter`](crate::dispatcher::BrowserAdapter) backed by
//! a `genatp_browser::Page`.
//!
//! This is a thin delegation layer — the actual CDP plumbing lives in
//! `genatp-browser` so the chromiumoxide dependency does not leak into the
//! orchestrator crate.

use std::sync::Arc;

use async_trait::async_trait;
use genatp_browser::{AccessibilityTree, Page};

use crate::dispatcher::BrowserAdapter;
use crate::error::CoreError;

/// `BrowserAdapter` backed by a `genatp_browser::Page`.
#[derive(Debug, Clone)]
pub struct PageAdapter {
    page: Arc<Page>,
}

impl PageAdapter {
    /// Wrap a page.
    #[must_use]
    pub fn new(page: Page) -> Self {
        Self {
            page: Arc::new(page),
        }
    }

    /// Borrow the underlying page.
    #[must_use]
    pub fn page(&self) -> &Page {
        &self.page
    }
}

#[async_trait]
impl BrowserAdapter for PageAdapter {
    async fn navigate(&self, url: &str) -> Result<(), CoreError> {
        self.page.navigate(url).await?;
        Ok(())
    }

    async fn click_node(&self, node_id: &str) -> Result<(), CoreError> {
        self.page.click_node(node_id).await?;
        Ok(())
    }

    async fn fill_node(&self, node_id: &str, value: &str) -> Result<(), CoreError> {
        self.page.fill_node(node_id, value).await?;
        Ok(())
    }

    async fn press_key(&self, key: &str) -> Result<(), CoreError> {
        self.page.press_key(key).await?;
        Ok(())
    }

    async fn click_coordinate(&self, x: f64, y: f64) -> Result<(), CoreError> {
        self.page.click_coordinate(x, y).await?;
        Ok(())
    }

    async fn scroll(&self, dx: i32, dy: i32) -> Result<(), CoreError> {
        self.page.scroll_by(dx, dy).await?;
        Ok(())
    }

    async fn current_url(&self) -> Result<Option<String>, CoreError> {
        Ok(self.page.url().await?)
    }

    async fn accessibility(&self) -> Result<AccessibilityTree, CoreError> {
        Ok(self.page.accessibility_tree().await?)
    }

    async fn screenshot_png(&self) -> Result<Vec<u8>, CoreError> {
        Ok(self.page.screenshot_png().await?)
    }
}
