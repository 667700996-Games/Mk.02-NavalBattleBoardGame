#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { releaseManifestDigest, sourceMaterials } from "./release-manifest.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const load = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));

function fail(errors, condition, message) {
  if (!condition) errors.push(message);
}

export function validateReleasePolicy() {
  const errors = [];
  const policy = load("config/release-policy.json");
  fail(
    errors,
    policy.schemaVersion === 1,
    "release policy schemaVersion must be 1",
  );
  fail(
    errors,
    policy.sameArtifactAcrossEnvironments === true,
    "artifact promotion must be immutable",
  );
  fail(
    errors,
    JSON.stringify(policy.environmentOrder) ===
      JSON.stringify(["development", "staging", "canary", "production"]),
    "environment order must be development, staging, canary, production",
  );
  let previous = null;
  const gatewayBindings = new Set();
  for (const name of policy.environmentOrder) {
    const environment = load(`deploy/environments/${name}.json`);
    fail(
      errors,
      environment.schemaVersion === 1,
      `${name} schemaVersion must be 1`,
    );
    fail(errors, environment.name === name, `${name} name mismatch`);
    fail(
      errors,
      environment.promotionFrom === previous,
      `${name} promotion source must be ${previous}`,
    );
    fail(
      errors,
      environment.githubEnvironment === name,
      `${name} GitHub environment mismatch`,
    );
    fail(
      errors,
      environment.replicas >= 1,
      `${name} requires at least one replica`,
    );
    fail(
      errors,
      /^127\.0\.0\.1:\d{4,5}$/.test(environment.gatewayBind),
      `${name} gateway must bind to a dedicated loopback port`,
    );
    fail(
      errors,
      !gatewayBindings.has(environment.gatewayBind),
      `${name} gateway binding overlaps`,
    );
    gatewayBindings.add(environment.gatewayBind);
    fail(
      errors,
      Array.isArray(environment.requiredSecrets) &&
        environment.requiredSecrets.length === 3,
      `${name} must declare the three runtime secret files`,
    );
    previous = name;
  }
  fail(
    errors,
    JSON.stringify(policy.canary.trafficStepsPercent) ===
      JSON.stringify([10, 25, 50, 100]),
    "canary rollout steps must be 10, 25, 50, 100",
  );
  fail(
    errors,
    policy.canary.minimumHoldSeconds >= 900,
    "canary hold must be at least 15 minutes",
  );
  fail(
    errors,
    policy.rollback.requireRehearsal === true,
    "rollback rehearsal must be mandatory",
  );
  const compose = readFileSync(
    resolve(root, "deploy/compose.release.yaml"),
    "utf8",
  );
  fail(
    errors,
    compose.includes("${MK01_SERVER_IMAGE:?"),
    "release compose must require server image",
  );
  fail(
    errors,
    compose.includes("${MK01_WEB_IMAGE:?"),
    "release compose must require web image",
  );
  fail(
    errors,
    !/^\s+build:/m.test(compose),
    "release compose must never rebuild promoted images",
  );
  fail(
    errors,
    (compose.match(/_FILE:/g) ?? []).length >= 3,
    "release compose must use secret files",
  );
  const deployScript = readFileSync(
    resolve(root, "scripts/deploy-release.sh"),
    "utf8",
  );
  const rollbackScript = readFileSync(
    resolve(root, "scripts/rollback-release.sh"),
    "utf8",
  );
  fail(
    errors,
    deployScript.includes("release-gate.mjs"),
    "deployment must execute the release gate",
  );
  fail(
    errors,
    deployScript.includes("--migrate-only"),
    "deployment must run migrations separately",
  );
  fail(
    errors,
    deployScript.includes("/api/ready"),
    "deployment must verify readiness through gateway",
  );
  fail(
    errors,
    rollbackScript.includes("dataRollback:false"),
    "rollback receipt must preserve data migrations",
  );
  fail(
    errors,
    rollbackScript.includes("recovery_seconds > 900"),
    "rollback must enforce its RTO",
  );
  const buildWorkflow = readFileSync(
    resolve(root, ".github/workflows/release-build.yml"),
    "utf8",
  );
  const promoteWorkflow = readFileSync(
    resolve(root, ".github/workflows/release-promote.yml"),
    "utf8",
  );
  for (const token of [
    "docker/build-push-action@v7",
    "actions/attest@v4",
    "anchore/sbom-action@v0",
    "cosign sign --yes",
    "release-manifest.mjs",
    "postgres:16-alpine",
    "redis:7-alpine",
    'REQUIRE_DISTRIBUTED_INTEGRATION: "true"',
    "npm test",
  ]) {
    fail(
      errors,
      buildWorkflow.includes(token),
      `release build workflow is missing ${token}`,
    );
  }
  fail(
    errors,
    promoteWorkflow.includes("environment: ${{ inputs.environment }}"),
    "promotion must use protected environments",
  );
  fail(
    errors,
    promoteWorkflow.includes("cosign verify"),
    "promotion must verify image signatures",
  );
  fail(
    errors,
    promoteWorkflow.includes("deploy-release.sh"),
    "promotion must use the deployment wrapper",
  );
  const serverDockerfile = readFileSync(
    resolve(root, "apps/server/Dockerfile"),
    "utf8",
  );
  const webDockerfile = readFileSync(
    resolve(root, "apps/web/Dockerfile"),
    "utf8",
  );
  fail(
    errors,
    serverDockerfile.includes("cargo build --locked --release"),
    "server image must honor Cargo.lock",
  );
  fail(
    errors,
    webDockerfile.includes("RUN npm ci"),
    "web image must honor package-lock.json",
  );
  if (errors.length) throw new Error(errors.join("\n"));
  return policy;
}

