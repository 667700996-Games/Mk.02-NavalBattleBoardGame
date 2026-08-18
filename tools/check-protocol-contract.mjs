import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const contractDirectory = resolve(root, "contracts");
const rustProtocol = await readFile(
  resolve(root, "apps/server/src/protocol.rs"),
  "utf8",
);
const rustLib = await readFile(resolve(root, "apps/server/src/lib.rs"), "utf8");
const typescript = await readFile(
  resolve(root, "apps/web/src/lib/types.ts"),
  "utf8",
);
const webProtocol = await readFile(
  resolve(root, "apps/web/src/lib/protocol.ts"),
  "utf8",
);
const checksumPath = resolve(contractDirectory, "checksums.sha256");

function section(source, start, end) {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  if (from < 0 || to < 0) throw new Error(`Contract section missing: ${start}`);
  return source.slice(from, to);
}

function numericConstant(source, name, fallbackName) {
  const direct = source.match(
    new RegExp(`${name}:?\\s*(?:u16\\s*)?=\\s*(\\d+)`),
  );
  if (direct) return Number(direct[1]);
  if (
    fallbackName &&
    new RegExp(`${name}:?[^=]*=\\s*${fallbackName}`).test(source)
  ) {
    return numericConstant(source, fallbackName);
  }
  return Number.NaN;
}

function rustEvents(name, nextName) {
  const end =
    nextName === "ProtocolError"
      ? "pub struct ProtocolError"
      : `pub enum ${nextName}`;
  return [
    ...section(rustProtocol, `pub enum ${name}`, end).matchAll(
      /serde\(rename = "([^"]+)"\)/g,
    ),
  ]
    .map((match) => match[1])
    .sort();
}

function typescriptEvents(name, nextName) {
  const eventPattern = /'((?:[a-z]+(?::[a-z-]+)+)|heartbeat|error)'/g;
  const end =
    nextName === "QUICK_COMMANDS"
      ? "export const QUICK_COMMANDS"
      : `export type ${nextName}`;
  return [
    ...new Set(
      [
        ...section(typescript, `export type ${name}`, end).matchAll(
          eventPattern,
        ),
      ].map((match) => match[1]),
    ),
  ].sort();
}

function capabilities(source, start, end, quotePattern) {
  return [...section(source, start, end).matchAll(quotePattern)].map(
    (match) => match[1],
  );
}

const rustVersion = numericConstant(rustLib, "PROTOCOL_VERSION");
const rustMinimum = numericConstant(rustLib, "MIN_SUPPORTED_PROTOCOL_VERSION");
const rustMaximum = numericConstant(
  rustLib,
  "MAX_SUPPORTED_PROTOCOL_VERSION",
  "PROTOCOL_VERSION",
);
const rustLegacy = numericConstant(rustLib, "LEGACY_DEFAULT_PROTOCOL_VERSION");
const rustWindow = numericConstant(
  rustProtocol,
  "PROTOCOL_COMPATIBILITY_WINDOW_DAYS",
);
const webVersion = numericConstant(webProtocol, "GAME_PROTOCOL_VERSION");
const webMinimum = numericConstant(
  webProtocol,
  "MIN_SUPPORTED_PROTOCOL_VERSION",
);
const webMaximum = numericConstant(
  webProtocol,
  "MAX_SUPPORTED_PROTOCOL_VERSION",
  "GAME_PROTOCOL_VERSION",
);
const webLegacy = numericConstant(
  webProtocol,
  "LEGACY_DEFAULT_PROTOCOL_VERSION",
);
const webWindow = numericConstant(
  webProtocol,
  "PROTOCOL_COMPATIBILITY_WINDOW_DAYS",
);
const rustCapabilities = capabilities(
  rustProtocol,
  "pub const PROTOCOL_CAPABILITIES: &[&str]",
  "];",
  /"([^"]+)"/g,
);
const webCapabilities = capabilities(
  webProtocol,
  "export const PROTOCOL_CAPABILITIES = [",
  "] as const",
  /'([^']+)'/g,
);

for (const [label, rust, web] of [
  ["current version", rustVersion, webVersion],
  ["minimum version", rustMinimum, webMinimum],
  ["maximum version", rustMaximum, webMaximum],
  ["legacy default", rustLegacy, webLegacy],
  ["compatibility window", rustWindow, webWindow],
]) {
  if (!Number.isInteger(rust) || rust !== web) {
    throw new Error(`Protocol ${label} drift: Rust=${rust}, TypeScript=${web}`);
  }
}
if (!(
  rustLegacy === rustMinimum &&
  rustVersion === rustMaximum &&
  rustMaximum - rustMinimum <= 1 &&
  rustWindow >= 30
)) {
  throw new Error(
    "Protocol support must retain at most one prior version for at least 30 days, default headerless clients to the oldest supported version, and end at current",
  );
}
if (
  JSON.stringify(rustCapabilities) !== JSON.stringify(webCapabilities) ||
  new Set(rustCapabilities).size !== rustCapabilities.length ||
  JSON.stringify(rustCapabilities) !==
    JSON.stringify([...rustCapabilities].sort())
) {
  throw new Error(
    "Protocol capabilities must be unique, sorted, and identical in Rust/TypeScript",
  );
}

