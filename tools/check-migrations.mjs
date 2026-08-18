import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const migrationDirectory = path.join(root, "apps/server/migrations");
const checksumPath = path.join(migrationDirectory, "checksums.sha256");
const serverBuildScriptPath = path.join(root, "apps/server/build.rs");
const migrationPattern = /^\d{12}_[a-z0-9_]+\.sql$/;
const files = (await readdir(migrationDirectory))
  .filter((file) => file.endsWith(".sql"))
  .sort();
const failures = [];
const oldVersionIncompatiblePatterns = [
  /\bDROP\s+(TABLE|COLUMN|SCHEMA|TYPE|INDEX|CONSTRAINT|VIEW|MATERIALIZED\s+VIEW|FUNCTION|TRIGGER)\b/,
  /\bTRUNCATE\b/,
  /\bDELETE\s+FROM\b/,
  /\bCREATE\s+OR\s+REPLACE\b/,
  /\bALTER\s+TYPE\b/,
  /\bALTER\s+TABLE\b[^;]*\bRENAME\b/,
  /\bALTER\s+COLUMN\b[^;]*\bTYPE\b/,
  /\bALTER\s+COLUMN\b[^;]*\bSET\s+NOT\s+NULL\b/,
  /\bALTER\s+COLUMN\b[^;]*\bDROP\s+DEFAULT\b/,
  /\b(ENABLE|FORCE)\s+ROW\s+LEVEL\s+SECURITY\b/,
  /\bREVOKE\b/,
];

for (const unsafeProbe of [
  "ALTER TABLE rooms DROP COLUMN snapshot",
  "DELETE FROM user_sessions",
  "CREATE OR REPLACE FUNCTION stable_contract()",
  "ALTER TYPE room_status ADD VALUE 'NEW'",
  "ALTER TABLE rooms ALTER COLUMN version DROP DEFAULT",
  "ALTER TABLE rooms ENABLE ROW LEVEL SECURITY",
]) {
  if (
    !oldVersionIncompatiblePatterns.some((pattern) => pattern.test(unsafeProbe))
  ) {
    failures.push(
      `migration policy self-test missed unsafe SQL: ${unsafeProbe}`,
    );
  }
}
for (const safeProbe of [
  "CREATE TABLE candidate_feature (id UUID PRIMARY KEY)",
  "ALTER TABLE rooms ADD COLUMN candidate_metadata JSONB NULL",
  "INSERT INTO candidate_feature SELECT id FROM rooms",
  "UPDATE rooms SET candidate_metadata = NULL WHERE candidate_metadata IS NULL",
]) {
  if (
    oldVersionIncompatiblePatterns.some((pattern) => pattern.test(safeProbe))
  ) {
    failures.push(
      `migration policy self-test rejected additive SQL: ${safeProbe}`,
    );
  }
}

const serverBuildScript = await readFile(serverBuildScriptPath, "utf8");
if (!serverBuildScript.includes("cargo:rerun-if-changed=migrations")) {
  failures.push(
    "server build.rs must invalidate the embedded migrator when migrations change",
  );
}

if (files.some((file) => !migrationPattern.test(file))) {
  failures.push("migration filenames must use YYYYMMDDNNNN_snake_case.sql");
}
for (let index = 1; index < files.length; index += 1) {
  if (files[index] <= files[index - 1])
    failures.push(`migration order is not monotonic: ${files[index]}`);
}

const expected = new Map(
  (await readFile(checksumPath, "utf8"))
    .trim()
    .split("\n")
    .map((line) => {
      const [checksum, file] = line.trim().split(/\s+/);
      return [file, checksum];
    }),
);

for (const file of files) {
  const sql = await readFile(path.join(migrationDirectory, file), "utf8");
  const checksum = createHash("sha256").update(sql).digest("hex");
  if (expected.get(file) !== checksum) {
    failures.push(
      `${file}: checksum is missing or changed; applied migrations are immutable`,
    );
  }
  const normalized = sql
    .replace(/--.*$/gm, " ")
    .replace(/\s+/g, " ")
    .toUpperCase();
  if (
    oldVersionIncompatiblePatterns.some((pattern) => pattern.test(normalized))
  ) {
    failures.push(
      `${file}: destructive or old-version-incompatible DDL is prohibited`,
    );
  }
  for (const statement of normalized.split(";")) {
    if (
      /\bADD\s+COLUMN\b/.test(statement) &&
      /\bNOT\s+NULL\b/.test(statement) &&
      !/\bDEFAULT\b/.test(statement)
    ) {
      failures.push(
        `${file}: NOT NULL additions require a backward-compatible default/backfill phase`,
      );
    }
  }
}
for (const file of expected.keys()) {
  if (!files.includes(file))
    failures.push(`${file}: checksummed migration was removed`);
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join("\n"));
  process.exit(1);
}
console.log(
  `Migration safety verified: ${files.length} immutable additive migrations.`,
);
