import { readFile, readdir } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const ownership = JSON.parse(
  await readFile(resolve(root, ".github/architecture-ownership.json"), "utf8"),
);
const architecture = await readFile(
  resolve(root, "docs/ARCHITECTURE.md"),
  "utf8",
);
const codeowners = await readFile(resolve(root, ".github/CODEOWNERS"), "utf8");
const pullRequestTemplate = await readFile(
  resolve(root, ".github/pull_request_template.md"),
  "utf8",
);
const packageDocument = JSON.parse(
  await readFile(resolve(root, "package.json"), "utf8"),
);
const ciWorkflow = await readFile(
  resolve(root, ".github/workflows/ci.yml"),
  "utf8",
);
const failures = [];

function fail(message) {
  failures.push(message);
}

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

function sameSet(left, right) {
  return (
    JSON.stringify(sortedUnique(left)) === JSON.stringify(sortedUnique(right))
  );
}

function lineCount(source) {
  return source.length === 0 ? 0 : source.replace(/\n$/, "").split("\n").length;
}

const ignoredDirectories = new Set([
  ".git",
  ".svelte-kit",
  "build",
  "node_modules",
  "playwright-report",
  "target",
  "test-results",
]);

async function collectFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await collectFiles(path)));
    else if (entry.isFile())
      files.push(relative(root, path).split(sep).join("/"));
  }
  return files;
}

function boundaryOwns(boundary, file) {
  return boundary.pathPrefixes.some((prefix) =>
    prefix.endsWith("/") ? file.startsWith(prefix) : file === prefix,
  );
}

function expectedCodeownersPattern(prefix) {
  return prefix.endsWith("/") ? `/${prefix}**` : `/${prefix}`;
}

function crateTargets(source) {
  const targets = new Set(
    [...source.matchAll(/crate::([a-z_][a-z0-9_]*)/g)].map((match) => match[1]),
  );
  for (const match of source.matchAll(/use\s+crate::\{([\s\S]*?)\};/g)) {
    for (const target of [
      "api",
      "app",
      "domain",
      "error",
      "protocol",
      "store",
      "ws",
    ]) {
      if (new RegExp(`(?:^|[,\\n])\\s*${target}\\s*::`, "m").test(match[1]))
        targets.add(target);
    }
  }
  return targets;
}

if (ownership.schemaVersion !== 1)
  fail("architecture ownership schemaVersion must be 1");
if (!/^@[A-Za-z0-9-]+$/.test(ownership.defaultReviewerAccount ?? "")) {
  fail("defaultReviewerAccount must be a bounded GitHub account");
}

const roles = new Map();
for (const role of ownership.roles ?? []) {
  if (!/^[a-z][a-z0-9-]+$/.test(role.id ?? "") || roles.has(role.id)) {
    fail(`invalid or duplicate architecture role: ${role.id}`);
    continue;
  }
  if (!/^@[A-Za-z0-9-]+$/.test(role.account ?? "") || !role.responsibility) {
    fail(`${role.id}: account and responsibility are required`);
  }
  roles.set(role.id, role);
}

const boundaries = new Map();
const allPrefixes = new Set();
for (const boundary of ownership.boundaries ?? []) {
  if (
    !/^[a-z][a-z0-9-]+$/.test(boundary.id ?? "") ||
    boundaries.has(boundary.id)
  ) {
    fail(`invalid or duplicate architecture boundary: ${boundary.id}`);
    continue;
  }
  if (!roles.has(boundary.ownerRole) || !boundary.summary) {
    fail(`${boundary.id}: valid ownerRole and summary are required`);
  }
  if (
    !Array.isArray(boundary.reviewerRoles) ||
    boundary.reviewerRoles.length === 0
  ) {
    fail(`${boundary.id}: reviewerRoles are required`);
  }
  for (const role of boundary.reviewerRoles ?? []) {
    if (!roles.has(role)) fail(`${boundary.id}: unknown reviewer role ${role}`);
  }
  if (
    !Array.isArray(boundary.pathPrefixes) ||
    boundary.pathPrefixes.length === 0
  ) {
    fail(`${boundary.id}: pathPrefixes are required`);
  }
  for (const prefix of boundary.pathPrefixes ?? []) {
    if (
      typeof prefix !== "string" ||
      prefix.startsWith("/") ||
      prefix.includes("..")
    ) {
      fail(`${boundary.id}: unsafe path prefix ${prefix}`);
    }
    if (allPrefixes.has(prefix))
      fail(`${prefix}: path prefix is assigned more than once`);
    allPrefixes.add(prefix);
  }
  if (!architecture.includes(`\`${boundary.id}\``)) {
    fail(`${boundary.id}: boundary is absent from docs/ARCHITECTURE.md`);
  }
  boundaries.set(boundary.id, boundary);
}

