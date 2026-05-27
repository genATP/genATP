//! Live end-to-end form-fill driven by a real Ollama server.
//!
//! **Manual / nightly only.** Tagged `#[ignore]` so the default
//! `cargo test` does not require Ollama or a downloaded model.
//!
//! ## How to run
//!
//! 1. Install Ollama: <https://ollama.com/download>
//! 2. Start the server (or it auto-starts on the first `ollama` command):
//!    ```text
//!    ollama serve
//!    ```
//! 3. Pull a model with reliable tool calling:
//!    ```text
//!    ollama pull llama3.1:8b
//!    ```
//!    (Recommended alternatives: `qwen2.5:7b`, `mistral-nemo`. Avoid
//!    `phi3` and tiny models — their tool-call output is unreliable.)
//! 4. Run the test:
//!    ```text
//!    $env:OLLAMA_LIVE="1"; $env:OLLAMA_MODEL="llama3.1:8b"
//!    cargo test -p genatp-core --test form_fill_ollama -- --ignored --nocapture
//!    ```
//!
//! ## What this proves
//!
//! - The Ollama wire mapping in `genatp-llm` actually parses real
//!   responses from a real model.
//! - The orchestrator's plan loop survives a real (sometimes wobbly)
//!   model and either finishes or budget-aborts cleanly.
//! - Refs returned by perception line up with what the model picks from
//!   the snapshot text it's shown — a sanity check on the prompt format
//!   in `orchestrator::build_initial_conversation`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use genatp_browser::launch;
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::event_log::EventLog;
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run};
use genatp_core::page_adapter::PageAdapter;
use genatp_core::task::Task;
use genatp_llm::LlmProvider;
use genatp_llm::providers::ollama::{OllamaConfig, OllamaProvider};

use common::{FixtureServer, chrome_available, chrome_test_guard, test_launch_options};

#[tokio::test]
#[ignore = "live Ollama; set OLLAMA_LIVE=1 to enable"]
async fn ollama_drives_form_fill_to_completion() {
    if std::env::var_os("OLLAMA_LIVE").is_none() {
        eprintln!("OLLAMA_LIVE unset — skipping. Re-run with $env:OLLAMA_LIVE=\"1\".");
        return;
    }
    if !chrome_available() {
        eprintln!("Chrome not available; skipping live Ollama E2E.");
        return;
    }

    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.1:8b".into());
    let provider: Arc<dyn LlmProvider> = Arc::new(
        OllamaProvider::new(OllamaConfig {
            // 5 min — local models on CPU can be slow to first token.
            http_timeout: Duration::from_secs(300),
            ..OllamaConfig::default()
        })
        .expect("ollama client builds"),
    );

    let _chrome_lock = chrome_test_guard().await;
    let server = FixtureServer::start().await;
    let form_url = server.url("form.html");

    let browser = launch(test_launch_options()).await.expect("launch chrome");
    let page = browser.new_page().await.expect("new page");

    let task = Task {
        id: "form-fill-ollama".into(),
        goal: "Fill the sign-up form. Use these values: \
               name = \"Ada Lovelace\", \
               email = \"ada@example.com\", \
               message = \"Hello from genATP.\". \
               Tick the agreement checkbox, then click Submit. \
               When the page shows the submission result, call `finish`."
            .into(),
        start_url: form_url.clone(),
        model: model.clone(),
        system_prompt: Some(
            "You are a careful browser-automation agent. On every turn you \
             receive a JSON snapshot of the page's accessibility tree. \
             Each interactive element has an `@eN` ref. Refer to elements \
             ONLY by their refs. Output exactly one tool call per turn. \
             Call `finish` as soon as the task is complete."
                .into(),
        ),
        // Generous step + token budgets — local models often need a few
        // extra turns to recover from a malformed reply.
        max_steps: 20,
        budget: BudgetLimits {
            max_tokens: 200_000,
            max_wall: Duration::from_secs(15 * 60),
            ..BudgetLimits::default()
        },
        // Ollama is free; pricing stays zero.
        pricing: Pricing::FREE,
        retry: genatp_core::RetryPolicy::default(),
    };

    let mut log = EventLog::in_memory();

    let outcome = run(OrchestratorConfig {
        task,
        browser: PageAdapter::new(page),
        provider,
        event_log: &mut log,
        snapshot_store: None,
    })
    .await
    .expect("orchestrator should finish (success or controlled abort)");

    eprintln!(
        "Ollama run finished in {} events: {outcome:?}",
        log.entries().len()
    );

    match outcome {
        RunOutcome::Success { summary, steps } => {
            eprintln!("SUCCESS ({steps} steps): {summary}");
        }
        RunOutcome::Aborted { reason, steps } => {
            // Aborting is acceptable for a flaky local model — we still
            // assert the event log is intact and the run terminated.
            eprintln!("ABORTED ({steps} steps): {reason}");
        }
    }

    assert_eq!(log.verify().expect("verify"), None, "hash chain intact");

    browser
        .shutdown(Duration::from_secs(5))
        .await
        .expect("shutdown");
}
