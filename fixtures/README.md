# `fixtures/`

Deterministic test material used across the workspace.

| Folder | Owner phase | Contents |
|---|---|---|
| `sites/` | Phase 0+ | Static HTML pages served by the fixture server. Phase 0 ships `hello.html`. |
| `cassettes/` | Phase 7+ | Golden cassettes used by replay/self-heal regression tests. |
| `snapshots/` | Phase 2+ | Insta-managed snapshot baselines for the perception crate. |
| `llm/` | Phase 3+ | Recorded LLM transcripts (deterministic mock provider replays). |

**Hard rules**

1. Fixtures must be deterministic — no time, no random IDs, no live network.
2. Fixtures with secrets are forbidden. Use synthetic credentials only.
3. New fixtures land with a one-line entry in this table explaining what they cover.
