<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    ArrowLeft,
    Check,
    ChevronLeft,
    ChevronRight,
    Clock3,
    Crosshair,
    Link2,
    Radio
  } from '@lucide/svelte';
  import GridBoard from '$lib/components/GridBoard.svelte';
  import { api, ApiError } from '$lib/api';
  import { analyzeReplay } from '$lib/game/replay-analysis';
  import {
    shipName,
    type CellAttackSnapshot,
    type GameReplay,
    type GameTimelineEvent,
    type OwnBoardSnapshot,
    type ReplayPlayer
  } from '$lib/types';

  let replay = $state<GameReplay | null>(null);
  let step = $state(0);
  let loading = $state(true);
  let error = $state('');
  let linkStatus = $state('');

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
  let analysis = $derived(replay ? analyzeReplay(replay) : null);

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

  function percentage(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function impactLabel(impact: 'CRITICAL' | 'HIGH' | 'MEDIUM'): string {
    if (impact === 'CRITICAL') return '승부 확정';
    if (impact === 'HIGH') return '중대 전환';
    return '주도권 변화';
  }

  async function copyReplayLink() {
    try {
      await navigator.clipboard.writeText(location.href);
      linkStatus = '참가자 전용 링크 복사됨';
    } catch {
      linkStatus = '주소창에서 링크를 복사해 주세요.';
    }
    setTimeout(() => (linkStatus = ''), 2200);
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
    <div class="replay-heading-actions">
      <div>
        <button class="button button--ghost" onclick={copyReplayLink}
          >{#if linkStatus === '참가자 전용 링크 복사됨'}<Check size={16} /> 복사됨{:else}<Link2
              size={16}
            /> 복기 링크 복사{/if}</button
        >
        <a class="button button--ghost" href={resolve('/stats')}
          ><ArrowLeft size={16} /> 전투 기록</a
        >
      </div>
      <small aria-live="polite">{linkStatus || '참가자 세션만 열람할 수 있습니다.'}</small>
    </div>
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

    <section class="balance-record panel" aria-labelledby="balance-record-title">
      <header>
        <h2 id="balance-record-title">검증된 밸런스 기록</h2>
        <strong>RULESET V{replay.balance.rulesetVersion} · PIN VERIFIED</strong>
      </header>
      <p>
        {replay.balance.manifest.boardSize}×{replay.balance.manifest.boardSize} 전장 ·
        {replay.balance.manifest.fleet.length}척 · 고속전
        {replay.balance.manifest.rapidTurnDurationSeconds}초 · 턴 최대
        {replay.balance.manifest.maximumTurnDurationSeconds}초 · 생존 함선당 1발 · 연속
        {replay.balance.manifest.consecutiveTimeoutForfeit}회 시간 초과 시 패배
      </p>
      <p>
        함대 ·
        {#each replay.balance.manifest.fleet as ship, index (ship.kind)}
          <span>{shipName(ship.kind)} {ship.cells}칸</span>{index <
          replay.balance.manifest.fleet.length - 1
            ? ' · '
            : ''}
        {/each}
      </p>
      <code title={replay.balance.checksum}>SHA-256 {replay.balance.checksum}</code>
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
            balance={replay.balance.manifest}
            mode="own"
            label={`${player.nickname} 공개 함대`}
            ownBoard={boardFor(player)}
            disabled={true}
          />
        </article>
      {/each}
    </section>

    {#if analysis}
      <section class="after-action" aria-labelledby="after-action-title">
        <header class="after-action-heading">
          <div>
            <p class="eyebrow">AUTHORITATIVE AFTER ACTION ANALYSIS</p>
            <h2 id="after-action-title">전술 분석</h2>
          </div>
          <p>서버 타임라인만 분석하며 숨겨졌던 함대 정보는 종료 경기에서만 사용합니다.</p>
        </header>

        <div class="analysis-grid">
          {#each analysis.players as playerAnalysis (playerAnalysis.playerId)}
            <article class:winner={playerAnalysis.won} class="analysis-card panel">
              <header>
                <div>
                  <small>{playerAnalysis.won ? 'VICTOR ANALYSIS' : 'FLEET ANALYSIS'}</small>
                  <h3>{playerAnalysis.nickname}</h3>
                </div>
                <strong>{percentage(playerAnalysis.accuracy)}</strong>
              </header>

              <dl class="analysis-stats">
                <div>
                  <dt>명중 / 발사</dt>
                  <dd>{playerAnalysis.hits} / {playerAnalysis.shots}</dd>
                </div>
                <div>
                  <dt>최대 연속 명중</dt>
                  <dd>{playerAnalysis.maxHitStreak}</dd>
                </div>
                <div>
                  <dt>최대 연속 빗나감</dt>
                  <dd>{playerAnalysis.maxMissStreak}</dd>
                </div>
                <div>
                  <dt>격침</dt>
                  <dd>{playerAnalysis.shipsSunk}</dd>
                </div>
                <div>
                  <dt>시간 초과</dt>
                  <dd>{playerAnalysis.timeouts}</dd>
                </div>
              </dl>

              <div class="phase-accuracy" aria-label={`${playerAnalysis.nickname} 구간별 명중률`}>
                {#each playerAnalysis.phases as phase (phase.id)}
                  <div>
                    <span
                      ><strong>{phase.label}</strong><small
                        >{phase.shots ? percentage(phase.accuracy) : '—'}</small
                      ></span
                    >
                    <progress
                      aria-label={`${phase.label} 명중률`}
                      max="100"
                      value={Math.round(phase.accuracy * 100)}
                    ></progress>
                    <small>{phase.hits}명중 / {phase.shots}발</small>
                  </div>
                {/each}
              </div>

              <div class="improvement-tips">
                <h4>다음 교전 개선 제안</h4>
                <ul>
                  {#each playerAnalysis.tips as tip, tipIndex (`${tipIndex}:${tip}`)}
                    <li>{tip}</li>
                  {/each}
                </ul>
              </div>
            </article>
          {/each}
        </div>

        <section class="decisive panel" aria-labelledby="decisive-title">
          <header>
            <div>
              <small>DECISIVE MOMENTS</small>
              <h3 id="decisive-title">결정적 전환점</h3>
            </div>
            <span>영향도 기준 최대 3건</span>
          </header>
          <ol>
            {#each analysis.decisiveMoments as moment, index (`${moment.eventIndex}:${moment.title}`)}
              <li>
                <span class:critical={moment.impact === 'CRITICAL'}
                  >{impactLabel(moment.impact)}</span
                >
                <div>
                  <small>#{index + 1} · {moment.turnNumber}턴 · {playerName(moment.playerId)}</small
                  >
                  <strong>{moment.title}</strong>
                  <p>{moment.detail}</p>
                </div>
                {#if moment.eventIndex !== null}
                  <button
                    class="moment-jump"
                    aria-label={`${moment.title} 사건 보기`}
                    onclick={() => (step = moment.eventIndex! + 1)}>사건 보기</button
                  >
                {/if}
              </li>
            {/each}
          </ol>
        </section>
      </section>
    {/if}

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
  .replay-heading-actions {
    display: grid;
    justify-items: end;
    gap: 6px;
  }
  .replay-heading-actions > div {
    display: flex;
    gap: 8px;
  }
  .replay-heading-actions small {
    min-height: 14px;
    color: var(--ink-500);
    font-size: 8px;
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
  .balance-record {
    margin-top: 14px;
  }
  .balance-record > header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px;
  }
  .balance-record h2,
  .balance-record p {
    margin: 0;
  }
  .balance-record p,
  .balance-record code {
    color: var(--ink-400);
    font-size: 9px;
  }
  .balance-record code {
    overflow-wrap: anywhere;
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
  .after-action {
    margin-top: 30px;
  }
  .after-action-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 14px;
  }
  .after-action-heading h2 {
    margin: 3px 0 0;
    font-size: 22px;
  }
  .after-action-heading > p {
    max-width: 480px;
    margin: 0;
    color: var(--ink-500);
    font-size: 10px;
    text-align: right;
  }
  .analysis-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
  }
  .analysis-card {
    min-width: 0;
    padding: 20px;
    border-top: 2px solid var(--line);
  }
  .analysis-card.winner {
    border-top-color: var(--amber-400);
  }
  .analysis-card > header,
  .decisive > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }
  .analysis-card header small,
  .decisive header small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.14em;
  }
  .analysis-card header h3,
  .decisive header h3 {
    margin: 3px 0 0;
    font-size: 17px;
  }
  .analysis-card header > strong {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 28px;
    letter-spacing: 0.04em;
  }
  .analysis-card.winner header > strong {
    color: var(--amber-400);
  }
  .analysis-stats {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 1px;
    margin: 18px 0;
    background: var(--line);
  }
  .analysis-stats div {
    min-width: 0;
    padding: 10px 8px;
    background: rgba(5, 14, 24, 0.96);
  }
  .analysis-stats dt {
    min-height: 24px;
    color: var(--ink-500);
    font-size: 8px;
  }
  .analysis-stats dd {
    margin: 4px 0 0;
    color: var(--ink-100);
    font-family: var(--font-display);
    font-size: 14px;
  }
  .phase-accuracy {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  .phase-accuracy > div {
    display: grid;
    gap: 5px;
  }
  .phase-accuracy span {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 9px;
  }
  .phase-accuracy span small,
  .phase-accuracy > div > small {
    color: var(--ink-500);
    font-size: 8px;
  }
  .phase-accuracy progress {
    width: 100%;
    height: 5px;
    overflow: hidden;
    border: 0;
    border-radius: 999px;
    color: var(--cyan-300);
    background: rgba(255, 255, 255, 0.07);
  }
  .phase-accuracy progress::-webkit-progress-bar {
    background: rgba(255, 255, 255, 0.07);
  }
  .phase-accuracy progress::-webkit-progress-value {
    background: var(--cyan-300);
  }
  .phase-accuracy progress::-moz-progress-bar {
    background: var(--cyan-300);
  }
  .improvement-tips {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .improvement-tips h4 {
    margin: 0 0 8px;
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.08em;
  }
  .improvement-tips ul {
    display: grid;
    gap: 7px;
    margin: 0;
    padding-left: 16px;
    color: var(--ink-300);
    font-size: 9px;
    line-height: 1.55;
  }
  .decisive {
    margin-top: 14px;
    padding: 20px;
  }
  .decisive > header > span {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
  }
  .decisive ol {
    display: grid;
    gap: 1px;
    margin: 16px 0 0;
    padding: 0;
    list-style: none;
    background: var(--line);
  }
  .decisive li {
    display: grid;
    grid-template-columns: 88px minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
    padding: 13px 14px;
    background: rgba(5, 14, 24, 0.96);
  }
  .decisive li > span {
    width: fit-content;
    padding: 4px 7px;
    border: 1px solid rgba(40, 223, 232, 0.25);
    border-radius: 3px;
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.07em;
  }
  .decisive li > span.critical {
    border-color: rgba(255, 183, 77, 0.35);
    color: var(--amber-400);
  }
  .decisive li div {
    display: grid;
    gap: 2px;
  }
  .decisive li small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
  }
  .decisive li strong {
    color: var(--ink-100);
    font-size: 11px;
  }
  .decisive li p {
    margin: 0;
    color: var(--ink-400);
    font-size: 9px;
  }
  .moment-jump {
    min-height: 34px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 8px;
    background: rgba(40, 223, 232, 0.05);
    cursor: pointer;
  }
  .moment-jump:hover,
  .moment-jump:focus-visible {
    border-color: var(--cyan-300);
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
    .replay-boards,
    .analysis-grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 620px) {
    .replay {
      padding-top: 34px;
    }
    .replay-heading {
      display: grid;
      align-items: start;
    }
    .replay-heading-actions {
      width: 100%;
      justify-items: start;
    }
    .replay-heading-actions > div {
      width: 100%;
    }
    .replay-heading .button {
      padding-inline: 10px;
      font-size: 8px;
    }
    .replay-console {
      padding: 15px;
    }
    .after-action-heading {
      display: grid;
    }
    .after-action-heading > p {
      text-align: left;
    }
    .analysis-stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .phase-accuracy {
      grid-template-columns: 1fr;
    }
    .decisive li {
      grid-template-columns: 1fr;
    }
    .moment-jump {
      justify-self: start;
    }
  }
</style>
