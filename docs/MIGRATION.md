# Migration Guide — v0.x → v1.0.0

This guide documents breaking changes and migration steps between genATP
releases. See [`CHANGELOG.md`](./CHANGELOG.md) for a full list of changes
in each release.

---

## Table of Contents

1. [v0.3.x → v1.0.0](#v03x--v100)
2. [v0.2.x → v0.3.x](#v02x--v03x)
3. [v0.1.x → v0.2.x](#v01x--v02x)

---

## v0.3.x → v1.0.0

### Breaking Changes

#### CLI subcommand reorganisation

The `atp` prefix has been promoted to `genatp` top-level commands:

| v0.3.x                                   | v1.0.0                                         |
| ---------------------------------------- | ----------------------------------------------- |
| `genatp serve --stdio`                   | `genatp serve --stdio` _(unchanged)_            |
| `genatp serve --port 3000`               | `genatp serve --port 3000` _(unchanged)_        |
| `genatp vault list`                      | `genatp vault list` _(unchanged)_               |
| `genatp record`                          | `genatp record` _(unchanged)_                   |
| `genatp cassette list`                   | `genatp cassette list` _(unchanged)_            |
| `atp test`, `atp run`                    | `genatp atp test`, `genatp atp run`             |
| —                                        | `genatp replay <run-id>` _(new)_                |
| —                                        | `genatp registry search <query>` _(new)_        |
| —                                        | `genatp registry publish <path> <pub>` _(new)_  |
| —                                        | `genatp registry install <slug>` _(new)_        |

For ATP Spec commands, use `genatp atp` instead of bare `atp`:

```bash
# v0.3.x (removed)
atp test login.atp.yml
atp run checkout.atp.yml
atp validate spec.yml

# v1.0.0
genatp atp test login.atp.yml
genatp atp run checkout.atp.yml
genatp atp validate spec.yml
```

#### Storage paths

The default data directory has been standardised. On first run, v1.0.0
automatically migrates existing data from the v0.3.x location:

| Component | v0.3.x | v1.0.0 |
|-----------|--------|--------|
| Vault | `~/.genatp/vault.sqlite` | `~/.local/share/genatp/genatp/data/vault.sqlite` |
| Cassettes | `~/.genatp/cassettes/` | `~/.local/share/genatp/genatp/data/cassettes/` |
| Event logs | `~/.genatp/logs/` | `~/.local/share/genatp/genatp/data/logs/` |
| Keys | `~/.genatp/keys/` | `~/.local/share/genatp/genatp/data/keys/` |

The old paths are **not deleted** — you can remove them manually after
verifying the migration succeeded:

```bash
genatp vault list          # verify vault migrated
genatp cassette list       # verify cassettes migrated
# Then, if everything looks correct:
rm -rf ~/.genatp           # remove old data directory
```

#### Version command

```bash
# v0.3.x
genatp version
# genatp 0.3.0

# v1.0.0
genatp version
# genatp 1.0.0
```

### New Features (no migration needed)

These additions in v1.0.0 require no code changes:

- **Cassette Registry**: `genatp registry search` / `install` / `publish`
- **Time-travel Debugger**: `genatp replay <run-id> --at N`, `genatp replay --serve`
- **Web UI**: React + Vite debugger at `http://localhost:5173`

---

## v0.2.x → v0.3.x

### Breaking Changes

#### Vault now requires OS keychain

The Auth Vault (Phase 6) stores its master encryption key in the OS
keychain. This is a one-time setup — the vault creates the keychain
entry automatically on first use.

**If you were using plaintext env vars for credentials**, switch to the
vault:

```python
# v0.2.x (no longer recommended)
agent.run("Log into github.com")  # credentials from env vars

# v0.3.x (preferred)
agent.run("Log into github.com")
# Then after successful login:
#   tools/call save_auth { "domain": "github.com" }
# Next run: tools/call load_auth { "domain": "github.com" }
```

#### MCP tool count increased

v0.3.0 adds `save_auth`, `load_auth`, `list_auth` — 12 tools total.
Existing clients automatically discover the new tools via `tools/list`.

### New Features

- **Auth Vault** with OS keychain integration
- **Capability-gated** vault access
- **Vault CLI**: `genatp vault list`, `revoke`, `path`

---

## v0.1.x → v0.2.x

### Breaking Changes

#### Python SDK replaces custom scripts

```python
# v0.1.x (removed)
# Custom scripts to drive the Rust binary

# v0.2.x (replaces all custom scripts)
from genatp import Agent

agent = Agent(model="claude-sonnet-4-20250514")
result = agent.run("Search for rust on example.com")
print(result.summary)
```

#### Import paths changed

```python
# v0.1.x (removed)
from genatp_sdk import Agent

# v0.2.x
from genatp import Agent
```

### New Features

- **Python SDK** (`pip install genatp`) on all 5 platforms
- **Async API**: `await agent.arun("...")`
- **LangChain adapter**: `from genatp.langchain import as_langchain_tools`
- **LlamaIndex adapter**: `from genatp.llamaindex import as_llamaindex_tools`
- **Type stubs**: full `.pyi` type hints
- **mkdocs documentation**: auto-generated API reference

---

## Appendix: Verifying your migration

After upgrading, run the smoke tests to confirm everything works:

```bash
# 1. Check binary
genatp version
# genatp 1.0.0

# 2. CLI smoke tests
genatp --help                         # shows all commands
genatp vault path                     # shows vault location
genatp cassette list                  # shows recorded cassettes

# 3. Unit tests
cargo test --workspace --exclude genatp-python --exclude genatp-node

# 4. Static analysis
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

If you encounter any migration issues, please open an issue at
<https://github.com/genATP/genATP/issues>.
