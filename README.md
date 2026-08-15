# Mk.01 — Naval Battle Board Game

![Mk.01 함대 작전 이미지](apps/web/static/og-mk01.png)

`Mk.01-GameProject-NavalBattleBoardGame`은 두 명의 플레이어가 각자의 10×10 해역에 함선을 비공개로 배치하고, WebSocket으로 턴을 교대하며 상대 함대를 먼저 격침하는 실시간 전략 게임입니다. 함선 좌표, 공격 판정, 턴, 승패는 Rust 서버만 관리하며 상대 함선 좌표는 클라이언트로 전송하지 않습니다.

## 제공 기능

- 안전한 익명 게스트 세션, 공개·비공개 방, 6자리 방 코드와 초대 링크
- 실시간 대기실, 빠른 매칭, 참가·양쪽 준비·방장 승인·배치·턴·결과 동기화
- 드래그 앤 드롭, 클릭/터치, `R` 회전, 자동 배치, 초기화가 가능한 함대 배치기
- 서버 권위형 규칙 검증, 턴/버전 순서 검증, UUID 요청 멱등성
- 새로고침 복구, 지수 백오프 재연결, 재접속 유예 시간과 자동 기권승 처리
- 서버 검증형 기권과 `Normal Victory`/`Surrender`/`Disconnect` 승리 원인 기록
- `HOST`/`GUEST` 역할과 서버 승인형 준비 토글, 방장 전용 명시적 작전 시작, 요청 UUID 기반 멱등 처리
- 방 단위 실시간 전술 채팅, 타입화된 빠른 명령·Unicode 이모지, 입력 중 표시, 최근 100개 메시지 복구
- 서버 UTC 기준 전체 작전 시간과 턴 마감, 시간 초과 턴 자동 교대와 3회 연속 초과 자동 패배
- `NORMAL`/`SURRENDER`/`DISCONNECT`/`TIMEOUT` 종료 원인과 시간 초과 통계 기록
- 재경기, 승패·명중률·턴·플레이 시간 통계, 전투 기록
- 데스크톱 2보드 레이아웃과 모바일 탭 전환, 키보드 조작, 고대비/모션 감소, 사운드 설정
- PostgreSQL 영속화, Redis 읽기 캐시, 구조화 JSON 로그, Docker Compose 운영 구성

## 게임 규칙

세로축은 A–J, 가로축은 1–10입니다. 방에 두 플레이어가 입장하고 양쪽이 대기실 준비를 완료해도 자동으로 진행되지 않으며, 방장이 `작전 시작`을 승인해야 함선 배치 단계로 이동합니다. 각 플레이어는 항공모함 5칸, 전함 4칸, 순양함 3칸, 잠수함 3칸, 구축함 2칸을 가로 또는 세로로 겹치지 않게 배치합니다. 두 플레이어가 모두 배치를 확정하면 서버가 선공을 무작위로 선택합니다.

한 턴에 좌표 하나만 공격할 수 있고, 이미 공격한 좌표는 다시 선택할 수 없습니다. 결과는 `MISS`, `HIT`, `SUNK`로 구분됩니다. 모든 함선의 17칸을 먼저 명중시킨 플레이어가 승리합니다.

기본 턴 제한은 60초입니다. 마감까지 공격하지 않으면 공격 없이 상대 턴으로 넘어가며, 같은 플레이어가 3회 연속으로 시간을 초과하면 `TIMEOUT` 패배가 확정됩니다. 정상 공격을 수행하면 해당 플레이어의 연속 초과 횟수는 0으로 초기화됩니다. 대기실 준비는 방장이 작전을 시작하기 전까지만 취소할 수 있고, 함선 배치 확정은 대기실 준비와 별개의 서버 상태입니다.

## 기술 스택

| 영역          | 기술                                                      |
| ------------- | --------------------------------------------------------- |
| 프런트엔드    | SvelteKit 2, Svelte 5, TypeScript 6, CSS 디자인 토큰      |
| API/게임 서버 | Rust 1.87, Axum 0.8, Tokio                                |
| 실시간        | WebSocket, 타입화 JSON 이벤트                             |
| 저장소        | PostgreSQL 17, SQLx migration, Redis 7.4                  |
| 테스트        | Rust unit/integration, Vitest, Playwright Chromium/WebKit |
| 운영          | adapter-node, Docker Compose, Caddy                       |

