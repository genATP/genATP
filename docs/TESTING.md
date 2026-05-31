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

## 7. MCP-level smoke test

Spin up the stdio server, hit the new tools manually:

```powershell
cargo run --release --bin genatp -- serve --stdio
```

In another shell, send a JSON-RPC `initialize` then a `tools/list`
and confirm the response includes `save_auth`, `load_auth`,
`list_auth`. The MCP integration tests cover the full round-trip:

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

## 10. Workspace-wide regression

Last gate before tagging:

```bash
cargo test --workspace --all-targets
```

Expected: every crate green; only the OS keychain tests skipped.

[development plan]: ./DEVELOPMENT_PLAN.md
