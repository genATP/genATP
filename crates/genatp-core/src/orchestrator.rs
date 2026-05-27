//! Plan-loop orchestrator.
//!
//! ## Loop
//!
//! ```text
//!   navigate(start_url)
//!   loop {
//!     snapshot = perception.compress(browser.accessibility())
//!     response = llm.complete(system + history + snapshot + tools)
//!     budget.record_step(response.usage)
//!     for tool_call in response.tool_calls {
//!       action = Action::from(tool_call)
//!       result = dispatcher.dispatch(action)
//!       if action == Finish { return }
//!     }
//!     if no tool_calls and finish_reason == Stop {
//!       // Model gave a textual final answer.
//!       return
//!     }
//!     if budget.check() == Halt { return BudgetExceeded }
//!     if step_count >= max_steps { return StepLimit }
//!   }
//! ```
//!
//! The orchestrator wraps the LLM call in [`RetryPolicy`] for transient
//! errors, and emits an [`Event`] to the [`EventLog`] for every meaningful
//! action, so the Phase 13 debugger can replay the entire run later.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use genatp_llm::{CompletionRequest, FinishReason, LlmProvider, Message};
use genatp_perception::snapshot::Ref;
use genatp_perception::{CompressOptions, CompressedSnapshot, RefMap, SnapshotStore, compress};
use tracing::{debug, info, warn};

use crate::action::{Action, ActionResult};
use crate::budget::{BudgetCheck, BudgetGuard};
use crate::dispatcher::{ActionDispatcher, BrowserAdapter};
use crate::error::{CoreError, Result};
use crate::event_log::{Event, EventLog};
use crate::perception_bridge::raw_tree_from_accessibility;
use crate::retry::RetryPolicy;
use crate::task::Task;

/// Final outcome of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// Agent emitted a `finish` tool call with a summary, or replied with a
    /// stop-reason text answer.
    Success { summary: String, steps: u32 },
    /// Step / budget / deadline limit reached.
    Aborted { reason: String, steps: u32 },
}

/// Orchestrator parameters distinct from the [`Task`] — wiring not state.
pub struct OrchestratorConfig<'a, B> {
    pub task: Task,
    pub browser: B,
    pub provider: Arc<dyn LlmProvider>,
    pub event_log: &'a mut EventLog,
    /// Optional snapshot store to persist every snapshot to disk.
    pub snapshot_store: Option<&'a SnapshotStore>,
}

