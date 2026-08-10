<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
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
  import { Badge, Button, Field, Surface } from '$lib/ui';

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
      await goto(resolve('/lobby'));
      return;
    }
    submitting = true;
    try {
      const created = await api.createSession(nickname);
      session.set(created);
      await goto(resolve('/lobby'));
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
    <div class="hero__status-line">
      <Badge tone="success" pulse>LIVE COMMAND NETWORK</Badge>
      <span>2 COMMANDERS · 10×10 SECTORS · ZERO INTEL LEAK</span>
    </div>

    <p class="eyebrow">MK.01 / REAL-TIME NAVAL WARFARE</p>
    <h1 class="display-title">보이지 않는 함대를,<br /><span>좌표 위에서 지휘하십시오.</span></h1>
    <p class="hero__lead">
      다섯 척의 함대를 숨기고 한 칸의 정보로 전장을 재구성하십시오. 당신의 선택은 숨겨지고, 모든
      판정은 서버에서 증명됩니다.
    </p>

    <Surface tone="elevated" padding="md" class="command-entry">
      <form
        onsubmit={(event) => {
          event.preventDefault();
          enterLobby();
        }}
      >
        <div class="command-entry__head">
          <div class="command-symbol"><Crosshair size={17} /></div>
          <div>
            <small>COMMANDER AUTHORIZATION</small>
            <strong>{existingSession ? '작전 세션 확인됨' : '지휘관 호출부호 등록'}</strong>
          </div>
          <span>{existingSession ? 'RESUME' : 'GUEST ACCESS'}</span>
        </div>
        <div class="command-entry__controls">
          <Field
            id="nickname"
            label="닉네임"
            bind:value={nickname}
            minlength={2}
            maxlength={16}
            autocomplete="nickname"
            placeholder="호출부호 입력 (2~16자)"
            disabled={existingSession || submitting}
            {error}
            required
          />
          <Button variant="primary" size="lg" type="submit" loading={submitting}>
            {existingSession ? '작전 복귀' : '작전 로비 입장'}
            <ArrowRight size={18} />
          </Button>
        </div>
      </form>
    </Surface>

    <div class="trust-row" aria-label="보안 특성">
      <span
        ><ShieldCheck size={15} /><strong>서버 권위 판정</strong><small>AUTHORITATIVE</small></span
      >
      <span><LockKeyhole size={15} /><strong>비공개 함선 좌표</strong><small>ENCRYPTED</small></span
      >
      <span><Radio size={15} /><strong>실시간 재접속</strong><small>RESILIENT</small></span>
    </div>
  </div>

  <div class="command-visual" aria-hidden="true">
    <div class="visual-coordinates visual-coordinates--top">43°37' N / 128°28' E</div>
    <div class="visual-coordinates visual-coordinates--side">SECTOR 07-N</div>
    <div class="radar-shell">
      <div class="radar-horizon"></div>
      <div class="radar-grid"></div>
      <div class="radar-ring radar-ring--one"></div>
      <div class="radar-ring radar-ring--two"></div>
      <div class="radar-cross radar-cross--x"></div>
      <div class="radar-cross radar-cross--y"></div>
      <div class="radar-sweep"></div>
      <div class="radar-origin"><Crosshair size={26} strokeWidth={1} /></div>
      <span class="contact contact--one"><i></i><em>TGT-04</em></span>
      <span class="contact contact--two"><i></i><em>TGT-09</em></span>
      <span class="contact contact--three"><i></i><em>TGT-12</em></span>
      <div class="fleet-trace fleet-trace--one"><i></i><i></i><i></i><i></i><i></i></div>
      <div class="fleet-trace fleet-trace--two"><i></i><i></i><i></i></div>
    </div>
    <div class="telemetry-card telemetry-card--top">
      <small>SONAR ARRAY</small><strong>ACTIVE</strong><span>SCAN RATE 04.8s</span>
    </div>
    <div class="telemetry-card telemetry-card--bottom">
      <small>OCEAN DEPTH</small><strong>4,218 m</strong><span>THERMAL LAYER STABLE</span>
    </div>
    <div class="visual-index"><span>01</span><i></i><span>04</span></div>
  </div>
</section>

<section class="mission-brief shell" aria-labelledby="mission-title">
  <header class="mission-brief__heading">
    <div>
      <p class="eyebrow">MISSION PROTOCOL</p>
      <h2 id="mission-title">세 단계의 정밀한 교전</h2>
    </div>
    <p>배치, 추론, 격침. 규칙은 간결하지만 모든 좌표에는 의도가 필요합니다.</p>
  </header>
  <div class="mission-grid">
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">01</span><Waves size={22} /><small>DEPLOY</small>
        <h3>은밀 배치</h3>
        <p>다섯 척의 함대를 해역 안에 자유롭게 편성합니다. 좌표는 오직 당신만 봅니다.</p>
      </article>
    </Surface>
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">02</span><Crosshair size={22} /><small>DEDUCE</small>
        <h3>좌표 추론</h3>
        <p>명중과 빗나감의 패턴을 읽고 적 함대의 형태를 좌표 위에 재구성합니다.</p>
      </article>
    </Surface>
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">03</span><RotateCw size={22} /><small>ENDURE</small>
        <h3>끊김 없는 작전</h3>
        <p>순간적인 통신 두절이나 새로고침 후에도 같은 턴과 같은 전장으로 복귀합니다.</p>
      </article>
    </Surface>
  </div>
</section>

<style>
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1.04fr) minmax(440px, 0.96fr);
    gap: clamp(48px, 7vw, 112px);
    align-items: center;
    min-height: min(890px, calc(100vh - 72px));
    padding-block: 64px 72px;
  }
  .hero__copy {
    position: relative;
    z-index: 2;
  }
  .hero__status-line {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 42px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }
  .display-title {
    max-width: 830px;
  }
  .display-title span {
    color: transparent;
    background: linear-gradient(110deg, #ecffff 8%, #74f7f7 48%, #2ba6e9 96%);
    background-clip: text;
    filter: drop-shadow(0 10px 32px rgba(20, 198, 213, 0.1));
  }
  .hero__lead {
    max-width: 700px;
    margin: 32px 0;
    color: var(--ink-300);
    font-size: clamp(14px, 1.4vw, 17px);
    line-height: 1.9;
    word-break: keep-all;
  }
  :global(.command-entry) {
    max-width: 720px;
  }
  :global(.command-entry) form {
    display: grid;
    gap: 20px;
  }
  .command-entry__head {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--line);
  }
  .command-symbol {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .command-entry__head > div:nth-child(2) {
    display: grid;
    gap: 3px;
  }
  .command-entry__head small,
  .command-entry__head > span {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .command-entry__head strong {
    font-size: 12px;
  }
  .command-entry__head > span {
    color: var(--green-400);
  }
  .command-entry__controls {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 12px;
  }
  .trust-row {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    margin-top: 24px;
  }
  .trust-row span {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 1px 7px;
    color: var(--cyan-400);
  }
  .trust-row :global(svg) {
    grid-row: 1 / 3;
  }
  .trust-row strong {
    color: var(--ink-300);
    font-size: 10px;
  }
  .trust-row small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .command-visual {
    position: relative;
    display: grid;
    place-items: center;
    aspect-ratio: 1;
  }
  .command-visual::before {
    position: absolute;
    inset: 7%;
    content: '';
    border-radius: 50%;
    background: radial-gradient(circle, rgba(15, 154, 178, 0.15), transparent 68%);
    filter: blur(26px);
  }
  .radar-shell {
    position: relative;
    width: min(100%, 600px);
    aspect-ratio: 1;
    overflow: hidden;
    border: 1px solid rgba(79, 222, 231, 0.28);
    border-radius: 50%;
    background: radial-gradient(circle, rgba(10, 65, 80, 0.46), rgba(2, 13, 20, 0.94) 67%);
    box-shadow:
      inset 0 0 100px rgba(22, 186, 204, 0.07),
      0 0 100px rgba(3, 123, 150, 0.08);
  }
  .radar-shell::after {
    position: absolute;
    inset: 4%;
    content: '';
    border: 1px solid rgba(57, 214, 226, 0.08);
    border-radius: 50%;
  }
  .radar-grid {
    position: absolute;
    inset: 0;
    opacity: 0.38;
    background-image:
      linear-gradient(rgba(54, 190, 208, 0.1) 1px, transparent 1px),
      linear-gradient(90deg, rgba(54, 190, 208, 0.1) 1px, transparent 1px);
    background-size: 10% 10%;
    mask-image: radial-gradient(circle, black, transparent 74%);
  }
  .radar-horizon {
    position: absolute;
    inset: 12%;
    border: 1px dashed rgba(73, 210, 220, 0.1);
    border-radius: 50%;
  }
  .radar-ring {
    position: absolute;
    inset: 24%;
    border: 1px solid rgba(73, 210, 220, 0.14);
    border-radius: 50%;
  }
  .radar-ring--two {
    inset: 39%;
  }
  .radar-cross {
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    height: 1px;
    background: rgba(73, 210, 220, 0.16);
  }
  .radar-cross--y {
    transform: rotate(90deg);
  }
  .radar-sweep {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(
      from 270deg at 100% 0,
      rgba(55, 230, 234, 0.34),
      rgba(55, 230, 234, 0.03) 24deg,
      transparent 48deg
    );
    animation: radar 6s linear infinite;
  }
  .radar-origin {
    position: absolute;
    z-index: 3;
    inset: 50% auto auto 50%;
    display: grid;
    width: 52px;
    height: 52px;
    place-items: center;
    border: 1px solid rgba(93, 242, 244, 0.2);
    border-radius: 50%;
    color: var(--cyan-200);
    background: rgba(4, 24, 33, 0.82);
    box-shadow: 0 0 28px rgba(40, 223, 232, 0.13);
    transform: translate(-50%, -50%);
  }
  .contact {
    position: absolute;
    z-index: 4;
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 5px;
  }
  .contact i {
    width: 7px;
    height: 7px;
    border: 1px solid #c9ffff;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 14px var(--cyan-300);
    animation: pulse 1.6s infinite;
  }
  .contact em {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 7px;
    font-style: normal;
    letter-spacing: 0.08em;
  }
  .contact--one {
    top: 29%;
    left: 63%;
  }
  .contact--two {
    top: 66%;
    left: 31%;
  }
  .contact--three {
    top: 48%;
    left: 77%;
  }
  .fleet-trace {
    position: absolute;
    z-index: 2;
    display: flex;
    gap: 2px;
    transform: rotate(-28deg);
  }
  .fleet-trace i {
    width: 13px;
    height: 6px;
    border: 1px solid rgba(137, 230, 235, 0.35);
    background: rgba(53, 142, 157, 0.38);
  }
  .fleet-trace--one {
    top: 38%;
    left: 25%;
  }
  .fleet-trace--two {
    right: 26%;
    bottom: 30%;
    transform: rotate(36deg);
  }
  .telemetry-card {
    position: absolute;
    z-index: 5;
    display: grid;
    gap: 3px;
    min-width: 144px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-left-color: var(--cyan-300);
    border-radius: 2px 10px 10px 2px;
    background: rgba(3, 16, 24, 0.86);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(14px);
  }
  .telemetry-card small,
  .visual-coordinates,
  .visual-index {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.17em;
  }
  .telemetry-card strong {
    font-family: var(--font-display);
    font-size: 15px;
    letter-spacing: 0.08em;
  }
  .telemetry-card span {
    color: var(--ink-400);
    font-size: 7px;
  }
  .telemetry-card--top {
    top: 12%;
    right: -2%;
  }
  .telemetry-card--bottom {
    bottom: 13%;
    left: -1%;
  }
  .visual-coordinates {
    position: absolute;
    z-index: 6;
  }
  .visual-coordinates--top {
    top: 0;
    left: 50%;
    transform: translateX(-50%);
  }
  .visual-coordinates--side {
    top: 50%;
    right: -34px;
    transform: rotate(90deg);
  }
  .visual-index {
    position: absolute;
    bottom: 1%;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 7px;
    transform: translateX(-50%);
  }
  .visual-index i {
    width: 56px;
    height: 1px;
    background: var(--line-strong);
  }
  .mission-brief {
    padding-block: 48px 112px;
  }
  .mission-brief__heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 40px;
    margin-bottom: 28px;
  }
  .mission-brief__heading h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: clamp(30px, 4vw, 46px);
    font-weight: 600;
  }
  .mission-brief__heading > p {
    max-width: 440px;
    margin-bottom: 4px;
    color: var(--ink-300);
    font-size: 13px;
    line-height: 1.8;
  }
  .mission-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
  }
  .mission-grid article {
    position: relative;
    min-height: 220px;
  }
  .mission-grid :global(svg) {
    color: var(--cyan-300);
  }
  .mission-grid article > small {
    display: block;
    margin: 25px 0 3px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.16em;
  }
  .mission-grid h3 {
    margin-bottom: 11px;
    font-size: 18px;
  }
  .mission-grid p {
    margin: 0;
    color: var(--ink-300);
    font-size: 12px;
    line-height: 1.8;
  }
  .mission-number {
    position: absolute;
    top: -8px;
    right: 0;
    color: rgba(110, 185, 201, 0.17);
    font-family: var(--font-display);
    font-size: 42px;
    font-weight: 700;
  }
  @media (max-width: 1040px) {
    .hero {
      grid-template-columns: 1fr;
      padding-top: 72px;
    }
    .hero__copy {
      max-width: 820px;
    }
    .command-visual {
      width: min(650px, 100%);
      margin-inline: auto;
    }
  }
  @media (max-width: 720px) {
    .hero {
      gap: 48px;
      min-height: auto;
      padding-block: 48px 56px;
    }
    .hero__status-line {
      display: block;
      margin-bottom: 32px;
    }
    .hero__status-line > span {
      display: none;
    }
    .hero__lead {
      margin-block: 24px;
      font-size: 14px;
    }
    .command-entry__controls {
      grid-template-columns: 1fr;
    }
    .command-entry__controls :global(.ui-button) {
      width: 100%;
    }
    .trust-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 14px;
    }
    .trust-row span:last-child {
      grid-column: 1 / -1;
    }
    .command-visual {
      width: 94%;
    }
    .telemetry-card {
      min-width: 122px;
      padding: 9px 10px;
    }
    .telemetry-card--top {
      right: -4%;
    }
    .telemetry-card--bottom {
      left: -4%;
    }
    .visual-coordinates--side {
      display: none;
    }
    .mission-brief {
      padding-block: 24px 80px;
    }
    .mission-brief__heading {
      display: block;
    }
    .mission-brief__heading > p {
      margin-top: 16px;
    }
    .mission-grid {
      grid-template-columns: 1fr;
    }
    .mission-grid article {
      min-height: auto;
    }
  }
  .hero { max-width: 1520px; min-height: min(900px, calc(100vh - 72px)); }
  .hero::before { position: absolute; z-index: -1; top: 22%; right: -14%; bottom: 0; left: -14%; content: ''; opacity: .26; pointer-events: none; background: radial-gradient(ellipse at center, rgba(42, 140, 151, .16), transparent 65%); }
  .hero__status-line { color: var(--ink-500); letter-spacing: .16em; }
  .display-title { font-family: var(--font-display); font-size: clamp(52px, 6.8vw, 94px); line-height: .98; letter-spacing: .01em; }
  .hero__lead { max-width: 620px; color: var(--ink-300); }
  :global(.command-entry) { border-radius: 8px 3px 8px 3px; border-color: rgba(83, 233, 232, .28); background: linear-gradient(145deg, rgba(8, 30, 38, .86), rgba(2, 13, 20, .94)); }
  .command-entry__head { border-bottom-color: var(--line); }
  .command-symbol { border-radius: 50%; color: var(--tactical); background: rgba(83, 233, 232, .08); }
  .command-entry__head > span { color: var(--safe); }
  .command-visual { filter: saturate(.84); }
  .radar-shell { border-radius: 10px 3px 10px 3px; clip-path: polygon(3% 0, 97% 0, 100% 3%, 100% 97%, 97% 100%, 3% 100%, 0 97%, 0 3%); background: radial-gradient(circle at 50% 48%, rgba(8, 78, 88, .32), rgba(2, 13, 20, .96) 67%); }
  .radar-shell::before { position: absolute; inset: 0; content: ''; opacity: .22; background: repeating-linear-gradient(165deg, transparent 0 8px, rgba(93, 191, 198, .035) 9px 10px); }
  .telemetry-card { border-radius: 3px; border-color: var(--line); background: rgba(2, 13, 20, .86); }
  .mission-brief { padding-top: 22px; }
  .mission-grid { gap: 12px; }
  :global(.mission-grid .ui-surface) { border-radius: 7px 2px 7px 2px; border-color: var(--line); background: linear-gradient(145deg, rgba(7, 28, 36, .82), rgba(2, 13, 20, .86)); }
  :global(.mission-grid .ui-surface:hover) { border-color: var(--line-active); }
  @media (max-width: 820px) { .hero { min-height: auto; } .display-title { font-size: clamp(49px, 12vw, 74px); } }
</style>
