# genATP — Real High-Level System Architecture (PPT)

Three faithful high-level views of **this project’s** architecture
(from [`ARCHITECTURE.md`](./ARCHITECTURE.md)). Not a generic PPT template.

Pick one for the slide.

---

## Variant A — System context (C4 Level 1)

Who talks to genATP, and what genATP talks to.

```mermaid
flowchart TB
    subgraph actors["Actors"]
        DEV["Developer"]
        AGENT["AI Agent<br/>Claude / GPT / Llama"]
        QA["QA Engineer"]
    end

    CORE["genATP Engine<br/>local or cloud"]

    subgraph external["External systems"]
        WEB["The Web<br/>target sites"]
        LLM["LLM Provider<br/>any"]
        REG["Cassette Registry"]
        KEY["OS Keychain"]
        PROXY["Proxy / Stealth Pool<br/>optional"]
    end

    DEV -->|"Python / TypeScript SDK"| CORE
    QA -->|"ATP Spec + CLI"| CORE
    AGENT -->|"MCP / HTTP"| CORE

    CORE -->|"HTTPS + stealth"| WEB
    CORE -->|"prompts when needed"| LLM
    CORE <-->|"install / publish"| REG
    CORE <-->|"store / unlock secrets"| KEY
    CORE -.->|"optional"| PROXY
```

**What this shows:** genATP is the web-access layer between agents/devs and the real web.

**Pick when:** You want the highest-level “where does this sit in the world?” slide.

---

## Variant B — Engine containers (C4 Level 2) ⭐ recommended

The real internal shape of the product: **one engine, five subsystems**, three client faces.

```mermaid
flowchart TB
    subgraph clients["Clients"]
        MCP["MCP Client<br/>Cursor · Claude Desktop"]
        PY["Python SDK"]
        TS["TypeScript SDK"]
        CLI["CLI<br/>record / run / test"]
    end

    subgraph engine["genATP Engine — single binary"]
        IFACE["Interface<br/>MCP server · SDK bindings · CLI"]
        ORCH["Orchestrator<br/>plan · budgets · retry/heal · cassette router"]
        PERC["Perception<br/>DOM snapshot · vision fallback · diff"]
        BROW["Browser Substrate<br/>CDP Chromium · stealth"]
        STATE["State<br/>Auth Vault · Cassette Store · Snapshot CAS"]
    end

    subgraph external["External"]
        LLM["LLM API"]
        SITE["Target website"]
        KEY["OS Keychain"]
        REG["Cassette Registry"]
    end

    MCP --> IFACE
    PY --> IFACE
    TS --> IFACE
    CLI --> IFACE

    IFACE --> ORCH
    ORCH --> PERC
    ORCH --> STATE
    ORCH -.->|"only when needed"| LLM
    PERC --> BROW
    BROW <--> SITE
    STATE <--> KEY
    STATE <-->|"install / publish"| REG
```

**What this shows:** All clients hit one Interface; Orchestrator decides plan vs cassette replay; Perception + Browser act on sites; State holds vault/cassettes.

**Pick when:** You want the real product architecture on the slide (best default).

---

## Variant C — Engine internals (C4 Level 3, simplified)

How a request moves inside the engine — including the **cassette hit / miss** split.

```mermaid
flowchart LR
    subgraph iface["Interface"]
        IN["MCP · REST · SDK"]
    end

    subgraph orch["Orchestrator"]
        ROUTER["Cassette Router"]
        PLAN["Planner"]
        BUDGET["Budget Guard"]
        HEAL["Self-Heal"]
    end

    subgraph perc["Perception"]
        POLICY["DOM vs Vision policy"]
        AX["A11y snapshot → @eN refs → diff"]
        VIS["Vision adapter"]
    end

    subgraph browser["Browser"]
        CDP["CDP driver + stealth"]
    end

    subgraph state["State"]
        VAULT["Auth Vault"]
        CAS["Cassette Store"]
        SNAP["Snapshot CAS"]
    end

    IN --> ROUTER
    ROUTER -->|"hit"| CAS
    ROUTER -->|"miss"| PLAN
    PLAN --> BUDGET --> POLICY
    POLICY --> AX
    POLICY --> VIS
    AX --> PLAN
    VIS --> PLAN
    PLAN --> CDP
    CDP --> VAULT
    ROUTER --> HEAL --> CAS
    AX --> SNAP
```

**What this shows:** Cassette hit = no LLM. Miss = plan under budget → perceive → act via CDP → persist vault/snapshots; heal updates cassettes.

**Pick when:** Judges should understand *why* genATP is cheaper/faster (router + perception + vault).

---

## Quick pick guide

| Goal | Variant |
|---|---|
| Big-picture placement in the ecosystem | **A — Context** |
| Real high-level system architecture of the product | **B — Engine** ⭐ |
| Deeper path: cassette router + perception + vault | **C — Internals** |

---

## Design tenets (optional one-liner under the diagram)

1. One engine, three faces (MCP, Python SDK, TypeScript SDK)  
2. Tokens are the most expensive resource  
3. State is content-addressed, signable, replayable  
4. Cloud-optional — localhost works fully offline (minus the page itself)

Source of truth: [`docs/ARCHITECTURE.md`](./ARCHITECTURE.md)
