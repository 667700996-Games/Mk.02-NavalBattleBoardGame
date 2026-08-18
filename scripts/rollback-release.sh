#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

environment_name="${1:?usage: rollback-release.sh ENV PREVIOUS_MANIFEST CONFIRM_RELEASE_ID REASON}"
manifest_path="${2:?previous release manifest path is required}"
confirmation="${3:?previous release ID confirmation is required}"
reason="${4:?rollback reason is required}"

case "$environment_name" in
  development|staging|canary|production) ;;
  *) echo "unsupported release environment: $environment_name" >&2; exit 2 ;;
esac

node tools/release-gate.mjs \
  --manifest-only true \
  --allow-historical true \
  --manifest "$manifest_path"

export MK01_DEPLOYMENT_ENV="$environment_name"
export MK01_RELEASE_ID="$(jq -er '.releaseId' "$manifest_path")"
if [[ "$confirmation" != "$MK01_RELEASE_ID" ]]; then
  echo "rollback confirmation must exactly match $MK01_RELEASE_ID" >&2
  exit 2
fi
if (( ${#reason} < 8 || ${#reason} > 500 )); then
  echo "rollback reason must contain 8 to 500 characters" >&2
  exit 2
fi

environment_path="deploy/environments/${environment_name}.json"
export MK01_SERVER_IMAGE="$(jq -er '.images.server' "$manifest_path")"
export MK01_WEB_IMAGE="$(jq -er '.images.web' "$manifest_path")"
export MK01_GATEWAY_BIND="$(jq -er '.gatewayBind' "$environment_path")"
replicas="$(jq -er '.replicas' "$environment_path")"
origin_variable="$(jq -er '.publicOriginVariable' "$environment_path")"
export MK01_PUBLIC_ORIGIN="${!origin_variable:-}"

if [[ -z "$MK01_PUBLIC_ORIGIN" || ! "$MK01_PUBLIC_ORIGIN" =~ ^https:// ]]; then
  echo "$origin_variable must contain the HTTPS public origin" >&2
  exit 2
fi
if [[ ! "${MK01_GATEWAY_IMAGE:-}" =~ ^[a-z0-9._/-]+@sha256:[a-f0-9]{64}$ ]]; then
  echo "MK01_GATEWAY_IMAGE must pin the approved gateway image by sha256 digest" >&2
  exit 2
fi
for secret_variable in MK01_DATABASE_URL_FILE MK01_REDIS_URL_FILE MK01_ADMIN_TOKEN_FILE; do
  secret_path="${!secret_variable:-}"
  if [[ -z "$secret_path" || ! -r "$secret_path" ]]; then
    echo "$secret_variable must point to a readable deployment secret" >&2
    exit 2
  fi
done

started_at="$(date +%s)"
compose=(docker compose --project-name "mk01-${environment_name}" --file deploy/compose.release.yaml)
"${compose[@]}" config --quiet
"${compose[@]}" pull
# Data is intentionally not rolled back. The previous application must read additive candidate migrations.
"${compose[@]}" up --detach --wait --remove-orphans --scale "server=${replicas}" --scale "web=${replicas}"
curl --fail --silent --show-error "http://${MK01_GATEWAY_BIND}/api/ready" >/dev/null
recovery_seconds="$(( $(date +%s) - started_at ))"
if (( recovery_seconds > 900 )); then
  echo "rollback exceeded the 900-second recovery budget" >&2
  exit 1
fi

mkdir -p .artifacts
receipt_path=".artifacts/rollback-${environment_name}.json"
jq -n \
  --arg environment "$environment_name" \
  --arg releaseId "$MK01_RELEASE_ID" \
  --arg manifestDigest "$(jq -er '.manifestDigest' "$manifest_path")" \
  --arg reason "$reason" \
  --arg rolledBackAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson recoverySeconds "$recovery_seconds" \
  '{schemaVersion:1,action:"ROLLBACK",environment:$environment,releaseId:$releaseId,manifestDigest:$manifestDigest,reason:$reason,dataRollback:false,ready:true,recoverySeconds:$recoverySeconds,rolledBackAt:$rolledBackAt}' \
  >"$receipt_path"
echo "Rolled back $environment_name to $MK01_RELEASE_ID in ${recovery_seconds}s; receipt: $receipt_path"
