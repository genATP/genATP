# Security policy

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately via one of the following:

1. **Preferred** — GitHub Security Advisory: [Report a vulnerability](https://github.com/genatp/genatp/security/advisories/new)
2. **Email** — `security@genatp.dev` (PGP key fingerprint published once Phase 16 ships)

Please include:

- A clear description of the issue and its impact.
- Steps to reproduce (a minimal cassette / ATP spec / code snippet is gold).
- Affected versions (`genatp --version` output).
- Your environment (OS, Rust version if building from source).
- Suggested mitigation if you have one.

## Our response timeline

| Severity | Initial response | Triage decision | Patch target |
|---|---|---|---|
| Critical (RCE, data exfil, vault compromise) | within 24h | within 48h | within 7 days |
| High (auth bypass, signature forgery) | within 48h | within 72h | within 14 days |
| Medium (DoS, info leak) | within 5 business days | within 10 business days | next minor release |
| Low | within 10 business days | within 20 business days | best-effort |

We will keep you informed throughout the process and credit you in the advisory and CHANGELOG unless you request anonymity.

## Supported versions

genATP follows semantic versioning. Until v1.0:

| Version | Supported |
|---|---|
| latest minor on `main` | yes |
| previous minor | yes (security only) |
| anything older | no |

After v1.0 we will support the current major + previous major for security patches for 12 months from the major release.

## Scope — what we treat as a vulnerability

| In scope | Out of scope |
|---|---|
| Vulnerabilities in genATP code (`crates/`, `bindings/`) | Vulnerabilities in dependencies (report upstream; we may file CVE backports) |
| Auth Vault crypto / key handling | A misconfigured user vault on a compromised machine |
| Cassette signature forgery / tampering | A user installing an unsigned third-party cassette |
| Registry server (`genatp-registry`) auth / authz bugs | DoS via legitimate request volume against your own self-hosted registry |
| MCP server transport / tool argument injection | Bugs in agents that consume our MCP tools (file with the agent vendor) |
| ATP Spec parser memory safety / DoS | Malicious cassette content executed inside the sandboxed browser (this is a feature, not a bug, when the user explicitly trusts the cassette) |
| Stealth engine that leaks identifying fingerprints | A bot-detection vendor improving their detection |

## Bug bounty

genATP does not currently operate a paid bug bounty. We will publicly acknowledge reporters in the advisory and CHANGELOG, and offer swag once available.

## Supply chain

- All releases are built by GitHub Actions from a tagged commit; the build provenance is attested via GitHub Artifact Attestations starting at v0.0.1 (Phase 0 produces unsigned artifacts; signing lands in Phase 16).
- All releases publish a SHA256SUMS file alongside the artifacts.
- All cassettes published via the official registry are signed with maintainer ed25519 keys; the public key set is pinned in `crates/genatp-registry/keys.toml`.
- We use `cargo deny` and `cargo audit` on every PR to catch known-vulnerable transitive dependencies.

## Hardening notes for users

If you are running genATP in production:

1. **Pin the version.** Update on a schedule, not on every release.
2. **Verify checksums and signatures** of installer artifacts.
3. **Run with the least-privileged OS user** that can spawn Chrome.
4. **Use `--vault-keychain`** (Phase 6) to bind the vault to the OS keychain.
5. **Audit cassettes before installing them**, especially from third-party publishers. `genatp inspect <cassette>` shows the recorded actions and assertions.
6. **Network egress filter** the headless browser if the workload allows.

## Cryptography

genATP uses well-established primitives only. No custom crypto. See `docs/ARCHITECTURE.md` § "Security Model".

| Use | Primitive |
|---|---|
| Vault encryption-at-rest | AES-256-GCM |
| Vault key derivation | Argon2id |
| Cassette signature | ed25519 |
| Snapshot content addressing | BLAKE3 |
| Registry transport | TLS 1.3 |

If you have concerns about any of these choices, file a discussion (not a vulnerability report — these are public design choices).
