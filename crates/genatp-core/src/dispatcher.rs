//! Maps [`Action`]s onto concrete browser operations.
//!
//! The dispatcher is parameterised over a [`BrowserAdapter`] so we can
//! drive the orchestrator from tests with an in-memory mock, and from
//! production with a real `chromiumoxide`-backed adapter.
//!
//! `@eN` refs returned by the LLM are resolved against the latest
//! [`genatp_perception::RefMap`] before being passed to the adapter.

use std::collections::HashMap;

use async_trait::async_trait;

use genatp_browser::AccessibilityTree;
use genatp_perception::snapshot::Ref;

use crate::action::{Action, ActionResult};
use crate::error::CoreError;

/// Minimum operations the orchestrator needs from a browser.
#[async_trait]
pub trait BrowserAdapter: Send + Sync + std::fmt::Debug {
    /// Navigate to `url`. Returns when the page commits or errors.
    async fn navigate(&self, url: &str) -> Result<(), CoreError>;

    /// Click an element by its CDP node id (resolved from `@eN`).
    async fn click_node(&self, node_id: &str) -> Result<(), CoreError>;

    /// Fill a text input by node id. Existing value is cleared first.
    async fn fill_node(&self, node_id: &str, value: &str) -> Result<(), CoreError>;

    /// Press a key on the currently focused element (or page if none).
    async fn press_key(&self, key: &str) -> Result<(), CoreError>;

    /// Scroll the viewport by relative pixels.
    async fn scroll(&self, dx: i32, dy: i32) -> Result<(), CoreError>;

    /// Click at absolute viewport coordinates (x, y). Used in vision mode
    /// when @eN refs are unavailable.
    async fn click_coordinate(&self, x: f64, y: f64) -> Result<(), CoreError>;

    /// Return the current document URL (post-redirect).
    async fn current_url(&self) -> Result<Option<String>, CoreError>;

    /// Fetch a fresh accessibility snapshot.
    async fn accessibility(&self) -> Result<AccessibilityTree, CoreError>;

    /// Capture a full-viewport PNG screenshot. Returns raw PNG bytes.
    /// Used in vision mode when the policy engine determines DOM
    /// accessibility is insufficient.
    async fn screenshot_png(&self) -> Result<Vec<u8>, CoreError>;
}

/// Map an `@eN` ref to its CDP node id using the perception ref→node table
/// the orchestrator built on the last snapshot.
pub fn ref_to_node_id<'a>(
    ref_str: &str,
    ref_to_node: &'a HashMap<Ref, String>,
) -> Result<&'a str, CoreError> {
    ref_to_node
        .get(ref_str)
        .map(String::as_str)
        .ok_or_else(|| CoreError::InvalidAction {
            name: "ref-resolve".into(),
            reason: format!("unknown ref `{ref_str}` — snapshot may be stale"),
        })
}

/// Dispatches one [`Action`] against a [`BrowserAdapter`].
///
/// The orchestrator passes a fresh `ref_to_node` lookup table built from
/// the most recent compression pass. Stale refs surface as
/// [`CoreError::InvalidAction`] without touching the page.
#[derive(Debug)]
pub struct ActionDispatcher<B> {
    browser: B,
}

impl<B: BrowserAdapter> ActionDispatcher<B> {
    pub fn new(browser: B) -> Self {
        Self { browser }
    }

    /// Borrow the underlying adapter.
    pub fn browser(&self) -> &B {
        &self.browser
    }

