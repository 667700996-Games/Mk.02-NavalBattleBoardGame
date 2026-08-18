#!/usr/bin/env bash
set -Eeuo pipefail

umask 077

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${DELETION_LEDGER_DESTINATION:?DELETION_LEDGER_DESTINATION is required}"
: "${DELETION_LEDGER_PASSPHRASE_FILE:?DELETION_LEDGER_PASSPHRASE_FILE is required}"

MK01_SERVER_BIN="${MK01_SERVER_BIN:-target/release/mk01-server}"

if [[ ! -d "${DELETION_LEDGER_DESTINATION}" || ! -r "${DELETION_LEDGER_PASSPHRASE_FILE}" || ! -x "${MK01_SERVER_BIN}" ]]; then
  echo "ledger destination, readable passphrase, and executable server binary are required" >&2
  exit 2
fi
command -v gpg >/dev/null || {
  echo "gpg is required" >&2
  exit 2
}

ledger_directory="$(cd "${DELETION_LEDGER_DESTINATION}" && pwd -P)"
if [[ "${ledger_directory}" == "/" ]]; then
  echo "refusing to use the filesystem root as a ledger destination" >&2
  exit 2
fi

ledger_timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
ledger_id="mk01-deletion-ledger-${ledger_timestamp}-$RANDOM"
ledger_file="${ledger_directory}/${ledger_id}.json.gpg"
checksum_file="${ledger_file}.sha256"
metadata_file="${ledger_file}.meta.json"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/mk01-deletion-ledger.XXXXXX")"
temporary_json="${temporary_directory}/${ledger_id}.json"
temporary_encrypted="${temporary_directory}/${ledger_id}.json.gpg"

cleanup() {
  rm -rf "${temporary_directory}"
}
trap cleanup EXIT

STORAGE_MODE=postgres "${MK01_SERVER_BIN}" --export-deletion-ledger >"${temporary_json}"
gpg --batch --yes --pinentry-mode loopback \
  --passphrase-file "${DELETION_LEDGER_PASSPHRASE_FILE}" \
  --symmetric --cipher-algo AES256 \
  --output "${temporary_encrypted}" "${temporary_json}"
mv "${temporary_encrypted}" "${ledger_file}"

if command -v sha256sum >/dev/null; then
  (cd "${ledger_directory}" && sha256sum "$(basename "${ledger_file}")") >"${checksum_file}"
else
  (cd "${ledger_directory}" && shasum -a 256 "$(basename "${ledger_file}")") >"${checksum_file}"
fi
printf '{"formatVersion":1,"ledgerId":"%s","createdAt":"%s","encryption":"GPG-AES256"}\n' \
  "${ledger_id}" "${ledger_timestamp}" >"${metadata_file}"

echo "${ledger_file}"
