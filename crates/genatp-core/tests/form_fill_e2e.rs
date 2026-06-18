//! End-to-end form-fill against the fixture site.
//!
//! Drives Chrome via [`PageAdapter`] + the orchestrator with a scripted
//! [`MockProvider`] so we don't need an LLM endpoint. The model script
//! mirrors what a real model would do: snapshot → fill → fill → fill →
//! click → click → finish.
//!
//! Gated on Chrome being available — skipped silently otherwise so CI
//! without Chrome (Phase 0 macOS runner) keeps green.

mod common;

use std::sync::Arc;
use std::time::Duration;

use genatp_browser::{AccessibilityTree, LaunchOptions, Page, launch};
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::event_log::{Event, EventLog};
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run};
use genatp_core::page_adapter::PageAdapter;
use genatp_core::task::Task;
use genatp_llm::{
    CompletionResponse, FinishReason, LlmProvider, Message, MockProvider, ToolCall, Usage,
};

use common::{FixtureServer, chrome_available, chrome_test_guard, test_launch_options};

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
            prompt_tokens: 100,
            completion_tokens: 30,
            total_tokens: 130,
        },
        provider: "mock".into(),
        model: "mock".into(),
    }
}

#[tokio::test]
async fn agent_fills_and_submits_signup_form() {
    if !chrome_available() {
        eprintln!("Chrome not available; skipping form-fill E2E test.");
        return;
    }

    let _chrome_lock = chrome_test_guard().await;
    let server = FixtureServer::start().await;
    let form_url = server.url("form.html");

    // Refs assigned by the perception compressor for fixtures/sites/form.html:
    //   @e8  Name textbox     @e11 Email textbox    @e14 Message textbox
    //   @e15 Agree checkbox   @e16 Submit button
    //
    // If you regenerate refs (e.g. by changing the page or the strip
    // policy), re-run with `GENATP_DUMP_SNAPSHOT=1` and update these.
    let provider = Arc::new(MockProvider::new(vec![
        scripted(
            "",
            vec![(
                "c1",
                "fill",
                serde_json::json!({"ref": "@e8", "value": "Ada Lovelace"}),
            )],
        ),
        scripted(
            "",
            vec![(
                "c2",
                "fill",
                serde_json::json!({"ref": "@e11", "value": "ada@example.com"}),
            )],
        ),
        scripted(
            "",
            vec![(
                "c3",
                "fill",
                serde_json::json!({"ref": "@e14", "value": "Hello, world."}),
            )],
        ),
        scripted(
            "",
            vec![("c4", "click", serde_json::json!({"ref": "@e15"}))],
        ),
        scripted(
            "",
            vec![("c5", "click", serde_json::json!({"ref": "@e16"}))],
        ),
        scripted(
            "",
            vec![(
                "c6",
                "finish",
                serde_json::json!({"summary": "Form submitted as Ada Lovelace."}),
            )],
        ),
    ]));

    let opts: LaunchOptions = test_launch_options();
    let browser = launch(opts).await.expect("launch chrome");
    let page: Page = browser.new_page().await.expect("new page");
    let page_for_verify = page.clone();

    // One-shot diagnostic: navigate + take a snapshot so we can see what
    // refs perception will assign to the form's controls before locking
    // them into the scripted tool calls. Stays in the test code (gated
    // behind GENATP_DUMP_SNAPSHOT) to aid future debugging.
    if std::env::var_os("GENATP_DUMP_SNAPSHOT").is_some() {
        page.navigate(&form_url).await.expect("navigate");
        let tree = page.accessibility_tree().await.expect("tree");
        let raw = genatp_core::perception_bridge::raw_tree_from_accessibility(&form_url, &tree);
        let mut refs = genatp_perception::RefMap::new();
        let snap = genatp_perception::compress(
            &raw,
            &mut refs,
            &genatp_perception::CompressOptions::default(),
        );
        eprintln!(
            "DIAGNOSTIC snapshot:\n{}",
            serde_json::to_string_pretty(&snap).unwrap()
        );
    }

    let task = Task {
        id: "form-fill-e2e".into(),
        goal: "Fill the sign-up form with Ada's details and submit.".into(),
        start_url: form_url.clone(),
        model: "mock".into(),
        system_prompt: None,
        max_steps: 12,
        budget: BudgetLimits {
            max_tokens: 50_000,
            max_wall: Duration::from_secs(60),
            ..BudgetLimits::default()
        },
        pricing: Pricing::FREE,
        retry: genatp_core::RetryPolicy::NEVER,
    };

    let mut log = EventLog::in_memory();
    let outcome = run(OrchestratorConfig {
        task,
        browser: PageAdapter::new(page),
        provider: provider as Arc<dyn LlmProvider>,
        event_log: &mut log,
        snapshot_store: None,
        policy_engine: genatp_perception::PolicyEngine::default(),
        vision_provider: None,
    })
    .await
    .expect("orchestrator should succeed");

    assert!(
        matches!(outcome, RunOutcome::Success { .. }),
        "expected Success; got {outcome:?}"
    );

    // Event log invariants.
    assert_eq!(log.verify().expect("verify"), None, "hash chain intact");

    let last = log.entries().last().expect("non-empty log");
    assert!(
        matches!(&last.event, Event::RunFinished { outcome, .. } if outcome == "success"),
        "last event must be RunFinished/success",
    );

    // Confirm the page actually shows the result panel — proves we drove a
    // real Chrome, not a no-op stub.
    let tree: AccessibilityTree = page_for_verify
        .accessibility_tree()
        .await
        .expect("post-run accessibility tree");
    let result_text = tree
        .nodes()
        .iter()
        .filter_map(|n| n.name.as_deref())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        result_text.contains("Ada"),
        "expected the submitted name in the post-run a11y tree, got: {result_text}"
    );

    browser
        .shutdown(Duration::from_secs(5))
        .await
        .expect("shutdown");
}