## 시스템 아키텍처

```mermaid
flowchart LR
  A[플레이어 A 브라우저] <-->|HTTPS / WSS| G[Caddy 게이트웨이]
  B[플레이어 B 브라우저] <-->|HTTPS / WSS| G
  G --> W[SvelteKit adapter-node]
  G --> S[Rust Axum 서버]
  S -->|room snapshot / result| P[(PostgreSQL)]
  S -.->|best-effort room cache| R[(Redis)]
```

브라우저는 자신의 배치/보드와 자신이 실행한 공격 결과만 받습니다. `GameRoom` 내부 스냅샷은 최근 채팅 100개를 포함해 PostgreSQL JSONB에 원자적으로 저장되고 Redis는 1시간 읽기 캐시로만 사용됩니다. Redis가 중단되어도 PostgreSQL로 게임을 계속할 수 있습니다. 채팅 추가는 공격용 게임 버전을 변경하지 않아 동시에 도착한 사격 요청과 충돌하지 않습니다.

상태 머신은 다음 전이만 허용합니다.

```text
WAITING_FOR_OPPONENT
  → WAITING_FOR_READY
  ⇄ READY_TO_START
  → PLACEMENT          (방장의 유효한 game:start만 허용)
  → PLAYING            (양쪽 함선 배치 확정)
  → FINISHED

대기실/배치/전투 → CANCELLED  (정책에 따른 방 종료)
FINISHED → WAITING_FOR_READY   (두 명 모두 재경기 동의)
```

`readyState`는 대기실 준비, `placementConfirmed`는 함선 배치 확정을 표현합니다. 두 플레이어가 준비되면 상태만 `READY_TO_START`가 되며 게임 객체나 배치 단계는 생성되지 않습니다. `game:start`와 `player:unready`는 같은 방 단위 Tokio 잠금에서 직렬화되므로 먼저 승인된 유효 전이 하나만 성공합니다. 연결 끊김은 별도 방 상태를 만들지 않고 각 플레이어의 `connectionState`로 표현합니다.

## 디렉터리 구조

```text
apps/
  server/                 Rust API·WebSocket·도메인·저장소
    migrations/           PostgreSQL SQLx 마이그레이션
    src/domain/           보드, 함선, 턴, 방 상태 머신
    src/store/            Memory / PostgreSQL+Redis 저장소
    tests/                HTTP 통합 테스트
  web/                    SvelteKit 웹 클라이언트
    src/lib/components/   배치·전투·결과 컴포넌트
    src/routes/           시작, 로비, 참가, 방, 기록, 설정, 오류
    e2e/                  2-브라우저 전체 경기·모바일 테스트
deploy/Caddyfile          로컬 Compose 게이트웨이
compose.yaml              PostgreSQL, Redis, server, web, gateway
```

## 로컬 실행

### 1. 빠른 개발 모드

필요 도구는 Rust 1.87 이상, Node.js 22 이상, npm 11 이상입니다.

```bash
cp .env.example .env
npm install
npm run dev
```

`http://localhost:5173`에 접속합니다. 기본 `STORAGE_MODE=memory`는 DB 없이 즉시 실행하기 위한 개발 모드이며 서버 종료 시 데이터가 사라집니다.

기존 개발 서버가 5173/8080 포트에 남아 있으면 새 프론트엔드가 이전 서버 계약과 혼용될 수 있습니다. 이 경우 실행 중인 `npm run dev`를 완전히 종료한 뒤 다시 시작하세요. 클라이언트는 `protocolVersion` 불일치를 감지하며, 이전 서버의 `WAITING`/자동 `PLACEMENT` 응답을 취소나 게임 시작으로 잘못 표시하지 않고 재시작 안내를 표시합니다.

### 2. 영속 로컬 스택

Docker Compose 2 또는 호환되는 Podman Compose가 필요합니다.

