---
name: Bug report
about: Report a defect in genATP
title: "[bug] "
labels: ["bug", "triage"]
assignees: []
---

## What happened

<!-- One paragraph. -->

## What did you expect to happen

## Steps to reproduce

```
1.
2.
3.
```

A minimal cassette / ATP spec / code snippet is gold.

## Environment

- `genatp --version`:
- OS + arch:
- Rust version (if building from source):
- Browser (Chrome / Chromium / Edge) version:
- Agent / SDK consumer (Claude Desktop / LangChain / your own / …):

## Logs

```
$ RUST_LOG=debug genatp <cmd>  # paste relevant output here
```

## Severity

- [ ] Crash / data loss
- [ ] Wrong result (silent or surfaced)
- [ ] Performance regression
- [ ] Cosmetic / docs

## Have you checked

- [ ] Latest release reproduces this
- [ ] No existing issue covers it
- [ ] Not a security issue (use `SECURITY.md` if it is)
