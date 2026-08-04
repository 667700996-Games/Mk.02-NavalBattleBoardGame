<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { Activity, Crosshair, History, Medal, Target, Timer, Trophy } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { session } from '$lib/stores';
  import type { HistoryItem } from '$lib/types';

  let games = $state<HistoryItem[]>([]);
  let loading = $state(true);
  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      games = (await api.history()).games;
    } catch {
      await goto(resolve('/'));
    } finally {
      loading = false;
    }
  });
  const won = (game: HistoryItem) => game.result.winnerId === game.selfPlayerId;
  const duration = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  const winTypeLabel = (game: HistoryItem) => {
    if (game.result.winType === 'SURRENDER') return 'SURRENDER';
    if (game.result.winType === 'DISCONNECT') return 'DISCONNECT';
    if (game.result.winType === 'TIMEOUT') return 'TIMEOUT';
    return 'NORMAL VICTORY';
  };
  let wins = $derived(games.filter(won).length);
  let averageAccuracy = $derived(
    games.length
      ? Math.round(
          (games.reduce(
            (total, game) =>
              total +
              (game.result.players.find((player) => player.playerId === game.selfPlayerId)
                ?.accuracy ?? 0),
            0
          ) /
            games.length) *
            100
        )
      : 0
  );
  let totalSunk = $derived(
    games.reduce(
      (total, game) =>
        total +
        (game.result.players.find((player) => player.playerId === game.selfPlayerId)?.shipsSunk ??
          0),
      0
    )
  );
</script>

