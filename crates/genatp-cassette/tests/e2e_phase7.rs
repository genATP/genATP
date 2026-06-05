//! Phase 7 — End-to-end integration test for the cassette recording pipeline.
//!
//! This test exercises the full lifecycle:
//!
//! 1. **Key generation** — sign/verify roundtrip, hex encoding, tamper detection
//! 2. **Event log construction** — simulate a realistic multi-step browser
//!    automation session (navigate → snapshot → LLM → fill → snapshot →
//!    LLM → click → snapshot → LLM → finish)
//! 3. **Cassette recording** — `CassetteRecorder::record()` converts the
//!    event log into a signed cassette
//! 4. **Signing + Storage** — `record_and_save_to()` compresses with zstd,
//!    appends an ed25519 signature, and persists to disk
//! 5. **Loading + Verification** — `storage::load()` decompresses, verifies
//!    the signature, parses JSON, verifies the hash chain
//! 6. **Tamper detection** — flip a byte in the saved file → load must fail
//! 7. **Slug/version listing** — `storage::list_slugs()` and `list_versions()`
//! 8. **Budget tracking** — verify spend is correctly captured in cassette header
//! 9. **Domain extraction** — domains are correctly extracted from event log
//! 10. **File format** — verify zstd(canonical_json) + 64-byte ed25519 sig format

use std::fs;

