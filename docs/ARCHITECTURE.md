# genATP — Architecture 

This document describes the system architecture for genATP at v1.0. All diagrams use Mermaid so they render natively on GitHub.

**Engineering plan:** [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) — 18-phase implementation, CDP-native browser substrate, testing gates.

> **Design tenets**
> 1. One engine, three faces (MCP, Python SDK, TS SDK).
> 2. Tokens are the most expensive resource — optimize them ruthlessly.
> 3. Every byte of state is content-addressed, signable, and replayable.
> 4. Cloud-optional. Localhost must work fully offline (minus the page itself).

---

## 1. System Context (C4 Level 1)

How genATP sits in a developer's world.

```mermaid
graph TB
    subgraph users["External actors"]
        DEV["Developer<br/>(builds agents)"]
        AGENT["AI Agent<br/>(Claude / GPT / Llama)"]
        QA["QA Engineer<br/>(authors ATP specs)"]
    end

    subgraph genatp["genATP Platform"]
        CORE["genATP Engine<br/>(local or cloud)"]
    end

    subgraph external["External systems"]
        WEB["The Web<br/>(target sites)"]
        LLM["LLM Provider<br/>(any)"]
        REG["Cassette Registry<br/>(genatp.dev)"]
        KEY["OS Keychain<br/>(Keychain / DPAPI / Secret Service)"]
        PROXY["Proxy / Stealth Pool<br/>(optional, paid)"]
    end

    DEV -->|Python / TS SDK| CORE
    QA -->|ATP YAML files + CLI| CORE
    AGENT -->|MCP / HTTP| CORE

    CORE -->|HTTPS + stealth| WEB
    CORE -->|prompts when needed| LLM
    CORE <-->|install / publish| REG
    CORE <-->|store / unlock secrets| KEY
    CORE -.->|optional egress| PROXY

    classDef ext fill:#1f2937,stroke:#6b7280,color:#fff
    classDef core fill:#312e81,stroke:#a5b4fc,color:#fff
    classDef user fill:#064e3b,stroke:#6ee7b7,color:#fff
    class WEB,LLM,REG,KEY,PROXY ext
    class CORE core
    class DEV,AGENT,QA user
```

---

## 2. Container View (C4 Level 2)

The genATP engine, broken into deployable processes.

```mermaid
graph TB
    subgraph clients["Clients"]
        CL_MCP["MCP Client<br/>(Claude Desktop, Cursor, …)"]
        CL_PY["Python SDK"]
        CL_TS["TypeScript SDK"]
        CL_CLI["CLI<br/>(atp record / run / test)"]
    end

    subgraph engine["genATP Engine (single binary or container)"]
        IFACE["Interface Layer<br/>• MCP server (stdio + HTTP)<br/>• Native SDK bindings<br/>• CLI dispatcher"]
        ORCH["Orchestrator<br/>• Plan<br/>• Budget guardrails<br/>• Retry / heal policy<br/>• Cassette router"]
        PERC["Perception<br/>• DOM snapshotter<br/>• Vision adapter<br/>• Diff engine"]
        BROW["Browser Substrate<br/>• CDP via chromiumoxide<br/>• Stealth profile mgr"]
        STATE["State<br/>• Auth Vault (AES-256)<br/>• Cassette Store<br/>• Snapshot CAS"]
    end

    subgraph external["External"]
        LLM["LLM API"]
        REG["Cassette Registry"]
        KEY["OS Keychain"]
        SITE["Target site"]
    end

    CL_MCP --> IFACE
    CL_PY --> IFACE
    CL_TS --> IFACE
    CL_CLI --> IFACE

    IFACE --> ORCH
    ORCH --> PERC
    ORCH --> STATE
    ORCH -.->|"only when needed"| LLM
    PERC --> BROW
    BROW <--> SITE
    STATE <--> KEY
    STATE <-->|"install / publish"| REG

    classDef iface fill:#312e81,stroke:#a5b4fc,color:#fff
    classDef orch fill:#0c4a6e,stroke:#7dd3fc,color:#fff
    classDef perc fill:#064e3b,stroke:#6ee7b7,color:#fff
    classDef state fill:#581c87,stroke:#d8b4fe,color:#fff
    classDef ext fill:#1f2937,stroke:#6b7280,color:#fff

    class IFACE iface
    class ORCH orch
    class PERC,BROW perc
    class STATE state
    class LLM,REG,KEY,SITE ext
```

