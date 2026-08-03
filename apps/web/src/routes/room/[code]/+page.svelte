<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { ArrowLeft, Check, Radio, ShieldCheck, Wifi, WifiOff } from '@lucide/svelte';
  import BattleView from '$lib/components/BattleView.svelte';
  import DisconnectedOverlay from '$lib/components/DisconnectedOverlay.svelte';
  import FleetPlacement from '$lib/components/FleetPlacement.svelte';
  import ResultView from '$lib/components/ResultView.svelte';
  import WaitingView from '$lib/components/WaitingView.svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { sounds } from '$lib/sound';
  import { gameError, gameSnapshot, lastAttack, session, socketStatus } from '$lib/stores';
  import type { Coordinate, ShipPlacement } from '$lib/types';

  const routeCode = page.params.code.toUpperCase();
  let loading = $state(true);
  let loadError = $state('');
  let placementSubmitting = $state(false);
  let attackPending = $state(false);
  let pendingRequestId = $state<string | null>(null);
  let lastSoundRequest = $state<string | null>(null);
  let resultSoundPlayed = $state(false);

  let snapshot = $derived($gameSnapshot);
  let selfPlayer = $derived(snapshot?.players.find((player) => player.id === snapshot?.selfPlayerId));
  let inviteUrl = $derived(
    typeof location === 'undefined' ? `/join/${routeCode}` : `${location.origin}/join/${routeCode}`
  );

  onMount(() => {
    let active = true;
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
      } catch (caught) {
        if (caught instanceof ApiError && caught.code === 'UNAUTHORIZED') {
          await goto(`/join/${routeCode}`);
          return;
        }
        loadError = caught instanceof ApiError ? caught.message : '전장 상태를 불러오지 못했습니다.';
      } finally {
        loading = false;
      }
    })();
    return () => {
      active = false;
      realtime.disconnect();
    };
  });

  $effect(() => {
    const attack = $lastAttack;
    if (!attack || attack.requestId === lastSoundRequest) return;
    lastSoundRequest = attack.requestId;
    attackPending = false;
    pendingRequestId = null;
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
      payload: { roomId: snapshot.room.id, playerId: snapshot.selfPlayerId }
    });
  }

  function fire(coordinate: Coordinate) {
    if (!snapshot || attackPending || snapshot.turnNumber === null) return;
    const requestId = crypto.randomUUID();
    pendingRequestId = requestId;
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
      pendingRequestId = null;
      gameError.set({ code: 'CONNECTION_REQUIRED', message: '실시간 연결이 복구된 뒤 다시 공격해 주세요.', retryable: true });
    }
  }

  async function leaveRoom() {
    if (!snapshot) return;
    try {
      await api.leaveRoom(snapshot.room.id);
    } finally {
      gameSnapshot.set(null);
      await goto('/lobby');
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
    <div class="loading-view"><div class="spinner"></div><p>암호화된 작전 채널 연결 중…</p></div>
  {:else if loadError}
    <section class="load-error panel"><WifiOff size={34} /><h1>작전 채널에 연결할 수 없습니다</h1><p>{loadError}</p><a class="button" href="/lobby"><ArrowLeft size={16} /> 로비로 복귀</a></section>
  {:else if snapshot}
    <div class="room-meta">
      <div><span class="status-pill"><span class="status-dot"></span>{snapshot.room.status}</span><strong>{snapshot.room.name}</strong><small>CODE {snapshot.room.code} · STATE V{snapshot.version}</small></div>
      <div class:offline={$socketStatus !== 'online'} class="connection-indicator">{#if $socketStatus === 'online'}<Wifi size={14} /> 실시간 연결{:else}<WifiOff size={14} /> {$socketStatus === 'reconnecting' ? '재연결 중' : '오프라인'}{/if}</div>
    </div>

    {#if snapshot.room.status === 'WAITING'}
      <WaitingView {snapshot} {inviteUrl} onleave={leaveRoom} />
    {:else if snapshot.room.status === 'PLACEMENT'}
      {#if selfPlayer?.placementConfirmed}
        <section class="confirmed-wait panel"><div class="confirmed-icon"><Check size={29} /></div><p class="eyebrow">DEPLOYMENT LOCKED</p><h1>함대 배치 확정 완료</h1><p>상대 지휘관의 배치 확정을 기다리고 있습니다. 양쪽이 완료되면 선공을 무작위로 결정합니다.</p><div class="player-ready-list">{#each snapshot.players as player}<div><span class:ready={player.placementConfirmed}><ShieldCheck size={17} /></span><strong>{player.nickname}</strong><em>{player.placementConfirmed ? '배치 확정' : '배치 중'}</em></div>{/each}</div></section>
      {:else}
        <FleetPlacement initialPlacement={snapshot.placement} confirmed={selfPlayer?.placementConfirmed} submitting={placementSubmitting} onconfirm={confirmPlacement} />
      {/if}
    {:else if snapshot.room.status === 'READY'}
      <div class="loading-view"><Radio size={28} class="cyan" /><p>교전 순서 결정 중…</p></div>
    {:else if snapshot.room.status === 'PLAYING'}
      <BattleView {snapshot} pending={attackPending} onfire={fire} />
    {:else if snapshot.room.status === 'DISCONNECTED'}
      {#if snapshot.ownBoard}<BattleView {snapshot} pending={false} disabled={true} onfire={() => {}} />{:else}<FleetPlacement initialPlacement={snapshot.placement} confirmed={true} onconfirm={() => {}} />{/if}
      <DisconnectedOverlay deadline={snapshot.reconnectDeadline} />
    {:else if snapshot.room.status === 'FINISHED'}
      <ResultView {snapshot} onrematch={rematch} onlobby={leaveRoom} />
    {:else}
      <section class="load-error panel"><WifiOff size={34} /><h1>작전이 취소되었습니다</h1><p>상대 지휘관이 이탈했거나 작전실이 종료되었습니다.</p><button class="button" onclick={leaveRoom}><ArrowLeft size={16} /> 로비로 복귀</button></section>
    {/if}
  {/if}
</div>

<style>
  .room-page{padding:28px 0 80px}.room-meta{display:flex;align-items:center;justify-content:space-between;gap:20px;margin-bottom:18px;padding-inline:3px}.room-meta>div:first-child{display:flex;align-items:center;gap:10px}.room-meta strong{font-size:12px}.room-meta small{color:#577585;font-family:Rajdhani;font-size:9px;letter-spacing:.1em}.connection-indicator{display:flex;align-items:center;gap:6px;color:var(--green-500);font-size:10px}.connection-indicator.offline{color:var(--amber-500)}.loading-view{display:grid;min-height:60vh;place-items:center;align-content:center;gap:16px;color:#7895a5;font-size:12px}.load-error{width:min(520px,100%);margin:90px auto;padding:38px;text-align:center}.load-error svg{color:var(--red-500)}.load-error h1{margin-top:20px;font-size:25px}.load-error p{margin-bottom:25px;color:var(--steel-300);font-size:13px;line-height:1.7}.confirmed-wait{width:min(650px,100%);margin:70px auto;padding:40px;text-align:center}.confirmed-icon{display:grid;width:70px;height:70px;place-items:center;margin:0 auto 22px;border:1px solid rgba(61,226,161,.4);border-radius:50%;color:var(--green-500);background:rgba(61,226,161,.08)}.confirmed-wait h1{font-size:28px}.confirmed-wait>p:not(.eyebrow){color:var(--steel-300);font-size:12px;line-height:1.7}.player-ready-list{display:grid;grid-template-columns:1fr 1fr;gap:9px;margin-top:25px}.player-ready-list>div{display:grid;grid-template-columns:auto 1fr;align-items:center;gap:5px 10px;padding:13px;border:1px solid var(--line);border-radius:9px;text-align:left}.player-ready-list span{grid-row:1/3;display:grid;width:34px;height:34px;place-items:center;border-radius:50%;color:#6c8999;background:rgba(75,112,130,.12)}.player-ready-list span.ready{color:var(--green-500);background:rgba(61,226,161,.09)}.player-ready-list strong{font-size:11px}.player-ready-list em{color:#6c8999;font-size:9px;font-style:normal}.player-ready-list .ready+strong+em{color:var(--green-500)}
  @media(max-width:720px){.room-page{padding-top:14px}.room-meta{align-items:start}.room-meta>div:first-child{display:grid;grid-template-columns:auto 1fr;gap:5px 8px}.room-meta small{grid-column:1/-1}.connection-indicator{margin-top:5px}.confirmed-wait{margin:30px auto;padding:30px 16px}.player-ready-list{grid-template-columns:1fr}}
</style>

