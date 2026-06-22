# ADR-004: OCI Distribution v1.1 for Cassette Registry

| Field        | Value                        |
|--------------|------------------------------|
| **Status**   | Accepted                     |
| **Date**     | 2026-06-22                   |
| **Phase**    | 14                           |
| **Version**  | v0.9.0-pre                   |

## Context

genATP cassettes (recorded agent trajectories) needed a distribution mechanism so users can publish, discover, and install them — similar to Docker Hub for container images or npm for JavaScript packages.

Requirements from the development plan:
- **Publish**: push a signed cassette to a registry with version metadata
- **Discover**: search by keyword (slug, description, goal)
- **Install**: pull a cassette by slug + semver constraint (`^1.2.3`, `~1.0.0`, `latest`)
- **Trust ladder**: unsigned → self-signed → org-signed → verified publisher
- **CLI commands**: `genatp registry search`, `install`, `publish`, `info`
- **Self-host server**: OCI-compatible API that a team can run locally

## Decision

Adopt **OCI Distribution Spec v1.1** as the foundation for the cassette registry API.

### Why OCI Distribution Spec

1. **Industry standard**: Used by Docker, GitHub Container Registry, Harbor, and every major container registry. Any engineer familiar with `docker pull/push` will recognize the pattern.
2. **URL structure aligns with cassette model**: `/v2/{namespace}/manifests/{tag}` maps naturally to `/v2/{slug}/manifests/{version}`.
3. **Content-addressable storage**: OCI blobs (addressed by digest) match genATP's BLAKE3 content-hash model for cassettes.
4. **Version already mature**: OCI Distribution Spec v1.1 (2024) supports referrers, subject manifests, and artifact types that map well to cassette metadata.
5. **Future-proof**: If genATP Cloud ships, the API can be served by standard OCI-compatible infrastructure.

### Deviations from strict OCI compliance

We intentionally diverge from pure OCI in these areas:

| OCI Spec                     | genATP implementation                | Rationale                                    |
|------------------------------|--------------------------------------|----------------------------------------------|
| `application/vnd.oci.image.manifest.v1+json` | Custom `RegistryManifest` JSON       | Cassette metadata is richer than OCI image config |
| Blob upload (POST/PUT)       | Inline publish via PUT manifest      | Cassettes are small (KB-scale); multistep upload adds latency |
| Referrers API                | Not implemented                      | Not needed at current scale                  |
| Media-type negotiation       | JSON only                            | Simpler CLI integration                      |

Our API is **OCI-inspired** rather than strictly OCI-conformant, with custom endpoints added:
- `GET /search?q=...` — full-text search
- `GET /publishers` — list registered publishers
- `GET /health` — health check

### Trust ladder design

The trust ladder follows a progressive-enforcement model:

```
Unsigned  →  SelfSigned  →  OrgSigned  →  Verified
   │             │              │              │
   ▼             ▼              ▼              ▼
 Blocked    Warning+      Auto-run in    Auto-run
             Confirm         org           everywhere
```

- **Unsigned**: cassettes with no publisher key → blocked by default
- **SelfSigned**: cassettes signed with a key not yet trusted → requires `--yes` flag
- **OrgSigned**: signed by an org key → auto-runs inside the org, warned outside
- **Verified**: publisher has been manually verified (admin function, 24h cooldown) → auto-runs everywhere

### Semver resolution

We implement npm-style semver resolution:
- `^1.2.3` — compatible (≥1.2.3, <2.0.0)
- `~1.2.3` — approximate (≥1.2.3, <1.3.0)
- `1.2.3` — exact match
- `1.2.3-beta.1` — prerelease
- `latest` / `*` — highest stable version
- Prereleases are excluded from `^`, `~`, and `latest` resolutions

### Storage backend

SQLite for self-host mode (Phase 14), with S3+Postgres planned for genATP Cloud:
- **Tables**: `publishers`, `cassettes` (manifest JSON), `cassette_domains`, `cassette_capabilities`
- **WAL mode** for concurrent read access
- **Transaction-protected publish** for atomicity
- **:memory:** databases used in tests for speed

## Consequences

### Positive

- Familiar OCI URL structure (`/v2/{slug}/tags/list`)
- Clean separation between storage, API, client, and manifest types
- Trust ladder prevents surprise execution of untrusted cassettes
- Semver resolution matches user expectations from npm/Cargo
- 41 unit tests covering all layers

### Negative

- Not a drop-in OCI-compatible registry (custom media types, missing blob endpoints)
- No referrer support for dependency tracking
- Self-host only in Phase 14; remote registry sync requires future work
- `client::install` currently requires the cassette to be present in local storage (no remote download fallback)

### Risks

- The 24h publisher cooldown is not enforced for pre-existing publishers
- No rate limiting on the API server
- No TLS support in the self-host server (users must reverse-proxy with nginx/Caddy)

## Alternatives considered

1. **npm registry API**: Rejected because npm's metadata format (package.json) doesn't map well to cassette capabilities and trust levels.
2. **Custom REST API**: Rejected because engineers already understand OCI patterns; inventing a new API would increase the learning curve.
3. **Flat-file registry (no DB)**: Rejected because SQLite enables efficient search, resolution, and publisher management without network dependencies.
4. **Use Docker Registry directly**: Rejected because storing cassettes as OCI artifacts loses cassette-specific metadata (domains, trust level, capabilities).

## References

- [OCI Distribution Spec v1.1](https://github.com/opencontainers/distribution-spec/blob/main/spec.md)
- [genATP DEVELOPMENT_PLAN.md](../DEVELOPMENT_PLAN.md) — Phase 14
- [genATP ARCHITECTURE.md](../ARCHITECTURE.md)
- [genATP PRD.md](../PRD.md)
