import { readFile } from "node:fs/promises";
import { readdir, stat } from "node:fs/promises";
import { extname, join, relative } from "node:path";

const root = join(process.cwd(), "apps/web/.svelte-kit/output/client");
const generatedClientRoot = join(
  process.cwd(),
  "apps/web/.svelte-kit/generated/client",
);
const budgetConfig = JSON.parse(
  await readFile(
    join(process.cwd(), "config/performance-budgets.json"),
    "utf8",
  ),
);
const categories = Object.entries(budgetConfig.artifact);
const extensionCategory = new Map(
  categories.flatMap(([name, budget]) =>
    budget.extensions.map((extension) => [extension, { name, budget }]),
  ),
);
const totals = new Map(categories.map(([name]) => [name, 0]));
const failures = [];
const routeEntryResults = [];

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function bytesFor(relativePath) {
  return (await stat(join(root, relativePath))).size;
}

async function checkRouteEntries() {
  const routeEntries = Object.entries(budgetConfig.routeEntries ?? {});
  if (!routeEntries.length) return;

  const [appSource, viteManifestSource] = await Promise.all([
    readFile(join(generatedClientRoot, "app.js"), "utf8"),
    readFile(join(root, ".vite/manifest.json"), "utf8"),
  ]);
  const viteManifest = JSON.parse(viteManifestSource);

  for (const [routeId, budget] of routeEntries) {
    if (
      !Number.isSafeInteger(budget.javascriptBytes) ||
      budget.javascriptBytes <= 0 ||
      !Number.isSafeInteger(budget.cssBytes) ||
      budget.cssBytes <= 0
    ) {
      failures.push(
        `${routeId} route entry budgets must be positive integer byte counts`,
      );
      continue;
    }

    const routePattern = new RegExp(
      `${escapeRegExp(JSON.stringify(routeId))}:\\s*\\[(\\d+)`,
    );
    const routeMatch = appSource.match(routePattern);
    if (!routeMatch) {
      failures.push(`${routeId} is not present in the generated route dictionary`);
      continue;
    }

    const nodeIndex = Number(routeMatch[1]);
    const manifestKey = `.svelte-kit/generated/client-optimized/nodes/${nodeIndex}.js`;
    const entry = viteManifest[manifestKey];
    if (!entry) {
      failures.push(`${routeId} has no client manifest entry (${manifestKey})`);
      continue;
    }

    const javascriptBytes = await bytesFor(entry.file);
    const cssBytes = (
      await Promise.all((entry.css ?? []).map((path) => bytesFor(path)))
    ).reduce((total, bytes) => total + bytes, 0);
    routeEntryResults.push({ routeId, javascriptBytes, cssBytes });

    if (javascriptBytes > budget.javascriptBytes) {
      failures.push(
        `${routeId} entry JavaScript is ${javascriptBytes} bytes; budget is ${budget.javascriptBytes}`,
      );
    }
    if (cssBytes > budget.cssBytes) {
      failures.push(
        `${routeId} entry CSS is ${cssBytes} bytes; budget is ${budget.cssBytes}`,
      );
    }
  }
}

async function walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      await walk(path);
      continue;
    }
    const extension = extname(entry.name);
    const bytes = (await stat(path)).size;
    if (extension === ".woff") {
      failures.push(
        `${relative(root, path)} uses legacy WOFF (${bytes} bytes)`,
      );
      continue;
    }
    const category = extensionCategory.get(extension);
    if (!category) continue;
    totals.set(category.name, (totals.get(category.name) ?? 0) + bytes);
    if (bytes > category.budget.perFileBytes) {
      failures.push(
        `${relative(root, path)} is ${bytes} bytes; ${category.name} file budget is ${category.budget.perFileBytes}`,
      );
    }
  }
}

await walk(root);
await checkRouteEntries();
for (const [name, bytes] of totals) {
  const budget = budgetConfig.artifact[name];
  if (bytes > budget.totalBytes) {
    failures.push(
      `${name} total is ${bytes} bytes; budget is ${budget.totalBytes}`,
    );
  }
}

if (failures.length) {
  console.error(`Bundle budget failed:\n- ${failures.join("\n- ")}`);
  process.exit(1);
}
console.log(
  `Bundle budget passed: ${[...totals].map(([name, bytes]) => `${name}=${bytes}`).join(", ")}`,
);
for (const result of routeEntryResults) {
  console.log(
    `Route entry passed: ${result.routeId} javascript=${result.javascriptBytes}, css=${result.cssBytes}`,
  );
}
