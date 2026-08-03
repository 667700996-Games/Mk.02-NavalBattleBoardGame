<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import {
    ArrowRight,
    Crosshair,
    LockKeyhole,
    Radio,
    RotateCw,
    ShieldCheck,
    Waves
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { session } from '$lib/stores';

  let nickname = '';
  let submitting = false;
  let error = '';
  let existingSession = false;

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      nickname = current.nickname;
      existingSession = true;
    } catch {
      existingSession = false;
    }
  });

  async function enterLobby() {
    error = '';
    if (existingSession) {
      await goto('/lobby');
      return;
    }
    submitting = true;
    try {
      const created = await api.createSession(nickname);
      session.set(created);
      await goto('/lobby');
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '지휘관 등록에 실패했습니다.';
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:head><title>Mk.01 — 실시간 온라인 해전</title></svelte:head>

<section class="hero shell">
  <div class="hero__copy">
    <div class="eyebrow">REAL-TIME NAVAL STRATEGY · MK.01</div>
    <h1 class="display-title">보이지 않는 함대를<br /><span>먼저 찾아내십시오.</span></h1>
    <p class="hero__lead">
      두 명의 지휘관, 두 개의 10×10 해역. 함대를 은밀히 배치하고 한 좌표씩 교전하여
      상대 전력을 먼저 무력화하는 실시간 전략 게임입니다.
    </p>

    <form class="command-entry panel" onsubmit={(event) => { event.preventDefault(); enterLobby(); }}>
      <div class="command-entry__heading">
        <span class="live-dot"></span>
        <div>
          <strong>{existingSession ? '작전 세션 확인됨' : '지휘관 호출부호 등록'}</strong>
          <small>{existingSession ? `${nickname} 지휘관으로 복귀합니다.` : '회원가입 없이 즉시 작전을 시작합니다.'}</small>
        </div>
      </div>
      <div class="command-entry__controls">
        <label class="sr-only" for="nickname">닉네임</label>
        <input
          id="nickname"
          class="input"
          bind:value={nickname}
          maxlength="16"
          minlength="2"
          autocomplete="nickname"
          placeholder="호출부호 입력 (2~16자)"
          disabled={existingSession || submitting}
          required
        />
        <button class="button button--primary" type="submit" disabled={submitting}>
          {submitting ? '접속 중…' : existingSession ? '작전 복귀' : '작전 로비 입장'}
          <ArrowRight size={18} />
        </button>
      </div>
      {#if error}<p class="form-error" role="alert">{error}</p>{/if}
    </form>

    <div class="trust-row">
      <span><ShieldCheck size={15} /> 서버 판정</span>
      <span><LockKeyhole size={15} /> 비공개 함선 좌표</span>
      <span><Radio size={15} /> 실시간 재접속</span>
    </div>
  </div>

  <div class="radar-scene" aria-hidden="true">
    <div class="radar">
      <div class="radar__grid"></div>
      <div class="radar__sweep"></div>
      <div class="radar__ring radar__ring--one"></div>
      <div class="radar__ring radar__ring--two"></div>
      <span class="radar__contact contact--one"></span>
      <span class="radar__contact contact--two"></span>
      <span class="radar__contact contact--three"></span>
      <div class="radar__bearing"><Crosshair size={24} strokeWidth={1} /></div>
    </div>
    <div class="telemetry telemetry--top"><small>SECTOR</small><strong>07-N</strong></div>
    <div class="telemetry telemetry--bottom"><small>SONAR</small><strong>ACTIVE</strong></div>
  </div>
</section>

<section class="intel shell" aria-labelledby="intel-title">
  <div class="intel__heading">
    <div>
      <p class="eyebrow">MISSION PROTOCOL</p>
      <h2 id="intel-title">세 단계로 완성되는 교전</h2>
    </div>
    <p>직관적인 규칙 위에 추론, 기억, 심리전을 더했습니다.</p>
  </div>
  <div class="intel-grid">
    <article>
      <span class="intel-number">01</span><Waves size={23} />
      <h3>은밀 배치</h3>
      <p>다섯 척의 함대를 직접 또는 자동으로 배치합니다. 위치는 오직 서버와 자신만 압니다.</p>
    </article>
    <article>
      <span class="intel-number">02</span><Crosshair size={23} />
      <h3>좌표 교전</h3>
      <p>턴마다 한 좌표를 선택하고 확인합니다. 명중과 빗나감 기록으로 적의 형태를 좁힙니다.</p>
    </article>
    <article>
      <span class="intel-number">03</span><RotateCw size={23} />
      <h3>끝까지 연결</h3>
      <p>새로고침이나 짧은 통신 두절 뒤에도 동일한 전장과 턴으로 안전하게 복귀합니다.</p>
    </article>
  </div>
</section>

<style>
  .hero { display: grid; grid-template-columns: minmax(0, 1.08fr) minmax(380px, .92fr); gap: 70px; align-items: center; min-height: min(790px, calc(100vh - 68px)); padding-block: 72px; }
  .hero__copy { position: relative; z-index: 1; }
  .display-title span { color: transparent; background: linear-gradient(105deg, #e8fcff, #39e0eb 65%, #238ee9); background-clip: text; }
  .hero__lead { max-width: 670px; margin: 26px 0 32px; color: #9eb7c5; font-size: clamp(15px, 1.7vw, 18px); line-height: 1.8; }
  .command-entry { max-width: 690px; padding: 20px; border-color: rgba(57,224,235,.22); }
  .command-entry__heading { display: flex; align-items: center; gap: 11px; margin-bottom: 15px; }
  .command-entry__heading div { display: grid; gap: 3px; }
  .command-entry__heading strong { font-size: 13px; }
  .command-entry__heading small { color: #7894a5; font-size: 11px; }
  .live-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--green-500); box-shadow: 0 0 12px var(--green-500); animation: pulse 1.7s ease-in-out infinite; }
  .command-entry__controls { display: grid; grid-template-columns: 1fr auto; gap: 10px; }
  .form-error { margin: 10px 0 0; color: #ff98a5; font-size: 12px; }
  .trust-row { display: flex; flex-wrap: wrap; gap: 18px; margin-top: 20px; color: #7894a5; font-size: 11px; }
  .trust-row span { display: inline-flex; align-items: center; gap: 6px; }
  .radar-scene { position: relative; display: grid; place-items: center; aspect-ratio: 1; }
  .radar { position: relative; width: min(100%, 510px); aspect-ratio: 1; overflow: hidden; border: 1px solid rgba(57,224,235,.3); border-radius: 50%; background: radial-gradient(circle, rgba(15,82,102,.34), rgba(3,16,25,.9) 66%); box-shadow: inset 0 0 90px rgba(15,211,222,.07), 0 0 100px rgba(7,118,148,.09); }
  .radar::before, .radar::after { position: absolute; inset: 50% 0 auto; height: 1px; content: ''; background: rgba(82,210,224,.17); }
  .radar::after { transform: rotate(90deg); }
  .radar__grid { position: absolute; inset: 0; opacity: .28; background-image: linear-gradient(rgba(47,170,190,.13) 1px, transparent 1px), linear-gradient(90deg, rgba(47,170,190,.13) 1px, transparent 1px); background-size: 10% 10%; }
  .radar__sweep { position: absolute; inset: 50% 50% 0 0; transform-origin: 100% 0; background: conic-gradient(from 270deg at 100% 0, rgba(57,224,235,.32), transparent 32deg); animation: radar 5s linear infinite; }
  .radar__ring { position: absolute; inset: 16%; border: 1px solid rgba(57,224,235,.17); border-radius: 50%; }
  .radar__ring--two { inset: 33%; }
  .radar__contact { position: absolute; width: 7px; height: 7px; border: 1px solid #9dffff; border-radius: 50%; background: var(--cyan-400); box-shadow: 0 0 18px var(--cyan-400); }
  .contact--one { top: 29%; left: 63%; }.contact--two { top: 67%; left: 34%; }.contact--three { top: 51%; left: 78%; }
  .radar__bearing { position: absolute; inset: 50% auto auto 50%; display: grid; width: 40px; height: 40px; place-items: center; color: var(--cyan-200); transform: translate(-50%,-50%); }
  .telemetry { position: absolute; display: grid; gap: 2px; padding: 10px 13px; border-left: 2px solid var(--cyan-400); background: rgba(4,18,28,.86); box-shadow: 0 12px 36px rgba(0,0,0,.3); }
  .telemetry small { color: #7395a6; font-family: Rajdhani; font-size: 9px; letter-spacing: .2em; }.telemetry strong { font-family: Rajdhani; letter-spacing: .12em; }.telemetry--top { top: 14%; right: 0; }.telemetry--bottom { bottom: 15%; left: 1%; }
  .intel { padding: 50px 0 110px; }
  .intel__heading { display: flex; align-items: end; justify-content: space-between; gap: 30px; margin-bottom: 28px; }
  .intel__heading h2 { margin: 0; font-family: Rajdhani, sans-serif; font-size: clamp(28px,4vw,42px); }.intel__heading > p { max-width: 370px; margin-bottom: 5px; color: var(--steel-300); }
  .intel-grid { display: grid; grid-template-columns: repeat(3,1fr); border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; background: rgba(5,18,28,.72); }
  .intel-grid article { position: relative; min-height: 235px; padding: 30px; border-right: 1px solid var(--line); }.intel-grid article:last-child { border: 0; }.intel-grid svg { color: var(--cyan-400); }
  .intel-number { position: absolute; top: 23px; right: 26px; color: rgba(110,164,184,.25); font-family: Rajdhani; font-size: 34px; font-weight: 700; }.intel-grid h3 { margin: 32px 0 10px; font-size: 18px; }.intel-grid p { margin: 0; color: #819dad; font-size: 13px; line-height: 1.8; }
  @media (max-width: 900px) { .hero { grid-template-columns: 1fr; min-height: auto; padding-block: 70px 40px; }.radar-scene { width: min(520px,100%); margin-inline: auto; }.intel-grid { grid-template-columns: 1fr; }.intel-grid article { min-height: auto; border-right: 0; border-bottom: 1px solid var(--line); } }
  @media (max-width: 600px) { .hero { gap: 44px; padding-top: 50px; }.command-entry__controls { grid-template-columns: 1fr; }.command-entry__controls .button { width: 100%; }.radar { width: 92%; }.telemetry--top { right: 0; }.intel__heading { display: block; }.intel__heading > p { margin-top: 15px; } }
</style>

