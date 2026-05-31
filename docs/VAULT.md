# genATP Auth Vault

> **Phase 6 / v0.3.0** — Log in once, stay logged in across restarts.

The Auth Vault is genATP's encrypted credential store. It lets an
agent persist cookies and `localStorage` for any domain it's been
granted, and rehydrate them into a fresh browser session without
re-prompting the user.

## TL;DR

| Concern | Choice |
| --- | --- |
| Cipher | AES-256-GCM, 96-bit random nonces, 16-byte tag |
| KDF | HKDF-SHA256 |
| Master KEK | 32 random bytes from `OsRng`, sealed by the OS keychain |
| Per-domain DEK | `HKDF(KEK, salt = b"genatp-vault-kdf-v1", info = b"genatp-vault/dek/<domain>")` |
| AAD | `<kind>:<domain>:<sub_key>` (binds ciphertexts to their row) |
| Storage | SQLite (single file, WAL mode, FK constraints) |
| Crate | [`genatp-vault`](../crates/genatp-vault/) |

## Threat model

- **In scope**: stolen / cloned vault file with no keychain access;
  process memory inspected after the vault handle is dropped (we
  zeroize); accidental log leakage (we redact).
- **Out of scope**: an attacker who already has an interactive shell
  on the user's account (they can ask the keychain themselves);
  ROP / debugger attacks against the live process; sub-domain
  hijacking that fools Chrome itself.

## Architecture

```
       ┌───────────────────────────┐    master key (KEK)
       │     OS keychain           │◀──────────────┐
       │  (Keychain / DPAPI / DBus)│               │
       └──────────────┬────────────┘               │
                      │  KeyStore::load            │
                      ▼                            │
              ┌─────────────────┐                  │
              │  HKDF-SHA256    │                  │
              │  domain-keyed   │ → per-domain DEK │
              └────────┬────────┘                  │
                       │                           │
                       ▼                           │
              ┌─────────────────┐                  │
              │ AES-256-GCM     │ ← AAD-bound      │
              │  seal / open    │   ciphertext     │
              └────────┬────────┘                  │
                       │                           │
                       ▼                           │
              ┌─────────────────┐                  │
              │   SQLite file   │ ─────────────────┘
              │   `vault.sqlite`│
              └─────────────────┘
```

## CLI

```bash
# Where will the vault live?
genatp vault path

# Inspect what's stored.
genatp vault list
# domain                                   count  updated
# github.com                                  4   2026-05-31 10:14:02 UTC
# stripe.com                                  3   2026-05-29 22:01:11 UTC

# Drop a domain (next agent run will re-prompt).
genatp vault revoke github.com
```

## MCP tools

The MCP server exposes three tools. Each operates against the
session-bound vault and respects the active capability grant.

| Tool | Use after | Returns |
| --- | --- | --- |
| `save_auth { domain }` | Successful interactive login | "saved N auth entries for example.com" |
| `load_auth { domain }` | Navigating to a same-origin URL | "hydrated N auth entries" / "no auth entries stored…" |
| `list_auth {}` | Anywhere | Tab-separated `domain\tN entries` lines |

A typical agent flow:

```
1. navigate https://github.com/login
2. load_auth github.com           # 0 the first time
3. (interactive login fills credentials & MFA)
4. save_auth github.com           # captures session cookies + LS
5. (next process) load_auth github.com  # zero prompts
```

## Capabilities

`Capability::for_domains([...])` is the runtime authorisation gate
above the vault. The CLI uses `Capability::any()` (you the human are
asking). Cassettes (Phase 7) and the orchestrator pass a tight grant
so a misbehaving LLM can't read auth for a domain it never declared.

## File location

Resolution order (first hit wins):

1. `$GENATP_VAULT_PATH` — full path including filename.
2. `$GENATP_HOME/vault.sqlite`
3. Platform default via `directories::ProjectDirs`:
   - macOS: `~/Library/Application Support/genatp/vault.sqlite`
   - Windows: `%APPDATA%\genatp\genatp\data\vault.sqlite`
   - Linux: `~/.local/share/genatp/vault.sqlite`

The vault file uses SQLite WAL mode, so you'll see two siblings
`vault.sqlite-wal` and `vault.sqlite-shm` while the process is open.

## Migrations

Schema version is tracked in `meta.vault_schema_version`. Pending
migrations apply transactionally on `Vault::open`. Future versions
**must** be additive: this binary refuses to open a file whose schema
version is greater than `SCHEMA_CURRENT`.

## Operational notes

- **Rotating the master key** is currently a "destroy + re-init"
  operation — `Vault::destroy()` removes the keychain entry and the
  SQLite file; opening again generates a fresh KEK. Online rotation
  ships in v0.4.0.
- **Multiple vaults** on one host are supported: the keychain entry
  is keyed off `blake3(absolute_path)`, so dev/test/staging vaults at
  different paths never collide.
- **Cross-platform** behaviour matches the [`keyring`] crate's
  defaults — on Linux, the user's session must have a working Secret
  Service (e.g. GNOME Keyring or KWallet). Headless Linux servers
  need an unlocked keyring or an alternative keystore.

## Testing matrix

| Layer | Where | Gate |
| --- | --- | --- |
| Unit | `crates/genatp-vault/src/**/tests` | All green |
| Property | `crates/genatp-vault/tests/property.rs` | proptest |
| Security | `crates/genatp-vault/tests/security.rs` | All green |
| OS keychain | `crates/genatp-vault/tests/os_keychain.rs` | `--ignored`, per-OS CI |
| Bench | `crates/genatp-vault/benches/vault_unlock.rs` | p95 < 100 ms |

See [TESTING.md](./TESTING.md) for the full Phase 6 testing guide
(commands + expected output).

[`keyring`]: https://docs.rs/keyring
