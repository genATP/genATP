# genATP — User Flows

End-to-end journeys for the primary personas, covering install → first success → power use → monetization touchpoints.

> **Personas (full detail in PRD.md)**
> - **Maya** — solo agent builder, Python, hobby/indie
> - **Devansh** — startup CTO, mixed Py/TS, scaling on a budget
> - **Lena** — Fortune 500 ML platform engineer, on-prem, compliance
> - **Tomás** — QA lead, replacing brittle Playwright tests

---

## Flow 1 — Maya's first 90 seconds (the moment we win or lose her)

**Goal:** From discovery (a tweet / GitHub link) to a working agent run in under 90 seconds.

```mermaid
journey
    title Maya's first-run journey
    section Discovery
      Sees viral benchmark tweet: 5: Maya
      Clicks GitHub link: 5: Maya
      Reads 3-line README quickstart: 5: Maya
    section Install
      pip install genatp: 5: Maya
      genatp init: 4: Maya
      Pastes Anthropic key: 4: Maya
    section First success
      Runs 5-line Python example: 5: Maya
      Agent navigates and returns result: 5: Maya
      Sees token cost in CLI footer: 5: Maya
    section Share
      Tweets the cost number: 5: Maya
      Stars the repo: 5: Maya
```

### Detailed flow

```mermaid
flowchart TD
    A([Maya lands on genatp.dev]) --> B{Sees<br/>3-line quickstart?}
    B -->|yes| C[pip install genatp]
    B -->|no, leaves| Z1[❌ Lost]

    C --> D[genatp init]
    D --> E{Has LLM key in env?}
    E -->|yes| G[Run first example]
    E -->|no| F[Prompt: paste key,<br/>store in OS keychain]
    F --> G

    G --> H[Engine launches local Chromium]
    H --> I[Snapshot homepage]
    I --> J[LLM plans next step]
    J --> K[Execute click / fill]
    K --> L{Task complete?}
    L -->|no| I
    L -->|yes| M[CLI prints result<br/>+ token cost + cost in $]

    M --> N[★ Star + share]

    style Z1 fill:#7f1d1d,stroke:#fca5a5,color:#fff
    style N fill:#064e3b,stroke:#6ee7b7,color:#fff
    style M fill:#312e81,stroke:#a5b4fc,color:#fff
```

**Critical design rules**
- `pip install genatp` must complete in < 30 s on a clean machine (Chromium fetched on first run if not present).
- The first run must print *something* within 5 s (loading dots are fine — silence is fatal).
- The CLI footer always shows `tokens: 412 · cost: $0.0011 · time: 4.2s`. This is the screenshot people tweet.

---

## Flow 2 — Devansh records a cassette and saves $400/month

**Goal:** Turn a daily, expensive AI agent run into a cheap, deterministic cassette replay with auto-healing.

```mermaid
sequenceDiagram
    autonumber
    participant D as Devansh
    participant CLI as genatp CLI
    participant ENG as Engine
    participant LLM as LLM
    participant SITE as Competitor site
    participant VAULT as Auth Vault
    participant CAS as Cassette Store

    Note over D,CAS: Day 1 — record once

    D->>CLI: atp record "scrape pricing from competitor.com"
    CLI->>ENG: start recording session
    ENG->>SITE: navigate
    ENG->>LLM: plan(snapshot)
    LLM-->>ENG: action plan
    ENG->>SITE: execute (login, navigate, extract)
    SITE-->>ENG: pricing data
    ENG->>VAULT: persist competitor.com session
    ENG->>CAS: save cassette "competitor-pricing@1.0"
    CAS-->>D: ✓ recorded · 8 steps · 11,400 tokens used

    Note over D,CAS: Day 2..N — replay forever

    D->>CLI: atp run competitor-pricing
    CLI->>ENG: replay cassette
    ENG->>VAULT: hydrate competitor.com session
    ENG->>SITE: deterministic actions (no LLM)
    SITE-->>ENG: same pricing data
    CAS-->>D: ✓ replayed · 8 steps · 0 LLM tokens

    Note over D,SITE: Day N+1 — site redesign

    D->>CLI: atp run competitor-pricing
    CLI->>ENG: replay cassette
    ENG->>SITE: deterministic actions
    SITE-->>ENG: DOM mismatch on step 4 ❌
    ENG->>LLM: heal(broken_step, screenshot, original_intent)
    LLM-->>ENG: patched selector
    ENG->>SITE: retry step 4
    SITE-->>ENG: ✓ data
    ENG->>CAS: write v1.1 (auto-bumped)
    CAS-->>D: ✓ replayed + auto-healed · 1 step re-planned · 800 tokens
```

