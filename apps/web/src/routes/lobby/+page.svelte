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
  import { localizeError, t } from '$lib/i18n';
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
  let spectatableRooms: RoomSummary[] = [];
  let spectatorDelaySeconds = 30;
  let loading = true;
  let error = '';
  let showCreate = false;
  let showJoin = false;
  let roomName = '';
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
    if (!roomName) roomName = $t('lobby.defaultRoomName');
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
          error = localizeError(caught, 'lobby.loadRoomsError');
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
      const [roomResponse, spectatorResponse] = await Promise.all([
        api.listRooms(),
        api.spectatableGames()
      ]);
      rooms = roomResponse.rooms;
      spectatableRooms = spectatorResponse.rooms;
      spectatorDelaySeconds = spectatorResponse.delaySeconds;
      error = '';
    } catch (caught) {
      error = localizeError(caught, 'lobby.loadRoomsError');
    } finally {
      loading = false;
    }
  }

  async function spectate(roomId: string) {
    await goto(resolve('/spectate/[roomId]', { roomId }));
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
      error = localizeError(caught, 'lobby.createError');
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
      error = localizeError(caught, 'lobby.joinError');
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
        error = $t('lobby.rankedAccountRequired');
        return;
      }
      if (matchPool === 'RANKED' && measuredLatency === null) {
        await measureLatency();
      }
      const response = await api.enqueueMatchmaking(currentMatchmakingPreferences());
      await acceptMatchmakingResponse(response);
    } catch (caught) {
      trackFunnelFailure('room_joined', 'matchmaking');
      error = localizeError(caught, 'lobby.matchmakingError');
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
      error = localizeError(caught, 'lobby.practiceError');
    } finally {
      practicing = false;
    }
  }
</script>

<svelte:head><title>{$t('lobby.metaTitle')}</title></svelte:head>

<div class="lobby-page">
  <div class="lobby shell">
    <header class="lobby-heading">
      <div>
        <div class="heading-signal">
          <Badge tone="success" pulse>{$t('lobby.networkOnline')}</Badge><span
            >{$t('lobby.sectorAccess')}</span
          >
        </div>
        <p class="eyebrow">{$t('lobby.eyebrow')}</p>
        <h1 class="page-title">{$t('lobby.title')}</h1>
        <p>
          {$t('lobby.greeting', { commander: $session?.nickname ?? '' })}
        </p>
      </div>
      <div class="lobby-heading__actions">
        <Button variant="outline" onclick={() => (showJoin = true)}
          ><KeyRound size={17} /> {$t('lobby.joinCode')}</Button
        >
        <Button variant="primary" onclick={() => (showCreate = true)}
          ><Plus size={17} /> {$t('lobby.createRoom')}</Button
        >
      </div>
    </header>

    {#if error}<div class="lobby-alert" role="alert">
        <span><Radio size={17} /></span>
        <div>
          <strong>{$t('lobby.channelError')}</strong>
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
      {spectatableRooms}
      {spectatorDelaySeconds}
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
      {spectate}
    />
  </div>
</div>
