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
  import { api } from '$lib/api';
  import { analyzeReplay } from '$lib/game/replay-analysis';
  import { localizeError, t } from '$lib/i18n';
  import {
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
  let linkStatus = $state<'copied' | 'manual' | ''>('');

  onMount(async () => {
    try {
      await api.currentSession();
      const roomId = page.params.roomId;
      if (!roomId) throw new Error('missing replay room id');
      replay = await api.replay(roomId);
      step = replay.timeline.length;
    } catch (caught) {
      error = localizeError(caught, 'replay.loadError');
    } finally {
      loading = false;
    }
  });

  let visibleEvents = $derived(replay?.timeline.slice(0, step) ?? []);
  let currentEvent = $derived(step > 0 ? replay?.timeline[step - 1] : null);
  let analysis = $derived(replay ? analyzeReplay(replay, $t) : null);

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
    return (
      replay?.players.find((player) => player.id === playerId)?.nickname ?? $t('common.commander')
    );
  }

  function eventLabel(event: GameTimelineEvent | null | undefined): string {
    if (!event) return $t('replay.beforeBattle');
    if (event.type === 'TURN_EXPIRED') {
      return $t('replay.timeoutEvent', {
        turn: event.payload.expiredTurnNumber,
        player: playerName(event.payload.expiredPlayerId)
      });
    }
    const coordinate = `${String.fromCharCode(65 + event.payload.coordinate.row)}${event.payload.coordinate.col + 1}`;
    const outcome = $t(`attackOutcome.${event.payload.outcome}`);
    return $t('replay.attackEvent', {
      turn: event.payload.turnNumber,
      player: playerName(event.payload.attackerId),
      coordinate,
      outcome
    });
  }

  function percentage(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function impactLabel(impact: 'CRITICAL' | 'HIGH' | 'MEDIUM'): string {
    if (impact === 'CRITICAL') return $t('replay.impactCritical');
    if (impact === 'HIGH') return $t('replay.impactHigh');
    return $t('replay.impactMedium');
  }

  async function copyReplayLink() {
    try {
      await navigator.clipboard.writeText(location.href);
      linkStatus = 'copied';
    } catch {
      linkStatus = 'manual';
    }
    setTimeout(() => (linkStatus = ''), 2200);
  }
</script>

<svelte:head><title>{$t('replay.metaTitle')}</title></svelte:head>

<main class="replay shell">
  <header class="replay-heading">
    <div>
      <p class="eyebrow">
        {$t('replay.eyebrow', { ruleset: replay?.rulesetVersion ?? '—' })}
      </p>
      <h1 class="page-title">{$t('replay.title')}</h1>
      <p>{replay?.roomName ?? $t('replay.decryptingRecord')}</p>
    </div>
    <div class="replay-heading-actions">
      <div>
        <button class="button" onclick={copyReplayLink}
          >{#if linkStatus === 'copied'}<Check size={16} /> {$t('common.copied')}{:else}<Link2
              size={16}
            />
            {$t('replay.copyLink')}{/if}</button
        >
        <a class="button" href={resolve('/stats')}
          ><ArrowLeft size={16} /> {$t('replay.battleRecords')}</a
        >
      </div>
      <small aria-live="polite"
        >{linkStatus === 'copied'
          ? $t('replay.linkCopied')
          : linkStatus === 'manual'
            ? $t('replay.copyManual')
            : $t('replay.participantOnly')}</small
      >
    </div>
  </header>

  {#if loading}
    <section class="replay-state panel">
      <div class="spinner"></div>
      <p>{$t('replay.decryptingTimeline')}</p>
    </section>
  {:else if error || !replay}
    <section class="replay-state panel" role="alert">
      <Radio size={28} />
      <h2>{$t('replay.signalLost')}</h2>
      <p>{error}</p>
      <button class="button" onclick={() => goto(resolve('/stats'))}
        >{$t('replay.returnArchive')}</button
      >
    </section>
  {:else}
    <section class="replay-console panel" aria-label={$t('replay.controls')}>
      <div class="replay-status">
        <span
          ><Clock3 size={15} />
          {$t('replay.step', {
            step,
            total: replay.timeline.length
          })}</span
        >
        <strong aria-live="polite">{eventLabel(currentEvent)}</strong>
        <small
          >{$t('replay.protocolRuleset', {
            protocol: replay.protocolVersion,
            ruleset: replay.rulesetVersion
          })}</small
        >
      </div>
      <div class="replay-controls">
        <button
          aria-label={$t('replay.previousEvent')}
          disabled={step === 0}
          onclick={() => (step -= 1)}><ChevronLeft size={18} /></button
        >
        <input
          aria-label={$t('replay.eventPosition')}
          type="range"
          min="0"
          max={replay.timeline.length}
          bind:value={step}
        />
        <button
          aria-label={$t('replay.nextEvent')}
          disabled={step === replay.timeline.length}
          onclick={() => (step += 1)}><ChevronRight size={18} /></button
        >
      </div>
    </section>

    <section class="replay-boards" aria-label={$t('replay.revealedFleets')}>
      {#each replay.players as player (player.id)}
        <article class="panel">
          <header>
            <div>
              <small
                >{player.kind === 'AI' ? $t('replay.aiOpponent') : $t('replay.fleetCommand')}</small
              >
              <h2>{player.nickname}</h2>
            </div>
            <span class:winner={replay.result.winnerId === player.id}
              ><Crosshair size={14} />
              {replay.result.winnerId === player.id
                ? $t('replay.winner')
                : $t('replay.fleet')}</span
            >
          </header>
          <GridBoard
            balance={replay.balance.manifest}
            mode="own"
            label={$t('replay.playerFleet', { player: player.nickname })}
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
            <p class="eyebrow">{$t('replay.analysisEyebrow')}</p>
            <h2 id="after-action-title">{$t('replay.analysisTitle')}</h2>
          </div>
          <p>{$t('replay.analysisDescription')}</p>
        </header>

        <div class="analysis-grid">
          {#each analysis.players as playerAnalysis (playerAnalysis.playerId)}
            <article class:winner={playerAnalysis.won} class="analysis-card panel">
              <header>
                <div>
                  <small
                    >{playerAnalysis.won
                      ? $t('replay.victorAnalysis')
                      : $t('replay.fleetAnalysis')}</small
                  >
                  <h3>{playerAnalysis.nickname}</h3>
                </div>
                <strong>{percentage(playerAnalysis.accuracy)}</strong>
              </header>

              <dl class="analysis-stats">
                <div>
                  <dt>{$t('replay.hitsShots')}</dt>
                  <dd>{playerAnalysis.hits} / {playerAnalysis.shots}</dd>
                </div>
                <div>
                  <dt>{$t('replay.maxHitStreak')}</dt>
                  <dd>{playerAnalysis.maxHitStreak}</dd>
                </div>
                <div>
                  <dt>{$t('replay.maxMissStreak')}</dt>
                  <dd>{playerAnalysis.maxMissStreak}</dd>
                </div>
                <div>
                  <dt>{$t('replay.shipsSunk')}</dt>
                  <dd>{playerAnalysis.shipsSunk}</dd>
                </div>
                <div>
                  <dt>{$t('replay.timeouts')}</dt>
                  <dd>{playerAnalysis.timeouts}</dd>
                </div>
              </dl>

              <div
                class="phase-accuracy"
                aria-label={$t('replay.phaseAccuracy', { player: playerAnalysis.nickname })}
              >
                {#each playerAnalysis.phases as phase (phase.id)}
                  <div>
                    <span
                      ><strong>{phase.label}</strong><small
                        >{phase.shots ? percentage(phase.accuracy) : '—'}</small
                      ></span
                    >
                    <progress
                      aria-label={$t('replay.phaseAccuracyLabel', { phase: phase.label })}
                      max="100"
                      value={Math.round(phase.accuracy * 100)}
                    ></progress>
                    <small
                      >{$t('replay.phaseHitsShots', {
                        hits: phase.hits,
                        shots: phase.shots
                      })}</small
                    >
                  </div>
                {/each}
              </div>

              <div class="improvement-tips">
                <h4>{$t('replay.improvementTips')}</h4>
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
              <small>{$t('replay.decisiveMoments')}</small>
              <h3 id="decisive-title">{$t('replay.decisiveTitle')}</h3>
            </div>
            <span>{$t('replay.maxThree')}</span>
          </header>
          <ol>
            {#each analysis.decisiveMoments as moment, index (`${moment.eventIndex}:${moment.title}`)}
              <li>
                <span class:critical={moment.impact === 'CRITICAL'}
                  >{impactLabel(moment.impact)}</span
                >
                <div>
                  <small
                    >{$t('replay.momentMeta', {
                      index: index + 1,
                      turn: moment.turnNumber,
                      player: playerName(moment.playerId)
                    })}</small
                  >
                  <strong>{moment.title}</strong>
                  <p>{moment.detail}</p>
                </div>
                {#if moment.eventIndex !== null}
                  <button
                    class="moment-jump"
                    aria-label={$t('replay.viewMomentLabel', { title: moment.title })}
                    onclick={() => (step = moment.eventIndex! + 1)}
                    >{$t('replay.viewMoment')}</button
                  >
                {/if}
              </li>
            {/each}
          </ol>
        </section>
      </section>
    {/if}

    <ol class="event-log panel" aria-label={$t('replay.eventList')}>
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
