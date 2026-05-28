//! Provider-agnostic live form-fill end-to-end.
//!
//! Gated on `GENATP_LIVE=1` so default `cargo test` never reaches the
//! network. The provider is whichever [`genatp_llm::resolve_from_env`]
//! picks — NVIDIA NIM, Anthropic, OpenAI, or Ollama — depending on env
//! vars (see `docs/CONFIGURATION.md`).
//!
//! ```bash
//! # Example: NVIDIA NIM with llama-4-maverick (free tier)
//! export NVIDIA_API_KEY=nvapi-…
//! export GENATP_LLM_PROVIDER=nim
//! export GENATP_LLM_MODEL=meta/llama-4-maverick-17b-128e-instruct
//! GENATP_LIVE=1 cargo test -p genatp-core --test form_fill_live \
//!   -- --ignored --nocapture
//! ```

mod common;

use std::time::Duration;

use genatp_browser::launch;
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::event_log::EventLog;
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run};
use genatp_core::page_adapter::PageAdapter;
use genatp_core::task::Task;
use genatp_llm::resolve_from_env;

use common::{FixtureServer, chrome_available, chrome_test_guard, test_launch_options};

#[tokio::test]
#[ignore = "live LLM; set GENATP_LIVE=1 and a provider env var to enable"]
async fn live_provider_drives_form_fill_to_completion() {
    if std::env::var_os("GENATP_LIVE").is_none() {
        eprintln!("GENATP_LIVE unset — skipping live form-fill.");
        return;
    }
    if !chrome_available() {
        eprintln!("Chrome not available; skipping live form-fill.");
        return;
    }

    let resolved = resolve_from_env().expect("provider from env");
    eprintln!(
        "live provider: {} (kind: {:?}, model: {})",
        resolved.provider.name(),
        resolved.kind,
        resolved.model
    );

    let _chrome_lock = chrome_test_guard().await;
    let server = FixtureServer::start().await;
    let form_url = server.url("form.html");

    let browser = launch(test_launch_options()).await.expect("launch chrome");
    let page = browser.new_page().await.expect("new page");

    let task = Task {
        id: "form-fill-live".into(),
        goal: "Fill the sign-up form. Use these values: \
               name = \"Ada Lovelace\", \
               email = \"ada@example.com\", \
               message = \"Hello from genATP.\". \
               Tick the agreement checkbox, then click Submit. \
               When the page shows the submission result, call `finish`."
            .into(),
        start_url: form_url.clone(),
        model: resolved.model.clone(),
        system_prompt: Some(
            "You are a careful browser-automation agent. On every turn you \
             receive a JSON snapshot of the page's accessibility tree. \
             Each interactive element has an `@eN` ref. Refer to elements \
             ONLY by their refs. Output exactly one tool call per turn. \
             Call `finish` as soon as the task is complete."
                .into(),
        ),
        max_steps: 20,
        budget: BudgetLimits {
            max_tokens: 200_000,
            max_wall: Duration::from_secs(15 * 60),
            ..BudgetLimits::default()
        },
        pricing: Pricing::FREE,
        retry: genatp_core::RetryPolicy::default(),
    };

    let mut log = EventLog::in_memory();

    let outcome = run(OrchestratorConfig {
        task,
        browser: PageAdapter::new(page),
        provider: resolved.provider,
        event_log: &mut log,
        snapshot_store: None,
    })
    .await
    .expect("orchestrator should finish (success or controlled abort)");

    eprintln!(
        "live run finished in {} events: {outcome:?}",
        log.entries().len()
    );

    match outcome {
        RunOutcome::Success { summary, steps } => {
            eprintln!("SUCCESS ({steps} steps): {summary}");
        }
        RunOutcome::Aborted { reason, steps } => {
            eprintln!("ABORTED ({steps} steps): {reason}");
        }
    }

    assert_eq!(log.verify().expect("verify"), None, "hash chain intact");

    browser
        .shutdown(Duration::from_secs(5))
        .await
        .expect("shutdown");
}
