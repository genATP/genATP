//! Probe which LLM provider `resolve_from_env()` selects and optionally
//! send one completion.
//!
//! ```bash
//! # Detection only (no network):
//! cargo run -p genatp-llm --example provider_probe
//!
//! # Live round-trip (needs credentials for the selected provider):
//! cargo run -p genatp-llm --example provider_probe -- --live
//! ```

use genatp_llm::{CompletionRequest, Message, resolve_from_env};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let live = std::env::args().any(|a| a == "--live");
    let resolved = resolve_from_env()?;
    println!(
        "provider: {} (kind: {:?}, model: {})",
        resolved.provider.name(),
        resolved.kind,
        resolved.model
    );

    if !live {
        println!("(detection only — pass --live to send one completion)");
        return Ok(());
    }

    let request = CompletionRequest::chat(
        &resolved.model,
        vec![Message::user("Reply with exactly: genatp-ok")],
    );
    let response = resolved.provider.complete(request).await?;
    let text = response.message.content.trim();
    println!("response: {text}");
    if text.is_empty() {
        eprintln!("warning: empty response body");
    } else {
        println!("live round-trip: OK");
    }
    Ok(())
}