```bash
POSTGRES_PASSWORD='replace-this-local-password' docker compose up --build
```

`http://localhost:8088`에 접속합니다. PostgreSQL과 Redis 데이터는 각각 named volume에 보존됩니다. SQLx 마이그레이션은 Rust 서버 시작 시 자동 적용됩니다.

## 환경 변수

| 변수                      | 기본값                    | 설명                                          |
| ------------------------- | ------------------------- | --------------------------------------------- |
| `SERVER_HOST`             | `0.0.0.0`                 | Rust 서버 바인드 주소                         |
| `SERVER_PORT`             | `8080`                    | Rust 서버 포트                                |
| `STORAGE_MODE`            | `memory`                  | `memory` 또는 `postgres`                      |
| `DATABASE_URL`            | 예제 참조                 | PostgreSQL 접속 URL                           |
| `REDIS_URL`               | `redis://localhost:6379/` | Redis 접속 URL                                |
| `PUBLIC_BASE_URL`         | `http://localhost:5173`   | 초대 URL에 쓰이는 공개 주소                   |
| `ALLOWED_ORIGINS`         | localhost 2개             | 쉼표로 구분한 CORS/WebSocket Origin 허용 목록 |
| `SECURE_COOKIES`          | `false`                   | HTTPS 운영에서는 반드시 `true`                |
| `SESSION_TTL_SECONDS`     | `2592000`                 | 게스트 세션 유효 기간                         |
| `RECONNECT_GRACE_SECONDS` | `90`                      | 재접속 유예 시간                              |
| `TURN_DURATION_SECONDS`   | `60`                      | 턴 제한(초), `0`이면 제한 없음                |
| `API_REQUESTS_PER_MINUTE` | `240`                     | 인증 세션별 HTTP/연결 요청 한도                |
| `SESSION_CREATIONS_PER_MINUTE` | `20`                | 클라이언트 IP별 게스트 세션 생성 한도          |
| `WEBSOCKET_EVENTS_PER_SECOND` | `60`                 | 세션별 수신 WebSocket 이벤트 한도              |
| `WEBSOCKET_SEND_QUEUE_CAPACITY` | `256`               | 연결별 송신 큐 한도; 초과 시 느린 연결 종료    |
| `MAX_WEBSOCKET_CONNECTIONS` | `10000`                 | 서버 인스턴스별 동시 WebSocket 연결 한도       |
| `TRUST_PROXY_HEADERS`     | `false`                   | 신뢰 프록시 뒤에서만 전달 IP 헤더 사용         |
| `RUST_LOG`                | info                      | `tracing` 로그 필터                           |

## REST API

모든 경로의 prefix는 `/api`입니다. 세션 생성 후 발급된 `mk01_session` HttpOnly 쿠키를 사용하며 JSON에 토큰을 반환하지 않습니다.

| Method        | 경로                    | 기능                                     |
| ------------- | ----------------------- | ---------------------------------------- |
| `GET`         | `/health`               | 프로세스 liveness                         |
| `GET`         | `/ready`                | 주 저장소 연결을 포함한 readiness          |
| `POST`        | `/sessions`             | 닉네임 검증 후 게스트 세션 생성          |
| `GET/DELETE`  | `/sessions/current`     | 현재 세션 복구 / 서버 세션 폐기           |
| `GET/POST`    | `/rooms`                | 공개 방 목록 / 방 생성                   |
| `POST`        | `/rooms/join`           | 방 코드로 참가                           |
| `GET`         | `/rooms/{roomId}`       | 본인 기준 비공개 필터 스냅샷             |
| `POST`        | `/rooms/{roomId}/leave` | 전투 전 방 나가기 또는 전투 중 이탈 처리 |
| `GET`         | `/games/recover`        | 진행 중 게임 복구                        |
| `GET`         | `/games/history`        | 최근 50개 경기 결과                      |
| `POST/DELETE` | `/matchmaking`          | 빠른 매칭 대기/취소                      |

오류는 `{ code, message, requestId }` 형태의 안전한 JSON으로 반환됩니다. 잘못된 JSON·UUID도 내부 파서 정보 대신 `INVALID_REQUEST`로 일관되게 처리합니다.

