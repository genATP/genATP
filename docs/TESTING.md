# genATP — Phase 6 Testing Guide

This guide walks through every test for the **Auth Vault** (Phase 6 /
v0.3.0). Run it top-to-bottom before tagging a release.

> All commands assume a Bash-or-PowerShell shell at the repo root.

## 0. Prerequisites

| Tool | Why |
| --- | --- |
| `cargo` (1.95+) | Workspace builds |
| Chrome / Chromium | End-to-end browser tests (Phase 1+) |
| Working OS keychain | Real-keychain integration tests (`--ignored`) |
|   - macOS: Keychain Access app | |
|   - Windows: Credential Manager | |
|   - Linux: GNOME Keyring or KWallet *unlocked* | |

Optional but recommended:

```bash
rustup component add clippy rustfmt
cargo install cargo-deny cargo-audit
```

## 1. Static gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Expected: all green. `cargo deny` warnings on `webpki-roots`'s
`CDLA-Permissive-2.0` are pre-allow-listed in `deny.toml`.

## 2. Unit tests

```bash
cargo test -p genatp-vault --lib
```

Expected: **36 passed; 0 failed**. These cover

- AES-GCM seal/open round-trip
- HKDF determinism (same inputs → same DEK)
- AES tag rejection on tampered ciphertext / nonce / AAD
- KEK never appears in `Debug` output
- Schema migration idempotence
- Domain canonicalisation rules
- Capability allow/deny logic
- `Vault::save` upserts, `revoke` cascades, capability gating

## 3. Property tests

```bash
cargo test -p genatp-vault --test property
```

Expected: **3 passed**. proptest fans out to 64 cases each:

- Wrong-domain DEK never decrypts
- AAD swap (sub_key A → sub_key B) never decrypts
- Same-domain DEK is deterministic across re-derivation

## 4. Security tests

```bash
cargo test -p genatp-vault --test security
```

Expected: **4 passed**. Pins the v0.3.0 acceptance criteria:

- `vault_file_contains_no_plaintext_after_save` — no
  user-supplied bytes in the SQLite file or the WAL/SHM siblings.
- `opening_vault_without_keychain_entry_returns_not_initialized` —
  the actionable error variant, not a generic "decrypt failed".
- `tampered_ciphertext_rejects_load` — bit-flip in the on-disk
  ciphertext makes `Vault::load` return `VaultError::Decrypt`.
- `vault_id_differs_for_distinct_paths` — two vaults at distinct
  paths use distinct keychain entries.

## 5. OS keychain integration (per-OS CI / local opt-in)

Default `cargo test` skips these; run explicitly:

```bash
cargo test -p genatp-vault --test os_keychain -- --ignored
```

Expected: **3 passed** (run on each platform once before release).

- macOS: a Keychain prompt may appear the first time. Choose
  *"Always Allow"*.
- Windows: silent.
- Linux: requires GNOME Keyring / KWallet to be **unlocked** in the
  current session — headless servers will fail with
  `VaultError::Keychain`.

## 6. Benchmark — unlock latency gate

```bash
cargo bench -p genatp-vault --bench vault_unlock
```

Expected: `vault_open_warm  time:  [< 100 ms]` median, **p95
under 100 ms** per the Phase 6 acceptance matrix.

Reference numbers (Windows 11 dev laptop, NVMe SSD):

```
vault_open_warm         time:   [2.0 ms 2.1 ms 2.2 ms]
```

## 7. MCP-level smoke test (HTTP, no MCP client)

Use this on Windows when you do not have Claude Desktop / Cursor wired
up. **Headed Chrome only appears after `navigate`.**

**Terminal A** — start server (rebuild after vault changes):

```powershell
cargo build --release -p genatp-cli
cargo run --release --bin genatp -- serve --port 3000 --headed
```

**Terminal B** — optional: load `.env` (genATP does not auto-read it):

```powershell
# One-liner to load .env into this PowerShell session:
Get-Content .env | ForEach-Object {
  if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
    Set-Item -Path "env:$($matches[1])" -Value $matches[2].Trim().Trim('"')
  }
}
# Or use the script flag: .\scripts\test-auth-vault-http.ps1 -LoadEnv
```

**Automated HTTP script** (login manually when prompted):

```powershell
.\scripts\test-auth-vault-http.ps1 -LoadEnv
# After login + save, restart Terminal A, then:
.\scripts\test-auth-vault-http.ps1 -LoadOnly -LoadEnv
```

**Manual JSON-RPC** — same as the script; see
`scripts/test-auth-vault-http.ps1` for the exact `Invoke-RestMethod`
bodies.

