<script lang="ts">
  import '../app.css';
  import '../fonts.css';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';
  import { Crosshair, History, Radio, Settings } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { installFunnelAbandonmentTracking } from '$lib/funnel';
  import { installRealUserMonitoring } from '$lib/performance';
  import {
    dismissHudNotification,
    gameError,
    hudNotifications,
    inputModality,
    preferences,
    session,
    socketStatus
  } from '$lib/stores';
  import { Avatar, Status, Toast, Tooltip } from '$lib/ui';

  let { children } = $props();
  let clock = $state('00:00');

  onMount(() => {
    const removeFunnelTracking = installFunnelAbandonmentTracking();
    const removePerformanceTracking = installRealUserMonitoring();
    const keyboardKeys = new Set([
      'Tab',
      'Enter',
      ' ',
      'Escape',
      'ArrowUp',
      'ArrowDown',
      'ArrowLeft',
      'ArrowRight',
      'r',
      'R'
    ]);
    const onKeydown = (event: KeyboardEvent) => {
      if (keyboardKeys.has(event.key)) inputModality.set('keyboard');
    };
    const onPointerdown = (event: PointerEvent) => {
      inputModality.set(event.pointerType === 'touch' ? 'touch' : 'pointer');
    };
    inputModality.set(matchMedia('(pointer: coarse)').matches ? 'touch' : 'pointer');
    window.addEventListener('keydown', onKeydown, true);
    window.addEventListener('pointerdown', onPointerdown, true);
    const updateClock = () =>
      (clock = new Date().toLocaleTimeString('ko-KR', { hour: '2-digit', minute: '2-digit' }));
    updateClock();
    const timer = setInterval(updateClock, 30_000);
    api
      .currentSession()
      .then((current) => session.set(current))
      .catch(() => session.set(null));
    return () => {
      removeFunnelTracking();
      removePerformanceTracking();
      clearInterval(timer);
      window.removeEventListener('keydown', onKeydown, true);
      window.removeEventListener('pointerdown', onPointerdown, true);
    };
  });

  const connectionText = (status: string) =>
    ({
      online: '실시간 연결',
      connecting: '연결 중',
      reconnecting: '재연결 중',
      offline: '연결 끊김'
    })[status] ?? '대기';

  const statusState = (status: string) =>
    status === 'online'
      ? 'online'
      : status === 'offline'
        ? 'danger'
        : status === 'idle'
          ? 'idle'
          : 'warning';

  const active = (path: string) => page.url.pathname.startsWith(path);

  $effect(() => {
    if (typeof document === 'undefined') return;
    document.documentElement.dataset.motion = $preferences.reducedMotion ? 'reduced' : 'full';
    document.documentElement.dataset.contrast = $preferences.highContrast ? 'high' : 'standard';
    document.documentElement.dataset.colorVision = $preferences.colorVision;
  });
</script>

<svelte:head>
  <title>Mk.01 — Naval Command</title>
  <meta
    name="description"
    content="두 지휘관이 실시간으로 맞붙는 서버 권위형 온라인 해전 전략 게임"
  />
  <meta property="og:type" content="website" />
  <meta property="og:locale" content="ko_KR" />
  <meta property="og:title" content="Mk.01 — Naval Command" />
  <meta
    property="og:description"
    content="함선을 배치하고 좌표를 추론하며 맞붙는 2인 실시간 해전 전략 게임"
  />
  <meta property="og:image" content="/og-mk01-command-v2.png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:image:alt" content="시안과 주황 함대가 대치하는 Mk.01 해군 전술 지도" />
  <meta name="twitter:card" content="summary_large_image" />
</svelte:head>

<header class="app-header">
  <div class="shell app-header__inner">
    <a class="brand" href={resolve($session ? '/lobby' : '/')} aria-label="Mk.01 홈">
      <span class="brand__mark"><Crosshair size={18} strokeWidth={1.5} /></span>
      <span class="brand__text">
        <strong>MK.01</strong>
        <small>NAVAL COMMAND SYSTEM</small>
      </span>
    </a>

    <nav class="nav-links" aria-label="주 메뉴">
      {#if $session}
        <Tooltip text="작전 로비" side="bottom">
          <a
            class:active={active('/lobby') || active('/room')}
            class="nav-link"
            aria-label="작전 로비"
            href={resolve('/lobby')}><Radio size={17} /><span>작전 로비</span></a
          >
        </Tooltip>
        <Tooltip text="전투 기록" side="bottom">
          <a
            class:active={active('/stats')}
            class="nav-link"
            aria-label="전투 기록"
            href={resolve('/stats')}><History size={17} /><span>전투 기록</span></a
          >
        </Tooltip>
      {/if}
      <Tooltip text="환경 설정" side="bottom">
        <a
          class:active={active('/settings')}
          class="nav-link"
          aria-label="환경 설정"
          href={resolve('/settings')}><Settings size={17} /><span>설정</span></a
        >
      </Tooltip>
    </nav>

    <div class="header-operator">
      <div class="header-clock" aria-label={`현재 시간 ${clock}`}>
        <small>LOCAL</small><strong>{clock}</strong>
      </div>
      {#if $session}
        <Status
          label="TACTICAL LINK"
          value={connectionText($socketStatus)}
          state={statusState($socketStatus)}
        />
        <span class="user-chip" title={`${$session.nickname} · ${connectionText($socketStatus)}`}>
          <Avatar
            name={$session.nickname}
            size="sm"
            status={$socketStatus === 'online'
              ? 'online'
              : $socketStatus === 'offline'
                ? 'offline'
                : 'reconnecting'}
          />
          <span>{$session.nickname}</span>
        </span>
      {:else}
        <Status label="SYSTEM" value="STANDBY" state="idle" />
      {/if}
    </div>
  </div>
</header>

<main class="main-content">{@render children()}</main>

{#if $gameError || $hudNotifications.length}
  <div class="toast-stack" aria-live="assertive">
    {#if $gameError}
      <Toast
        tone="danger"
        title={$gameError.code}
        message={$gameError.message}
        onclose={() => gameError.set(null)}
      />
    {/if}
    {#each $hudNotifications as notification (notification.id)}
      <Toast
        tone={notification.tone}
        title={notification.title}
        message={notification.message}
        onclose={() => dismissHudNotification(notification.id)}
      />
    {/each}
  </div>
{/if}

<style>
  .header-clock {
    display: grid;
    justify-items: end;
    line-height: 1;
  }
  .header-clock small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.16em;
  }
  .header-clock strong {
    margin-top: 3px;
    color: var(--ink-300);
    font-family: var(--font-display);
    font-size: 12px;
    letter-spacing: 0.08em;
  }
  @media (max-width: 880px) {
    .header-clock,
    .header-operator :global(.ui-status) {
      display: none;
    }
  }
</style>
