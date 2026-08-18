<script lang="ts">
  import { Headphones, KeyRound, LockKeyhole, Search, ShieldCheck } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { api } from '$lib/api';
  import { formatDateTime, localizeError, t, type MessageKey } from '$lib/i18n';
  import type { SupportAccountSnapshot } from '$lib/types';

  let token = $state('');
  let operatorId = $state('');
  let query = $state('');
  let result = $state<SupportAccountSnapshot | null>(null);
  let selectedSessionId = $state('');
  let reason = $state('');
  let confirmation = $state('');
  let loading = $state(false);
  let acting = $state(false);
  let notice = $state('');

  const formatTime = (value: string) => formatDateTime(value);
  const errorMessage = (error: unknown, fallback: MessageKey) =>
    localizeError(error, fallback, true);

  async function findAccount() {
    if (!token.trim() || query.trim().length < 3 || loading) return;
    loading = true;
    notice = '';
    result = null;
    try {
      result = await api.supportAccount(token.trim(), query.trim());
      selectedSessionId = '';
      reason = '';
      confirmation = '';
    } catch (caught) {
      notice = errorMessage(caught, 'support.lookupError');
    } finally {
      loading = false;
    }
  }

  async function revokeSessions() {
    if (
      !result ||
      acting ||
      operatorId.trim().length < 2 ||
      reason.trim().length < 8 ||
      confirmation !== result.account.handle
    )
      return;
    acting = true;
    notice = '';
    try {
      const response = await api.revokeSupportSessions(
        token.trim(),
        operatorId.trim(),
        result.account.id,
        reason.trim(),
        selectedSessionId || undefined
      );
      notice = $t('support.revokedNotice', {
        count: response.action.affectedSessionIds.length
      });
      result = await api.supportAccount(token.trim(), result.account.id);
      reason = '';
      confirmation = '';
      selectedSessionId = '';
    } catch (caught) {
      notice = errorMessage(caught, 'support.revokeError');
    } finally {
      acting = false;
    }
  }
</script>

<svelte:head><title>{$t('support.metaTitle')}</title></svelte:head>

