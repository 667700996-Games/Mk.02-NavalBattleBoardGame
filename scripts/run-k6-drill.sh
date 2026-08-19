#!/usr/bin/env bash
set -euo pipefail

profile="${1:-load}"
if [[ "$profile" != "load" && "$profile" != "soak" ]]; then
  echo "usage: scripts/run-k6-drill.sh [load|soak]" >&2
  exit 2
fi

server_port="${RELIABILITY_SERVER_PORT:-18082}"
host_origin="http://127.0.0.1:${server_port}"
ready_timeout_seconds="${RELIABILITY_READY_TIMEOUT_SECONDS:-180}"
server_pid=''

if [[ ! "$ready_timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo 'RELIABILITY_READY_TIMEOUT_SECONDS must be a positive integer.' >&2
  exit 2
fi

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

mkdir -p .artifacts
if [[ "${RELIABILITY_EXTERNAL_SERVER:-false}" != "true" ]]; then
  cargo build --locked -p mk01-server
  server_binary="${RELIABILITY_SERVER_BINARY:-${CARGO_TARGET_DIR:-target}/debug/mk01-server}"
  if [[ ! -x "$server_binary" ]]; then
    echo "Reliability server binary is not executable: $server_binary" >&2
    exit 1
  fi

  STORAGE_MODE=memory \
  SERVER_PORT="$server_port" \
  PUBLIC_BASE_URL="$host_origin" \
  ALLOWED_ORIGINS="$host_origin,http://host.docker.internal:${server_port}" \
  HTTP_REQUESTS_PER_MINUTE_PER_IP=0 \
  API_REQUESTS_PER_MINUTE=0 \
  SESSION_CREATIONS_PER_MINUTE=0 \
  RUST_LOG=warn \
    "$server_binary" >.artifacts/${profile}-server.log 2>&1 &
  server_pid="$!"

  ready_deadline=$((SECONDS + ready_timeout_seconds))
  while (( SECONDS < ready_deadline )); do
    if curl --fail --silent "$host_origin/api/ready" >/dev/null; then
      break
    fi
    if ! kill -0 "$server_pid" >/dev/null 2>&1; then
      echo 'Reliability server exited before becoming ready.' >&2
      tail -n 200 ".artifacts/${profile}-server.log" >&2 || true
      exit 1
    fi
    sleep 0.5
  done
  if ! curl --fail --silent "$host_origin/api/ready" >/dev/null; then
    echo "Reliability server did not become ready within ${ready_timeout_seconds}s." >&2
    tail -n 200 ".artifacts/${profile}-server.log" >&2 || true
    exit 1
  fi
fi

export RELIABILITY_PROFILE="$profile"
export RELIABILITY_SUMMARY=".artifacts/${profile}-summary.json"

if command -v k6 >/dev/null 2>&1; then
  export RELIABILITY_BASE_URL="${RELIABILITY_BASE_URL:-$host_origin}"
  k6 run tests/reliability/k6/game-api.js
elif command -v docker >/dev/null 2>&1; then
  container_origin="$host_origin"
  network_args=(--network host)
  if [[ "$(uname -s)" == "Darwin" ]]; then
    container_origin="http://host.docker.internal:${server_port}"
    network_args=()
  fi
  docker run --rm "${network_args[@]}" \
    --user "$(id -u):$(id -g)" \
    --volume "$PWD:/work" \
    --workdir /work \
    --env RELIABILITY_PROFILE="$profile" \
    --env RELIABILITY_BASE_URL="$container_origin" \
    --env RELIABILITY_DURATION="${RELIABILITY_DURATION:-}" \
    --env RELIABILITY_VUS="${RELIABILITY_VUS:-}" \
    --env RELIABILITY_SUMMARY="$RELIABILITY_SUMMARY" \
    grafana/k6:0.54.0 run tests/reliability/k6/game-api.js
else
  echo 'k6 or Docker is required to run the reliability profile.' >&2
  exit 1
fi

jq -e '
  def threshold_passed($metric; $threshold):
    .metrics[$metric].thresholds[$threshold] as $result
    | ($result == true or $result.ok == true);

  threshold_passed("checks"; "rate>0.99")
    and threshold_passed("workflow_failures"; "rate<0.01")
    and threshold_passed("websocket_failures"; "rate<0.01")
    and threshold_passed("critical_http_duration"; "p(95)<250")
    and threshold_passed("critical_http_duration"; "p(99)<600")
' \
  "$RELIABILITY_SUMMARY" >/dev/null
