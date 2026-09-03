//! Scoped self-heal: when a Click/Fill ref drifts, ask the LLM once for a
//! corrected `@eN` and continue replay. Successful heals bump the cassette
//! minor version (see [`crate::router`]).

use std::sync::Arc;

use genatp_core::action::Action;
use genatp_core::dispatcher::BrowserAdapter;
use genatp_core::perception_bridge::raw_tree_from_accessibility;
use genatp_llm::{CompletionRequest, LlmProvider, Message};
use genatp_perception::{CompressOptions, compress};
use tracing::info;

use crate::error::{CassetteError, Result};

/// LLM wiring for self-heal.
pub struct HealConfig {
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
}

/// Attempt to repair a failed Click/Fill by choosing a new `@eN` from the
/// live snapshot. Returns `Ok(None)` when heal is not applicable.
pub async fn maybe_heal_step(
    adapter: &dyn BrowserAdapter,
    failed: &Action,
    cfg: &HealConfig,
) -> Result<Option<Action>> {
    let (old_ref, kind, fill_value) = match failed {
        Action::Click { r#ref } => (r#ref.clone(), "click", None),
        Action::Fill { r#ref, value } => (r#ref.clone(), "fill", Some(value.clone())),
        _ => return Ok(None),
    };

    let tree = adapter
        .accessibility()
        .await
        .map_err(|e| CassetteError::Heal(format!("accessibility: {e}")))?;
    let url = adapter
        .current_url()
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let raw = raw_tree_from_accessibility(url, &tree);
    let mut refs = genatp_perception::RefMap::new();
    let snap = compress(&raw, &mut refs, &CompressOptions::default());
    let snap_json = serde_json::to_string(&snap)
        .map_err(|e| CassetteError::Heal(format!("snapshot json: {e}")))?;

    let prompt = format!(
        "You are repairing a browser automation cassette step.\n\
         The recorded action `{kind}` on ref `{old_ref}` failed because the element \
         is missing or changed.\n\
         Current page snapshot (JSON):\n{snap_json}\n\n\
         Reply with ONLY a JSON object, no markdown:\n\
         - for click: {{\"name\":\"click\",\"ref\":\"@eN\"}}\n\
         - for fill:  {{\"name\":\"fill\",\"ref\":\"@eN\",\"value\":\"...\"}}\n\
         Choose the best matching interactive element. Prefer the same purpose as `{old_ref}`."
    );

    let request = CompletionRequest::chat(&cfg.model, vec![Message::user(prompt)]);
    let response = cfg
        .provider
        .complete(request)
        .await
        .map_err(|e| CassetteError::Heal(format!("llm: {e}")))?;

    let text = response.message.content.clone();
    let text = text.trim();
    let json_str = extract_json_object(text).unwrap_or(text);
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| CassetteError::Heal(format!("heal parse: {e}; body={text}")))?;

    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or(kind);
    let new_ref = value
        .get("ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CassetteError::Heal("heal response missing ref".into()))?
        .to_owned();

    let action = match name {
        "fill" => {
            let value = value
                .get("value")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or(fill_value)
                .unwrap_or_default();
            Action::Fill {
                r#ref: new_ref,
                value,
            }
        }
        _ => Action::Click { r#ref: new_ref },
    };

    info!(?action, "self-heal proposed patched action");
    Ok(Some(action))
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end >= start {
        Some(&text[start..=end])
    } else {
        None
    }
}
