<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onDestroy, onMount } from 'svelte';
  import { ArrowLeft, Check, Rocket, ShieldCheck, Wifi, WifiOff } from '@lucide/svelte';
  import BattleView from '$lib/components/BattleView.svelte';
  import ChatDrawer from '$lib/components/ChatDrawer.svelte';
  import DisconnectedOverlay from '$lib/components/DisconnectedOverlay.svelte';
  import FleetPlacement from '$lib/components/FleetPlacement.svelte';
  import ResultView from '$lib/components/ResultView.svelte';
  import WaitingView from '$lib/components/WaitingView.svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { sounds } from '$lib/sound';
  import { Button, Modal } from '$lib/ui';
  import {
    gameError,
    gameSnapshot,
    lastAttack,
    resetRoomRealtimeState,
    session,
    socketStatus
  } from '$lib/stores';
  import type { Coordinate, ShipPlacement } from '$lib/types';

  const routeCode = (page.params.code ?? '').toUpperCase();
  let loading = $state(true);
  let loadError = $state('');
  let placementSubmitting = $state(false);
  let attackPending = $state(false);
  let surrenderPending = $state(false);
  let readyPending = $state(false);
  let startPending = $state(false);
  let showStart = $state(false);
  let lastSoundRequest = $state<string | null>(null);
  let resultSoundPlayed = $state(false);
  let launchSequence = $state(false);
  let launchStage = $state(0);
  let previousRoomStatus = '';
  let launchTimer: ReturnType<typeof setInterval> | null = null;
  const launchStages = [
    'OPERATION AUTHORIZED',
    'ENCRYPTING TACTICAL CHANNEL',
    'LOADING BATTLESPACE',
    'DEPLOY FLEET'
  ];

  let snapshot = $derived($gameSnapshot);
  let selfPlayer = $derived(
    snapshot?.players.find((player) => player.id === snapshot?.selfPlayerId)
  );
  let hasDisconnectedPlayer = $derived(
    snapshot?.players.some((player) => player.connectionState !== 'ONLINE') ?? false
  );
  let inviteUrl = $derived(
    typeof location === 'undefined' ? `/join/${routeCode}` : `${location.origin}/join/${routeCode}`
  );

  function startLaunchSequence() {
    if (launchSequence) return;
    launchSequence = true;
    launchStage = 0;
    if (launchTimer) clearInterval(launchTimer);
    launchTimer = setInterval(() => {
      if (launchStage >= launchStages.length - 1) {
        if (launchTimer) clearInterval(launchTimer);
        launchTimer = null;
        setTimeout(() => (launchSequence = false), 480);
        return;
      }
      launchStage += 1;
    }, 460);
  }

  onMount(() => {
    let active = true;
    resetRoomRealtimeState();
    (async () => {
      try {
        const current = await api.currentSession();
        if (!active) return;
        session.set(current);
        const recovered = await api.recover();
        if (!recovered || recovered.room.code !== routeCode) {
          loadError = '이 세션에서 복구할 수 있는 작전실이 아닙니다.';
          return;
        }
        gameSnapshot.set(recovered);
        realtime.connect();
        realtime.sync(recovered.room.id);
      } catch (caught) {
        if (caught instanceof ApiError && caught.code === 'UNAUTHORIZED') {
          await goto(resolve('/join/[code]', { code: routeCode }));
          return;
        }
        loadError =
          caught instanceof ApiError ? caught.message : '전장 상태를 불러오지 못했습니다.';
      } finally {
        loading = false;
      }
    })();
    return () => {
      active = false;
      realtime.disconnect();
      resetRoomRealtimeState();
      if (launchTimer) clearInterval(launchTimer);
    };
  });

  onDestroy(() => {
    if (launchTimer) clearInterval(launchTimer);
  });

  $effect(() => {
    const status = snapshot?.room.status ?? '';
    if (status === 'PLACEMENT' && previousRoomStatus !== 'PLACEMENT') startLaunchSequence();
    previousRoomStatus = status;
  });

  $effect(() => {
    const attack = $lastAttack;
    if (!attack || attack.requestId === lastSoundRequest) return;
    lastSoundRequest = attack.requestId;
    attackPending = false;
    if (attack.outcome === 'MISS') sounds.miss();
    else if (attack.outcome === 'SUNK') sounds.sunk();
    else sounds.hit();
  });

  $effect(() => {
    if (snapshot?.room.status === 'FINISHED' && snapshot.result && !resultSoundPlayed) {
      resultSoundPlayed = true;
      if (snapshot.result.winnerId === snapshot.selfPlayerId) sounds.victory();
    }
    if (selfPlayer?.placementConfirmed) placementSubmitting = false;
    if ($gameError) {
      readyPending = false;
      startPending = false;
      showStart = false;
    }
    if (snapshot?.roomState === 'PLACEMENT') {
      startPending = false;
      showStart = false;
    }
    if (snapshot?.room.status === 'FINISHED' || $gameError) surrenderPending = false;
  });

  $effect(() => {
    if (!snapshot || !selfPlayer) return;
    if (snapshot.roomVersion >= 0 && selfPlayer.readyState) readyPending = false;
  });

  function confirmPlacement(placements: ShipPlacement[]) {
    if (!snapshot || placementSubmitting || $socketStatus !== 'online') return;
    placementSubmitting = true;
    gameError.set(null);
    realtime.send({
      type: 'ships:place',
      payload: {
        roomId: snapshot.room.id,
        playerId: snapshot.selfPlayerId,
        placements
      }
    });
    realtime.send({
      type: 'ships:confirm',
      payload: {
        roomId: snapshot.room.id,
        playerId: snapshot.selfPlayerId,
        placements
      }
    });
  }

  function setLobbyReady(ready: boolean) {
    if (!snapshot || readyPending || startPending || $socketStatus !== 'online') return;
    readyPending = true;
    gameError.set(null);
    const sent = realtime.send({
      type: ready ? 'player:ready' : 'player:unready',
      payload: {
        requestId: crypto.randomUUID(),
        roomId: snapshot.room.id,
        playerId: snapshot.selfPlayerId
      }
    });
    if (!sent) {
      readyPending = false;
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: '실시간 연결이 복구된 뒤 준비 상태를 다시 변경해 주세요.',
        retryable: true
      });
    }
  }

  function startGame() {
    if (
      !snapshot ||
      startPending ||
      !snapshot.canStartGame ||
      snapshot.selfPlayerId !== snapshot.hostPlayerId ||
      $socketStatus !== 'online'
    )
      return;
    startPending = true;
    showStart = false;
    gameError.set(null);
    const sent = realtime.send({
      type: 'game:start',
      payload: {
        requestId: crypto.randomUUID(),
        roomId: snapshot.roomId,
        playerId: snapshot.selfPlayerId,
        roomVersion: snapshot.roomVersion
      }
    });
    if (!sent) {
      startPending = false;
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: '실시간 연결이 복구된 뒤 작전 시작을 다시 승인해 주세요.',
        retryable: true
      });
    }
  }

  function fire(coordinate: Coordinate) {
    if (!snapshot || attackPending || snapshot.turnNumber === null) return;
    const requestId = crypto.randomUUID();
    attackPending = true;
    const sent = realtime.send({
      type: 'attack:fire',
      payload: {
        requestId,
        roomId: snapshot.room.id,
        playerId: snapshot.selfPlayerId,
        coordinate,
        expectedVersion: snapshot.version,
        turnNumber: snapshot.turnNumber
      }
    });
    if (!sent) {
      attackPending = false;
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: '실시간 연결이 복구된 뒤 다시 공격해 주세요.',
        retryable: true
      });
    }
  }

  function surrender() {
    if (!snapshot || surrenderPending || $socketStatus !== 'online') return;
    surrenderPending = true;
    gameError.set(null);
    const sent = realtime.send({
      type: 'game:surrender',
      payload: { roomId: snapshot.room.id, playerId: snapshot.selfPlayerId }
    });
    if (!sent) {
      surrenderPending = false;
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: '실시간 연결이 복구된 뒤 기권을 다시 요청해 주세요.',
        retryable: true
      });
    }
  }

  async function leaveRoom() {
    if (!snapshot) return;
    try {
      await api.leaveRoom(snapshot.room.id);
    } finally {
      gameSnapshot.set(null);
      await goto(resolve('/lobby'));
    }
  }

  function rematch() {
    if (!snapshot) return;
    realtime.send({ type: 'game:rematch', payload: { roomId: snapshot.room.id } });
  }