const criticalDirectories = [
  ".github",
  "apps/server",
  "apps/web",
  "config",
  "contracts",
  "deploy",
  "docs",
  "fuzz",
  "ops",
  "scripts",
  "tests",
  "tools",
];
const criticalRootFiles = [
  ".dockerignore",
  ".editorconfig",
  ".env.example",
  ".gitignore",
  "Cargo.lock",
  "Cargo.toml",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "compose.yaml",
  "deny.toml",
  "package-lock.json",
  "package.json",
];
const criticalFiles = [
  ...(
    await Promise.all(
      criticalDirectories.map((directory) =>
        collectFiles(resolve(root, directory)),
      ),
    )
  ).flat(),
  ...criticalRootFiles,
].sort();
for (const file of criticalFiles) {
  const matches = [...boundaries.values()].filter((boundary) =>
    boundaryOwns(boundary, file),
  );
  if (matches.length !== 1) {
    fail(
      `${file}: expected exactly one owner boundary, found ${matches.map(({ id }) => id).join(", ") || "none"}`,
    );
  }
}

const codeownerEntries = codeowners
  .split("\n")
  .map((line) => line.trim())
  .filter((line) => line && !line.startsWith("#"))
  .map((line) => line.split(/\s+/));
if (
  !codeownerEntries.some(
    ([pattern, ...accounts]) =>
      pattern === "*" && accounts.includes(ownership.defaultReviewerAccount),
  )
) {
  fail("CODEOWNERS must have the configured default reviewer");
}
for (const boundary of boundaries.values()) {
  const ownerAccount = roles.get(boundary.ownerRole)?.account;
  for (const prefix of boundary.pathPrefixes) {
    const expectedPattern = expectedCodeownersPattern(prefix);
    const entry = codeownerEntries.find(
      ([pattern]) => pattern === expectedPattern,
    );
    if (!entry || !entry.slice(1).includes(ownerAccount)) {
      fail(
        `${prefix}: CODEOWNERS must assign ${ownerAccount} with ${expectedPattern}`,
      );
    }
  }
}

for (const field of ["minimumApprovals", "crossBoundaryMinimumApprovals"]) {
  if (
    !Number.isInteger(ownership.reviewPolicy?.[field]) ||
    ownership.reviewPolicy[field] < 1
  ) {
    fail(`reviewPolicy.${field} must be a positive integer`);
  }
}
if (
  ownership.reviewPolicy?.crossBoundaryMinimumApprovals <
  ownership.reviewPolicy?.minimumApprovals
) {
  fail(
    "cross-boundary review cannot require fewer approvals than a single-boundary change",
  );
}
for (const field of ["independentApprovalRequiredFor", "adrRequiredFor"]) {
  const values = ownership.reviewPolicy?.[field];
  if (
    !Array.isArray(values) ||
    values.length < 3 ||
    new Set(values).size !== values.length
  ) {
    fail(`reviewPolicy.${field} must contain unique governed change classes`);
  }
}

const decisionIds = [];
for (const decision of ownership.decisionRecords ?? []) {
  decisionIds.push(decision.id);
  const source = await readFile(resolve(root, decision.path), "utf8").catch(
    () => null,
  );
  if (!source) {
    fail(`${decision.id}: ADR file is missing at ${decision.path}`);
    continue;
  }
  if (
    !source.startsWith(`# ${decision.id}:`) ||
    !source.includes("- Status: Accepted")
  ) {
    fail(`${decision.id}: ADR title/status is invalid`);
  }
  for (const metadata of [
    "Date",
    "Decision owner",
    "Reviewer roles",
    "Last reviewed",
  ]) {
    if (!source.includes(`- ${metadata}:`))
      fail(`${decision.id}: missing ${metadata}`);
  }
  for (const heading of [
    "Context",
    "Decision",
    "Rejected alternatives",
    "Consequences",
    "Verification",
    "Review triggers",
  ]) {
    if (!source.includes(`## ${heading}`))
      fail(`${decision.id}: missing ${heading}`);
  }
  if (!architecture.includes(`\`${decision.id}\``)) {
    fail(`${decision.id}: accepted decision is absent from architecture index`);
  }
}
if (
  decisionIds.length === 0 ||
  new Set(decisionIds).size !== decisionIds.length
) {
  fail("decisionRecords must contain unique ADR IDs");
}