<svelte:head><title>전투 기록 · Mk.01</title></svelte:head>
<div class="stats-page shell">
  <header>
    <div>
      <p class="eyebrow">OPERATION ARCHIVE / AFTER ACTION DATABASE</p>
      <h1 class="page-title">전투 기록</h1>
      <p>완료된 교전 결과와 명중 통계를 확인합니다.</p>
    </div>
    <span><Activity size={14} /> ARCHIVE SYNCHRONIZED</span>
  </header>
  {#if !loading && games.length > 0}
    <section class="archive-overview" aria-label="전투 기록 요약">
      <article class="panel">
        <small>TOTAL OPERATIONS</small><strong>{games.length}</strong><span>누적 작전</span>
      </article>
      <article class="panel">
        <small>MISSION SUCCESS</small><strong>{Math.round((wins / games.length) * 100)}%</strong
        ><span>{wins}회 승리</span>
      </article>
      <article class="panel">
        <small>AVERAGE ACCURACY</small><strong>{averageAccuracy}%</strong><span>평균 명중률</span>
      </article>
      <article class="panel">
        <small>HOSTILES NEUTRALIZED</small><strong>{totalSunk}</strong><span>누적 격침</span>
      </article>
    </section>
  {/if}
  {#if loading}<div class="empty-state">
      <div class="spinner"></div>
    </div>{:else if games.length === 0}<section class="empty-state panel">
      <div>
        <History size={34} class="muted" />
        <h2>아직 완료된 전투가 없습니다</h2>
        <p class="muted">첫 작전을 완료하면 기록이 이곳에 보존됩니다.</p>
        <a class="button button--primary" href={resolve('/lobby')}>작전 로비로 이동</a>
      </div>
    </section>{:else}<div class="history-list">
      {#each games as game (game.roomId)}<article class="history-row panel">
          <span class:loss={!won(game)} class="result-mark"
            >{#if won(game)}<Trophy size={20} />{:else}<Medal size={20} />{/if}</span
          >
          <div class="history-name">
            <small>{new Date(game.result.finishedAt).toLocaleDateString('ko-KR')}</small><strong
              >{game.roomName}</strong
            ><em>{won(game) ? '승리' : '패배'}</em><span class="win-type">{winTypeLabel(game)}</span
            >
          </div>
          <div>
            <Target size={14} /><span>명중률</span><strong
              >{Math.round(
                (game.result.players.find((p) => p.playerId === game.selfPlayerId)?.accuracy ?? 0) *
                  100
              )}%</strong
            >
          </div>
          <div>
            <Crosshair size={14} /><span>총 턴</span><strong>{game.result.totalTurns}</strong>
          </div>
          <div>
            <Timer size={14} /><span>시간</span><strong
              >{duration(game.result.durationSeconds)}</strong
            >
          </div>
        </article>{/each}
    </div>{/if}
</div>

<style>
  .stats-page {
    padding: 64px 0 100px;
  }
  .stats-page header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 28px;
  }
  .stats-page header h1 {
    margin-bottom: 7px;
  }
  .stats-page header > div > p:last-child {
    color: var(--steel-300);
  }
  .stats-page header > span {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--green-400);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.13em;
  }
  .archive-overview {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin-bottom: 24px;
  }
  .archive-overview article {
    position: relative;
    display: grid;
    gap: 4px;
    min-height: 122px;
    padding: 18px;
    overflow: hidden;
  }
  .archive-overview article::after {
    position: absolute;
    right: -25px;
    bottom: -55px;
    width: 100px;
    height: 100px;
    content: '';
    border: 1px solid rgba(40, 223, 232, 0.11);
    border-radius: 50%;
    box-shadow: 0 0 30px rgba(40, 223, 232, 0.05);
  }
  .archive-overview small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .archive-overview strong {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 31px;
  }
  .archive-overview span {
    color: var(--ink-400);
    font-size: 9px;
  }
  .history-list {
    display: grid;
    gap: 10px;
  }
  .history-row {
    display: grid;
    grid-template-columns: auto 1fr repeat(3, 120px);
    align-items: center;
    gap: 18px;
    padding: 18px;
    border-radius: 13px;
    transition: 250ms var(--ease-out);
  }
  .history-row:hover {
    border-color: var(--line-strong);
    transform: translateX(3px);
  }
  .result-mark {
    display: grid;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid rgba(255, 180, 60, 0.3);
    border-radius: 50%;
    color: var(--amber-500);
    background: rgba(255, 180, 60, 0.07);
  }
  .result-mark.loss {
    border-color: rgba(255, 83, 100, 0.28);
    color: var(--red-500);
    background: rgba(255, 83, 100, 0.06);
  }
  .history-name {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 8px;
  }
  .history-name small {
    color: #688696;
    font-size: 9px;
  }
  .history-name strong {
    grid-column: 1/2;
    font-size: 13px;
  }
  .history-name em {
    grid-row: 2;
    grid-column: 2;
    color: var(--cyan-400);
    font-size: 10px;
    font-style: normal;
  }
  .history-name .win-type {
    grid-column: 1 / -1;
    width: fit-content;
    padding: 3px 6px;
    border: 1px solid rgba(40, 223, 232, 0.14);
    border-radius: 999px;
    color: var(--ink-400);
    background: rgba(40, 223, 232, 0.04);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.08em;
  }
  .history-row > div:not(.history-name) {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 2px 5px;
    color: #7190a0;
  }
  .history-row > div:not(.history-name) span {
    font-size: 9px;
  }
  .history-row > div:not(.history-name) strong {
    grid-column: 2;
    color: #d8e8ef;
    font-family: Rajdhani;
  }
  @media (max-width: 760px) {
    .stats-page {
      padding-top: 40px;
    }
    .history-row {
      grid-template-columns: auto 1fr repeat(3, 1fr);
      gap: 11px;
    }
    .archive-overview {
      grid-template-columns: 1fr 1fr;
    }
    .stats-page header > span {
      display: none;
    }
    .history-row > div:not(.history-name) {
      grid-row: 2;
    }
    .result-mark {
      grid-row: 1;
    }
    .history-name {
      grid-column: 2/5;
    }
    .history-row > div:not(.history-name) :global(svg) {
      display: none;
    }
    .history-row > div:not(.history-name) strong {
      grid-column: 1;
    }
  }
</style>