---

## 3. Component View — the Engine internals (C4 Level 3)

```mermaid
graph LR
    subgraph iface["Interface Layer"]
        MCP[MCP Server]
        REST[REST + WebSocket API]
        SDK[SDK Bindings<br/>Py / TS / Rust core]
    end

    subgraph orch["Orchestrator"]
        PLAN[Planner]
        BUDGET[Budget Guard]
        ROUTER[Cassette Router]
        HEAL[Self-Heal Engine]
    end

    subgraph perc["Perception"]
        AX[A11y Snapshotter]
        REFC[Ref Compressor<br/>elements → @e1 @e2 …]
        DIFF[Snapshot Diff]
        VISION[Vision Adapter<br/>screenshot → tokens]
        POLICY[Mode Policy<br/>DOM vs Vision]
    end

    subgraph browser["Browser Substrate"]
        CDP[CDP Driver]
        STEALTH[Stealth Profile]
        FP[Fingerprint Pool]
        INPUT[Human-Cadence Input]
    end

    subgraph state["State"]
        VAULT[Auth Vault]
        CAS_STORE[Cassette Store]
        CAS_INDEX[Content-Addressed<br/>Snapshot Store]
        EVT[Event Log<br/>append-only]
    end

    MCP --> ROUTER
    REST --> ROUTER
    SDK --> ROUTER

    ROUTER -->|"cassette hit"| CAS_STORE
    ROUTER -->|"cassette miss"| PLAN
    PLAN --> BUDGET
    BUDGET --> POLICY
    POLICY -->|"DOM mode"| AX
    POLICY -->|"vision mode"| VISION
    AX --> REFC --> DIFF
    DIFF --> PLAN
    VISION --> PLAN

    PLAN --> CDP
    CDP --> STEALTH
    CDP --> FP
    CDP --> INPUT

    CDP --> VAULT
    PLAN --> EVT
    DIFF --> CAS_INDEX
    ROUTER --> HEAL
    HEAL --> CAS_STORE

    classDef i fill:#312e81,stroke:#a5b4fc,color:#fff
    classDef o fill:#0c4a6e,stroke:#7dd3fc,color:#fff
    classDef p fill:#064e3b,stroke:#6ee7b7,color:#fff
    classDef b fill:#7c2d12,stroke:#fdba74,color:#fff
    classDef s fill:#581c87,stroke:#d8b4fe,color:#fff

    class MCP,REST,SDK i
    class PLAN,BUDGET,ROUTER,HEAL o
    class AX,REFC,DIFF,VISION,POLICY p
    class CDP,STEALTH,FP,INPUT b
    class VAULT,CAS_STORE,CAS_INDEX,EVT s
```

---

## 4. Data Flow — a single agent step

Sequence for one `click` action, showing the cost-optimal path.

```mermaid
sequenceDiagram
    autonumber
    participant A as AI Agent
    participant I as Interface (MCP)
    participant O as Orchestrator
    participant R as Cassette Router
    participant P as Perception
    participant B as Browser
    participant V as Vault
    participant L as LLM (last resort)

    A->>I: tools/call click(@e7)
    I->>O: dispatch(click, @e7)
    O->>R: lookup(current_step_hash)

    alt Cassette hit (zero-LLM path)
        R-->>O: deterministic action + expected_diff
        O->>B: cdp.click(selector)
        B-->>O: new accessibility tree
        O->>P: diff(new, expected)
        alt diff matches
            P-->>O: ✓ snapshot patch only
            O-->>I: result + 80 tokens
        else diff mismatch (page changed)
            O->>L: heal(prompt, screenshot)
            L-->>O: new selector + patch
            O->>R: update cassette (versioned)
            O->>B: retry click
            B-->>O: success
        end
    else Cassette miss (LLM-driven)
        O->>P: snapshot()
        P->>B: get accessibility tree
        B-->>P: raw tree
        P->>P: compress + diff vs last
        P-->>O: 400–800 token snapshot
        O->>L: plan(snapshot, task)
        L-->>O: action plan
        O->>B: execute
        B-->>O: result
        O->>R: persist as cassette (if successful)
    end

    O->>V: persist cookies / storage<br/>(if domain auth changed)
    I-->>A: result
```