/// Public entry point — run a single task to completion.
///
/// # Errors
/// - [`CoreError::Budget`] if budget enforcement returns a hard halt.
/// - [`CoreError::Llm`] for non-retryable provider failures.
/// - [`CoreError::Browser`] for browser-level failures the dispatcher
///   could not recover from.
/// - [`CoreError::StepLimit`] when the loop exceeds `task.max_steps`.
pub async fn run<B: BrowserAdapter>(cfg: OrchestratorConfig<'_, B>) -> Result<RunOutcome> {
    let task = cfg.task.clone();
    let mut budget = BudgetGuard::new(task.budget, task.pricing)?;
    let dispatcher = ActionDispatcher::new(&cfg.browser);
    let mut ref_map = RefMap::new();
    let mut conversation: Vec<Message> = build_initial_conversation(&task);
    let tools = Action::default_tool_definitions();
    let started_at = Instant::now();

    cfg.event_log.append(Event::RunStarted {
        task_id: task.id.clone(),
        goal: task.goal.clone(),
    })?;

    // 0) Initial navigation. Counts as the first orchestrator step.
    info!(task_id = %task.id, "starting orchestrator run");
    let nav_action = Action::Navigate {
        url: task.start_url.clone(),
    };
    cfg.event_log.append(Event::ActionRequested {
        action: nav_action.clone(),
    })?;
    let nav_result = dispatcher
        .dispatch(nav_action.clone(), &HashMap::new())
        .await?;
    cfg.event_log.append(Event::ActionResult {
        action: nav_action,
        success: nav_result.success,
        message: nav_result.message,
    })?;

    let mut step: u32 = 0;
    loop {
        step += 1;
        debug!(step, "loop iteration");

        // Step deadline check.
        if step > task.max_steps {
            cfg.event_log.append(Event::RunFinished {
                outcome: "aborted".into(),
                steps: step - 1,
                spend: budget.spend(),
            })?;
            return Err(CoreError::StepLimit(task.max_steps));
        }

        // 1) Snapshot.
        let tree = cfg.browser.accessibility().await?;
        let url = cfg
            .browser
            .current_url()
            .await?
            .unwrap_or_else(|| task.start_url.clone());
        let raw = raw_tree_from_accessibility(&url, &tree);
        let token_estimator = genatp_perception::TokenEstimator::default();
        let snapshot: CompressedSnapshot =
            compress(&raw, &mut ref_map, &CompressOptions::default());
        // Build the a11y_id → backend_dom_node_id lookup. Synthetic nodes
        // (WebArea, etc.) lack a backend id; they're silently absent from
        // the table, which makes "click @e1" surface as InvalidAction
        // rather than a generic Browser error.
        let a11y_to_backend: HashMap<String, String> = tree
            .nodes()
            .iter()
            .filter_map(|n| {
                n.backend_dom_node_id
                    .map(|b| (n.node_id.clone(), b.to_string()))
            })
            .collect();
        let ref_to_node = build_ref_lookup(&raw, &mut ref_map, &a11y_to_backend);

        let token_estimate = token_estimator
            .estimate_snapshot(&snapshot)
            .map_err(|e| CoreError::EventLog(format!("token estimate: {e}")))?
            as usize;
        let snapshot_hash = if let Some(store) = cfg.snapshot_store {
            let hash = store
                .put(&snapshot)
                .map_err(|e| CoreError::EventLog(format!("snapshot store put: {e}")))?;
            hash.to_hex()
        } else {
            String::new()
        };
        cfg.event_log.append(Event::Snapshot {
            url: url.clone(),
            snapshot_hash,
            token_estimate,
        })?;

        // 2) LLM call (with retries).
        let snapshot_msg = Message::user(format!(
            "Current page URL: {url}\n\nAccessibility snapshot (compressed):\n```json\n{}\n```\n\nDecide the next tool call. Use `finish` when the task is done.",
            serde_json::to_string_pretty(&snapshot).unwrap_or_default(),
        ));
        conversation.push(snapshot_msg);

        let request =
            CompletionRequest::chat(&task.model, conversation.clone()).with_tools(tools.clone());

        cfg.event_log.append(Event::LlmRequest {
            provider: cfg.provider.name().to_string(),
            model: task.model.clone(),
            prompt_messages: u32::try_from(conversation.len()).unwrap_or(u32::MAX),
        })?;

        let response = match retrying_complete(cfg.provider.as_ref(), &request, &task.retry).await {
            Ok(r) => r,
            Err(err) => {
                let retryable = err.is_retryable();
                cfg.event_log.append(Event::LlmError {
                    provider: cfg.provider.name().to_string(),
                    message: err.to_string(),
                    retryable,
                })?;
                cfg.event_log.append(Event::RunFinished {
                    outcome: "aborted".into(),
                    steps: step,
                    spend: budget.spend(),
                })?;
                return Err(CoreError::Llm(err));
            }
        };

        cfg.event_log.append(Event::LlmResponse {
            provider: cfg.provider.name().to_string(),
            model: response.model.clone(),
            usage: response.usage,
            finish_reason: format!("{:?}", response.finish_reason),
        })?;

        // Update conversation with the assistant turn so the model sees its
        // own prior tool calls on the next iteration.
        conversation.push(response.message.clone());

        // 3) Budget enforcement.
        match budget.record_step(response.usage) {
            BudgetCheck::Ok => {}
            BudgetCheck::Warn(dim) => {
                warn!(dim, "budget soft warning");
                cfg.event_log.append(Event::BudgetWarn {
                    dimension: dim.to_string(),
                    spend: budget.spend(),
                })?;
            }
            check @ BudgetCheck::Halt(dim) => {
                cfg.event_log.append(Event::BudgetExceeded {
                    dimension: dim.to_string(),
                    spend: budget.spend(),
                })?;
                cfg.event_log.append(Event::RunFinished {
                    outcome: "aborted".into(),
                    steps: step,
                    spend: budget.spend(),
                })?;
                let halt = budget
                    .halt_error(check)
                    .expect("BudgetCheck::Halt yields halt_error");
                return Err(CoreError::Budget(halt));
            }
        }

        // 4) Dispatch any tool calls the model emitted.
        if !response.message.tool_calls.is_empty() {
            for call in &response.message.tool_calls {
                let action = Action::from_tool_call(call)?;
                cfg.event_log.append(Event::ActionRequested {
                    action: action.clone(),
                })?;
                let result: ActionResult =
                    dispatcher.dispatch(action.clone(), &ref_to_node).await?;
                cfg.event_log.append(Event::ActionResult {
                    action: action.clone(),
                    success: result.success,
                    message: result.message.clone(),
                })?;

                if let Action::Finish { summary } = action {
                    cfg.event_log.append(Event::RunFinished {
                        outcome: "success".into(),
                        steps: step,
                        spend: budget.spend(),
                    })?;
                    return Ok(RunOutcome::Success {
                        summary,
                        steps: step,
                    });
                }

                // Append a tool-result message for the model's next turn.
                conversation.push(Message::tool(
                    serde_json::to_string(&result).unwrap_or_else(|_| "ok".into()),
                    &call.id,
                ));
            }
        } else if response.finish_reason == FinishReason::Stop {
            // Model produced a textual final answer.
            cfg.event_log.append(Event::RunFinished {
                outcome: "success".into(),
                steps: step,
                spend: budget.spend(),
            })?;
            return Ok(RunOutcome::Success {
                summary: response.message.content.clone(),
                steps: step,
            });
        }

        // Wall-time check (independent of LLM usage).
        if !task.budget.max_wall.is_zero() && started_at.elapsed() >= task.budget.max_wall {
            cfg.event_log.append(Event::BudgetExceeded {
                dimension: "wall".into(),
                spend: budget.spend(),
            })?;
            cfg.event_log.append(Event::RunFinished {
                outcome: "aborted".into(),
                steps: step,
                spend: budget.spend(),
            })?;
            return Err(CoreError::Deadline(started_at.elapsed()));
        }
    }
}

