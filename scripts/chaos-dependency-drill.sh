#!/usr/bin/env bash
set -euo pipefail

server_origin="${CHAOS_SERVER_ORIGIN:-http://127.0.0.1:18084}"
proxy_api="${TOXIPROXY_API:-http://127.0.0.1:8474}"
postgres_proxy="${POSTGRES_PROXY_NAME:-postgres}"
evidence_file="${CHAOS_EVIDENCE_FILE:-.artifacts/chaos-evidence.json}"
recovery_budget_ms="${CHAOS_RECOVERY_BUDGET_MS:-15000}"
protocol_header='x-mk01-protocol-version: 2'
scratch_dir="$(mktemp -d)"
cookie_file="$scratch_dir/cookies.txt"

set_proxy() {
  local enabled="$1"
  curl --fail --silent --show-error \
    --request POST \
    --header 'Content-Type: application/json' \
    --data "{\"enabled\":${enabled}}" \
    "$proxy_api/proxies/$postgres_proxy" >/dev/null
}

cleanup() {
  set_proxy true >/dev/null 2>&1 || true
  rm -f "$cookie_file"
  rmdir "$scratch_dir" 2>/dev/null || true
}
trap cleanup EXIT

curl --fail --silent --show-error --cookie-jar "$cookie_file" \
  --header "$protocol_header" --header 'Content-Type: application/json' \
  --data '{"nickname":"ChaosCaptain"}' "$server_origin/api/sessions" >/dev/null

room_json="$(curl --fail --silent --show-error --cookie "$cookie_file" \
  --header "$protocol_header" --header 'Content-Type: application/json' \
  --data '{"name":"Durability Probe","visibility":"PRIVATE","rules":{"mode":"CLASSIC","turnDurationSeconds":60}}' \
  "$server_origin/api/rooms")"
room_id="$(jq -er '.snapshot.room.id' <<<"$room_json")"
room_code="$(jq -er '.snapshot.room.code' <<<"$room_json")"

set_proxy false
outage_started_ms="$(node -e 'process.stdout.write(String(Date.now()))')"
readiness_failed=false
for _ in {1..10}; do
  status="$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 2 \
    "$server_origin/api/ready" || true)"
  if [[ "$status" != "200" ]]; then
    readiness_failed=true
    break
  fi
  sleep 0.25
done
[[ "$readiness_failed" == true ]]

liveness_status="$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 2 \
  "$server_origin/api/health")"
[[ "$liveness_status" == "200" ]]

set_proxy true
recovered=false
deadline_ms=$((outage_started_ms + recovery_budget_ms))
while true; do
  now_ms="$(node -e 'process.stdout.write(String(Date.now()))')"
  if [[ "$now_ms" -gt "$deadline_ms" ]]; then break; fi
  status="$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 2 \
    "$server_origin/api/ready" || true)"
  if [[ "$status" == "200" ]]; then
    recovered=true
    break
  fi
  sleep 0.25
done
[[ "$recovered" == true ]]
recovered_ms="$(node -e 'process.stdout.write(String(Date.now()))')"
recovery_ms=$((recovered_ms - outage_started_ms))
[[ "$recovery_ms" -le "$recovery_budget_ms" ]]

recovered_room="$(curl --fail --silent --show-error --cookie "$cookie_file" \
  --header "$protocol_header" "$server_origin/api/rooms/$room_id")"
jq -e --arg id "$room_id" --arg code "$room_code" \
  '.room.id == $id and .room.code == $code and .room.name == "Durability Probe"' \
  <<<"$recovered_room" >/dev/null

jq -n \
  --arg scenario 'postgres_connection_reset' \
  --arg roomId "$room_id" \
  --arg roomCode "$room_code" \
  --argjson recoveryMs "$recovery_ms" \
  --argjson recoveryBudgetMs "$recovery_budget_ms" \
  --argjson livenessStatus "$liveness_status" \
  '{schemaVersion:1,scenario:$scenario,readinessFailedClosed:true,livenessStatus:$livenessStatus,recoveryMs:$recoveryMs,recoveryBudgetMs:$recoveryBudgetMs,snapshotPreserved:true,roomId:$roomId,roomCode:$roomCode}' \
  >"$evidence_file"

jq -e '.readinessFailedClosed and .livenessStatus == 200 and .recoveryMs <= .recoveryBudgetMs and .snapshotPreserved' \
  "$evidence_file" >/dev/null
echo "Chaos recovery passed in ${recovery_ms}ms; evidence: $evidence_file"
