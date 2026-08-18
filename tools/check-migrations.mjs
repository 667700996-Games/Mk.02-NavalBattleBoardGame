import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const migrationDirectory = path.join(root, 'apps/server/migrations');
const checksumPath = path.join(migrationDirectory, 'checksums.sha256');
const migrationPattern = /^\d{12}_[a-z0-9_]+\.sql$/;
const files = (await readdir(migrationDirectory))
  .filter((file) => file.endsWith('.sql'))
  .sort();
const failures = [];

if (files.some((file) => !migrationPattern.test(file))) {
  failures.push('migration filenames must use YYYYMMDDNNNN_snake_case.sql');
}
for (let index = 1; index < files.length; index += 1) {
  if (files[index] <= files[index - 1]) failures.push(`migration order is not monotonic: ${files[index]}`);
}

const expected = new Map(
  (await readFile(checksumPath, 'utf8'))
    .trim()
    .split('\n')
    .map((line) => {
      const [checksum, file] = line.trim().split(/\s+/);
      return [file, checksum];
    })
);

for (const file of files) {
  const sql = await readFile(path.join(migrationDirectory, file), 'utf8');
  const checksum = createHash('sha256').update(sql).digest('hex');
  if (expected.get(file) !== checksum) {
    failures.push(`${file}: checksum is missing or changed; applied migrations are immutable`);
  }
  const normalized = sql.replace(/--.*$/gm, ' ').replace(/\s+/g, ' ').toUpperCase();
  const destructivePatterns = [
    /\bDROP\s+(TABLE|COLUMN|SCHEMA|TYPE|INDEX)\b/,
    /\bTRUNCATE\b/,
    /\bALTER\s+TABLE\b[^;]*\bRENAME\b/,
    /\bALTER\s+COLUMN\b[^;]*\bTYPE\b/,
    /\bALTER\s+COLUMN\b[^;]*\bSET\s+NOT\s+NULL\b/
  ];
  if (destructivePatterns.some((pattern) => pattern.test(normalized))) {
    failures.push(`${file}: destructive or old-version-incompatible DDL is prohibited`);
  }
  for (const statement of normalized.split(';')) {
    if (/\bADD\s+COLUMN\b/.test(statement) && /\bNOT\s+NULL\b/.test(statement) && !/\bDEFAULT\b/.test(statement)) {
      failures.push(`${file}: NOT NULL additions require a backward-compatible default/backfill phase`);
    }
  }
}
for (const file of expected.keys()) {
  if (!files.includes(file)) failures.push(`${file}: checksummed migration was removed`);
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'));
  process.exit(1);
}
console.log(`Migration safety verified: ${files.length} immutable additive migrations.`);
