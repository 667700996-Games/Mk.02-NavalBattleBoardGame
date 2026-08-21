# Cloudflare Workers 런타임

Mk.01은 한 저장소와 한 SvelteKit 프런트엔드에서 두 서버 런타임을 병렬로
제공합니다. `apps/server`의 Rust/Axum/Tokio 서버와 Memory/PostgreSQL/Redis 경로는
그대로 유지하며, `apps/worker`는 같은 공개 API와 WebSocket 게임 계약을
Cloudflare Workers와 SQLite-backed Durable Objects로 구현합니다.

## 요청 경로

Cloudflare 배포는 한 Worker와 한 origin만 노출합니다.

```text
Browser
  ├─ /, /room/*, /_app/* → SvelteKit Worker + static assets
  ├─ /api/*               → Cloudflare HTTP adapter
  └─ /ws                  → authenticated GameRoom Durable Object WebSocket
```

`apps/web/.svelte-kit/cloudflare/_worker.js`는 adapter가 생성하는 SvelteKit
핸들러이고, 실제 진입점 `apps/worker/src/index.ts`가 `/api`와 `/ws`를 먼저
분배한 뒤 나머지를 이 핸들러에 위임합니다. 브라우저의 기본 API/WebSocket
주소는 계속 same-origin 상대 경로입니다.

## Durable Object 구성

| 바인딩        | 클래스                       | 책임                                                                                                                      |
| ------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `GAME_ROOMS`  | `GameRoomDurableObject`      | 방별 강한 일관성, Classic/Rapid/Salvo, AI, 배치, 턴, 판정, 승패, 채팅, 재접속, 관전/리플레이, 알람, hibernating WebSocket |
| `ACCOUNTS`    | `AccountDurableObject`       | 게스트/계정 세션, 복구 키, 토큰 회전, 원격 해제, 자료 내보내기/삭제 조정                                                  |
| `LOBBY`       | `LobbyDurableObject`         | 6자리 초대 코드, 공개 방과 관전 가능 방 색인                                                                              |
| `RATE_LIMITS` | `EdgeRateLimitDurableObject` | IP/세션별 HTTP, 세션 생성, WebSocket 연결 슬라이딩 윈도우                                                                 |
| `PROGRESSION` | `ProgressionDurableObject`   | 전적, XP, 업적, 임무 보상, 랭크 평점/감쇠/시즌 보상/리더보드                                                              |
| `MATCHMAKING` | `MatchmakingDurableObject`   | 일반·랭크 영속 큐, 검색 범위 확대, 리전/지연/평점, 재대전 억제, 중복 매칭 방지                                            |
| `SAFETY`      | `SafetyDurableObject`        | 플레이어 음소거·차단과 채팅·매칭 억제 정책                                                                                |
| `OPERATIONS`  | `OperationsDurableObject`    | 신고 증거, 운영 조치, 지원 감사, 무결성 신호, 퍼널/RUM 집계                                                               |
| `CONTENT`     | `ContentDurableObject`       | 시즌·이벤트·기능 플래그·보상 튜닝의 검증, 예약 발행, 감사 리비전, 롤백                                                    |

게임방은 `roomId`별, 속도 제한은 IP/세션 scope key별 인스턴스를 사용하고 나머지
도메인 색인/원장은 각 namespace의 `global-v1` 인스턴스를 사용합니다. 모든 클래스는
`new_sqlite_classes` migration `v1`~`v8`으로 선언되어 있습니다. D1, KV, R2,
Containers, PostgreSQL, Redis 또는 외부 유료 DB는 Cloudflare 실행의 필수 의존성이
아닙니다.

종료·취소 방은 결과 원장 투영을 마친 뒤 90일 동안 리플레이를 보존하고 DO
alarm으로 정리합니다. 계정 삭제는 세션, 진행도, 소셜, 신고/무결성 자료를 함께
삭제하고 보존 중인 방 기록을 익명화합니다.

## 구현된 계약

- 게스트 생성, 계정 전환/로그인, 복구 키, 세션 목록/원격 해제, 자료 내보내기와 삭제
- 공개·비공개 방, 준비/취소, 방장 시작, 서버 검증 배치, 2인 실제 게임과 항복
- Classic/Rapid/Salvo 규칙, 턴 제한, 연결 끊김 유예, 결과 투영, 채팅/입력 상태
- 신병/장교/제독 AI 연습, 일반/랭크 매칭, 시즌 평점·배치전·보상·감쇠·리더보드
- 영속 전적, 진행도/업적/임무, 참가자 리플레이, 30초 지연 관전
- 친구/파티/직접 초대/presence/최근 상대, 음소거/차단과 통신 필터
- 서버 증거 기반 신고, 경고/정지/차단/취소, 지원 세션 해제 감사, 무결성 탐지
- live content 검증/발행/예약 활성화/롤백과 공개 `/content/live`
- `/health`, 실제 저장소 probe를 수행하는 `/ready`, Prometheus `/metrics`, 퍼널/RUM 수집

HTTP 경로와 JSON/WebSocket 이벤트는 Rust 서버 계약을 유지합니다. 게임 판정과
상대 함선 은닉은 `apps/worker/src/domain`의 순수 상태 머신에서 수행하고, DO는
직렬화·영속화·전송 adapter 역할을 담당합니다.

## 보안 경계

- `mk01_session`은 256-bit 난수 토큰이며 저장소에는 SHA-256 해시만 보관합니다.
- 쿠키는 `HttpOnly`, `SameSite=Lax`, `Path=/`이고 HTTPS에서 `Secure`가 추가됩니다.
- `/ws`는 same-origin, 세션 만료, 계정 정지/차단, 현재 방 멤버십을 검증합니다.
- 세션 해제와 운영 정지/차단은 열린 WebSocket과 매칭 티켓까지 폐기합니다.
- 배치, 명중, 턴, 타이머와 승패는 DO의 권위 상태 머신만 결정합니다. 종료 전
  상대 함대는 참가자나 관전자 응답에 포함하지 않습니다.
