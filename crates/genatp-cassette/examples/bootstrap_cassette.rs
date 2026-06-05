//! Bootstrap a real cassette into `~/.genatp/` so the CLI can list/inspect it.
//!
//! Usage: `cargo run --example bootstrap_cassette`
//!
//! This ensures the signing keypair exists and creates a sample cassette.

use std::fs;

use genatp_cassette::record::CassetteRecorder;
use genatp_cassette::signing;
use genatp_cassette::storage;
use genatp_core::action::Action;
use genatp_core::budget::BudgetSpend;
use genatp_core::event_log::{Event, EventLog};
use genatp_llm::Usage;

fn build_sample_log() -> EventLog {
    let mut log = EventLog::in_memory();

    log.append(Event::RunStarted {
        task_id: "cli-test".into(),
        goal: "Navigate to example.com and verify the page title".into(),
    })
    .unwrap();

    // Navigate
    log.append(Event::ActionRequested {
        action: Action::Navigate {
            url: "https://example.com".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Navigate {
            url: "https://example.com".into(),
        },
        success: true,
        message: Some("Navigated to example.com".into()),
    })
    .unwrap();

    // Step 1: Snapshot → LLM → Click
    log.append(Event::Snapshot {
        url: "https://example.com".into(),
        snapshot_hash: "a1b2c3d4e5f6a7b8c9d0e1f2".into(),
        token_estimate: 200,
    })
    .unwrap();
    log.append(Event::LlmRequest {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        prompt_messages: 3,
    })
    .unwrap();
    log.append(Event::LlmResponse {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        usage: Usage {
            prompt_tokens: 180,
            completion_tokens: 25,
            total_tokens: 205,
        },
        finish_reason: "tool_calls".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Click {
            r#ref: "@e3".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Click {
            r#ref: "@e3".into(),
        },
        success: true,
        message: Some("Clicked 'Learn more' link".into()),
    })
    .unwrap();

    // Step 2: Snapshot → LLM → Finish
    log.append(Event::Snapshot {
        url: "https://example.com/more".into(),
        snapshot_hash: "f2e3d4c5b6a79887a6b5c4d3".into(),
        token_estimate: 150,
    })
    .unwrap();
    log.append(Event::LlmRequest {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        prompt_messages: 2,
    })
    .unwrap();
    log.append(Event::LlmResponse {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        usage: Usage {
            prompt_tokens: 120,
            completion_tokens: 30,
            total_tokens: 150,
        },
        finish_reason: "end_turn".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Finish {
            summary: "Verified the page title — 'Example Domain'".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Finish {
            summary: "Verified the page title — 'Example Domain'".into(),
        },
        success: true,
        message: None,
    })
    .unwrap();

    log.append(Event::RunFinished {
        outcome: "success".into(),
        steps: 2,
        spend: BudgetSpend {
            tokens: 355,
            usd: 0.012,
            steps: 2,
            started_at_utc: None,
        },
    })
    .unwrap();

    log
}

fn main() {
    println!("🗜️  genATP Phase 7 — CLI Bootstrap");
    println!();

    // Step 1: Ensure signing keypair exists.
    println!("🔑 Ensuring signing keypair...");
    let verifying_key = signing::ensure_keypair().expect("keypair generation");
    let pub_hex = signing::public_key_hex(&verifying_key);
    println!("   Public key: {pub_hex}");
    println!("   Key dir:    {:?}", signing::keys_dir().unwrap());

    // Step 2: Record a cassette.
    println!("\n📼 Recording cassette...");
    let slug = "example-verify-title";
    let log = build_sample_log();
    let cassette = CassetteRecorder::new(slug, "Verify page title on example.com")
        .record(&log)
        .expect("recording failed");
    println!("   Steps: {}", cassette.steps.len());

    // Step 3: Sign and save to ~/.genatp/cassettes/.
    println!("\n💾 Saving and signing...");
    let signing_key = signing::load_signing_key().expect("load signing key");
    let root = storage::cassettes_dir().expect("cassettes dir");
    let path = storage::save(&cassette, &root, &signing_key).expect("save");
    println!("   Saved to:  {}", path.display());

    // Step 4: Load and verify.
    println!("\n🔍 Verifying cassette...");
    let loaded = storage::load(&path, &verifying_key).expect("load");
    loaded.verify_hash_chain().expect("hash chain");
    println!("   Signature: VALID ✓");
    println!("   Hash chain: VALID ✓");
    println!("   Slug:      {}", loaded.header.slug);
    println!("   Version:   {}", loaded.header.version);
    println!("   Goal:      {}", loaded.header.goal);
    println!("   Model:     {}", loaded.header.model);
    println!("   Domains:   {:?}", loaded.header.domains);
    println!(
        "   File size: {} bytes",
        fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    );
    println!();

    // Step 5: List slugs.
    println!("📋 All cassettes:");
    let slugs = storage::list_slugs(&root).expect("list slugs");
    if slugs.is_empty() {
        println!("   (none)");
    } else {
        for slug in &slugs {
            let versions = storage::list_versions(&root, slug).unwrap_or_default();
            println!("   {slug:<40} {:?}", versions);
        }
    }

    println!("\n✅ Done. Try:\n");
    println!("   cargo run -p genatp-cli -- cassette list");
    println!("   cargo run -p genatp-cli -- cassette inspect example-verify-title");
}
