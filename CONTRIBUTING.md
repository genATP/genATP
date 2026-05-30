# Contributing to genATP

Thank you for your interest in contributing. genATP is **source-available, not open-source** — please read [`LICENSE`](./LICENSE) before contributing. By submitting a contribution you agree your work is licensed under the same terms (see "Inbound = Outbound" below).

---

## Quick start

```bash
# 1. Fork + clone
git clone https://github.com/<you>/genatp.git
cd genatp

# 2. Install pinned Rust toolchain (auto-picks up rust-toolchain.toml)
rustup show

# 3. Install pre-commit hooks (< 2 min on a fresh clone)
pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push

# 4. Verify the workspace builds and tests cleanly
cargo build --workspace --exclude genatp-python
cargo test --workspace --exclude genatp-python
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

### Python SDK (`bindings/python/`)

```bash
# Install maturin (once): cargo install maturin --locked
cd bindings/python
maturin develop --release          # builds genatp._native + installs editable wheel
python -m pytest tests/ -v       # pure-python + native smoke tests
python examples/mock_smoke.py      # end-to-end with mock provider (needs Chrome)
```

Set `GENATP_PY_LIVE=1` and a provider API key to run live integration tests.
See [`bindings/python/README.md`](./bindings/python/README.md).

If any of those fail on a fresh clone of `main`, that is a P0 bug. Open an issue immediately.

---

## How we work

### 1. Issues first

For anything beyond a typo fix, **open an issue first**. Discussions are cheaper than rebases.

### 2. Branch + PR

- Branch from `main`. Branch name: `<kind>/<short-slug>` (e.g. `feat/cassette-record`, `fix/vault-keychain-macos`).
- Keep PRs **small and focused**. One concern per PR. Easier review = faster merge.
- All PRs must pass the [CI](./.github/workflows/ci.yml) gate (lint + 5-target test + coverage). Branch protection enforces this.

### 3. Commit style

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(cassette): add zstd dictionary training for snapshot diffs
fix(vault): handle macOS keychain ENOENT on first run
docs(adr): record decision to drop Playwright (#42)
test(perception): expand a11y tree golden coverage
```

Allowed types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`, `build`, `ci`, `revert`.

Each commit must compile + pass tests on its own (we may rebase to enforce this).

### 4. Tests are non-negotiable

The dev plan codifies the testing taxonomy. Every PR must:

- Add or update tests for behavioural changes (unit / property / integration / e2e as appropriate).
- Not regress workspace coverage below the codecov ratchet (see [`codecov.yml`](./codecov.yml)).
- Pass `cargo clippy --workspace -- -D warnings` (we treat clippy warnings as errors).

If you're adding code to a Phase that has a higher bar (e.g. `genatp-vault` requires ≥ 80% mutation kill rate), the bar applies to your PR too.

### 5. Architecture changes need an ADR

Any change to:

- Public API surfaces (CLI, MCP tool list, SDK function signatures, ATP Spec schema)
- Cassette wire format
- Storage layout (vault, snapshot CAS, registry layers)
- Cross-crate dependency direction

…requires an [ADR](./docs/adr/0000-template.md) merged before or alongside the implementation. Number it sequentially.

---

## Local checks (cheat sheet)

**Run this before every commit** (matches CI + pre-commit):

```powershell
# Windows
.\scripts\preflight.ps1

# macOS / Linux / Git Bash
./scripts/preflight.sh
```

Add `-Quick` to skip the full test matrix when Chrome is not installed locally.

| Command | Why |
|---|---|
| `.\scripts\preflight.ps1` | **Recommended** — fmt, clippy, test, deny in one shot |
| `cargo fmt --all` | Format (pre-commit also applies fmt on commit) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint. Same gate CI runs. |
| `cargo test --workspace` | Unit + integration tests. |
| `cargo deny check` | License + security advisories + bans. |
| `cargo audit` | Known CVEs in dependency graph (CI lint job). |
| `cargo llvm-cov --workspace --html` | Local coverage report. |
| `cargo bench --workspace` | Local benchmark run (Phase 1+). |

Run `pre-commit run --all-files` if you want to dry-run hooks without committing.

---

## What we will / will not accept

**Will accept:**

- Bug fixes with a regression test.
- Performance improvements with a Criterion bench showing the win.
- New cassette tools / ATP-spec features that have an issue + ADR.
- Documentation improvements.
- Test coverage for under-tested code paths.

**Will not accept (without prior discussion):**

- Adding new top-level dependencies (especially anything with native build steps). Justify in the issue.
- Replacing the CDP substrate, vault crypto, or cassette format.
- Changes that lower the lint/coverage/MSRV bar.
- AI-generated code that you have not reviewed and tested yourself.

---

## Inbound = Outbound

By submitting a pull request, issue, or any other contribution, you agree that:

1. You have the right to contribute the work.
2. Your contribution is licensed under the [genATP Personal Use License](./LICENSE), the same license that covers the project. genATP can re-license your contribution under a commercial license at our discretion.
3. The contribution is provided **as-is**, with no warranty.

If your employer owns rights to your work, you must obtain their permission before contributing.

---

## Security

Found a vulnerability? **Do not open a public issue.** See [`SECURITY.md`](./SECURITY.md).

---

## Code of conduct

Be kind. Assume good faith. Disagree on the technical merits, not the person. Maintainers may close discussions that drift from this — this is a working repo, not a forum.

---

## Questions

Open a discussion or, for substantive design questions, open an issue with the `discussion` label.
