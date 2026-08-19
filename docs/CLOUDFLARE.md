# Cloudflare Workers 런타임

Mk.01은 하나의 프런트엔드와 두 가지 서버 런타임을 병렬로 제공합니다. 기존
`apps/server` Rust/Axum 서버는 Memory, PostgreSQL, Redis 실행을 계속 담당하고,
`apps/worker`는 Cloudflare Workers와 Durable Objects에서 같은 `/api` 및 `/ws`
계약의 실시간 게임을 제공합니다.

## 요청 경로

Cloudflare에서는 하나의 Worker가 동일 출처를 제공합니다.

```text
Browser
  ├─ /, /room/*, /_app/* → SvelteKit Worker + static assets
  ├─ /api/*             → Cloudflare HTTP adapter
  └─ /ws                → authenticated GameRoom Durable Object WebSocket
```

SvelteKit adapter의 생성물은 `apps/web/.svelte-kit/cloudflare/_worker.js`에만 쓰여지고,
실제 배포 진입점인 `apps/worker/src/index.ts`를 덮어쓰지 않습니다. 진입점은
API/WebSocket을 먼저 분배하고 나머지 요청을 SvelteKit Worker에 위임합니다.

## Durable Object 구성

| 바인딩        | 클래스                       | 책임                                                                              |
| ------------- | ---------------------------- | --------------------------------------------------------------------------------- |
| `GAME_ROOMS`  | `GameRoomDurableObject`      | 방별 강한 일관성, 배치, 턴, 판정, 승패, 채팅, 재접속, 알람, hibernating WebSocket |
| `ACCOUNTS`    | `AccountDurableObject`       | 게스트/계정 세션, 복구 키 해시, 원격 세션 해제, 현재 방                           |
| `LOBBY`       | `LobbyDurableObject`         | 6자리 초대 코드 유일성과 공개 방 색인                                             |
| `RATE_LIMITS` | `EdgeRateLimitDurableObject` | IP/세션별 HTTP, 세션 생성, WebSocket 연결 슬라이딩 윈도우                         |

모든 클래스는 Free Plan에서 사용 가능한 SQLite-backed Durable Object migration을
사용합니다. 현재 Cloudflare 런타임은 D1, KV, R2, Containers, 외부 DB를 필수로
사용하지 않습니다.

## 보안 경계

- `mk01_session`은 256-bit 난수 토큰이며 저장소에는 SHA-256 해시만 보관합니다.
- 쿠키는 `HttpOnly`, `SameSite=Lax`, `Path=/`이고 HTTPS에서 `Secure`가 추가됩니다.
- `/ws`는 동일 Origin, 세션 만료, 현재 방 멤버십을 검증한 뒤 방 DO에
  내부 세션 ID만 전달합니다.
- 현재/원격 세션 해제는 인증 레코드를 지우는 것에서 끝나지 않고, 해당 방 DO의
  열린 WebSocket을 닫고 서버 재접속 유예 상태를 기록합니다.
- 함선 배치, 명중 판정, 턴, 타이머, 승패는 모두 DO의 도메인 상태 머신이
  결정합니다. 종료 전에 상대 함대는 snapshot에 포함되지 않습니다.
- 요청 크기는 64 KiB, 채팅은 최근 100건, WebSocket 수신은 연결별 초당 60건으로
  제한합니다.
- 배포에 런타임 secret이 필요하지 않으며 Cloudflare API 토큰은 GitHub Actions에만
  저장합니다.

## 로컬 실행과 검증

Node.js 22 이상과 npm 11 이상이 필요합니다.

```bash
npm install
npm run dev:cloudflare
```

Wrangler의 기본 주소 `http://localhost:8787`에서 홈, API, WebSocket이 함께
제공됩니다. 로컬 DO 상태는 Wrangler의 로컬 저장소에 보존됩니다.

```bash
npm run check
npm --workspace @mk01/worker run test
npm run build:cloudflare
npx playwright install chromium
npm run test:e2e:cloudflare
```

마지막 명령은 독립 브라우저 컨텍스로 다음 세 시나리오를 검증합니다.

1. 준비, 방장 시작, 배치, 33회의 공격 교환, 함대 전멸 승패, 상대 함대 비노출,
   전투 중 새로고침 복구
2. 채팅, 입력 상태, 채팅 기록 복구, WebSocket 재접속, 항복, 양쪽 즉시 승패
3. 게스트 생성, 계정 전환과 토큰 교체, 두 번째 기기 로그인, 세션 목록,
   원격 해제, 기존 WebSocket 종료, 해시 비노출

## 수동 배포

Cloudflare 계정에 Wrangler로 로그인한 뒤 다음을 실행합니다.

```bash
npm ci
npm run build:cloudflare
npm run deploy:cloudflare
```

첫 배포에서 `v1`/`v2` SQLite Durable Object migration과 네 바인딩이 자동으로
적용됩니다. Worker 이름은
`mk-01-gameproject-navalbattleboardgame`입니다.

## GitHub 자동 배포

`.github/workflows/cloudflare-deploy.yml`은 `main`의 Cloudflare 관련 파일이 변경될 때
검사, 테스트, SvelteKit 빌드 후 하나의 Worker를 배포합니다.

GitHub repository 설정에 다음을 추가합니다.

- Actions secret `CLOUDFLARE_API_TOKEN`: 해당 Worker와 Durable Objects 편집 권한을 가진
  최소 권한 API 토큰
- Actions secret `CLOUDFLARE_ACCOUNT_ID`: Cloudflare 계정 ID
- Actions variable `CLOUDFLARE_DEPLOY_ENABLED=true`: `main` push 자동 배포 활성화

위 variable이 없으면 push 배포 job은 안전하게 skip됩니다. `workflow_dispatch`는
별도로 수동 실행할 수 있습니다. Cloudflare Dashboard의 Workers Builds를 이미
사용하는 경우 중복 배포를 피하려면 두 방식 중 하나만 활성화합니다.

## 런타임 차이

Cloudflare 런타임의 현재 완전 지원 범위는 게스트/계정 세션, 방 목록·생성·참가,
대기실 준비/시작, 배치, Classic/Rapid/Salvo 전투, 턴 마감, 재접속 유예,
채팅, 항복입니다. 게임 방 상태와 계정/세션은 Durable Object storage에
영속화됩니다.

Rust/PostgreSQL 런타임의 AI 연습, 빠른/랭크 매칭, 전적·리플레이·관전,
XP/미션/랭킹, 소셜/신고/운영자/무결성 운영 API는 아직 Cloudflare adapter에
연결되지 않았습니다. 이 경로들은 거짓 응답을 반환하지 않고 명시적 404로 종료됩니다.
이 영속 운영 기능이 필요한 배포는 현재 Rust/PostgreSQL/Redis 런타임을 사용해야 합니다.