</script>

<svelte:head><title>{snapshot?.room.name ?? '전장 연결'} · Mk.01</title></svelte:head>

<div class="room-page shell">
  {#if loading}
    <div class="loading-view">
      <div class="spinner"></div>
      <p>암호화된 작전 채널 연결 중…</p>
    </div>
  {:else if loadError}
    <section class="load-error panel">
      <WifiOff size={34} />
      <h1>작전 채널에 연결할 수 없습니다</h1>
      <p>{loadError}</p>
      <a class="button" href={resolve('/lobby')}><ArrowLeft size={16} /> 로비로 복귀</a>
    </section>
  {:else if snapshot}
    <div class="room-meta">
      <div>
        <span class="status-pill"><span class="status-dot"></span>{snapshot.room.status}</span
        ><strong>{snapshot.room.name}</strong><small
          >CODE {snapshot.room.code} · STATE V{snapshot.version}</small
        >
      </div>
      <div class:offline={$socketStatus !== 'online'} class="connection-indicator">
        {#if $socketStatus === 'online'}<Wifi size={14} /> 실시간 연결{:else}<WifiOff size={14} />
          {$socketStatus === 'reconnecting' ? '재연결 중' : '오프라인'}{/if}
      </div>
    </div>

    {#if snapshot.room.status === 'WAITING_FOR_OPPONENT' || snapshot.room.status === 'WAITING_FOR_READY' || snapshot.room.status === 'READY_TO_START'}
      <WaitingView
        {snapshot}
        {inviteUrl}
        online={$socketStatus === 'online'}
        {readyPending}
        {startPending}
        onready={() => setLobbyReady(true)}
        onunready={() => setLobbyReady(false)}
        onstart={() => (showStart = true)}
        onleave={leaveRoom}
      />
    {:else if snapshot.room.status === 'PLACEMENT' && snapshot.roomState === 'PLACEMENT' && snapshot.gameId && snapshot.placementStartedAt}
      {#if selfPlayer?.placementConfirmed}
        <section class="confirmed-wait panel">
          <div class="confirmed-icon"><Check size={29} /></div>
          <p class="eyebrow">DEPLOYMENT LOCKED</p>
          <h1>함대 배치 확정 완료</h1>
          <p>
            상대 지휘관의 배치 확정을 기다리고 있습니다. 양쪽 함대가 배치되면 선공을 무작위로
            결정합니다.
          </p>
          <div class="player-ready-list">
            {#each snapshot.players as player (player.id)}<div>
                <span class:ready={player.placementConfirmed}><ShieldCheck size={17} /></span
                ><strong>{player.nickname}</strong><em
                  >{player.placementConfirmed ? '배치 확정' : '배치 중'}</em
                >
              </div>{/each}
          </div>
        </section>
      {:else}
        <FleetPlacement
          initialPlacement={snapshot.placement}
          confirmed={selfPlayer?.placementConfirmed}
          submitting={placementSubmitting}
          onconfirm={confirmPlacement}
        />
      {/if}
    {:else if snapshot.room.status === 'PLAYING'}
      <BattleView
        {snapshot}
        pending={attackPending}
        {surrenderPending}
        onfire={fire}
        onsurrender={surrender}
      />
    {:else if snapshot.room.status === 'FINISHED'}
      <ResultView {snapshot} onrematch={rematch} onlobby={leaveRoom} />
    {:else if snapshot.room.status === 'CANCELLED'}
      <section class="load-error panel">
        <WifiOff size={34} />
        <h1>작전이 취소되었습니다</h1>
        <p>상대 지휘관이 이탈했거나 작전실이 종료되었습니다.</p>
        <button class="button" onclick={leaveRoom}><ArrowLeft size={16} /> 로비로 복귀</button>
      </section>
    {:else}
      <section class="load-error panel" role="alert">
        <WifiOff size={34} />
        <h1>서버 버전을 확인해 주세요</h1>
        <p>
          대기실 상태 정보가 현재 화면과 호환되지 않습니다. 기존 개발 서버를 완전히 종료한 뒤 <code
            >npm run dev</code
          >로 다시 시작해 주세요.
        </p>
        <a class="button" href={resolve('/lobby')}><ArrowLeft size={16} /> 로비로 복귀</a>
      </section>
    {/if}
    <ChatDrawer
      roomId={snapshot.room.id}
      selfPlayerId={snapshot.selfPlayerId}
      online={$socketStatus === 'online'}
      readOnly={snapshot.room.status === 'CANCELLED'}
    />
    {#if hasDisconnectedPlayer && (snapshot.room.status === 'PLACEMENT' || snapshot.room.status === 'PLAYING')}
      <DisconnectedOverlay deadline={snapshot.reconnectDeadline} />
    {/if}
  {/if}
</div>

<Modal
  open={showStart}
  eyebrow="HOST AUTHORIZATION"
  title="작전을 시작하시겠습니까?"
  description="두 지휘관의 준비가 완료되었습니다. 작전을 시작하면 함선 배치 단계로 이동합니다."
  onclose={() => (showStart = false)}
>
  <div class="start-modal-actions">
    <Button variant="ghost" full onclick={() => (showStart = false)}>취소</Button>
    <Button
      variant="primary"
      full
      loading={startPending}
      disabled={!snapshot?.canStartGame || $socketStatus !== 'online'}
      onclick={startGame}><Rocket size={15} /> 작전 시작</Button
    >
  </div>
</Modal>

<style>
  .room-page {
    padding: 28px 0 80px;
  }
  .room-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 18px;
    padding: 12px 16px;
    border: 1px solid var(--line);
    border-radius: 13px;
    background: linear-gradient(90deg, rgba(8, 29, 41, 0.72), rgba(3, 15, 23, 0.46));
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.18);
  }
  .room-meta > div:first-child {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .room-meta strong {
    font-size: 12px;
  }
  .room-meta small {
    color: #577585;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.1em;
  }
  .connection-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--green-500);
    font-size: 10px;
  }
  .connection-indicator::before {
    width: 5px;
    height: 5px;
    content: '';
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 9px currentColor;
  }
  .connection-indicator.offline {
    color: var(--amber-500);
  }
  .loading-view {
    display: grid;
    min-height: 60vh;
    place-items: center;
    align-content: center;
    gap: 16px;
    color: #7895a5;
    font-size: 12px;
  }
  .loading-view::before {
    width: 86px;
    height: 86px;
    content: '';
    border: 1px solid rgba(40, 223, 232, 0.22);
    border-radius: 50%;
    background:
      conic-gradient(from 0deg, transparent, rgba(40, 223, 232, 0.38), transparent 25%),
      repeating-radial-gradient(circle, transparent 0 14px, rgba(40, 223, 232, 0.08) 15px 16px);
    animation: radar 2s linear infinite;
  }
  .load-error {
    width: min(520px, 100%);
    margin: 90px auto;
    padding: 38px;
    text-align: center;
  }
  .load-error :global(svg) {
    color: var(--red-500);
  }
  .load-error h1 {
    margin-top: 20px;
    font-size: 25px;
  }
  .load-error p {
    margin-bottom: 25px;
    color: var(--steel-300);
    font-size: 13px;
    line-height: 1.7;
  }
  .confirmed-wait {
    width: min(650px, 100%);
    margin: 70px auto;
    padding: 40px;
    text-align: center;
  }
  .confirmed-icon {
    display: grid;
    width: 70px;
    height: 70px;
    place-items: center;
    margin: 0 auto 22px;
    border: 1px solid rgba(61, 226, 161, 0.4);
    border-radius: 50%;
    color: var(--green-500);
    background: rgba(61, 226, 161, 0.08);
  }
  .confirmed-wait h1 {
    font-size: 28px;
  }
  .confirmed-wait > p:not(.eyebrow) {
    color: var(--steel-300);
    font-size: 12px;
    line-height: 1.7;
  }
  .player-ready-list {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 9px;
    margin-top: 25px;
  }
  .player-ready-list > div {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 5px 10px;
    padding: 13px;
    border: 1px solid var(--line);
    border-radius: 9px;
    text-align: left;
  }
  .player-ready-list span {
    grid-row: 1/3;
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border-radius: 50%;
    color: #6c8999;
    background: rgba(75, 112, 130, 0.12);
  }
  .player-ready-list span.ready {
    color: var(--green-500);
    background: rgba(61, 226, 161, 0.09);
  }
  .player-ready-list strong {
    font-size: 11px;
  }
  .player-ready-list em {
    color: #6c8999;
    font-size: 9px;
    font-style: normal;
  }
  .player-ready-list .ready + strong + em {
    color: var(--green-500);
  }
  .start-modal-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 22px;
  }
  @media (max-width: 720px) {
    .room-page {
      padding-top: 14px;
    }
    .room-meta {
      align-items: start;
    }
    .room-meta > div:first-child {
      display: grid;
      grid-template-columns: auto 1fr;
      gap: 5px 8px;
    }
    .room-meta small {
      grid-column: 1/-1;
    }
    .connection-indicator {
      margin-top: 5px;
    }
    .confirmed-wait {
      margin: 30px auto;
      padding: 30px 16px;
    }
    .player-ready-list {
      grid-template-columns: 1fr;
    }
    .start-modal-actions {
      grid-template-columns: 1fr;
    }
  }
  .room-page { max-width: 1500px; padding-top: 18px; }
  .room-meta { margin-bottom: 12px; padding: 10px 13px; border-radius: 5px 2px 5px 2px; border-color: var(--line); background: rgba(2, 13, 20, .72); box-shadow: none; }
  .room-meta > div:first-child { gap: 12px; }
  .room-meta strong { font-family: var(--font-display); font-size: 16px; letter-spacing: .04em; }
  .room-meta small { color: var(--ink-500); }
  .status-pill { border-radius: 3px; }
  .connection-indicator { font-family: var(--font-display); font-size: 9px; letter-spacing: .08em; }
  .load-error, .confirmed-wait { border-radius: 10px 3px 10px 3px; border-color: var(--line); background: linear-gradient(145deg, rgba(7, 28, 36, .9), rgba(2, 13, 20, .96)); }
  .confirmed-wait h1 { font-family: var(--font-display); font-size: 35px; letter-spacing: .03em; }
  @media (max-width: 720px) { .room-page { padding-top: 10px; } .room-meta { padding: 9px; } }
</style>
