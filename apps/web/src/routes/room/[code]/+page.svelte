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
  import { trackFunnelAbandoned, trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import {
    cancelBattleInteraction,
    trackBattleInteractionResult,
    trackBattleInteractionStarted
  } from '$lib/performance';
  import { realtime } from '$lib/realtime';
  import { sounds } from '$lib/sound';
  import { Button, Modal } from '$lib/ui';
  import { localizeError, roomStatusMessageKey, t, type MessageKey } from '$lib/i18n';
  import {
    gameError,
    gameSnapshot,
    lastAttack,
    lastSkill,
    resetRoomRealtimeState,
    session,
    socketStatus
  } from '$lib/stores';
  import type { Coordinate, ShipPlacement, TacticalSkillKind } from '$lib/types';

  const routeCode = (page.params.code ?? '').toUpperCase();
  let loading = $state(true);
  let loadError = $state('');
  let loadErrorKey = $state<MessageKey | null>(null);
  let placementSubmitting = $state(false);
  let attackPending = $state(false);
  let skillPending = $state(false);
  let surrenderPending = $state(false);
  let readyPending = $state(false);
  let startPending = $state(false);
  let showStart = $state(false);
  let lastSoundRequest = $state<string | null>(null);
  let resultSoundPlayed = $state(false);
  let launchSequence = $state(false);
  let launchStage = $state(0);
  let resultTransition = $state(false);
  let previousRoomStatus = '';
  let launchTimer: ReturnType<typeof setInterval> | null = null;
  let resultTimer: ReturnType<typeof setTimeout> | null = null;
  const launchStages: MessageKey[] = [
    'room.launchAuthorized',
    'room.launchEncrypting',
    'room.launchLoading',
    'room.launchDeploy'
  ];

  let snapshot = $derived($gameSnapshot);
  let selfPlayer = $derived(
    snapshot?.players.find((player) => player.id === snapshot?.selfPlayerId)
  );
  let opponentPlayer = $derived(
    snapshot?.players.find((player) => player.id !== snapshot?.selfPlayerId && player.kind !== 'AI')
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
          loadErrorKey = 'room.recoveryMismatch';
          return;
        }
        gameSnapshot.set(recovered);
        trackFunnelReached('room_joined');
        realtime.connect();
        realtime.sync(recovered.room.id);
      } catch (caught) {
        if (caught instanceof ApiError && caught.code === 'UNAUTHORIZED') {
          await goto(resolve('/join/[code]', { code: routeCode }));
          return;
        }
        trackFunnelFailure('room_joined', 'recovery');
        loadError = localizeError(caught, 'room.loadError');
      } finally {
        loading = false;
      }
    })();
    return () => {
      active = false;
      realtime.disconnect();
      resetRoomRealtimeState();
      if (launchTimer) clearInterval(launchTimer);
      if (resultTimer) clearTimeout(resultTimer);
    };
  });

  onDestroy(() => {
    if (launchTimer) clearInterval(launchTimer);
    if (resultTimer) clearTimeout(resultTimer);
  });

  $effect(() => {
    const status = snapshot?.room.status ?? '';
    if (status === 'PLACEMENT' && previousRoomStatus !== 'PLACEMENT') startLaunchSequence();
    if (status === 'FINISHED' && previousRoomStatus === 'PLAYING') {
      resultTransition = true;
      if (resultTimer) clearTimeout(resultTimer);
      resultTimer = setTimeout(() => {
        resultTransition = false;
        resultTimer = null;
      }, 720);
    }
    if (status !== 'FINISHED') resultTransition = false;
    previousRoomStatus = status;
  });

  $effect(() => {
    const skill = $lastSkill;
    if (!skill || skill.attackerId !== snapshot?.selfPlayerId) return;
    skillPending = false;
  });

  $effect(() => {
    const attack = $lastAttack;
    if (!attack || attack.requestId === lastSoundRequest) return;
    lastSoundRequest = attack.requestId;
    attackPending = false;
    trackBattleInteractionResult(attack.requestId);
    if (attack.attackerId === snapshot?.selfPlayerId) trackFunnelReached('first_attack');
    if (attack.outcome === 'MISS') sounds.miss();
    else if (attack.outcome === 'SUNK') sounds.sunk();
    else sounds.hit();
  });

  $effect(() => {
    if (selfPlayer?.placementConfirmed) trackFunnelReached('placement_completed');
    if (snapshot?.room.status === 'FINISHED' && snapshot.result && !resultSoundPlayed) {
      trackFunnelReached('match_completed');
      resultSoundPlayed = true;
      if (snapshot.result.winnerId === snapshot.selfPlayerId) sounds.victory();
      else sounds.defeat();
    }
    if (selfPlayer?.placementConfirmed) placementSubmitting = false;
    if ($gameError) {
      if (placementSubmitting) {
        trackFunnelFailure('placement_completed', 'placement');
        placementSubmitting = false;
      }
      if (attackPending) {
        trackFunnelFailure('first_attack', 'attack');
        attackPending = false;
      }
      skillPending = false;
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
        message: $t('room.readyConnectionError'),
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
        message: $t('room.startConnectionError'),
        retryable: true
      });
    }
  }

  function fire(coordinate: Coordinate) {
    if (!snapshot || attackPending || snapshot.turnNumber === null) return;
    const requestId = crypto.randomUUID();
    attackPending = true;
    trackBattleInteractionStarted(requestId);
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
      cancelBattleInteraction(requestId);
      attackPending = false;
      trackFunnelFailure('first_attack', 'network');
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: $t('room.attackConnectionError'),
        retryable: true
      });
    }
  }

  function fireSkill(skill: TacticalSkillKind, targets: Coordinate[]) {
    if (!snapshot || attackPending || skillPending || snapshot.turnNumber === null) return;
    const requestId = crypto.randomUUID();
    skillPending = true;
    gameError.set(null);
    const sent = realtime.send({
      type: 'skill:fire',
      payload: {
        requestId,
        roomId: snapshot.room.id,
        playerId: snapshot.selfPlayerId,
        skill,
        targets,
        expectedVersion: snapshot.version,
        turnNumber: snapshot.turnNumber
      }
    });
    if (!sent) {
      skillPending = false;
      gameError.set({
        code: 'CONNECTION_REQUIRED',
        message: $t('room.attackConnectionError'),
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
        message: $t('room.surrenderConnectionError'),
        retryable: true
      });
    }
  }

  async function leaveRoom() {
    if (!snapshot) return;
    const returnToSinglePlayer = snapshot.practiceDifficulty !== null;
    if (snapshot.room.status !== 'FINISHED') trackFunnelAbandoned();
    try {
      await api.leaveRoom(snapshot.room.id);
    } finally {
      gameSnapshot.set(null);
      await goto(returnToSinglePlayer ? resolve('/single-player') : resolve('/lobby'));
    }
  }
