<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowLeft, ChevronLeft, ChevronRight, Clock3, Crosshair, Radio } from '@lucide/svelte';
  import GridBoard from '$lib/components/GridBoard.svelte';
  import { api, ApiError } from '$lib/api';
  import type {
    CellAttackSnapshot,
    GameReplay,
    GameTimelineEvent,
    OwnBoardSnapshot,
    ReplayPlayer
  } from '$lib/types';

  let replay = $state<GameReplay | null>(null);
  let step = $state(0);
  let loading = $state(true);
  let error = $state('');

  onMount(async () => {
    try {
      await api.currentSession();
      const roomId = page.params.roomId;
      if (!roomId) throw new Error('missing replay room id');
      replay = await api.replay(roomId);
      step = replay.timeline.length;
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '복기 데이터를 불러오지 못했습니다.';
    } finally {
      loading = false;
    }
  });

  let visibleEvents = $derived(replay?.timeline.slice(0, step) ?? []);
  let currentEvent = $derived(step > 0 ? replay?.timeline[step - 1] : null);

  function attacksFor(playerId: string): Extract<GameTimelineEvent, { type: 'ATTACK' }>[] {
    return visibleEvents.filter(
      (event): event is Extract<GameTimelineEvent, { type: 'ATTACK' }> =>
        event.type === 'ATTACK' && event.payload.targetId === playerId
    );
  }

  function boardFor(player: ReplayPlayer): OwnBoardSnapshot {
    const attacks = attacksFor(player.id);
    const hitKeys = new Set(
      attacks
        .filter((event) => event.payload.outcome !== 'MISS')
        .map((event) => `${event.payload.coordinate.row}:${event.payload.coordinate.col}`)
    );
    const attacksReceived: CellAttackSnapshot[] = attacks.map((event) => ({
      coordinate: event.payload.coordinate,
      outcome: event.payload.outcome
    }));
    return {
      ships: player.fleet.map((ship) => {
        const hits = ship.cells.filter((cell) => hitKeys.has(`${cell.row}:${cell.col}`));
        return {
          kind: ship.kind,
          cells: ship.cells,
          hits,
          sunk: hits.length === ship.cells.length
        };
      }),
      attacksReceived
    };
  }

  function playerName(playerId: string | null | undefined): string {
    return replay?.players.find((player) => player.id === playerId)?.nickname ?? '지휘관';
  }

  function eventLabel(event: GameTimelineEvent | null | undefined): string {
    if (!event) return '교전 개시 전 · 양측 함대 배치 공개';
    if (event.type === 'TURN_EXPIRED') {
      return `${event.payload.expiredTurnNumber}턴 · ${playerName(event.payload.expiredPlayerId)} 시간 초과`;
    }
    const coordinate = `${String.fromCharCode(65 + event.payload.coordinate.row)}${event.payload.coordinate.col + 1}`;
    const outcome =
      event.payload.outcome === 'MISS'
        ? '빗나감'
        : event.payload.outcome === 'HIT'
          ? '명중'
          : '격침';
    return `${event.payload.turnNumber}턴 · ${playerName(event.payload.attackerId)} ${coordinate} 공격 · ${outcome}`;
  }
</script>

<svelte:head><title>전투 복기 · Mk.01</title></svelte:head>