fn build_initial_conversation(task: &Task) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    let default_system = "You are an autonomous browser agent. \
You receive compressed accessibility snapshots of the current page and must \
call one of the provided tools (navigate, click, fill, press, scroll, wait, \
finish) on each turn. Refer to elements only by their @eN refs. Call `finish` \
with a summary as soon as the task is complete.";
    messages.push(Message::system(
        task.system_prompt.as_deref().unwrap_or(default_system),
    ));
    messages.push(Message::user(format!("Goal: {}", task.goal)));
    messages
}

/// Build a `Ref -> CDP node_id` lookup the dispatcher uses to resolve
/// LLM-supplied `@eN` references. Re-runs ref assignment over the raw
/// tree because the compress() call above consumed (advanced) the
/// `RefMap`'s state — for stability we must use the *same* identity
/// resolution, but the function returns its own internal `node_id -> ref`
/// map (which we invert here).
///
/// Phase 4 will surface this directly out of `compress()`; for now the
/// double pass keeps the API of `compress()` unchanged.
fn build_ref_lookup(
    raw: &genatp_perception::raw::RawTree,
    ref_map: &mut RefMap,
    a11y_to_backend: &HashMap<String, String>,
) -> HashMap<Ref, String> {
    // Strip + ref-assign mirrors what `compress` does internally, but
    // returns the table we need. RefMap stays stable across both calls.
    let stripped = genatp_perception::strip(raw, &genatp_perception::StripPolicy::default());
    let node_to_ref = ref_map.assign_for_tree(&stripped);
    let mut out: HashMap<Ref, String> = HashMap::new();
    for (a11y_node_id, r) in node_to_ref {
        if let Some(backend) = a11y_to_backend.get(&a11y_node_id) {
            out.insert(r, backend.clone());
        }
        // Nodes without a backend id (e.g. WebArea) intentionally do not
        // appear — they're not actionable.
    }
    out
}

async fn retrying_complete(
    provider: &dyn LlmProvider,
    request: &CompletionRequest,
    policy: &RetryPolicy,
) -> genatp_llm::Result<genatp_llm::CompletionResponse> {
    let mut rng = fastrand::Rng::new();
    let mut attempt: u32 = 0;
    let mut cumulative = Duration::ZERO;
    loop {
        let result = provider.complete(request.clone()).await;
        match result {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                let retryable = err.is_retryable();
                if !retryable || !policy.should_retry(attempt, cumulative) {
                    return Err(err);
                }
                let mut delay = policy.delay_for(attempt + 1, &mut rng);
                if let Some(suggested) = err.retry_after() {
                    delay = delay.max(suggested);
                }
                cumulative = cumulative.saturating_add(delay);
                if cumulative > policy.max_total_delay {
                    return Err(err);
                }
                debug!(attempt, ?delay, "llm retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

// Allow passing `&B` as a `BrowserAdapter` argument without consuming it.
#[async_trait::async_trait]
impl<T: BrowserAdapter + ?Sized> BrowserAdapter for &T {
    async fn navigate(&self, url: &str) -> Result<()> {
        (**self).navigate(url).await
    }
    async fn click_node(&self, node_id: &str) -> Result<()> {
        (**self).click_node(node_id).await
    }
    async fn fill_node(&self, node_id: &str, value: &str) -> Result<()> {
        (**self).fill_node(node_id, value).await
    }
    async fn press_key(&self, key: &str) -> Result<()> {
        (**self).press_key(key).await
    }
    async fn scroll(&self, dx: i32, dy: i32) -> Result<()> {
        (**self).scroll(dx, dy).await
    }
    async fn current_url(&self) -> Result<Option<String>> {
        (**self).current_url().await
    }
    async fn accessibility(&self) -> Result<genatp_browser::AccessibilityTree> {
        (**self).accessibility().await
    }
}
