<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    Check,
    Contrast,
    Copy,
    Download,
    Gauge,
    KeyRound,
    LogOut,
    Monitor,
    Palette,
    ShieldCheck,
    Trash2,
    UserRound,
    Volume2
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, preferences, session, type ColorVisionMode } from '$lib/stores';
  import { sounds } from '$lib/sound';
  import type { AccountSession } from '$lib/types';

  let signingOut = $state(false);
  let logoutError = $state('');
  let handle = $state('');
  let accountError = $state('');
  let upgrading = $state(false);
  let recovery = $state<{ accountId: string; recoveryKey: string } | null>(null);
  let copied = $state(false);
  let accountSessions = $state<AccountSession[]>([]);
  let currentSessionId = $state('');
  let exportingAccount = $state(false);
  let deletingAccount = $state(false);
  let deletionRecoveryKey = $state('');
  let deletionConfirmation = $state('');

  function setColorVision(event: Event) {
    const colorVision = (event.currentTarget as HTMLSelectElement).value as ColorVisionMode;
    preferences.update((current) => ({ ...current, colorVision }));
  }

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      handle = current.nickname;
      if (current.accountId) await loadAccountSessions();
    } catch {
      await goto(resolve('/'));
    }
  });

  async function loadAccountSessions() {
    const response = await api.accountSessions();
    currentSessionId = response.currentSessionId;
    accountSessions = response.sessions;
  }

  async function upgradeAccount() {
    upgrading = true;
    accountError = '';
    try {
      const response = await api.upgradeAccount(handle);
      recovery = { accountId: response.account.id, recoveryKey: response.recoveryKey };
      session.update((current) =>
        current
          ? { ...current, accountId: response.account.id, nickname: response.account.handle }
          : current
      );
      await loadAccountSessions();
    } catch (caught) {
      accountError = caught instanceof ApiError ? caught.message : '계정을 생성하지 못했습니다.';
    } finally {
      upgrading = false;
    }
  }

  async function copyRecovery() {
    if (!recovery) return;
    await navigator.clipboard.writeText(
      `MK.01 ACCOUNT\n${recovery.accountId}\n${recovery.recoveryKey}`
    );
    copied = true;
    setTimeout(() => (copied = false), 1_800);
  }

  async function revokeSession(sessionId: string) {
    accountError = '';
    try {
      await api.revokeAccountSession(sessionId);
      await loadAccountSessions();
    } catch (caught) {
      accountError = caught instanceof ApiError ? caught.message : '세션을 폐기하지 못했습니다.';
    }
  }

  async function exportAccountData() {
    exportingAccount = true;
    accountError = '';
    try {
      const archive = await api.exportAccountData();
      const blob = new Blob([JSON.stringify(archive, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `mk01-account-${archive.requestId}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (caught) {
      accountError =
        caught instanceof ApiError ? caught.message : '계정 자료를 내보내지 못했습니다.';
    } finally {
      exportingAccount = false;
    }
  }

  async function deleteAccount() {
    deletingAccount = true;
    accountError = '';
    try {
      await api.deleteAccount(deletionRecoveryKey.trim(), deletionConfirmation.trim());
      realtime.disconnect();
      gameSnapshot.set(null);
      session.set(null);
      await goto(resolve('/'));
    } catch (caught) {
      accountError = caught instanceof ApiError ? caught.message : '계정을 삭제하지 못했습니다.';
    } finally {
      deletingAccount = false;
    }
  }

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
              aria-label="작전 사운드"
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
            ><input
              type="checkbox"
              aria-label="동작 줄이기"
              bind:checked={$preferences.reducedMotion}
            /><span></span><em>{$preferences.reducedMotion ? '켜짐' : '꺼짐'}</em></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Contrast size={20} /></span>
          <div>
            <strong>고대비 모드</strong>
            <p>격자선과 텍스트의 대비를 높여 전장 정보를 더 명확하게 표시합니다.</p>
          </div>
          <label class="switch"
            ><input
              type="checkbox"
              aria-label="고대비 모드"
              bind:checked={$preferences.highContrast}
            /><span></span><em>{$preferences.highContrast ? '켜짐' : '꺼짐'}</em></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Palette size={20} /></span>
          <div>
            <strong>색각 표시 프리셋</strong>
            <p>적록·청황 구분을 보완하는 전술 색상 팔레트를 선택합니다.</p>
          </div>
          <label class="vision-select">
            <span class="sr-only">색각 표시 프리셋</span>
            <select class="select" value={$preferences.colorVision} onchange={setColorVision}>
              <option value="standard">표준</option>
              <option value="protanopia">적색맹 보정</option>
              <option value="deuteranopia">녹색맹 보정</option>
              <option value="tritanopia">청황색맹 보정</option>
            </select>
          </label>
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
        <section class="account-panel panel" aria-labelledby="account-title">
          <header>
            <span><UserRound size={20} /></span>
            <div>
              <small>COMMAND IDENTITY</small>
              <strong id="account-title"
                >{$session.accountId ? '지휘 계정' : '게스트 기록 보존'}</strong
              >
              <p>
                {$session.accountId
                  ? '복구 키로 다른 장치에서 같은 기록과 신원에 접속할 수 있습니다.'
                  : '현재 게스트 세션을 계정으로 승격하면 기존 전투 기록이 그대로 유지됩니다.'}
              </p>
            </div>
          </header>
          {#if !$session.accountId}
            <form
              class="upgrade-form"
              onsubmit={(event) => {
                event.preventDefault();
                upgradeAccount();
              }}
            >
              <label for="account-handle"
                ><span>계정 호출부호</span><input
                  id="account-handle"
                  bind:value={handle}
                  minlength="2"
                  maxlength="16"
                  required
                /></label
              >
              <button class="button button--primary" disabled={upgrading}
                ><KeyRound size={15} /> {upgrading ? '승격 중…' : '기록 보존 계정 생성'}</button
              >
            </form>
          {/if}
          {#if recovery}
            <aside class="recovery-card" role="status">
              <strong>복구 키는 지금 한 번만 표시됩니다</strong>
              <p>
                안전한 암호 관리자에 계정 ID와 복구 키를 함께 저장하십시오. 서버는 원문을 보관하지
                않습니다.
              </p>
              <dl>
                <div>
                  <dt>ACCOUNT ID</dt>
                  <dd>{recovery.accountId}</dd>
                </div>
                <div>
                  <dt>RECOVERY KEY</dt>
                  <dd>{recovery.recoveryKey}</dd>
                </div>
              </dl>
              <button class="button" type="button" onclick={copyRecovery}
                >{#if copied}<Check size={15} /> 복사됨{:else}<Copy size={15} /> 자격 증명 복사{/if}</button
              >
            </aside>
          {/if}
          {#if $session.accountId && accountSessions.length}
            <div class="device-list">
              <h3><Monitor size={15} /> 활성 장치 세션</h3>
              {#each accountSessions as device (device.id)}
                <article>
                  <div>
                    <strong>{device.id === currentSessionId ? '현재 장치' : '연결된 장치'}</strong
                    ><span>최근 사용 {new Date(device.lastSeenAt).toLocaleString('ko-KR')}</span>
                  </div>
                  {#if device.id !== currentSessionId}<button
                      type="button"
                      aria-label="이 장치 세션 폐기"
                      onclick={() => revokeSession(device.id)}><Trash2 size={15} /></button
                    >{/if}
                </article>
              {/each}
            </div>
          {/if}
          {#if $session.accountId}
            <section class="privacy-controls" aria-labelledby="privacy-controls-title">
              <div>
                <small>DATA CONTROL</small>
                <h3 id="privacy-controls-title">계정 자료 관리</h3>
                <p>
                  자격 증명을 제외한 계정·전투·보상·소셜·신고 자료를 JSON으로 내려받을 수 있습니다.
                </p>
              </div>
              <button
                class="button"
                type="button"
                onclick={exportAccountData}
                disabled={exportingAccount}
              >
                <Download size={15} />
                {exportingAccount ? '자료 준비 중…' : '내 자료 내보내기'}
              </button>
              <form
                class="account-deletion"
                onsubmit={(event) => {
                  event.preventDefault();
                  deleteAccount();
                }}
              >
                <strong>계정 영구 삭제</strong>
                <p>
                  모든 장치가 로그아웃되고 개인 자료가 삭제됩니다. 완료된 전투 기록은 통계 무결성을
                  위해 익명화됩니다. 이 작업은 되돌릴 수 없습니다.
                </p>
                <label for="deletion-recovery-key"
                  ><span>복구 키</span><input
                    id="deletion-recovery-key"
                    type="password"
                    autocomplete="off"
                    bind:value={deletionRecoveryKey}
                    required
                  /></label
                >
                <label for="deletion-confirmation"
                  ><span>확인을 위해 DELETE 입력</span><input
                    id="deletion-confirmation"
                    bind:value={deletionConfirmation}
                    pattern="DELETE"
                    required
                  /></label
                >
                <button
                  class="button button--danger"
                  type="submit"
                  disabled={deletingAccount || deletionConfirmation !== 'DELETE'}
                >
                  <Trash2 size={15} />
                  {deletingAccount ? '계정 삭제 중…' : '계정 영구 삭제'}
                </button>
              </form>
            </section>
          {/if}
          {#if accountError}<p class="account-error" role="alert">{accountError}</p>{/if}
        </section>
      {/if}
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
  .account-panel {
    display: grid;
    gap: 18px;
    margin-top: 14px;
    padding: 20px;
  }
  .account-panel > header {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 13px;
    align-items: center;
  }
  .account-panel > header > span {
    display: grid;
    width: 42px;
    height: 42px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 9px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .account-panel > header div {
    display: grid;
    gap: 3px;
  }
  .account-panel small {
    color: var(--ink-500);
    font: 700 7px var(--font-display);
    letter-spacing: 0.14em;
  }
  .account-panel p {
    margin: 0;
    color: var(--ink-400);
    font-size: 10px;
    line-height: 1.6;
  }
  .upgrade-form {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 10px;
  }
  .upgrade-form label {
    display: grid;
    gap: 5px;
    color: var(--ink-400);
    font-size: 9px;
  }
  .upgrade-form input {
    min-height: 42px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--ink-100);
    background: rgba(2, 14, 21, 0.8);
  }
  .recovery-card {
    display: grid;
    gap: 10px;
    padding: 15px;
    border: 1px solid rgba(255, 209, 107, 0.3);
    border-radius: 8px;
    background: rgba(255, 209, 107, 0.05);
  }
  .recovery-card > strong {
    color: var(--amber-400);
    font-size: 12px;
  }
  .recovery-card dl {
    display: grid;
    gap: 7px;
    margin: 0;
  }
  .recovery-card dl div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .recovery-card dt {
    color: var(--ink-500);
    font: 700 7px var(--font-display);
  }
  .recovery-card dd {
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--ink-200);
    font-family: monospace;
    font-size: 10px;
  }
  .recovery-card .button {
    width: fit-content;
  }
  .device-list {
    display: grid;
    gap: 6px;
  }
  .device-list h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 0 3px;
    font-size: 11px;
  }
  .device-list article {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .device-list article > div {
    display: grid;
    gap: 2px;
  }
  .device-list article strong {
    font-size: 10px;
  }
  .device-list article span {
    color: var(--ink-500);
    font-size: 8px;
  }
  .device-list button {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid rgba(255, 83, 100, 0.24);
    border-radius: 5px;
    color: var(--red-400);
    background: rgba(255, 83, 100, 0.05);
    cursor: pointer;
  }
  .privacy-controls {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px;
    align-items: center;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .privacy-controls h3 {
    margin: 3px 0 4px;
    font-size: 12px;
  }
  .account-deletion {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: minmax(160px, 1fr) minmax(160px, 1fr) auto;
    gap: 10px;
    align-items: end;
    padding: 14px;
    border: 1px solid rgba(255, 83, 100, 0.24);
    border-radius: 7px;
    background: rgba(255, 83, 100, 0.04);
  }
  .account-deletion > strong,
  .account-deletion > p {
    grid-column: 1 / -1;
  }
  .account-deletion > strong {
    color: var(--critical);
    font-size: 11px;
  }
  .account-deletion label {
    display: grid;
    gap: 5px;
    color: var(--ink-400);
    font-size: 9px;
  }
  .account-deletion input {
    min-height: 40px;
    padding: 9px 11px;
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--ink-100);
    background: rgba(2, 14, 21, 0.8);
  }
  .account-error {
    color: var(--critical) !important;
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
  .vision-select {
    width: 172px;
  }
  .vision-select .select {
    height: 42px;
    font-size: 11px;
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
    .vision-select {
      grid-column: 2;
      width: min(100%, 220px);
    }
    .setting-row p {
      line-height: 1.6;
    }
    .session-panel {
      align-items: stretch;
      flex-direction: column;
    }
    .upgrade-form {
      grid-template-columns: 1fr;
    }
    .privacy-controls,
    .account-deletion {
      grid-template-columns: 1fr;
    }
    .privacy-controls > .button {
      width: 100%;
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
