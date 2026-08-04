<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    Activity,
    Check,
    Clock3,
    Crosshair,
    Flag,
    Radio,
    Shield,
    Waves,
    X
  } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import { sounds } from '$lib/sound';
  import { chatMessages } from '$lib/stores';
  import { Button, Modal } from '$lib/ui';
  import {
    FLEET,
    coordinateKey,
    coordinateLabel,
    shipName,
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
  let activeBoard = $state<'target' | 'own'>('target');
  let showSurrender = $state(false);
  let clientNow = $state(Date.now());
  let timerAnnouncement = $state('');
  let announcedTurn = 0;
  let announcedSeconds: number[] = [];
  let clockTimer: ReturnType<typeof setInterval> | null = null;

  let myTurn = $derived(snapshot.currentPlayerId === snapshot.selfPlayerId);
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
      .slice(-5)
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
      remainingSeconds === 0 ||
      attackedKeys.has(coordinateKey(coordinate))
    )
      return;
    selected = coordinate;
    sounds.select();
  }

  function fire() {
    if (!selected || !canFire) return;
    onfire(selected);
    selected = null;
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
        ? '턴 제한 시간이 만료되었습니다. 서버 판정을 기다립니다.'
        : `턴 제한 시간 ${seconds}초 남았습니다.`;
  });

  onMount(() => {
    clockTimer = setInterval(() => (clientNow = Date.now()), 250);
  });

  onDestroy(() => {
    if (clockTimer) clearInterval(clockTimer);
  });
</script>

