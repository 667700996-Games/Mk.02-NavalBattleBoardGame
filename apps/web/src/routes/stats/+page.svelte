<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { Crosshair, History, Medal, Target, Timer, Trophy } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { session } from '$lib/stores';
  import type { HistoryItem } from '$lib/types';

  let games: HistoryItem[] = [];
  let loading = true;
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
</script>

<svelte:head><title>전투 기록 · Mk.01</title></svelte:head>
<div class="stats-page shell">
  <header>
    <p class="eyebrow">OPERATION ARCHIVE</p>
    <h1 class="page-title">전투 기록</h1>
    <p>완료된 교전 결과와 명중 통계를 확인합니다.</p>
  </header>
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
            ><em>{won(game) ? '승리' : '패배'}</em>
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
    margin-bottom: 28px;
  }
  .stats-page header h1 {
    margin-bottom: 7px;
  }
  .stats-page header > p:last-child {
    color: var(--steel-300);
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
