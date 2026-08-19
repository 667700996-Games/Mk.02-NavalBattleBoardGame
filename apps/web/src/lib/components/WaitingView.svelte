<script lang="ts">
  import {
    Check,
    CircleDot,
    Copy,
    LogOut,
    Radio,
    Rocket,
    Share2,
    ShieldCheck,
    UserRound,
    Wifi,
    WifiOff
  } from '@lucide/svelte';
  import { Badge, Button } from '$lib/ui';
  import { sounds } from '$lib/sound';
  import { formatDateTime, formatNumber, t, type MessageKey } from '$lib/i18n';
  import type { GameSnapshot, PlayerPublic } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    inviteUrl: string;
    online: boolean;
    readyPending: boolean;
    startPending: boolean;
    onready: () => void;
    onunready: () => void;
    onstart: () => void;
    onleave: () => void;
  }

  let {
    snapshot,
    inviteUrl,
    online,
    readyPending,
    startPending,
    onready,
    onunready,
    onstart,
    onleave
  }: Props = $props();
  let copied = $state(false);

  let selfPlayer = $derived(snapshot.players.find((player) => player.id === snapshot.selfPlayerId));
  let hostPlayer = $derived(snapshot.players.find((player) => player.id === snapshot.hostPlayerId));
  let guestPlayer = $derived(snapshot.players.find((player) => player.role === 'GUEST'));
  let isHost = $derived(snapshot.selfPlayerId === snapshot.hostPlayerId);
  let observedPlayerCount = $state(0);
  let allReady = $derived(
    snapshot.players.length === 2 &&
      snapshot.players.every((player) => player.readyState === 'READY')
  );
  let readyPlayerCount = $derived(
    snapshot.players.filter((player) => player.readyState === 'READY').length
  );
  let startDisabledReason = $derived.by<MessageKey | null>(() => {
    if (snapshot.gameId || snapshot.roomState === 'PLACEMENT') return 'waiting.alreadyStarted';
    if (snapshot.players.length !== 2) return 'waiting.opponentMissing';
    if (snapshot.players.some((player) => player.connectionState !== 'ONLINE')) {
      return 'waiting.playerOffline';
    }
    if (!allReady) return 'waiting.opponentNotReady';
    if (snapshot.roomState !== 'READY_TO_START') return 'waiting.syncing';
    if (!online) return 'waiting.connectionRequired';
    return null;
  });
  let headingKey = $derived<MessageKey>(
    snapshot.roomState === 'WAITING_FOR_OPPONENT'
      ? 'waiting.headingOpponent'
      : snapshot.roomState === 'READY_TO_START'
        ? isHost
          ? 'waiting.headingHostReady'
          : 'waiting.headingGuestReady'
        : 'waiting.headingReadiness'
  );

  $effect(() => {
    const playerCount = snapshot.players.length;
    if (observedPlayerCount > 0 && playerCount > observedPlayerCount) sounds.connected();
    observedPlayerCount = playerCount;
  });

  async function copyInvite() {
    try {
      await navigator.clipboard.writeText(inviteUrl);
      copied = true;
      setTimeout(() => (copied = false), 2_000);
    } catch {
      copied = false;
    }
  }

  async function shareInvite() {
    if (navigator.share) {
      await navigator.share({
        title: `${snapshot.room.name} · Mk.01`,
        text: $t('waiting.shareText'),
        url: inviteUrl
      });
    } else await copyInvite();
  }

  function readyTime(player: PlayerPublic): string {
    if (!player.readyAt) return $t('waiting.approvalPending');
    return formatDateTime(player.readyAt, {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    });
  }
</script>