**Token math (design targets)**: a cassette-hit costs **0 LLM tokens** (deterministic replay; optional heal uses a scoped LLM call only on drift). A cassette-miss plan step costs ~600 tokens (compressed snapshot + reply). A vision-fallback step costs ~12,000 tokens. The cassette router tries replay before planning whenever a matching cassette exists.

---

## 5. Perception Mode Policy

The decision tree that picks between DOM and Vision, automatically.

```mermaid
flowchart TD
    A[Page loaded or DOM mutation] --> B{A11y tree<br/>has named elements?}
    B -->|yes| C[Build DOM snapshot]
    B -->|no / sparse| F{Canvas or<br/>shadow-DOM heavy?}

    C --> D{Snapshot stable<br/>across 500ms?}
    D -->|yes| E[Emit compressed diff<br/>~400-800 tokens]
    D -->|no| F

    F -->|yes| G[Screenshot crop<br/>around viewport]
    F -->|no| H[Wait + retry DOM<br/>up to 3x]
    H -->|still bad| G

    G --> I[Send to vision model<br/>~12k tokens]
    E --> J[Action ready]
    I --> J

    style E fill:#064e3b,stroke:#6ee7b7,color:#fff
    style I fill:#7c2d12,stroke:#fdba74,color:#fff
    style J fill:#312e81,stroke:#a5b4fc,color:#fff
```

---

## 6. Cassette Lifecycle

How a "skill" goes from recorded → replayed → healed → shared.

```mermaid
stateDiagram-v2
    [*] --> Recording: atp record <task>
    Recording --> Validated: human / LLM marks success
    Validated --> Signed: sign with project key
    Signed --> LocalStore: write to ~/.genatp/cassettes
    Signed --> Published: atp publish

    Published --> Registry: genatp.dev/r/...
    Registry --> Installed: atp install <slug>

    LocalStore --> Replaying: agent.run()
    Installed --> Replaying

    Replaying --> Verified: diff matches expectation
    Replaying --> Drifted: structural change
    Drifted --> Healing: LLM patches selectors
    Healing --> Verified: heal succeeded
    Healing --> Failed: heal failed
    Failed --> Recording: re-record offered

    Verified --> [*]
```

A cassette is a signed JSON document:

```jsonc
{
  "id": "cas_2j9z…",
  "version": "1.3.0",
  "domain": "github.com",
  "task_hash": "sha256:…",
  "steps": [
    { "act": "navigate", "url": "https://github.com/login" },
    { "act": "fill", "ref": "@e1", "from": "vault:github/user" },
    { "act": "click", "ref": "@e4", "expect_diff_hash": "…" }
  ],
  "auth_required": ["github.com"],
  "signature": "ed25519:…",
  "published_by": "@maya",
  "verified_by": ["genatp-team"]
}
```

---

## 7. Auth Vault

```mermaid
graph LR
    subgraph user["User boundary"]
        BIO[Biometric / passkey]
        OS[OS Keychain<br/>Keychain · DPAPI · Secret Service]
    end

    subgraph vault["Auth Vault"]
        KEK[Key-Encryption Key<br/>unlocked by OS]
        DEK[Per-domain<br/>Data Keys]
        STORE[(Encrypted store<br/>~/.genatp/vault.db)]
    end

    subgraph runtime["Runtime"]
        ENG[genATP Engine]
        BC[Browser Context<br/>per domain]
    end

    BIO --> OS
    OS -->|"unwrap"| KEK
    KEK -->|"AES-256-GCM"| DEK
    DEK -->|"decrypt on use"| STORE
    STORE --> ENG
    ENG -->|"hydrate cookies +<br/>localStorage + IDB"| BC

    classDef sec fill:#581c87,stroke:#d8b4fe,color:#fff
    classDef sys fill:#1f2937,stroke:#6b7280,color:#fff
    classDef rt fill:#064e3b,stroke:#6ee7b7,color:#fff
    class KEK,DEK,STORE sec
    class BIO,OS sys
    class ENG,BC rt
```

