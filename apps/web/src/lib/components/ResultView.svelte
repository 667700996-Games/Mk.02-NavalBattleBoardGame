<script lang="ts">
  import { resolve } from '$app/paths';
  import {
    ArrowLeft,
    Check,
    Crosshair,
    Flag,
    Medal,
    Play,
    RotateCcw,
    Share2,
    Target,
    Timer,
    Trophy
  } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import type { GameSnapshot } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    onrematch: () => void;
    onlobby: () => void;
  }
  let { snapshot, onrematch, onlobby }: Props = $props();
  let won = $derived(snapshot.result?.winnerId === snapshot.selfPlayerId);
  let stats = $derived(
    snapshot.result?.players.find((player) => player.playerId === snapshot.selfPlayerId)
  );
  let opponentStats = $derived(
    snapshot.result?.players.find((player) => player.playerId !== snapshot.selfPlayerId)
  );
  let rematchRequested = $derived(snapshot.rematchRequestedBy.includes(snapshot.selfPlayerId));
  let operationStatus = $derived(won ? 'OPERATION COMPLETE' : 'OPERATION FAILED');
  let outcomeLabel = $derived.by(() => {
    switch (snapshot.result?.winType) {
      case 'SURRENDER':
        return won ? 'Victory by Surrender' : 'Defeat by Surrender';
      case 'DISCONNECT':
        return won ? 'Victory by Disconnect' : 'Defeat by Disconnect';
      case 'TIMEOUT':
        return won ? 'Victory by Timeout' : 'Defeat by Timeout';
      default:
        return won ? 'Normal Victory' : 'Normal Defeat';
    }
  });
  let outcomeSummary = $derived.by(() => {
    if (snapshot.result?.winType === 'SURRENDER') {
      return won ? '적 지휘관이 작전을 포기했습니다.' : '작전 포기로 교전이 종료되었습니다.';
    }
    if (snapshot.result?.winType === 'DISCONNECT') {
      return won
        ? '적 지휘관의 연결 복구 시간이 만료되었습니다.'
        : '연결 복구 시간이 만료되어 작전이 종료되었습니다.';
    }
    if (snapshot.result?.winType === 'TIMEOUT') {
      return won
        ? '적 지휘관이 3회 연속 작전 시간을 초과했습니다.'
        : '3회 연속 작전 시간 초과로 교전이 종료되었습니다.';
    }
    return won
      ? '상대 함대 전력을 모두 무력화했습니다.'
      : '아군 함대가 전투 불능 상태에 도달했습니다.';
  });
  let shared = $state(false);
  const formatDuration = (seconds: number) => {
    const hours = Math.floor(seconds / 3_600);
    const minutes = Math.floor((seconds % 3_600) / 60);
    const rest = seconds % 60;
    return hours > 0
      ? `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
      : `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
  };

  async function shareResult() {
    const title = `${won ? '작전 승리' : '작전 완료'} · Mk.01`;
    const text = `${outcomeLabel} · ${snapshot.room.name}에서 ${stats?.hits ?? 0}회 명중, ${Math.round((stats?.accuracy ?? 0) * 100)}% 명중률을 기록했습니다.`;
    if (navigator.share) await navigator.share({ title, text, url: location.origin });
    else await navigator.clipboard.writeText(`${title}\n${text}\n${location.origin}`);
    shared = true;
    setTimeout(() => (shared = false), 1800);
  }
</script>

