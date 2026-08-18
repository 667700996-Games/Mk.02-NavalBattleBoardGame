<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    ArrowRight,
    Bot,
    DoorOpen,
    History,
    KeyRound,
    LockKeyhole,
    Plus,
    Radio,
    RefreshCw,
    Search,
    ShieldCheck,
    X
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { trackFunnelAbandoned, trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, session, socketStatus } from '$lib/stores';
  import { Avatar, Badge, Button, Field, Modal, Skeleton, Surface } from '$lib/ui';
  import type {
    AiDifficulty,
    GameMode,
    MatchmakingPool,
    MatchmakingPreferences,
    MatchmakingRegion,
    MatchmakingResponse,
    MatchmakingTicket,
    RoomSummary,
    RoomVisibility
  } from '$lib/types';

  let rooms: RoomSummary[] = [];
  let loading = true;
  let error = '';
  let showCreate = false;
  let showJoin = false;
  let roomName = '북태평양 교전';
  let visibility: RoomVisibility = 'PUBLIC';
  let gameMode: GameMode = 'CLASSIC';
  let turnDurationSeconds = 60;
  let roomCode = '';
  let submitting = false;
  let matching = false;
  let practicing = false;
  let queuedAt: Date | null = null;
  let elapsed = 0;
  let matchPool: MatchmakingPool = 'CASUAL';
  let rankedRegion: MatchmakingRegion = 'KOREA';
  let measuredLatency: number | null = null;
  let matchmakingTicket: MatchmakingTicket | null = null;

  onMount(() => {
    let refreshTimer: ReturnType<typeof setInterval>;
    let queueTimer: ReturnType<typeof setInterval>;
    let matchmakingPollTimer: ReturnType<typeof setInterval>;
    let unsubscribe: (() => void) | undefined;
    (async () => {
      try {
        const current = await api.currentSession();
        session.set(current);
        trackFunnelReached('lobby_entered');
        const recovered = await api.recover();
        if (recovered && recovered.room.status !== 'CANCELLED') {
          gameSnapshot.set(recovered);
          await goto(resolve('/room/[code]', { code: recovered.room.code }));
          return;
        }
        if (recovered?.room.status === 'CANCELLED') await api.leaveRoom(recovered.room.id);
        realtime.connect();
        await loadRooms();
        refreshTimer = setInterval(loadRooms, 7_500);
        unsubscribe = gameSnapshot.subscribe((snapshot) => {
          if (
            matching &&
            snapshot?.players.length === 2 &&
            (snapshot.room.status === 'WAITING_FOR_READY' ||
              snapshot.room.status === 'READY_TO_START')
          ) {
            goto(resolve('/room/[code]', { code: snapshot.room.code }));
          }
        });
        queueTimer = setInterval(() => {
          elapsed = queuedAt ? Math.floor((Date.now() - queuedAt.getTime()) / 1000) : 0;
        }, 1_000);
        matchmakingPollTimer = setInterval(() => {
          if (matching) void pollMatchmaking();
        }, 3_000);
      } catch (caught) {
        if (caught instanceof ApiError && caught.code === 'SERVER_PROTOCOL_MISMATCH') {
          error = caught.message;
          loading = false;
          return;
        }
        trackFunnelFailure('lobby_entered', 'authentication');
        await goto(resolve('/'));
      }
    })();
    return () => {
      if (refreshTimer) clearInterval(refreshTimer);
      if (queueTimer) clearInterval(queueTimer);
      if (matchmakingPollTimer) clearInterval(matchmakingPollTimer);
      unsubscribe?.();
    };
  });

  async function loadRooms() {
    try {
      rooms = (await api.listRooms()).rooms;
      error = '';
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '공개 방 목록을 불러오지 못했습니다.';
    } finally {
      loading = false;
    }
  }

  async function createRoom() {
    submitting = true;
    error = '';
    try {
      const response = await api.createRoom(roomName, visibility, {
        mode: gameMode,
        turnDurationSeconds: gameMode === 'RAPID' ? 30 : turnDurationSeconds
      });
      gameSnapshot.set(response.snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: response.snapshot.room.code }));
    } catch (caught) {
      trackFunnelFailure('room_joined', 'room_entry');
      error = caught instanceof ApiError ? caught.message : '작전실을 만들지 못했습니다.';
    } finally {
      submitting = false;
    }
  }

  async function joinRoom(code = roomCode) {
    submitting = true;
    error = '';
    try {
      const snapshot = await api.joinRoom(code);
      gameSnapshot.set(snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: snapshot.room.code }));
    } catch (caught) {
      trackFunnelFailure('room_joined', 'room_entry');
      error = caught instanceof ApiError ? caught.message : '작전실에 참가하지 못했습니다.';
    } finally {
      submitting = false;
    }
  }

  async function toggleMatchmaking() {
    if (matching) {
      await api.cancelMatchmaking();
      trackFunnelAbandoned('lobby_entered');
      matching = false;
      queuedAt = null;
      matchmakingTicket = null;
      return;
    }
    try {
      if (matchPool === 'RANKED' && !$session?.accountId) {
        error = '랭크 매칭은 계정 업그레이드 후 이용할 수 있습니다.';
        return;
      }
      if (matchPool === 'RANKED' && measuredLatency === null) {
        await measureLatency();
      }
      const response = await api.enqueueMatchmaking(currentMatchmakingPreferences());
      await acceptMatchmakingResponse(response);
    } catch (caught) {
      trackFunnelFailure('room_joined', 'matchmaking');
      error = caught instanceof ApiError ? caught.message : '빠른 매칭을 시작하지 못했습니다.';
    }
  }

  function currentMatchmakingPreferences(): MatchmakingPreferences | undefined {
    return matchPool === 'RANKED'
      ? { pool: 'RANKED', region: rankedRegion, latencyMs: measuredLatency ?? 300 }
      : undefined;
  }

  async function acceptMatchmakingResponse(response: MatchmakingResponse) {
    matchmakingTicket = response.ticket;
    if (response.snapshot) {
      matching = false;
      gameSnapshot.set(response.snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: response.snapshot.room.code }));
      return;
    }
    matching = true;
    queuedAt = new Date(response.queuedAt ?? Date.now());
  }

  async function pollMatchmaking() {
    try {
      await acceptMatchmakingResponse(
        await api.enqueueMatchmaking(currentMatchmakingPreferences())
      );
    } catch {
      // Keep the durable ticket and retry on the next polling interval.
    }
  }

  async function measureLatency() {
    measuredLatency = await api.measureMatchmakingLatency();
  }

  async function startPractice(difficulty: AiDifficulty) {
    practicing = true;
    error = '';
    try {
      const snapshot = await api.createPractice(difficulty);
      gameSnapshot.set(snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: snapshot.room.code }));
    } catch (caught) {
      trackFunnelFailure('room_joined', 'room_entry');
      error = caught instanceof ApiError ? caught.message : 'AI 전술 훈련을 시작하지 못했습니다.';
    } finally {
      practicing = false;
    }
  }

  const age = (createdAt: string) => {
    const minutes = Math.max(0, Math.floor((Date.now() - new Date(createdAt).getTime()) / 60_000));
    return minutes < 1 ? '방금 전' : `${minutes}분 전`;
  };