### State view

```mermaid
stateDiagram-v2
    [*] --> NotRecorded
    NotRecorded --> Recording: atp record
    Recording --> Recorded: success
    Recording --> Aborted: error / budget exceeded
    Recorded --> Replaying: atp run
    Replaying --> Replayed: hash chain valid
    Replaying --> Healing: drift detected
    Healing --> Replayed: heal ok
    Healing --> ReRecord: heal failed
    ReRecord --> Recording
    Replayed --> Replaying: next scheduled run
    Aborted --> [*]
```

**Cost outcome:**
- Before: 50 competitor sites × 180,000 tokens/run × 30 runs/mo ≈ **$2,700 / month**
- After: 50 × ~400 tokens/run × 30 runs (replay) ≈ **$6 / month**, plus occasional heal costs (~$5)
- **~$2,500 / month saved** with no change to the agent code.

---

## Flow 3 — Tomás replaces brittle Playwright tests with ATP specs

**Goal:** A QA engineer authors a single `.atp.yml` file that works as a test, an automation, and a documentation artifact.

```mermaid
flowchart LR
    A[Tomás writes<br/>checkout.atp.yml] --> B{Run mode?}
    B -->|test| C[atp test<br/>in CI]
    B -->|automate| D[atp run<br/>on cron]
    B -->|agent tool| E[agent.use<br/>checkout.atp.yml]

    C --> F{Assertions pass?}
    F -->|yes| G[✅ CI green]
    F -->|no| H[❌ CI red<br/>+ time-travel link]
    H --> I[Tomás clicks link<br/>frame-by-frame replay]
    I --> J[Sees diff<br/>fixes spec or files bug]

    D --> K[Cassette replay,<br/>0 LLM calls]
    K --> L[Result piped to Slack]

    E --> M[Agent inherits<br/>cassette capability]

    style G fill:#064e3b,stroke:#6ee7b7,color:#fff
    style H fill:#7f1d1d,stroke:#fca5a5,color:#fff
    style M fill:#312e81,stroke:#a5b4fc,color:#fff
```

### CI integration sequence

```mermaid
sequenceDiagram
    participant DEV as Developer commits
    participant CI as GitHub Actions
    participant ATP as atp test runner
    participant ENG as genATP Engine
    participant SLACK as Slack
    participant DEBUG as Time-Travel UI

    DEV->>CI: git push
    CI->>ATP: atp test specs/**/*.atp.yml
    ATP->>ENG: spawn ephemeral browser pool
    loop each spec
        ENG->>ENG: run cassette (replay) or LLM (drift)
        ENG-->>ATP: pass / fail + snapshot chain
    end
    ATP-->>CI: junit.xml + cost report

    alt failure
        ATP->>SLACK: ❌ checkout.atp.yml failed<br/>link to replay
        SLACK->>DEBUG: click link
        DEBUG-->>SLACK: scrubbable replay UI
    else success
        ATP->>SLACK: ✅ 47 specs passed · $0.13 total
    end
```

---

## Flow 4 — Lena passes a Fortune-500 security review

**Goal:** A platform engineer evaluates genATP for on-prem deployment and signs an enterprise contract.

```mermaid
flowchart TD
    A([Lena finds genATP on GitHub]) --> B[Reviews SECURITY.md]
    B --> C{Audit-grade<br/>features?}
    C -->|yes| D[Books demo]
    C -->|no| Z1[❌ Lost to closed-source competitor]

    D --> E[Demo: vault, signed cassettes,<br/>audit log, SBOM]
    E --> F[Requests SOC 2 report]
    F --> G[Pen-test report shared under NDA]
    G --> H[POC on internal VPC]

    H --> I{Cassettes work behind<br/>corporate proxy?}
    I -->|yes| J[POC success]
    I -->|no| K[Engineering ticket<br/>filed, 1-week SLA]
    K --> J

    J --> L[Procurement → contract]
    L --> M[💰 $50k/year ARR]

    style Z1 fill:#7f1d1d,stroke:#fca5a5,color:#fff
    style M fill:#064e3b,stroke:#6ee7b7,color:#fff
```

