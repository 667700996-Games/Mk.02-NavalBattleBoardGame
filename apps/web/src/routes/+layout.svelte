<script lang="ts">
  import '../app.css';
  import '../fonts.css';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';
  import { Crosshair, History, Radio, Settings, Users } from '@lucide/svelte';
  import { ApiError, api } from '$lib/api';
  import { installFunnelAbandonmentTracking } from '$lib/funnel';
  import { installRealUserMonitoring } from '$lib/performance';
  import { installAudioDirector } from '$lib/sound';
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
  import {
    formatDateTime,
    initializeLocale,
    launchLocales,
    localizeError,
    locale,
    setLocale,
    t,
    type Locale,
    type MessageKey
  } from '$lib/i18n';

  let { children } = $props();
  let now = $state<Date | null>(null);
  const clock = $derived.by(() => {
    void $locale;
    return now
      ? formatDateTime(now, { hour: '2-digit', minute: '2-digit', hour12: false })
      : '--:--';
  });

  onMount(() => {
    document.documentElement.dataset.hydrated = 'true';
    initializeLocale();
    const removeFunnelTracking = installFunnelAbandonmentTracking();
    const removePerformanceTracking = installRealUserMonitoring();
    const removeAudioDirector = installAudioDirector();
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
    const updateClock = () => (now = new Date());
    updateClock();
    const timer = setInterval(updateClock, 30_000);
    api.protocolCompatibility().catch((caught: unknown) => {
      if (caught instanceof ApiError && caught.code === 'SERVER_PROTOCOL_MISMATCH') {
        gameError.set({ code: caught.code, message: caught.message, retryable: false });
      }
    });
    api
      .currentSession()
      .then((current) => session.set(current))
      .catch(() => session.set(null));
    return () => {
      delete document.documentElement.dataset.hydrated;
      removeFunnelTracking();
      removePerformanceTracking();
      removeAudioDirector();
      clearInterval(timer);
      window.removeEventListener('keydown', onKeydown, true);
      window.removeEventListener('pointerdown', onPointerdown, true);
    };
  });

  const connectionText = (status: string) => {
    const key =
      (
        {
          online: 'connection.online',
          connecting: 'connection.connecting',
          reconnecting: 'connection.reconnecting',
          offline: 'connection.offline'
        } satisfies Record<string, MessageKey>
      )[status] ?? 'connection.idle';
    return $t(key);
  };

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
    document.documentElement.dataset.effectQuality = $preferences.effectQuality;
    document.documentElement.dataset.fleetSkin = $preferences.cosmetics.fleetSkin;
    document.documentElement.dataset.boardTheme = $preferences.cosmetics.boardTheme;
    document.documentElement.dataset.effectTheme = $preferences.cosmetics.effectTheme;
    document.documentElement.dataset.profileEmblem = $preferences.cosmetics.profileEmblem;
    document.documentElement.dataset.presentationFrame = $preferences.cosmetics.presentationFrame;
    document.documentElement.lang = $locale === 'en-XA' ? 'en' : $locale;
  });
</script>

<svelte:head>
  <title>{$t('layout.meta.title')}</title>
  <meta name="description" content={$t('layout.meta.description')} />
  <meta property="og:type" content="website" />
  <meta property="og:locale" content={$locale.replace('-', '_')} />
  <meta property="og:title" content={$t('layout.meta.title')} />
  <meta property="og:description" content={$t('layout.meta.ogDescription')} />
  <meta property="og:image" content="/og-mk01-command-v2.png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:image:alt" content={$t('layout.meta.ogImageAlt')} />
  <meta name="twitter:card" content="summary_large_image" />
</svelte:head>

<header class="app-header">
  <div class="shell app-header__inner">
    <a class="brand" href={resolve($session ? '/play' : '/')} aria-label={$t('layout.home')}>
      <span class="brand__mark"><Crosshair size={18} strokeWidth={1.5} /></span>
      <span class="brand__text">
        <strong>MK.01</strong>
        <small>{$t('layout.brandTagline')}</small>
      </span>
    </a>

    <nav class="nav-links" aria-label={$t('layout.mainNavigation')}>
      {#if $session}
        <Tooltip text={$t('layout.playSelection')} side="bottom">
          <a
            class:active={active('/play') || active('/single-player')}
            class="nav-link"
            aria-label={$t('layout.playSelection')}
            href={resolve('/play')}><Radio size={17} /><span>{$t('layout.playSelection')}</span></a
          >
        </Tooltip>
        <Tooltip text={$t('layout.socialHub')} side="bottom">
          <a
            class:active={active('/social')}
            class="nav-link"
            aria-label={$t('layout.socialHub')}
            href={resolve('/social')}><Users size={17} /><span>{$t('layout.socialHub')}</span></a
          >
        </Tooltip>
        <Tooltip text={$t('layout.battleHistory')} side="bottom">
          <a
            class:active={active('/stats')}
            class="nav-link"
            aria-label={$t('layout.battleHistory')}
            href={resolve('/stats')}
            ><History size={17} /><span>{$t('layout.battleHistory')}</span></a
          >
        </Tooltip>
      {/if}
      <Tooltip text={$t('layout.settings')} side="bottom">
        <a
          class:active={active('/settings')}
          class="nav-link"
          aria-label={$t('layout.settings')}
          href={resolve('/settings')}
          ><Settings size={17} /><span>{$t('layout.settingsShort')}</span></a
        >
      </Tooltip>
    </nav>

    <div class="header-operator">
      <label class="locale-control">
        <span class="sr-only">{$t('locale.selector')}</span>
        <select
          aria-label={$t('locale.selector')}
          value={$locale}
          onchange={(event) => setLocale(event.currentTarget.value as Locale)}
        >
          {#each launchLocales as option (option)}
            <option value={option}>
              {$t(option === 'ko-KR' ? 'locale.koKR' : 'locale.enUS')}
            </option>
          {/each}
          {#if $locale === 'en-XA'}<option value="en-XA">{$t('locale.enXA')}</option>{/if}
        </select>
      </label>
      <div class="header-clock" aria-label={$t('layout.currentTime', { time: clock })}>
        <small>{$t('layout.localTime')}</small><strong>{clock}</strong>
      </div>
      {#if $session}
        <Status
          label={$t('layout.tacticalLink')}
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
        <Status label={$t('layout.system')} value={$t('layout.standby')} state="idle" />
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
        message={localizeError($gameError, 'error.requestFailed')}
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
  .locale-control select {
    min-height: 30px;
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 4px 24px 4px 8px;
    color: var(--ink-300);
    background: var(--navy-950);
    font: 700 9px var(--font-display);
    letter-spacing: 0.04em;
  }
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
  @media (max-width: 560px) {
    .locale-control select {
      width: 42px;
      padding-inline: 5px;
    }
  }
</style>
