<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowRight, KeyRound, Radio } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import { gameSnapshot, session } from '$lib/stores';
  import { Button, Field, Surface } from '$lib/ui';

  const code = (page.params.code ?? '').toUpperCase();
  let nickname = '';
  let needsSession = true;
  let loading = true;
  let joining = false;
  let error = '';

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      needsSession = false;
    } catch {
      needsSession = true;
    } finally {
      loading = false;
    }
  });

  async function join() {
    joining = true;
    error = '';
    let sessionCreated = false;
    try {
      if (needsSession) {
        const created = await api.createSession(nickname);
        session.set(created);
        sessionCreated = true;
        trackFunnelReached('session_created');
      }
      const snapshot = await api.joinRoom(code);
      gameSnapshot.set(snapshot);
      trackFunnelReached('room_joined');
      await goto(resolve('/room/[code]', { code: snapshot.room.code }));
    } catch (caught) {
      trackFunnelFailure(
        needsSession && !sessionCreated ? 'session_created' : 'room_joined',
        needsSession && !sessionCreated ? 'session_creation' : 'room_entry'
      );
      error = caught instanceof ApiError ? caught.message : '초대 채널에 접속하지 못했습니다.';
    } finally {
      joining = false;
    }
  }
</script>

<svelte:head><title>작전 초대 {code} · Mk.01</title></svelte:head>

<div class="join-page shell">
  <div class="join-orbit" aria-hidden="true"><i></i><i></i><i></i></div>
  <Surface class="invite-card" tone="elevated" padding="lg">
    <div class="secure-rail"><span></span> SECURE CHANNEL / AES-256 <em>VERIFIED</em></div>
    <div class="invite-icon"><Radio size={28} /></div>
    <p class="eyebrow">ENCRYPTED INVITATION</p>
    <h1>작전 참가 요청</h1>
    <p class="muted">보안 채널을 통해 1:1 해전 작전실로 초대받았습니다.</p>
    <div class="code-block"><small>OPERATION CODE</small><strong>{code}</strong></div>
    {#if loading}
      <div class="spinner"></div>
    {:else}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          join();
        }}
      >
        {#if needsSession}
          <Field
            id="nickname"
            label="지휘관 호출부호"
            bind:value={nickname}
            minlength={2}
            maxlength={16}
            placeholder="호출부호를 입력하십시오"
            autocomplete="nickname"
            required
          />
        {/if}
        {#if error}<p class="join-error" role="alert">{error}</p>{/if}
        <Button
          variant="primary"
          size="lg"
          full
          type="submit"
          loading={joining}
          disabled={joining || (needsSession && nickname.length < 2)}
          ><KeyRound size={17} /> 초대 수락 <ArrowRight size={17} /></Button
        >
      </form>
    {/if}
  </Surface>
</div>

<style>
  .join-page {
    position: relative;
    display: grid;
    min-height: calc(100vh - 68px);
    place-items: center;
    padding-block: 50px;
  }
  :global(.invite-card) {
    position: relative;
    width: min(480px, 100%);
    text-align: center;
    z-index: 2;
  }
  .join-orbit {
    position: absolute;
    width: min(720px, 86vw);
    aspect-ratio: 1;
    border: 1px solid rgba(40, 223, 232, 0.06);
    border-radius: 50%;
    background: repeating-radial-gradient(
      circle,
      transparent 0 78px,
      rgba(40, 223, 232, 0.045) 79px 80px
    );
    animation: radar 26s linear infinite;
  }
  .join-orbit::after {
    position: absolute;
    inset: 50% 50% 0 0;
    content: '';
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(40, 223, 232, 0.14), transparent 30deg);
  }
  .join-orbit i {
    position: absolute;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 12px var(--cyan-300);
  }
  .join-orbit i:nth-child(1) {
    top: 18%;
    left: 33%;
  }
  .join-orbit i:nth-child(2) {
    top: 64%;
    left: 78%;
  }
  .join-orbit i:nth-child(3) {
    top: 74%;
    left: 24%;
  }
  .secure-rail {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 8px;
    margin-bottom: 22px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.12em;
    text-align: left;
  }
  .secure-rail span {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--green-400);
    box-shadow: 0 0 8px var(--green-400);
  }
  .secure-rail em {
    color: var(--green-400);
    font-style: normal;
  }
  .invite-icon {
    display: grid;
    width: 66px;
    height: 66px;
    place-items: center;
    margin: 0 auto 24px;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    color: var(--cyan-400);
    background: rgba(22, 199, 217, 0.08);
    box-shadow: 0 0 40px rgba(22, 199, 217, 0.08);
  }
  :global(.invite-card) h1 {
    font-family: Rajdhani, sans-serif;
    font-size: 32px;
  }
  .code-block {
    display: grid;
    gap: 3px;
    margin: 28px 0;
    padding: 17px;
    border: 1px dashed rgba(57, 224, 235, 0.34);
    border-radius: 10px;
    background: rgba(1, 12, 20, 0.68);
  }
  .code-block small {
    color: #6d899a;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.2em;
  }
  .code-block strong {
    color: var(--cyan-200);
    font-family: Rajdhani;
    font-size: 28px;
    letter-spacing: 0.22em;
  }
  :global(.invite-card) form {
    display: grid;
    gap: 18px;
    text-align: left;
  }
  :global(.invite-card) form :global(.ui-field) {
    text-align: left;
  }
  .join-error {
    margin: 0;
    color: #ff9eaa;
    font-size: 12px;
  }
  .spinner {
    margin: 30px auto;
  }
  @media (max-width: 600px) {
    .join-page {
      min-height: calc(100vh - 60px);
      padding-block: 25px;
    }
    :global(.invite-card) {
      width: 100%;
    }
  }
</style>
