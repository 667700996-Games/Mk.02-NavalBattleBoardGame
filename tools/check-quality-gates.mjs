import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const policyPath = path.join(root, "config/quality-gates.json");
const policy = JSON.parse(fs.readFileSync(policyPath, "utf8"));
const rootPackage = JSON.parse(
  fs.readFileSync(path.join(root, "package.json"), "utf8"),
);
const architectureOwnership = JSON.parse(
  fs.readFileSync(
    path.join(root, ".github/architecture-ownership.json"),
    "utf8",
  ),
);
const validOwnerRoles = new Set(
  architectureOwnership.roles.map((role) => role.id),
);
const failures = [];
const requiredSuites = [
  "component",
  "accessibility",
  "visualRegression",
  "property",
  "fuzz",
  "load",
  "soak",
  "chaos",
];

function fail(message) {
  failures.push(message);
}

function relativeExists(relativePath) {
  return fs.existsSync(path.join(root, relativePath));
}

function scriptFromCommand(command) {
  return /^npm run ([\w:-]+)$/.exec(command)?.[1] ?? null;
}

if (policy.schemaVersion !== 1) fail("quality policy schemaVersion must be 1");
if (!validOwnerRoles.has(policy.policyOwner))
  fail("quality policy must name a registered owner");
if (
  !Number.isInteger(policy.reviewCadenceDays) ||
  policy.reviewCadenceDays > 90
) {
  fail(
    "quality policy review cadence must be an integer no greater than 90 days",
  );
}

for (const name of requiredSuites) {
  const suite = policy.suites?.[name];
  if (!suite) {
    fail(`missing required ${name} suite policy`);
    continue;
  }
  if (!validOwnerRoles.has(suite.owner))
    fail(`${name} suite has no registered owner`);
  if (!relativeExists(suite.source))
    fail(`${name} suite source is missing: ${suite.source}`);
  if (!suite.threshold || Object.keys(suite.threshold).length === 0) {
    fail(`${name} suite has no owned threshold`);
  }
  const commandScript = scriptFromCommand(suite.command);
  if (!commandScript || !rootPackage.scripts?.[commandScript]) {
    fail(`${name} suite command is not a root npm script: ${suite.command}`);
  }
}