</script>

<svelte:head><title>작전 로비 · Mk.01</title></svelte:head>

<div class="lobby shell">
  <header class="lobby-heading">
    <div>
      <div class="heading-signal">
        <Badge tone="success" pulse>COMMAND NETWORK ONLINE</Badge><span
          >SECTOR ACCESS / PACIFIC FLEET</span
        >
      </div>
      <p class="eyebrow">OPERATIONS LOBBY</p>
      <h1 class="page-title">작전 로비</h1>
      <p><strong>{$session?.nickname}</strong> 지휘관, 신호를 선택하거나 새 작전을 편성하십시오.</p>
    </div>
    <div class="lobby-heading__actions">
      <Button variant="outline" onclick={() => (showJoin = true)}
        ><DoorOpen size={17} /> 코드 참가</Button
      >
      <Button variant="primary" onclick={() => (showCreate = true)}
        ><Plus size={17} /> 작전실 생성</Button
      >
    </div>
  </header>

  {#if error}<div class="lobby-alert" role="alert">
      <span><Radio size={17} /></span>
      <div>
        <strong>CHANNEL ERROR</strong>
        <p>{error}</p>
      </div>
    </div>{/if}

  <section class="command-dashboard" aria-label="작전 현황">
    <Surface tone="elevated" padding="lg" class="quick-match">
      <div class="quick-match__radar" class:searching={matching}>
        <div class="quick-match__sweep"></div>
        <Radio size={32} /><span></span>
      </div>
      <div class="quick-match__copy">
        <Badge tone={matching ? 'warning' : 'cyan'} pulse={matching}
          >{matching ? 'SEARCHING SIGNALS' : 'QUICK DEPLOYMENT'}</Badge
        >
        <h2>
          {matching ? '상대 지휘관 탐색 중' : matchPool === 'RANKED' ? '랭크 교전' : '빠른 교전'}
        </h2>
        <p>
          {matching
            ? `${elapsed}초 경과 · ${matchmakingTicket?.searchWindow.phase ?? 'EXACT'} 범위에서 대기 중입니다.`
            : matchPool === 'RANKED'
              ? '레이팅·리전 RTT 기반 1:1 매칭입니다.'
              : '같은 신호를 기다리는 지휘관과 즉시 1:1 비공개 작전을 편성합니다.'}
        </p>
        <div class="matchmaking-profile" aria-label="매칭 조건">
          <div class="matchmaking-pool" role="group" aria-label="매칭 유형">
            <button
              type="button"
              class:active={matchPool === 'CASUAL'}
              aria-pressed={matchPool === 'CASUAL'}
              disabled={matching}
              onclick={() => (matchPool = 'CASUAL')}>일반</button
            >
            <button
              type="button"
              class:active={matchPool === 'RANKED'}
              aria-pressed={matchPool === 'RANKED'}
              disabled={matching}
              onclick={() => (matchPool = 'RANKED')}>랭크</button
            >
          </div>
          {#if matchPool === 'RANKED'}
            <label>
              <span>리전</span>
              <select bind:value={rankedRegion} disabled={matching} aria-label="랭크 매칭 리전">
                <option value="KOREA">한국</option>
                <option value="JAPAN">일본</option>
                <option value="SOUTHEAST_ASIA">동남아시아</option>
                <option value="NORTH_AMERICA_WEST">북미 서부</option>
                <option value="NORTH_AMERICA_EAST">북미 동부</option>
                <option value="EUROPE">유럽</option>
              </select>
            </label>
            <button class="latency-probe" type="button" disabled={matching} onclick={measureLatency}
              >{measuredLatency ? `${measuredLatency}ms 재측정` : 'RTT 측정'}</button
            >
          {/if}
        </div>
        <div class="matching-telemetry">
          <span><i></i> ENCRYPTED LINK</span><span>SOLO PARTY</span><span
            >{matchmakingTicket?.rating
              ? `RATING ${matchmakingTicket.rating}`
              : 'RANDOM INITIATIVE'}</span
          >
        </div>
      </div>
      <Button variant={matching ? 'danger' : 'primary'} size="lg" onclick={toggleMatchmaking}>
        {#if matching}<X size={17} /> 매칭 취소{:else}<Search size={17} /> 상대 찾기{/if}
      </Button>
    </Surface>

    <div class="dashboard-side">
      <Surface tone="elevated" padding="md" class="practice-card">
        <div class="practice-heading">
          <span><Bot size={19} /></span>
          <div>
            <small>AI TACTICAL RANGE</small><strong>AI 연습 교전</strong>
            <p>서버 권위 AI와 난이도별 실전 훈련</p>
          </div>
        </div>
        <div class="practice-options" aria-label="AI 난이도 선택">
          <button disabled={practicing} onclick={() => startPractice('RECRUIT')}
            ><span>신병</span><small>RECRUIT</small></button
          >
          <button disabled={practicing} onclick={() => startPractice('OFFICER')}
            ><span>장교</span><small>OFFICER</small></button
          >
          <button disabled={practicing} onclick={() => startPractice('ADMIRAL')}
            ><span>제독</span><small>ADMIRAL</small></button
          >
        </div>
      </Surface>
      <Surface tone="interactive" padding="md">
        <a class="dashboard-action" href={resolve('/tutorial')}>
          <span><ShieldCheck size={19} /></span>
          <div>
            <small>COMMAND ACADEMY</small><strong>작전 튜토리얼</strong>
            <p>배치·공격·턴·재접속 훈련</p>
          </div>
          <ArrowRight size={16} />
        </a>
      </Surface>
      <Surface tone="interactive" padding="md">
        <a class="dashboard-action" href={resolve('/stats')}>
          <span><History size={19} /></span>
          <div>
            <small>OPERATION ARCHIVE</small><strong>전투 기록</strong>
            <p>완료한 교전과 명중 통계</p>
          </div>
          <ArrowRight size={16} />
        </a>
      </Surface>
      <Surface tone="quiet" padding="md">
        <div class="network-card">
          <ShieldCheck size={19} />
          <div>
            <small>TACTICAL NETWORK</small><strong
              >{$socketStatus === 'online' ? '실시간 동기화 중' : '채널 준비 중'}</strong
            >
          </div>
          <Badge tone={$socketStatus === 'online' ? 'success' : 'warning'}
            >{$socketStatus.toUpperCase()}</Badge
          >
        </div>
      </Surface>
    </div>
  </section>

  <section class="room-section" aria-labelledby="public-room-title">
    <div class="section-heading">
      <div>
        <p class="eyebrow">OPEN CHANNELS</p>
        <h2 id="public-room-title">공개 작전실</h2>
        <p>
          {rooms.length
            ? `${rooms.length}개 채널이 신규 지휘관을 기다리고 있습니다.`
            : 'SCANNING TACTICAL CHANNELS / 활성 작전 신호 대기 중'}
        </p>
      </div>
      <Button variant="ghost" size="sm" onclick={loadRooms}
        ><RefreshCw size={15} /> 채널 스캔</Button
      >
    </div>

    <div class="room-grid">
      {#if loading}
        {#each Array.from({ length: 3 }) as _, index (index)}
          <Surface tone="quiet" padding="md"
            ><div class="room-skeleton">
              <Skeleton width="46%" height="12px" /><Skeleton width="72%" height="22px" /><Skeleton
                height="74px"
              /><Skeleton width="100%" height="40px" />
            </div></Surface
          >
        {/each}
      {:else if rooms.length === 0}
        <Surface tone="quiet" padding="lg" class="rooms-empty">
          <div class="empty-radar"><Radio size={27} /></div>
          <h3>NO ACTIVE OPERATIONS DETECTED</h3>
          <p>전술 채널 스캔이 완료되었습니다. 첫 작전을 편성하거나 빠른 교전을 시작하십시오.</p>
          <Button variant="outline" onclick={() => (showCreate = true)}
            ><Plus size={15} /> 첫 채널 편성</Button
          >
        </Surface>
      {:else}
        {#each rooms as room (room.id)}
          <Surface tone="interactive" padding="md" class="room-card">
            <article>
              <div class="room-card__top">
                <Badge tone="success" pulse>OPEN</Badge><span>{age(room.createdAt)}</span>
              </div>
              <div class="room-card__title">
                <small>OPERATION / {room.code}</small>
                <h3>{room.name}</h3>
              </div>
              <div class="room-card__crew">
                <Avatar name="HOST" status="online" />
                <div>
                  <small>COMMAND CREW</small><strong
                    >{room.playerCount} / {room.capacity} 지휘관</strong
                  >
                </div>
                <div class="crew-slots">
                  <i class="filled"></i><i class:filled={room.playerCount > 1}></i>
                </div>
              </div>
              <div class="room-card__meta">
                <span><Radio size={13} /> PUBLIC CHANNEL</span><span
                  ><KeyRound size={13} /> {room.code}</span
                ><span>{room.rules.mode}</span>
              </div>
              <Button
                variant="secondary"
                full
                onclick={() => joinRoom(room.code)}
                disabled={submitting}>채널 참가 <ArrowRight size={15} /></Button
              >
            </article>
          </Surface>
        {/each}
      {/if}
    </div>
  </section>
</div>

<Modal
  open={showCreate}
  title="새 작전실 편성"
  eyebrow="NEW OPERATION"
  description="작전 이름과 보안 범위를 지정하십시오. 편성 후 초대 코드가 즉시 발급됩니다."
  onclose={() => (showCreate = false)}
>
  <form
    class="operation-form"
    onsubmit={(event) => {
      event.preventDefault();
      createRoom();
    }}
  >
    <Field
      id="room-name"
      label="작전실 이름"
      bind:value={roomName}
      minlength={2}
      maxlength={32}
      required
    />
    <fieldset>
      <legend>공개 범위</legend>
      <div class="visibility-grid">
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PUBLIC" /><span
            ><Radio size={18} /><strong>공개</strong><small>OPEN CHANNEL</small><em
              >로비에서 누구나 참가</em
            ></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PRIVATE" /><span
            ><LockKeyhole size={18} /><strong>비공개</strong><small>SECURE CHANNEL</small><em
              >초대 링크와 코드로만 참가</em
            ></span
          ></label
        >
      </div>
    </fieldset>
    <fieldset>
      <legend>교전 규칙</legend>
      <div class="mode-grid">
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="CLASSIC" /><span
            ><strong>클래식</strong><small>CLASSIC</small><em>턴마다 한 발 사격</em></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="RAPID" /><span
            ><strong>신속전</strong><small>RAPID</small><em>고정 30초 턴</em></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="SALVO" /><span
            ><strong>일제사격</strong><small>SALVO</small><em>생존 함선당 한 발</em></span
          ></label
        >
      </div>
      <label class="duration-choice" for="turn-duration">
        <span><strong>턴 제한 시간</strong><small>TURN LIMIT</small></span>
        <select id="turn-duration" bind:value={turnDurationSeconds} disabled={gameMode === 'RAPID'}>
          <option value={0}>제한 없음</option>
          <option value={30}>30초</option>
          <option value={45}>45초</option>
          <option value={60}>60초</option>
          <option value={90}>90초</option>
          <option value={120}>120초</option>
        </select>
      </label>
    </fieldset>
    <Button variant="primary" size="lg" type="submit" loading={submitting} full
      >작전실 편성 <ArrowRight size={17} /></Button
    >
  </form>
</Modal>

<Modal
  open={showJoin}
  title="보안 코드로 참가"
  eyebrow="SECURE CHANNEL"
  description="초대받은 6자리 작전 코드를 입력하십시오."
  onclose={() => (showJoin = false)}
>
  <form
    class="operation-form"
    onsubmit={(event) => {
      event.preventDefault();
      joinRoom();
    }}
  >
    <Field
      id="room-code"
      label="작전 코드"
      bind:value={roomCode}
      minlength={6}
      maxlength={6}
      placeholder="ABC123"
      autocomplete="off"
      code
      required
    />
    <Button
      variant="primary"
      size="lg"
      type="submit"
      loading={submitting}
      disabled={roomCode.length !== 6}
      full><KeyRound size={17} /> 채널 접속</Button
    >
  </form>
</Modal>

<style>
  .lobby {
    padding: 64px 0 112px;
  }
  .lobby-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 40px;
    margin-bottom: 40px;
  }
  .heading-signal {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 30px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.12em;
  }
  .lobby-heading h1 {
    margin-bottom: 10px;
  }
  .lobby-heading > div:first-child > p:last-child {
    margin: 0;
    color: var(--ink-300);
    font-size: 13px;
  }
  .lobby-heading > div:first-child > p:last-child strong {
    color: var(--ink-100);
  }
  .lobby-heading__actions {
    display: flex;
    gap: 10px;
  }
  .lobby-alert {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px;
    margin-bottom: 20px;
    padding: 15px;
    border: 1px solid rgba(255, 114, 128, 0.28);
    border-radius: 14px;
    color: var(--red-400);
    background: rgba(91, 18, 32, 0.22);
  }
  .lobby-alert > span {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border-radius: 9px;
    background: rgba(240, 72, 94, 0.1);
  }
  .lobby-alert strong {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
  }
  .lobby-alert p {
    margin: 3px 0 0;
    color: #e1aab1;
    font-size: 11px;
  }
  .command-dashboard {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 330px;
    gap: 16px;
  }
  :global(.quick-match) :global(.ui-surface__content) {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 24px;
  }
  .quick-match__radar {
    position: relative;
    display: grid;
    width: 92px;
    height: 92px;
    place-items: center;
    overflow: hidden;
    border: 1px solid rgba(40, 223, 232, 0.3);
    border-radius: 50%;
    color: var(--cyan-300);
    background: radial-gradient(circle, rgba(23, 132, 150, 0.21), transparent 68%);
  }
  .quick-match__radar::before,
  .quick-match__radar::after {
    position: absolute;
    inset: 50% 0 auto;
    height: 1px;
    content: '';
    background: rgba(40, 223, 232, 0.16);
  }
  .quick-match__radar::after {
    transform: rotate(90deg);
  }
  .quick-match__radar > span {
    position: absolute;
    top: 26%;
    right: 25%;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 10px var(--cyan-300);
  }
  .quick-match__sweep {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(40, 223, 232, 0.38), transparent 42deg);
    animation: radar 3.8s linear infinite;
  }
  .quick-match__radar.searching {
    border-color: rgba(255, 209, 107, 0.42);
    color: var(--amber-400);
  }
  .quick-match__copy h2 {
    margin: 10px 0 5px;
    font-family: var(--font-display);
    font-size: 27px;
    font-weight: 600;
  }
  .quick-match__copy > p {
    margin: 0;
    color: var(--ink-300);
    font-size: 12px;
    line-height: 1.7;
  }
  .matchmaking-profile {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 11px;
  }
  .matchmaking-pool {
    display: flex;
  }
  .matchmaking-profile button,
  .matchmaking-profile select {
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--ink-300);
    background: rgba(2, 14, 20, 0.7);
    font: 600 9px var(--font-display);
  }
  .matchmaking-pool button.active {
    color: var(--cyan-300);
  }
  .matchmaking-profile label {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-500);
    font: 600 9px var(--font-display);
  }
  .matchmaking-profile .latency-probe {
    color: var(--amber-400);
  }
  .matchmaking-profile :disabled {
    opacity: 0.6;
  }
  .matching-telemetry {
    display: flex;
    flex-wrap: wrap;
    gap: 13px;
    margin-top: 15px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .matching-telemetry span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .matching-telemetry i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--green-400);
    box-shadow: 0 0 6px var(--green-400);
  }
  .dashboard-side {
    display: grid;
    gap: 16px;
  }
  :global(.practice-card) :global(.ui-surface__content) {
    display: grid;
    gap: 12px;
  }
  .practice-heading {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 12px;
  }
  .practice-heading > span {
    display: grid;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid rgba(255, 209, 107, 0.24);
    border-radius: 11px;
    color: var(--amber-400);
    background: rgba(255, 209, 107, 0.06);
  }
  .practice-heading > div {
    display: grid;
    gap: 2px;
  }
  .practice-heading small,
  .practice-options small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .practice-heading strong {
    font-size: 12px;
  }
  .practice-heading p {
    margin: 0;
    color: var(--ink-400);
    font-size: 9px;
  }
  .practice-options {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .practice-options button {
    display: grid;
    gap: 2px;
    min-height: 46px;
    padding: 7px 4px;
    border: 1px solid var(--line);
    border-radius: 6px 2px 6px 2px;
    color: var(--ink-200);
    background: rgba(6, 25, 32, 0.76);
    cursor: pointer;
  }
  .practice-options button:hover:not(:disabled),
  .practice-options button:focus-visible {
    border-color: var(--cyan-300);
    color: white;
    background: rgba(40, 223, 232, 0.08);
  }
  .practice-options button:focus-visible {
    outline: 2px solid var(--cyan-300);
    outline-offset: 2px;
  }
  .practice-options button:disabled {
    cursor: wait;
    opacity: 0.55;
  }
  .dashboard-action {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
  }
  .dashboard-action > span {
    display: grid;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 11px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .dashboard-action > div {
    display: grid;
    gap: 2px;
  }
  .dashboard-action small,
  .network-card small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.14em;
  }
  .dashboard-action strong,
  .network-card strong {
    font-size: 12px;
  }
  .dashboard-action p {
    margin: 0;
    color: var(--ink-400);
    font-size: 9px;
  }
  .network-card {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 11px;
    color: var(--green-400);
  }
  .network-card > div {
    display: grid;
    gap: 2px;
  }
  .network-card strong {
    color: var(--ink-200);
  }
  .room-section {
    margin-top: 56px;
  }
  .section-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 20px;
  }
  .section-heading h2 {
    margin: 0 0 5px;
    font-family: var(--font-display);
    font-size: 30px;
    font-weight: 600;
  }
  .section-heading p:last-child {
    margin: 0;
    color: var(--ink-400);
    font-size: 11px;
  }
  .room-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 16px;
  }
  :global(.room-card) article {
    display: grid;
    gap: 18px;
  }
  .room-card__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .room-card__top > span {
    color: var(--ink-500);
    font-size: 9px;
  }
  .room-card__title small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.13em;
  }
  .room-card__title h3 {
    margin: 5px 0 0;
    overflow: hidden;
    font-size: 17px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .room-card__crew {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 11px;
    padding: 13px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(3, 15, 23, 0.5);
  }
  .room-card__crew > div:nth-child(2) {
    display: grid;
    gap: 2px;
  }
  .room-card__crew small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .room-card__crew strong {
    font-size: 11px;
  }
  .crew-slots {
    display: flex;
    gap: 4px;
  }
  .crew-slots i {
    width: 5px;
    height: 18px;
    border-radius: 3px;
    background: #1d3743;
  }
  .crew-slots i.filled {
    background: var(--green-400);
    box-shadow: 0 0 7px rgba(79, 226, 173, 0.35);
  }
  .room-card__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.07em;
  }
  .room-card__meta span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .room-skeleton {
    display: grid;
    gap: 16px;
  }
  :global(.rooms-empty) {
    grid-column: 1 / -1;
    text-align: center;
  }
  :global(.rooms-empty) :global(.ui-surface__content) {
    display: grid;
    min-height: 280px;
    place-items: center;
    align-content: center;
    gap: 12px;
  }
  :global(.rooms-empty) h3,
  :global(.rooms-empty) p {
    margin: 0;
  }
  :global(.rooms-empty) p {
    color: var(--ink-400);
    font-size: 11px;
  }
  .empty-radar {
    display: grid;
    width: 62px;
    height: 62px;
    place-items: center;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    color: var(--cyan-300);
    background: radial-gradient(circle, rgba(40, 223, 232, 0.11), transparent 68%);
  }
  .operation-form {
    display: grid;
    gap: 22px;
  }
  .operation-form fieldset {
    padding: 0;
    border: 0;
  }
  .operation-form legend {
    margin-bottom: 9px;
    color: var(--ink-200);
    font-size: 12px;
    font-weight: 700;
  }
  .visibility-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .mode-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }
  .mode-grid .choice span {
    grid-template-columns: 1fr;
    min-height: 86px;
  }
  .duration-choice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 10px;
    padding: 11px 12px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: rgba(4, 20, 28, 0.72);
  }
  .duration-choice > span {
    display: grid;
    gap: 2px;
  }
  .duration-choice strong {
    font-size: 11px;
  }
  .duration-choice small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .duration-choice select {
    min-width: 120px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 5px;
    color: var(--ink-100);
    background: var(--navy-900);
  }
  .duration-choice select:disabled {
    opacity: 0.55;
  }
  .choice {
    position: relative;
  }
  .choice input {
    position: absolute;
    opacity: 0;
  }
  .choice span {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 9px;
    min-height: 112px;
    align-content: center;
    padding: 15px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(3, 15, 23, 0.55);
    cursor: pointer;
    transition: 180ms var(--ease-out);
  }
  .choice :global(svg) {
    grid-row: 1 / 4;
    color: var(--cyan-300);
  }
  .choice strong {
    font-size: 12px;
  }
  .choice small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.1em;
  }
  .choice em {
    color: var(--ink-400);
    font-size: 9px;
    font-style: normal;
    line-height: 1.5;
  }
  .choice input:checked + span {
    border-color: var(--cyan-300);
    background: rgba(17, 95, 106, 0.15);
    box-shadow:
      0 0 0 3px rgba(40, 223, 232, 0.07),
      var(--glow-cyan);
  }
  .choice input:focus-visible + span {
    outline: 2px solid var(--cyan-300);
    outline-offset: 3px;
  }
  @media (max-width: 1050px) {
    .command-dashboard {
      grid-template-columns: 1fr;
    }
    .dashboard-side {
      grid-template-columns: 1fr 1fr;
    }
    .room-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }
  @media (max-width: 720px) {
    .lobby {
      padding: 44px 0 88px;
    }
    .lobby-heading {
      display: block;
    }
    .heading-signal > span {
      display: none;
    }
    .lobby-heading__actions {
      margin-top: 24px;
    }
    .lobby-heading__actions :global(.ui-button) {
      flex: 1;
      padding-inline: 10px;
    }
    :global(.quick-match) :global(.ui-surface__content) {
      grid-template-columns: auto 1fr;
      gap: 16px;
    }
    :global(.quick-match) :global(.ui-button) {
      grid-column: 1 / -1;
      width: 100%;
    }
    .quick-match__radar {
      width: 68px;
      height: 68px;
    }
    .quick-match__copy h2 {
      font-size: 22px;
    }
    .dashboard-side,
    .room-grid {
      grid-template-columns: 1fr;
    }
    .section-heading {
      align-items: start;
    }
    .section-heading :global(.ui-button) {
      min-width: 40px;
      padding-inline: 10px;
    }
    .section-heading :global(.ui-button) :global(span) {
      font-size: 0;
    }
    .visibility-grid {
      grid-template-columns: 1fr;
    }
    .mode-grid {
      grid-template-columns: 1fr;
    }
  }
  .lobby {
    position: relative;
    max-width: 1480px;
    overflow-x: clip;
    padding-top: 42px;
  }
  .lobby-heading {
    align-items: end;
    padding-bottom: 20px;
    border-bottom: 1px solid var(--line);
  }
  .lobby-heading h1 {
    font-family: var(--font-display);
    font-size: clamp(34px, 4.2vw, 54px);
    letter-spacing: 0.03em;
  }
  .heading-signal {
    border-radius: 50%;
    border-color: rgba(83, 233, 232, 0.34);
  }
  .command-dashboard {
    gap: 12px;
  }
  :global(.quick-match) {
    border-radius: 9px 3px 9px 3px;
    border-color: rgba(83, 233, 232, 0.3);
    background: linear-gradient(145deg, rgba(8, 33, 41, 0.9), rgba(2, 13, 20, 0.96));
  }
  .quick-match__copy h2 {
    font-family: var(--font-display);
    font-size: 30px;
    letter-spacing: 0.04em;
  }
  .quick-match__copy p {
    color: var(--ink-400);
  }
  .quick-match__radar {
    border-radius: 8px 3px 8px 3px;
  }
  .dashboard-side {
    gap: 10px;
  }
  .dashboard-action,
  .network-card {
    border-radius: 5px 2px 5px 2px;
    border-color: var(--line);
    background: rgba(3, 16, 23, 0.72);
  }
  .room-section {
    margin-top: 42px;
  }
  .section-heading {
    padding-bottom: 13px;
    border-bottom: 1px solid var(--line);
  }
  .section-heading h2 {
    font-family: var(--font-display);
    font-size: 24px;
    letter-spacing: 0.04em;
  }
  .room-grid {
    gap: 10px;
  }
  :global(.room-card) {
    border-radius: 6px 2px 6px 2px;
    border-color: rgba(113, 178, 190, 0.17);
    background: linear-gradient(150deg, rgba(7, 27, 36, 0.8), rgba(2, 13, 20, 0.88));
  }
  :global(.room-card:hover) {
    border-color: var(--line-active);
    transform: translateY(-2px);
  }
  .room-card__top {
    border-bottom-color: var(--line);
  }
  .room-card__meta {
    color: var(--ink-500);
  }
  @media (max-width: 720px) {
    .lobby {
      padding-top: 30px;
    }
  }
  .lobby::before {
    position: absolute;
    z-index: -1;
    top: 210px;
    right: -12%;
    left: -12%;
    height: 440px;
    content: '';
    opacity: 0.2;
    pointer-events: none;
    background:
      linear-gradient(rgba(83, 233, 232, 0.07) 1px, transparent 1px),
      linear-gradient(90deg, rgba(83, 233, 232, 0.07) 1px, transparent 1px),
      radial-gradient(ellipse at 50% 35%, rgba(33, 138, 156, 0.14), transparent 64%);
    background-size:
      72px 72px,
      72px 72px,
      auto;
    mask-image: linear-gradient(180deg, transparent, black 18%, transparent 92%);
  }
  .command-dashboard {
    position: relative;
  }
  :global(.quick-match) {
    min-height: 182px;
    border-radius: 6px 2px 6px 2px;
    border-top: 2px solid var(--tactical);
    box-shadow:
      0 24px 60px rgba(0, 0, 0, 0.28),
      0 0 36px rgba(83, 233, 232, 0.06);
  }
  .quick-match__copy h2 {
    font-size: clamp(26px, 3vw, 34px);
  }
  .room-section {
    position: relative;
  }
  .room-section::before {
    position: absolute;
    top: 62px;
    right: 0;
    left: 0;
    height: 1px;
    content: '';
    background: linear-gradient(
      90deg,
      var(--tactical),
      transparent 35%,
      transparent 65%,
      var(--warning)
    );
    opacity: 0.22;
  }
  .room-grid {
    grid-template-columns: 1fr;
  }
  :global(.room-card) {
    border-radius: 4px 1px 4px 1px;
    background: rgba(3, 15, 22, 0.78);
  }
  :global(.room-card) article {
    display: grid;
    grid-template-columns:
      142px minmax(180px, 1.15fr) minmax(170px, 0.85fr) minmax(180px, 0.9fr)
      168px;
    align-items: center;
    gap: 16px;
  }
  .room-card__top,
  .room-card__title,
  .room-card__crew,
  .room-card__meta {
    min-width: 0;
  }
  .room-card__top {
    display: grid;
    gap: 7px;
    border: 0;
  }
  .room-card__title h3 {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .room-card__crew {
    padding-left: 14px;
    border-left: 1px solid var(--line);
  }
  .room-card__meta {
    display: grid;
    gap: 5px;
    font-size: 9px;
  }
  :global(.room-card) article > :global(.ui-button) {
    width: 100%;
  }
  :global(.rooms-empty) {
    min-height: 230px;
    display: grid;
    place-items: center;
    align-content: center;
    text-align: center;
  }
  :global(.rooms-empty h3) {
    margin: 16px 0 7px;
    color: var(--ink-200);
    font: 700 17px var(--font-display);
    letter-spacing: 0.08em;
  }
  :global(.rooms-empty p) {
    max-width: 480px;
    color: var(--ink-400);
    font-size: 11px;
  }
  @media (max-width: 1050px) {
    .room-grid {
      grid-template-columns: 1fr;
    }
    :global(.room-card) article {
      grid-template-columns: 128px minmax(180px, 1fr) minmax(150px, 0.8fr) 150px;
    }
    .room-card__meta {
      display: none;
    }
  }
  @media (max-width: 720px) {
    :global(.room-card) article {
      grid-template-columns: 1fr auto;
      gap: 12px;
    }
    .room-card__top,
    .room-card__title {
      grid-column: 1;
    }
    .room-card__top {
      grid-row: 1;
    }
    .room-card__title {
      grid-row: 2;
    }
    .room-card__crew {
      grid-column: 1 / -1;
      grid-row: 3;
      padding: 12px 0 0;
      border-top: 1px solid var(--line);
      border-left: 0;
    }
    :global(.room-card) article > :global(.ui-button) {
      grid-column: 1 / -1;
      grid-row: 4;
    }
  }
</style>