<section class="waiting panel" aria-labelledby="waiting-title">
  <header class="waiting-heading">
    <div
      class:ready={snapshot.roomState === 'READY_TO_START'}
      class="waiting__radar"
      aria-hidden="true"
    >
      <div class="waiting__sweep"></div>
      {#if snapshot.roomState === 'READY_TO_START'}<ShieldCheck size={27} />{:else}<Radio
          size={25}
        />{/if}
    </div>
    <div>
      <p class="eyebrow">{$t('waiting.eyebrow')}</p>
      <h1 id="waiting-title">{$t(headingKey)}</h1>
      <p class="muted">{$t('waiting.description')}</p>
    </div>
    <Badge tone={snapshot.roomState === 'READY_TO_START' ? 'success' : 'cyan'} pulse>
      {snapshot.roomState}
    </Badge>
  </header>

  <div class="room-identity">
    <div><small>{$t('waiting.operation')}</small><strong>{snapshot.room.name}</strong></div>
    <div>
      <small>{$t('waiting.secureCode')}</small><strong class="code">{snapshot.room.code}</strong>
    </div>
    <div><small>{$t('waiting.roomVersion')}</small><strong>V{snapshot.roomVersion}</strong></div>
  </div>

  <div class="invite-bar">
    <span>{inviteUrl}</span>
    <button
      class="icon-button"
      onclick={copyInvite}
      aria-label={$t('waiting.copyInvite')}
      title={$t('waiting.copyLink')}
    >
      {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}
    </button>
    <button
      class="icon-button"
      onclick={shareInvite}
      aria-label={$t('waiting.shareInvite')}
      title={$t('waiting.share')}
    >
      <Share2 size={16} />
    </button>
  </div>

  <div class:armed={allReady} class="stage-readiness" aria-live="polite">
    <span class="stage-readiness__signal"><i></i> {$t('waiting.fleetLinkStatus')}</span>
    <strong
      >{allReady
        ? $t('waiting.allReady')
        : $t('waiting.readyCount', {
            ready: formatNumber(readyPlayerCount),
            total: formatNumber(2)
          })}</strong
    >
    <small>
      {allReady ? $t('waiting.hostCanStart') : $t('waiting.allMustConfirm')}
    </small>
  </div>

  <div class="room-command-grid">
    <div class="player-slots" aria-label={$t('waiting.readinessLabel')}>
      {#each [hostPlayer, guestPlayer] as player, index (player?.role ?? 'EMPTY_GUEST')}
        {#if index === 1}
          <div class:active={allReady} class="tactical-link" aria-hidden="true">
            <span class="tactical-link__line"></span>
            <strong>VS</strong>
            <small>{allReady ? $t('waiting.linkEstablished') : $t('waiting.awaitingLink')}</small>
            <span class="tactical-link__line"></span>
          </div>
        {/if}
        {#if player}
          <article
            class:player-slot--ready={player.readyState === 'READY'}
            class:player-slot--offline={player.connectionState !== 'ONLINE'}
            class="player-slot"
          >
            <span class="player-avatar"><UserRound size={21} /></span>
            <div class="player-identity">
              <small
                >{player.role === 'HOST'
                  ? $t('waiting.commandAuthority')
                  : $t('waiting.secondCommander')}</small
              >
              <strong>{player.nickname}</strong>
              <div class="player-badges">
                <Badge tone={player.role === 'HOST' ? 'cyan' : 'neutral'}>
                  {player.role === 'HOST' ? $t('waiting.host') : $t('waiting.guest')}
                </Badge>
                <span class:offline={player.connectionState !== 'ONLINE'} class="connection-state">
                  {#if player.connectionState === 'ONLINE'}<Wifi size={11} />
                    {$t('waiting.connected')}{:else}<WifiOff size={11} />
                    {player.connectionState === 'RECONNECTING'
                      ? $t('waiting.reconnecting')
                      : $t('waiting.offline')}{/if}
                </span>
              </div>
            </div>
            <div class:ready={player.readyState === 'READY'} class="ready-state">
              {#if player.readyState === 'READY'}<Check size={17} />{:else}<CircleDot
                  size={15}
                />{/if}
              <span>
                <strong
                  >{player.readyState === 'READY'
                    ? $t('waiting.ready')
                    : $t('waiting.notReady')}</strong
                >
                <small>{readyTime(player)}</small>
              </span>
            </div>
          </article>
        {:else}
          <article class="player-slot player-slot--pending">
            <span class="player-avatar"><UserRound size={21} /></span>
            <div class="player-identity">
              <small>{$t('waiting.secondCommander')}</small>
              <strong>{$t('waiting.awaitingOpponent')}</strong>
              <span class="scanning"><i></i> {$t('waiting.scanningInvite')}</span>
            </div>
          </article>
        {/if}
      {/each}
    </div>

    <aside class="command-actions" aria-label={$t('waiting.controls')}>
      <header>
        <Radio size={14} /><span>{$t('waiting.authorizationControl')}</span><em
          >{$t('waiting.serverVerified')}</em
        >
      </header>
      <div class="action-body">
        <div class="self-readiness">
          <small>{$t('waiting.yourReadiness')}</small>
          <strong
            >{selfPlayer?.readyState === 'READY'
              ? $t('waiting.selfReady')
              : $t('waiting.selfNeedsApproval')}</strong
          >
          <p>{$t('waiting.readinessPersists')}</p>
        </div>
        <div class="ready-control">
          <small>{$t('waiting.yourReadiness')}</small>
          <Button
            variant={selfPlayer?.readyState === 'READY' ? 'outline' : 'success'}
            size="lg"
            full
            loading={readyPending}
            disabled={!online || startPending}
            onclick={() => {
              sounds.ready();
              if (selfPlayer?.readyState === 'READY') onunready();
              else onready();
            }}
          >
            {#if selfPlayer?.readyState === 'READY'}{$t('waiting.cancelReady')}{:else}<Check
                size={17}
              />
              {$t('waiting.confirmReady')}{/if}
          </Button>
        </div>

        <div class="host-control">
          <div class="start-divider"><span>{$t('waiting.hostAuthorization')}</span></div>
          {#if isHost}
            <Button
              variant="primary"
              size="lg"
              full
              loading={startPending}
              disabled={!snapshot.canStartGame || !online || readyPending}
              onclick={() => {
                sounds.start();
                onstart();
              }}
            >
              <Rocket size={17} />
              {$t('waiting.start')}
            </Button>
            <p class:available={!startDisabledReason} class="start-reason">
              {startDisabledReason ? $t(startDisabledReason) : $t('waiting.canStart')}
            </p>
          {:else}
            <div class:ready={allReady} class="guest-guidance">
              <ShieldCheck size={20} />
              <span>
                <strong
                  >{allReady ? $t('waiting.awaitingHost') : $t('waiting.awaitingEveryone')}</strong
                >
                <small>
                  {allReady ? $t('waiting.onlyHostStarts') : $t('waiting.twoPlayersRequired')}
                </small>
              </span>
            </div>
          {/if}
        </div>
      </div>
    </aside>
  </div>

  <div class="system-log" aria-label={$t('waiting.statusLog')}>
    <span>SYS</span>
    <p>{$t(headingKey)}</p>
    <time>V{snapshot.roomVersion}</time>
  </div>

  <Button variant="secondary" size="sm" class="leave-button" onclick={onleave}>
    <LogOut size={15} />
    {$t('waiting.leave')}
  </Button>
</section>

<style>
  .waiting {
    width: min(1040px, 100%);
    margin: 0 auto;
    padding: 34px;
    background:
      radial-gradient(circle at 15% 0%, rgba(37, 197, 215, 0.08), transparent 34%),
      rgba(4, 17, 26, 0.84);
  }
  .waiting-heading {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 20px;
    align-items: center;
  }
  .waiting-heading h1 {
    margin: 3px 0 6px;
    font-family: Rajdhani, sans-serif;
    font-size: clamp(24px, 3vw, 32px);
  }
  .waiting-heading .muted {
    max-width: 680px;
    margin: 0;
    line-height: 1.65;
  }
  .waiting__radar {
    position: relative;
    display: grid;
    width: 72px;
    height: 72px;
    place-items: center;
    overflow: hidden;
    border: 1px solid rgba(57, 224, 235, 0.3);
    border-radius: 50%;
    color: var(--cyan-400);
    background: radial-gradient(circle, rgba(33, 158, 178, 0.18), transparent 66%);
  }
  .waiting__radar.ready {
    color: var(--green-400);
    border-color: rgba(66, 211, 146, 0.38);
    box-shadow: 0 0 32px rgba(66, 211, 146, 0.1);
  }
  .waiting__radar::before,
  .waiting__radar::after {
    position: absolute;
    inset: 50% 0 auto;
    height: 1px;
    content: '';
    background: currentColor;
    opacity: 0.13;
  }
  .waiting__radar::after {
    transform: rotate(90deg);
  }
  .waiting__radar :global(svg) {
    position: relative;
    z-index: 2;
  }
  .waiting__sweep {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(57, 224, 235, 0.42), transparent 40deg);
    animation: radar 2.8s linear infinite;
  }
  .room-identity {
    display: grid;
    grid-template-columns: 1.5fr 1fr 0.6fr;
    margin: 26px 0 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(3, 15, 24, 0.58);
  }
  .room-identity > div {
    display: grid;
    gap: 4px;
    padding: 14px 16px;
    border-right: 1px solid var(--line);
  }
  .room-identity > div:last-child {
    border: 0;
  }
  .room-identity small,
  .self-readiness small {
    color: #638091;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.16em;
  }
  .room-identity strong {
    font-size: 13px;
  }
  .room-identity .code {
    color: var(--cyan-200);
    font-family: Rajdhani;
    font-size: 20px;
    letter-spacing: 0.18em;
  }
  .invite-bar {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 7px;
    align-items: center;
    padding: 7px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(2, 11, 18, 0.74);
  }
  .invite-bar > span {
    overflow: hidden;
    padding-left: 10px;
    color: #7894a3;
    font-size: 11px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .room-command-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(310px, 0.85fr);
    gap: 14px;
    margin-top: 22px;
  }
  .player-slots {
    display: grid;
    gap: 12px;
  }
  .player-slot {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 13px;
    min-height: 110px;
    padding: 16px;
    border: 1px solid var(--line);
    border-radius: 12px;
    text-align: left;
    background: linear-gradient(115deg, rgba(8, 28, 40, 0.74), rgba(4, 18, 27, 0.45));
    transition:
      border-color 180ms ease,
      transform 180ms ease;
  }
  .player-slot--ready {
    border-color: rgba(66, 211, 146, 0.35);
    box-shadow: inset 3px 0 rgba(66, 211, 146, 0.45);
  }
  .player-slot--offline {
    border-color: rgba(246, 173, 53, 0.35);
  }
  .player-avatar {
    display: grid;
    width: 46px;
    height: 46px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 50%;
    color: #8babb9;
    background: rgba(4, 16, 25, 0.72);
  }
  .player-identity {
    display: grid;
    gap: 4px;
  }
  .player-identity > small {
    color: #617e8e;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.14em;
  }
  .player-identity > strong {
    font-size: 14px;
  }
  .player-badges {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 3px;
  }
  .connection-state {
    display: flex;
    gap: 4px;
    align-items: center;
    color: var(--green-500);
    font-size: 9px;
  }
  .connection-state.offline {
    color: var(--amber-500);
  }
  .ready-state {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 112px;
    padding: 10px;
    border: 1px solid rgba(130, 174, 191, 0.14);
    border-radius: 9px;
    color: #7894a3;
    background: rgba(2, 11, 18, 0.5);
  }
  .ready-state.ready {
    color: var(--green-400);
    border-color: rgba(66, 211, 146, 0.26);
  }
  .ready-state > span {
    display: grid;
    gap: 2px;
  }
  .ready-state strong {
    font-size: 10px;
  }
  .ready-state small {
    color: #638091;
    font-size: 8px;
  }
  .player-slot--pending {
    grid-template-columns: auto 1fr;
    border-style: dashed;
    opacity: 0.74;
  }
  .scanning {
    display: flex;
    gap: 6px;
    align-items: center;
    color: #7894a3;
    font-size: 9px;
  }
  .scanning i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--amber-500);
    animation: pulse 1.2s infinite;
  }
  .command-actions {
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(2, 13, 21, 0.64);
  }
  .command-actions > header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 11px 13px;
    border-bottom: 1px solid var(--line);
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
  }
  .command-actions header em {
    color: var(--green-400);
    font-size: 7px;
    font-style: normal;
  }
  .action-body {
    display: grid;
    gap: 12px;
    padding: 18px;
  }
  .self-readiness {
    display: grid;
    gap: 4px;
  }
  .self-readiness strong {
    font-size: 15px;
  }
  .self-readiness p {
    margin: 2px 0 5px;
    color: #7894a3;
    font-size: 10px;
    line-height: 1.55;
  }
  .start-divider {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 3px 0;
    color: #587484;
    font-family: Rajdhani;
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .start-divider::before,
  .start-divider::after {
    flex: 1;
    height: 1px;
    content: '';
    background: var(--line);
  }
  .start-reason {
    min-height: 28px;
    margin: -3px 0 0;
    color: var(--amber-500);
    font-size: 9px;
    line-height: 1.5;
    text-align: center;
  }
  .start-reason.available {
    color: var(--green-400);
  }
  .guest-guidance {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 13px;
    border: 1px solid var(--line);
    border-radius: 9px;
    color: var(--amber-500);
    background: rgba(246, 173, 53, 0.05);
  }
  .guest-guidance.ready {
    color: var(--green-400);
    border-color: rgba(66, 211, 146, 0.25);
    background: rgba(66, 211, 146, 0.05);
  }
  .guest-guidance span {
    display: grid;
    gap: 3px;
  }
  .guest-guidance strong {
    font-size: 10px;
  }
  .guest-guidance small {
    color: #7894a3;
    font-size: 9px;
    line-height: 1.5;
  }
  .system-log {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 10px;
    align-items: center;
    margin-top: 14px;
    padding: 10px 13px;
    border: 1px solid rgba(76, 169, 189, 0.12);
    border-radius: 8px;
    color: #7894a3;
    background: rgba(2, 11, 18, 0.48);
    font-size: 9px;
  }
  .system-log span {
    color: var(--cyan-400);
    font-family: Rajdhani;
    letter-spacing: 0.12em;
  }
  .system-log p {
    margin: 0;
  }
  .system-log time {
    color: #587484;
    font-family: var(--font-mono);
  }
  :global(.leave-button) {
    display: flex;
    margin: 20px auto 0;
    color: #7893a2;
  }
  @media (max-width: 760px) {
    .waiting {
      padding: 24px 16px;
    }
    .waiting-heading {
      grid-template-columns: auto 1fr;
    }
    .waiting-heading > :global(.ui-badge) {
      grid-column: 2;
      justify-self: start;
    }
    .room-command-grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 560px) {
    .waiting-heading {
      grid-template-columns: 1fr;
      text-align: center;
    }
    .waiting__radar {
      margin: 0 auto;
    }
    .waiting-heading > :global(.ui-badge) {
      grid-column: 1;
      justify-self: center;
    }
    .room-identity {
      grid-template-columns: 1fr;
    }
    .room-identity > div {
      border-right: 0;
      border-bottom: 1px solid var(--line);
    }
    .player-slot {
      grid-template-columns: auto 1fr;
    }
    .ready-state {
      grid-column: 1 / -1;
      width: 100%;
    }
  }

  .waiting {
    position: relative;
    width: min(1380px, 100%);
    padding: clamp(22px, 2.2vw, 34px);
    overflow: hidden;
    border-radius: 10px 3px 10px 3px;
    border-color: rgba(104, 195, 204, 0.22);
    background: linear-gradient(145deg, rgba(7, 27, 36, 0.94), rgba(2, 12, 19, 0.96));
  }
  .waiting::before {
    position: absolute;
    inset: 0;
    content: '';
    opacity: 0.22;
    pointer-events: none;
    background: repeating-linear-gradient(
      150deg,
      transparent 0 12px,
      rgba(95, 185, 192, 0.025) 13px 14px
    );
  }
  .waiting > * {
    position: relative;
    z-index: 1;
  }
  .waiting-heading {
    padding-bottom: 20px;
    border-bottom: 1px solid var(--line);
  }
  .waiting-heading h1 {
    font-family: var(--font-display);
    font-size: clamp(27px, 3.2vw, 38px);
    letter-spacing: 0.02em;
  }
  .waiting-heading .muted {
    color: var(--ink-400);
  }
  .waiting__radar {
    width: 78px;
    height: 78px;
    border-radius: 8px 3px 8px 3px;
    background: linear-gradient(145deg, rgba(83, 233, 232, 0.12), rgba(2, 14, 20, 0.85));
  }
  .room-identity {
    margin: 18px 0 10px;
    border: 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    background: rgba(1, 9, 15, 0.38);
  }
  .room-identity > div {
    padding: 12px 14px;
  }
  .room-identity strong {
    font-family: var(--font-display);
    letter-spacing: 0.04em;
  }
  .invite-bar {
    border-radius: 4px;
    border-color: var(--line-subtle);
    background: rgba(0, 8, 13, 0.62);
  }
  .stage-readiness {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    margin-top: 18px;
    padding: 11px 14px;
    border: 1px solid rgba(111, 181, 202, 0.17);
    border-left: 2px solid var(--cyan-400);
    color: var(--ink-400);
    background: linear-gradient(90deg, rgba(43, 174, 187, 0.08), rgba(2, 13, 20, 0.2));
  }
  .stage-readiness.armed {
    border-color: rgba(104, 215, 170, 0.34);
    border-left-color: var(--safe);
    background: linear-gradient(90deg, rgba(104, 215, 170, 0.12), rgba(2, 18, 23, 0.2));
  }
  .stage-readiness__signal {
    display: inline-flex;
    gap: 7px;
    align-items: center;
    color: var(--cyan-300);
    font: 700 8px var(--font-display);
    letter-spacing: 0.15em;
  }
  .stage-readiness__signal i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 8px currentColor;
    animation: pulse 1.8s ease-in-out infinite;
  }
  .stage-readiness strong {
    color: var(--ink-100);
    font: 700 15px var(--font-display);
    letter-spacing: 0.08em;
  }
  .stage-readiness small {
    color: var(--ink-500);
    font: 600 7px var(--font-display);
    letter-spacing: 0.12em;
    text-align: right;
  }
  .stage-readiness.armed .stage-readiness__signal,
  .stage-readiness.armed strong {
    color: var(--safe);
  }
  .room-command-grid {
    grid-template-columns: 1fr;
    gap: 14px;
    margin-top: 14px;
  }
  .player-slots {
    position: relative;
    grid-template-columns: minmax(0, 1fr) minmax(112px, 0.2fr) minmax(0, 1fr);
    gap: clamp(14px, 2vw, 30px);
    align-items: stretch;
  }
  .player-slots::before {
    display: none;
  }
  .tactical-link {
    display: grid;
    min-height: 218px;
    align-content: center;
    justify-items: center;
    gap: 9px;
    color: var(--ink-500);
    font-family: var(--font-display);
    text-align: center;
  }
  .tactical-link strong {
    display: grid;
    width: 42px;
    height: 42px;
    place-items: center;
    border: 1px solid rgba(237, 181, 82, 0.42);
    border-radius: 50%;
    color: var(--warning);
    background: rgba(6, 25, 35, 0.96);
    font-size: 13px;
    letter-spacing: 0.1em;
    box-shadow: 0 0 22px rgba(237, 181, 82, 0.08);
  }
  .tactical-link small {
    color: var(--ink-500);
    font-size: 7px;
    letter-spacing: 0.14em;
  }
  .tactical-link__line {
    display: block;
    width: 1px;
    height: 28px;
    background: linear-gradient(var(--line-active), transparent);
  }
  .tactical-link__line:last-child {
    background: linear-gradient(transparent, var(--line-active));
  }
  .tactical-link.active {
    color: var(--safe);
  }
  .tactical-link.active strong {
    border-color: rgba(104, 215, 170, 0.55);
    color: var(--safe);
    box-shadow: 0 0 25px rgba(104, 215, 170, 0.16);
  }
  .tactical-link.active .tactical-link__line {
    background: linear-gradient(var(--safe), transparent);
  }
  .tactical-link.active .tactical-link__line:last-child {
    background: linear-gradient(transparent, var(--safe));
  }
  .player-slot {
    grid-template-columns: auto minmax(0, 1fr) auto;
    min-height: 218px;
    align-content: center;
    padding: 28px clamp(18px, 2vw, 30px);
    border-radius: 7px 2px 7px 2px;
    border-color: rgba(117, 177, 190, 0.18);
    background: linear-gradient(155deg, rgba(7, 35, 45, 0.86), rgba(2, 15, 22, 0.9));
  }
  .player-slot:first-child {
    border-top: 2px solid var(--tactical);
  }
  .player-slot:nth-child(3) {
    border-top: 2px solid var(--warning);
  }
  .player-slot--ready {
    border-color: rgba(104, 215, 170, 0.38);
    background: linear-gradient(155deg, rgba(10, 54, 48, 0.72), rgba(2, 17, 23, 0.92));
  }
  .player-avatar {
    border-radius: 50%;
    color: var(--tactical);
    background: rgba(83, 233, 232, 0.08);
  }
  .player-slot:nth-child(3) .player-avatar {
    color: var(--warning);
    background: rgba(237, 181, 82, 0.08);
  }
  .player-identity strong {
    font-family: var(--font-display);
    overflow: hidden;
    font-size: clamp(19px, 1.65vw, 24px);
    letter-spacing: 0.03em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .player-badges {
    margin-top: 9px;
  }
  .ready-state {
    min-width: 142px;
    margin: 0;
    padding: 8px 0 8px 16px;
    border-top: 0;
    border-left: 1px solid var(--line);
  }
  .ready-state strong {
    font: 700 13px var(--font-display);
    letter-spacing: 0.05em;
  }
  .command-actions {
    border-radius: 7px 2px 7px 2px;
    border-color: rgba(83, 233, 232, 0.24);
    background: rgba(2, 13, 20, 0.7);
  }
  .command-actions > header {
    color: var(--tactical);
  }
  .action-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(220px, 0.68fr) minmax(250px, 0.94fr);
    gap: 18px;
    align-items: center;
    padding: 20px clamp(20px, 2.2vw, 30px);
  }
  .self-readiness strong {
    font-family: var(--font-display);
    font-size: 21px;
  }
  .self-readiness p {
    color: var(--ink-500);
  }
  .ready-control,
  .host-control {
    display: grid;
    gap: 10px;
    align-content: center;
  }
  .ready-control {
    padding-inline: 18px;
    border-inline: 1px solid var(--line);
  }
  .ready-control > small {
    color: var(--ink-500);
    font: 700 8px var(--font-display);
    letter-spacing: 0.13em;
    text-align: center;
  }
  .host-control {
    min-height: 92px;
  }
  .start-divider {
    margin: 0;
  }
  .start-divider::before,
  .start-divider::after {
    background: var(--line);
  }
  .start-reason {
    color: var(--ink-500);
  }
  .start-reason.available {
    color: var(--safe);
  }
  .guest-guidance {
    border-radius: 4px;
    border-color: rgba(104, 215, 170, 0.24);
    background: rgba(104, 215, 170, 0.06);
  }
  .system-log {
    margin-top: 16px;
    border-color: var(--line);
    border-radius: 3px;
    background: rgba(1, 8, 13, 0.5);
  }
  @media (max-width: 820px) {
    .player-slots {
      grid-template-columns: minmax(0, 1fr) 78px minmax(0, 1fr);
      gap: 12px;
    }
    .player-slot {
      grid-template-columns: auto minmax(0, 1fr);
      min-height: 186px;
      padding: 22px 16px;
    }
    .ready-state {
      grid-column: 1 / -1;
      width: 100%;
      min-width: 0;
      padding: 11px 0 0;
      border-top: 1px solid var(--line);
      border-left: 0;
    }
    .action-body {
      grid-template-columns: minmax(0, 1fr) minmax(220px, 0.9fr);
    }
    .host-control {
      grid-column: 1 / -1;
      grid-template-columns: minmax(0, 1fr) minmax(230px, 0.75fr);
      align-items: center;
    }
    .host-control .start-divider {
      display: none;
    }
  }
  @media (max-width: 580px) {
    .waiting {
      padding: 18px 14px;
    }
    .waiting-heading {
      grid-template-columns: auto 1fr;
    }
    .waiting-heading > :global(.ui-badge) {
      grid-column: 2;
      justify-self: start;
    }
    .player-slots {
      grid-template-columns: 1fr;
    }
    .tactical-link {
      min-height: 82px;
      grid-row: auto;
    }
    .tactical-link__line {
      width: 54px;
      height: 1px;
    }
    .tactical-link__line:last-child {
      background: linear-gradient(90deg, transparent, var(--line-active));
    }
    .player-slot {
      min-height: 145px;
    }
    .stage-readiness {
      grid-template-columns: 1fr;
      gap: 4px;
      text-align: center;
    }
    .stage-readiness__signal {
      justify-content: center;
    }
    .stage-readiness small {
      text-align: center;
    }
    .action-body,
    .host-control {
      grid-template-columns: 1fr;
    }
    .ready-control {
      padding: 16px 0;
      border-block: 1px solid var(--line);
      border-inline: 0;
    }
    .room-identity {
      grid-template-columns: 1fr 1fr;
    }
    .room-identity > div:first-child {
      grid-column: 1 / -1;
    }
    .room-identity > div:nth-child(2) {
      border-bottom: 0;
    }
  }
</style>
