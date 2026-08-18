import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];
const read = (...parts) => readFileSync(join(root, ...parts), 'utf8');
const json = (...parts) => JSON.parse(read(...parts));
const policy = json('.github', 'incident-response-policy.json');
const ruleDocument = json('ops', 'observability', 'prometheus-rules.json');
const dashboard = json(
  'ops',
  'observability',
  'grafana',
  'dashboards',
  'mk01-service.json'
);

function fail(message) {
  failures.push(message);
}

function collectFiles(directory, extension) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collectFiles(path, extension);
    return extname(entry.name) === extension ? [path] : [];
  });
}

function metricNames(text) {
  return new Set(text.match(/\bmk01_[a-z0-9_]+\b/g) ?? []);
}

const serverSource = collectFiles(join(root, 'apps', 'server', 'src'), '.rs')
  .map((file) => readFileSync(file, 'utf8'))
  .join('\n');
const applicationMetrics = metricNames(serverSource);
const externallyOwnedMetrics = new Set(['mk01_backup_age_seconds']);

function metricIsOwned(metric) {
  if (applicationMetrics.has(metric) || externallyOwnedMetrics.has(metric)) return true;
  const base = metric.replace(/_(bucket|count|sum)$/, '');
  return applicationMetrics.has(base) || externallyOwnedMetrics.has(base);
}

function validateExpression(expression, owner) {
  if (typeof expression !== 'string' || expression.trim() === '') {
    fail(`${owner}: PromQL expression is missing`);
    return;
  }
  for (const metric of metricNames(expression)) {
    if (!metricIsOwned(metric)) fail(`${owner}: ${metric} is not emitted or externally owned`);
  }
  if (/\b(account_id|session_id|room_id|player_id|request_id|nickname)\s*(=|!=|=~|!~)/.test(expression)) {
    fail(`${owner}: identity or unbounded gameplay labels are prohibited`);
  }
}

if (policy.schemaVersion !== 1) fail('incident policy schemaVersion must be 1');
if (!Array.isArray(ruleDocument.groups) || ruleDocument.groups.length === 0) {
  fail('at least one Prometheus rule group is required');
}

const rules = ruleDocument.groups.flatMap((group) => {
  if (!group.name || !/^\d+(s|m)$/.test(group.interval ?? '')) {
    fail('every Prometheus group needs a name and bounded seconds/minutes interval');
  }
  return group.rules ?? [];
});
const alerts = new Map();
for (const rule of rules) {
  if (!rule.alert) {
    fail('recording rules are not allowed in the paging bundle without an explicit contract');
    continue;
  }
  if (alerts.has(rule.alert)) fail(`${rule.alert}: alert name is duplicated`);
  alerts.set(rule.alert, rule);
  validateExpression(rule.expr, rule.alert);
  if (!/^\d+(s|m|h)$/.test(rule.for ?? '')) fail(`${rule.alert}: bounded for duration is required`);
  if (!['page', 'ticket'].includes(rule.labels?.severity)) {
    fail(`${rule.alert}: severity must route to page or ticket`);
  }
  if (rule.labels?.service !== 'mk01' || !rule.labels?.slo) {
    fail(`${rule.alert}: service=mk01 and an slo label are required`);
  }
  for (const field of ['summary', 'description', 'runbook_url']) {
    if (typeof rule.annotations?.[field] !== 'string' || rule.annotations[field].trim() === '') {
      fail(`${rule.alert}: annotation ${field} is required`);
    }
  }
  const runbook = rule.annotations?.runbook_url ?? '';
  if (!runbook.startsWith('https://github.com/orca-crew/Mk.01-GameProject-NavalBattleBoardGame/')) {
    fail(`${rule.alert}: runbook_url must point to the versioned project runbook`);
  }
}

for (const name of policy.requiredAlerts ?? []) {
  if (!alerts.has(name)) fail(`required alert ${name} is missing`);
}
if (![...alerts.values()].some((rule) => rule.labels?.severity === 'page')) {
  fail('at least one paging alert is required');
}
if (![...alerts.values()].some((rule) => rule.labels?.severity === 'ticket')) {
  fail('at least one ticket alert is required');
}

const fastBurn = alerts.get('Mk01AvailabilityFastBurn')?.expr ?? '';
if (!fastBurn.includes('[1h]') || !fastBurn.includes('[5m]') || !fastBurn.includes('14.4')) {
  fail('fast availability burn must pair the one-hour and five-minute 14.4x windows');
}
const slowBurn = alerts.get('Mk01AvailabilitySlowBurn')?.expr ?? '';
if (!slowBurn.includes('[6h]') || !slowBurn.includes('[30m]') || !slowBurn.includes('> 6')) {
  fail('slow availability burn must pair the six-hour and thirty-minute 6x windows');
}
const disconnectAlert = alerts.get('Mk01UnexpectedDisconnectRate')?.expr ?? '';
if (!disconnectAlert.includes('>= 100') || !disconnectAlert.includes('> 0.005')) {
  fail('disconnect alert must enforce the 100-session denominator and 0.5% objective');
}

if (dashboard.uid !== 'mk01-service-slo' || dashboard.editable !== false) {
  fail('Grafana dashboard must have the stable mk01-service-slo UID and be immutable');
}
for (const tag of ['mk01', 'production', 'slo']) {
  if (!(dashboard.tags ?? []).includes(tag)) fail(`Grafana dashboard is missing the ${tag} tag`);
}
if (!/^([1-5]?\d)s$/.test(dashboard.refresh ?? '')) {
  fail('Grafana refresh must be between 1 and 59 seconds');
}
const datasourceVariable = dashboard.templating?.list?.find((item) => item.name === 'datasource');
if (datasourceVariable?.type !== 'datasource' || datasourceVariable.query !== 'prometheus') {
  fail('Grafana dashboard needs a Prometheus datasource variable');
}