**Properties**
- Vault never leaves disk un-encrypted; KEK lives only in OS keychain.
- Per-domain DEKs mean a single compromised domain key doesn't leak others.
- Vault entries are *capabilities*: a cassette declares which domains it needs, the engine prompts the user once, and the grant is scoped.

---

## 8. Deployment Topologies

### 8.1 Local-only (default)

```mermaid
graph LR
    DEV["Developer's<br/>laptop"] --> ENG["genATP Engine<br/>(localhost)"]
    ENG --> BROWSER["Local Chromium"]
    BROWSER --> WEB["The Web"]
    ENG --> LLM["LLM API<br/>(user's key)"]
```

Zero infrastructure. Zero accounts. Auth vault on disk, encrypted by OS keychain.

### 8.2 Self-hosted server (team)

```mermaid
graph TB
    subgraph team["Team boundary (VPC / on-prem)"]
        AGENTS["AI Agents<br/>(internal services)"]
        ENG["genATP Engine<br/>(Docker / k8s)"]
        BROWSERS["Browser Pool<br/>(headless Chromium replicas)"]
        VAULT_SVC["Vault Service<br/>(Hashicorp / AWS KMS)"]
        REG_MIRROR["Private Cassette<br/>Mirror"]
    end
    WEB["The Web"]
    LLM["LLM API"]

    AGENTS --> ENG
    ENG --> BROWSERS
    BROWSERS --> WEB
    ENG --> VAULT_SVC
    ENG --> REG_MIRROR
    ENG --> LLM
```

### 8.3 Hosted / managed (out of scope for this repo)

Managed cloud pools, residential proxies, and HSM vaults are **not** implemented in the self-hosted open product. The diagram below is a possible future commercial topology only — do not treat it as shipped architecture.

```mermaid
graph TB
    USER["Developer's agent<br/>(anywhere)"]
    EDGE["Edge gateway<br/>+ WAF"]
    ORCH["Orchestrator pool"]
    POOL["Stealth browser pool<br/>(rotating fingerprints +<br/>residential proxies)"]
    VAULT["Managed Vault<br/>(HSM-backed)"]
    REG["Public Registry"]
    BILLING["Billing"]

    USER -->|TLS + API key| EDGE
    EDGE --> ORCH
    ORCH --> POOL
    ORCH --> VAULT
    ORCH --> REG
    ORCH --> BILLING
    POOL --> WEB["The Web"]
```

---

## 9. Token Optimization Pipeline

Every snapshot passes through this pipeline before reaching the LLM.

```mermaid
flowchart LR
    A["Raw accessibility tree<br/>~50,000 tokens"] --> B["Strip non-interactive<br/>~12,000 tokens"]
    B --> C["Compress refs to @eN<br/>~6,000 tokens"]
    C --> D{First snapshot<br/>this run?}
    D -->|yes| E["Send full<br/>~4,000 tokens"]
    D -->|no| F["Diff vs previous<br/>~400-800 tokens"]
    E --> G["LLM"]
    F --> G

    style A fill:#7c2d12,stroke:#fdba74,color:#fff
    style E fill:#0c4a6e,stroke:#7dd3fc,color:#fff
    style F fill:#064e3b,stroke:#6ee7b7,color:#fff
    style G fill:#312e81,stroke:#a5b4fc,color:#fff
```

Numbers from internal benchmarks against the Playwright MCP baseline on WebVoyager subset. End-to-end this delivers a 60×–120× token reduction vs the naive raw-HTML path and a ~10× reduction vs Playwright MCP.

---

## 10. Snapshot Content-Addressed Storage