### Enterprise capabilities required at each gate

```mermaid
graph LR
    G1[Gate 1<br/>Security review] --> R1[• Signed cassettes<br/>• OS keychain<br/>• No telemetry by default]
    G2[Gate 2<br/>Architecture review] --> R2[• On-prem container<br/>• Air-gapped registry mirror<br/>• OIDC SSO]
    G3[Gate 3<br/>Procurement] --> R3[• SOC 2 Type II<br/>• MSA / DPA<br/>• Indemnification]
    G4[Gate 4<br/>Pilot] --> R4[• 24/7 SLA<br/>• Customer-specific cassette policy<br/>• Audit log to SIEM]

    style R1 fill:#312e81,stroke:#a5b4fc,color:#fff
    style R2 fill:#312e81,stroke:#a5b4fc,color:#fff
    style R3 fill:#312e81,stroke:#a5b4fc,color:#fff
    style R4 fill:#312e81,stroke:#a5b4fc,color:#fff
```

---

## Flow 5 — Authentication & the Vault

**Goal:** An agent logs in once. Across every subsequent run, restart, and machine reboot, the session is restored without prompts or human intervention — yet remains compromise-resistant.

```mermaid
sequenceDiagram
    autonumber
    participant A as Agent
    participant ENG as Engine
    participant V as Vault
    participant OS as OS Keychain
    participant U as User (one-time)
    participant SITE as github.com

    Note over A,SITE: First run

    A->>ENG: navigate(github.com)
    ENG->>SITE: GET /
    SITE-->>ENG: redirect → /login
    ENG->>A: snapshot shows login form
    A->>ENG: fill + submit (may require MFA prompt to U)
    U->>ENG: MFA code (one time)
    ENG->>SITE: auth success → cookies + localStorage
    ENG->>OS: derive KEK (biometric / passkey)
    OS-->>ENG: KEK
    ENG->>V: encrypt(cookies, DEK_github)
    V-->>ENG: stored

    Note over A,SITE: All later runs

    A->>ENG: navigate(github.com)
    ENG->>V: load(github.com)
    V->>OS: unwrap KEK (biometric or session)
    OS-->>V: KEK
    V-->>ENG: decrypted cookies + localStorage
    ENG->>SITE: GET / with hydrated state
    SITE-->>ENG: ✓ logged-in homepage
    ENG-->>A: snapshot (already authed)
```

### What the user sees

```mermaid
flowchart TD
    A[Agent needs github.com] --> B{Vault entry<br/>exists?}
    B -->|no| C[Open login page,<br/>agent fills if possible]
    C --> D{MFA required?}
    D -->|yes| E[Prompt user once<br/>via OS notification]
    D -->|no| F[Auto-fill from vault<br/>secrets if configured]
    E --> G[Login success]
    F --> G
    G --> H[Vault stores session,<br/>scoped to github.com]
    B -->|yes| I[Hydrate session<br/>0 prompts]
    H --> J[Agent continues]
    I --> J

    style E fill:#7c2d12,stroke:#fdba74,color:#fff
    style I fill:#064e3b,stroke:#6ee7b7,color:#fff
```

---

## Flow 6 — Discovering and installing a community Cassette

**Goal:** A developer needs to interact with Stripe's billing portal. They install a community-maintained cassette instead of writing one.

