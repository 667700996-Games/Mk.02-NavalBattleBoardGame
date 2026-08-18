<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { KeyRound, Plus, Radio } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { trackFunnelAbandoned, trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, session, socketStatus } from '$lib/stores';
  import { Badge, Button } from '$lib/ui';
  import LobbyCommandDashboard from '$lib/components/lobby/LobbyCommandDashboard.svelte';
  import LobbyRoomOperations from '$lib/components/lobby/LobbyRoomOperations.svelte';
  import './lobby.css';
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
</script>

<svelte:head><title>작전 로비 · Mk.01</title></svelte:head>

<div class="lobby-page">
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
        <p>
          <strong>{$session?.nickname}</strong> 지휘관, 신호를 선택하거나 새 작전을 편성하십시오.
        </p>
      </div>
      <div class="lobby-heading__actions">
        <Button variant="outline" onclick={() => (showJoin = true)}
          ><KeyRound size={17} /> 코드 참가</Button
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

    <LobbyCommandDashboard
      {matching}
      {elapsed}
      bind:matchPool
      bind:rankedRegion
      {measuredLatency}
      {matchmakingTicket}
      {practicing}
      socketStatus={$socketStatus}
      {toggleMatchmaking}
      {measureLatency}
      {startPractice}
    />

    <LobbyRoomOperations
      {rooms}
      {loading}
      {submitting}
      bind:openCreate={showCreate}
      bind:openJoin={showJoin}
      bind:roomName
      bind:visibility
      bind:gameMode
      bind:turnDurationSeconds
      bind:roomCode
      {loadRooms}
      {createRoom}
      {joinRoom}
    />
  </div>
</div>
