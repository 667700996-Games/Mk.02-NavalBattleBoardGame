import { readdir, stat } from 'node:fs/promises';
import { extname, join, relative } from 'node:path';

const root = join(process.cwd(), 'apps/web/.svelte-kit/output/client');
const limits = {
  '.js': { perFile: 100_000, total: 300_000 },
  '.css': { perFile: 90_000, total: 180_000 },
  '.woff2': { perFile: 550_000, total: 1_200_000 }
};
const totals = new Map(Object.keys(limits).map((extension) => [extension, 0]));
const failures = [];

async function walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      await walk(path);
      continue;
    }
    const extension = extname(entry.name);
    const bytes = (await stat(path)).size;
    if (extension === '.woff') {
      failures.push(`${relative(root, path)} uses legacy WOFF (${bytes} bytes)`);
      continue;
    }
    const budget = limits[extension];
    if (!budget) continue;
    totals.set(extension, (totals.get(extension) ?? 0) + bytes);
    if (bytes > budget.perFile) {
      failures.push(
        `${relative(root, path)} is ${bytes} bytes; ${extension} file budget is ${budget.perFile}`
      );
    }
  }
}

await walk(root);
for (const [extension, bytes] of totals) {
  const budget = limits[extension];
  if (bytes > budget.total) {
    failures.push(`${extension} total is ${bytes} bytes; budget is ${budget.total}`);
  }
}

if (failures.length) {
  console.error(`Bundle budget failed:\n- ${failures.join('\n- ')}`);
  process.exit(1);
}
console.log(
  `Bundle budget passed: ${[...totals].map(([extension, bytes]) => `${extension}=${bytes}`).join(', ')}`
);
