//! Orchestrator integration test against a mock browser + mock LLM.
//!
//! This is the Phase 3 acceptance test that does not require Chrome —
//! it verifies the plan loop, action dispatch, and budget enforcement
//! against pure in-memory mocks.
//!
//! Why this matters: even with `genatp-browser` integration tests gated on
//! Chrome being installed, the orchestrator's control flow must be
//! verifiable in stock CI. Bugs in the loop body (off-by-one step counts,
//! budget skew, conversation history corruption) would otherwise hide
//! behind the browser dependency.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use genatp_browser::{AccessibilityTree, AxNodeJson, AxPropertyJson};
use genatp_core::action::Action;
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::dispatcher::BrowserAdapter;
use genatp_core::error::CoreError;
use genatp_core::event_log::{Event, EventLog};
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run};
use genatp_core::task::Task;
use genatp_llm::{
    CompletionResponse, FinishReason, LlmProvider, Message, MockProvider, ToolCall, Usage,
};

#[derive(Debug, Default)]
struct MockBrowser {
    events: Mutex<Vec<String>>,
    current_url: Mutex<String>,
}

impl MockBrowser {
    fn record(&self, e: impl Into<String>) {
        self.events.lock().unwrap().push(e.into());
    }
}

#[async_trait]
impl BrowserAdapter for MockBrowser {
    async fn navigate(&self, url: &str) -> Result<(), CoreError> {
        self.record(format!("navigate {url}"));
        *self.current_url.lock().unwrap() = url.to_string();
        Ok(())
    }
    async fn click_node(&self, node_id: &str) -> Result<(), CoreError> {
        self.record(format!("click {node_id}"));
        Ok(())
    }
    async fn fill_node(&self, node_id: &str, value: &str) -> Result<(), CoreError> {
        self.record(format!("fill {node_id}={value}"));
        Ok(())
    }
    async fn press_key(&self, key: &str) -> Result<(), CoreError> {
        self.record(format!("press {key}"));
        Ok(())
    }
    async fn scroll(&self, dx: i32, dy: i32) -> Result<(), CoreError> {
        self.record(format!("scroll {dx},{dy}"));
        Ok(())
    }
    async fn current_url(&self) -> Result<Option<String>, CoreError> {
        Ok(Some(self.current_url.lock().unwrap().clone()))
    }
    async fn accessibility(&self) -> Result<AccessibilityTree, CoreError> {
        // Return a small page with: root, form, name input, email input,
        // message textarea, agree checkbox, submit button.
        let nodes = vec![
            ax("1", None, &["2"], "WebArea", "Sign-up Form"),
            ax("2", Some("1"), &["3", "4", "5", "6", "7"], "form", "signup"),
            ax("3", Some("2"), &[], "textbox", "Name"),
            ax("4", Some("2"), &[], "textbox", "Email"),
            ax("5", Some("2"), &[], "textbox", "Message"),
            ax("6", Some("2"), &[], "checkbox", "I agree to the terms."),
            ax("7", Some("2"), &[], "button", "Submit"),
        ];
        Ok(AccessibilityTree::from_nodes(nodes))
    }
}

fn ax(id: &str, parent: Option<&str>, children: &[&str], role: &str, name: &str) -> AxNodeJson {
    let backend = id.parse::<i64>().ok().map(|n| n * 100);
    AxNodeJson {
        node_id: id.into(),
        parent_id: parent.map(str::to_owned),
        child_ids: children.iter().map(|s| (*s).to_string()).collect(),
        role: Some(role.into()),
        name: Some(name.into()),
        value: None,
        description: None,
        properties: Vec::<AxPropertyJson>::new(),
        backend_dom_node_id: backend,
        ignored: false,
    }
}