## WebSocket 이벤트 계약

연결 경로는 `/ws`이며 쿠키 세션과 `Origin` 허용 목록을 모두 검증합니다. 공통 envelope는 다음과 같습니다.

```json
{
  "type": "attack:fire",
  "payload": {
    "requestId": "uuid",
    "roomId": "uuid",
    "playerId": "uuid",
    "coordinate": { "row": 0, "col": 0 },
    "expectedVersion": 12,
    "turnNumber": 4
  }
}
```

| 클라이언트 이벤트 | 핵심 payload                                                                     |
| ----------------- | -------------------------------------------------------------------------------- |
| `room:create`     | `name`, `visibility`                                                             |
| `room:join`       | `code`                                                                           |
| `room:leave`      | `roomId`                                                                         |
| `player:ready`    | `requestId`, `roomId`, `playerId`                                                |
| `ships:place`     | `roomId`, `playerId`, `placements[]`                                             |
| `ships:confirm`   | `roomId`, `playerId`, `placements[]`                                             |
| `player:unready`  | `requestId`, `roomId`, `playerId`                                                |
| `game:start`      | `requestId`, `roomId`, `playerId`, `roomVersion`                                 |
| `attack:fire`     | `requestId`, `roomId`, `playerId`, `coordinate`, `expectedVersion`, `turnNumber` |
| `game:surrender`  | `roomId`, `playerId`                                                             |
| `chat:send`       | `roomId`, `clientMessageId`, `type`, `content`, `commandId`                      |
| `chat:typing`     | `roomId`, `isTyping`                                                             |
| `game:rematch`    | `roomId`                                                                         |
| `game:sync`       | `roomId`                                                                         |
| `heartbeat`       | `clientTime`                                                                     |

| 서버 이벤트                                 | 용도                                            |
| ------------------------------------------- | ----------------------------------------------- |
| `room:created`, `room:updated`              | 방 생성/상태·역할·준비·연결 갱신                |
| `player:joined`, `player:left`              | 참가자 변경                                     |
| `player:ready:accepted/rejected`            | 대기실 준비 승인/거절과 멱등 결과               |
| `player:unready:accepted/rejected`          | 대기실 준비 취소 승인/거절과 멱등 결과          |
| `game:start:accepted/rejected`              | 방장 시작 요청의 검증 결과                      |
| `game:placement-started`                    | 양쪽 클라이언트의 함선 배치 단계 개시           |
| `placement:accepted`, `placement:rejected`  | 배치 검증 결과                                  |
| `game:started`, `turn:started/changed`      | 선공/턴과 UTC 마감 시각 동기화                  |
| `turn:expired`, `game:timer-sync`           | 시간 초과 판정과 재접속 타이머 보정             |
| `attack:result`, `ship:sunk`                | 공격·격침 결과                                  |
| `game:finished`                             | 승패와 통계                                     |
| `game:surrendered`                          | 기권자·승자·서버 판정 시각                      |
| `chat:message`                              | 플레이어 또는 `SYSTEM` 작전 메시지              |
| `chat:history`                              | 방의 최근 메시지 최대 100개                     |
| `chat:typing`                               | 상대 지휘관 입력 상태(비영속)                   |
| `chat:rejected`                             | 메시지 타입·허용 목록·속도 제한 거절            |
| `player:disconnected`, `player:reconnected` | 연결 상태                                       |
| `game:snapshot`                             | 세션별 전체 공개 상태                           |
| `heartbeat`                                 | 연결 생존 확인                                  |
| `error`                                     | `code`, 사용자 메시지, `retryable`, `requestId` |

`room:updated`/`game:snapshot`은 `protocolVersion`, `roomId`, `roomState`, `hostPlayerId`, `players`, `canStartGame`, `roomVersion`, `gameId`, `serverTimestamp`를 포함합니다. 현재 계약 버전은 `2`이며 `/api/health`와 `/api/rooms`에서도 같은 버전을 제공합니다. 플레이어 공개 상태에는 `role`, `readyState`, `connectionState`, `joinedAt`, `readyAt`이 포함되지만 세션 토큰과 `sessionId`는 포함되지 않습니다. `canStartGame`은 UI 힌트일 뿐이며 서버는 방장·인원·양쪽 준비·연결·상태·버전·중복 요청을 잠금 안에서 다시 검증합니다.

