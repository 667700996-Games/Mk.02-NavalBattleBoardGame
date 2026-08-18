#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

environment_name="${1:?usage: deploy-release.sh ENV MANIFEST EVIDENCE [PREVIOUS_MANIFEST]}"
manifest_path="${2:?release manifest path is required}"
evidence_path="${3:?release evidence path is required}"
previous_manifest_path="${4:-}"

case "$environment_name" in
  development|staging|canary|production) ;;
  *) echo "unsupported release environment: $environment_name" >&2; exit 2 ;;
esac

mkdir -p .artifacts
decision_path=".artifacts/release-decision-${environment_name}.json"
gate_arguments=(
  --environment "$environment_name"
  --manifest "$manifest_path"
  --evidence "$evidence_path"
  --output "$decision_path"
)
if [[ -n "$previous_manifest_path" ]]; then
  gate_arguments+=(--previous "$previous_manifest_path")
fi
node tools/release-gate.mjs "${gate_arguments[@]}"

environment_path="deploy/environments/${environment_name}.json"
export MK01_DEPLOYMENT_ENV="$environment_name"
export MK01_RELEASE_ID="$(jq -er '.releaseId' "$manifest_path")"
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

compose=(docker compose --project-name "mk01-${environment_name}" --file deploy/compose.release.yaml)
"${compose[@]}" config --quiet
"${compose[@]}" pull
"${compose[@]}" run --rm --no-deps server --migrate-only
"${compose[@]}" up --detach --wait --remove-orphans --scale "server=${replicas}" --scale "web=${replicas}"

probe_origin="http://${MK01_GATEWAY_BIND}"
for attempt in $(seq 1 30); do
  if curl --fail --silent --show-error "${probe_origin}/api/ready" >/dev/null; then
    break
  fi
  if [[ "$attempt" == "30" ]]; then
    echo "release did not become ready through the environment gateway" >&2
    exit 1
  fi
  sleep 2
done
protocol="$(curl --fail --silent --show-error "${probe_origin}/api/protocol")"
jq -e '.minimumSupportedVersion <= 2 and .maximumSupportedVersion >= 2' <<<"$protocol" >/dev/null

receipt_path=".artifacts/deployment-${environment_name}.json"
jq -n \
  --arg environment "$environment_name" \
  --arg releaseId "$MK01_RELEASE_ID" \
  --arg manifestDigest "$(jq -er '.manifestDigest' "$manifest_path")" \
  --arg serverImage "$MK01_SERVER_IMAGE" \
  --arg webImage "$MK01_WEB_IMAGE" \
  --arg gatewayImage "$MK01_GATEWAY_IMAGE" \
  --arg deployedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson replicas "$replicas" \
  '{schemaVersion:1,action:"DEPLOY",environment:$environment,releaseId:$releaseId,manifestDigest:$manifestDigest,images:{server:$serverImage,web:$webImage,gateway:$gatewayImage},replicas:$replicas,ready:true,protocolV2Accepted:true,deployedAt:$deployedAt}' \
  >"$receipt_path"
echo "Deployed $MK01_RELEASE_ID to $environment_name; receipt: $receipt_path"
