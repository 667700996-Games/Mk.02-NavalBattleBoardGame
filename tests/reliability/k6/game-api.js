import http from "k6/http";
import ws from "k6/ws";
import { check, sleep } from "k6";
import { Rate, Trend } from "k6/metrics";

const profile = __ENV.RELIABILITY_PROFILE || "load";
const baseUrl = __ENV.RELIABILITY_BASE_URL || "http://127.0.0.1:18082";
const websocketUrl = baseUrl.replace(/^http/, "ws") + "/ws";
const duration =
  __ENV.RELIABILITY_DURATION || (profile === "soak" ? "30m" : "20s");
const vus = Number(__ENV.RELIABILITY_VUS || (profile === "soak" ? 4 : 8));
const protocolHeaders = {
  Accept: "application/json",
  "Content-Type": "application/json",
  "x-mk01-protocol-version": "2",
};

const workflowFailures = new Rate("workflow_failures");
const websocketFailures = new Rate("websocket_failures");
const criticalDuration = new Trend("critical_http_duration", true);

const thresholds = {
  checks: ["rate>0.99"],
  workflow_failures: ["rate<0.01"],
  websocket_failures: ["rate<0.01"],
  critical_http_duration: ["p(95)<250", "p(99)<600"],
  http_req_failed: ["rate<0.01"],
};

export const options =
  profile === "soak"
    ? {
        scenarios: {
          authenticated_soak: {
            executor: "constant-vus",
            vus,
            duration,
            gracefulStop: "10s",
          },
        },
        thresholds,
        noCookiesReset: true,
      }
    : {
        scenarios: {
          authenticated_load: {
            executor: "ramping-vus",
            startVUs: 1,
            stages: [
              { duration: "5s", target: vus },
              { duration, target: vus },
              { duration: "5s", target: 0 },
            ],
            gracefulRampDown: "5s",
          },
        },
        thresholds,
        noCookiesReset: true,
      };

let sessionReady = false;

function ensureSession() {
  if (sessionReady) return true;
  const response = http.post(
    `${baseUrl}/api/sessions`,
    JSON.stringify({ nickname: `Load${String(__VU).padStart(3, "0")}` }),
    { headers: protocolHeaders, tags: { journey: "session" } },
  );
  sessionReady = check(response, {
    "session created": (result) => result.status === 201,
  });
  workflowFailures.add(!sessionReady);
  return sessionReady;
}

function exerciseHttpJourney() {
  const responses = http.batch([
    [
      "GET",
      `${baseUrl}/api/rooms`,
      null,
      { headers: protocolHeaders, tags: { critical: "true" } },
    ],
    [
      "GET",
      `${baseUrl}/api/games/recover`,
      null,
      { headers: protocolHeaders, tags: { critical: "true" } },
    ],
    [
      "GET",
      `${baseUrl}/api/content/live`,
      null,
      { headers: protocolHeaders, tags: { critical: "true" } },
    ],
  ]);
  let failed = false;
  for (const response of responses) {
    criticalDuration.add(response.timings.duration);
    failed =
      failed ||
      !check(response, {
        "critical API returned 200": (result) => result.status === 200,
      });
  }
  workflowFailures.add(failed);
}

function exerciseWebsocketJourney() {
  let heartbeatReceived = false;
  const response = ws.connect(
    websocketUrl,
    {
      headers: {
        Origin: baseUrl,
        "Sec-WebSocket-Protocol": "mk01.v2",
      },
      tags: { journey: "heartbeat" },
    },
    (socket) => {
      socket.on("open", () => {
        socket.send(
          JSON.stringify({
            type: "heartbeat",
            payload: { clientTime: new Date().toISOString() },
          }),
        );
      });
      socket.on("message", (raw) => {
        const event = JSON.parse(raw);
        if (event.type === "heartbeat") {
          heartbeatReceived = true;
          socket.close();
        }
      });
      socket.setTimeout(() => socket.close(), 1_000);
    },
  );
  const failed = !response || response.status !== 101 || !heartbeatReceived;
  check(response, {
    "websocket upgraded": (result) => Boolean(result) && result.status === 101,
  });
  check(heartbeatReceived, {
    "heartbeat acknowledged": (received) => received,
  });
  websocketFailures.add(failed);
}

export default function () {
  if (!ensureSession()) {
    sleep(1);
    return;
  }
  exerciseHttpJourney();
  exerciseWebsocketJourney();
  sleep(0.25);
}

export function handleSummary(data) {
  const destination =
    __ENV.RELIABILITY_SUMMARY || `.artifacts/${profile}-summary.json`;
  return { [destination]: JSON.stringify(data, null, 2) };
}