    /// Execute `action`. Maps it onto adapter calls, returning a
    /// structured [`ActionResult`] suitable for the event log.
    ///
    /// `ref_to_node` is the snapshot-local mapping built by perception.
    /// `Finish` does not touch the browser; it returns success and lets the
    /// orchestrator end the run.
    pub async fn dispatch(
        &self,
        action: Action,
        ref_to_node: &HashMap<Ref, String>,
    ) -> Result<ActionResult, CoreError> {
        let result = match &action {
            Action::Navigate { url } => {
                self.browser.navigate(url).await?;
                ok(&action, "navigated")
            }
            Action::Click { r#ref } => {
                let nid = ref_to_node_id(r#ref, ref_to_node)?;
                self.browser.click_node(nid).await?;
                ok(&action, "clicked")
            }
            Action::ClickCoordinate { x, y } => {
                self.browser.click_coordinate(*x, *y).await?;
                ok(&action, "clicked_coordinate")
            }
            Action::Fill { r#ref, value } => {
                let nid = ref_to_node_id(r#ref, ref_to_node)?;
                self.browser.fill_node(nid, value).await?;
                ok(&action, "filled")
            }
            Action::Press { key } => {
                self.browser.press_key(key).await?;
                ok(&action, "pressed")
            }
            Action::Scroll { dx, dy } => {
                self.browser.scroll(*dx, *dy).await?;
                ok(&action, "scrolled")
            }
            Action::Wait { ms } => {
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(*ms))).await;
                ok(&action, "waited")
            }
            // Handled by the orchestrator (blocks on UserPrompt). Never reaches here
            // in production; kept for exhaustive match + unit tests.
            Action::AskUser { .. } => ok(&action, "ask_user"),
            Action::Finish { .. } => ok(&action, "finish"),
        };
        Ok(result)
    }
}

fn ok(action: &Action, msg: &'static str) -> ActionResult {
    ActionResult {
        action: action.clone(),
        success: true,
        message: Some(msg.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingBrowser {
        events: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl BrowserAdapter for RecordingBrowser {
        async fn navigate(&self, url: &str) -> Result<(), CoreError> {
            self.events.lock().unwrap().push(format!("navigate {url}"));
            Ok(())
        }
        async fn click_node(&self, node_id: &str) -> Result<(), CoreError> {
            self.events.lock().unwrap().push(format!("click {node_id}"));
            Ok(())
        }
        async fn fill_node(&self, node_id: &str, value: &str) -> Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("fill {node_id}={value}"));
            Ok(())
        }
        async fn press_key(&self, key: &str) -> Result<(), CoreError> {
            self.events.lock().unwrap().push(format!("press {key}"));
            Ok(())
        }
        async fn click_coordinate(&self, x: f64, y: f64) -> Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("click_coordinate {x},{y}"));
            Ok(())
        }
        async fn scroll(&self, dx: i32, dy: i32) -> Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("scroll {dx},{dy}"));
            Ok(())
        }
        async fn current_url(&self) -> Result<Option<String>, CoreError> {
            Ok(Some("https://example.com".into()))
        }
        async fn accessibility(&self) -> Result<AccessibilityTree, CoreError> {
            Ok(AccessibilityTree::from_nodes(vec![]))
        }
        async fn screenshot_png(&self) -> Result<Vec<u8>, CoreError> {
            Ok(vec![])
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn click_resolves_ref_to_node_id() {
        let mut refs: HashMap<Ref, String> = HashMap::new();
        refs.insert("@e1".into(), "42".into());
        let d = ActionDispatcher::new(RecordingBrowser::default());
        let result = d
            .dispatch(
                Action::Click {
                    r#ref: "@e1".into(),
                },
                &refs,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(d.browser().events.lock().unwrap()[0], "click 42");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_ref_errors_without_calling_browser() {
        let refs: HashMap<Ref, String> = HashMap::new();
        let d = ActionDispatcher::new(RecordingBrowser::default());
        let err = d
            .dispatch(
                Action::Click {
                    r#ref: "@e99".into(),
                },
                &refs,
            )
            .await
            .expect_err("must error");
        assert!(matches!(err, CoreError::InvalidAction { .. }));
        assert!(d.browser().events.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn finish_does_not_touch_browser() {
        let refs: HashMap<Ref, String> = HashMap::new();
        let d = ActionDispatcher::new(RecordingBrowser::default());
        let result = d
            .dispatch(
                Action::Finish {
                    summary: "done".into(),
                },
                &refs,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(d.browser().events.lock().unwrap().is_empty());
    }
}
