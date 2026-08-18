#!/usr/bin/env bash
set -Eeuo pipefail

umask 077

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${BACKUP_DESTINATION:?BACKUP_DESTINATION is required}"
: "${BACKUP_PASSPHRASE_FILE:?BACKUP_PASSPHRASE_FILE is required}"

if [[ ! -d "${BACKUP_DESTINATION}" || ! -r "${BACKUP_PASSPHRASE_FILE}" ]]; then
  echo "backup destination must exist and passphrase file must be readable" >&2
  exit 2
fi

for command_name in pg_dump gpg; do
  command -v "${command_name}" >/dev/null || {
    echo "${command_name} is required" >&2
    exit 2
  }
done

backup_directory="$(cd "${BACKUP_DESTINATION}" && pwd -P)"
if [[ "${backup_directory}" == "/" ]]; then
  echo "refusing to use the filesystem root as a backup destination" >&2
  exit 2
fi

backup_timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_id="mk01-${backup_timestamp}-$RANDOM"
backup_file="${backup_directory}/${backup_id}.dump.gpg"
checksum_file="${backup_file}.sha256"
metadata_file="${backup_file}.json"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/mk01-backup.XXXXXX")"
temporary_dump="${temporary_directory}/${backup_id}.dump"
temporary_encrypted="${temporary_directory}/${backup_id}.dump.gpg"

cleanup() {
  rm -rf "${temporary_directory}"
}
trap cleanup EXIT

pg_dump --format=custom --compress=9 --no-owner --no-acl --file="${temporary_dump}" "${DATABASE_URL}"
gpg --batch --yes --pinentry-mode loopback \
  --passphrase-file "${BACKUP_PASSPHRASE_FILE}" \
  --symmetric --cipher-algo AES256 \
  --output "${temporary_encrypted}" "${temporary_dump}"

mv "${temporary_encrypted}" "${backup_file}"
if command -v sha256sum >/dev/null; then
  (cd "${backup_directory}" && sha256sum "$(basename "${backup_file}")") >"${checksum_file}"
else
  (cd "${backup_directory}" && shasum -a 256 "$(basename "${backup_file}")") >"${checksum_file}"
fi
printf '{"formatVersion":1,"backupId":"%s","createdAt":"%s","encryption":"GPG-AES256","databaseFormat":"pg_dump-custom"}\n' \
  "${backup_id}" "${backup_timestamp}" >"${metadata_file}"

echo "${backup_file}"
