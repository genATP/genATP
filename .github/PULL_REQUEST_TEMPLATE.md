<!--
Thanks for contributing to genATP. Please fill this in. Empty fields slow down review.
-->

## What does this PR do?

<!-- One paragraph. The "why", not just the "what". -->

## Linked issues / ADRs

- Closes #
- ADR: `docs/adr/NNNN-…` (required if this changes a public API, wire format, or storage layout)

## Phase

<!-- Which phase from docs/DEVELOPMENT_PLAN.md does this belong to? -->

Phase: <!-- e.g. 2 (Perception) -->

## Type of change

- [ ] Bug fix (no API change)
- [ ] Feature (new behaviour, no breaking change)
- [ ] Breaking change (semver-major; requires ADR)
- [ ] Performance
- [ ] Refactor (no behaviour change)
- [ ] Documentation only
- [ ] Test only / CI only

## Testing

- [ ] Added or updated unit tests
- [ ] Added or updated integration / e2e tests where appropriate
- [ ] Added a regression test (for bug fixes)
- [ ] Added a Criterion bench (for perf changes)
- [ ] `cargo test --workspace` green locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` green locally
- [ ] `cargo fmt --all -- --check` green locally
- [ ] `cargo deny check` green locally

## Reviewer checklist

- [ ] Public API changes have rustdoc + an ADR
- [ ] No new dependencies, OR new dependency justified above (license, maintenance, size)
- [ ] No `unsafe` introduced (or, if necessary, has a `// SAFETY:` justification block)
- [ ] No regressions in coverage (codecov check passes)
- [ ] No regressions in benches (bench-regression check passes, if applicable)
- [ ] Touches secrets / crypto / vault → flagged for security review

## Notes for reviewers

<!-- Anything reviewers should pay particular attention to. Tradeoffs you considered. Things you punted on. -->
