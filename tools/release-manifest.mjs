#!/usr/bin/env node
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function fileDigest(path) {
  return sha256(readFileSync(resolve(root, path)));
}

function material(name, paths) {
  const files = [...paths]
    .sort()
    .map((path) => ({ path, sha256: fileDigest(path) }));
  return {
    name,
    files,
    sha256: sha256(
      files.map((file) => `${file.path}\0${file.sha256}`).join("\n"),
    ),
  };
}

export function sourceMaterials() {
  const migrationDirectory = resolve(root, "apps/server/migrations");
  const migrations = readdirSync(migrationDirectory)
    .filter((name) => name.endsWith(".sql") || name === "checksums.sha256")
    .map((name) => relative(root, join(migrationDirectory, name)));
  return {
    migrations: material("migrations", migrations),
    protocolContracts: material("protocolContracts", [
      "contracts/protocol-v2.client-fixtures.json",
      "contracts/protocol-v2.events.json",
    ]),
    balanceRules: material("balanceRules", [
      "apps/server/src/domain/balance.rs",
    ]),
    cargoLock: material("cargoLock", ["Cargo.lock"]),
    packageLock: material("packageLock", ["package-lock.json"]),
  };
}

function argumentsFrom(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error(`invalid argument near ${key ?? "<end>"}`);
    }
    values[key.slice(2)] = value;
  }
  return values;
}

export function createReleaseManifest(input) {
  const policy = JSON.parse(
    readFileSync(resolve(root, "config/release-policy.json"), "utf8"),
  );
  const imagePattern = new RegExp(policy.artifact.imageDigestPattern);
  for (const component of policy.artifact.requiredComponents) {
    if (!imagePattern.test(input.images?.[component] ?? "")) {
      throw new Error(
        `${component} image must be a repository pinned by sha256 digest`,
      );
    }
  }
  if (!/^[a-f0-9]{40}$/.test(input.gitSha)) {
    throw new Error(
      "git SHA must contain exactly 40 lowercase hexadecimal characters",
    );
  }
  if (
    !Number.isSafeInteger(input.sourceDateEpoch) ||
    input.sourceDateEpoch < 1
  ) {
    throw new Error("source date epoch must be a positive integer");
  }
  for (const [name, value] of Object.entries(input.attestations ?? {})) {
    if (typeof value === "string" && value.trim()) continue;
    if (
      typeof value === "object" &&
      value !== null &&
      Object.values(value).every(
        (entry) => typeof entry === "string" && entry.trim(),
      )
    ) {
      continue;
    }
    throw new Error(`${name} attestation reference is required`);
  }
  const unsigned = {
    schemaVersion: policy.artifact.manifestSchemaVersion,
    releaseId: `${input.gitSha.slice(0, 12)}-${input.sourceDateEpoch}`,
    source: {
      gitSha: input.gitSha,
      sourceDateEpoch: input.sourceDateEpoch,
      sourceDate: new Date(input.sourceDateEpoch * 1000).toISOString(),
    },
    images: input.images,
    materials: sourceMaterials(),
    attestations: input.attestations,
  };
  return { ...unsigned, manifestDigest: releaseManifestDigest(unsigned) };
}

export function releaseManifestDigest(manifest) {
  const { manifestDigest: _ignored, ...unsigned } = manifest;
  return `sha256:${sha256(JSON.stringify(unsigned))}`;
}

function main() {
  const args = argumentsFrom(process.argv.slice(2));
  const gitSha =
    args["git-sha"] ??
    execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: root,
      encoding: "utf8",
    }).trim();
  const sourceDateEpoch = Number(
    args["source-date-epoch"] ??
      execFileSync("git", ["show", "-s", "--format=%ct", gitSha], {
        cwd: root,
        encoding: "utf8",
      }).trim(),
  );
  const manifest = createReleaseManifest({
    gitSha,
    sourceDateEpoch,
    images: { server: args["server-image"], web: args["web-image"] },
    attestations: {
      sbom: { server: args["server-sbom"], web: args["web-sbom"] },
      provenance: args.provenance,
      keylessSignature: args.signature,
    },
  });
  const output = args.output;
  if (!output) throw new Error("--output is required");
  writeFileSync(
    resolve(root, output),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  process.stdout.write(`${manifest.releaseId} ${manifest.manifestDigest}\n`);
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`release manifest failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