```mermaid
sequenceDiagram
    participant D as Developer
    participant CLI as atp CLI
    participant REG as Cassette Registry
    participant ENG as Engine
    participant VAULT as Vault

    D->>CLI: atp search "stripe billing"
    CLI->>REG: search(q="stripe billing")
    REG-->>CLI: 7 results · top: @stripe-official/billing@2.1
    D->>CLI: atp install @stripe-official/billing
    CLI->>REG: download cassette + signature
    REG-->>CLI: cas + ed25519 sig
    CLI->>CLI: verify signature against keyring
    CLI-->>D: ⚠ This cassette requires: stripe.com vault grant
    D->>CLI: y
    CLI->>VAULT: register capability request

    Note over D,VAULT: Next agent run

    D->>ENG: agent.use("@stripe-official/billing").run("cancel my sub")
    ENG->>VAULT: get stripe.com grant
    VAULT-->>ENG: ok (scoped)
    ENG->>ENG: replay cassette
    ENG-->>D: ✓ subscription canceled · 0 LLM tokens
```

### Trust ladder

```mermaid
graph LR
    A[Unsigned cassette] -->|"refuses to run"| X[❌ Blocked]
    B[Self-signed cassette] -->|"warns user"| Y[⚠ Manual approval]
    C[Org-signed cassette] -->|"checks org keyring"| Z1[✓ Auto-run within org]
    D["Verified-publisher cassette<br/>(genATP-signed)"] -->|"green checkmark in UI"| Z2[✓ Trusted]

    style X fill:#7f1d1d,stroke:#fca5a5,color:#fff
    style Y fill:#7c2d12,stroke:#fdba74,color:#fff
    style Z1 fill:#064e3b,stroke:#6ee7b7,color:#fff
    style Z2 fill:#064e3b,stroke:#6ee7b7,color:#fff
```

---

## Flow 7 — DOM-mode → Vision fallback (the auto-switch in action)

**Goal:** An agent encounters a canvas-heavy page (e.g., a Figma file or Tableau dashboard) and the engine transparently switches to vision without the agent noticing.

```mermaid
flowchart TD
    A[Agent: extract chart data] --> B[Engine snapshots page]
    B --> C{A11y tree<br/>has labeled<br/>elements?}
    C -->|yes, rich| D[DOM mode<br/>~600 tokens]
    C -->|no, canvas/iframe| E[Screenshot crop<br/>+ vision model<br/>~12,000 tokens]

    D --> F[Agent receives<br/>structured snapshot]
    E --> F

    F --> G[Agent emits action]
    G --> H{Action target<br/>resolvable?}
    H -->|yes via @eN ref| I[Click via CDP]
    H -->|only via pixel coords| J[Click via coordinates<br/>from vision]
    I --> K[Result]
    J --> K

    K --> L{Result success?}
    L -->|no, after retry| M[Log to event store<br/>+ flag for human review]
    L -->|yes| N[Append to cassette]

    style D fill:#064e3b,stroke:#6ee7b7,color:#fff
    style E fill:#7c2d12,stroke:#fdba74,color:#fff
    style N fill:#312e81,stroke:#a5b4fc,color:#fff
```

The agent never has to decide; the routing happens inside the engine. From the agent's perspective both modes return the same shape of structured snapshot.

---

## Flow 8 — Budget guardrails (production safety)

**Goal:** A production agent on a runaway loop must abort before burning $100 of tokens.

```mermaid
sequenceDiagram
    participant A as Agent
    participant O as Orchestrator
    participant G as Budget Guard
    participant H as Hook (Slack / Pager)

    A->>O: run("scrape 1000 pages")
    O->>G: register budget · 50k tokens, 5 min
    loop steps
        O->>G: charge(step_cost)
        G-->>O: ok
    end

    G->>G: 80% reached
    G->>H: warning("80% of budget used")

    G->>G: 100% reached
    G->>O: 🛑 hard stop
    O->>A: BudgetExceededError
    O->>H: alert + replay link
```

Budgets are declarative (`budget: { tokens: 50000, seconds: 300, usd: 1.00 }`) and apply to ATP specs, SDK calls, and MCP sessions equally. Soft thresholds (80%) notify; hard thresholds (100%) abort.

---

## Flow Index — when to read which

| You want to understand… | Read |
|---|---|
| How a new user gets to "wow" | Flow 1 |
| How cassettes save money | Flow 2 |
| How tests work | Flow 3 |
| How we sell to enterprise | Flow 4 |
| How auth persistence works | Flow 5 |
| How community cassettes work | Flow 6 |
| How DOM↔vision switching works | Flow 7 |
| How we keep production safe | Flow 8 |