const componentSource = fs.readFileSync(
  path.join(root, policy.suites.component.source),
  "utf8",
);
const componentTests = (componentSource.match(/\bit\(/g) ?? []).length;
if (componentTests < policy.suites.component.threshold.minimumBehaviorTests) {
  fail(`component suite has ${componentTests} behavior tests`);
}

const accessibilitySource = fs.readFileSync(
  path.join(root, policy.suites.accessibility.source),
  "utf8",
);
if (
  !accessibilitySource.includes("expect(result.violations") ||
  !accessibilitySource.includes("toEqual([])")
) {
  fail("accessibility suite does not fail on every discovered violation");
}
const auditedStates = Math.max(
  0,
  (accessibilitySource.match(/await audit\(/g) ?? []).length,
);
if (
  auditedStates < policy.suites.accessibility.threshold.minimumAuditedStates
) {
  fail(`accessibility suite audits only ${auditedStates} states`);
}

const baselineDirectory = path.join(
  root,
  "apps/web/e2e/visual-regression.spec.ts-snapshots",
);
const approvedBaselines = fs.existsSync(baselineDirectory)
  ? fs.readdirSync(baselineDirectory).filter((name) => name.endsWith(".png"))
      .length
  : 0;
if (
  approvedBaselines !==
  policy.suites.visualRegression.threshold.approvedBaselines
) {
  fail(
    `visual suite has ${approvedBaselines} baselines; expected ${policy.suites.visualRegression.threshold.approvedBaselines}`,
  );
}

const propertySource = fs.readFileSync(
  path.join(root, policy.suites.property.source),
  "utf8",
);
const configuredPropertyCases = Number(
  /const PROPERTY_CASES: u64 = ([\d_]+);/
    .exec(propertySource)?.[1]
    .replaceAll("_", ""),
);
if (
  configuredPropertyCases !==
  policy.suites.property.threshold.generatedPlacementCases
) {
  fail("property case threshold does not match the executable test constant");
}
const attackPermutations = Number(
  /for case in 0\.\.([\d_]+)/.exec(propertySource)?.[1],
);
if (
  attackPermutations !== policy.suites.property.threshold.attackPermutations
) {
  fail("attack-permutation threshold does not match the executable test loop");
}

const corpusDirectory = path.join(root, "fuzz/corpus/protocol_json");
const corpusSeeds = fs.existsSync(corpusDirectory)
  ? fs.readdirSync(corpusDirectory).filter((name) => !name.startsWith("."))
      .length
  : 0;
if (corpusSeeds < policy.suites.fuzz.threshold.minimumCorpusSeeds) {
  fail(`fuzz corpus has ${corpusSeeds} seeds`);
}

const visualConfig = fs.readFileSync(
  path.join(root, "apps/web/playwright.visual.config.ts"),
  "utf8",
);
if (
  !visualConfig.includes(
    `maxDiffPixelRatio: ${policy.suites.visualRegression.threshold.maximumDiffPixelRatio}`,
  )
) {
  fail("visual diff threshold does not match the Playwright configuration");
}

const fuzzRunner = fs.readFileSync(
  path.join(root, "scripts/run-protocol-fuzz.sh"),
  "utf8",
);
if (
  !fuzzRunner.includes(
    `FUZZ_SECONDS:-${policy.suites.fuzz.threshold.pullRequestSeconds}`,
  ) ||
  !fuzzRunner.includes(
    `-timeout=${policy.suites.fuzz.threshold.timeoutSeconds}`,
  )
) {
  fail("fuzz thresholds do not match the executable runner");
}

const reliabilitySource = fs.readFileSync(
  path.join(root, policy.suites.load.source),
  "utf8",
);
for (const token of [
  `p(95)<${policy.suites.load.threshold.p95Milliseconds}`,
  `p(99)<${policy.suites.load.threshold.p99Milliseconds}`,
  `rate<${policy.suites.load.threshold.maximumFailureRate}`,
]) {
  if (!reliabilitySource.includes(token))
    fail(`reliability runner is missing threshold token: ${token}`);
}
const vusPattern = new RegExp(
  `profile === ["']soak["'] \\? ${policy.suites.soak.threshold.virtualUsers} : ${policy.suites.load.threshold.virtualUsers}`,
);
const durationPattern = new RegExp(
  `profile === ["']soak["'] \\? ["']${policy.suites.soak.threshold.minimumMinutes}m["'] : ["']${policy.suites.load.threshold.steadySeconds}s["']`,
);
if (!vusPattern.test(reliabilitySource) || !durationPattern.test(reliabilitySource)) {
  fail("load/soak duration or virtual-user threshold does not match the executable runner");
}

const chaosSource = fs.readFileSync(
  path.join(root, policy.suites.chaos.source),
  "utf8",
);
if (
  !chaosSource.includes(
    `CHAOS_RECOVERY_BUDGET_MS:-${policy.suites.chaos.threshold.maximumRecoveryMilliseconds}`,
  )
) {
  fail("chaos recovery threshold does not match the executable drill");
}

for (const [platform, coverage] of Object.entries(policy.coverage ?? {}).filter(
  ([name]) => name === "web" || name === "rust",
)) {
  if (!validOwnerRoles.has(coverage.owner))
    fail(`${platform} coverage has no registered owner`);
  if (
    !coverage.total ||
    !coverage.files ||
    Object.keys(coverage.files).length === 0
  ) {
    fail(`${platform} coverage lacks risk-based total and file thresholds`);
  }
  const commandScript = scriptFromCommand(coverage.command);
  if (!commandScript || !rootPackage.scripts?.[commandScript]) {
    fail(`${platform} coverage command is not executable from the root`);
  }
  for (const file of Object.keys(coverage.files ?? {})) {
    if (!relativeExists(file))
      fail(`${platform} coverage target is missing: ${file}`);
  }
}

if (
  !Array.isArray(policy.coverage?.knownRiskGaps) ||
  policy.coverage.knownRiskGaps.length === 0
) {
  fail("coverage policy must identify known behavior gaps");
} else {
  for (const gap of policy.coverage.knownRiskGaps) {
    const reviewAt = Date.parse(`${gap.reviewBy}T23:59:59Z`);
    if (
      !relativeExists(gap.path) ||
      !gap.behavior ||
      !gap.compensatingSuite ||
      !validOwnerRoles.has(gap.owner) ||
      !Number.isFinite(reviewAt) ||
      reviewAt < Date.now()
    ) {
      fail(`invalid known coverage gap: ${gap.path ?? "unknown"}`);
    }
  }
}

const workflowText = [
  ".github/workflows/ci.yml",
  ".github/workflows/reliability.yml",
]
  .filter(relativeExists)
  .map((file) => fs.readFileSync(path.join(root, file), "utf8"))
  .join("\n");
for (const command of [
  "test:coverage:web",
  "test:coverage:rust",
  "test:fuzz",
  "test:visual",
  "test:load",
  "test:soak",
  "test:chaos",
]) {
  if (!workflowText.includes(command))
    fail(`CI does not schedule npm run ${command}`);
}
if (
  !workflowText.includes(
    `FUZZ_SECONDS: "${policy.suites.fuzz.threshold.scheduledSeconds}"`,
  )
) {
  fail("scheduled fuzz duration does not match the quality policy");
}
if (
  !workflowText.includes(
    `RELIABILITY_DURATION: ${policy.suites.soak.threshold.minimumMinutes}m`,
  )
) {
  fail("scheduled soak duration does not match the quality policy");
}

function findSummaryBySuffix(summary, suffix) {
  return Object.entries(summary).find(([name]) =>
    name.replaceAll("\\", "/").endsWith(suffix),
  )?.[1];
}

function checkMetric(label, actual, minimum) {
  if (typeof actual !== "number" || actual + Number.EPSILON < minimum) {
    fail(`${label} is ${actual ?? "missing"}%; minimum is ${minimum}%`);
  }
}

if (process.argv.includes("--coverage")) {
  const webReportPath = path.join(root, policy.coverage.web.report);
  const rustReportPath = path.join(root, policy.coverage.rust.report);
  if (!fs.existsSync(webReportPath))
    fail(`missing web coverage report: ${policy.coverage.web.report}`);
  if (!fs.existsSync(rustReportPath))
    fail(`missing Rust coverage report: ${policy.coverage.rust.report}`);

  if (fs.existsSync(webReportPath)) {
    const report = JSON.parse(fs.readFileSync(webReportPath, "utf8"));
    for (const [metric, minimum] of Object.entries(policy.coverage.web.total)) {
      checkMetric(`web total ${metric}`, report.total?.[metric]?.pct, minimum);
    }
    for (const [file, thresholds] of Object.entries(
      policy.coverage.web.files,
    )) {
      const summary = findSummaryBySuffix(report, file);
      for (const [metric, minimum] of Object.entries(thresholds)) {
        checkMetric(`${file} ${metric}`, summary?.[metric]?.pct, minimum);
      }
    }
  }

  if (fs.existsSync(rustReportPath)) {
    const report = JSON.parse(fs.readFileSync(rustReportPath, "utf8"))
      .data?.[0];
    for (const [metric, minimum] of Object.entries(
      policy.coverage.rust.total,
    )) {
      checkMetric(
        `Rust total ${metric}`,
        report?.totals?.[metric]?.percent,
        minimum,
      );
    }
    for (const [file, thresholds] of Object.entries(
      policy.coverage.rust.files,
    )) {
      const summary = report?.files?.find((entry) =>
        entry.filename.replaceAll("\\", "/").endsWith(file),
      )?.summary;
      for (const [metric, minimum] of Object.entries(thresholds)) {
        checkMetric(`${file} ${metric}`, summary?.[metric]?.percent, minimum);
      }
    }
  }
}

if (failures.length > 0) {
  console.error("Quality gate policy failed:");
  failures.forEach((message) => console.error(`- ${message}`));
  process.exit(1);
}

console.log(
  `Quality gate policy passed: ${requiredSuites.length} owned suites, ${Object.keys(policy.coverage.web.files).length} web and ${Object.keys(policy.coverage.rust.files).length} Rust risk targets.`,
);