const baseline = JSON.parse(
  await readFile(resolve(root, ownership.baselineReview), "utf8").catch(
    () => "{}",
  ),
);
if (
  baseline.schemaVersion !== 1 ||
  !/^ARCH-REVIEW-\d{4}-\d{2}-\d{2}-[A-Z0-9-]+$/.test(baseline.reviewId ?? "") ||
  !/^[0-9a-f]{7,40}$/.test(baseline.baseCommit ?? "") ||
  baseline.outcome !== "accepted-with-tracked-debt"
) {
  fail("baseline architecture review metadata or outcome is invalid");
}
if (
  !roles.has(baseline.reviewer?.role) ||
  baseline.reviewer?.account !== roles.get(baseline.reviewer?.role)?.account
) {
  fail("baseline architecture reviewer must map to a configured role/account");
}
if (!sameSet(baseline.reviewedBoundaries ?? [], [...boundaries.keys()])) {
  fail("baseline review must cover every architecture boundary");
}
if (!sameSet(baseline.reviewedDecisions ?? [], decisionIds)) {
  fail("baseline review must cover every accepted ADR");
}
for (const requiredCheck of [
  "authority",
  "dependency-direction",
  "durability",
  "compatibility",
  "ownership",
]) {
  const check = baseline.checks?.find(({ id }) => id === requiredCheck);
  if (check?.result !== "pass" || !check.evidence)
    fail(`baseline review check ${requiredCheck} must pass with evidence`);
}
for (const findingId of ["ARCH-001", "ARCH-002"]) {
  const finding = baseline.findings?.find(({ id }) => id === findingId);
  if (
    finding?.status !== "open" ||
    !roles.has(finding.ownerRole) ||
    !finding.acceptance
  ) {
    fail(
      `${findingId}: tracked architecture debt needs open status, owner, and acceptance`,
    );
  }
  if (!architecture.includes(`\`${findingId}\``))
    fail(`${findingId}: finding is absent from architecture debt`);
}

const domainFiles = criticalFiles.filter(
  (file) => file.startsWith("apps/server/src/domain/") && file.endsWith(".rs"),
);
for (const file of domainFiles) {
  const targets = crateTargets(await readFile(resolve(root, file), "utf8"));
  for (const forbidden of ["api", "app", "protocol", "store", "ws"]) {
    if (targets.has(forbidden))
      fail(
        `${file}: authoritative domain cannot depend on crate::${forbidden}`,
      );
  }
}
const storeFiles = criticalFiles.filter(
  (file) => file.startsWith("apps/server/src/store/") && file.endsWith(".rs"),
);
for (const file of storeFiles) {
  const targets = crateTargets(await readFile(resolve(root, file), "utf8"));
  for (const forbidden of ["api", "app", "protocol", "ws"]) {
    if (targets.has(forbidden))
      fail(`${file}: persistence adapter cannot depend on crate::${forbidden}`);
  }
}
const browserGameFiles = criticalFiles.filter(
  (file) =>
    file.startsWith("apps/web/src/lib/game/") && /\.(ts|svelte)$/.test(file),
);
for (const file of browserGameFiles) {
  const source = await readFile(resolve(root, file), "utf8");
  if (
    /from\s+['"]\$lib\/(api|components|realtime|routes|stores|ui)(?:['"/])/.test(
      source,
    )
  ) {
    fail(
      `${file}: pure browser game logic cannot depend on network, global state, route, or presentation modules`,
    );
  }
}

const requiredDecompositionFiles = [
  "apps/server/src/app/accounts.rs",
  "apps/server/src/app/connections.rs",
  "apps/server/src/app/live_content.rs",
  "apps/server/src/app/matchmaking.rs",
  "apps/server/src/app/metrics.rs",
  "apps/server/src/app/rooms.rs",
  "apps/server/src/app/router.rs",
  "apps/server/src/app/safety.rs",
  "apps/server/src/app/timers.rs",
  "apps/server/src/domain/room/chat.rs",
  "apps/server/src/domain/room/projection.rs",
  "apps/server/src/domain/room/state.rs",
  "apps/server/src/domain/room/timers.rs",
  "apps/web/src/lib/components/lobby/LobbyCommandDashboard.svelte",
  "apps/web/src/lib/components/lobby/LobbyRoomOperations.svelte",
  "apps/web/src/routes/lobby/lobby.css",
];
for (const file of requiredDecompositionFiles) {
  if (!criticalFiles.includes(file))
    fail(`${file}: required responsibility module is missing`);
}