<main class="replay shell">
  <header class="replay-heading">
    <div>
      <p class="eyebrow">AFTER ACTION REPLAY / RULESET {replay?.rulesetVersion ?? '—'}</p>
      <h1 class="page-title">전투 복기</h1>
      <p>{replay?.roomName ?? '작전 기록을 복호화하고 있습니다.'}</p>
    </div>
    <a class="button button--ghost" href={resolve('/stats')}><ArrowLeft size={16} /> 전투 기록</a>
  </header>

  {#if loading}
    <section class="replay-state panel">
      <div class="spinner"></div>
      <p>작전 타임라인 복호화 중</p>
    </section>
  {:else if error || !replay}
    <section class="replay-state panel" role="alert">
      <Radio size={28} />
      <h2>REPLAY SIGNAL LOST</h2>
      <p>{error}</p>
      <button class="button" onclick={() => goto(resolve('/stats'))}>기록실로 복귀</button>
    </section>
  {:else}
    <section class="replay-console panel" aria-label="전투 복기 조작기">
      <div class="replay-status">
        <span><Clock3 size={15} /> STEP {step} / {replay.timeline.length}</span>
        <strong aria-live="polite">{eventLabel(currentEvent)}</strong>
        <small>PROTOCOL {replay.protocolVersion} · RULESET {replay.rulesetVersion}</small>
      </div>
      <div class="replay-controls">
        <button aria-label="이전 사건" disabled={step === 0} onclick={() => (step -= 1)}
          ><ChevronLeft size={18} /></button
        >
        <input
          aria-label="복기 사건 위치"
          type="range"
          min="0"
          max={replay.timeline.length}
          bind:value={step}
        />
        <button
          aria-label="다음 사건"
          disabled={step === replay.timeline.length}
          onclick={() => (step += 1)}><ChevronRight size={18} /></button
        >
      </div>
    </section>

    <section class="replay-boards" aria-label="양측 공개 함대">
      {#each replay.players as player (player.id)}
        <article class="panel">
          <header>
            <div>
              <small>{player.kind === 'AI' ? 'AI OPPONENT' : 'FLEET COMMAND'}</small>
              <h2>{player.nickname}</h2>
            </div>
            <span class:winner={replay.result.winnerId === player.id}
              ><Crosshair size={14} />
              {replay.result.winnerId === player.id ? 'WINNER' : 'FLEET'}</span
            >
          </header>
          <GridBoard
            mode="own"
            label={`${player.nickname} 공개 함대`}
            ownBoard={boardFor(player)}
            disabled={true}
          />
        </article>
      {/each}
    </section>

    <ol class="event-log panel" aria-label="작전 사건 목록">
      {#each replay.timeline as event, index (index)}
        <li class:active={index + 1 === step} class:future={index + 1 > step}>
          <button onclick={() => (step = index + 1)}
            ><span>{String(index + 1).padStart(2, '0')}</span><strong>{eventLabel(event)}</strong
            ></button
          >
        </li>
      {/each}
    </ol>
  {/if}
</main>

<style>
  .replay {
    padding: 56px 0 100px;
  }
  .replay-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 24px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--line);
  }
  .replay-heading h1 {
    margin-bottom: 5px;
  }
  .replay-heading > div > p:last-child {
    margin: 0;
    color: var(--ink-400);
  }
  .replay-state {
    display: grid;
    min-height: 340px;
    place-items: center;
    align-content: center;
    gap: 12px;
    text-align: center;
  }
  .replay-state h2,
  .replay-state p {
    margin: 0;
  }
  .replay-state :global(svg) {
    color: var(--red-400);
  }
  .replay-console {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(280px, 0.7fr);
    align-items: center;
    gap: 28px;
    padding: 18px 22px;
  }
  .replay-status {
    display: grid;
    gap: 4px;
  }
  .replay-status span,
  .replay-status small {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.12em;
  }
  .replay-status strong {
    font-size: 13px;
  }
  .replay-status small {
    color: var(--ink-500);
  }
  .replay-controls {
    display: grid;
    grid-template-columns: 38px 1fr 38px;
    align-items: center;
    gap: 9px;
  }
  .replay-controls button {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 5px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.05);
    cursor: pointer;
  }
  .replay-controls button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .replay-controls input {
    width: 100%;
    accent-color: var(--cyan-300);
  }
  .replay-boards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-top: 14px;
  }
  .replay-boards article {
    min-width: 0;
    padding: 18px;
  }
  .replay-boards header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 14px;
  }
  .replay-boards header small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.14em;
  }
  .replay-boards h2 {
    margin: 3px 0 0;
    font-size: 17px;
  }
  .replay-boards header > span {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
  }
  .replay-boards header > span.winner {
    color: var(--amber-400);
  }
  .event-log {
    display: grid;
    gap: 1px;
    max-height: 280px;
    margin: 14px 0 0;
    padding: 12px;
    overflow: auto;
    list-style: none;
  }
  .event-log li button {
    display: grid;
    grid-template-columns: 34px 1fr;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    color: var(--ink-300);
    text-align: left;
    background: transparent;
    cursor: pointer;
  }
  .event-log li span {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 9px;
  }
  .event-log li strong {
    font-size: 10px;
    font-weight: 500;
  }
  .event-log li.active button {
    color: white;
    background: rgba(40, 223, 232, 0.08);
  }
  .event-log li.future {
    opacity: 0.42;
  }
  @media (max-width: 850px) {
    .replay-console,
    .replay-boards {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 620px) {
    .replay {
      padding-top: 34px;
    }
    .replay-heading {
      align-items: start;
    }
    .replay-heading .button {
      padding-inline: 10px;
      font-size: 0;
    }
    .replay-console {
      padding: 15px;
    }
  }
</style>
