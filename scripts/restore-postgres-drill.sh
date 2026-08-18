#!/usr/bin/env bash
set -Eeuo pipefail

umask 077

: "${BACKUP_FILE:?BACKUP_FILE is required}"
: "${BACKUP_PASSPHRASE_FILE:?BACKUP_PASSPHRASE_FILE is required}"
: "${TARGET_DATABASE_URL:?TARGET_DATABASE_URL is required}"

MK01_SERVER_BIN="${MK01_SERVER_BIN:-target/release/mk01-server}"
RESTORE_RTO_SECONDS="${RESTORE_RTO_SECONDS:-900}"
MAX_BACKUP_AGE_SECONDS="${MAX_BACKUP_AGE_SECONDS:-43200}"
RESTORE_EVIDENCE_FILE="${RESTORE_EVIDENCE_FILE:-${BACKUP_FILE}.restore.json}"

if [[ ! -f "${BACKUP_FILE}" || ! -r "${BACKUP_PASSPHRASE_FILE}" || ! -x "${MK01_SERVER_BIN}" ]]; then
  echo "backup, readable passphrase, and executable server binary are required" >&2
  exit 2
fi

target_without_query="${TARGET_DATABASE_URL%%\?*}"
target_database="${target_without_query##*/}"
case "${target_database}" in
  *_restore | *_drill) ;;
  *)
    echo "TARGET_DATABASE_URL database name must end in _restore or _drill" >&2
    exit 2
    ;;
esac

for command_name in pg_restore gpg; do
  command -v "${command_name}" >/dev/null || {
    echo "${command_name} is required" >&2
    exit 2
  }
done

checksum_file="${BACKUP_FILE}.sha256"
if [[ ! -f "${checksum_file}" ]]; then
  echo "backup checksum file is missing" >&2
  exit 2
fi
backup_directory="$(cd "$(dirname "${BACKUP_FILE}")" && pwd -P)"
if command -v sha256sum >/dev/null; then
  (cd "${backup_directory}" && sha256sum --check "$(basename "${checksum_file}")")
else
  (cd "${backup_directory}" && shasum -a 256 --check "$(basename "${checksum_file}")")
fi

if backup_modified_at="$(stat -f %m "${BACKUP_FILE}" 2>/dev/null)"; then
  :
else
  backup_modified_at="$(stat -c %Y "${BACKUP_FILE}")"
fi
started_epoch="$(date +%s)"
backup_age_seconds="$((started_epoch - backup_modified_at))"
if (( backup_age_seconds > MAX_BACKUP_AGE_SECONDS )); then
  echo "backup age ${backup_age_seconds}s exceeds RPO gate ${MAX_BACKUP_AGE_SECONDS}s" >&2
  exit 1
fi

temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/mk01-restore.XXXXXX")"
temporary_dump="${temporary_directory}/restore.dump"
cleanup() {
  rm -rf "${temporary_directory}"
}
trap cleanup EXIT

gpg --batch --yes --pinentry-mode loopback \
  --passphrase-file "${BACKUP_PASSPHRASE_FILE}" \
  --decrypt --output "${temporary_dump}" "${BACKUP_FILE}"
pg_restore --exit-on-error --clean --if-exists --no-owner --no-acl \
  --dbname="${TARGET_DATABASE_URL}" "${temporary_dump}"

DATABASE_URL="${TARGET_DATABASE_URL}" STORAGE_MODE=postgres \
  "${MK01_SERVER_BIN}" --migrate-only
verification_report="$(DATABASE_URL="${TARGET_DATABASE_URL}" STORAGE_MODE=postgres \
  "${MK01_SERVER_BIN}" --verify-restore)"

finished_epoch="$(date +%s)"
restore_seconds="$((finished_epoch - started_epoch))"
if (( restore_seconds > RESTORE_RTO_SECONDS )); then
  echo "restore time ${restore_seconds}s exceeds RTO gate ${RESTORE_RTO_SECONDS}s" >&2
  exit 1
fi

checked_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '{"formatVersion":1,"checkedAt":"%s","backup":"%s","backupAgeSeconds":%s,"restoreSeconds":%s,"rpoGateSeconds":%s,"rtoGateSeconds":%s,"database":%s}\n' \
  "${checked_at}" "$(basename "${BACKUP_FILE}")" "${backup_age_seconds}" "${restore_seconds}" \
  "${MAX_BACKUP_AGE_SECONDS}" "${RESTORE_RTO_SECONDS}" "${verification_report}" \
  >"${RESTORE_EVIDENCE_FILE}"

echo "${RESTORE_EVIDENCE_FILE}"
