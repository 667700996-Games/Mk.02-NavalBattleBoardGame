# Release Artifact, Promotion, and Rollback Contract

Mk.01 builds the server and web images once, identifies both by OCI SHA-256 digest, and promotes
that exact release manifest through development, staging, canary, and production. An environment
may change configuration, scale, traffic weight, and secrets, but it may not rebuild or retag the
application. `config/release-policy.json` is the machine-readable authority and
`npm run release:check` rejects drift between this document, the environment definitions, the
Compose runtime, deployment scripts, and GitHub workflows.

## Immutable artifact

The `Release Artifact` workflow verifies the source, performs a locked Rust/npm build, pushes one
server image and one web image, generates SPDX SBOMs, emits GitHub/Sigstore provenance, applies
keyless Cosign signatures, and writes `.artifacts/release-manifest.json`. The manifest binds:

- the full 40-character Git SHA and commit-derived `SOURCE_DATE_EPOCH`;
- the server and web repository names plus immutable image digests;
- every migration and the checksum ledger, both frozen V2 protocol contracts, balance rules, and
  Cargo/npm lockfiles;
- SBOM and provenance attestation URLs and the registry signature references; and
- a digest over the complete manifest body.

The workflow uses BuildKit provenance and SBOM output as well as explicit GitHub attestations. A
promotion runner verifies both Cosign certificate identities before it reads deployment secrets.
The release Compose file contains no `build:` key and refuses image references without supplied
values, so destination environments cannot silently rebuild the source.

## Environment chain

| Environment | Source      | Replicas | Initial traffic | Hold  | Approvals |
| ----------- | ----------- | -------: | --------------: | ----: | --------: |
| development | none        |        1 |            100% |    0s |         0 |
| staging     | development |        2 |            100% |    0s |         1 |
| canary      | staging     |        2 |             10% | 900s |         1 |
| production  | canary      |        3 |            100% | 900s |         2 |

Each definition in `deploy/environments/` has a distinct loopback gateway binding, protected
GitHub environment, explicit promotion predecessor, scale, public-origin variable, and three
secret-file dependencies. The deployment host or upstream load balancer terminates public TLS and
routes only the approved traffic percentage to that environment gateway. The pinned Caddy runtime
image is infrastructure input `MK01_GATEWAY_IMAGE`; server and web remain the promoted release
subjects.

Before the workflow is enabled, create all four GitHub environments, apply their required-reviewer
rules, register a dedicated `self-hosted`, `linux`, `mk01-deploy` runner, and supply `PUBLIC_ORIGIN`,
`GATEWAY_IMAGE`, `DATABASE_URL`, `REDIS_URL`, and `ADMIN_TOKEN`. The gateway image variable must
also be a repository pinned by SHA-256. Secret values are written with `umask 077` to runner-temp
files and mounted read-only; they are never placed in the manifest or deployment receipt.

## Evidence and preflight

Promotion consumes a separate `release-evidence-<environment>` artifact. Evidence is not created
by the deployment script and no missing field defaults to success. Its `releaseId`, manifest
digest, destination, observation timestamp, promotion source, distinct approvals, and every named
check must match. Evidence older than six hours is rejected.
`deploy/release-evidence.example.json` is the complete producer contract; copy its shape but replace
every identifier and measurement with retained output from the named suite or observability query.

The ten required checks are quality, unit, integration, contract, E2E, visual, performance,
migration compatibility, active-match compatibility, and backup freshness. The migration proof
must show an additive-only ledger with valid checksums and a stable-server restart after candidate
migrations. The active-match proof must report zero incompatible snapshots and a successful
stable/candidate recovery. Backups must be no older than 12 hours and the last restore drill no
older than 90 days.

Canary evidence requires at least 1,000 API requests, 100 accepted commands, 100 completed sockets,
a 15-minute observation, and the following stop limits:

| Signal                     | Promotion limit |
| -------------------------- | --------------: |
| Availability               |        ≥ 99.90% |
| API error rate             |          ≤ 2.0% |
| Command p95 / p99          |  ≤ 250 / 600 ms |
| Unexpected disconnect rate |          ≤ 2.0% |
| Supported protocol rejects |               0 |
| Distributed event failures |               0 |

Canary admission proves the 10% step. Production admission proves the observed 10%, 25%, 50%, and
100% steps. An absent metric, insufficient denominator, stale observation, duplicated approval,
or threshold breach returns `BLOCK` with a named reason.

## Deployment

`scripts/deploy-release.sh ENV MANIFEST EVIDENCE [PREVIOUS_MANIFEST]` performs the following fixed
sequence:

1. evaluate the release evidence and write a decision artifact;
2. read only the digest-pinned images and release ID from the approved manifest;
3. validate HTTPS origin, loopback gateway, pinned gateway image, and readable secret files;
4. render and pull the release Compose graph;
5. run `mk01-server --migrate-only` as a separate fail-closed preflight;
6. replace server/web replicas with the same release digest and wait for health;
7. verify readiness through the gateway and confirm protocol V2 remains accepted; and
8. retain an identity-free receipt containing environment, manifest digest, image digests, scale,
   readiness, compatibility, and deployment timestamp.

The environment evidence producer is responsible for exercising the staged candidate and
recording the metrics above before the next traffic step. Never turn a failed observation into a
pass by deleting series, reducing sample floors, rebuilding an image, or changing a threshold in
the incident branch.

## Rollback

Production promotion requires a distinct previous release manifest, two approvals, and a rehearsal
that restored that exact manifest within 900 seconds. `scripts/rollback-release.sh` requires the
old release ID to be typed exactly plus an 8–500 character incident reason. It validates the
historical manifest and attestations, pulls its old server/web digests, replaces application
containers, verifies readiness, enforces the 15-minute RTO, and writes a rollback receipt.

Database state is never rolled backward automatically. Candidate migrations are additive and the
previous application has already been tested against them. A data problem uses an approved
forward-fix or the independent restore procedure; it never runs an unreviewed down migration while
active matches exist. The web artifact is rolled back first for client negotiation failures, and
the server follows only when the previous artifact still accepts every pinned active snapshot.