export function releaseManifestErrors(
  manifest,
  policy = validateReleasePolicy(),
  verifyCurrentSource = true,
) {
  const errors = [];
  const imagePattern = new RegExp(policy.artifact.imageDigestPattern);
  fail(
    errors,
    manifest.schemaVersion === 1,
    "manifest schemaVersion must be 1",
  );
  fail(
    errors,
    manifest.manifestDigest === releaseManifestDigest(manifest),
    "manifest digest is invalid",
  );
  fail(
    errors,
    imagePattern.test(manifest.images?.server ?? ""),
    "server image is not digest-pinned",
  );
  fail(
    errors,
    imagePattern.test(manifest.images?.web ?? ""),
    "web image is not digest-pinned",
  );
  if (verifyCurrentSource) {
    const materials = sourceMaterials();
    for (const material of policy.artifact.requiredMaterials) {
      fail(
        errors,
        manifest.materials?.[material]?.sha256 === materials[material]?.sha256,
        `${material} source digest drifted`,
      );
    }
  } else {
    for (const material of policy.artifact.requiredMaterials) {
      fail(
        errors,
        Boolean(manifest.materials?.[material]?.sha256),
        `${material} digest is missing`,
      );
    }
  }
  for (const attestation of policy.artifact.requiredAttestations) {
    fail(
      errors,
      Boolean(manifest.attestations?.[attestation]),
      `${attestation} attestation is missing`,
    );
  }
  return errors;
}