const panels = dashboard.panels ?? [];
const panelTitles = new Set();
const panelIds = new Set();
for (const panel of panels) {
  if (!panel.title || panelTitles.has(panel.title)) fail(`dashboard panel title is missing or duplicated: ${panel.title}`);
  panelTitles.add(panel.title);
  if (!Number.isInteger(panel.id) || panelIds.has(panel.id)) fail(`${panel.title}: panel ID is missing or duplicated`);
  panelIds.add(panel.id);
  if (!panel.description || !panel.gridPos) fail(`${panel.title}: description and grid position are required`);
  if (!(panel.targets ?? []).length) fail(`${panel.title}: at least one Prometheus target is required`);
  const references = new Set();
  for (const target of panel.targets ?? []) {
    if (!target.refId || references.has(target.refId)) fail(`${panel.title}: target refId is missing or duplicated`);
    references.add(target.refId);
    validateExpression(target.expr, `${panel.title}/${target.refId}`);
  }
}
for (const title of policy.requiredDashboardPanels ?? []) {
  if (!panelTitles.has(title)) fail(`required dashboard panel ${title} is missing`);
}

const operations = read('docs', 'OPERATIONS.md');
const operationAnchors = new Set(
  [...operations.matchAll(/^#{2,6}\s+(.+)$/gm)].map((match) =>
    match[1]
      .toLowerCase()
      .replace(/[^a-z0-9\s-]/g, '')
      .trim()
      .replace(/\s+/g, '-')
  )
);
for (const [name, rule] of alerts) {
  const fragment = rule.annotations.runbook_url.split('#')[1];
  if (!fragment || !operationAnchors.has(fragment)) fail(`${name}: runbook fragment #${fragment} does not exist`);
}

const incidentRunbook = read('docs', 'INCIDENT_RESPONSE.md');
for (const role of ['Incident commander', 'Operations lead', 'Game integrity lead', 'Communications lead', 'Scribe']) {
  if (!incidentRunbook.includes(role)) fail(`incident runbook is missing role ${role}`);
}
for (const [severity, limits] of Object.entries(policy.severities ?? {})) {
  if (!incidentRunbook.includes(severity)) fail(`incident runbook is missing ${severity}`);
  if (!incidentRunbook.includes(`${limits.acknowledgeMinutes} min`)) {
    fail(`incident runbook is missing the ${severity} acknowledgement clock`);
  }
  const cadence =
    limits.publicUpdateMinutes % 60 === 0
      ? `${limits.publicUpdateMinutes / 60} hour`
      : `${limits.publicUpdateMinutes} min`;
  if (!incidentRunbook.includes(cadence)) {
    fail(`incident runbook is missing the ${severity} public update cadence`);
  }
  if (!incidentRunbook.includes(`${limits.postmortemBusinessDays} business days`)) {
    fail(`incident runbook is missing the ${severity} review deadline`);
  }
}
for (const relativePath of policy.requiredTemplates ?? []) {
  if (!existsSync(join(root, relativePath))) fail(`required incident template ${relativePath} is missing`);
}

const statusTemplate = read('docs', 'templates', 'STATUS_UPDATE.md');
for (const section of ['Player impact', 'Current action', 'What we know', 'Resolution criteria']) {
  if (!statusTemplate.includes(`## ${section}`)) fail(`status template is missing ${section}`);
}
if (!statusTemplate.includes('Next update no later than')) fail('status template needs a next-update deadline');

const postmortemTemplate = read('docs', 'templates', 'POSTMORTEM.md');
for (const section of [
  'Executive summary',
  'Impact',
  'Detection and response',
  'Timeline',
  'Root cause and contributing factors',
  'Recovery and verification',
  'Corrective actions',
  'Follow-up communication'
]) {
  if (!postmortemTemplate.includes(`## ${section}`)) fail(`postmortem template is missing ${section}`);
}
for (const field of ['Owner', 'Due date', 'Verification and status']) {
  if (!postmortemTemplate.includes(field)) fail(`postmortem action table is missing ${field}`);
}

const dashboardProvisioning = read(
  'ops',
  'observability',
  'grafana',
  'provisioning',
  'dashboards',
  'mk01.yaml'
);
if (!dashboardProvisioning.includes('/etc/grafana/mk01-dashboards') || !dashboardProvisioning.includes('editable: false')) {
  fail('Grafana dashboard provisioning must mount the immutable dashboard directory');
}
const prometheusConfig = read('ops', 'observability', 'prometheus.yml');
if (!prometheusConfig.includes('/etc/prometheus/mk01-rules/*.json') || !prometheusConfig.includes('/api/metrics')) {
  fail('Prometheus config must load the rule bundle and scrape /api/metrics');
}
const ciWorkflow = read('.github', 'workflows', 'ci.yml');
if (
  !ciWorkflow.includes('observability-config:') ||
  !ciWorkflow.includes('promtool') ||
  !ciWorkflow.includes('amtool')
) {
  fail('CI must validate alert rules and routing with official promtool and amtool');
}
const alertmanagerConfig = read('ops', 'observability', 'alertmanager.yml');
for (const contract of [
  'receiver: mk01-page',
  'receiver: mk01-ticket',
  'severity="page"',
  'severity="ticket"',
  'send_resolved: true',
  'repeat_interval: 30m'
]) {
  if (!alertmanagerConfig.includes(contract)) fail(`Alertmanager routing is missing ${contract}`);
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'));
  process.exit(1);
}

console.log(
  `Observability gate passed: ${panels.length} panels, ${alerts.size} alerts, ${applicationMetrics.size} application metric names.`
);
