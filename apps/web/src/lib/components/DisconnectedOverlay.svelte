<script lang="ts">
  import { Radio, RefreshCw } from '@lucide/svelte';
  import { t } from '$lib/i18n';
  interface Props {
    deadline: string | null;
  }
  let { deadline }: Props = $props();
  let remaining = $state(0);
  $effect(() => {
    const update = () =>
      (remaining = deadline
        ? Math.max(0, Math.ceil((new Date(deadline).getTime() - Date.now()) / 1000))
        : 90);
    update();
    const timer = setInterval(update, 1_000);
    return () => clearInterval(timer);
  });
</script>

<div
  class="disconnect-overlay"
  role="alertdialog"
  aria-modal="true"
  aria-labelledby="disconnect-title"
>
  <section class="disconnect-card panel">
    <div class="disconnect-icon"><Radio size={27} /></div>
    <p class="eyebrow">{$t('disconnect.eyebrow')}</p>
    <h2 id="disconnect-title">{$t('disconnect.title')}</h2>
    <p>{$t('disconnect.description')}</p>
    <strong class="countdown">{remaining}<small>{$t('disconnect.seconds')}</small></strong>
    <div class="reconnect-track" aria-hidden="true">
      <i style={`width:${Math.min(100, (remaining / 90) * 100)}%`}></i>
    </div>
    <span><RefreshCw size={13} /> {$t('disconnect.checking')}</span>
  </section>
</div>

<style>
  .disconnect-overlay {
    position: fixed;
    z-index: 70;
    inset: 68px 0 0;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgba(1, 7, 11, 0.76);
    backdrop-filter: blur(10px);
  }
  .disconnect-card {
    width: min(450px, 100%);
    padding: 35px;
    text-align: center;
    background:
      radial-gradient(circle at 50% 12%, rgba(246, 173, 53, 0.1), transparent 38%),
      rgba(5, 18, 27, 0.95);
  }
  .disconnect-icon {
    display: grid;
    width: 62px;
    height: 62px;
    place-items: center;
    margin: 0 auto 20px;
    border: 1px solid rgba(255, 180, 60, 0.4);
    border-radius: 50%;
    color: var(--amber-500);
    background: rgba(255, 180, 60, 0.08);
    animation: pulse 1.5s infinite;
  }
  .disconnect-card h2 {
    font-size: 24px;
  }
  .disconnect-card > p:not(.eyebrow) {
    color: var(--steel-300);
    font-size: 12px;
  }
  .countdown {
    display: block;
    margin: 22px 0;
    color: var(--amber-500);
    font-family: Rajdhani;
    font-size: 46px;
  }
  .countdown small {
    margin-left: 5px;
    font-size: 12px;
  }
  .reconnect-track {
    height: 2px;
    margin: -10px 0 22px;
    overflow: hidden;
    background: rgba(246, 173, 53, 0.12);
  }
  .reconnect-track i {
    display: block;
    height: 100%;
    margin-left: auto;
    background: var(--amber-400);
    box-shadow: 0 0 9px var(--amber-400);
    transition: width 1s linear;
  }
  .disconnect-card > span {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: #6e8b9b;
    font-size: 10px;
  }
  @media (max-width: 720px) {
    .disconnect-overlay {
      inset: 60px 0 0;
    }
  }
</style>