use ed25519_dalek::SigningKey;
use genatp_cassette::record::CassetteRecorder;
use genatp_cassette::schema::{compute_step_hash, step_payload_for_hash};
use genatp_cassette::signing;
use genatp_cassette::storage;
use genatp_core::action::Action;
use genatp_core::budget::BudgetSpend;
use genatp_core::event_log::{Event, EventLog};
use genatp_llm::Usage;
use rand::rngs::OsRng;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Construct a realistic multi-step event log that mimics a real agent session
/// on a web page. The scenario: navigate to a site → fill a form → click
/// submit → verify → finish.
fn build_realistic_event_log() -> EventLog {
    let mut log = EventLog::in_memory();

    // ── Task started ──────────────────────────────────────────────────────
    log.append(Event::RunStarted {
        task_id: "e2e-test-real-world".into(),
        goal: "Fill in the contact form on example.com and verify the success message".into(),
    })
    .unwrap();

    // ── Step 0: Initial navigation ────────────────────────────────────────
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
        message: Some("Page loaded successfully".into()),
    })
    .unwrap();

    // ── Step 1: Snapshot → LLM response → Fill email field ────────────────
    log.append(Event::Snapshot {
        url: "https://example.com/login".into(),
        snapshot_hash: "b3f1a2c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0".into(),
        token_estimate: 450,
    })
    .unwrap();
    log.append(Event::LlmRequest {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        prompt_messages: 5,
    })
    .unwrap();
    log.append(Event::LlmResponse {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        usage: Usage {
            prompt_tokens: 420,
            completion_tokens: 85,
            total_tokens: 505,
        },
        finish_reason: "tool_calls".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Fill {
            r#ref: "@e12".into(),
            value: "john.doe@example.com".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Fill {
            r#ref: "@e12".into(),
            value: "john.doe@example.com".into(),
        },
        success: true,
        message: Some("Email field filled".into()),
    })
    .unwrap();

    // ── Step 2: Snapshot → LLM response → Click submit ────────────────────
    log.append(Event::Snapshot {
        url: "https://example.com/login".into(),
        snapshot_hash: "a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b0".into(),
        token_estimate: 380,
    })
    .unwrap();
    log.append(Event::LlmRequest {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        prompt_messages: 6,
    })
    .unwrap();
    log.append(Event::LlmResponse {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        usage: Usage {
            prompt_tokens: 510,
            completion_tokens: 42,
            total_tokens: 552,
        },
        finish_reason: "tool_calls".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Click {
            r#ref: "@e7".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Click {
            r#ref: "@e7".into(),
        },
        success: true,
        message: Some("Submit button clicked".into()),
    })
    .unwrap();

    // ── Step 3: Snapshot → LLM response → Scroll to verify ────────────────
    log.append(Event::Snapshot {
        url: "https://example.com/success".into(),
        snapshot_hash: "f1e2d3c4b5a69788796a5b4c3d2e1f0a9b8c7d6e".into(),
        token_estimate: 320,
    })
    .unwrap();
    log.append(Event::LlmRequest {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        prompt_messages: 4,
    })
    .unwrap();
    log.append(Event::LlmResponse {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        usage: Usage {
            prompt_tokens: 290,
            completion_tokens: 60,
            total_tokens: 350,
        },
        finish_reason: "tool_calls".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Scroll { dx: 0, dy: 200 },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Scroll { dx: 0, dy: 200 },
        success: true,
        message: Some("Scrolled to success message".into()),
    })
    .unwrap();

    // ── Step 4: Snapshot → LLM response → Finish ──────────────────────────
    log.append(Event::Snapshot {
        url: "https://example.com/success".into(),
        snapshot_hash: "d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3".into(),
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
            completion_tokens: 35,
            total_tokens: 215,
        },
        finish_reason: "end_turn".into(),
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Finish {
            summary: "Successfully filled contact form and verified the success message".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Finish {
            summary: "Successfully filled contact form and verified the success message".into(),
        },
        success: true,
        message: Some("Task completed".into()),
    })
    .unwrap();

    // ── Task finished ─────────────────────────────────────────────────────
    log.append(Event::RunFinished {
        outcome: "success".into(),
        steps: 4,
        spend: BudgetSpend {
            tokens: 1622,
            usd: 0.048,
            steps: 4,
            started_at_utc: None,
        },
    })
    .unwrap();

    log
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn e2e_key_generation() {
    // Test 1: Key generation roundtrip.
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let pub_hex = signing::public_key_hex(&verifying_key);
    assert_eq!(pub_hex.len(), 64, "public key hex should be 64 chars");

    let recovered = signing::public_key_from_hex(&pub_hex).unwrap();
    assert_eq!(verifying_key, recovered, "public key hex roundtrip");

    // Sign and verify.
    let payload = b"e2e test payload";
    let sig = signing::sign(&signing_key, payload);
    signing::verify(&verifying_key, payload, &sig).unwrap();

    // Verification must reject a tampered payload.
    let tampered = b"tampered payload";
    assert!(
        signing::verify(&verifying_key, tampered, &sig).is_err(),
        "tampered payload must fail verification"
    );

    println!("[PASS] Key generation: sign/verify roundtrip ok");
    println!("[PASS] Key generation: tamper detection ok");
    println!("[PASS] Key generation: public key hex roundtrip ok");
}

#[test]
fn e2e_full_recording_pipeline() {
    // Test 2: Full recording pipeline — build log → record → save → load → verify.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let slug = "contact-form-submit";
    let description = "Fill contact form on example.com and verify success";

    // ── Record ────────────────────────────────────────────────────────────
    let log = build_realistic_event_log();
    let recorder = CassetteRecorder::new(slug, description);
    let (cassette, path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    assert_eq!(cassette.header.slug, slug);
    assert_eq!(cassette.header.version, "1.0.0");
    assert_eq!(
        cassette.header.goal,
        "Fill in the contact form on example.com and verify the success message"
    );
    assert_eq!(cassette.header.model, "claude-sonnet-4-20250514");
    assert_eq!(cassette.header.start_url, "https://example.com"); // The recorder produces one step per Snapshot+ActionResult pair.
    // The initial navigation ActionResult has no preceding Snapshot so
    // it is skipped, giving us 4 steps: Fill, Click, Scroll, Finish.
    assert_eq!(cassette.steps.len(), 4, "should have 4 steps");

    // Verify step structure.
    assert!(
        matches!(cassette.steps[0].action, Action::Fill { .. }),
        "step 0 should be Fill action"
    );
    assert!(
        matches!(cassette.steps[1].action, Action::Click { .. }),
        "step 1 should be Click action"
    );
    assert!(
        matches!(cassette.steps[2].action, Action::Scroll { .. }),
        "step 2 should be Scroll action"
    );
    assert!(
        matches!(cassette.steps[3].action, Action::Finish { .. }),
        "step 3 should be Finish action"
    );

    assert_eq!(cassette.header.step_count, 4);
    assert_eq!(
        cassette.header.signer_public_key,
        signing::public_key_hex(&verifying_key)
    );

    // ── File on disk ──────────────────────────────────────────────────────
    assert!(path.exists(), "cassette file must exist on disk");
    assert!(
        path.extension().unwrap() == "cassette",
        "file must have .cassette extension"
    );
    assert!(
        path.to_string_lossy().contains(slug),
        "path must contain slug"
    );
    assert!(
        path.to_string_lossy().contains("versions"),
        "path must contain versions dir"
    );
    assert!(
        path.to_string_lossy().contains("1.0.0"),
        "path must contain version"
    );

    let file_size = fs::metadata(&path).unwrap().len();
    assert!(
        file_size < 50_000,
        "cassette file should be compact (< 50 KB), got {file_size} bytes"
    );
    println!(
        "[PASS] Cassette file on disk: {} ({} bytes)",
        path.display(),
        file_size
    );

    // ── Load and verify ───────────────────────────────────────────────────
    let loaded = storage::load(&path, &verifying_key).unwrap();
    assert_eq!(loaded.header.slug, cassette.header.slug);
    assert_eq!(loaded.header.version, cassette.header.version);
    assert_eq!(loaded.steps.len(), cassette.steps.len());
    assert_eq!(loaded.header.tip_hash, cassette.header.tip_hash);
    assert_eq!(loaded.header.domains, vec!["example.com"]);

    // Verify each step's structure.
    for (i, step) in loaded.steps.iter().enumerate() {
        assert_eq!(step.seq, i as u32, "step sequence must match index");
        assert!(step.action_success, "all steps must be successful");
        assert!(!step.hash.is_empty(), "hash must not be empty");
        if i > 0 {
            assert_eq!(
                step.parent_hash,
                loaded.steps[i - 1].hash,
                "step {i} must chain to step {}",
                i - 1
            );
        } else {
            assert!(
                step.parent_hash.is_empty(),
                "first step parent_hash must be empty"
            );
        }
    }

    println!("[PASS] Cassette load: signature valid ✓");
    println!("[PASS] Cassette load: hash chain valid ✓");
    println!(
        "[PASS] Cassette load: all {} steps verified",
        loaded.steps.len()
    );
}

#[test]
fn e2e_tamper_detection() {
    // Test 3: Tamper detection — flip a byte in the saved cassette and
    // verify that loading fails.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let log = build_realistic_event_log();

    let recorder = CassetteRecorder::new("tamper-test", "tamper detection test");
    let (_cassette, path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    // ── Tamper with the file ──────────────────────────────────────────────
    let mut raw = fs::read(&path).unwrap();
    // Flip a bit in the compressed payload area (not the signature).
    let payload_len = raw.len() - 64; // 64-byte signature at the end
    if payload_len > 0 {
        let tamper_pos = payload_len / 2;
        raw[tamper_pos] ^= 0xAA;
    }
    fs::write(&path, &raw).unwrap();

    // ── Load must fail ────────────────────────────────────────────────────
    let result = storage::load(&path, &verifying_key);
    assert!(result.is_err(), "loading a tampered cassette must fail");
    let err = result.unwrap_err();
    let err_str = format!("{err}");
    assert!(
        err_str.contains("signature invalid"),
        "error must mention signature, got: {err_str}"
    );

    println!("[PASS] Tamper detection: corrupted payload rejected ✓");
    println!("[PASS] Tamper detection: error message confirms signature failure ✓");
}

#[test]
fn e2e_list_slugs_and_versions() {
    // Test 4: Slug and version listing.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);
    let log = build_realistic_event_log();

    // Record first slug.
    let recorder1 = CassetteRecorder::new("slug-alpha", "first slug");
    recorder1
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    // Record second slug.
    let recorder2 = CassetteRecorder::new("slug-beta", "second slug");
    recorder2
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    // Record a second version of slug-alpha by manually creating one.
    let log_v2 = {
        let mut log = EventLog::in_memory();
        log.append(Event::RunStarted {
            task_id: "v2".into(),
            goal: "Updated goal for slug-alpha v2".into(),
        })
        .unwrap();
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
            message: None,
        })
        .unwrap();
        log.append(Event::Snapshot {
            url: "https://example.com".into(),
            snapshot_hash: "snap_v2".into(),
            token_estimate: 100,
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Finish {
                summary: "done".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Finish {
                summary: "done".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();
        log.append(Event::RunFinished {
            outcome: "success".into(),
            steps: 1,
            spend: BudgetSpend::default(),
        })
        .unwrap();
        log
    };

    let recorder_v2 = CassetteRecorder::new("slug-alpha", "second version");
    let mut cassette_v2 = recorder_v2.record(&log_v2).unwrap();
    cassette_v2.header.version = "1.1.0".into();
    cassette_v2.header.signer_public_key = signing::public_key_hex(&signing_key.verifying_key());
    let path_v2 = storage::save(&cassette_v2, &root, &signing_key).unwrap();
    assert!(path_v2.exists());

    // ── List slugs ────────────────────────────────────────────────────────
    let slugs = storage::list_slugs(&root).unwrap();
    assert!(
        slugs.contains(&"slug-alpha".to_string()),
        "must contain slug-alpha"
    );
    assert!(
        slugs.contains(&"slug-beta".to_string()),
        "must contain slug-beta"
    );
    assert_eq!(slugs.len(), 2, "must have exactly 2 slugs");

    // ── List versions ─────────────────────────────────────────────────────
    let versions_alpha = storage::list_versions(&root, "slug-alpha").unwrap();
    assert_eq!(
        versions_alpha,
        vec!["1.0.0", "1.1.0"],
        "slug-alpha must have 2 versions in order"
    );

    let versions_beta = storage::list_versions(&root, "slug-beta").unwrap();
    assert_eq!(
        versions_beta,
        vec!["1.0.0"],
        "slug-beta must have 1 version"
    );

    // ── Latest version path ───────────────────────────────────────────────
    let latest = storage::latest_version_path(&root, "slug-alpha").unwrap();
    assert!(
        latest.to_string_lossy().ends_with("1.1.0.cassette"),
        "latest must be 1.1.0, got: {}",
        latest.display()
    );

    // ── Non-existent slug ─────────────────────────────────────────────────
    let err = storage::list_versions(&root, "nonexistent").unwrap_err();
    let err_str = format!("{err}");
    assert!(
        err_str.contains("no cassettes for slug"),
        "must return slug not found error"
    );

    // ── Empty directory ───────────────────────────────────────────────────
    let empty_root = tmp.path().join("empty_cassettes");
    let empty_slugs = storage::list_slugs(&empty_root).unwrap();
    assert!(empty_slugs.is_empty(), "empty dir must return no slugs");

    println!("[PASS] Slug listing: found {} slugs", slugs.len());
    println!(
        "[PASS] Version listing: slug-alpha has {:?}",
        versions_alpha
    );
    println!("[PASS] Version listing: slug-beta has {:?}", versions_beta);
    println!("[PASS] Latest version resolution: {}", latest.display());
    println!("[PASS] Error handling: non-existent slug properly rejected");
    println!("[PASS] Edge case: empty directory handled gracefully");
}

#[test]
fn e2e_budget_spend_tracking() {
    // Test 5: Verify budget spend is correctly tracked in the cassette.
    let log = build_realistic_event_log();
    let recorder = CassetteRecorder::new("budget-test", "Budget spend tracking");

    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);

    let (cassette, _path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    let spend = &cassette.header.spend;
    assert_eq!(spend.tokens, 1622);
    assert!((spend.usd - 0.048).abs() < f64::EPSILON);
    assert_eq!(spend.steps, 4);

    println!(
        "[PASS] Budget spend: tokens={}, usd=${}, steps={}",
        spend.tokens, spend.usd, spend.steps
    );
}

#[test]
fn e2e_hash_chain_integrity() {
    // Test 6: Verify the hash chain is correctly computed and verified.
    let log = build_realistic_event_log();
    let recorder = CassetteRecorder::new("hash-chain-test", "Hash chain integrity");

    let cassette = recorder.record(&log).unwrap();

    // The hash chain must verify.
    cassette.verify_hash_chain().unwrap();

    // Verify each step hash is computed correctly.
    let mut prev_hash = String::new();
    for (i, step) in cassette.steps.iter().enumerate() {
        let payload = step_payload_for_hash(step);
        let expected_hash = compute_step_hash(&prev_hash, &payload);
        assert_eq!(step.hash, expected_hash, "step {i} hash mismatch");
        assert_eq!(step.parent_hash, prev_hash, "step {i} parent_hash mismatch");
        prev_hash = step.hash.clone();
    }

    // Verify tip hash in header matches the last step's hash.
    assert_eq!(
        cassette.header.tip_hash,
        cassette.steps.last().unwrap().hash,
        "header tip_hash must match last step hash"
    );

    println!(
        "[PASS] Hash chain: all {} steps linked correctly",
        cassette.steps.len()
    );
    println!("[PASS] Hash chain: tip_hash matches header");
}

#[test]
fn e2e_serialization_deserialization_roundtrip() {
    // Test 7: JSON serde roundtrip for the full Cassette struct.
    let log = build_realistic_event_log();
    let recorder = CassetteRecorder::new("serde-test", "Serde roundtrip");

    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);

    let (cassette, _path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    // JSON roundtrip.
    let json = serde_json::to_string_pretty(&cassette).unwrap();
    assert!(json.contains("\"slug\": \"serde-test\""));
    assert!(json.contains("\"version\": \"1.0.0\""));
    assert!(json.contains("\"step_count\": 4"));
    assert!(json.contains("\"action\":"));

    // Verify JSON is valid and parseable.
    let _deserialized: genatp_cassette::Cassette = serde_json::from_str(&json).unwrap();

    println!("[PASS] Serialization: JSON roundtrip valid ✓");
    println!("[PASS] JSON size: {} bytes", json.len());
}

#[test]
fn e2e_domains_extraction() {
    // Test 8: Verify domain extraction from the event log.
    let log = build_realistic_event_log();
    let recorder = CassetteRecorder::new("domains-test", "Domain extraction");

    let cassette = recorder.record(&log).unwrap();

    assert!(
        cassette.header.domains.contains(&"example.com".to_string()),
        "must extract example.com from the log"
    );
    assert_eq!(
        cassette.header.domains.len(),
        1,
        "should have exactly 1 unique domain"
    );

    println!("[PASS] Domain extraction: {:?}", cassette.header.domains);
}

#[test]
fn e2e_cassette_file_format_integrity() {
    // Test 9: Verify the on-disk file format matches the spec:
    //   zstd(canonical_json(Cassette)) + 64-byte ed25519 sig
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let log = build_realistic_event_log();

    let recorder = CassetteRecorder::new("format-test", "File format integrity");
    let (_cassette, path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    let raw = fs::read(&path).unwrap();

    // Must be at least 64 bytes (the signature) + some payload.
    assert!(raw.len() > 64, "file must be > 64 bytes");

    // The last 64 bytes must be the signature.
    let (compressed, sig_bytes) = raw.split_at(raw.len() - 64);
    assert_eq!(sig_bytes.len(), 64, "signature must be 64 bytes");

    // Verify the signature.
    let sig = ed25519_dalek::Signature::from_slice(sig_bytes).unwrap();
    signing::verify(&verifying_key, compressed, &sig).unwrap();

    // Decompress and verify it's valid JSON.
    let decompressed = zstd::decode_all(compressed).unwrap();
    let _cassette_from_file: genatp_cassette::Cassette =
        serde_json::from_slice(&decompressed).unwrap();

    // Check compression ratio.
    let json_len = decompressed.len();
    let compressed_len = compressed.len();
    let ratio = json_len as f64 / compressed_len as f64;
    assert!(
        ratio > 1.5,
        "compression ratio should be > 1.5x, got {ratio:.1}x"
    );

    println!("[PASS] File format: zstd(canonical_json) + 64-byte ed25519 sig ✓");
    println!(
        "[PASS] File format: JSON size={}B, compressed={}B, ratio={:.1}x",
        json_len, compressed_len, ratio
    );
    println!("[PASS] File format: total file size = {} bytes", raw.len());
}

#[test]
fn e2e_multiple_domains_extraction() {
    // Test 10: Verify extraction of multiple domains from different URLs.
    let mut log = EventLog::in_memory();

    log.append(Event::RunStarted {
        task_id: "multi-domain".into(),
        goal: "Multi-domain test".into(),
    })
    .unwrap();

    // Navigate to domain 1.
    log.append(Event::ActionRequested {
        action: Action::Navigate {
            url: "https://github.com".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Navigate {
            url: "https://github.com".into(),
        },
        success: true,
        message: None,
    })
    .unwrap();

    // Step on domain 1.
    log.append(Event::Snapshot {
        url: "https://github.com/login".into(),
        snapshot_hash: "hash1".into(),
        token_estimate: 100,
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Click {
            r#ref: "@e1".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Click {
            r#ref: "@e1".into(),
        },
        success: true,
        message: None,
    })
    .unwrap();

    // Navigate to domain 2.
    log.append(Event::ActionRequested {
        action: Action::Navigate {
            url: "https://docs.github.com".into(),
        },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Navigate {
            url: "https://docs.github.com".into(),
        },
        success: true,
        message: None,
    })
    .unwrap();

    // Step on domain 2.
    log.append(Event::Snapshot {
        url: "https://docs.github.com/en".into(),
        snapshot_hash: "hash2".into(),
        token_estimate: 100,
    })
    .unwrap();
    log.append(Event::ActionRequested {
        action: Action::Scroll { dx: 0, dy: 100 },
    })
    .unwrap();
    log.append(Event::ActionResult {
        action: Action::Scroll { dx: 0, dy: 100 },
        success: true,
        message: None,
    })
    .unwrap();

    log.append(Event::RunFinished {
        outcome: "success".into(),
        steps: 2,
        spend: BudgetSpend::default(),
    })
    .unwrap();

    let recorder = CassetteRecorder::new("multi-domain", "Multi-domain test");
    let cassette = recorder.record(&log).unwrap();

    let domains = &cassette.header.domains;
    assert!(domains.contains(&"github.com".to_string()));
    assert!(domains.contains(&"docs.github.com".to_string()));
    assert_eq!(domains.len(), 2);

    println!("[PASS] Multi-domain extraction: {:?}", domains);
}

#[test]
fn e2e_wrong_key_rejected() {
    // Test 11: Loading a cassette with the wrong key must fail.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("cassettes");
    let signing_key = SigningKey::generate(&mut OsRng);
    let wrong_key = SigningKey::generate(&mut OsRng);
    let log = build_realistic_event_log();

    let recorder = CassetteRecorder::new("wrong-key-test", "wrong key rejection");
    let (_cassette, path) = recorder
        .record_and_save_to(&log, &root, &signing_key)
        .unwrap();

    let result = storage::load(&path, &wrong_key.verifying_key());
    assert!(result.is_err(), "wrong key must fail verification");
    let err = result.unwrap_err();
    let err_str = format!("{err}");
    assert!(
        err_str.contains("signature invalid"),
        "error must mention signature, got: {err_str}"
    );

    println!("[PASS] Wrong key rejection: correctly rejected ✓");
}

#[test]
fn e2e_goal_start_url_metadata() {
    // Test 12: Verify goal, start_url, and model are correctly extracted.
    let log = {
        let mut l = EventLog::in_memory();
        l.append(Event::RunStarted {
            task_id: "meta-test".into(),
            goal: "Navigate to example.com and click the button".into(),
        })
        .unwrap();
        l.append(Event::ActionRequested {
            action: Action::Navigate {
                url: "https://example.com/test".into(),
            },
        })
        .unwrap();
        l.append(Event::ActionResult {
            action: Action::Navigate {
                url: "https://example.com/test".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();
        l.append(Event::Snapshot {
            url: "https://example.com/test".into(),
            snapshot_hash: "snap1".into(),
            token_estimate: 100,
        })
        .unwrap();
        l.append(Event::LlmRequest {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            prompt_messages: 2,
        })
        .unwrap();
        l.append(Event::LlmResponse {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            usage: Usage {
                prompt_tokens: 50,
                completion_tokens: 10,
                total_tokens: 60,
            },
            finish_reason: "tool_calls".into(),
        })
        .unwrap();
        l.append(Event::ActionRequested {
            action: Action::Click {
                r#ref: "@e3".into(),
            },
        })
        .unwrap();
        l.append(Event::ActionResult {
            action: Action::Click {
                r#ref: "@e3".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();
        l.append(Event::RunFinished {
            outcome: "success".into(),
            steps: 1,
            spend: BudgetSpend::default(),
        })
        .unwrap();
        l
    };

    let recorder = CassetteRecorder::new("meta-test", "Metadata extraction");
    let cassette = recorder.record(&log).unwrap();

    assert_eq!(
        cassette.header.goal,
        "Navigate to example.com and click the button"
    );
    assert_eq!(cassette.header.start_url, "https://example.com/test");
    assert_eq!(cassette.header.model, "gpt-4o");

    println!(
        "[PASS] Metadata extraction: goal='{}', start_url='{}', model='{}'",
        cassette.header.goal, cassette.header.start_url, cassette.header.model
    );
}

#[test]
fn e2e_empty_log_rejected() {
    // Test 13: Empty event log must be rejected.
    let log = EventLog::in_memory();
    let recorder = CassetteRecorder::new("empty-test", "should fail");
    let err = recorder.record(&log).unwrap_err();
    let err_str = format!("{err}");
    assert!(
        err_str.contains("empty"),
        "must mention empty, got: {err_str}"
    );

    println!("[PASS] Empty log rejection: correctly rejected with message ✓");
}

#[test]
fn e2e_failed_run_rejected() {
    // Test 14: Failed run must be rejected.
    let mut log = EventLog::in_memory();
    log.append(Event::RunStarted {
        task_id: "fail".into(),
        goal: "will fail".into(),
    })
    .unwrap();
    log.append(Event::RunFinished {
        outcome: "aborted".into(),
        steps: 0,
        spend: BudgetSpend::default(),
    })
    .unwrap();

    let recorder = CassetteRecorder::new("fail-test", "should fail");
    let err = recorder.record(&log).unwrap_err();
    let err_str = format!("{err}");
    assert!(
        err_str.contains("not `success`"),
        "must mention not success, got: {err_str}"
    );

    println!("[PASS] Failed run rejection: correctly rejected with message ✓");
}