const boundedServerModules = criticalFiles.filter(
  (file) =>
    file === "apps/server/src/app.rs" ||
    /^apps\/server\/src\/app\/[^/]+\.rs$/.test(file),
);
for (const file of boundedServerModules) {
  const source = await readFile(resolve(root, file), "utf8");
  if (lineCount(source) > 800)
    fail(`${file}: service orchestration module exceeds 800 lines`);
}

const boundedRoomModules = criticalFiles.filter(
  (file) =>
    file === "apps/server/src/domain/room.rs" ||
    /^apps\/server\/src\/domain\/room\/[^/]+\.rs$/.test(file),
);
for (const file of boundedRoomModules) {
  const source = await readFile(resolve(root, file), "utf8");
  if (lineCount(source) > 1_000)
    fail(
      `${file}: authoritative room responsibility module exceeds 1000 lines`,
    );
}

const routeComponents = criticalFiles.filter(
  (file) =>
    file.startsWith("apps/web/src/routes/") && file.endsWith("/+page.svelte"),
);
for (const file of routeComponents) {
  const source = await readFile(resolve(root, file), "utf8");
  const styleStart = source.indexOf("<style>");
  const orchestrationAndMarkup =
    styleStart === -1 ? source : source.slice(0, styleStart);
  if (lineCount(orchestrationAndMarkup) > 650)
    fail(`${file}: route orchestration and markup exceed 650 lines`);
  if (lineCount(source) > 1_200)
    fail(`${file}: route component exceeds 1200 lines`);
}

const lobbyRoutePath = "apps/web/src/routes/lobby/+page.svelte";
const lobbyRoute = await readFile(resolve(root, lobbyRoutePath), "utf8");
if (lineCount(lobbyRoute) > 400)
  fail(`${lobbyRoutePath}: lobby orchestration route exceeds 400 lines`);
for (const componentName of ["LobbyCommandDashboard", "LobbyRoomOperations"]) {
  if (!lobbyRoute.includes(`${componentName}.svelte`))
    fail(`${lobbyRoutePath}: missing ${componentName} presentation boundary`);
}

const lobbyStylesPath = "apps/web/src/routes/lobby/lobby.css";
const lobbyStyles = await readFile(resolve(root, lobbyStylesPath), "utf8");
if (lineCount(lobbyStyles) > 1_000)
  fail(`${lobbyStylesPath}: lobby presentation stylesheet exceeds 1000 lines`);
if (!lobbyStyles.includes(".lobby-page {"))
  fail(`${lobbyStylesPath}: presentation styles must remain route scoped`);

const lobbyPresentationFiles = criticalFiles.filter(
  (file) =>
    file.startsWith("apps/web/src/lib/components/lobby/") &&
    file.endsWith(".svelte"),
);
for (const file of lobbyPresentationFiles) {
  const source = await readFile(resolve(root, file), "utf8");
  if (lineCount(source) > 250)
    fail(`${file}: lobby presentation component exceeds 250 lines`);
  if (/from\s+['"]\$lib\/(api|realtime|stores)(?:['"/])/.test(source)) {
    fail(
      `${file}: lobby presentation cannot own network or global-state orchestration`,
    );
  }
}

const roomRoute = await readFile(
  resolve(root, "apps/web/src/routes/room/[code]/+page.svelte"),
  "utf8",
);
for (const componentName of [
  "BattleView",
  "ChatDrawer",
  "FleetPlacement",
  "ResultView",
  "WaitingView",
]) {
  if (!roomRoute.includes(`${componentName}.svelte`))
    fail(`room route is missing the ${componentName} responsibility component`);
}

for (const required of [
  "Architecture boundary IDs changed",
  "Accountable owner role(s)",
  "ADR added/updated",
  "Independent approval is mandatory",
  "`npm run architecture:check`",
]) {
  if (!pullRequestTemplate.includes(required))
    fail(`pull request template is missing: ${required}`);
}
if (
  packageDocument.scripts?.["architecture:check"] !==
  "node tools/check-architecture.mjs"
) {
  fail("package.json must expose the architecture:check script");
}
if (!packageDocument.scripts?.lint?.includes("npm run architecture:check")) {
  fail("the local lint gate must include architecture:check");
}
if (!ciWorkflow.includes("- run: npm run architecture:check")) {
  fail("CI quality must run architecture:check explicitly");
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join("\n"));
  process.exit(1);
}

console.log(
  `Architecture gate passed: ${boundaries.size} boundaries, ${roles.size} roles, ${decisionIds.length} accepted ADRs, ${criticalFiles.length} owned critical files.`,
);
