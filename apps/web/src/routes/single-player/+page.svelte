<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowLeft, ArrowRight, Bot, Crosshair, Radio, ShieldCheck } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import { localizeError, t } from '$lib/i18n';
  import { gameSnapshot, session } from '$lib/stores';
  import { Badge, Button, Surface } from '$lib/ui';
  import type { AiDifficulty } from '$lib/types';

  let loading = true;
  let practicing = false;
  let error = '';
  let multiplayerRoomActive = false;

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      const recovered = await api.recover();
      if (recovered?.room.status === 'CANCELLED') {
        await api.leaveRoom(recovered.room.id);
      } else if (recovered?.practiceDifficulty) {
        gameSnapshot.set(recovered);
        await goto(resolve('/room/[code]', { code: recovered.room.code }));
        return;
      } else if (recovered) {
        multiplayerRoomActive = true;
      }
    } catch {
      await goto(resolve('/'));
      return;
    } finally {
      loading = false;
    }
  });

  async function startPractice(difficulty: AiDifficulty) {
    practicing = true;
    error = '';
    try {
      const snapshot = await api.createPractice(difficulty);
      gameSnapshot.set(snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: snapshot.room.code }));
    } catch (caught) {
      trackFunnelFailure('room_joined', 'room_entry');
      error = localizeError(caught, 'singlePlayer.practiceError');
    } finally {
      practicing = false;
    }
  }
</script>

<svelte:head><title>{$t('singlePlayer.metaTitle')}</title></svelte:head>

