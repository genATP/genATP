# Changelog

All notable changes to **genATP** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

See [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) for the full phase roadmap and
[`MIGRATION.md`](./MIGRATION.md) for upgrade guides between releases.

---

## [1.0.0] — 2026-06-22 — General Availability

### Added

- **Cassette Registry (Phase 14)** — OCI Distribution v1.1–inspired cassette registry:
  - `genatp registry search`, `install`, `publish`, `info`, `publishers`, `serve`
  - Trust ladder: unsigned blocked, self-signed warns, verified auto-runs
  - Semver resolution (`^`, `~`, exact, `latest`, prerelease)
  - SQLite-backed storage with transaction-protected publish
  - REST API (`/v2/`, `/v2/{slug}/tags/list`, `/v2/{slug}/manifests/{version}`, `/search`, `/publishers`, `/health`)
  - ADR-004 documenting OCI deviations and design decisions
  - 22 unit tests

- **Time-travel Debugger (Phase 13)** — Frame-by-frame replay of any past run:
  - Event log reader with BLAKE3 hash chain verification
  - REST API: `GET /runs`, `/runs/:id`, `/runs/:id/timeline`, `/runs/:id/steps/:seq`, `/runs/:id/steps/:seq/snapshot`, `/runs/:id/diff`
  - Web UI (React + Vite): timeline scrubber, diff viewer, snapshot browser
  - CLI: `genatp replay <run-id>`, `genatp replay <run-id> --at N`, `genatp replay --serve`
  - Replay engine with `--at N` step targeting
  - Thread-safe SnapshotStore (Arc<Mutex<>> for SQLite)
  - 12 unit tests (debugger) + 18 CLI tests

- **Full v1.0.0 release**: GitHub release page with binaries for 5 platforms, SBOM, and release notes

### Changed

- **Stealth engine (Phase 11)** — All 9 built-in profiles now have unique locale+timezone combinations
- **Workspace version** updated from `0.7.1` to `1.0.0`
- **README.md** updated for v1.0 GA (status, roadmap, version references)
- **Testing guides** updated with Phase 13–14 coverage

### Fixed

- Pre-existing stealth profile duplicate (`chrome-128-win-server` had `en-US|America/New_York` same as `chrome-128-win11`; changed to `America/Denver`)
- All 0 clippy warnings across workspace

---

## [0.3.0] — 2026-06-05 — Auth Vault

### Added

- Encrypted auth vault (AES-256-GCM, OS keychain–wrapped KEK)
- Per-domain data encryption keys via HKDF-SHA256
- Capability-gated vault access (declared domains only)
- MCP tools: `save_auth`, `load_auth`, `list_auth`
- CLI: `genatp vault list`, `genatp vault revoke <domain>`
- Security tests: ciphertext tamper detection, vault file plaintext check, wrong-domain DEK isolation
- OS keychain integration tests (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- Property tests: DEK non-interference across domains
- Vault unlock benchmark (< 100 ms p95)

---

## [0.2.0] — 2026-05-29 — Python SDK

### Added

- PyO3 native bindings via maturin
- `genatp` Python package on PyPI
- `Agent` class with sync + async API
- Type stubs (`.pyi`) for full type safety
- LangChain adapter (`genatp.langchain`)
- LlamaIndex adapter (`genatp.llamaindex`)
- mkdocs documentation site
- Python 3.10–3.13 support across 5 platforms
- PRD 5-line quickstart verified on all targets

---

## [0.1.0] — 2026-07-27 — First Public Release

### Added

- MCP server (stdio + HTTP) with 9 browser tools: `navigate`, `snapshot`, `click`, `fill`, `extract`, `scroll`, `screenshot`, `get_url`, `go_back`
- Token-optimized perception pipeline: a11y stripping (50k→12k tokens), ref compression (12k→6k), diff-only updates (400–800 tokens/step)
- LLM orchestrator with plan loop, action dispatch, and budget guardrails (token, time, USD)
- Retry policy with exponential backoff
- Event log (append-only, BLAKE3 hash-chained)
- Provider abstraction (`genatp-llm` trait): Anthropic, OpenAI, Google, NVIDIA NIM, Ollama
- VCR fixture recording for deterministic CI testing
- CDP-native browser substrate (chromiumoxide): launch, navigate, a11y tree, cookies, storage
- BrowserPool with acquire/release/max-concurrency
- Stealth defaults: `navigator.webdriver` removal, WebGL spoofing, plugin emulation
- CLI: `genatp serve`, `genatp version`
- CI/CD: 5-target build matrix, lint/test/coverage gates, release workflow
- Cargo workspace with 11 Rust crates
- Architecture: ADR-001 (Rust + CDP over Playwright)

---

## [0.0.0] — 2026-06-01 — Project Foundation

### Added

- Initial Cargo workspace with all crate stubs
- `rust-toolchain.toml` (MSRV 1.95)
- `deny.toml` (license, ban, advisory policy)
- GitHub Actions: build + test matrix (5 targets), clippy, fmt, cargo-deny
- Pre-commit hooks (rustfmt, clippy, taplo fmt, cargo-deny)
- Fixture server skeleton (`fixtures/sites/hello.html`)
- ADR template (`docs/adr/0000-template.md`)
- `CONTRIBUTING.md`, `SECURITY.md`, `LICENSE`

[1.0.0]: https://github.com/genATP/genATP/releases/tag/v1.0.0
[0.3.0]: https://github.com/genATP/genATP/releases/tag/v0.3.0
[0.2.0]: https://github.com/genATP/genATP/releases/tag/v0.2.0
[0.1.0]: https://github.com/genATP/genATP/releases/tag/v0.1.0
[0.0.0]: https://github.com/genATP/genATP/releases/tag/v0.0.0