</script>

<svelte:head><title>{snapshot?.room.name ?? $t('room.connectingTitle')} · Mk.01</title></svelte:head
>

<div class="room-page shell">
  {#if loading}
    <div class="loading-view">
      <div class="spinner"></div>
      <p>{$t('room.connecting')}</p>
    </div>
  {:else if loadError}
    <section class="load-error panel">
      <WifiOff size={34} />
      <h1>{$t('room.connectionFailed')}</h1>
      <p>{loadErrorKey ? $t(loadErrorKey) : loadError}</p>
      <a class="button" href={resolve('/lobby')}><ArrowLeft size={16} /> {$t('room.returnLobby')}</a
      >
    </section>
  {:else if snapshot}
    <div class="room-meta">
      <div>
        <span class="status-pill"
          ><span class="status-dot"></span>{$t(roomStatusMessageKey(snapshot.room.status))}</span
        ><strong>{snapshot.room.name}</strong><small
          >{$t('room.codeState', {
            code: snapshot.room.code,
            version: snapshot.version
          })}</small
        >
      </div>
      <div class:offline={$socketStatus !== 'online'} class="connection-indicator">
        {#if $socketStatus === 'online'}<Wifi size={14} />
          {$t('room.liveConnection')}{:else}<WifiOff size={14} />
          {$socketStatus === 'reconnecting' ? $t('room.reconnecting') : $t('room.offline')}{/if}
      </div>
    </div>

    {#if launchSequence}
      <div class="launch-sequence" role="status" aria-live="polite">
        <span class="launch-sequence__radar"><i></i><b></b></span>
        <small>{$t('room.commandLink')}</small>
        <strong>{$t(launchStages[launchStage])}</strong>
        <div class="launch-sequence__steps">
          {#each launchStages as stage, index (stage)}<i class:active={index <= launchStage}
            ></i>{/each}
        </div>
      </div>
    {/if}

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
          <p class="eyebrow">{$t('room.deploymentLocked')}</p>
          <h1>{$t('room.deploymentConfirmed')}</h1>
          <p>{$t('room.deploymentWait')}</p>
          <div class="player-ready-list">
            {#each snapshot.players as player (player.id)}<div>
                <span class:ready={player.placementConfirmed}><ShieldCheck size={17} /></span
                ><strong>{player.nickname}</strong><em
                  >{player.placementConfirmed
                    ? $t('room.placementConfirmed')
                    : $t('room.placing')}</em
                >
              </div>{/each}
          </div>
        </section>
      {:else}
        <FleetPlacement
          balance={snapshot.balance.manifest}
          initialPlacement={snapshot.placement}
          confirmed={selfPlayer?.placementConfirmed}
          submitting={placementSubmitting}
          onconfirm={confirmPlacement}
        />
      {/if}
    {:else if snapshot.room.status === 'PLAYING'}
      <BattleView
        {snapshot}
        pending={attackPending || skillPending}
        {surrenderPending}
        onfire={fire}
        onskill={fireSkill}
        onsurrender={surrender}
      />
    {:else if snapshot.room.status === 'FINISHED'}
      {#if resultTransition}
        <section
          class:result-recognition--loss={snapshot.result?.winnerId !== snapshot.selfPlayerId}
          class="result-recognition"
          role="status"
          aria-live="polite"
        >
          <span class="result-recognition__pulse"><Check size={20} /></span>
          <small>{$t('room.finalImpact')}</small>
          <strong>{$t('room.resultRecognized')}</strong>
          <span>{$t('room.reportCompiling')}</span>
        </section>
      {/if}
      <ResultView
        {snapshot}
        onreturn={leaveRoom}
        returnLabel={$t(
          snapshot.practiceDifficulty ? 'result.returnSinglePlayer' : 'result.returnLobby'
        )}
      />
    {:else if snapshot.room.status === 'CANCELLED'}
      <section class="load-error panel">
        <WifiOff size={34} />
        <h1>{$t('room.cancelled')}</h1>
        <p>{$t('room.cancelledDescription')}</p>
        <button class="button" onclick={leaveRoom}
          ><ArrowLeft size={16} />
          {$t(snapshot.practiceDifficulty ? 'room.returnSinglePlayer' : 'room.returnLobby')}</button
        >
      </section>
    {:else}
      <section class="load-error panel" role="alert">
        <WifiOff size={34} />
        <h1>{$t('room.versionTitle')}</h1>
        <p>
          {$t('room.versionDescriptionBefore')} <code>npm run dev</code>
          {$t('room.versionDescriptionAfter')}
        </p>
        <a class="button" href={resolve('/lobby')}
          ><ArrowLeft size={16} /> {$t('room.returnLobby')}</a
        >
      </section>
    {/if}
    <ChatDrawer
      roomId={snapshot.room.id}
      selfPlayerId={snapshot.selfPlayerId}
      online={$socketStatus === 'online'}
      readOnly={snapshot.room.status === 'CANCELLED'}
      targetPlayerId={opponentPlayer?.id}
      targetNickname={opponentPlayer?.nickname}
    />
    {#if hasDisconnectedPlayer && (snapshot.room.status === 'PLACEMENT' || snapshot.room.status === 'PLAYING')}
      <DisconnectedOverlay deadline={snapshot.reconnectDeadline} />
    {/if}
  {/if}
</div>

<Modal
  open={showStart}
  eyebrow={$t('room.hostAuthorization')}
  title={$t('room.startTitle')}
  description={$t('room.startDescription')}
  onclose={() => (showStart = false)}
>
  <div class="start-modal-actions">
    <Button variant="secondary" full onclick={() => (showStart = false)}>{$t('room.cancel')}</Button
    >
    <Button
      variant="primary"
      full
      loading={startPending}
      disabled={!snapshot?.canStartGame || $socketStatus !== 'online'}
      onclick={startGame}><Rocket size={15} /> {$t('room.start')}</Button
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
  .room-page {
    max-width: 1500px;
    padding-top: 18px;
  }
  .room-meta {
    margin-bottom: 12px;
    padding: 10px 13px;
    border-radius: 5px 2px 5px 2px;
    border-color: var(--line);
    background: rgba(2, 13, 20, 0.72);
    box-shadow: none;
  }
  .room-meta > div:first-child {
    gap: 12px;
  }
  .room-meta strong {
    font-family: var(--font-display);
    font-size: 16px;
    letter-spacing: 0.04em;
  }
  .room-meta small {
    color: var(--ink-500);
  }
  .status-pill {
    border-radius: 3px;
  }
  .connection-indicator {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.08em;
  }
  .load-error,
  .confirmed-wait {
    border-radius: 10px 3px 10px 3px;
    border-color: var(--line);
    background: linear-gradient(145deg, rgba(7, 28, 36, 0.9), rgba(2, 13, 20, 0.96));
  }
  .confirmed-wait h1 {
    font-family: var(--font-display);
    font-size: 35px;
    letter-spacing: 0.03em;
  }
  .launch-sequence {
    position: relative;
    z-index: 4;
    display: grid;
    justify-items: center;
    gap: 6px;
    margin: -2px 0 12px;
    padding: 14px;
    border: 1px solid rgba(83, 233, 232, 0.28);
    border-radius: 5px 2px 5px 2px;
    background: rgba(2, 16, 22, 0.94);
    box-shadow: 0 16px 35px rgba(0, 0, 0, 0.28);
    animation: launch-in 180ms var(--ease-out) both;
    pointer-events: none;
  }
  .launch-sequence__radar {
    position: relative;
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border: 1px solid var(--tactical);
    border-radius: 50%;
    color: var(--tactical);
  }
  .launch-sequence__radar::before,
  .launch-sequence__radar::after {
    position: absolute;
    content: '';
    background: currentColor;
    opacity: 0.35;
  }
  .launch-sequence__radar::before {
    width: 100%;
    height: 1px;
  }
  .launch-sequence__radar::after {
    width: 1px;
    height: 100%;
  }
  .launch-sequence__radar i {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(83, 233, 232, 0.6), transparent 38deg);
    animation: launch-sweep 1.1s linear infinite;
  }
  .launch-sequence__radar b {
    position: relative;
    z-index: 2;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--tactical);
    box-shadow: 0 0 9px var(--tactical);
  }
  .launch-sequence small {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.18em;
  }
  .launch-sequence strong {
    color: var(--ink-100);
    font: 700 18px var(--font-display);
    letter-spacing: 0.11em;
  }
  .launch-sequence__steps {
    display: flex;
    gap: 5px;
    margin-top: 4px;
  }
  .launch-sequence__steps i {
    width: 32px;
    height: 2px;
    background: var(--line);
  }
  .launch-sequence__steps i.active {
    background: var(--tactical);
    box-shadow: 0 0 8px rgba(83, 233, 232, 0.45);
  }
  @keyframes launch-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @keyframes launch-sweep {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .launch-sequence,
    .launch-sequence__radar i {
      animation: none;
    }
  }
  @media (max-width: 720px) {
    .room-page {
      padding-top: 10px;
    }
    .room-meta {
      padding: 9px;
    }
  }
  .result-recognition {
    position: relative;
    z-index: 6;
    display: grid;
    justify-items: center;
    gap: 7px;
    width: min(560px, 100%);
    margin: 0 auto 12px;
    padding: 20px;
    border: 1px solid rgba(104, 215, 170, 0.42);
    border-top: 2px solid var(--safe);
    background: rgba(3, 21, 25, 0.94);
    box-shadow:
      0 18px 46px rgba(0, 0, 0, 0.3),
      0 0 30px rgba(104, 215, 170, 0.08);
    animation: report-recognition 180ms var(--ease-out) both;
  }
  .result-recognition__pulse {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border: 1px solid var(--safe);
    border-radius: 50%;
    color: var(--safe);
    animation: report-pulse 620ms ease-out both;
  }
  .result-recognition small,
  .result-recognition > span:last-child {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.16em;
  }
  .result-recognition strong {
    color: var(--ink-50);
    font: 700 21px var(--font-display);
    letter-spacing: 0.12em;
  }
  .result-recognition--loss {
    border-color: rgba(238, 86, 103, 0.44);
    border-top-color: var(--critical);
    background: rgba(29, 10, 18, 0.94);
  }
  .result-recognition--loss .result-recognition__pulse {
    border-color: var(--critical);
    color: var(--critical);
  }
  @keyframes report-recognition {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @keyframes report-pulse {
    50% {
      box-shadow:
        0 0 0 9px rgba(104, 215, 170, 0.07),
        0 0 22px rgba(104, 215, 170, 0.26);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .result-recognition,
    .result-recognition__pulse {
      animation: none;
    }
  }
</style>
