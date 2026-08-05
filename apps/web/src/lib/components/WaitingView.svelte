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
  let allReady = $derived(
    snapshot.players.length === 2 &&
      snapshot.players.every((player) => player.readyState === 'READY')
  );
  let startDisabledReason = $derived.by(() => {
    if (snapshot.gameId || snapshot.roomState === 'PLACEMENT') return '게임이 이미 시작되었습니다.';
    if (snapshot.players.length !== 2) return '상대 지휘관이 아직 입장하지 않았습니다.';
    if (snapshot.players.some((player) => player.connectionState !== 'ONLINE')) {
      return '연결이 끊긴 플레이어가 있습니다.';
    }
    if (!allReady) return '상대 지휘관의 준비를 기다리고 있습니다.';
    if (snapshot.roomState !== 'READY_TO_START') return '최신 방 상태를 동기화하고 있습니다.';
    if (!online) return '실시간 연결이 복구될 때까지 기다려 주세요.';
    return '';
  });
  let heading = $derived(
    snapshot.roomState === 'WAITING_FOR_OPPONENT'
      ? '상대 지휘관의 입장을 기다리고 있습니다.'
      : snapshot.roomState === 'READY_TO_START'
        ? isHost
          ? '모든 지휘관의 준비가 완료되었습니다. 작전을 시작하십시오.'
          : '준비가 완료되었습니다. 방장의 작전 개시를 기다리고 있습니다.'
        : '모든 지휘관이 준비를 완료해야 합니다.'
  );

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
        text: '온라인 해전 작전실에 참가하세요.',
        url: inviteUrl
      });
    } else await copyInvite();
  }

  function readyTime(player: PlayerPublic): string {
    if (!player.readyAt) return '승인 대기';
    return new Date(player.readyAt).toLocaleTimeString('ko-KR', {
      hour: '2-digit',
      minute: '2-digit'
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
      <p class="eyebrow">PRE-OPERATION COMMAND ROOM</p>
      <h1 id="waiting-title">{heading}</h1>
      <p class="muted">
        양쪽 지휘관이 준비를 완료한 뒤 방장이 작전 시작을 승인해야 함선 배치 채널이 열립니다.
      </p>
    </div>
    <Badge tone={snapshot.roomState === 'READY_TO_START' ? 'success' : 'cyan'} pulse>
      {snapshot.roomState}
    </Badge>
  </header>

  <div class="room-identity">
    <div><small>OPERATION</small><strong>{snapshot.room.name}</strong></div>
    <div><small>SECURE CODE</small><strong class="code">{snapshot.room.code}</strong></div>
    <div><small>ROOM VERSION</small><strong>V{snapshot.roomVersion}</strong></div>
  </div>

  <div class="invite-bar">
    <span>{inviteUrl}</span>
    <button class="icon-button" onclick={copyInvite} aria-label="초대 링크 복사" title="링크 복사">
      {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}
    </button>
    <button class="icon-button" onclick={shareInvite} aria-label="초대 링크 공유" title="공유">
      <Share2 size={16} />
    </button>
  </div>

  <div class="room-command-grid">
    <div class="player-slots" aria-label="지휘관 준비 상태">
      {#each [hostPlayer, guestPlayer] as player (player?.role ?? 'EMPTY_GUEST')}
        {#if player}
          <article
            class:player-slot--ready={player.readyState === 'READY'}
            class:player-slot--offline={player.connectionState !== 'ONLINE'}
            class="player-slot"
          >
            <span class="player-avatar"><UserRound size={21} /></span>
            <div class="player-identity">
              <small>{player.role === 'HOST' ? 'COMMAND AUTHORITY' : 'SECOND COMMANDER'}</small>
              <strong>{player.nickname}</strong>
              <div class="player-badges">
                <Badge tone={player.role === 'HOST' ? 'cyan' : 'neutral'}>
                  {player.role === 'HOST' ? 'HOST' : 'GUEST'}
                </Badge>
                <span class:offline={player.connectionState !== 'ONLINE'} class="connection-state">
                  {#if player.connectionState === 'ONLINE'}<Wifi size={11} /> 연결됨{:else}<WifiOff
                      size={11}
                    />
                    {player.connectionState === 'RECONNECTING' ? '재접속 중' : '오프라인'}{/if}
                </span>
              </div>
            </div>
            <div class:ready={player.readyState === 'READY'} class="ready-state">
              {#if player.readyState === 'READY'}<Check size={17} />{:else}<CircleDot
                  size={15}
                />{/if}
              <span>
                <strong>{player.readyState === 'READY' ? '준비 완료' : '준비 대기'}</strong>
                <small>{readyTime(player)}</small>
              </span>
            </div>
          </article>
        {:else}
          <article class="player-slot player-slot--pending">
            <span class="player-avatar"><UserRound size={21} /></span>
            <div class="player-identity">
              <small>SECOND COMMANDER</small>
              <strong>상대 지휘관을 기다리는 중</strong>
              <span class="scanning"><i></i> 초대 채널 탐색 중</span>
            </div>
          </article>
        {/if}
      {/each}
    </div>

    <aside class="command-actions" aria-label="작전 준비 제어">
      <header><Radio size={14} /><span>AUTHORIZATION CONTROL</span><em>SERVER VERIFIED</em></header>
      <div class="action-body">
        <div class="self-readiness">
          <small>YOUR READINESS</small>
          <strong>{selfPlayer?.readyState === 'READY' ? '작전 준비 완료' : '준비 승인 필요'}</strong
          >
          <p>준비 상태는 서버에 저장되며 새로고침하거나 잠시 재접속해도 복구됩니다.</p>
        </div>
        <Button
          variant={selfPlayer?.readyState === 'READY' ? 'outline' : 'success'}
          size="lg"
          full
          loading={readyPending}
          disabled={!online || startPending}
          onclick={selfPlayer?.readyState === 'READY' ? onunready : onready}
        >
          {#if selfPlayer?.readyState === 'READY'}준비 취소{:else}<Check size={17} /> 준비 완료{/if}
        </Button>

        <div class="start-divider"><span>HOST AUTHORIZATION</span></div>
        {#if isHost}
          <Button
            variant="primary"
            size="lg"
            full
            loading={startPending}
            disabled={!snapshot.canStartGame || !online || readyPending}
            onclick={onstart}
          >
            <Rocket size={17} /> 작전 시작
          </Button>
          <p class:available={!startDisabledReason} class="start-reason">
            {startDisabledReason ||
              '모든 시작 조건이 충족되었습니다. 최종 승인을 진행할 수 있습니다.'}
          </p>
        {:else}
          <div class:ready={allReady} class="guest-guidance">
            <ShieldCheck size={20} />
            <span>
              <strong>{allReady ? '방장의 작전 개시 대기' : '모든 지휘관의 준비 대기'}</strong>
              <small>
                {allReady
                  ? '준비가 완료되었습니다. 방장만 작전을 시작할 수 있습니다.'
                  : '두 플레이어가 준비 완료 상태가 되어야 합니다.'}
              </small>
            </span>
          </div>
        {/if}
      </div>
    </aside>
  </div>

  <div class="system-log" aria-label="대기실 상태 로그">
    <span>SYS</span>
    <p>{heading}</p>
    <time>V{snapshot.roomVersion}</time>
  </div>

  <Button variant="ghost" size="sm" class="leave-button" onclick={onleave}>
    <LogOut size={15} /> 작전실 나가기
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
</style>