Confirm tools: `save_auth`, `load_auth`, `list_auth` in `tools/list`.
Integration tests:

```bash
cargo test -p genatp-mcp --tests
```

## 8. CLI smoke test

```bash
cargo run --bin genatp -- vault path
cargo run --bin genatp -- vault list
```

Expected: `path` prints the resolved location (creates the parent
dir if missing); `list` prints `vault not initialised yet…` on a
fresh box and a table with `domain  count  updated` columns once
the agent has saved at least one entry.

## 9. End-to-end acceptance

This is the human-in-the-loop test from the [development plan]:

1. Start `genatp serve --headed`.
2. Drive an agent (Claude Desktop, Cursor, MCP Inspector) to:
   1. `navigate` to a real auth site you control (a fixture app, or
      your own GitHub OAuth flow).
   2. Log in interactively.
   3. Call `save_auth { domain: "<your-domain>" }`.
3. Kill the `genatp` process.
4. Restart `genatp serve --headed`.
5. Drive the agent to:
   1. `navigate` back to the same URL.
   2. Call `load_auth { domain: "<your-domain>" }`.
   3. Confirm the agent is **already authenticated**, with no
      additional prompts.
6. Run `genatp vault revoke <domain>`.
7. Repeat step 4 — confirm the agent now prompts again.

Pass criteria:

- Login persists across restarts (step 5 succeeds without prompting).
- Revocation forces a re-login (step 7 surfaces the prompt again).
- The on-disk vault file does not contain the cookie value in
  plaintext (`grep -aF $COOKIE_VALUE vault.sqlite` returns nothing).

## 10. Phase 13 — Time-Travel Debugger

### 10.1 Debugger unit tests

```bash
cargo test -p genatp-debugger
```

Expected: **12 passed; 0 failed**.

- `event_log_invalid_path` — non-existent directory returns error
- `event_log_empty_directory` — directory with no `.jsonl` files returns error
- `event_log_read_entries` — reads a log and returns correct entry count
- `event_log_timeline_order` — entries are sorted by seq in the timeline
- `event_log_run_summary` — `RunSummary` has correct metadata
- `event_log_hash_chain_invalid` — tampered hash chain detected
- `event_log_hash_chain_valid` — valid hash chain passes
- `snapshot_resolution` — finds correct snapshot at a given step
- `replay_to_step_basic` — replay returns correct summary
- `replay_to_step_at_n` — `--at N` returns correct step
- `replay_to_step_with_snapshot` — snapshot resolution works
- `replay_past_end` — requesting step beyond end returns last step

### 10.2 Event log integration

```bash
cargo test -p genatp-core -- --test event_log
```

Expected: EventLog core tests pass (BLAKE3 hash chaining, JSON-Lines
persistence, round-trip serialization).

### 10.3 CLI replay smoke test

```bash
# Show run summary (no event logs on fresh system — expected graceful message)
cargo run --release -p genatp-cli -- replay non-existent-run
# Expected: "runs directory not found at ..."

# Start the debugger API server
cargo run --release -p genatp-cli -- replay --serve --port 3001
# Expected: "Starting debugger API server at http://127.0.0.1:3001"
```

### 10.4 API server health check (in a second terminal)

```powershell
# Health check
Invoke-RestMethod -Uri http://127.0.0.1:3001/health
# Expected: {"status":"ok"}
```

### 10.5 Web UI (optional, requires Node.js)

```bash
cd crates/genatp-debugger/ui
npm install
npm run dev
```

Open `http://localhost:5173` in a browser while the API server on
port 3001 is running. The UI shows the timeline scrubber and
snapshot diff viewer.

### 10.6 Acceptance criteria

1. ✅ Any past run reproducible step-by-step from event log + CAS
2. ✅ CLI `genatp replay <run_id> --at N` returns state for any step
3. ✅ Timeline loads events, diff viewer highlights snapshot changes
4. ✅ Hash chain integrity verified with detection of tampered entries

---

## 11. Phase 14 — Cassette Registry (OCI Distribution v1.1)

### 11.1 Registry unit tests

```bash
cargo test -p genatp-registry
```

Expected: **22 passed; 0 failed**.