`row`/`col`은 0–9입니다. `ships:place`는 항공모함·전함·순양함·잠수함·구축함을 각각 한 번씩 정확히 포함해야 합니다. 확정 시 같은 배치를 다시 보내며 서버 저장본과 일치해야 합니다. 서버는 세션으로 실제 `playerId`와 방장 ID를 다시 확인하며, 준비·시작·공격·채팅 UUID가 중복되면 기존 결과만 재전송합니다. 공격은 현재 `turnNumber`와 서버 마감 시각을 모두 통과해야 합니다.

채팅 `type`은 클라이언트가 보낼 수 있는 `TEXT`, `QUICK_COMMAND`, `EMOJI`와 서버 전용 `SYSTEM`으로 나뉩니다. 빠른 명령은 `GOOD_GAME`, `WAIT_A_MOMENT`, `READY`, `NICE_SHOT`, `LUCKY`, `GO_FIRST`, `REMATCH`, `THANK_YOU` ID만 허용하고 표시 문구는 서버가 결정합니다. 이모지는 저장소의 공유 허용 목록에 있는 Unicode 값만 받습니다. `chat:send`와 `chat:typing`은 현재 방 멤버에게만 전달됩니다.

`turn:started`, `turn:changed`, `game:timer-sync`는 `gameId`, `turnNumber`, `activePlayerId`, `gameStartedAt`, `turnStartedAt`, `turnDeadlineAt`, `turnDurationSeconds`, `serverTimestamp`를 제공합니다. 클라이언트는 절대 마감 시각과 서버 시각 오차로 화면만 계산하며, 만료와 승패는 서버 스케줄러만 확정합니다.

## 재접속과 게임 복구 정책

- WebSocket이 끊기면 해당 플레이어를 `RECONNECTING`으로 표시하고 현재 방 상태, 준비 상태, 배치와 절대 마감 시각을 그대로 저장합니다.
- 클라이언트는 0.6–10초 지수 백오프로 재연결하고, 연결 즉시 `game:sync`로 자신에게 허용된 스냅샷과 `chat:history`를 받습니다.
- 기본 90초 안에 같은 HttpOnly 세션으로 복귀하면 역할·준비·연결·방 버전을 서버 스냅샷에서 복원합니다. 준비가 유지되더라도 연결이 복구되기 전에는 시작할 수 없습니다.
- 대기실 참가자의 유예 시간이 끝나면 슬롯에서 제거하고 방장을 `NOT_READY`로 초기화합니다. 대기실 방장의 유예 시간이 끝나면 방을 종료합니다. 전투 중에는 온라인 상대의 기권승으로 처리합니다.
- 서버 재시작 시 활성 방, 재접속 마감, `turnDeadlineAt`을 PostgreSQL JSONB에서 다시 불러옵니다. 이미 지난 턴은 턴 번호·활성 플레이어·마감 키를 대조해 한 번만 즉시 만료 처리합니다.

## 보안 설계

- 256-bit 무작위 게스트 토큰을 HttpOnly, SameSite=Lax 쿠키로 전달하고 SHA-256 해시만 저장합니다.
- WebSocket upgrade의 `Origin`을 명시적 허용 목록으로 검증해 cross-site WebSocket hijacking을 차단합니다.
- JSON은 알 수 없는 필드를 거부하고 본문/프레임을 64 KiB로 제한합니다.
- 채팅은 HTML 구분자·제어 문자를 거부하고 Svelte 텍스트 바인딩으로만 렌더링합니다. 일반 텍스트는 300자로 제한하며 이모지·빠른 명령도 포함해 플레이어별 2초당 3회, 10초당 8회 제한과 3초 차단을 적용합니다. 같은 빠른 명령의 2초 이내 반복도 거부합니다.
- 배치, 턴, 공격, 격침, 승패, 요청 순서를 모두 서버에서 재검증합니다.
- 클라이언트 스냅샷은 세션별로 생성되며 상대 `Board`, 함선 좌표, 세션 ID/토큰을 포함하지 않습니다.
- DB/Redis 오류는 구조화 로그에만 원인을 남기고, 클라이언트에는 안전한 오류 코드와 추적 UUID만 보냅니다.
- E2E는 모든 수신 WebSocket 프레임을 감사해 `targetBoard.ships`/`sessionId`가 없음을 확인합니다.

