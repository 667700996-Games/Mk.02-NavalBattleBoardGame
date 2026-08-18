import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const load = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));
const policy = load("config/localization-policy.json");
const errors = [];
const fail = (condition, message) => {
  if (!condition) errors.push(message);
};

fail(policy.schemaVersion === 1, "localization policy schemaVersion must be 1");
fail(
  JSON.stringify(policy.launchLocales) === JSON.stringify(["ko-KR", "en-US"]),
  "launch locales must contain Korean and English in deterministic order",
);
fail(policy.defaultLocale === "ko-KR", "Korean must remain the default locale");
fail(
  policy.pseudoLocale === "en-XA",
  "en-XA must be the automated pseudo locale",
);

const catalogs = Object.fromEntries(
  policy.launchLocales.map((locale) => [
    locale,
    load(`${policy.catalogDirectory}/${locale}.json`),
  ]),
);
const referenceKeys = Object.keys(catalogs[policy.defaultLocale]).sort();
fail(
  referenceKeys.length > 0,
  "default localization catalog must not be empty",
);

const placeholders = (message) =>
  [...message.matchAll(/\{([a-zA-Z][a-zA-Z0-9]*)\}/g)]
    .map((match) => match[1])
    .sort();

for (const [locale, catalog] of Object.entries(catalogs)) {
  const keys = Object.keys(catalog).sort();
  fail(
    JSON.stringify(keys) === JSON.stringify(referenceKeys),
    `${locale} catalog keys differ from ${policy.defaultLocale}`,
  );
  for (const key of referenceKeys) {
    const message = catalog[key];
    fail(
      typeof message === "string" && message.trim().length > 0,
      `${locale}:${key} is empty`,
    );
    fail(!/[<>]/.test(message ?? ""), `${locale}:${key} must not contain HTML`);
    fail(
      JSON.stringify(placeholders(message ?? "")) ===
        JSON.stringify(placeholders(catalogs[policy.defaultLocale][key] ?? "")),
      `${locale}:${key} placeholder contract differs from ${policy.defaultLocale}`,
    );
    if (locale === "en-US" && key !== "locale.koKR") {
      fail(
        !/[가-힣]/.test(message ?? ""),
        `en-US:${key} still contains Korean copy`,
      );
    }
  }
}

const runtime = readFileSync(
  resolve(root, "apps/web/src/lib/i18n/index.ts"),
  "utf8",
);
for (const formatter of policy.requiredIntlFormatters) {
  fail(
    runtime.includes(`Intl.${formatter}`),
    `runtime must use Intl.${formatter}`,
  );
}
fail(
  runtime.includes("pseudoLocalize"),
  "runtime must expose deterministic pseudolocalization",
);
fail(
  runtime.includes(`source.length * ${policy.pseudoPaddingRatio}`),
  "pseudo runtime padding must match the policy ratio",
);
fail(
  1 + policy.pseudoPaddingRatio >= policy.minimumPseudoExpansion,
  "pseudo padding ratio cannot satisfy the minimum expansion floor",
);

const technicalMarkup = [
  /^MK\.01$/,
  /^MK01-NCS$/,
  /^SEOUL \/ KR$/,
  /^SYS$/,
  /^VS$/,
  /^AI$/,
  /^RP$/,
  /^SHA-256$/,
  /^ABC123$/,
  /^npm run dev$/,
  /^\d{3}\u00b0$/,
  /^\d{2}\u00b0\d{2}' [NS] \/ \d{3}\u00b0\d{2}' [EW]$/,
  /^SECTOR [A-Z0-9-]+$/,
  /^\d{3} NM$/,
  /^(?:TGT|FRD|UNK)-\d{2}$/,
  /^\d{3}\u00b0 \/ \d+(?:\.\d+)? NM$/,
];
const isTechnicalMarkup = (value) =>
  technicalMarkup.some((pattern) => pattern.test(value));
const sourceFiles = policy.localizedSourceRoots.flatMap((directory) =>
  readdirSync(resolve(root, directory), { recursive: true })
    .filter(
      (entry) => typeof entry === "string" && /\.(?:svelte|ts)$/.test(entry),
    )
    .filter((entry) => !/\.test\.ts$/.test(entry))
    .map((entry) => resolve(root, directory, entry)),
);

for (const path of sourceFiles) {
  const source = readFileSync(path, "utf8");
  const displayPath = path.slice(root.length + 1);
  fail(
    !/[\uac00-\ud7a3]/.test(source),
    `${displayPath} contains hard-coded Korean copy`,
  );
  if (!path.endsWith(".svelte")) continue;

  const markup = (
    source.includes("</script>")
      ? source.split("</script>").slice(1).join("</script>")
      : source
  )
    .split("<style>")[0]
    .replace(/<!--[\s\S]*?-->/g, "");
  for (const match of markup.matchAll(
    /\b(?:aria-label|title|placeholder|alt|eyebrow|description|label)=["']([^"'{}]+)["']/g,
  )) {
    const literal = match[1].trim();
    fail(
      !/[A-Za-z]/.test(literal) || isTechnicalMarkup(literal),
      `${displayPath} contains hard-coded user-facing attribute: ${literal}`,
    );
  }
  for (const match of markup.matchAll(/>([^<>{}]*[A-Za-z][^<>{}]*)</g)) {
    const literal = match[1].replace(/\s+/g, " ").trim();
    if (!literal) continue;
    fail(
      isTechnicalMarkup(literal),
      `${displayPath} contains hard-coded user-facing text: ${literal}`,
    );
  }
}

const appCss = readFileSync(resolve(root, "apps/web/src/app.css"), "utf8");
for (const font of policy.requiredFallbackFonts) {
  fail(appCss.includes(font), `global font stack is missing ${font}`);
}
const fontGenerator = readFileSync(
  resolve(root, "tools/check-font-subsets.mjs"),
  "utf8",
);
fail(
  fontGenerator.includes("json|rs|svelte|ts"),
  "font subset input must include localized JSON catalogs",
);

const viteConfig = readFileSync(
  resolve(root, "apps/web/vite.config.ts"),
  "utf8",
);
fail(
  viteConfig.includes("manualChunks") &&
    viteConfig.includes("locale-${locale}") &&
    policy.launchLocales.every((locale) => viteConfig.includes(locale)),
  "every launch catalog must build into a dedicated locale chunk",
);
const performanceBudgets = load("config/performance-budgets.json");
const expectedLocaleChunks = policy.launchLocales.map(
  (locale) => `locale-${locale}`,
);
fail(
  JSON.stringify(performanceBudgets.localeBundles?.required) ===
    JSON.stringify(expectedLocaleChunks),
  "locale bundle budget must cover the exact launch locale set",
);
fail(
  performanceBudgets.localeBundles?.perFileBytes > 0 &&
    performanceBudgets.localeBundles?.totalBytes > 0,
  "locale chunks must have positive per-file and aggregate byte budgets",
);

if (errors.length) {
  throw new Error(errors.join("\n"));
}
console.log(
  `Localization catalog gate passed: ${referenceKeys.length} keys across ${policy.launchLocales.length} launch locales, ${policy.pseudoLocale} pseudo locale.`,
);