const clientEvents = rustEvents("ClientEvent", "ServerEvent");
const serverEvents = rustEvents("ServerEvent", "ProtocolError");
const webClientEvents = typescriptEvents("ClientEvent", "ServerEvent");
const webServerEvents = typescriptEvents("ServerEvent", "QUICK_COMMANDS");
for (const [label, rust, web] of [
  ["client", clientEvents, webClientEvents],
  ["server", serverEvents, webServerEvents],
]) {
  if (JSON.stringify(rust) !== JSON.stringify(web)) {
    throw new Error(
      `${label} event drift:\nRust ${JSON.stringify(rust)}\nWeb  ${JSON.stringify(web)}`,
    );
  }
}

const manifestName = `protocol-v${rustVersion}.events.json`;
const manifestPath = resolve(contractDirectory, manifestName);
const generated = `${JSON.stringify(
  {
    protocolVersion: rustVersion,
    minimumSupportedVersion: rustMinimum,
    maximumSupportedVersion: rustMaximum,
    legacyDefaultVersion: rustLegacy,
    compatibilityWindowDays: rustWindow,
    capabilities: rustCapabilities,
    clientEvents,
    serverEvents,
  },
  null,
  2,
)}\n`;

const expectedChecksums = new Map(
  (await readFile(checksumPath, "utf8"))
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [checksum, file] = line.trim().split(/\s+/);
      return [file, checksum];
    }),
);

if (process.argv.includes("--write")) {
  const current = await readFile(manifestPath, "utf8").catch(() => null);
  if (
    current !== null &&
    current !== generated &&
    expectedChecksums.has(manifestName)
  ) {
    throw new Error(
      `Protocol V${rustVersion} is frozen; bump PROTOCOL_VERSION before generating`,
    );
  }
  await writeFile(manifestPath, generated);
  console.log(`Wrote ${manifestPath}`);
} else if ((await readFile(manifestPath, "utf8")) !== generated) {
  throw new Error(
    `Generated V${rustVersion} protocol manifest is stale or breaking`,
  );
}

const contractFiles = await readdir(contractDirectory);
for (let version = rustMinimum; version <= rustMaximum; version += 1) {
  for (const name of [
    `protocol-v${version}.events.json`,
    `protocol-v${version}.client-fixtures.json`,
  ]) {
    if (!contractFiles.includes(name)) {
      throw new Error(`Supported protocol artifact is missing: ${name}`);
    }
  }
}

const frozenFiles = contractFiles
  .filter((file) =>
    /^protocol-v\d+\.(events|client-fixtures)\.json$/.test(file),
  )
  .sort();
for (const file of frozenFiles) {
  const content = await readFile(resolve(contractDirectory, file));
  const actual = createHash("sha256").update(content).digest("hex");
  if (expectedChecksums.get(file) !== actual) {
    throw new Error(`${file}: frozen protocol checksum is missing or changed`);
  }
}
for (const file of expectedChecksums.keys()) {
  if (!frozenFiles.includes(file))
    throw new Error(`${file}: frozen protocol artifact was removed`);
}

for (let version = rustMinimum; version <= rustMaximum; version += 1) {
  const versionManifest = JSON.parse(
    await readFile(
      resolve(contractDirectory, `protocol-v${version}.events.json`),
      "utf8",
    ),
  );
  const fixtures = JSON.parse(
    await readFile(
      resolve(contractDirectory, `protocol-v${version}.client-fixtures.json`),
      "utf8",
    ),
  );
  const fixtureEvents = Array.isArray(fixtures)
    ? fixtures.map((fixture) => fixture?.type).sort()
    : [];
  const versionEvents = Array.isArray(versionManifest.clientEvents)
    ? [...versionManifest.clientEvents].sort()
    : [];
  if (
    versionManifest.protocolVersion !== version ||
    !Array.isArray(fixtures) ||
    fixtures.some(
      (fixture) =>
        typeof fixture !== "object" ||
        fixture === null ||
        typeof fixture.type !== "string" ||
        typeof fixture.payload !== "object" ||
        fixture.payload === null,
    ) ||
    new Set(fixtureEvents).size !== fixtureEvents.length ||
    JSON.stringify(fixtureEvents) !== JSON.stringify(versionEvents)
  ) {
    throw new Error(
      `V${version} client fixtures must cover its frozen client events exactly once`,
    );
  }
}

console.log(
  `Protocol contract V${rustMinimum}-V${rustMaximum} passed (${clientEvents.length} client, ${serverEvents.length} server events, ${rustCapabilities.length} capabilities)`,
);
