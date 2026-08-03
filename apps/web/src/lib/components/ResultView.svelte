<script lang="ts">
  import { ArrowLeft, Crosshair, Medal, RotateCcw, Target, Timer, Trophy } from '@lucide/svelte';
  import type { GameSnapshot } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    onrematch: () => void;
    onlobby: () => void;
  }
  let { snapshot, onrematch, onlobby }: Props = $props();
  let won = $derived(snapshot.result?.winnerId === snapshot.selfPlayerId);
  let stats = $derived(snapshot.result?.players.find((player) => player.playerId === snapshot.selfPlayerId));
  let opponentStats = $derived(snapshot.result?.players.find((player) => player.playerId !== snapshot.selfPlayerId));
  let rematchRequested = $derived(snapshot.rematchRequestedBy.includes(snapshot.selfPlayerId));
  const formatDuration = (seconds: number) => `${Math.floor(seconds / 60)}분 ${seconds % 60}초`;
</script>

<section class:result--loss={!won} class="result panel">
  <div class="result__emblem">{#if won}<Trophy size={42} />{:else}<Medal size={42} />{/if}</div>
  <p class="eyebrow">OPERATION COMPLETE</p>
  <h1>{won ? '작전 승리' : '작전 패배'}</h1>
  <p class="result__summary">{won ? '상대 함대 전력을 모두 무력화했습니다.' : '아군 함대가 전투 불능 상태에 도달했습니다.'}</p>

  <div class="result-score"><div class:score-winner={won}><small>{snapshot.players.find((player)=>player.id===snapshot.selfPlayerId)?.nickname}</small><strong>{stats?.hits ?? 0}</strong><span>명중</span></div><em>VS</em><div class:score-winner={!won}><small>{snapshot.players.find((player)=>player.id!==snapshot.selfPlayerId)?.nickname}</small><strong>{opponentStats?.hits ?? 0}</strong><span>명중</span></div></div>

  <div class="stats-grid">
    <article><Target size={19} /><span>명중률</span><strong>{Math.round((stats?.accuracy ?? 0) * 100)}%</strong><small>{stats?.hits ?? 0} / {stats?.shots ?? 0} 공격</small></article>
    <article><Crosshair size={19} /><span>격침</span><strong>{stats?.shipsSunk ?? 0}</strong><small>총 5척 중</small></article>
    <article><Timer size={19} /><span>작전 시간</span><strong>{formatDuration(snapshot.result?.durationSeconds ?? 0)}</strong><small>{snapshot.result?.totalTurns ?? 0} 총 턴</small></article>
  </div>

  <div class="result-actions"><button class="button button--primary" onclick={onrematch} disabled={rematchRequested}><RotateCcw size={16} /> {rematchRequested ? '상대 응답 대기 중' : '재대결 요청'}</button><button class="button" onclick={onlobby}><ArrowLeft size={16} /> 로비로 복귀</button></div>
</section>

<style>
  .result{width:min(780px,100%);margin:0 auto;padding:42px;text-align:center;border-color:rgba(57,224,235,.28)}.result--loss{border-color:rgba(255,83,100,.22)}.result__emblem{display:grid;width:88px;height:88px;place-items:center;margin:0 auto 22px;border:1px solid rgba(255,180,60,.46);border-radius:50%;color:var(--amber-500);background:radial-gradient(circle,rgba(255,180,60,.16),transparent 66%);box-shadow:0 0 45px rgba(255,180,60,.08)}.result--loss .result__emblem{border-color:rgba(255,83,100,.4);color:var(--red-500);background:radial-gradient(circle,rgba(255,83,100,.13),transparent 66%)}.result h1{margin-bottom:5px;font-family:Rajdhani,sans-serif;font-size:42px}.result__summary{color:var(--steel-300)}.result-score{display:grid;grid-template-columns:1fr auto 1fr;align-items:center;gap:25px;margin:30px 0;padding:20px;border-block:1px solid var(--line)}.result-score>div{display:grid;gap:2px}.result-score small{color:#7894a4}.result-score strong{font-family:Rajdhani;font-size:35px}.result-score span{color:#607d8d;font-size:9px}.result-score em{color:#557283;font-family:Rajdhani;font-size:13px;font-style:normal}.result-score .score-winner strong{color:var(--cyan-400)}.stats-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}.stats-grid article{display:grid;place-items:center;gap:4px;padding:17px;border:1px solid var(--line);border-radius:10px;background:rgba(4,18,28,.5)}.stats-grid :global(svg){color:var(--cyan-400)}.stats-grid span{color:#7895a5;font-size:9px}.stats-grid strong{font-family:Rajdhani;font-size:21px}.stats-grid small{color:#5d7a8a;font-size:8px}.result-actions{display:flex;justify-content:center;gap:9px;margin-top:28px}
  @media(max-width:600px){.result{padding:30px 16px}.stats-grid{grid-template-columns:1fr 1fr}.stats-grid article:last-child{grid-column:1/-1}.result-actions{display:grid}.result-score{gap:10px}.result h1{font-size:35px}}
</style>
