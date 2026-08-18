import { readFile } from "node:fs/promises";
import { readdir, stat } from "node:fs/promises";
import { extname, join, relative } from "node:path";

const root = join(process.cwd(), "apps/web/.svelte-kit/output/client");
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
