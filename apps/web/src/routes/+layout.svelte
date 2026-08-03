<script lang="ts">
  import '@fontsource/ibm-plex-sans-kr/korean-400.css';
  import '@fontsource/ibm-plex-sans-kr/korean-700.css';
  import '@fontsource/ibm-plex-sans-kr/latin-400.css';
  import '@fontsource/ibm-plex-sans-kr/latin-700.css';
  import '@fontsource/rajdhani/latin-500.css';
  import '@fontsource/rajdhani/latin-600.css';
  import '@fontsource/rajdhani/latin-700.css';
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';
  import { Crosshair, History, Radio, Settings, UserRound, X } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { gameError, session, socketStatus } from '$lib/stores';

  let { children } = $props();

  onMount(async () => {
    try {
      session.set(await api.currentSession());
    } catch {
      session.set(null);
    }
  });

  const connectionText = (status: string) =>
    ({
      online: '실시간 연결',
      connecting: '연결 중',
      reconnecting: '재연결 중',
      offline: '연결 끊김'
    })[status] ?? '대기';
</script>

<svelte:head>
  <title>Mk.01 — Naval Command</title>
  <meta
    name="description"
    content="두 지휘관이 실시간으로 맞붙는 서버 권위형 온라인 해전 전략 게임"
  />
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
        <a class="nav-link" href={resolve('/lobby')}><Radio size={17} /><span>작전 로비</span></a>
        <a class="nav-link" href={resolve('/stats')}><History size={17} /><span>전투 기록</span></a>
      {/if}
      <a class="nav-link" href={resolve('/settings')}><Settings size={17} /><span>설정</span></a>
    </nav>

    {#if $session}
      <span class="user-chip" title={`${$session.nickname} · ${connectionText($socketStatus)}`}>
        <UserRound size={14} />
        {$session.nickname}
      </span>
    {/if}
  </div>
</header>

<main class="main-content">{@render children()}</main>

{#if $gameError}
  <div class="toast-stack" aria-live="assertive">
    <div class="toast">
      <span class="danger"><Radio size={18} /></span>
      <div>
        <strong>{$gameError.code}</strong>
        <p>{$gameError.message}</p>
      </div>
      <button class="icon-button" aria-label="오류 닫기" onclick={() => gameError.set(null)}>
        <X size={16} />
      </button>
    </div>
  </div>
{/if}