- 요청은 64 KiB, 채팅은 100건, 연결별 WebSocket 입력은 초당 60건으로 제한합니다.
- 과도한 이벤트와 불가능한 명령 순서는 운영 무결성 원장에 집계됩니다.
- 운영 API는 32자 이상의 `ADMIN_TOKEN`과 변경 요청의 `X-Operator-Id`를 요구합니다.
  토큰을 저장소나 일반 프런트엔드 환경변수에 커밋하지 않습니다.

## 로컬 실행과 검증

Node.js 22 이상과 npm 11 이상이 필요합니다.

```bash
npm install
npm run dev:cloudflare
```

Wrangler 기본 주소 `http://localhost:8787`에서 UI, `/api`, `/ws`, 로컬 Durable
Objects가 함께 실행됩니다. 운영 API까지 수동 검사할 때만 별도 터미널에서 다음처럼
로컬 전용 토큰을 주입합니다.

```bash
npm run build:web:cloudflare
npm --workspace @mk01/worker run dev -- --var ADMIN_TOKEN:replace-with-32-plus-local-characters
```

전체 정적/브라우저 검증 명령은 다음과 같습니다.

```bash
npm run check
npm --workspace @mk01/worker run test
npm run build:cloudflare
npx playwright install chromium
npm run test:e2e:cloudflare
```

Cloudflare E2E는 계정 수명주기, 실제 2인 전체 경기, 새로고침/재접속, 채팅/항복,
AI, 매칭/랭크, 관전, 소셜, 신고/운영/지원, live content, 퍼널과 RUM을 독립
브라우저 컨텍스트로 검증합니다.

## 수동 배포

Wrangler 로그인과 빌드 후 Worker를 배포합니다. 운영 API를 사용할 배포는 비밀을
한 번 등록합니다. 이후 버전 배포에서도 binding은 유지됩니다.

```bash
npm ci
npx wrangler login
npm run build:cloudflare
npm run deploy:cloudflare
npx wrangler secret put ADMIN_TOKEN --config apps/web/wrangler.jsonc
```

첫 배포는 `v1`~`v7` SQLite migration과 위 아홉 namespace를 생성합니다. 그 뒤
`secret put`이 운영 비밀을 포함한 새 Worker 버전을 배포합니다. Worker 이름은
`mk-01-gameproject-navalbattleboardgame`입니다. 배포 후 다음 probe를 확인합니다.

```bash
curl -fsS https://YOUR-WORKER.workers.dev/api/health
curl -fsS https://YOUR-WORKER.workers.dev/api/ready
curl -fsS https://YOUR-WORKER.workers.dev/api/metrics | head
```

## GitHub 자동 배포

`.github/workflows/cloudflare-deploy.yml`은 `main`의 Cloudflare 관련 파일 변경 시
검사, Worker 테스트, SvelteKit 빌드 후 하나의 Worker를 배포합니다.

GitHub repository 설정에 다음을 추가합니다.

- Actions secret `CLOUDFLARE_API_TOKEN`: 대상 Worker와 Durable Objects 편집 권한의
  최소 권한 API 토큰
- Actions secret `CLOUDFLARE_ACCOUNT_ID`: Cloudflare 계정 ID
- Actions variable `CLOUDFLARE_DEPLOY_ENABLED=true`: `main` push 자동 배포 활성화

`ADMIN_TOKEN`은 GitHub secret로 Worker에 전달하지 않고 Wrangler의 Worker secret로
별도 등록합니다. variable이 없으면 push deploy job은 skip되고
`workflow_dispatch`는 수동 실행할 수 있습니다. Cloudflare Workers Builds를 함께
설정했다면 중복 배포가 되지 않도록 한 경로만 활성화합니다.

## Free Plan과 런타임 차이

이 구성은 Cloudflare Containers나 Paid 전용 KV-backed DO를 사용하지 않습니다.
Cloudflare 공식 문서상 Workers Free는 일일 Worker/DO 요청 및 SQLite 행 읽기·쓰기,
총 저장량 한도가 있으며 초과 작업은 다음 UTC 리셋까지 실패합니다. 현재 한도는
[Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/)과
[Durable Objects pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/)에서
배포 직전에 다시 확인하십시오.

기능 계약은 양쪽 런타임에 구현되어 있지만 운영 방식은 다음처럼 다릅니다.

- Rust 운영은 PostgreSQL CAS/임대와 Redis fan-out/공유 제한을 사용하고,
  Cloudflare 운영은 방별 DO 직렬화와 SQLite storage를 사용합니다.
- Rust의 환경변수로 조정 가능한 일부 용량/타이머 정책은 Cloudflare에서 Free Plan용
  고정 안전값을 사용합니다(60초 턴, 90초 재접속, 90일 종료 방 보존).
- Cloudflare `/metrics`는 DO에 누적한 게임 제품 지표를 노출하지만 이 저장소가
  Prometheus/Grafana 외부 수집기를 Cloudflare에 자동 설치하지는 않습니다.
- 전역 계정·매칭·진행도·소셜·운영 색인은 소규모 Free Plan 배포에 맞춘 singleton
  DO입니다. 한도에 접근하는 트래픽에서는 namespace sharding과 유료 용량 검토가
  필요합니다.
- 게스트 기록은 해당 세션에 연결됩니다. 여러 기기에서 장기 보존하려면 계정으로
  전환하고 한 번 표시되는 복구 키를 안전하게 보관해야 합니다.
