import { readFile } from "node:fs/promises";

const args = process.argv.slice(2);
const command = args.shift();
const baseUrl = (process.env.MK01_BASE_URL ?? "http://127.0.0.1:8080").replace(
  /\/$/,
  "",
);
const operatorId = process.env.OPERATOR_ID?.trim();

function usage() {
  console.log(`Usage:
  npm run content:ops -- history [--limit 25]
  npm run content:ops -- validate <payload.json> --expected <revision>
  npm run content:ops -- publish <payload.json> --expected <revision> --confirm
  npm run content:ops -- rollback --expected <revision> --target <revision> --note <reason> --confirm

Environment:
  MK01_BASE_URL  Server origin (default http://127.0.0.1:8080)
  ADMIN_TOKEN or ADMIN_TOKEN_FILE
  OPERATOR_ID    Required for validate, publish, and rollback`);
}

function option(name) {
  const index = args.indexOf(name);
  if (index === -1 || index === args.length - 1) return undefined;
  return args[index + 1];
}

function unsignedInteger(value, label) {
  if (!value || !/^\d+$/.test(value)) {
    throw new Error(`${label} must be an unsigned integer`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    throw new Error(`${label} exceeds the safe integer range`);
  }
  return parsed;
}

async function adminToken() {
  if (process.env.ADMIN_TOKEN?.trim()) return process.env.ADMIN_TOKEN.trim();
  if (process.env.ADMIN_TOKEN_FILE?.trim()) {
    const value = (await readFile(process.env.ADMIN_TOKEN_FILE, "utf8")).trim();
    if (value) return value;
  }
  throw new Error("ADMIN_TOKEN or ADMIN_TOKEN_FILE is required");
}

async function request(path, init = {}) {
  const token = await adminToken();
  const response = await fetch(`${baseUrl}/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      Authorization: `Bearer ${token}`,
      ...(operatorId ? { "X-Operator-Id": operatorId } : {}),
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(
      `${response.status} ${body.code ?? "REQUEST_FAILED"}: ${body.message ?? "No response body"}`,
    );
  }
  return body;
}

function requireOperator() {
  if (!operatorId) throw new Error("OPERATOR_ID is required for this command");
}

async function payloadFromFile(path) {
  if (!path || path.startsWith("--")) {
    throw new Error("a live-content payload JSON file is required");
  }
  return JSON.parse(await readFile(path, "utf8"));
}

function printValidation(validation) {
  console.log(
    JSON.stringify(
      {
        valid: validation.valid,
        candidateRevision: validation.candidateRevision,
        issues: validation.issues,
      },
      null,
      2,
    ),
  );
}

async function validate(payload, expectedRevision) {
  return request("/admin/content/validate", {
    method: "POST",
    body: JSON.stringify({ expectedRevision, payload }),
  });
}

try {
  if (!command || command === "--help" || command === "help") {
    usage();
  } else if (command === "history") {
    const limit = unsignedInteger(option("--limit") ?? "25", "--limit");
    console.log(
      JSON.stringify(
        await request(`/admin/content/revisions?limit=${Math.min(100, limit)}`),
        null,
        2,
      ),
    );
  } else if (command === "validate" || command === "publish") {
    requireOperator();
    const payload = await payloadFromFile(args[0]);
    const expectedRevision = unsignedInteger(option("--expected"), "--expected");
    const validation = await validate(payload, expectedRevision);
    printValidation(validation);
    if (!validation.valid) process.exitCode = 2;
    if (command === "publish") {
      if (!args.includes("--confirm")) {
        throw new Error("publish requires --confirm after a successful dry run");
      }
      if (!validation.valid) {
        throw new Error("invalid live content was not published");
      }
      const published = await request("/admin/content/revisions", {
        method: "POST",
        body: JSON.stringify({ expectedRevision, payload }),
      });
      console.log(
        JSON.stringify(
          {
            publishedRevision: published.revision,
            activateAt: published.activateAt,
            operatorId: published.operatorId,
          },
          null,
          2,
        ),
      );
    }
  } else if (command === "rollback") {
    requireOperator();
    if (!args.includes("--confirm")) {
      throw new Error("rollback requires --confirm");
    }
    const expectedRevision = unsignedInteger(option("--expected"), "--expected");
    const targetRevision = unsignedInteger(option("--target"), "--target");
    const changeNote = option("--note")?.trim();
    if (!changeNote) throw new Error("rollback requires --note");
    const rolledBack = await request("/admin/content/rollback", {
      method: "POST",
      body: JSON.stringify({ expectedRevision, targetRevision, changeNote }),
    });
    console.log(
      JSON.stringify(
        {
          publishedRevision: rolledBack.revision,
          rolledBackFromRevision: rolledBack.rolledBackFromRevision,
          activateAt: rolledBack.activateAt,
        },
        null,
        2,
      ),
    );
  } else {
    throw new Error(`unknown command: ${command}`);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
