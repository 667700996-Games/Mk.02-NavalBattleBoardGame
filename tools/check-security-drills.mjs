import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const policy = JSON.parse(
  readFileSync(join(root, '.github', 'security-response-policy.json'), 'utf8')
);
const drillDirectory = join(root, 'docs', 'security-drills');
const drillFiles = readdirSync(drillDirectory)
  .filter((name) => name.endsWith('.json'))
  .sort();

function fail(file, message) {
  throw new Error(`${file}: ${message}`);
}

function minutesBetween(start, end) {
  return (Date.parse(end) - Date.parse(start)) / 60_000;
}

if (drillFiles.length === 0) throw new Error('at least one security drill record is required');

for (const file of drillFiles) {
  const drill = JSON.parse(readFileSync(join(drillDirectory, file), 'utf8'));
  if (drill.schemaVersion !== policy.schemaVersion) fail(file, 'schema version does not match policy');
  if (drill.result !== 'PASS') fail(file, 'only a passing completed drill is release evidence');
  if (drill.exerciseType !== 'AUTOMATED_TABLETOP' && drill.exerciseType !== 'HUMAN_STAGING') {
    fail(file, 'exerciseType must be AUTOMATED_TABLETOP or HUMAN_STAGING');
  }

  for (const role of policy.requiredRoles) {
    if (typeof drill.owners?.[role] !== 'string' || drill.owners[role].trim() === '') {
      fail(file, `missing owner for ${role}`);
    }
  }

  const phaseTimes = new Map();
  let previousTime = Number.NEGATIVE_INFINITY;
  for (const event of drill.timeline ?? []) {
    const parsed = Date.parse(event.at);
    if (!Number.isFinite(parsed) || parsed < previousTime) fail(file, 'timeline must be valid and ordered');
    if (typeof event.evidence !== 'string' || event.evidence.trim() === '') {
      fail(file, `${event.phase ?? 'unknown phase'} lacks evidence`);
    }
    if (phaseTimes.has(event.phase)) fail(file, `duplicate ${event.phase} phase`);
    phaseTimes.set(event.phase, event.at);
    previousTime = parsed;
  }
  for (const phase of policy.requiredTimelinePhases) {
    if (!phaseTimes.has(phase)) fail(file, `missing ${phase} timeline phase`);
  }

  const limits = policy.severities[drill.severity];
  if (!limits) fail(file, `unknown severity ${drill.severity}`);
  const detected = phaseTimes.get('DETECTED');
  const elapsed = {
    acknowledge: minutesBetween(detected, phaseTimes.get('ACKNOWLEDGED')),
    mitigate: minutesBetween(detected, phaseTimes.get('MITIGATED')),
    patch: minutesBetween(detected, phaseTimes.get('PATCH_DEPLOYED'))
  };
  if (elapsed.acknowledge < 0 || elapsed.acknowledge > limits.acknowledgeMinutes) {
    fail(file, `acknowledgement exceeded ${limits.acknowledgeMinutes} minutes`);
  }
  if (elapsed.mitigate < 0 || elapsed.mitigate > limits.mitigateMinutes) {
    fail(file, `mitigation exceeded ${limits.mitigateMinutes} minutes`);
  }
  if (elapsed.patch < 0 || elapsed.patch > limits.patchMinutes) {
    fail(file, `patch exceeded ${limits.patchMinutes} minutes`);
  }

  for (const control of policy.requiredControls) {
    if (!(drill.controlsValidated ?? []).includes(control)) fail(file, `missing ${control} control`);
  }
  if (!(drill.controlEvidence ?? []).length) fail(file, 'controlEvidence is required');
  for (const evidence of drill.controlEvidence) {
    if (!evidence.control || !evidence.command || evidence.outcome !== 'PASSED') {
      fail(file, 'every control evidence item needs control, command, and PASSED outcome');
    }
  }
  if (!(drill.followUps ?? []).length) fail(file, 'owned follow-up actions are required');
  for (const followUp of drill.followUps) {
    if (!followUp.id || !followUp.owner || !followUp.action || !Number.isFinite(Date.parse(followUp.dueAt))) {
      fail(file, 'every follow-up needs id, owner, action, and dueAt');
    }
  }
  if (!(drill.limitations ?? []).length) fail(file, 'exercise limitations must be explicit');

  console.log(
    `${file}: ${drill.severity} PASS (ack ${elapsed.acknowledge}m, mitigate ${elapsed.mitigate}m, patch ${elapsed.patch}m)`
  );
}

console.log(`Security response drill gate passed: ${drillFiles.length} record(s).`);