<main class="support-page shell">
  <header class="page-head">
    <div>
      <p class="eyebrow">{$t('support.eyebrow')}</p>
      <h1 class="page-title">{$t('support.title')}</h1>
      <p>{$t('support.description')}</p>
    </div>
    <Headphones size={34} aria-hidden="true" />
  </header>

  <nav class="admin-nav" aria-label={$t('admin.tools')}>
    <a aria-current="page" href={resolve('/admin/support')}>{$t('admin.support')}</a>
    <a href={resolve('/admin/moderation')}>{$t('admin.trustSafety')}</a>
  </nav>

  <form
    class="lookup panel"
    onsubmit={(event) => {
      event.preventDefault();
      findAccount();
    }}
  >
    <header>
      <LockKeyhole size={20} aria-hidden="true" />
      <div>
        <h2>{$t('support.operatorSearch')}</h2>
        <p>{$t('support.operatorSearchDescription')}</p>
      </div>
    </header>
    <label
      >{$t('support.operatorId')}
      <input bind:value={operatorId} minlength="2" maxlength="64" required /></label
    >
    <label>
      {$t('support.adminToken')}
      <input bind:value={token} type="password" minlength="32" autocomplete="off" required />
    </label>
    <label>
      {$t('support.accountQuery')}
      <input bind:value={query} minlength="3" maxlength="64" autocomplete="off" required />
    </label>
    <button type="submit" disabled={loading || query.trim().length < 3}>
      <Search size={15} aria-hidden="true" />
      {loading ? $t('support.searching') : $t('support.searchAccount')}
    </button>
  </form>

  {#if result}
    <section class="identity panel" aria-labelledby="support-account-heading">
      <header>
        <div>
          <p class="eyebrow">{$t('support.verifiedAccount')}</p>
          <h2 id="support-account-heading">{result.account.handle}</h2>
        </div>
        <ShieldCheck size={26} aria-label={$t('support.exactMatch')} />
      </header>
      <dl>
        <div>
          <dt>{$t('settings.accountId')}</dt>
          <dd>{result.account.id}</dd>
        </div>
        <div>
          <dt>{$t('support.createdAt')}</dt>
          <dd>{formatTime(result.account.createdAt)}</dd>
        </div>
        <div>
          <dt>{$t('support.activeSessions')}</dt>
          <dd>{$t('support.sessionCount', { count: result.sessions.length })}</dd>
        </div>
      </dl>
    </section>

    <div class="support-grid">
      <section class="sessions panel">
        <header>
          <h2>{$t('support.sessionSecurity')}</h2>
          <KeyRound size={19} aria-hidden="true" />
        </header>
        {#if result.sessions.length === 0}
          <p class="empty">{$t('support.noActiveSessions')}</p>
        {:else}
          <fieldset>
            <legend>{$t('support.revokeScope')}</legend>
            <label class="session-choice">
              <input bind:group={selectedSessionId} type="radio" value="" />
              <span
                ><strong>{$t('support.allActiveSessions')}</strong><small
                  >{$t('support.compromiseResponse')}</small
                ></span
              >
            </label>
            {#each result.sessions as session (session.id)}
              <label class="session-choice">
                <input bind:group={selectedSessionId} type="radio" value={session.id} />
                <span>
                  <strong>{session.nickname}</strong>
                  <small>{$t('support.lastActive', { time: formatTime(session.lastSeenAt) })}</small
                  >
                  <small
                    >{session.currentRoomId
                      ? $t('support.activeRoom', { room: session.currentRoomId })
                      : $t('support.noActiveRoom')}</small
                  >
                </span>
              </label>
            {/each}
          </fieldset>
          <form
            class="revoke-form"
            onsubmit={(event) => {
              event.preventDefault();
              revokeSessions();
            }}
          >
            <label>
              {$t('support.verifiedReason')}
              <textarea bind:value={reason} minlength="8" maxlength="500" rows="3" required
              ></textarea>
            </label>
            <label>
              {$t('support.confirmDangerousAction', { handle: result.account.handle })}
              <input bind:value={confirmation} autocomplete="off" required />
            </label>
            <button
              class="danger"
              type="submit"
              disabled={acting ||
                reason.trim().length < 8 ||
                confirmation !== result.account.handle}
            >
              <KeyRound size={14} aria-hidden="true" />
              {acting
                ? $t('support.recordingAudit')
                : selectedSessionId
                  ? $t('support.revokeSelected')
                  : $t('support.revokeAll')}
            </button>
          </form>
        {/if}
      </section>

      <section class="audit panel" aria-live="polite">
        <header>
          <h2>{$t('support.auditHistory')}</h2>
          <span>{result.actions.length}</span>
        </header>
        {#if result.actions.length === 0}
          <p class="empty">{$t('support.noActions')}</p>
        {:else}
          {#each result.actions as action (action.id)}
            <article>
              <strong>{action.action}</strong>
              <small>{action.operatorId} · {formatTime(action.createdAt)}</small>
              <p>{action.reason}</p>
              <small
                >{$t('support.sessionsRevoked', {
                  count: action.affectedSessionIds.length
                })}</small
              >
            </article>
          {/each}
        {/if}
      </section>
    </div>
  {/if}

  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
</main>

<style>
  .support-page {
    display: grid;
    gap: 18px;
    padding-block: 42px 80px;
  }
  .page-head,
  .identity > header,
  .sessions > header,
  .audit > header,
  .lookup > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .page-head > :global(svg),
  .identity > header :global(svg) {
    color: var(--cyan-300);
  }
  .page-head p,
  .lookup p {
    max-width: 760px;
    margin: 0;
    color: var(--ink-400);
  }
  .admin-nav {
    display: flex;
    gap: 8px;
  }
  .admin-nav a {
    padding: 8px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    color: var(--ink-400);
    font: 10px var(--font-display);
    text-decoration: none;
  }
  .admin-nav a[aria-current='page'] {
    border-color: var(--line-hot);
    color: var(--cyan-200);
    background: rgba(40, 223, 232, 0.08);
  }
  .lookup {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)) 150px;
    align-items: end;
    gap: 12px;
    padding: 20px;
  }
  .lookup > header {
    grid-column: 1 / -1;
    justify-content: flex-start;
  }
  h2,
  .lookup p {
    margin-block: 0;
  }
  label {
    display: grid;
    gap: 6px;
    color: var(--ink-400);
    font-size: 10px;
  }
  input,
  textarea {
    width: 100%;
    padding: 10px 11px;
    border: 1px solid var(--line);
    border-radius: 9px;
    outline: 0;
    color: var(--ink-100);
    background: rgba(3, 13, 20, 0.82);
    font: inherit;
  }
  input:focus,
  textarea:focus {
    border-color: var(--line-hot);
    box-shadow: 0 0 0 3px rgba(40, 223, 232, 0.07);
  }
  button {
    display: flex;
    min-height: 40px;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 1px solid var(--line-hot);
    border-radius: 9px;
    color: var(--cyan-200);
    background: rgba(40, 223, 232, 0.09);
    cursor: pointer;
    font: 10px var(--font-display);
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  button.danger {
    border-color: rgba(255, 89, 94, 0.55);
    color: var(--red-400);
    background: rgba(255, 89, 94, 0.08);
  }
  .identity {
    display: grid;
    gap: 14px;
    padding: 18px;
  }
  .identity dl {
    display: grid;
    grid-template-columns: 1fr 1fr 160px;
    gap: 10px;
    margin: 0;
  }
  .identity dl div {
    min-width: 0;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  dt {
    color: var(--ink-500);
    font-size: 8px;
  }
  dd {
    overflow-wrap: anywhere;
    margin: 4px 0 0;
    color: var(--ink-200);
    font-size: 10px;
  }
  .support-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(300px, 0.9fr);
    gap: 14px;
  }
  .sessions,
  .audit {
    display: grid;
    align-content: start;
    gap: 14px;
    padding: 18px;
  }
  fieldset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: 8px;
    color: var(--ink-400);
    font-size: 9px;
  }
  .session-choice {
    grid-template-columns: auto 1fr;
    align-items: start;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .session-choice input {
    width: auto;
    margin-top: 2px;
  }
  .session-choice span {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .session-choice small {
    overflow-wrap: anywhere;
    color: var(--ink-500);
    font-size: 8px;
  }
  .revoke-form {
    display: grid;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .audit > header span {
    color: var(--amber-500);
    font: 11px var(--font-display);
  }
  .audit article {
    display: grid;
    gap: 5px;
    padding: 11px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: rgba(3, 14, 22, 0.5);
  }
  .audit article p {
    margin: 0;
    color: var(--ink-300);
    font-size: 10px;
  }
  .audit article small {
    color: var(--ink-500);
    font-size: 8px;
  }
  .empty,
  .notice {
    margin: 0;
    color: var(--ink-500);
    font-size: 10px;
  }
  .notice {
    color: var(--amber-500);
  }
  @media (max-width: 900px) {
    .lookup {
      grid-template-columns: 1fr 1fr;
    }
    .support-grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 620px) {
    .lookup,
    .identity dl {
      grid-template-columns: 1fr;
    }
    .page-head {
      align-items: flex-start;
    }
  }
</style>
