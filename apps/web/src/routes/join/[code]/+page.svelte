<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { ArrowRight, KeyRound, Radio } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { gameSnapshot, session } from '$lib/stores';

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
    try {
      if (needsSession) {
        const created = await api.createSession(nickname);
        session.set(created);
      }
      const snapshot = await api.joinRoom(code);
      gameSnapshot.set(snapshot);
      await goto(`/room/${snapshot.room.code}`);
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '초대 채널에 접속하지 못했습니다.';
    } finally {
      joining = false;
    }
  }
</script>

<svelte:head><title>작전 초대 {code} · Mk.01</title></svelte:head>

<div class="join-page shell">
  <section class="invite-card panel">
    <div class="invite-icon"><Radio size={28} /></div>
    <p class="eyebrow">ENCRYPTED INVITATION</p>
    <h1>작전 참가 요청</h1>
    <p class="muted">보안 채널을 통해 1:1 해전 작전실로 초대받았습니다.</p>
    <div class="code-block"><small>OPERATION CODE</small><strong>{code}</strong></div>
    {#if loading}
      <div class="spinner"></div>
    {:else}
      <form onsubmit={(event) => { event.preventDefault(); join(); }}>
        {#if needsSession}
          <div class="field"><label for="nickname">지휘관 호출부호</label><input id="nickname" class="input" bind:value={nickname} minlength="2" maxlength="16" placeholder="닉네임 입력" required /></div>
        {/if}
        {#if error}<p class="join-error" role="alert">{error}</p>{/if}
        <button class="button button--primary button--wide" type="submit" disabled={joining || (needsSession && nickname.length < 2)}><KeyRound size={17} /> {joining ? '접속 중…' : '초대 수락'} <ArrowRight size={17} /></button>
      </form>
    {/if}
  </section>
</div>

<style>
  .join-page { display:grid; min-height:calc(100vh - 68px); place-items:center; padding-block:50px; }.invite-card { width:min(480px,100%); padding:38px; text-align:center; }.invite-icon { display:grid; width:66px; height:66px; place-items:center; margin:0 auto 24px; border:1px solid var(--line-strong); border-radius:50%; color:var(--cyan-400); background:rgba(22,199,217,.08); box-shadow:0 0 40px rgba(22,199,217,.08); }.invite-card h1 { font-family:Rajdhani,sans-serif; font-size:32px; }.code-block { display:grid; gap:3px; margin:28px 0; padding:17px; border:1px dashed rgba(57,224,235,.34); border-radius:10px; background:rgba(1,12,20,.68); }.code-block small { color:#6d899a; font-family:Rajdhani; font-size:9px; letter-spacing:.2em; }.code-block strong { color:var(--cyan-200); font-family:Rajdhani; font-size:28px; letter-spacing:.22em; }.invite-card form { display:grid; gap:18px; text-align:left; }.join-error { margin:0; color:#ff9eaa; font-size:12px; }.spinner { margin:30px auto; }
  @media(max-width:600px){.join-page{min-height:calc(100vh - 60px);padding-block:25px}.invite-card{padding:30px 20px}}
</style>