**manifest.rs (11 tests):**
- `semver_parse_valid` — parses `"1.2.3"` correctly
- `semver_parse_prerelease` — parses `"1.2.3-beta.1"` correctly
- `semver_parse_invalid` — rejects empty, malformed strings
- `constraint_exact_match` — exact `"1.2.3"` matches only identical version
- `constraint_compatible` — `^1.2.3` matches 1.5.0 but not 2.0.0
- `constraint_approximate` — `~1.2.3` matches 1.2.9 but not 1.3.0
- `constraint_any` — `"latest"` matches all stable, excludes prerelease
- `resolve_versions` — picks highest matching (1.5.0 from ^1.0.0)
- `resolve_no_match` — returns None when no version matches
- `trust_level_behaviors` — Unsigned blocked, SelfSigned warns, Verified trusted
- `evaluate_trust_ladder` — verified publisher grants Verified trust
- `unsigned_manifest_blocked` — empty key → Unsigned

**storage.rs (6 tests):**
- `publish_and_get` — round-trips slug + version
- `list_versions` — lists all versions sorted
- `resolve_constraint` — resolves ^/~ constraints correctly
- `search_cassettes` — keyword search matches slug
- `publisher_management` — upsert + verify publisher
- `list_slugs` — distinct slugs from all cassettes
- `publisher_cooldown` — new publisher in 24h cooldown period

**client.rs (2 tests):**
- `publish_and_install_roundtrip` — publishes a cassette and returns manifest
- `search_works` — searches by keyword through storage layer

### 11.2 CLI registry smoke test

```bash
# Search (empty registry)
cargo run --release -p genatp-cli -- registry search "test"
# Expected: No cassettes found matching "test"

# Info (non-existent slug)
cargo run --release -p genatp-cli -- registry info non-existent
# Expected: cassette non-existent matching "latest" not found

# List publishers
cargo run --release -p genatp-cli -- registry publishers
# Expected: No registered publishers.

# Start the registry API server
cargo run --release -p genatp-cli -- registry serve --port 3002
# Expected: Starting registry API server at http://127.0.0.1:3002
```

### 11.3 Registry API server health check

```powershell
# In a second terminal while the server runs:
Invoke-RestMethod -Uri http://127.0.0.1:3002/health
# Expected: {"status":"ok"}

Invoke-RestMethod -Uri http://127.0.0.1:3002/v2/
# Expected: {"version":"1.1"}

Invoke-RestMethod -Uri http://127.0.0.1:3002/v2/test-cassette/tags/list
# Expected: {"name":"test-cassette","tags":[]}
```

### 11.4 CLI integration tests

```bash
cargo test -p genatp-cli
```

Expected: **18 passed; 0 failed**. Tests cover:
- Subcommand parsing for all CLI commands (serve, vault, record,
  cassette, atp, replay, registry)
- Flag combinations and validation

### 11.5 Full workspace check

```bash
cargo check --workspace --exclude genatp-python --exclude genatp-node
cargo clippy --workspace --exclude genatp-python --exclude genatp-node -- -D warnings
cargo fmt --all -- --check
```

Expected: Compilation clean, **zero clippy warnings**, zero formatting errors.

### 11.6 Known pre-existing issue

The `genatp-browser` integration test `all_profiles_have_unique_locale_timezone_combinations`
fails on the `chrome-128-win-server` profile which has a duplicate
`en-US|America/New_York` locale+timezone combo. This is a fixture data
issue, **not caused by Phase 13 or Phase 14**. All other 116 tests pass.

### 11.7 Acceptance criteria

1. ✅ `genatp registry search` finds cassettes by keyword
2. ✅ `genatp registry install` enforces trust ladder (unsigned blocked, self-signed warns)
3. ✅ `genatp registry publish` creates manifest + saves cassette
4. ✅ Semver resolution works (`^1.0.0` → `1.5.0`, `~1.0.0` → `1.0.0`)
5. ✅ OCI-compatible API at `/v2/{slug}/tags/list` and `/v2/{slug}/manifests/{version}`
6. ✅ SQLite-backed storage with transaction-protected publish
7. ✅ ADR-004 documents design decisions and deviations from strict OCI

---

## 12. Workspace-wide regression

Last gate before tagging — run the full workspace test suite:

```bash
cargo test --workspace --exclude genatp-python --exclude genatp-node 2>&1 | grep -E "^(test |test result)"
```

Expected: **~160 tests passed, 0 failed** (the exact count depends on
configuration). The only ignored tests are the OS keychain tests and
live browser probe tests (require `GENATP_STEALTH_LIVE=1`).

[development plan]: ./DEVELOPMENT_PLAN.md

## Real-world prompt testing

For end-user, prompt-only QA scenarios that mimic daily genATP usage, see [Real-World Prompt Testing Guide](./REAL_WORLD_PROMPT_TESTING_GUIDE.md).
