<script lang="ts">
  import { Radio, RefreshCw } from '@lucide/svelte';
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
    <p class="eyebrow">SIGNAL INTERRUPTED</p>
    <h2 id="disconnect-title">상대 지휘관 재접속 대기</h2>
    <p>전장 상태와 현재 턴은 서버에 안전하게 보존되어 있습니다.</p>
    <strong class="countdown">{remaining}<small>SEC</small></strong><span
      ><RefreshCw size={13} /> 연결 상태를 자동으로 확인하고 있습니다.</span
    >
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