<section class="single-page shell" aria-labelledby="single-title">
  <nav class="single-nav" aria-label={$t('singlePlayer.navigation')}>
    <Button variant="secondary" onclick={() => goto(resolve('/play'))}>
      <ArrowLeft size={17} />
      {$t('singlePlayer.changeMode')}
    </Button>
  </nav>

  <header class="single-heading">
    <div>
      <Badge tone="cyan" pulse>{$t('singlePlayer.simulationReady')}</Badge>
      <p class="eyebrow">{$t('singlePlayer.eyebrow')}</p>
      <h1 id="single-title" class="page-title">{$t('singlePlayer.title')}</h1>
    </div>
  </header>

  {#if error}
    <div class="single-alert" role="alert">
      <Radio size={18} />
      <div>
        <strong>{$t('singlePlayer.channelError')}</strong>
        <p>{error}</p>
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="single-loading" role="status">
      <span></span>
      <p>{$t('singlePlayer.loading')}</p>
    </div>
  {:else if multiplayerRoomActive}
    <Surface tone="elevated" padding="lg" class="active-operation">
      <ShieldCheck size={30} />
      <div>
        <small>{$t('singlePlayer.activeOperationCode')}</small>
        <h2>{$t('singlePlayer.activeMultiplayer')}</h2>
        <p>{$t('singlePlayer.activeMultiplayerDescription')}</p>
      </div>
      <Button variant="primary" onclick={() => goto(resolve('/lobby'))}>
        {$t('singlePlayer.returnMultiplayer')}
        <ArrowRight size={17} />
      </Button>
    </Surface>
  {:else}
    <Surface tone="elevated" padding="lg" class="practice-panel">
      <div class="practice-intro">
        <span class="practice-icon"><Bot size={34} strokeWidth={1.4} /></span>
        <div>
          <small>{$t('dashboard.aiRange')}</small>
          <h2>{$t('dashboard.aiPractice')}</h2>
          <p>{$t('dashboard.aiDescription')}</p>
        </div>
      </div>

      <div class="practice-divider">
        <span></span><small>{$t('singlePlayer.tacticalCore')}</small>
      </div>

      <div class="difficulty-heading">
        <div>
          <Crosshair size={18} />
          <strong>{$t('dashboard.aiDifficulty')}</strong>
        </div>
        <span>{$t('singlePlayer.selectDifficulty')}</span>
      </div>

      <div class="difficulty-grid" aria-label={$t('dashboard.aiDifficulty')}>
        <button disabled={practicing} onclick={() => startPractice('RECRUIT')}>
          <small>01</small><strong>{$t('dashboard.recruit')}</strong><span
            >{$t('dashboard.recruitCode')}</span
          ><em>{$t('singlePlayer.recruitDescription')}</em>
        </button>
        <button disabled={practicing} onclick={() => startPractice('OFFICER')}>
          <small>02</small><strong>{$t('dashboard.officer')}</strong><span
            >{$t('dashboard.officerCode')}</span
          ><em>{$t('singlePlayer.officerDescription')}</em>
        </button>
        <button disabled={practicing} onclick={() => startPractice('ADMIRAL')}>
          <small>03</small><strong>{$t('dashboard.admiral')}</strong><span
            >{$t('dashboard.admiralCode')}</span
          ><em>{$t('singlePlayer.admiralDescription')}</em>
        </button>
      </div>
    </Surface>
  {/if}
</section>

<style>
  .single-page {
    min-height: calc(100vh - 72px);
    padding-block: 42px 88px;
  }
  .single-nav {
    margin-bottom: 32px;
  }
  .single-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 44px;
    margin-bottom: 30px;
  }
  .single-heading .eyebrow {
    margin: 20px 0 8px;
  }
  .single-heading h1 {
    margin: 0;
    font-size: clamp(40px, 5vw, 64px);
  }
  .single-alert {
    display: flex;
    align-items: start;
    gap: 12px;
    margin-bottom: 18px;
    padding: 14px 16px;
    border: 1px solid rgba(255, 83, 100, 0.28);
    color: var(--danger);
    background: rgba(90, 14, 27, 0.18);
  }
  .single-alert div {
    display: grid;
    gap: 3px;
  }
  .single-alert p {
    margin: 0;
    color: var(--ink-300);
    font-size: 11px;
  }
  :global(.practice-panel) {
    border-radius: 10px 3px 10px 3px;
    border-color: rgba(83, 233, 232, 0.26);
    background:
      radial-gradient(circle at 12% 8%, rgba(40, 223, 232, 0.09), transparent 30%),
      linear-gradient(145deg, rgba(8, 30, 38, 0.92), rgba(2, 13, 20, 0.97));
  }
  .practice-intro {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 20px;
  }
  .practice-icon {
    display: grid;
    width: 72px;
    height: 72px;
    place-items: center;
    border: 1px solid rgba(255, 209, 107, 0.3);
    border-radius: 50%;
    color: var(--amber-400);
    background: rgba(255, 209, 107, 0.055);
    box-shadow: 0 0 36px rgba(255, 209, 107, 0.06);
  }
  .practice-intro small,
  .difficulty-heading span,
  .practice-divider small {
    color: var(--ink-500);
    font: 700 8px var(--font-display);
    letter-spacing: 0.15em;
  }
  .practice-intro h2 {
    margin: 4px 0 8px;
    font: 600 clamp(28px, 3.4vw, 42px) var(--font-display);
  }
  .practice-intro p {
    margin: 0;
    color: var(--ink-300);
    font-size: 13px;
  }
  .practice-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-block: 30px 24px;
  }
  .practice-divider span {
    height: 1px;
    flex: 1;
    background: var(--line);
  }
  .difficulty-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 14px;
  }
  .difficulty-heading > div {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--cyan-300);
  }
  .difficulty-heading strong {
    color: var(--ink-200);
    font-size: 13px;
  }
  .difficulty-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  .difficulty-grid button {
    position: relative;
    display: grid;
    min-height: 180px;
    padding: 24px 20px;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 7px 2px 7px 2px;
    color: var(--ink-200);
    text-align: left;
    background: rgba(4, 20, 28, 0.76);
    cursor: pointer;
    transition: 180ms var(--ease-out);
  }
  .difficulty-grid button:hover:not(:disabled),
  .difficulty-grid button:focus-visible {
    border-color: var(--cyan-300);
    color: white;
    background: rgba(40, 223, 232, 0.075);
    transform: translateY(-2px);
  }
  .difficulty-grid button:focus-visible {
    outline: 2px solid var(--cyan-300);
    outline-offset: 3px;
  }
  .difficulty-grid button:disabled {
    cursor: wait;
    opacity: 0.55;
  }
  .difficulty-grid small {
    position: absolute;
    top: 12px;
    right: 14px;
    color: var(--ink-600);
    font: 700 22px var(--font-display);
  }
  .difficulty-grid strong {
    align-self: end;
    font: 600 24px var(--font-display);
  }
  .difficulty-grid span {
    color: var(--cyan-300);
    font: 700 8px var(--font-display);
    letter-spacing: 0.14em;
  }
  .difficulty-grid em {
    margin-top: 12px;
    color: var(--ink-400);
    font-size: 10px;
    font-style: normal;
    line-height: 1.6;
    word-break: keep-all;
  }
  .single-loading {
    display: grid;
    min-height: 360px;
    place-items: center;
    align-content: center;
    gap: 14px;
    color: var(--ink-400);
  }
  .single-loading span {
    width: 28px;
    height: 28px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--cyan-300);
    border-radius: 50%;
    animation: single-spin 0.8s linear infinite;
  }
  :global(.active-operation) {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 20px;
  }
  :global(.active-operation > .ui-surface__content) {
    display: contents;
  }
  :global(.active-operation small) {
    color: var(--ink-500);
    font: 700 8px var(--font-display);
    letter-spacing: 0.14em;
  }
  :global(.active-operation h2) {
    margin: 3px 0 8px;
  }
  :global(.active-operation p) {
    margin: 0;
    color: var(--ink-300);
    font-size: 12px;
  }
  @media (max-width: 760px) {
    .single-page {
      padding-block: 32px 72px;
    }
    .single-heading {
      display: grid;
      gap: 18px;
    }
    .difficulty-grid {
      grid-template-columns: 1fr;
    }
    .difficulty-grid button {
      min-height: 140px;
    }
    :global(.active-operation) {
      grid-template-columns: 1fr;
    }
  }
  @keyframes single-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