<section class="battle" aria-labelledby="battle-status">
  <header class:turn-banner--mine={myTurn} class="turn-banner panel">
    <div class="turn-banner__icon">
      {#if myTurn}<Crosshair size={24} />{:else}<Radio size={24} />{/if}
    </div>
    <div>
      <span>TURN {String(snapshot.turnNumber ?? 0).padStart(2, '0')}</span>
      <h1 id="battle-status">
        {disabled
          ? '통신 복구 대기'
          : myTurn
            ? '공격 좌표를 지정하십시오'
            : `${opponent?.nickname ?? '상대'} 지휘관의 응답 대기`}
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
        <small>{timerTone === 'expired' ? 'EXPIRED' : myTurn ? 'TURN LIMIT' : 'ENEMY TIME'}</small>
      </div>
      <div class="elapsed-clock">
        <small>ELAPSED</small><strong>{formatClock(elapsedSeconds)}</strong><span
          >TIMEOUT {me?.consecutiveTimeoutCount ?? 0}/3</span
        >
      </div>
    </div>
    <div class="turn-banner__side">
      <small>CURRENT COMMAND</small><strong class:cyan={myTurn}
        >{myTurn ? 'YOUR TURN' : 'OPPONENT'}</strong
      >
      <button
        class="surrender-trigger"
        type="button"
        disabled={surrenderPending}
        onclick={() => (showSurrender = true)}><Flag size={12} /> 작전 포기</button
      >
    </div>
    <button
      class="mobile-surrender"
      type="button"
      aria-label="작전 포기"
      disabled={surrenderPending}
      onclick={() => (showSurrender = true)}><Flag size={15} /></button
    >
  </header>
  <span class="sr-only" aria-live="assertive">{timerAnnouncement}</span>

  <div class="mobile-tabs" role="tablist" aria-label="전투 보드 선택">
    <button
      class:active={activeBoard === 'target'}
      role="tab"
      aria-selected={activeBoard === 'target'}
      onclick={() => (activeBoard = 'target')}><Crosshair size={15} /> 공격 해역</button
    >
    <button
      class:active={activeBoard === 'own'}
      role="tab"
      aria-selected={activeBoard === 'own'}
      onclick={() => (activeBoard = 'own')}><Shield size={15} /> 아군 해역</button
    >
  </div>

  <div class="battle-grid">
    <div class:hidden-mobile={activeBoard !== 'target'} class="board-panel panel">
      <div class="board-panel__heading">
        <div>
          <span>ENEMY WATERS</span>
          <h2>상대 공격 보드</h2>
        </div>
        <em>{snapshot.targetBoard?.attacks.length ?? 0}회 공격</em>
      </div>
      <GridBoard
        mode="target"
        label="상대 해역 공격 보드"
        targetBoard={snapshot.targetBoard}
        {selected}
        interactive={myTurn}
        {disabled}
        oncell={choose}
      />
      <div class="board-legend">
        <span><i class="legend-miss"></i> 빗나감</span><span><i class="legend-hit"></i> 명중</span
        ><span><i class="legend-sunk"></i> 격침</span>
      </div>
    </div>

    <div class:hidden-mobile={activeBoard !== 'own'} class="board-panel panel">
      <div class="board-panel__heading">
        <div>
          <span>FRIENDLY WATERS</span>
          <h2>아군 함선 보드</h2>
        </div>
        <em>{snapshot.ownBoard?.attacksReceived.length ?? 0}회 피격</em>
      </div>
      <GridBoard
        mode="own"
        label="아군 함선 방어 보드"
        ownBoard={snapshot.ownBoard}
        disabled={true}
      />
      <div class="fleet-health">
        {#each snapshot.ownBoard?.ships ?? [] as ship (ship.kind)}
          <span
            class:sunk={ship.sunk}
            title={`${shipName(ship.kind)} ${ship.hits.length}/${ship.cells.length}`}
            ><i style={`--health:${(ship.cells.length - ship.hits.length) / ship.cells.length}`}
            ></i></span
          >
        {/each}
      </div>
    </div>

    <aside class="fire-control panel">
      <div class="fire-control__title">
        <Crosshair size={17} />
        <div><small>FIRE CONTROL</small><strong>사격 통제</strong></div>
      </div>
      <div class:coordinate-lock--active={selected} class="coordinate-lock">
        <small>SELECTED COORDINATE</small><strong
          >{selected ? coordinateLabel(selected) : '— —'}</strong
        ><span>{selected ? '좌표 잠금 완료' : '공격 보드에서 좌표 선택'}</span>
      </div>
      <button
        class="button button--primary button--wide fire-button"
        disabled={!canFire}
        onclick={fire}
        >{#if pending}<span class="mini-spinner"></span> 판정 대기{:else}<Crosshair size={17} /> 공격
          실행{/if}</button
      >
      {#if selected}<button class="clear-selection" onclick={() => (selected = null)}
          ><X size={13} /> 선택 취소</button
        >{/if}
      <div class="enemy-fleet">
        <small>ENEMY FLEET STATUS</small>{#each FLEET as ship (ship.kind)}<div
            class:sunk={sunkShips.has(ship.kind)}
          >
            <span>{ship.name}</span><span class="mini-ship"
              >{#each Array.from({ length: ship.size }) as _, index (index)}<i></i>{/each}</span
            >{#if sunkShips.has(ship.kind)}<Check size={13} />{/if}
          </div>{/each}
      </div>
      <div class="commanders">
        <div><span class="online-dot"></span><small>YOU</small><strong>{me?.nickname}</strong></div>
        <div>
          <span class:offline-dot={opponent?.connectionState !== 'ONLINE'} class="online-dot"
          ></span><small>OPPONENT</small><strong>{opponent?.nickname}</strong>
        </div>
      </div>
    </aside>

    <section class="battle-log panel" aria-labelledby="battle-log-title">
      <header>
        <div class="battle-log__signal"><Activity size={16} /></div>
        <div>
          <small>TACTICAL EVENT STREAM</small>
          <h2 id="battle-log-title">Battle Log</h2>
        </div>
        <span>LIVE / {String(snapshot.version).padStart(3, '0')}</span>
      </header>
      {#if battleLog.length || systemLog.length}
        <ol>
          {#each systemLog as entry (entry.messageId)}
            <li class="log-system">
              <span>SYS</span>
              <Activity size={14} />
              <strong>SYSTEM EVENT</strong>
              <em>{entry.content}</em>
            </li>
          {/each}
          {#each battleLog as entry (coordinateKey(entry.coordinate))}
            <li class:log-hit={entry.outcome !== 'MISS'} class:log-sunk={entry.outcome === 'SUNK'}>
              <span>{String(entry.sequence).padStart(2, '0')}</span>
              {#if entry.outcome === 'MISS'}<Waves size={14} />{:else}<Crosshair size={14} />{/if}
              <strong>SECTOR {coordinateLabel(entry.coordinate)}</strong>
              <em
                >{entry.outcome === 'MISS'
                  ? '빗나감'
                  : entry.outcome === 'HIT'
                    ? '명중'
                    : `${entry.sunkShip ? shipName(entry.sunkShip) : '함선'} 격침`}</em
              >
            </li>
          {/each}
        </ol>
      {:else}
        <p>사격 명령을 기다리고 있습니다. 첫 공격 이후 전술 이벤트가 기록됩니다.</p>
      {/if}
    </section>
  </div>
</section>

<Modal
  open={showSurrender}
  eyebrow="IRREVERSIBLE COMMAND"
  title="작전을 종료하시겠습니까?"
  description="기권하면 즉시 패배 처리되며 되돌릴 수 없습니다."
  onclose={() => (showSurrender = false)}
>
  <div class="surrender-modal-actions">
    <Button variant="ghost" full onclick={() => (showSurrender = false)}>취소</Button>
    <Button
      variant="danger"
      full
      loading={surrenderPending}
      onclick={() => {
        showSurrender = false;
        onsurrender();
      }}><Flag size={15} /> 기권</Button
    >
  </div>
</Modal>

<style>
  .turn-banner {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 16px;
    margin-bottom: 18px;
    padding: 16px 20px;
    border-radius: 14px;
  }
  .turn-banner--mine {
    border-color: rgba(57, 224, 235, 0.44);
    background: linear-gradient(100deg, rgba(14, 65, 80, 0.96), rgba(5, 25, 37, 0.96));
    box-shadow: 0 12px 50px rgba(22, 199, 217, 0.08);
  }
  .turn-banner__icon {
    display: grid;
    width: 45px;
    height: 45px;
    place-items: center;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    color: var(--cyan-400);
    background: rgba(22, 199, 217, 0.08);
  }
  .turn-banner span,
  .turn-banner__side small {
    color: #6c8999;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.15em;
  }
  .turn-banner h1 {
    margin: 3px 0 0;
    font-size: 17px;
  }
  .turn-banner__side {
    display: grid;
    gap: 3px;
    text-align: right;
  }
  .timer-hud {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border: 1px solid rgba(40, 223, 232, 0.16);
    border-radius: 12px;
    background: rgba(3, 16, 25, 0.55);
    transition: 220ms var(--ease-out);
  }
  .turn-clock {
    position: relative;
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 0 5px;
    min-width: 84px;
    padding-left: 9px;
  }
  .turn-clock::before {
    position: absolute;
    left: 0;
    width: 4px;
    height: 32px;
    content: '';
    border-radius: 3px;
    background: conic-gradient(var(--cyan-300) var(--timer-progress), rgba(40, 223, 232, 0.12) 0);
    box-shadow: 0 0 9px rgba(40, 223, 232, 0.2);
  }
  .turn-clock span {
    display: grid;
    color: var(--cyan-300);
  }
  .turn-clock strong {
    color: var(--cyan-200);
    font-family: var(--font-display);
    font-size: 17px;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.06em;
  }
  .turn-clock small {
    grid-column: 1 / -1;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .elapsed-clock {
    display: grid;
    gap: 1px;
    padding-left: 10px;
    border-left: 1px solid var(--line);
  }
  .elapsed-clock small,
  .elapsed-clock span {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.1em;
  }
  .elapsed-clock strong {
    color: var(--ink-200);
    font-family: var(--font-display);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .timer-hud--warning {
    border-color: rgba(255, 180, 60, 0.28);
  }
  .timer-hud--warning .turn-clock strong,
  .timer-hud--warning .turn-clock span {
    color: var(--amber-500);
  }
  .timer-hud--danger,
  .timer-hud--expired {
    border-color: rgba(255, 83, 100, 0.3);
  }
  .timer-hud--danger .turn-clock strong,
  .timer-hud--danger .turn-clock span,
  .timer-hud--expired .turn-clock strong,
  .timer-hud--expired .turn-clock span {
    color: var(--red-400);
  }
  .timer-hud--danger .turn-clock {
    animation: timer-pulse 1s ease-in-out infinite;
  }
  @keyframes timer-pulse {
    50% {
      opacity: 0.7;
    }
  }
  .turn-banner__side strong {
    font-family: Rajdhani;
    font-size: 13px;
    letter-spacing: 0.11em;
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
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.07em;
    transition: 180ms ease;
  }
  .surrender-trigger:hover {
    color: var(--red-400);
  }
  .surrender-trigger:disabled {
    cursor: wait;
    opacity: 0.4;
  }
  .mobile-surrender {
    display: none;
  }
  .battle-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 265px;
    gap: 16px;
    align-items: start;
  }
  .board-panel {
    padding: 14px;
    border-radius: 15px;
  }
  .board-panel__heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    margin: 2px 4px 12px;
  }
  .board-panel__heading span,
  .fire-control small {
    color: #617e8e;
    font-family: Rajdhani;
    font-size: 8px;
    letter-spacing: 0.15em;
  }
  .board-panel__heading h2 {
    margin: 3px 0 0;
    font-size: 14px;
  }
  .board-panel__heading em {
    color: #7794a4;
    font-size: 9px;
    font-style: normal;
  }
  .board-legend {
    display: flex;
    justify-content: center;
    gap: 15px;
    margin-top: 11px;
    color: #7895a5;
    font-size: 9px;
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
  .legend-miss {
    background: #6bb6d1;
  }
  .legend-hit {
    background: #ff7e46;
    box-shadow: 0 0 5px #ff6a3d;
  }
  .legend-sunk {
    background: #ff5364;
  }
  .fleet-health {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 5px;
    margin-top: 10px;
  }
  .fleet-health span {
    height: 5px;
    overflow: hidden;
    border-radius: 5px;
    background: #1c3645;
  }
  .fleet-health i {
    display: block;
    width: calc(var(--health) * 100%);
    height: 100%;
    background: var(--green-500);
  }
  .fleet-health span.sunk i {
    background: var(--red-500);
  }
  .fire-control {
    padding: 17px;
    border-radius: 15px;
  }
  .battle-log {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: 250px 1fr;
    gap: 20px;
    padding: 14px 18px;
    overflow: hidden;
  }
  .battle-log header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    border-right: 1px solid var(--line);
    padding-right: 18px;
  }
  .battle-log header small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.15em;
  }
  .battle-log header h2 {
    margin: 2px 0 0;
    font-size: 13px;
  }
  .battle-log header > span {
    color: var(--success-400);
    font-family: var(--font-mono);
    font-size: 8px;
  }
  .battle-log__signal {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid rgba(40, 223, 232, 0.24);
    border-radius: 9px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .battle-log ol {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(130px, 1fr);
    gap: 6px;
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
    min-height: 48px;
    padding: 7px 10px;
    border: 1px solid var(--line);
    border-radius: 9px;
    color: var(--cyan-300);
    background: rgba(4, 18, 28, 0.52);
  }
  .battle-log li > span {
    color: var(--ink-600);
    font-family: var(--font-mono);
    font-size: 8px;
  }
  .battle-log li strong {
    color: var(--ink-200);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.05em;
  }
  .battle-log li em {
    grid-column: 2 / -1;
    color: var(--ink-500);
    font-size: 8px;
    font-style: normal;
  }
  .battle-log li.log-hit {
    color: var(--warning-400);
  }
  .battle-log li.log-sunk {
    color: var(--danger-400);
    border-color: rgba(255, 94, 74, 0.24);
  }
  .battle-log li.log-system {
    color: var(--green-400);
    border-color: rgba(79, 226, 173, 0.16);
    background: rgba(79, 226, 173, 0.025);
  }
  .battle-log > p {
    align-self: center;
    margin: 0;
    color: var(--ink-500);
    font-size: 10px;
  }
  .fire-control__title {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--line);
    color: var(--cyan-400);
  }
  .fire-control__title div {
    display: grid;
    gap: 2px;
  }
  .fire-control__title strong {
    color: #d9e9f0;
    font-size: 13px;
  }
  .coordinate-lock {
    display: grid;
    place-items: center;
    min-height: 125px;
    margin: 14px 0;
    padding: 14px;
    border: 1px dashed rgba(87, 154, 179, 0.23);
    border-radius: 10px;
    background: rgba(2, 13, 21, 0.58);
    text-align: center;
  }
  .coordinate-lock--active {
    border-color: rgba(255, 180, 60, 0.52);
    background: rgba(96, 61, 13, 0.1);
  }
  .coordinate-lock strong {
    margin: 6px 0 3px;
    color: #668494;
    font-family: Rajdhani;
    font-size: 32px;
    letter-spacing: 0.18em;
  }
  .coordinate-lock--active strong {
    color: var(--amber-500);
    text-shadow: 0 0 18px rgba(255, 180, 60, 0.25);
  }
  .coordinate-lock span {
    color: #607d8d;
    font-size: 9px;
  }
  .fire-button {
    min-height: 48px;
  }
  .clear-selection {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    width: 100%;
    margin-top: 7px;
    border: 0;
    color: #6f8b9a;
    background: none;
    cursor: pointer;
    font-size: 9px;
  }
  .mini-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(0, 20, 24, 0.25);
    border-top-color: #04161a;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  .enemy-fleet {
    display: grid;
    gap: 7px;
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .enemy-fleet > small {
    margin-bottom: 3px;
  }
  .enemy-fleet > div {
    display: grid;
    grid-template-columns: 1fr auto 14px;
    align-items: center;
    gap: 6px;
    color: #a7bdc8;
    font-size: 10px;
  }
  .enemy-fleet > div.sunk {
    color: #607b8a;
    text-decoration: line-through;
  }
  .enemy-fleet > div.sunk :global(svg) {
    color: var(--red-500);
  }
  .mini-ship {
    display: flex;
    gap: 1px;
  }
  .mini-ship i {
    width: 5px;
    height: 4px;
    background: #4f8295;
  }
  .sunk .mini-ship i {
    background: #6a3743;
  }
  .commanders {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    margin-top: 18px;
    padding-top: 15px;
    border-top: 1px solid var(--line);
  }
  .commanders > div {
    position: relative;
    display: grid;
    gap: 2px;
    padding-left: 10px;
  }
  .commanders small {
    font-size: 7px;
  }
  .commanders strong {
    overflow: hidden;
    font-size: 9px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .online-dot {
    position: absolute;
    top: 4px;
    left: 0;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--green-500);
    box-shadow: 0 0 6px var(--green-500);
  }
  .offline-dot {
    background: var(--red-500);
    box-shadow: 0 0 6px var(--red-500);
  }
  .mobile-tabs {
    display: none;
  }
  .surrender-modal-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 20px;
  }
  @media (max-width: 1120px) {
    .turn-banner {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }
    .turn-banner__side {
      display: none;
    }
    .battle-grid {
      grid-template-columns: 1fr 1fr;
    }
    .fire-control {
      grid-column: 1/-1;
      display: grid;
      grid-template-columns: 180px minmax(200px, 1fr) 220px;
      gap: 15px;
      align-items: center;
    }
    .fire-control__title {
      border: 0;
      padding: 0;
    }
    .coordinate-lock {
      min-height: 90px;
      margin: 0;
    }
    .enemy-fleet {
      grid-column: 1/-1;
      grid-template-columns: repeat(5, 1fr);
      margin: 0;
    }
    .enemy-fleet > small {
      grid-column: 1/-1;
    }
    .commanders {
      display: none;
    }
    .battle-log {
      grid-template-columns: 210px 1fr;
    }
    .clear-selection {
      display: none;
    }
  }
  @media (max-width: 720px) {
    .turn-banner {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }
    .timer-hud {
      grid-column: 1 / -1;
      justify-content: center;
      width: 100%;
    }
    .turn-banner {
      grid-template-columns: auto 1fr auto;
      padding: 13px;
    }
    .turn-banner__side {
      display: none;
    }
    .mobile-surrender {
      display: grid;
      width: 36px;
      height: 36px;
      place-items: center;
      border: 1px solid var(--line);
      border-radius: 9px;
      color: var(--ink-400);
      background: rgba(4, 16, 24, 0.55);
    }
    .turn-banner h1 {
      font-size: 14px;
    }
    .mobile-tabs {
      display: grid;
      grid-template-columns: 1fr 1fr;
      margin-bottom: 10px;
      padding: 3px;
      border: 1px solid var(--line);
      border-radius: 10px;
      background: rgba(4, 16, 25, 0.7);
    }
    .mobile-tabs button {
      display: flex;
      min-height: 40px;
      align-items: center;
      justify-content: center;
      gap: 7px;
      border: 0;
      border-radius: 7px;
      color: #7794a4;
      background: transparent;
      font-size: 11px;
    }
    .mobile-tabs button.active {
      color: var(--cyan-200);
      background: rgba(31, 117, 141, 0.28);
    }
    .battle-grid {
      display: block;
    }
    .board-panel {
      padding: 8px;
    }
    .board-panel.hidden-mobile {
      display: none;
    }
    .fire-control {
      position: sticky;
      z-index: 20;
      bottom: max(8px, env(safe-area-inset-bottom));
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 8px;
      margin-top: 10px;
      padding: 10px;
      background: rgba(6, 23, 34, 0.97);
      backdrop-filter: blur(14px);
    }
    .fire-control__title,
    .enemy-fleet,
    .commanders {
      display: none;
    }
    .coordinate-lock {
      display: flex;
      min-height: 48px;
      align-items: center;
      justify-content: space-between;
      margin: 0;
      padding: 7px 11px;
      text-align: left;
    }
    .coordinate-lock small,
    .coordinate-lock span {
      display: none;
    }
    .coordinate-lock strong {
      margin: 0;
      font-size: 25px;
    }
    .fire-button {
      width: auto;
      min-width: 135px;
    }
    .clear-selection {
      display: none;
    }
    .board-legend {
      margin-bottom: 3px;
    }
    .battle-log {
      display: none;
    }
  }
</style>
