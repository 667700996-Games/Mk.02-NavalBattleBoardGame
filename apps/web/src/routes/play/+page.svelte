<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowRight, BookOpen, Crosshair, Radio, ShieldCheck, Users } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { t } from '$lib/i18n';
  import { session } from '$lib/stores';
  import { Badge } from '$lib/ui';

  let ready = false;

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      ready = true;
    } catch {
      await goto(resolve('/'));
    }
  });

  async function chooseTutorial() {
    await goto(resolve('/tutorial'));
  }

  async function chooseSinglePlayer() {
    await goto(resolve('/single-player'));
  }

  async function chooseMultiplayer() {
    await goto(resolve('/lobby'));
  }
</script>

<svelte:head><title>{$t('playMode.metaTitle')}</title></svelte:head>

<section class="mode-page shell" aria-labelledby="mode-title">
  <header class="mode-heading">
    <Badge tone="success" pulse>{$t('playMode.sessionReady')}</Badge>
    <p class="eyebrow">{$t('playMode.eyebrow')}</p>
    <h1 id="mode-title" class="page-title">{$t('playMode.title')}</h1>
    <p>
      {$t('playMode.description', { commander: $session?.nickname ?? '' })}
    </p>
  </header>

  {#if ready}
    <div class="mode-grid">
      <button
        type="button"
        class="mode-card mode-card--tutorial"
        aria-label={$t('playMode.chooseTutorial')}
        onclick={chooseTutorial}
      >
        <span class="mode-card__index">{$t('playMode.tutorialIndex')}</span>
        <span class="mode-card__icon"><BookOpen size={34} strokeWidth={1.4} /></span>
        <span class="mode-card__copy">
          <small>{$t('playMode.tutorialCode')}</small>
          <strong>{$t('playMode.tutorialTitle')}</strong>
          <span>{$t('playMode.tutorialDescription')}</span>
        </span>
        <span class="mode-card__features">
          <span><ShieldCheck size={15} /> {$t('playMode.tutorialFeatureOne')}</span>
          <span><Radio size={15} /> {$t('playMode.tutorialFeatureTwo')}</span>
        </span>
        <span class="mode-card__action"
          >{$t('playMode.tutorialAction')} <ArrowRight size={19} /></span
        >
      </button>

      <button
        type="button"
        class="mode-card mode-card--single"
        aria-label={$t('playMode.chooseSingle')}
        onclick={chooseSinglePlayer}
      >
        <span class="mode-card__index">{$t('playMode.singleIndex')}</span>
        <span class="mode-card__icon"><Crosshair size={34} strokeWidth={1.4} /></span>
        <span class="mode-card__copy">
          <small>{$t('playMode.singleCode')}</small>
          <strong>{$t('playMode.singleTitle')}</strong>
          <span>{$t('playMode.singleDescription')}</span>
        </span>
        <span class="mode-card__features">
          <span><ShieldCheck size={15} /> {$t('playMode.singleFeatureOne')}</span>
          <span><Radio size={15} /> {$t('playMode.singleFeatureTwo')}</span>
        </span>
        <span class="mode-card__action">{$t('playMode.singleAction')} <ArrowRight size={19} /></span
        >
      </button>

      <button
        type="button"
        class="mode-card mode-card--multi"
        aria-label={$t('playMode.chooseMulti')}
        onclick={chooseMultiplayer}
      >
        <span class="mode-card__index">{$t('playMode.multiIndex')}</span>
        <span class="mode-card__icon"><Users size={34} strokeWidth={1.4} /></span>
        <span class="mode-card__copy">
          <small>{$t('playMode.multiCode')}</small>
          <strong>{$t('playMode.multiTitle')}</strong>
          <span>{$t('playMode.multiDescription')}</span>
        </span>
        <span class="mode-card__features">
          <span><Users size={15} /> {$t('playMode.multiFeatureOne')}</span>
          <span><Radio size={15} /> {$t('playMode.multiFeatureTwo')}</span>
        </span>
        <span class="mode-card__action">{$t('playMode.multiAction')} <ArrowRight size={19} /></span>
      </button>
    </div>
  {:else}
    <div class="mode-loading" role="status">
      <span></span>
      <p>{$t('playMode.verifying')}</p>
    </div>
  {/if}
</section>

<style>
  .mode-page {
    display: grid;
    align-content: center;
    min-height: calc(100vh - 72px);
    padding-block: clamp(56px, 8vw, 108px);
  }
  .mode-heading {
    max-width: 760px;
    margin: 0 auto 38px;
    text-align: center;
  }
  .mode-heading .eyebrow {
    margin: 24px 0 10px;
  }
  .mode-heading h1 {
    margin-bottom: 18px;
    font-size: clamp(40px, 5.5vw, 68px);
  }
  .mode-heading > p:last-child {
    margin: 0;
    color: var(--ink-300);
    font-size: clamp(14px, 1.5vw, 17px);
    line-height: 1.8;
    word-break: keep-all;
  }
  .mode-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 18px;
    width: min(1280px, 100%);
    margin-inline: auto;
  }
  .mode-card {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 22px 18px;
    min-height: 420px;
    padding: clamp(28px, 4vw, 42px);
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 10px 3px 10px 3px;
    color: var(--ink-100);
    text-align: left;
    background: linear-gradient(145deg, rgba(8, 30, 38, 0.92), rgba(2, 13, 20, 0.97));
    box-shadow: var(--shadow-lg);
    cursor: pointer;
    transition: 220ms var(--ease-out);
    transition-property: border-color, transform, box-shadow, background;
  }
  .mode-card::after {
    position: absolute;
    right: -20%;
    bottom: -42%;
    width: 320px;
    aspect-ratio: 1;
    border: 1px solid rgba(83, 233, 232, 0.08);
    border-radius: 50%;
    content: '';
    box-shadow:
      0 0 0 48px rgba(83, 233, 232, 0.025),
      0 0 0 96px rgba(83, 233, 232, 0.018);
  }
  .mode-card:hover,
  .mode-card:focus-visible {
    border-color: var(--cyan-300);
    background: linear-gradient(145deg, rgba(10, 42, 51, 0.96), rgba(2, 16, 23, 0.98));
    box-shadow:
      var(--shadow-lg),
      0 0 44px rgba(40, 223, 232, 0.09);
    transform: translateY(-4px);
  }
  .mode-card:focus-visible {
    outline: 2px solid var(--cyan-300);
    outline-offset: 4px;
  }
  .mode-card--multi .mode-card__icon,
  .mode-card--multi .mode-card__action {
    color: var(--green-400);
  }
  .mode-card--tutorial .mode-card__icon,
  .mode-card--tutorial .mode-card__action,
  .mode-card--tutorial .mode-card__features :global(svg) {
    color: var(--amber-400);
  }
  .mode-card__index {
    position: absolute;
    top: 18px;
    right: 20px;
    color: var(--ink-600);
    font: 700 8px var(--font-display);
    letter-spacing: 0.16em;
  }
  .mode-card__icon {
    display: grid;
    width: 66px;
    height: 66px;
    place-items: center;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .mode-card__copy {
    display: grid;
    align-content: center;
    gap: 4px;
  }
  .mode-card__copy small {
    color: var(--ink-500);
    font: 700 8px var(--font-display);
    letter-spacing: 0.16em;
  }
  .mode-card__copy strong {
    font: 600 clamp(26px, 3vw, 36px) var(--font-display);
    letter-spacing: -0.02em;
  }
  .mode-card__copy > span {
    color: var(--ink-300);
    font-size: 12px;
    line-height: 1.7;
    word-break: keep-all;
  }
  .mode-card__features {
    z-index: 1;
    grid-column: 1 / -1;
    display: grid;
    align-content: center;
    gap: 10px;
    padding-block: 22px;
    border-block: 1px solid var(--line);
  }
  .mode-card__features > span {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--ink-300);
    font-size: 11px;
  }
  .mode-card__features :global(svg) {
    color: var(--cyan-400);
  }
  .mode-card__action {
    z-index: 1;
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 9px;
    color: var(--cyan-300);
    font: 700 13px var(--font-display);
    letter-spacing: 0.05em;
  }
  .mode-loading {
    display: grid;
    min-height: 420px;
    place-items: center;
    align-content: center;
    gap: 14px;
    color: var(--ink-400);
  }
  .mode-loading span {
    width: 28px;
    height: 28px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--cyan-300);
    border-radius: 50%;
    animation: mode-spin 0.8s linear infinite;
  }
  @media (max-width: 1180px) {
    .mode-grid {
      grid-template-columns: 1fr;
      width: min(720px, 100%);
    }
    .mode-card {
      min-height: 320px;
    }
  }
  @media (max-width: 760px) {
    .mode-page {
      min-height: auto;
      padding-block: 48px 72px;
    }
    .mode-card {
      min-height: 340px;
    }
  }
  @keyframes mode-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
