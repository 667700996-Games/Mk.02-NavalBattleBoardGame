import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const rustProtocol = await readFile(resolve(root, 'apps/server/src/protocol.rs'), 'utf8');
const rustLib = await readFile(resolve(root, 'apps/server/src/lib.rs'), 'utf8');
const typescript = await readFile(resolve(root, 'apps/web/src/lib/types.ts'), 'utf8');
const webProtocol = await readFile(resolve(root, 'apps/web/src/lib/protocol.ts'), 'utf8');
const manifestPath = resolve(root, 'contracts/protocol-v2.events.json');

function section(source, start, end) {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  if (from < 0 || to < 0) throw new Error(`Contract section missing: ${start}`);
  return source.slice(from, to);
}

function rustEvents(name, nextName) {
  const end = nextName === 'ProtocolError' ? 'pub struct ProtocolError' : `pub enum ${nextName}`;
  return [...section(rustProtocol, `pub enum ${name}`, end).matchAll(/serde\(rename = "([^"]+)"\)/g)]
    .map((match) => match[1])
    .sort();
}

function typescriptEvents(name, nextName) {
  const eventPattern = /'((?:[a-z]+(?::[a-z-]+)+)|heartbeat|error)'/g;
  const end = nextName === 'QUICK_COMMANDS' ? 'export const QUICK_COMMANDS' : `export type ${nextName}`;
  return [...new Set(
    [...section(typescript, `export type ${name}`, end).matchAll(eventPattern)]
      .map((match) => match[1])
  )].sort();
}

const rustVersion = Number(rustLib.match(/PROTOCOL_VERSION:\s*u16\s*=\s*(\d+)/)?.[1]);
const webVersion = Number(webProtocol.match(/GAME_PROTOCOL_VERSION\s*=\s*(\d+)/)?.[1]);
if (!Number.isInteger(rustVersion) || rustVersion !== webVersion) {
  throw new Error(`Protocol version drift: Rust=${rustVersion}, TypeScript=${webVersion}`);
}

const clientEvents = rustEvents('ClientEvent', 'ServerEvent');
const serverEvents = rustEvents('ServerEvent', 'ProtocolError');
const webClientEvents = typescriptEvents('ClientEvent', 'ServerEvent');
const webServerEvents = typescriptEvents('ServerEvent', 'QUICK_COMMANDS');
for (const [label, rust, web] of [
  ['client', clientEvents, webClientEvents],
  ['server', serverEvents, webServerEvents]
]) {
  if (JSON.stringify(rust) !== JSON.stringify(web)) {
    throw new Error(`${label} event drift:\nRust ${JSON.stringify(rust)}\nWeb  ${JSON.stringify(web)}`);
  }
}

const generated = `${JSON.stringify({ protocolVersion: rustVersion, clientEvents, serverEvents }, null, 2)}\n`;
if (process.argv.includes('--write')) {
  await writeFile(manifestPath, generated);
  console.log(`Wrote ${manifestPath}`);
} else {
  const committed = await readFile(manifestPath, 'utf8');
  if (committed !== generated) {
    throw new Error('Generated protocol manifest is stale; run npm run contract:generate');
  }
  console.log(`Protocol contract v${rustVersion} passed (${clientEvents.length} client, ${serverEvents.length} server events)`);
}