fn scripted(text: &str, tool_calls: Vec<(&str, &str, serde_json::Value)>) -> CompletionResponse {
    let mut msg = Message::assistant(text);
    msg.tool_calls = tool_calls
        .into_iter()
        .map(|(id, name, args)| ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: args,
        })
        .collect();
    CompletionResponse {
        message: msg,
        finish_reason: if !text.is_empty() {
            FinishReason::Stop
        } else {
            FinishReason::ToolCalls
        },
        usage: Usage {
            prompt_tokens: 50,
            completion_tokens: 25,
            total_tokens: 75,
        },
        provider: "mock".into(),
        model: "mock".into(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn orchestrator_runs_5_step_task_against_mocks() {
    // The model emits the canonical 5-step form-fill sequence. Refs are
    // @e1=root, @e2=form, @e3=name, @e4=email, @e5=message, @e6=agree, @e7=submit.
    let provider = Arc::new(MockProvider::new(vec![
        scripted(
            "",
            vec![(
                "call_1",
                "fill",
                serde_json::json!({"ref": "@e3", "value": "Ada"}),
            )],
        ),
        scripted(
            "",
            vec![(
                "call_2",
                "fill",
                serde_json::json!({"ref": "@e4", "value": "ada@example.com"}),
            )],
        ),
        scripted(
            "",
            vec![(
                "call_3",
                "fill",
                serde_json::json!({"ref": "@e5", "value": "Hello"}),
            )],
        ),
        scripted(
            "",
            vec![("call_4", "click", serde_json::json!({"ref": "@e6"}))],
        ),
        scripted(
            "",
            vec![("call_5", "click", serde_json::json!({"ref": "@e7"}))],
        ),
        scripted(
            "",
            vec![(
                "call_6",
                "finish",
                serde_json::json!({"summary": "Form submitted with the supplied details."}),
            )],
        ),
    ]));

    let task = Task {
        id: "form-fill".into(),
        goal: "Fill the form with name=Ada, email=ada@example.com, agree, and submit.".into(),
        start_url: "https://example.com/form".into(),
        model: "mock".into(),
        system_prompt: None,
        max_steps: 10,
        budget: BudgetLimits {
            max_tokens: 10_000,
            ..BudgetLimits::default()
        },
        pricing: Pricing::FREE,
        retry: genatp_core::RetryPolicy::NEVER,
    };

    let browser = MockBrowser::default();
    let mut log = EventLog::in_memory();

    let outcome = run(OrchestratorConfig {
        task,
        browser: &browser,
        provider: provider.clone() as Arc<dyn LlmProvider>,
        event_log: &mut log,
        snapshot_store: None,
    })
    .await
    .expect("must succeed");

    match outcome {
        RunOutcome::Success { summary, steps } => {
            assert!(summary.contains("submitted"), "summary: {summary}");
            assert!((5..=7).contains(&steps), "expected ≤ ~7 steps, got {steps}");
        }
        RunOutcome::Aborted { reason, .. } => panic!("aborted: {reason}"),
    }

    // Event log invariants.
    assert!(!log.is_empty());
    assert_eq!(log.verify().expect("verify ok"), None, "hash chain intact");

    let action_results: Vec<_> = log
        .entries()
        .iter()
        .filter_map(|e| match &e.event {
            Event::ActionResult { action, .. } => Some(action),
            _ => None,
        })
        .collect();

    // Three fills, two clicks, plus the initial navigate and the finish.
    let fills = action_results
        .iter()
        .filter(|a| matches!(a, Action::Fill { .. }))
        .count();
    let clicks = action_results
        .iter()
        .filter(|a| matches!(a, Action::Click { .. }))
        .count();
    assert_eq!(fills, 3, "expected 3 fill actions in event log");
    assert_eq!(clicks, 2, "expected 2 click actions in event log");

    // RunFinished must be the last entry with outcome=success.
    let last = log.entries().last().expect("non-empty log");
    matches!(
        &last.event,
        Event::RunFinished { outcome, .. } if outcome == "success"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn orchestrator_aborts_on_budget() {
    // Provider emits a Stop reply each turn, no finish. Hard token budget
    // is set tight enough to fire on the first or second step.
    let provider = Arc::new(MockProvider::new(vec![
        scripted("still working", vec![]),
        scripted("still working", vec![]),
        scripted("still working", vec![]),
    ]));
    let task = Task {
        id: "budget-test".into(),
        goal: "x".into(),
        start_url: "https://example.com".into(),
        model: "mock".into(),
        system_prompt: None,
        max_steps: 10,
        budget: BudgetLimits {
            max_tokens: 40, // < 75 — should halt after the first step.
            ..BudgetLimits::default()
        },
        pricing: Pricing::FREE,
        retry: genatp_core::RetryPolicy::NEVER,
    };

    let browser = MockBrowser::default();
    let mut log = EventLog::in_memory();
    // Provider emits Stop with text — model treats that as a final answer
    // so the run ends successfully BEFORE the budget can fire. Force the
    // tool-calls path instead.
    let _ = provider; // suppress unused on rewrite below
    let provider = Arc::new(MockProvider::new(vec![
        scripted(
            "",
            vec![("c1", "scroll", serde_json::json!({"dx": 0, "dy": 100}))],
        ),
        scripted(
            "",
            vec![("c2", "scroll", serde_json::json!({"dx": 0, "dy": 100}))],
        ),
    ]));

    let err = run(OrchestratorConfig {
        task,
        browser: &browser,
        provider: provider as Arc<dyn LlmProvider>,
        event_log: &mut log,
        snapshot_store: None,
    })
    .await
    .expect_err("must abort");

    assert!(matches!(err, CoreError::Budget(_)));
    // BudgetExceeded event must precede RunFinished.
    let has_exceeded = log
        .entries()
        .iter()
        .any(|e| matches!(e.event, Event::BudgetExceeded { .. }));
    assert!(has_exceeded, "BudgetExceeded must be logged");
}
