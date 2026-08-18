<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Activity, Check, Clock3, Crosshair, Flag, Radio, Waves, X } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import InputPrompt from './InputPrompt.svelte';
  import { sounds } from '$lib/sound';
  import { chatMessages, gameError, lastAttack } from '$lib/stores';
  import { Button, Modal } from '$lib/ui';
  import { formatNumber, gameModeMessageKey, shipName, shipMessageKey, t } from '$lib/i18n';
  import Vessel from './Vessel.svelte';
  import {
    coordinateKey,
    coordinateLabel,
    fleetForBalance,
    type AttackOutcome,
    type Coordinate,
    type GameSnapshot
  } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    pending?: boolean;
    disabled?: boolean;
    surrenderPending?: boolean;
    onfire: (coordinate: Coordinate) => void;
    onsurrender: () => void;
  }
  let {
    snapshot,
    pending = false,
    disabled = false,
    surrenderPending = false,
    onfire,
    onsurrender
  }: Props = $props();

  let selected = $state<Coordinate | null>(null);
  let showSurrender = $state(false);
  let clientNow = $state(Date.now());
  let timerAnnouncement = $state('');
  let combatEvent = $state<{ outcome: AttackOutcome; coordinate: Coordinate } | null>(null);
  let fireSequence = $state<{
    coordinate: Coordinate;
    stage: 'LOCK' | 'FIRE' | 'IMPACT';
  } | null>(null);
  let turnPulse = $state(false);
  let announcedTurn = 0;
  let announcedSeconds: number[] = [];
  let seenAttackRequest = '';
  let clockTimer: ReturnType<typeof setInterval> | null = null;
  let impactTimer: ReturnType<typeof setTimeout> | null = null;
  let fireTimer: ReturnType<typeof setTimeout> | null = null;
  let turnPulseTimer: ReturnType<typeof setTimeout> | null = null;
  let observedTurnPlayer = '';

  let myTurn = $derived(snapshot.currentPlayerId === snapshot.selfPlayerId);
  let balanceFleet = $derived(fleetForBalance(snapshot.balance.manifest));
  let me = $derived(snapshot.players.find((player) => player.id === snapshot.selfPlayerId));
  let opponent = $derived(snapshot.players.find((player) => player.id !== snapshot.selfPlayerId));
  let serverOffsetMs = $derived(new Date(snapshot.serverTimestamp).getTime() - Date.now());
  let serverNow = $derived(clientNow + serverOffsetMs);
  let remainingSeconds = $derived.by(() => {
    if (!snapshot.turnDeadlineAt) return null;
    return Math.max(
      0,
      Math.ceil((new Date(snapshot.turnDeadlineAt).getTime() - serverNow) / 1_000)
    );
  });
  let elapsedSeconds = $derived(
    snapshot.gameStartedAt
      ? Math.max(0, Math.floor((serverNow - new Date(snapshot.gameStartedAt).getTime()) / 1_000))
      : 0
  );
  let timerProgress = $derived(
    remainingSeconds === null || !snapshot.turnDurationSeconds
      ? 1
      : Math.max(0, Math.min(1, remainingSeconds / snapshot.turnDurationSeconds))
  );
  let timerTone = $derived(
    remainingSeconds === null
      ? 'normal'
      : remainingSeconds === 0
        ? 'expired'
        : remainingSeconds <= 10
          ? 'danger'
          : remainingSeconds <= 20
            ? 'warning'
            : 'normal'
  );
  let attackedKeys = $derived(
    new Set(snapshot.targetBoard?.attacks.map((attack) => coordinateKey(attack.coordinate)) ?? [])
  );
  let canFire = $derived(
    Boolean(
      selected &&
      !fireSequence &&
      myTurn &&
      !pending &&
      !disabled &&
      remainingSeconds !== 0 &&
      !attackedKeys.has(coordinateKey(selected))
    )
  );
  let sunkShips = $derived(
    new Set(
      snapshot.targetBoard?.attacks
        .filter((attack) => attack.sunkShip)
        .map((attack) => attack.sunkShip) ?? []
    )
  );
  let battleLog = $derived(
    (snapshot.targetBoard?.attacks ?? [])
      .map((attack, index) => ({ ...attack, sequence: index + 1 }))
      .slice(-6)
      .reverse()
  );
  let systemLog = $derived(
    $chatMessages
      .filter((message) => message.type === 'SYSTEM')
      .slice(-4)
      .reverse()
  );

  function choose(coordinate: Coordinate) {
    if (
      !myTurn ||
      pending ||
      disabled ||
      fireSequence ||
      remainingSeconds === 0 ||
      attackedKeys.has(coordinateKey(coordinate))
    )
      return;
    selected = coordinate;
    sounds.select();
  }

  function fire() {
    if (!selected || !canFire) return;
    const coordinate = selected;
    selected = null;
    fireSequence = { coordinate, stage: 'LOCK' };
    sounds.targetLock();
    if (fireTimer) clearTimeout(fireTimer);
    onfire(coordinate);
    fireTimer = setTimeout(() => {
      if (fireSequence?.stage === 'LOCK') {
        fireSequence = { coordinate, stage: 'FIRE' };
        sounds.fire();
      }
      fireTimer = null;
    }, 150);
  }

  const formatClock = (seconds: number) => {
    const hours = Math.floor(seconds / 3_600);
    const minutes = Math.floor((seconds % 3_600) / 60);
    const rest = seconds % 60;
    return hours > 0
      ? `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
      : `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
  };

  $effect(() => {
    const attack = $lastAttack;
    if (!attack || attack.requestId === seenAttackRequest) return;
    seenAttackRequest = attack.requestId;
    combatEvent = { outcome: attack.outcome, coordinate: attack.coordinate };
    if (
      fireSequence &&
      coordinateKey(fireSequence.coordinate) === coordinateKey(attack.coordinate)
    ) {
      fireSequence = { ...fireSequence, stage: 'IMPACT' };
      setTimeout(() => {
        fireSequence = null;
      }, 880);
    }
    if (impactTimer) clearTimeout(impactTimer);
    impactTimer = setTimeout(() => (combatEvent = null), 1_100);
  });

  $effect(() => {
    if ($gameError && fireSequence) {
      if (fireTimer) clearTimeout(fireTimer);
      fireTimer = null;
      fireSequence = null;
    }
  });

  $effect(() => {
    const playerId = snapshot.currentPlayerId;
    if (!playerId) return;
    if (observedTurnPlayer && observedTurnPlayer !== playerId) {
      turnPulse = true;
      sounds.turn();
      if (turnPulseTimer) clearTimeout(turnPulseTimer);
      turnPulseTimer = setTimeout(() => (turnPulse = false), 620);
    }
    observedTurnPlayer = playerId;
  });

  $effect(() => {
    const turn = snapshot.turnNumber ?? 0;
    const seconds = remainingSeconds;
    if (turn !== announcedTurn) {
      announcedTurn = turn;
      announcedSeconds = [];
      timerAnnouncement = '';
    }
    if (
      seconds === null ||
      ![10, 5, 3, 2, 1, 0].includes(seconds) ||
      announcedSeconds.includes(seconds)
    ) {
      return;
    }
    announcedSeconds.push(seconds);
    if (myTurn && seconds > 0) sounds.countdown(seconds);
    timerAnnouncement =
      seconds === 0
        ? $t('battle.timerExpiredAnnouncement')
        : $t('battle.timerAnnouncement', { seconds: formatNumber(seconds) });
  });

  onMount(() => {
    clockTimer = setInterval(() => (clientNow = Date.now()), 250);
  });

  onDestroy(() => {
    if (clockTimer) clearInterval(clockTimer);
    if (impactTimer) clearTimeout(impactTimer);
    if (fireTimer) clearTimeout(fireTimer);
    if (turnPulseTimer) clearTimeout(turnPulseTimer);
  });