Every snapshot, action, and diff is hashed and chained. This gives us:

```mermaid
graph LR
    S0["Snapshot S0<br/>hash: a1b2…"] -->|"action A0"| S1["Snapshot S1<br/>hash: c3d4…<br/>parent: a1b2"]
    S1 -->|"action A1"| S2["Snapshot S2<br/>hash: e5f6…<br/>parent: c3d4"]
    S2 -->|"action A2"| S3["Snapshot S3<br/>hash: g7h8…<br/>parent: e5f6"]

    style S0 fill:#581c87,stroke:#d8b4fe,color:#fff
    style S1 fill:#581c87,stroke:#d8b4fe,color:#fff
    style S2 fill:#581c87,stroke:#d8b4fe,color:#fff
    style S3 fill:#581c87,stroke:#d8b4fe,color:#fff
```

Capabilities unlocked:
- **Time-travel debugging** — `atp replay --at S2` reproduces exact state.
- **Deterministic cassettes** — replay verifies hash chain on every step.
- **Audit trails** — every production run is fully reconstructable.
- **De-duplication** — identical snapshots stored once across all runs.

---

## 11. Technology Choices

| Layer | Choice | Why |
|---|---|---|
| Core engine | Rust (Tokio) | Cold-start in milliseconds, no GC pauses in hot path, memory safety |
| Browser driver | **Direct CDP via `chromiumoxide`** (soft-fork when needed) | Sub-ms action dispatch; avoids Playwright Node IPC (~15–40 ms/step) and detectable CDP fingerprint |
| Browser binary | System Chrome + runtime CDP overrides (v0.1–v0.4); patched Chromium build (v0.5+) | Proven stealth path before custom Chromium pipeline |
| MCP server | `rmcp` (official Rust MCP SDK) | Spec-compliant, stdio + HTTP |
| HTTP API | `axum` | Standard async Rust stack |
| Python SDK | PyO3 + maturin | Native speed, single source of truth |
| TS SDK | NAPI-RS | Same engine, no behavioural drift |
| Local storage | SQLite (`rusqlite`) | Vault, cassettes, snapshot index — zero infra |
| Cloud storage | Postgres + S3 | Multi-tenant, content-addressed blobs |
| Content addressing | BLAKE3 | Faster than SHA-256; native Rust |
| Vault crypto | `ring` + OS keychain (Keychain / DPAPI / Secret Service) | Audited, native |
| Cassette format | JSON (JCS) + zstd, signed with ed25519 | Inspectable, compact, verifiable |
| Registry | OCI Distribution Spec v1.1 on S3 + CDN | Industry standard |
| LLM | In-house `genatp-llm` trait (no LangChain in core) | Provider-agnostic; adapters optional |
| Telemetry | Opt-in, OpenTelemetry, never page content | Privacy-first |

Full rationale and rejected alternatives: [`DEVELOPMENT_PLAN.md` §2](./DEVELOPMENT_PLAN.md#2-tech-stack-decision-record).

---

## 12. Security Model

- **Cassettes are capabilities, not blank checks.** A cassette declares the domains and vault entries it needs. The engine prompts the user once before execution; grants are scoped, revocable, and logged.
- **Signed publishing.** Public Registry entries require ed25519 signatures; the CLI shows the signer for every install.
- **Sandboxed replay.** Cassettes execute in a hardened browser context: no extension installs, no file-system access from page JS, no `localhost` navigation unless explicitly allowed.
- **Vault isolation.** Per-domain DEKs; browser contexts are spawned with only the secrets the cassette declared.
- **No data ever leaves localhost in self-hosted Personal mode.** Telemetry is fully opt-in and content-redacted by default.

---

## 13. Performance Budgets

| Operation | Budget |
|---|---|
| Cold start (CLI → first snapshot) | < 2 s |
| Page snapshot (DOM mode) | < 250 ms |
| Cassette replay step | < 50 ms (no LLM) |
| Vision fallback step | < 1.5 s |
| Vault unlock (per session) | < 100 ms |
| 20-step WebVoyager task (avg) | < 35,000 tokens total |
