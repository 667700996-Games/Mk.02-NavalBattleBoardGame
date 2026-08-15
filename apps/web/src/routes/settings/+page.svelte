<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Contrast, Gauge, LogOut, ShieldCheck, Volume2 } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, preferences, session } from '$lib/stores';
  import { sounds } from '$lib/sound';

  let signingOut = $state(false);
  let logoutError = $state('');

  async function signOut() {
    signingOut = true;
    logoutError = '';
    try {
      await api.deleteCurrentSession();
      realtime.disconnect();
      gameSnapshot.set(null);
      session.set(null);
      await goto(resolve('/'));
    } catch (caught) {
      logoutError =
        caught instanceof ApiError
          ? caught.message
          : '세션을 종료하지 못했습니다. 다시 시도해 주세요.';
    } finally {
      signingOut = false;
    }
  }
</script>

<svelte:head><title>환경 설정 · Mk.01</title></svelte:head>
<div class="settings-page shell">
  <header>
    <p class="eyebrow">SYSTEM PREFERENCES</p>
    <h1 class="page-title">환경 설정</h1>
    <p>이 장치에만 적용되는 표시와 사운드 옵션입니다.</p>
  </header>
  <div class="settings-layout">
    <aside class="system-profile panel" aria-label="시스템 프로필">
      <div class="profile-radar"><i></i><span></span></div>
      <p class="eyebrow">LOCAL CONTROL PROFILE</p>
      <h2>COMMAND DISPLAY</h2>
      <p>이 장치의 작전 인터페이스를 지휘 환경에 맞게 조정하십시오.</p>
      <dl>
        <div>
          <dt>RENDER MODE</dt>
          <dd>TACTICAL / WEB</dd>
        </div>
        <div>
          <dt>SECURITY</dt>
          <dd>SERVER AUTHORITATIVE</dd>
        </div>
        <div>
          <dt>PROFILE SCOPE</dt>
          <dd>THIS DEVICE</dd>
        </div>
      </dl>
    </aside>
    <div class="settings-main">
      <section class="settings-panel panel">
        <div class="setting-row">
          <span class="setting-icon"><Volume2 size={20} /></span>
          <div>
            <strong>작전 사운드</strong>
            <p>좌표 선택, 명중, 격침, 승리 신호음을 재생합니다.</p>
          </div>
          <label class="switch"
            ><input
              type="checkbox"
              bind:checked={$preferences.sound}
              onchange={() => $preferences.sound && sounds.select()}
            /><span></span><em>{$preferences.sound ? '켜짐' : '꺼짐'}</em></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Gauge size={20} /></span>
          <div>
            <strong>동작 줄이기</strong>
            <p>레이더 회전과 전투 효과 등 비필수 애니메이션을 최소화합니다.</p>
          </div>
          <label class="switch"
            ><input type="checkbox" bind:checked={$preferences.reducedMotion} /><span></span><em
              >{$preferences.reducedMotion ? '켜짐' : '꺼짐'}</em
            ></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Contrast size={20} /></span>
          <div>
            <strong>고대비 모드</strong>
            <p>격자선과 텍스트의 대비를 높여 전장 정보를 더 명확하게 표시합니다.</p>
          </div>
          <label class="switch"
            ><input type="checkbox" bind:checked={$preferences.highContrast} /><span></span><em
              >{$preferences.highContrast ? '켜짐' : '꺼짐'}</em
            ></label
          >
        </div>
      </section>
      <aside class="security-note">
        <ShieldCheck size={18} />
        <div>
          <strong>공정한 전장을 위한 서버 검증</strong>
          <p>
            표시 설정은 게임 판정에 영향을 주지 않습니다. 함선 위치, 공격, 턴, 승패는 서버에서만
            검증됩니다.
          </p>
        </div>
      </aside>
      {#if $session}
        <section class="session-panel panel" aria-labelledby="session-control-title">
          <div>
            <small>SESSION CONTROL</small>
            <strong id="session-control-title">이 장치의 지휘 세션</strong>
            <p>로그아웃하면 서버의 인증 세션도 즉시 폐기되며 다시 사용할 수 없습니다.</p>
            {#if logoutError}<p class="session-error" role="alert">{logoutError}</p>{/if}
          </div>
          <button
            class="button button--danger"
            type="button"
            onclick={signOut}
            disabled={signingOut}
          >
            <LogOut size={16} />
            {signingOut ? '세션 종료 중…' : '로그아웃 및 세션 폐기'}
          </button>
        </section>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings-page {
    padding: 64px 0 100px;
  }
  .settings-page header {
    margin-bottom: 28px;
  }
  .settings-page header h1 {
    margin-bottom: 7px;
  }
  .settings-page header > p:last-child {
    color: var(--steel-300);
  }
  .settings-panel {
    overflow: hidden;
  }
  .settings-layout {
    display: grid;
    grid-template-columns: 310px minmax(0, 1fr);
    gap: 18px;
    align-items: start;
  }
  .settings-main {
    min-width: 0;
  }
  .session-panel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    margin-top: 14px;
    padding: 20px;
  }
  .session-panel div {
    display: grid;
    gap: 5px;
  }
  .session-panel small {
    color: var(--critical);
    font: 700 8px var(--font-display);
    letter-spacing: 0.14em;
  }
  .session-panel p {
    margin: 0;
    color: var(--ink-400);
    font-size: 10px;
  }
  .session-panel .session-error {
    color: var(--critical);
  }
  .system-profile {
    position: sticky;
    top: 92px;
    padding: 26px;
    overflow: hidden;
    background:
      radial-gradient(circle at 50% 14%, rgba(40, 223, 232, 0.1), transparent 34%),
      rgba(5, 18, 28, 0.85);
  }
  .profile-radar {
    position: relative;
    width: 136px;
    height: 136px;
    margin: 0 auto 26px;
    overflow: hidden;
    border: 1px solid rgba(40, 223, 232, 0.2);
    border-radius: 50%;
    background:
      linear-gradient(rgba(40, 223, 232, 0.08) 1px, transparent 1px),
      linear-gradient(90deg, rgba(40, 223, 232, 0.08) 1px, transparent 1px),
      repeating-radial-gradient(circle, transparent 0 21px, rgba(40, 223, 232, 0.09) 22px 23px);
    background-size:
      34px 34px,
      34px 34px,
      auto;
  }
  .profile-radar i {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(from 270deg at 100% 0, rgba(40, 223, 232, 0.4), transparent 36deg);
    animation: radar 3s linear infinite;
  }
  .profile-radar span {
    position: absolute;
    top: 35%;
    left: 68%;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 10px var(--cyan-300);
  }
  .system-profile h2 {
    margin: 4px 0 8px;
    font-size: 20px;
  }
  .system-profile > p:not(.eyebrow) {
    color: var(--ink-400);
    font-size: 10px;
    line-height: 1.7;
  }
  .system-profile dl {
    display: grid;
    gap: 10px;
    margin: 22px 0 0;
  }
  .system-profile dl div {
    display: grid;
    gap: 3px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .system-profile dt {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.15em;
  }
  .system-profile dd {
    margin: 0;
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 9px;
  }
  .setting-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 16px;
    min-height: 105px;
    padding: 20px 24px;
    border-bottom: 1px solid var(--line);
  }
  .setting-row:last-child {
    border: 0;
  }
  .setting-icon {
    display: grid;
    width: 42px;
    height: 42px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 10px;
    color: var(--cyan-400);
    background: rgba(22, 199, 217, 0.06);
  }
  .setting-row > div {
    display: grid;
    gap: 5px;
  }
  .setting-row strong {
    font-size: 13px;
  }
  .setting-row p {
    margin: 0;
    color: #7894a4;
    font-size: 11px;
  }
  .switch {
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .switch input {
    position: absolute;
    opacity: 0;
  }
  .switch span {
    position: relative;
    width: 44px;
    height: 24px;
    border: 1px solid #375367;
    border-radius: 99px;
    background: #142b3a;
    transition: 0.2s;
  }
  .switch span::after {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    content: '';
    border-radius: 50%;
    background: #718e9e;
    transition: 0.2s;
  }
  .switch input:checked + span {
    border-color: var(--cyan-400);
    background: rgba(22, 199, 217, 0.25);
  }
  .switch input:checked + span::after {
    left: 23px;
    background: var(--cyan-200);
    box-shadow: 0 0 8px var(--cyan-400);
  }
  .switch em {
    min-width: 25px;
    color: #6d8a9a;
    font-size: 9px;
    font-style: normal;
  }
  .security-note {
    display: flex;
    gap: 12px;
    margin-top: 18px;
    padding: 17px;
    color: #7895a5;
  }
  .security-note :global(svg) {
    flex: none;
    color: var(--green-500);
  }
  .security-note strong {
    color: #adcad5;
    font-size: 11px;
  }
  .security-note p {
    margin: 4px 0 0;
    font-size: 10px;
    line-height: 1.6;
  }
  @media (max-width: 600px) {
    .settings-page {
      width: calc(100% - 24px);
      padding-top: 40px;
    }
    .setting-row {
      grid-template-columns: auto 1fr;
      padding: 17px 14px;
    }
    .switch {
      grid-column: 2;
    }
    .setting-row p {
      line-height: 1.6;
    }
    .session-panel {
      align-items: stretch;
      flex-direction: column;
    }
  }
  @media (max-width: 860px) {
    .settings-layout {
      grid-template-columns: 1fr;
    }
    .system-profile {
      position: relative;
      top: auto;
    }
    .profile-radar {
      width: 96px;
      height: 96px;
      float: right;
      margin: 0 0 0 16px;
    }
    .system-profile dl {
      grid-template-columns: repeat(3, 1fr);
      clear: both;
    }
  }
  @media (max-width: 600px) {
    .system-profile dl {
      grid-template-columns: 1fr;
    }
  }
</style>