취약점 제보 절차는 [SECURITY.md](SECURITY.md)를 참조하세요.

## 테스트와 품질 검사

```bash
npm run check       # Rust check + Svelte/TypeScript check
npm run lint        # rustfmt + clippy -D warnings + Prettier + ESLint
npm run test        # Rust unit/integration + Vitest
npx playwright install chromium webkit
npm run test:e2e    # 독립 브라우저 2개의 전체 경기 + 모바일
npm run build       # Rust release + SvelteKit adapter-node
```

Rust 테스트는 대기실 7단계 상태 머신, 준비 멱등성, 방장 권한, 오래된 버전, 연결 끊김, 시작/준비 취소 경쟁, 참가자 이탈 초기화, 배치, 명중/격침, 공격/만료 경쟁, 3회 만료 패배, 재시작 복구, 채팅 검증, JSON 영속화와 공개 정보 필터를 검증합니다. Playwright는 두 브라우저가 참가 후 대기실에 머무르는지, 양쪽 준비 후에도 자동 시작되지 않는지, 참가자의 변조된 `game:start`가 `NOT_HOST`로 거부되는지, 새로고침 후 준비가 복구되는지와 이후 전체 게임 회귀를 검증합니다.

## 프로덕션 빌드·배포

```bash
npm ci
npm run build
STORAGE_MODE=postgres DATABASE_URL='postgres://...' REDIS_URL='redis://...' \
  PUBLIC_BASE_URL='https://game.example.com' \
  ALLOWED_ORIGINS='https://game.example.com' SECURE_COOKIES=true \
  ./target/release/mk01-server

HOST=0.0.0.0 PORT=3000 ORIGIN='https://game.example.com' node apps/web/build
```

운영에서는 API와 웹을 같은 HTTPS origin으로 reverse proxy하고 `/api/*`, `/ws`를 Rust 서버로 전달하세요. TLS 종단, 영속 볼륨, PostgreSQL 백업, 로그 수집, 헬스체크를 활성화해야 합니다. 제공된 `compose.yaml`과 `deploy/Caddyfile`은 로컬/단일 호스트 기준이므로 공개 도메인에서는 `PUBLIC_BASE_URL`, `ALLOWED_ORIGINS`, `ORIGIN`, Caddy 주소, 80/443 포트를 해당 도메인으로 변경하세요.

## 알려진 제한

- 빠른 매칭 큐와 WebSocket 연결 허브는 현재 서버 프로세스 내부에 있습니다. 현 배포는 Rust 서버 1개 replica를 기준으로 하며, 수평 확장 시 Redis Pub/Sub 또는 별도 실시간 게이트웨이가 필요합니다.
- 턴 만료 스케줄러도 현재 단일 Rust 서버의 방 잠금과 영속 마감 키를 기준으로 합니다. 다중 replica 배포 전에는 Redis 분산 락/작업 큐로 동일한 `(roomId, turnNumber, activePlayerId, deadline)` 소유권을 조정해야 합니다.
- 게스트 세션은 디바이스 간 계정 동기화를 제공하지 않습니다. 브라우저 쿠키를 삭제하면 기존 게스트 기록에 다시 접근할 수 없습니다.
- SvelteKit 2.70.2의 안정화 버전은 `cookie@0.6` 의존성에 대한 low-severity 이름/경로 문자 검증 권고를 남깁니다. 이 앱은 사용자 입력으로 SvelteKit 쿠키 이름·경로를 생성하지 않고, 인증 쿠키는 Rust의 고정 이름 `mk01_session`으로만 발급합니다. SvelteKit 3 안정화 후 업그레이드를 권장합니다.

## 라이선스

[MIT](LICENSE)