export function evaluateRelease({
  environmentName,
  manifest,
  evidence,
  previousManifest,
  now = new Date(),
}) {
  const policy = validateReleasePolicy();
  const environment = load(`deploy/environments/${environmentName}.json`);
  const errors = releaseManifestErrors(manifest, policy);
  fail(
    errors,
    evidence.schemaVersion === 1,
    "evidence schemaVersion must be 1",
  );
  fail(
    errors,
    evidence.releaseId === manifest.releaseId,
    "evidence releaseId mismatch",
  );
  fail(
    errors,
    evidence.environment === environmentName,
    "evidence environment mismatch",
  );
  const observedAt = new Date(evidence.observedAt);
  const evidenceAgeSeconds = (now.getTime() - observedAt.getTime()) / 1000;
  fail(
    errors,
    Number.isFinite(observedAt.getTime()),
    "evidence observedAt is invalid",
  );
  fail(errors, evidenceAgeSeconds >= 0, "evidence observedAt is in the future");
  fail(
    errors,
    evidenceAgeSeconds <= policy.preflight.maximumEvidenceAgeSeconds,
    "release evidence is stale",
  );
  fail(
    errors,
    evidence.artifactManifestDigest === manifest.manifestDigest,
    "evidence artifact manifest digest mismatch",
  );
  for (const check of policy.preflight.requiredChecks) {
    fail(
      errors,
      evidence.checks?.[check] === true,
      `required preflight check failed: ${check}`,
    );
  }
  fail(
    errors,
    evidence.migration?.additiveOnly === true,
    "migrations are not additive-only",
  );
  fail(
    errors,
    evidence.migration?.checksumVerified === true,
    "migration checksum was not verified",
  );
  fail(
    errors,
    evidence.migration?.stableRestartedAfterCandidate === true,
    "stable restart after candidate migration was not proven",
  );
  fail(
    errors,
    evidence.activeMatches?.incompatibleSnapshots === 0,
    "incompatible active snapshots remain",
  );
  fail(
    errors,
    evidence.activeMatches?.stableCandidateRecovery === true,
    "mixed stable/candidate active-match recovery was not proven",
  );
  fail(
    errors,
    new Set(
      (evidence.approvals ?? []).filter(
        (approval) => typeof approval === "string" && approval.trim(),
      ),
    ).size >= environment.requiredApprovals,
    `${environmentName} requires ${environment.requiredApprovals} distinct approval(s)`,
  );
  fail(
    errors,
    evidence.backup?.ageSeconds <= policy.preflight.maximumBackupAgeSeconds,
    "backup freshness gate failed",
  );
  fail(
    errors,
    evidence.backup?.restoreDrillAgeDays <=
      policy.preflight.maximumRestoreDrillAgeDays,
    "restore drill freshness gate failed",
  );
  if (environment.promotionFrom) {
    fail(
      errors,
      Boolean(previousManifest),
      `${environmentName} requires the promoted manifest`,
    );
    fail(
      errors,
      evidence.promotionFrom?.environment === environment.promotionFrom,
      `promotion must come from ${environment.promotionFrom}`,
    );
    fail(
      errors,
      evidence.promotionFrom?.manifestDigest === manifest.manifestDigest,
      "promotion attempted to rebuild or replace the artifact",
    );
  }
  if (environmentName === "canary" || environmentName === "production") {
    const metrics = evidence.canary?.metrics ?? {};
    const limits = policy.canary.thresholds;
    const minimumSamples = policy.canary.minimumSamples;
    fail(
      errors,
      evidence.canary?.holdSeconds >= environment.minimumHoldSeconds,
      "canary observation window is too short",
    );
    fail(
      errors,
      metrics.availability >= limits.availabilityMinimum,
      "canary availability failed",
    );
    fail(
      errors,
      metrics.apiErrorRate <= limits.apiErrorRateMaximum,
      "canary API error rate failed",
    );
    fail(
      errors,
      metrics.commandP95Milliseconds <= limits.commandP95MillisecondsMaximum,
      "canary command p95 failed",
    );
    fail(
      errors,
      metrics.commandP99Milliseconds <= limits.commandP99MillisecondsMaximum,
      "canary command p99 failed",
    );
    fail(
      errors,
      metrics.unexpectedDisconnectRate <=
        limits.unexpectedDisconnectRateMaximum,
      "canary disconnect rate failed",
    );
    fail(
      errors,
      metrics.protocolRejections <= limits.protocolRejectionsMaximum,
      "canary protocol rejection gate failed",
    );
    fail(
      errors,
      metrics.distributedEventFailures <=
        limits.distributedEventFailuresMaximum,
      "canary distributed event gate failed",
    );
    fail(
      errors,
      metrics.apiRequests >= minimumSamples.apiRequests,
      "canary API sample floor failed",
    );
    fail(
      errors,
      metrics.commands >= minimumSamples.commands,
      "canary command sample floor failed",
    );
    fail(
      errors,
      metrics.websockets >= minimumSamples.websockets,
      "canary WebSocket sample floor failed",
    );
    const requiredSteps =
      environmentName === "production"
        ? policy.canary.trafficStepsPercent
        : [policy.canary.trafficStepsPercent[0]];
    fail(
      errors,
      requiredSteps.every((step) =>
        evidence.canary?.completedTrafficStepsPercent?.includes(step),
      ),
      "required canary traffic steps were not observed",
    );
  }
  if (environmentName === "production") {
    fail(
      errors,
      Boolean(previousManifest),
      "production requires a previous rollback manifest",
    );
    for (const error of previousManifest
      ? releaseManifestErrors(previousManifest, policy, false)
      : []) {
      errors.push(`previous rollback manifest: ${error}`);
    }
    fail(
      errors,
      evidence.rollback?.rehearsed === true,
      "rollback was not rehearsed",
    );
    fail(
      errors,
      evidence.rollback?.recoverySeconds <=
        policy.rollback.maximumRecoverySeconds,
      "rollback recovery exceeded the budget",
    );
    fail(
      errors,
      evidence.rollback?.restoredManifestDigest ===
        previousManifest?.manifestDigest,
      "rollback did not restore the previous manifest",
    );
    fail(
      errors,
      previousManifest?.images?.server !== manifest.images.server ||
        previousManifest?.images?.web !== manifest.images.web,
      "previous rollback manifest must identify a distinct release",
    );
  }
  return {
    schemaVersion: 1,
    decision: errors.length ? "BLOCK" : "PROMOTE",
    environment: environmentName,
    releaseId: manifest.releaseId,
    manifestDigest: manifest.manifestDigest,
    evaluatedAt: evidence.observedAt,
    errors,
  };
}

function args(argv) {
  const result = {};
  for (let index = 0; index < argv.length; index += 2) {
    if (!argv[index]?.startsWith("--") || argv[index + 1] === undefined) {
      throw new Error(`invalid argument near ${argv[index] ?? "<end>"}`);
    }
    result[argv[index].slice(2)] = argv[index + 1];
  }
  return result;
}

function main() {
  const input = args(process.argv.slice(2));
  if (input["policy-only"] === "true") {
    const policy = validateReleasePolicy();
    process.stdout.write(
      `Release policy passed: ${policy.environmentOrder.length} environments.\n`,
    );
    return;
  }
  if (input["manifest-only"] === "true") {
    const manifest = load(input.manifest);
    const errors = releaseManifestErrors(
      manifest,
      validateReleasePolicy(),
      input["allow-historical"] !== "true",
    );
    if (errors.length) throw new Error(errors.join("\n"));
    process.stdout.write(
      `Release manifest passed: ${manifest.releaseId} ${manifest.manifestDigest}.\n`,
    );
    return;
  }
  const decision = evaluateRelease({
    environmentName: input.environment,
    manifest: load(input.manifest),
    evidence: load(input.evidence),
    previousManifest: input.previous ? load(input.previous) : null,
  });
  if (input.output)
    writeFileSync(
      resolve(root, input.output),
      `${JSON.stringify(decision, null, 2)}\n`,
    );
  process.stdout.write(`${JSON.stringify(decision, null, 2)}\n`);
  if (decision.decision !== "PROMOTE") process.exitCode = 1;
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`release gate failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
