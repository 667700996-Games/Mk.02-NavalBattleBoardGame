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
    UserRound
  } from '@lucide/svelte';
  import { api } from '$lib/api';
  import AudioSettings from '$lib/components/settings/AudioSettings.svelte';
  import PresentationSettings from '$lib/components/settings/PresentationSettings.svelte';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, preferences, session, type ColorVisionMode } from '$lib/stores';
  import { formatDateTime, localizeError, t } from '$lib/i18n';
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
      accountError = localizeError(caught, 'settings.createAccountError');
    } finally {
      upgrading = false;
    }
  }

  async function copyRecovery() {
    if (!recovery) return;
    await navigator.clipboard.writeText(
      `${$t('settings.accountCredentialHeader')}\n${recovery.accountId}\n${recovery.recoveryKey}`
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
      accountError = localizeError(caught, 'settings.revokeSessionError');
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
      accountError = localizeError(caught, 'settings.exportError');
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
      accountError = localizeError(caught, 'settings.deleteError');
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
      logoutError = localizeError(caught, 'settings.logoutError');
    } finally {
      signingOut = false;
    }
  }
</script>

<svelte:head><title>{$t('settings.metaTitle')}</title></svelte:head>
<div class="settings-page shell">
  <header>
    <p class="eyebrow">{$t('settings.eyebrow')}</p>
    <h1 class="page-title">{$t('settings.title')}</h1>
    <p>{$t('settings.description')}</p>
  </header>
  <div class="settings-layout">
    <aside class="system-profile panel" aria-label={$t('settings.systemProfile')}>
      <div class="profile-radar"><i></i><span></span></div>
      <p class="eyebrow">{$t('settings.localProfile')}</p>
      <h2>{$t('settings.commandDisplay')}</h2>
      <p>{$t('settings.profileDescription')}</p>
      <dl>
        <div>
          <dt>{$t('settings.renderMode')}</dt>
          <dd>{$t('settings.tacticalWeb')}</dd>
        </div>
        <div>
          <dt>{$t('settings.security')}</dt>
          <dd>{$t('settings.serverAuthoritative')}</dd>
        </div>
        <div>
          <dt>{$t('settings.profileScope')}</dt>
          <dd>{$t('settings.thisDevice')}</dd>
        </div>
      </dl>
    </aside>
    <div class="settings-main">
      <AudioSettings />
      <section class="settings-panel panel">
        <div class="setting-row">
          <span class="setting-icon"><Gauge size={20} /></span>
          <div>
            <strong>{$t('settings.reducedMotion')}</strong>
            <p>{$t('settings.reducedMotionDescription')}</p>
          </div>
          <label class="switch"
            ><input
              type="checkbox"
              aria-label={$t('settings.reducedMotion')}
              bind:checked={$preferences.reducedMotion}
            /><span></span><em>{$preferences.reducedMotion ? $t('common.on') : $t('common.off')}</em
            ></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Contrast size={20} /></span>
          <div>
            <strong>{$t('settings.highContrast')}</strong>
            <p>{$t('settings.highContrastDescription')}</p>
          </div>
          <label class="switch"
            ><input
              type="checkbox"
              aria-label={$t('settings.highContrast')}
              bind:checked={$preferences.highContrast}
            /><span></span><em>{$preferences.highContrast ? $t('common.on') : $t('common.off')}</em
            ></label
          >
        </div>
        <div class="setting-row">
          <span class="setting-icon"><Palette size={20} /></span>
          <div>
            <strong>{$t('settings.colorVision')}</strong>
            <p>{$t('settings.colorVisionDescription')}</p>
          </div>
          <label class="vision-select">
            <span class="sr-only">{$t('settings.colorVision')}</span>
            <select class="select" value={$preferences.colorVision} onchange={setColorVision}>
              <option value="standard">{$t('settings.colorStandard')}</option>
              <option value="protanopia">{$t('settings.colorProtanopia')}</option>
              <option value="deuteranopia">{$t('settings.colorDeuteranopia')}</option>
              <option value="tritanopia">{$t('settings.colorTritanopia')}</option>
            </select>
          </label>
        </div>
      </section>
      <PresentationSettings />
      <aside class="security-note">
        <ShieldCheck size={18} />
        <div>
          <strong>{$t('settings.serverFairness')}</strong>
          <p>{$t('settings.serverFairnessDescription')}</p>
        </div>
      </aside>
      {#if $session}
        <section class="account-panel panel" aria-labelledby="account-title">
          <header>
            <span><UserRound size={20} /></span>
            <div>
              <small>{$t('settings.commandIdentity')}</small>
              <strong id="account-title"
                >{$session.accountId
                  ? $t('settings.commandAccount')
                  : $t('settings.preserveGuest')}</strong
              >
              <p>
                {$session.accountId
                  ? $t('settings.accountDescription')
                  : $t('settings.guestDescription')}
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
                ><span>{$t('settings.accountHandle')}</span><input
                  id="account-handle"
                  bind:value={handle}
                  minlength="2"
                  maxlength="16"
                  required
                /></label
              >
              <button class="button button--primary" disabled={upgrading}
                ><KeyRound size={15} />
                {upgrading
                  ? $t('settings.upgrading')
                  : $t('settings.createPreservedAccount')}</button
              >
            </form>
          {/if}
          {#if recovery}
            <aside class="recovery-card" role="status">
              <strong>{$t('settings.recoveryOnce')}</strong>
              <p>{$t('settings.recoveryDescription')}</p>
              <dl>
                <div>
                  <dt>{$t('settings.accountId')}</dt>
                  <dd>{recovery.accountId}</dd>
                </div>
                <div>
                  <dt>{$t('settings.recoveryKey')}</dt>
                  <dd>{recovery.recoveryKey}</dd>
                </div>
              </dl>
              <button class="button" type="button" onclick={copyRecovery}
                >{#if copied}<Check size={15} /> {$t('settings.copied')}{:else}<Copy size={15} />
                  {$t('settings.copyCredentials')}{/if}</button
              >
            </aside>
          {/if}
          {#if $session.accountId && accountSessions.length}
            <div class="device-list">
              <h3><Monitor size={15} /> {$t('settings.activeSessions')}</h3>
              {#each accountSessions as device (device.id)}
                <article>
                  <div>
                    <strong
                      >{device.id === currentSessionId
                        ? $t('settings.currentDevice')
                        : $t('settings.connectedDevice')}</strong
                    ><span
                      >{$t('settings.lastUsed', {
                        time: formatDateTime(device.lastSeenAt)
                      })}</span
                    >
                  </div>
                  {#if device.id !== currentSessionId}<button
                      type="button"
                      aria-label={$t('settings.revokeDevice')}
                      onclick={() => revokeSession(device.id)}><Trash2 size={15} /></button
                    >{/if}
                </article>
              {/each}
            </div>
          {/if}
          {#if $session.accountId}
            <section class="privacy-controls" aria-labelledby="privacy-controls-title">
              <div>
                <small>{$t('settings.dataControl')}</small>
                <h3 id="privacy-controls-title">{$t('settings.accountData')}</h3>
                <p>{$t('settings.accountDataDescription')}</p>
              </div>
              <button
                class="button"
                type="button"
                onclick={exportAccountData}
                disabled={exportingAccount}
              >
                <Download size={15} />
                {exportingAccount ? $t('settings.preparingData') : $t('settings.exportData')}
              </button>
              <form
                class="account-deletion"
                onsubmit={(event) => {
                  event.preventDefault();
                  deleteAccount();
                }}
              >
                <strong>{$t('settings.deleteAccount')}</strong>
                <p>{$t('settings.deleteDescription')}</p>
                <label for="deletion-recovery-key"
                  ><span>{$t('settings.recoveryKey')}</span><input
                    id="deletion-recovery-key"
                    type="password"
                    autocomplete="off"
                    bind:value={deletionRecoveryKey}
                    required
                  /></label
                >
                <label for="deletion-confirmation"
                  ><span>{$t('settings.deleteConfirmation')}</span><input
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
                  {deletingAccount ? $t('settings.deleting') : $t('settings.deleteAccount')}
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
            <small>{$t('settings.sessionControl')}</small>
            <strong id="session-control-title">{$t('settings.deviceSession')}</strong>
            <p>{$t('settings.logoutDescription')}</p>
            {#if logoutError}<p class="session-error" role="alert">{logoutError}</p>{/if}
          </div>
          <button
            class="button button--danger"
            type="button"
            onclick={signOut}
            disabled={signingOut}
          >
            <LogOut size={16} />
            {signingOut ? $t('settings.signingOut') : $t('settings.logout')}
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
