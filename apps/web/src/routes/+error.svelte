<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { ArrowLeft, Radio } from '@lucide/svelte';
  import { t } from '$lib/i18n';
</script>

<svelte:head
  ><title>{page.status === 404 ? $t('error.notFoundTitle') : $t('error.systemTitle')} · Mk.01</title
  ></svelte:head
>
<div class="error-page shell">
  <div class="error-radar" aria-hidden="true"><i></i></div>
  <section class="panel">
    <div class="error-code">{page.status}</div>
    <div class="diagnostic">
      <span></span>
      {$t('error.diagnostic')}
      <em>{$t('error.fault', { status: page.status })}</em>
    </div>
    <Radio size={35} />
    <h1>
      {page.status === 404 ? $t('error.notFoundHeading') : $t('error.systemHeading')}
    </h1>
    <p>
      {page.status === 404 ? $t('error.notFoundDescription') : $t('error.systemDescription')}
    </p>
    <a class="button button--primary" href={resolve('/lobby')}
      ><ArrowLeft size={16} /> {$t('error.returnLobby')}</a
    >
    <footer>
      <span>MK01-NCS</span><span>{$t('error.secureFallback')}</span><span>SEOUL / KR</span>
    </footer>
  </section>
</div>

<style>
  .error-page {
    position: relative;
    display: grid;
    min-height: calc(100vh - 68px);
    place-items: center;
    padding-block: 40px;
  }
  .error-page section {
    position: relative;
    width: min(520px, 100%);
    padding: 45px;
    text-align: center;
    overflow: hidden;
    background:
      radial-gradient(circle at 50% 10%, rgba(40, 223, 232, 0.09), transparent 38%),
      rgba(4, 17, 26, 0.9);
  }
  .error-radar {
    position: absolute;
    width: min(720px, 88vw);
    aspect-ratio: 1;
    border: 1px solid rgba(40, 223, 232, 0.045);
    border-radius: 50%;
    background: repeating-radial-gradient(
      circle,
      transparent 0 80px,
      rgba(40, 223, 232, 0.04) 81px 82px
    );
  }
  .error-radar i {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(240, 72, 94, 0.12), transparent 30deg);
    animation: radar 9s linear infinite;
  }
  .diagnostic {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 7px;
    margin-bottom: 26px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.14em;
    text-align: left;
  }
  .diagnostic span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--red-400);
    box-shadow: 0 0 8px var(--red-400);
  }
  .diagnostic em {
    color: var(--red-400);
    font-style: normal;
  }
  .error-page :global(svg) {
    color: var(--cyan-400);
  }
  .error-code {
    position: absolute;
    top: -35px;
    right: 10px;
    color: rgba(71, 151, 177, 0.1);
    font-family: Rajdhani;
    font-size: 140px;
    font-weight: 700;
  }
  .error-page h1 {
    position: relative;
    margin-top: 25px;
    font-size: 27px;
  }
  .error-page p {
    position: relative;
    margin-bottom: 25px;
    color: var(--steel-300);
    line-height: 1.7;
  }
  .error-page footer {
    position: relative;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 28px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
    color: var(--ink-600);
    font-family: var(--font-mono);
    font-size: 6px;
    letter-spacing: 0.08em;
  }
  @media (max-width: 720px) {
    .error-page {
      min-height: calc(100vh - 60px);
    }
    .error-page section {
      padding: 35px 18px;
    }
  }
</style>
