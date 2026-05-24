# ADR-NNNN: {{ short title of decision }}

- **Status**: Proposed | Accepted | Deprecated | Superseded by ADR-XXXX
- **Date**: YYYY-MM-DD
- **Deciders**: @maintainer-1, @maintainer-2
- **Phase**: {{ Phase number from `docs/DEVELOPMENT_PLAN.md` }}
- **Tags**: `architecture`, `crypto`, `api`, `tooling`, … (lower-case, comma-separated)

> **One-sentence summary.** Replace this block with a single sentence the casual reader can absorb in 5 seconds.

## Context

What is the situation that requires a decision? Be specific:

- What is broken or missing today?
- What constraints (technical, legal, organisational, performance) apply?
- What did we learn from prototypes / spikes?

Cite issues, PRs, prior ADRs, and external sources by URL.

## Decision

State the decision in plain prose. Include the **operative verb** ("we will use", "we will not use", "we will postpone").

If the decision has multiple parts, number them.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Option A | … |
| Option B | … |
| Status quo (do nothing) | … |

For each rejected alternative, give a one-line "why not" — enough that a future reader does not have to re-litigate the choice.

## Consequences

### Positive

- …

### Negative / costs

- …
- (Always include this — every decision has costs.)

### Neutral / follow-ups

- Tickets that must be filed: #…
- Crates/files that must change: `crates/…`
- ADRs that this supersedes: ADR-XXXX

## Validation

How will we know this decision was right (or wrong)?

- Metric or outcome we will watch for the next N weeks.
- Trigger that would cause us to revisit (e.g. "if cold-start exceeds 500 ms in CI").

## References

- [Link 1](https://…)
- [Link 2](https://…)