<section class:result--loss={!won} class="result panel">
  <span class="result__watermark" aria-hidden="true">{won ? 'VICTORY' : 'DEFEAT'}</span>
  <div class="result__classification"><span></span> AFTER ACTION REPORT <span></span></div>
  <div class="result__emblem">
    {#if won}<Trophy size={42} />{:else}<Medal size={42} />{/if}
  </div>
  <p class="eyebrow">{operationStatus}</p>
  <h1>{won ? '작전 승리' : '작전 패배'}</h1>
  <p class="result__outcome"><Flag size={13} /> {outcomeLabel}</p>
  <p class="result__summary">{outcomeSummary}</p>

  <div class="result-score">
    <div class:score-winner={won}>
      <small
        >{snapshot.players.find((player) => player.id === snapshot.selfPlayerId)?.nickname}</small
      ><strong>{stats?.hits ?? 0}</strong><span>명중</span>
    </div>
    <em>VS</em>
    <div class:score-winner={!won}>
      <small
        >{snapshot.players.find((player) => player.id !== snapshot.selfPlayerId)?.nickname}</small
      ><strong>{opponentStats?.hits ?? 0}</strong><span>명중</span>
    </div>
  </div>

  <div class="stats-grid">
    <article>
      <Target size={19} /><span>명중률</span><strong
        >{Math.round((stats?.accuracy ?? 0) * 100)}%</strong
      ><small>{stats?.hits ?? 0} / {stats?.shots ?? 0} 공격</small>
    </article>
    <article>
      <Crosshair size={19} /><span>격침</span><strong>{stats?.shipsSunk ?? 0}</strong><small
        >총 5척 중</small
      >
    </article>
    <article>
      <Timer size={19} /><span>작전 시간</span><strong
        >{formatDuration(snapshot.result?.durationSeconds ?? 0)}</strong
      ><small>{snapshot.result?.totalTurns ?? 0} 총 턴</small>
    </article>
    <article>
      <Flag size={19} /><span>시간 초과</span><strong>{stats?.totalTimeouts ?? 0}</strong><small
        >연속 3회 시 자동 패배</small
      >
    </article>
  </div>

  {#if snapshot.revealedBoard}
    <section class="report-intel" aria-labelledby="report-intel-title">
      <header>
        <div>
          <small>FINAL INTELLIGENCE / DECLASSIFIED</small>
          <h2 id="report-intel-title">적 함대 배치 복기</h2>
        </div>
        <span>FOG OF WAR / LIFTED</span>
      </header>
      <div class="report-intel__layout">
        <GridBoard
          mode="own"
          label="공개된 적 함대 배치"
          ownBoard={snapshot.revealedBoard}
          disabled={true}
        />
        <div class="report-intel__copy">
          <p>
            교전 종료와 함께 적 함대의 실제 위치가 공개되었습니다. 공격 기록과 함선의 형태를 겹쳐
            다음 작전의 추론 근거를 확인하십시오.
          </p>
          <div class="report-intel__legend">
            <span><i class="report-ship"></i> 적 함선</span><span
              ><i class="report-hit"></i> 명중 지점</span
            ><span><i class="report-miss"></i> 빗나간 좌표</span>
          </div>
        </div>
      </div>
    </section>
  {/if}

  <div class="result-actions">
    <button class="button button--primary" onclick={onrematch} disabled={rematchRequested}
      ><RotateCcw size={16} /> {rematchRequested ? '상대 응답 대기 중' : '재대결 요청'}</button
    ><button class="button" onclick={shareResult}
      >{#if shared}<Check size={16} /> 공유 정보 복사됨{:else}<Share2 size={16} /> 결과 공유{/if}</button
    ><a class="button" href={resolve('/replay/[roomId]', { roomId: snapshot.room.id })}
      ><Play size={16} /> 전투 복기</a
    ><button class="button button--ghost" onclick={onlobby}
      ><ArrowLeft size={16} /> 로비로 복귀</button
    >
  </div>
</section>

<style>
  .result {
    position: relative;
    width: min(780px, 100%);
    margin: 0 auto;
    padding: 42px;
    text-align: center;
    border-color: rgba(57, 224, 235, 0.28);
    overflow: hidden;
    background:
      radial-gradient(circle at 50% 0%, rgba(40, 223, 232, 0.12), transparent 38%),
      linear-gradient(155deg, rgba(10, 32, 44, 0.96), rgba(3, 13, 21, 0.96));
    box-shadow:
      0 40px 100px rgba(0, 0, 0, 0.45),
      0 0 80px rgba(40, 223, 232, 0.055);
  }
  .result--loss {
    border-color: rgba(255, 83, 100, 0.22);
  }
  .result__watermark {
    position: absolute;
    top: 10px;
    left: 50%;
    color: rgba(111, 244, 246, 0.035);
    font-family: var(--font-display);
    font-size: clamp(74px, 13vw, 142px);
    font-weight: 700;
    letter-spacing: 0.12em;
    line-height: 1;
    transform: translateX(-50%);
    pointer-events: none;
  }
  .result--loss .result__watermark {
    color: rgba(255, 114, 128, 0.035);
  }
  .result__classification {
    position: relative;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 12px;
    margin-bottom: 24px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.22em;
  }
  .result__classification span {
    height: 1px;
    background: var(--line);
  }
  .result__emblem {
    position: relative;
    display: grid;
    width: 88px;
    height: 88px;
    place-items: center;
    margin: 0 auto 22px;
    border: 1px solid rgba(255, 180, 60, 0.46);
    border-radius: 50%;
    color: var(--amber-500);
    background: radial-gradient(circle, rgba(255, 180, 60, 0.16), transparent 66%);
    box-shadow: 0 0 45px rgba(255, 180, 60, 0.08);
  }
  .result__emblem::after {
    position: absolute;
    inset: -9px;
    content: '';
    border: 1px dashed rgba(255, 209, 107, 0.2);
    border-radius: 50%;
    animation: radar 12s linear infinite;
  }
  .result--loss .result__emblem {
    border-color: rgba(255, 83, 100, 0.4);
    color: var(--red-500);
    background: radial-gradient(circle, rgba(255, 83, 100, 0.13), transparent 66%);
  }
  .result h1 {
    margin-bottom: 5px;
    font-family: Rajdhani, sans-serif;
    font-size: 42px;
  }
  .result__summary {
    color: var(--steel-300);
  }
  .result__outcome {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 6px;
    padding: 5px 9px;
    border: 1px solid rgba(40, 223, 232, 0.2);
    border-radius: 999px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .result--loss .result__outcome {
    border-color: rgba(255, 83, 100, 0.2);
    color: var(--red-400);
    background: rgba(255, 83, 100, 0.06);
  }
  .result-score {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 25px;
    margin: 30px 0;
    padding: 20px;
    border-block: 1px solid var(--line);
  }
  .result-score > div {
    display: grid;
    gap: 2px;
  }
  .result-score small {
    color: #7894a4;
  }
  .result-score strong {
    font-family: Rajdhani;
    font-size: 35px;
  }
  .result-score span {
    color: #607d8d;
    font-size: 9px;
  }
  .result-score em {
    color: #557283;
    font-family: Rajdhani;
    font-size: 13px;
    font-style: normal;
  }
  .result-score .score-winner strong {
    color: var(--cyan-400);
  }
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
  }
  .stats-grid article {
    display: grid;
    place-items: center;
    gap: 4px;
    padding: 17px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(4, 18, 28, 0.5);
    transition: 240ms var(--ease-out);
  }
  .stats-grid article:hover {
    border-color: var(--line-strong);
    transform: translateY(-2px);
  }
  .stats-grid :global(svg) {
    color: var(--cyan-400);
  }
  .stats-grid span {
    color: #7895a5;
    font-size: 9px;
  }
  .stats-grid strong {
    font-family: Rajdhani;
    font-size: 21px;
  }
  .stats-grid small {
    color: #5d7a8a;
    font-size: 8px;
  }
  .result-actions {
    display: flex;
    justify-content: center;
    gap: 9px;
    margin-top: 28px;
  }
  @media (max-width: 600px) {
    .result {
      padding: 30px 16px;
    }
    .stats-grid {
      grid-template-columns: 1fr 1fr;
    }
    .result-actions {
      display: grid;
    }
    .result-score {
      gap: 10px;
    }
    .result h1 {
      font-size: 35px;
    }
  }

  .result {
    width: min(980px, 100%);
    padding: 34px;
    border-radius: 10px 3px 10px 3px;
    border-color: rgba(83, 233, 232, 0.3);
    background: linear-gradient(150deg, rgba(8, 29, 37, 0.96), rgba(2, 12, 19, 0.98));
  }
  .result--loss {
    border-color: rgba(238, 86, 103, 0.28);
  }
  .result--loss .eyebrow {
    color: var(--critical);
  }
  .result h1 {
    font-family: var(--font-display);
    font-size: clamp(42px, 6vw, 62px);
    letter-spacing: 0.03em;
  }
  .result__classification {
    color: var(--ink-500);
  }
  .result__emblem {
    border-radius: 10px 3px 10px 3px;
  }
  .result-score {
    margin: 24px 0;
    background: rgba(1, 9, 14, 0.3);
  }
  .result-score strong {
    font-family: var(--font-display);
    font-size: 42px;
  }
  .stats-grid article {
    border-radius: 5px 2px 5px 2px;
    background: rgba(2, 13, 20, 0.65);
  }
  .report-intel {
    margin-top: 22px;
    padding-top: 20px;
    border-top: 1px solid var(--line);
    text-align: left;
  }
  .report-intel > header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .report-intel > header small {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.15em;
  }
  .report-intel > header h2 {
    margin: 4px 0 0;
    font-family: var(--font-display);
    font-size: 21px;
  }
  .report-intel > header span {
    color: var(--safe);
    font: 600 8px var(--font-display);
    letter-spacing: 0.1em;
  }
  .report-intel__layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 220px;
    gap: 18px;
    align-items: center;
  }
  .report-intel__layout :global(.board-wrap) {
    max-width: 500px;
  }
  .report-intel__copy {
    color: var(--ink-400);
    font-size: 11px;
    line-height: 1.8;
  }
  .report-intel__copy p {
    margin: 0 0 16px;
  }
  .report-intel__legend {
    display: grid;
    gap: 8px;
    color: var(--ink-300);
    font: 600 9px var(--font-display);
  }
  .report-intel__legend span {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .report-intel__legend i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .report-ship {
    background: #75b9bd;
  }
  .report-hit {
    background: #ff7e46;
  }
  .report-miss {
    background: #6bb6d1;
  }
  @media (max-width: 650px) {
    .result {
      padding: 26px 14px;
    }
    .report-intel__layout {
      grid-template-columns: 1fr;
    }
    .report-intel__copy {
      padding: 0 6px;
    }
  }
</style>
