import assert from "node:assert/strict";
import test from "node:test";

import { evaluateRelease, validateReleasePolicy } from "./release-gate.mjs";
import { createReleaseManifest } from "./release-manifest.mjs";

const digest = (character) =>
  `registry.example/mk01@sha256:${character.repeat(64)}`;

function manifest({
  gitSha = "a".repeat(40),
  server = "1",
  web = "2",
  epoch = 1_787_050_800,
} = {}) {
  return createReleaseManifest({
    gitSha,
    sourceDateEpoch: epoch,
    images: { server: digest(server), web: digest(web) },
    attestations: {
      sbom: { server: "oci://server.sbom", web: "oci://web.sbom" },
      provenance: "oci://provenance",
      keylessSignature: "rekor://entry",
    },
  });
}

function evidence(candidate, environment, promotionFrom = null) {
  return {
    schemaVersion: 1,
    releaseId: candidate.releaseId,
    artifactManifestDigest: candidate.manifestDigest,
    environment,
    observedAt: "2026-08-18T12:00:00.000Z",
    checks: Object.fromEntries(
      validateReleasePolicy().preflight.requiredChecks.map((check) => [
        check,
        true,
      ]),
    ),
    migration: {
      additiveOnly: true,
      checksumVerified: true,
      stableRestartedAfterCandidate: true,
    },
    activeMatches: { incompatibleSnapshots: 0, stableCandidateRecovery: true },
    promotionFrom: promotionFrom
      ? { environment: promotionFrom, manifestDigest: candidate.manifestDigest }
      : null,
    approvals:
      environment === "production" ? ["release-a", "release-b"] : ["release-a"],
    backup: { ageSeconds: 300, restoreDrillAgeDays: 7 },
    canary: {
      holdSeconds: 900,
      completedTrafficStepsPercent:
        environment === "production" ? [10, 25, 50, 100] : [10],
      metrics: {
        availability: 0.9999,
        apiErrorRate: 0.001,
        commandP95Milliseconds: 80,
        commandP99Milliseconds: 160,
        unexpectedDisconnectRate: 0.002,
        protocolRejections: 0,
        distributedEventFailures: 0,
        apiRequests: 2000,
        commands: 500,
        websockets: 200,
      },
    },
    rollback: {
      rehearsed: true,
      recoverySeconds: 120,
      restoredManifestDigest: null,
    },
  };
}

test("release policy defines an immutable four-environment promotion chain", () => {
  const policy = validateReleasePolicy();
  assert.deepEqual(policy.environmentOrder, [
    "development",
    "staging",
    "canary",
    "production",
  ]);
  assert.equal(policy.sameArtifactAcrossEnvironments, true);
});

test("development, staging and canary accept the same digest with complete evidence", () => {
  const candidate = manifest();
  const development = evidence(candidate, "development");
  development.approvals = [];
  assert.equal(
    evaluateRelease({
      environmentName: "development",
      manifest: candidate,
      evidence: development,
      previousManifest: null,
      now: new Date("2026-08-18T13:00:00.000Z"),
    }).decision,
    "PROMOTE",
  );
  assert.equal(
    evaluateRelease({
      environmentName: "staging",
      manifest: candidate,
      evidence: evidence(candidate, "staging", "development"),
      previousManifest: candidate,
      now: new Date("2026-08-18T13:00:00.000Z"),
    }).decision,
    "PROMOTE",
  );
  assert.equal(
    evaluateRelease({
      environmentName: "canary",
      manifest: candidate,
      evidence: evidence(candidate, "canary", "staging"),
      previousManifest: candidate,
      now: new Date("2026-08-18T13:00:00.000Z"),
    }).decision,
    "PROMOTE",
  );
});

test("production requires a distinct rehearsed rollback manifest and two approvals", () => {
  const candidate = manifest();
  const previous = manifest({
    gitSha: "b".repeat(40),
    server: "3",
    web: "4",
    epoch: 1_787_000_000,
  });
  const proof = evidence(candidate, "production", "canary");
  proof.rollback.restoredManifestDigest = previous.manifestDigest;
  const result = evaluateRelease({
    environmentName: "production",
    manifest: candidate,
    evidence: proof,
    previousManifest: previous,
    now: new Date("2026-08-18T13:00:00.000Z"),
  });
  assert.equal(result.decision, "PROMOTE");
  assert.deepEqual(result.errors, []);
});

test("a canary threshold breach fails closed with a named reason", () => {
  const candidate = manifest();
  const proof = evidence(candidate, "canary", "staging");
  proof.canary.metrics.protocolRejections = 1;
  const result = evaluateRelease({
    environmentName: "canary",
    manifest: candidate,
    evidence: proof,
    previousManifest: candidate,
    now: new Date("2026-08-18T13:00:00.000Z"),
  });
  assert.equal(result.decision, "BLOCK");
  assert.ok(result.errors.includes("canary protocol rejection gate failed"));
});

test("source drift and incomplete checks cannot reuse an older manifest", () => {
  const candidate = manifest();
  const proof = evidence(candidate, "development");
  proof.approvals = [];
  candidate.materials.balanceRules.sha256 = "0".repeat(64);
  proof.checks.activeMatchCompatibility = false;
  const result = evaluateRelease({
    environmentName: "development",
    manifest: candidate,
    evidence: proof,
    previousManifest: null,
    now: new Date("2026-08-18T13:00:00.000Z"),
  });
  assert.equal(result.decision, "BLOCK");
  assert.ok(result.errors.includes("balanceRules source digest drifted"));
  assert.ok(
    result.errors.includes(
      "required preflight check failed: activeMatchCompatibility",
    ),
  );
  assert.ok(result.errors.includes("manifest digest is invalid"));
});