</script>

<section class="battle" aria-labelledby="battle-status">
  <header
    class:turn-banner--mine={myTurn}
    class:turn-banner--pulse={turnPulse}
    class="turn-banner panel"
  >
    <div class="turn-banner__icon" aria-hidden="true">
      {#if myTurn}<Crosshair size={24} />{:else}<Radio size={24} />{/if}
    </div>
    <div class="turn-banner__copy">
      <span class="turn-banner__eyebrow"
        >{$t('battle.turn', {
          turn: String(snapshot.turnNumber ?? 0).padStart(2, '0'),
          control: myTurn ? $t('battle.tacticalControl') : $t('battle.signalMonitor')
        })}</span
      >
      <h1 id="battle-status">
        {disabled
          ? $t('battle.reconnectWait')
          : myTurn
            ? snapshot.rules.mode === 'SALVO'
              ? $t('battle.salvoPrompt', {
                  shots: formatNumber(snapshot.shotsRemainingInTurn ?? 1)
                })
              : $t('battle.attackPrompt')
            : $t('battle.opponentWait', {
                opponent: opponent?.nickname ?? $t('battle.opponentFallback')
              })}
      </h1>
    </div>
    <div
      class="timer-hud"
      class:timer-hud--warning={timerTone === 'warning'}
      class:timer-hud--danger={timerTone === 'danger'}
      class:timer-hud--expired={timerTone === 'expired'}
    >
      <div class="turn-clock" style={`--timer-progress:${timerProgress * 360}deg`}>
        <span><Clock3 size={13} /></span>
        <strong>{remainingSeconds === null ? '∞' : formatClock(remainingSeconds)}</strong>
        <small
          >{timerTone === 'expired'
            ? $t('battle.expired')
            : myTurn
              ? $t('battle.turnLimit')
              : $t('battle.enemyTime')}</small
        >
      </div>
      <div class="elapsed-clock">
        <small>{$t('battle.elapsed')}</small><strong>{formatClock(elapsedSeconds)}</strong><span
          >{$t('battle.timeoutCounter', {
            current: formatNumber(me?.consecutiveTimeoutCount ?? 0),
            limit: formatNumber(snapshot.balance.manifest.consecutiveTimeoutForfeit)
          })}</span
        >
      </div>
    </div>
    <div class="turn-banner__side">
      <small>{$t('battle.currentCommand')}</small>
      <strong class:cyan={myTurn}>{myTurn ? $t('battle.yourTurn') : $t('battle.opponent')}</strong>
      <button
        class="surrender-trigger"
        type="button"
        disabled={surrenderPending}
        onclick={() => (showSurrender = true)}
      >
        <Flag size={12} />
        {$t('battle.surrender')}
      </button>
    </div>
    <button
      class="mobile-surrender"
      type="button"
      aria-label={$t('battle.surrender')}
      disabled={surrenderPending}
      onclick={() => (showSurrender = true)}
    >
      <Flag size={15} />
    </button>
  </header>
  <span class="sr-only" aria-live="assertive">{timerAnnouncement}</span>

  <div class="combat-strip" aria-label={$t('battle.summary')}>
    <span
      ><i></i>
      {$t('battle.battlespace', {
        size: formatNumber(snapshot.balance.manifest.boardSize)
      })}</span
    >
    <span>{$t('battle.round', { round: String(snapshot.turnNumber ?? 0).padStart(2, '0') })}</span>
    <span
      >{snapshot.rules.mode === 'SALVO'
        ? $t('battle.modeShots', {
            mode: $t(gameModeMessageKey(snapshot.rules.mode)),
            shots: formatNumber(snapshot.shotsRemainingInTurn ?? 1)
          })
        : $t(gameModeMessageKey(snapshot.rules.mode))}</span
    >
    <span>{$t('battle.linkVersion', { version: String(snapshot.version).padStart(3, '0') })}</span>
  </div>
  <InputPrompt context="targeting" />

  {#if combatEvent}
    <div
      class:combat-event--miss={combatEvent.outcome === 'MISS'}
      class:combat-event--hit={combatEvent.outcome !== 'MISS'}
      class:combat-event--sunk={combatEvent.outcome === 'SUNK'}
      class="combat-event"
      role="status"
      aria-live="polite"
    >
      <span class="combat-event__signal"><Crosshair size={15} /></span>
      <div>
        <small
          >{$t('battle.lastAction', {
            coordinate: coordinateLabel(combatEvent.coordinate)
          })}</small
        ><strong
          >{combatEvent.outcome === 'MISS'
            ? $t('battle.noContact')
            : combatEvent.outcome === 'HIT'
              ? $t('battle.hitConfirmed')
              : $t('battle.vesselDestroyed')}</strong
        >
      </div>
    </div>
  {/if}

  {#if fireSequence}
    <div
      class:fire-sequence--fire={fireSequence.stage === 'FIRE'}
      class:fire-sequence--impact={fireSequence.stage === 'IMPACT'}
      class="fire-sequence"
      role="status"
      aria-live="polite"
    >
      <span class="fire-sequence__reticle"><Crosshair size={22} /></span>
      <div>
        <small
          >{$t('battle.sector', { coordinate: coordinateLabel(fireSequence.coordinate) })}</small
        >
        <strong
          >{fireSequence.stage === 'LOCK'
            ? $t('battle.targetLock')
            : fireSequence.stage === 'FIRE'
              ? $t('battle.fireControl')
              : $t('battle.impactConfirmed')}</strong
        >
      </div>
      <span class="fire-sequence__ticks"><i></i><i></i><i></i></span>
    </div>
  {/if}

  <div class="battle-grid">
    <section
      class="board-panel board-panel--friendly panel"
      aria-labelledby="friendly-waters-title"
    >
      <div class="board-panel__heading">
        <div>
          <span>{$t('battle.friendlyWaters')}</span>
          <h2 id="friendly-waters-title">{$t('battle.friendlyBoard')}</h2>
        </div>
        <em
          >{$t('battle.attacksReceived', {
            count: formatNumber(snapshot.ownBoard?.attacksReceived.length ?? 0)
          })}</em
        >
      </div>
      <div class="board-readout">
        <span><i class="signal-dot signal-dot--safe"></i>{$t('battle.ownFleetVisible')}</span
        ><strong>{$t('battle.shieldArray')}</strong>
      </div>
      <GridBoard
        balance={snapshot.balance.manifest}
        mode="own"
        label={$t('battle.friendlyBoardLabel')}
        ownBoard={snapshot.ownBoard}
        disabled={true}
      />
      <div class="board-legend">
        <span><i class="legend-ship"></i> {$t('battle.ship')}</span><span
          ><i class="legend-hit"></i> {$t('battle.damaged')}</span
        ><span><i class="legend-sunk"></i> {$t('battle.sunk')}</span>
      </div>
    </section>

    <section class="board-panel board-panel--hostile panel" aria-labelledby="hostile-waters-title">
      <div class="board-panel__heading">
        <div>
          <span>{$t('battle.hostileWaters')}</span>
          <h2 id="hostile-waters-title">{$t('battle.targetBoard')}</h2>
        </div>
        <em
          >{$t('battle.searched', {
            count: formatNumber(snapshot.targetBoard?.attacks.length ?? 0)
          })}</em
        >
      </div>
      <div class="board-readout">
        <span><i class="signal-dot signal-dot--active"></i>{$t('battle.unknownContacts')}</span
        ><strong>{myTurn ? $t('battle.targetingEnabled') : $t('battle.sonarListening')}</strong>
      </div>
      <GridBoard
        balance={snapshot.balance.manifest}
        mode="target"
        label={$t('battle.targetBoardLabel')}
        targetBoard={snapshot.targetBoard}
        {selected}
        interactive={myTurn && !pending && !fireSequence}
        {disabled}
        oncell={choose}
      />
      <div class="board-legend">
        <span><i class="legend-miss"></i> {$t('board.miss')}</span><span
          ><i class="legend-hit"></i> {$t('board.hit')}</span
        ><span><i class="legend-sunk"></i> {$t('board.sunk')}</span>
      </div>
    </section>

    <aside class="fire-control panel" aria-label={$t('battle.fireControlAria')}>
      <div class="fire-control__title">
        <Crosshair size={17} />
        <div>
          <small>{$t('battle.fireControlCode')}</small><strong>{$t('battle.fireControl')}</strong>
        </div>
      </div>
      <div class:coordinate-lock--active={selected} class="coordinate-lock">
        <small>{$t('battle.targetLock')}</small><strong
          >{selected ? coordinateLabel(selected) : '— —'}</strong
        ><span>{selected ? $t('battle.reticleReady') : $t('battle.selectCoordinate')}</span>
      </div>
      <button
        class="button button--primary button--wide fire-button"
        disabled={!canFire}
        onclick={fire}
      >
        {#if pending}<span class="mini-spinner"></span>
          {$t('battle.awaitingResult')}{:else}<Crosshair size={17} />
          {$t('battle.executeAttack')}{/if}
      </button>
      {#if selected}<button class="clear-selection" onclick={() => (selected = null)}
          ><X size={13} /> {$t('battle.clearSelection')}</button
        >{/if}

      <div class="enemy-fleet">
        <small>{$t('battle.enemyFleetStatus')}</small>
        {#each balanceFleet as ship (ship.kind)}
          <div class:sunk={sunkShips.has(ship.kind)}>
            <span>{$t(shipMessageKey(ship.kind))}</span><span class="mini-ship"
              ><Vessel
                kind={ship.kind}
                state={sunkShips.has(ship.kind) ? 'sunk' : 'docked'}
              /></span
            >{#if sunkShips.has(ship.kind)}<Check size={13} />{:else}<em>{$t('battle.unknown')}</em
              >{/if}
          </div>
        {/each}
      </div>
      <div class="commanders">
        <div>
          <span class="online-dot"></span><small>{$t('battle.you')}</small><strong
            >{me?.nickname}</strong
          >
        </div>
        <div>
          <span class:offline-dot={opponent?.connectionState !== 'ONLINE'} class="online-dot"
          ></span><small>{$t('battle.opponent')}</small><strong>{opponent?.nickname}</strong>
        </div>
      </div>
    </aside>

    <section class="battle-log panel" aria-labelledby="battle-log-title">
      <header>
        <div class="battle-log__signal"><Activity size={16} /></div>
        <div>
          <small>{$t('battle.tacticalEventStream')}</small>
          <h2 id="battle-log-title">{$t('battle.log')}</h2>
        </div>
        <span
          >{$t('battle.liveVersion', { version: String(snapshot.version).padStart(3, '0') })}</span
        >
      </header>
      {#if battleLog.length || systemLog.length}
        <ol>
          {#each systemLog as entry (entry.messageId)}<li class="log-system">
              <span>SYS</span><Activity size={14} /><strong>{$t('battle.systemEvent')}</strong><em
                >{entry.content}</em
              >
            </li>{/each}
          {#each battleLog as entry (coordinateKey(entry.coordinate))}<li
              class:log-hit={entry.outcome !== 'MISS'}
              class:log-sunk={entry.outcome === 'SUNK'}
            >
              <span>{String(entry.sequence).padStart(2, '0')}</span
              >{#if entry.outcome === 'MISS'}<Waves size={14} />{:else}<Crosshair
                  size={14}
                />{/if}<strong
                >{$t('battle.sector', { coordinate: coordinateLabel(entry.coordinate) })}</strong
              ><em
                >{entry.outcome === 'MISS'
                  ? $t('board.miss')
                  : entry.outcome === 'HIT'
                    ? $t('board.hit')
                    : $t('battle.shipSunk', {
                        ship: entry.sunkShip ? shipName(entry.sunkShip) : $t('battle.ship')
                      })}</em
              >
            </li>{/each}
        </ol>
      {:else}<p>{$t('battle.awaitingFire')}</p>{/if}
    </section>
  </div>
</section>

<Modal
  open={showSurrender}
  eyebrow={$t('battle.irreversible')}
  title={$t('battle.surrenderTitle')}
  description={$t('battle.surrenderDescription')}
  onclose={() => (showSurrender = false)}
>
  <div class="surrender-modal-actions">
    <Button variant="ghost" full onclick={() => (showSurrender = false)}
      >{$t('battle.cancel')}</Button
    ><Button
      variant="danger"
      full
      loading={surrenderPending}
      onclick={() => {
        showSurrender = false;
        onsurrender();
      }}><Flag size={15} /> {$t('battle.confirmSurrender')}</Button
    >
  </div>
</Modal>

<style>
  .battle {
    position: relative;
    padding-bottom: 28px;
  }
  .turn-banner {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 16px;
    margin-bottom: 10px;
    padding: 14px 18px;
    border-radius: 10px 3px 10px 3px;
    background: linear-gradient(100deg, rgba(8, 25, 34, 0.94), rgba(3, 13, 20, 0.9));
  }
  .turn-banner--mine {
    border-color: rgba(83, 233, 232, 0.42);
    background: linear-gradient(100deg, rgba(10, 47, 55, 0.96), rgba(4, 20, 28, 0.94));
  }
  .turn-banner__icon {
    display: grid;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid var(--line-active);
    border-radius: 50%;
    color: var(--tactical);
    background: rgba(83, 233, 232, 0.08);
  }
  .turn-banner__eyebrow,
  .turn-banner__side small {
    color: var(--ink-400);
    font: 600 9px var(--font-display);
    letter-spacing: 0.15em;
  }
  .turn-banner h1 {
    margin: 4px 0 0;
    font-size: 17px;
  }
  .turn-banner__side {
    display: grid;
    gap: 3px;
    min-width: 106px;
    text-align: right;
  }
  .turn-banner__side strong {
    color: var(--ink-200);
    font: 700 13px var(--font-display);
    letter-spacing: 0.11em;
  }
  .turn-banner__side strong.cyan {
    color: var(--tactical);
  }
  .timer-hud {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border: 1px solid rgba(83, 233, 232, 0.18);
    background: rgba(2, 13, 19, 0.62);
  }
  .turn-clock {
    position: relative;
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 0 5px;
    min-width: 86px;
    padding-left: 9px;
  }
  .turn-clock::before {
    position: absolute;
    left: 0;
    width: 4px;
    height: 32px;
    content: '';
    border-radius: 3px;
    background: conic-gradient(var(--tactical) var(--timer-progress), rgba(83, 233, 232, 0.1) 0);
  }
  .turn-clock span {
    display: grid;
    color: var(--tactical);
  }
  .turn-clock strong {
    color: #d7ffff;
    font: 700 17px var(--font-display);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.06em;
  }
  .turn-clock small,
  .elapsed-clock small,
  .elapsed-clock span {
    color: var(--ink-500);
    font: 600 7px var(--font-display);
    letter-spacing: 0.1em;
  }
  .turn-clock small {
    grid-column: 1 / -1;
  }
  .elapsed-clock {
    display: grid;
    gap: 1px;
    padding-left: 10px;
    border-left: 1px solid var(--line);
  }
  .elapsed-clock strong {
    color: var(--ink-200);
    font: 600 13px var(--font-display);
    font-variant-numeric: tabular-nums;
  }
  .timer-hud--warning {
    border-color: rgba(237, 181, 82, 0.35);
  }
  .timer-hud--warning .turn-clock strong,
  .timer-hud--warning .turn-clock span {
    color: var(--warning);
  }
  .timer-hud--danger,
  .timer-hud--expired {
    border-color: rgba(238, 86, 103, 0.38);
  }
  .timer-hud--danger .turn-clock strong,
  .timer-hud--danger .turn-clock span,
  .timer-hud--expired .turn-clock strong,
  .timer-hud--expired .turn-clock span {
    color: var(--critical);
  }
  .timer-hud--danger .turn-clock {
    animation: timer-pulse 1s ease-in-out infinite;
  }
  .surrender-trigger {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 5px;
    margin-top: 3px;
    padding: 0;
    border: 0;
    color: var(--ink-500);
    background: transparent;
    cursor: pointer;
    font: 600 8px var(--font-display);
    letter-spacing: 0.07em;
  }
  .surrender-trigger:hover {
    color: var(--critical);
  }
  .surrender-trigger:disabled {
    cursor: wait;
    opacity: 0.4;
  }
  .mobile-surrender {
    display: none;
  }
  .combat-strip {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    margin: 0 4px 10px;
    color: var(--ink-500);
    font: 600 9px var(--font-display);
    letter-spacing: 0.12em;
  }
  .combat-strip span:first-child {
    color: var(--ink-300);
  }
  .combat-strip i,
  .signal-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-right: 6px;
    border-radius: 50%;
    background: var(--safe);
    box-shadow: 0 0 9px currentColor;
  }
  .battle > :global(.input-prompt) {
    margin: 0 4px 10px;
  }
  .signal-dot--active {
    color: var(--tactical);
    background: var(--tactical);
  }
  .signal-dot--safe {
    color: var(--safe);
  }
  .combat-event {
    position: absolute;
    z-index: 5;
    top: 72px;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 260px;
    padding: 9px 12px;
    border: 1px solid rgba(129, 205, 224, 0.32);
    background: rgba(3, 16, 23, 0.96);
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.3);
    transform: translateX(-50%);
    animation: event-in 180ms var(--ease-out) both;
  }
  .combat-event__signal {
    display: grid;
    width: 28px;
    height: 28px;
    place-items: center;
    color: var(--tactical);
    border: 1px solid currentColor;
    border-radius: 50%;
  }
  .combat-event small {
    display: block;
    color: var(--ink-400);
    font: 600 8px var(--font-display);
    letter-spacing: 0.12em;
  }
  .combat-event strong {
    display: block;
    margin-top: 2px;
    color: var(--ink-100);
    font: 700 14px var(--font-display);
    letter-spacing: 0.08em;
  }
  .combat-event--hit .combat-event__signal,
  .combat-event--hit strong {
    color: #ffb76d;
  }
  .combat-event--sunk .combat-event__signal,
  .combat-event--sunk strong {
    color: var(--critical);
  }
  .battle-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 266px;
    grid-template-areas: 'friendly hostile control' 'log log control';
    gap: 10px;
    align-items: start;
  }
  .board-panel {
    min-width: 0;
    padding: 13px 14px 12px;
    border-radius: 10px 3px 10px 3px;
    background: rgba(4, 20, 28, 0.78);
  }
  .board-panel--friendly {
    grid-area: friendly;
    border-top: 1px solid rgba(104, 215, 170, 0.3);
  }
  .board-panel--hostile {
    grid-area: hostile;
    border-top: 1px solid rgba(83, 233, 232, 0.4);
    background: rgba(2, 15, 23, 0.88);
  }
  .board-panel--hostile::after {
    position: absolute;
    inset: 0;
    content: '';
    opacity: 0.3;
    pointer-events: none;
    background: repeating-linear-gradient(
      165deg,
      transparent 0 12px,
      rgba(98, 190, 200, 0.025) 13px 14px
    );
  }
  .board-panel > * {
    position: relative;
    z-index: 1;
  }
  .board-panel__heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    margin: 2px 3px 7px;
  }
  .board-panel__heading span,
  .fire-control small {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.15em;
  }
  .board-panel__heading h2 {
    margin: 3px 0 0;
    font-size: 14px;
  }
  .board-panel__heading em {
    color: var(--ink-400);
    font: 500 9px var(--font-display);
    font-style: normal;
  }
  .board-readout {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin: 0 3px 7px;
    padding-bottom: 7px;
    border-bottom: 1px solid var(--line);
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.08em;
  }
  .board-readout strong {
    color: var(--ink-400);
    font-size: 8px;
  }
  .board-legend {
    display: flex;
    justify-content: center;
    gap: 15px;
    margin-top: 7px;
    color: var(--ink-400);
    font: 500 9px var(--font-display);
  }
  .board-legend span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .board-legend i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .legend-ship {
    background: #75b9bd;
  }
  .legend-miss {
    background: #6bb6d1;
  }
  .legend-hit {
    background: #ff7e46;
    box-shadow: 0 0 5px #ff6a3d;
  }
  .legend-sunk {
    background: var(--critical);
  }
  .fire-control {
    grid-area: control;
    min-width: 0;
    min-height: 100%;
    padding: 16px;
    border-radius: 10px 3px 10px 3px;
    background: linear-gradient(165deg, rgba(8, 31, 39, 0.92), rgba(2, 13, 20, 0.94));
  }
  .fire-control__title {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 13px;
    border-bottom: 1px solid var(--line);
    color: var(--tactical);
  }
  .fire-control__title div {
    display: grid;
    gap: 2px;
  }
  .fire-control__title small {
    color: var(--ink-500);
  }
  .fire-control__title strong {
    color: var(--ink-100);
    font-size: 14px;
  }
  .coordinate-lock {
    display: grid;
    gap: 3px;
    margin: 16px 0 12px;
    padding: 16px 13px 14px;
    border: 1px solid var(--line);
    background: rgba(1, 10, 16, 0.7);
  }
  .coordinate-lock small {
    color: var(--ink-500);
  }
  .coordinate-lock strong {
    color: var(--ink-400);
    font: 700 32px var(--font-display);
    letter-spacing: 0.14em;
  }
  .coordinate-lock span {
    color: var(--ink-500);
    font-size: 9px;
  }
  .coordinate-lock--active {
    border-color: rgba(83, 233, 232, 0.52);
    background: linear-gradient(135deg, rgba(23, 92, 94, 0.24), rgba(2, 13, 19, 0.72));
  }
  .coordinate-lock--active strong {
    color: var(--tactical);
  }
  .coordinate-lock--active::after {
    display: block;
    height: 1px;
    content: '';
    background: var(--tactical);
    box-shadow: 0 0 12px var(--tactical);
  }
  .fire-button {
    min-height: 46px;
  }
  .clear-selection {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    width: 100%;
    margin-top: 7px;
    padding: 5px;
    border: 0;
    color: var(--ink-500);
    background: transparent;
    cursor: pointer;
    font: 600 9px var(--font-display);
  }
  .clear-selection:hover {
    color: var(--ink-200);
  }
  .enemy-fleet {
    display: grid;
    gap: 8px;
    margin-top: 22px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .enemy-fleet > small {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.14em;
  }
  .enemy-fleet > div {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 6px;
    color: var(--ink-300);
    font-size: 10px;
  }
  .enemy-fleet > div.sunk {
    color: var(--critical);
  }
  .enemy-fleet em {
    color: var(--ink-500);
    font: 600 7px var(--font-display);
    font-style: normal;
    letter-spacing: 0.08em;
  }
  .mini-ship {
    display: block;
    width: 72px;
    height: 20px;
  }
  .mini-ship :global(.vessel) {
    width: 100%;
    height: 100%;
  }
  .commanders {
    display: grid;
    gap: 7px;
    margin-top: 24px;
    padding-top: 13px;
    border-top: 1px solid var(--line);
  }
  .commanders div {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0 7px;
    align-items: center;
  }
  .commanders small {
    color: var(--ink-500);
    font: 600 7px var(--font-display);
    letter-spacing: 0.12em;
  }
  .commanders strong {
    grid-column: 2;
    color: var(--ink-200);
    font-size: 10px;
  }
  .online-dot {
    grid-row: 1 / 3;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--safe);
    box-shadow: 0 0 7px var(--safe);
  }
  .offline-dot {
    background: var(--critical);
    box-shadow: 0 0 7px var(--critical);
  }
  .battle-log {
    grid-area: log;
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 16px;
    padding: 12px 16px;
    overflow: hidden;
    border-radius: 10px 3px 10px 3px;
  }
  .battle-log header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 9px;
    padding-right: 16px;
    border-right: 1px solid var(--line);
  }
  .battle-log header small {
    color: var(--ink-500);
    font: 600 8px var(--font-display);
    letter-spacing: 0.14em;
  }
  .battle-log header h2 {
    margin: 2px 0 0;
    font-size: 13px;
  }
  .battle-log header > span {
    color: var(--safe);
    font: 600 8px var(--font-mono);
  }
  .battle-log__signal {
    display: grid;
    width: 31px;
    height: 31px;
    place-items: center;
    border: 1px solid rgba(83, 233, 232, 0.28);
    color: var(--tactical);
  }
  .battle-log ol {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(132px, 1fr);
    gap: 5px;
    margin: 0;
    padding: 0;
    overflow-x: auto;
    list-style: none;
  }
  .battle-log li {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: center;
    gap: 6px;
    min-height: 46px;
    padding: 7px 8px;
    border: 1px solid var(--line);
    background: rgba(2, 12, 18, 0.58);
    color: var(--ink-400);
    font-size: 9px;
  }
  .battle-log li > span {
    color: var(--ink-500);
    font: 600 8px var(--font-mono);
  }
  .battle-log li strong {
    color: var(--ink-300);
    font: 600 9px var(--font-display);
  }
  .battle-log li em {
    grid-column: 3;
    color: var(--ink-500);
    font-size: 9px;
    font-style: normal;
  }
  .battle-log li.log-hit {
    border-color: rgba(255, 151, 91, 0.22);
  }
  .battle-log li.log-hit :global(svg) {
    color: #ff965b;
  }
  .battle-log li.log-sunk {
    border-color: rgba(238, 86, 103, 0.4);
  }
  .battle-log li.log-sunk :global(svg),
  .battle-log li.log-sunk em {
    color: var(--critical);
  }
  .battle-log .log-system {
    grid-template-columns: auto auto 1fr;
  }
  .battle-log > p {
    align-self: center;
    margin: 0;
    color: var(--ink-500);
    font-size: 10px;
  }
  .turn-banner--pulse {
    animation: command-shift 620ms var(--ease-out) both;
  }
  .fire-sequence {
    position: relative;
    z-index: 8;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    width: min(390px, 100%);
    margin: -4px auto 8px;
    padding: 10px 13px;
    border: 1px solid rgba(83, 233, 232, 0.52);
    border-left: 3px solid var(--tactical);
    background: linear-gradient(90deg, rgba(7, 42, 49, 0.96), rgba(2, 16, 23, 0.92));
    box-shadow:
      0 12px 34px rgba(0, 0, 0, 0.28),
      0 0 22px rgba(83, 233, 232, 0.08);
    animation: fire-sequence-in 150ms var(--ease-out) both;
  }
  .fire-sequence__reticle {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid currentColor;
    border-radius: 50%;
    color: var(--tactical);
    animation: fire-reticle 620ms ease-in-out infinite;
  }
  .fire-sequence div {
    display: grid;
    gap: 2px;
  }
  .fire-sequence small {
    color: var(--ink-400);
    font: 600 8px var(--font-display);
    letter-spacing: 0.16em;
  }
  .fire-sequence strong {
    color: var(--ink-50);
    font: 700 16px var(--font-display);
    letter-spacing: 0.12em;
  }
  .fire-sequence__ticks {
    display: flex;
    gap: 4px;
  }
  .fire-sequence__ticks i {
    display: block;
    width: 4px;
    height: 18px;
    background: var(--line-active);
    transform: skew(-16deg);
  }
  .fire-sequence--fire {
    border-color: rgba(255, 150, 91, 0.62);
    border-left-color: var(--orange-400);
    background: linear-gradient(90deg, rgba(78, 41, 29, 0.96), rgba(22, 17, 19, 0.94));
  }
  .fire-sequence--fire .fire-sequence__reticle {
    color: var(--orange-400);
  }
  .fire-sequence--impact {
    border-color: rgba(255, 114, 128, 0.68);
    border-left-color: var(--critical);
    background: linear-gradient(90deg, rgba(92, 27, 39, 0.96), rgba(24, 13, 20, 0.94));
  }
  .fire-sequence--impact .fire-sequence__reticle {
    color: var(--critical);
    animation: fire-impact 420ms ease-out both;
  }
  @keyframes command-shift {
    0% {
      box-shadow: 0 0 0 rgba(83, 233, 232, 0);
    }
    35% {
      box-shadow: 0 0 32px rgba(83, 233, 232, 0.22);
    }
  }
  @keyframes fire-sequence-in {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @keyframes fire-reticle {
    50% {
      transform: scale(1.12);
      opacity: 0.58;
    }
  }
  @keyframes fire-impact {
    50% {
      transform: scale(1.28);
      filter: drop-shadow(0 0 10px currentColor);
    }
  }
  @keyframes timer-pulse {
    50% {
      opacity: 0.68;
    }
  }
  @keyframes event-in {
    from {
      opacity: 0;
      transform: translate(-50%, -8px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
  @media (max-width: 1120px) {
    .battle-grid {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
      grid-template-areas: 'friendly hostile' 'control control' 'log log';
    }
    .fire-control {
      min-height: auto;
      display: grid;
      grid-template-columns: 1.1fr 1fr 1fr;
      gap: 0 15px;
      align-items: start;
    }
    .fire-control__title {
      grid-column: 1 / -1;
    }
    .coordinate-lock {
      margin: 12px 0 0;
    }
    .enemy-fleet,
    .commanders {
      margin-top: 12px;
    }
  }
  @media (max-width: 760px) {
    .turn-banner {
      grid-template-columns: auto 1fr auto;
    }
    .turn-banner__side {
      display: none;
    }
    .mobile-surrender {
      display: grid;
      width: 32px;
      height: 32px;
      place-items: center;
      border: 1px solid var(--line);
      color: var(--ink-400);
      background: transparent;
    }
    .timer-hud {
      grid-column: 2 / -1;
      justify-self: start;
    }
    .combat-strip {
      flex-wrap: wrap;
    }
    .battle-grid {
      display: block;
    }
    .board-panel,
    .fire-control,
    .battle-log {
      margin-bottom: 10px;
    }
    .fire-control {
      display: block;
    }
    .battle-log {
      display: block;
    }
    .battle-log header {
      margin-bottom: 10px;
      padding: 0 0 9px;
      border-right: 0;
      border-bottom: 1px solid var(--line);
    }
    .battle-log ol {
      display: flex;
    }
    .battle-log li {
      min-width: 148px;
    }
    .combat-event {
      top: 116px;
      min-width: min(280px, calc(100vw - 36px));
    }
  }
  @media (max-width: 620px) {
    .turn-banner {
      gap: 10px;
      padding: 12px;
    }
    .turn-banner__icon {
      width: 36px;
      height: 36px;
    }
    .turn-banner h1 {
      font-size: 14px;
    }
    .turn-banner__eyebrow {
      font-size: 8px;
    }
    .timer-hud {
      grid-column: 1 / -1;
      width: 100%;
      justify-content: space-between;
    }
    .board-panel {
      padding: 9px;
    }
    .board-readout {
      font-size: 7px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .combat-event {
      animation: none;
    }
  }
</style>
