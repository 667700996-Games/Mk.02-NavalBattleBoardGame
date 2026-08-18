<script lang="ts">
  import {
    Ban,
    ChevronDown,
    FileSearch,
    LockKeyhole,
    RotateCcw,
    ShieldAlert
  } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { api } from '$lib/api';
  import { formatDateTime, localizeError, t, type MessageKey } from '$lib/i18n';
  import type {
    IntegritySignal,
    IntegritySignalKind,
    ModerationActionKind,
    ModerationCase,
    ReportStatus
  } from '$lib/types';

  let token = $state('');
  let operatorId = $state('');
  let authenticated = $state(false);
  let loading = $state(false);
  let notice = $state('');
  let cases = $state<ModerationCase[]>([]);
  let nextBefore = $state<string | null>(null);
  let search = $state('');
  let status = $state<ReportStatus | ''>('OPEN');
  let selectedId = $state<string | null>(null);
  let signals = $state<IntegritySignal[]>([]);
  let signalKind = $state<IntegritySignalKind | ''>('');
  let action = $state<ModerationActionKind>('WARN');
  let reason = $state('');
  let durationHours = $state(24);
  let reversesActionId = $state('');
  let acting = $state(false);
  let selectedCase = $derived(cases.find((item) => item.report.id === selectedId) ?? null);
  let reversibleActions = $derived(
    selectedCase?.actions.filter(
      (item) =>
        item.action !== 'REVERSE' &&
        item.action !== 'DISMISS' &&
        !selectedCase?.actions.some((candidate) => candidate.reversesActionId === item.id)
    ) ?? []
  );

  function message(error: unknown, fallback: MessageKey) {
    return localizeError(error, fallback, true);
  }

  async function load(reset = true) {
    if (!token.trim()) return;
    loading = true;
    notice = '';
    try {
      const response = await api.moderationCases(token.trim(), {
        status: status || undefined,
        search: search.trim() || undefined,
        before: reset ? undefined : (nextBefore ?? undefined),
        limit: 25
      });
      cases = reset ? response.cases : [...cases, ...response.cases];
      nextBefore = response.nextBefore;
      authenticated = true;
      if (reset) selectedId = response.cases[0]?.report.id ?? null;
      if (reset) await loadSignals();
    } catch (caught) {
      authenticated = false;
      notice = message(caught, 'moderation.queueError');
    } finally {
      loading = false;
    }
  }

  async function loadSignals() {
    try {
      signals = (
        await api.integritySignals(token.trim(), {
          kind: signalKind || undefined,
          search: search.trim() || undefined,
          limit: 20
        })
      ).signals;
    } catch (caught) {
      notice = message(caught, 'moderation.signalsError');
    }
  }

  async function submitAction() {
    if (!selectedCase || reason.trim().length < 4 || acting) return;
    acting = true;
    notice = '';
    try {
      await api.moderateReport(
        token.trim(),
        operatorId.trim(),
        selectedCase.report.id,
        action,
        reason.trim(),
        action === 'SUSPEND' ? durationHours : undefined,
        action === 'REVERSE' ? reversesActionId : undefined
      );
      reason = '';
      reversesActionId = '';
      notice = $t('moderation.actionRecorded', {
        player: selectedCase.report.targetNickname,
        action: $t(`moderationAction.${action}`)
      });
      await load(true);
    } catch (caught) {
      notice = message(caught, 'moderation.actionError');
    } finally {
      acting = false;
    }
  }

  const formatTime = (value: string) => formatDateTime(value);
</script>

<svelte:head><title>{$t('moderation.metaTitle')}</title></svelte:head>

<main class="moderation-page shell">
  <header class="page-head">
    <div>
      <p class="eyebrow">{$t('moderation.eyebrow')}</p>
      <h1 class="page-title">{$t('moderation.title')}</h1>
      <p>{$t('moderation.description')}</p>
    </div>
    <ShieldAlert size={34} aria-hidden="true" />
  </header>
  <nav class="admin-nav" aria-label={$t('admin.tools')}>
    <a href={resolve('/admin/support')}>{$t('admin.support')}</a>
    <a aria-current="page" href={resolve('/admin/moderation')}>{$t('admin.trustSafety')}</a>
  </nav>

  {#if !authenticated}
    <form
      class="access-panel panel"
      onsubmit={(event) => {
        event.preventDefault();
        load(true);
      }}
    >
      <LockKeyhole size={28} />
      <div>
        <h2>{$t('moderation.operatorAuth')}</h2>
        <p>{$t('moderation.operatorAuthDescription')}</p>
      </div>
      <label
        >{$t('support.operatorId')}
        <input bind:value={operatorId} minlength="2" maxlength="64" required /></label
      >
      <label
        >{$t('support.adminToken')}
        <input
          bind:value={token}
          type="password"
          minlength="32"
          autocomplete="off"
          required
        /></label
      >
      <button class="primary-button" type="submit" disabled={loading}
        >{loading ? $t('moderation.verifying') : $t('moderation.openQueue')}</button
      >
      {#if notice}<p class="notice" role="alert">{notice}</p>{/if}
    </form>
  {:else}
    <section class="queue-tools panel" aria-label={$t('moderation.reportSearch')}>
      <label>
        {$t('moderation.status')}
        <select bind:value={status}>
          <option value="">{$t('common.all')}</option>
          <option value="OPEN">{$t('reportStatus.OPEN')}</option>
          <option value="REVIEWING">{$t('reportStatus.REVIEWING')}</option>
          <option value="ACTIONED">{$t('reportStatus.ACTIONED')}</option>
          <option value="DISMISSED">{$t('reportStatus.DISMISSED')}</option>
        </select>
      </label>
      <label>
        {$t('moderation.evidenceSearch')}
        <input
          bind:value={search}
          maxlength="128"
          placeholder={$t('moderation.searchPlaceholder')}
        />
      </label>
      <button type="button" onclick={() => load(true)} disabled={loading}
        ><FileSearch size={15} /> {$t('common.search')}</button
      >
    </section>

    <details class="integrity-panel panel">
      <summary
        ><ShieldAlert size={15} />
        {$t('moderation.integritySignals')}
        <strong>{signals.length}</strong></summary
      >
      <div class="integrity-tools">
        <label
          >{$t('moderation.signalKind')}
          <select bind:value={signalKind} onchange={loadSignals}>
            <option value="">{$t('common.all')}</option>
            <option value="IMPOSSIBLE_ORDER">{$t('integritySignal.IMPOSSIBLE_ORDER')}</option>
            <option value="AUTOMATION">{$t('integritySignal.AUTOMATION')}</option>
            <option value="COLLUSION">{$t('integritySignal.COLLUSION')}</option>
            <option value="INTENTIONAL_STALLING"
              >{$t('integritySignal.INTENTIONAL_STALLING')}</option
            >
          </select></label
        >
        <p>{$t('moderation.signalDisclaimer')}</p>
      </div>
      <div class="integrity-list">
        {#if signals.length === 0}
          <p class="empty">{$t('moderation.noSignals')}</p>
        {:else}
          {#each signals as signal (signal.id)}
            <article>
              <span class="signal-severity"
                >{$t('moderation.severity', { severity: signal.severity })}</span
              >
              <strong>{$t(`integritySignal.${signal.kind}`)}</strong>
              <small>{signal.subjectIdentityId} · {formatTime(signal.lastObservedAt)}</small>
              <p>
                {$t('moderation.confidenceOccurrences', {
                  confidence: Math.round(signal.confidence * 100),
                  occurrences: signal.occurrences
                })}
              </p>
              <details>
                <summary>{$t('moderation.detectionEvidence')}</summary>
                <pre>{JSON.stringify(signal.evidence, null, 2)}</pre>
              </details>
            </article>
          {/each}
        {/if}
      </div>
    </details>

    <div class="operations-grid">
      <section class="case-list panel" aria-label={$t('moderation.caseList')}>
        <header>
          <strong>{$t('moderation.caseQueue')}</strong><small
            >{$t('moderation.caseCount', { count: cases.length })}</small
          >
        </header>
        {#if loading && cases.length === 0}
          <p class="empty">{$t('moderation.syncingQueue')}</p>
        {:else if cases.length === 0}
          <p class="empty">{$t('moderation.noReports')}</p>
        {:else}
          {#each cases as item (item.report.id)}
            <button
              type="button"
              class:selected={selectedId === item.report.id}
              onclick={() => (selectedId = item.report.id)}
            >
              <span class={`status status--${item.report.status.toLowerCase()}`}
                >{$t(`reportStatus.${item.report.status}`)}</span
              >
              <strong>{item.report.targetNickname}</strong>
              <small
                >{$t(`reportCategory.${item.report.category}`)} ·
                {formatTime(item.report.createdAt)}</small
              >
              <p>{item.report.details}</p>
            </button>
          {/each}
          {#if nextBefore}
            <button class="load-more" type="button" onclick={() => load(false)} disabled={loading}
              ><ChevronDown size={14} /> {$t('moderation.loadEarlier')}</button
            >
          {/if}
        {/if}
      </section>

      <section class="case-detail panel" aria-live="polite">
        {#if selectedCase}
          <header>
            <div>
              <p class="eyebrow">
                {$t('moderation.caseId', { id: selectedCase.report.id.slice(0, 8) })}
              </p>
              <h2>{selectedCase.report.targetNickname}</h2>
            </div>
            <span class={`status status--${selectedCase.report.status.toLowerCase()}`}
              >{$t(`reportStatus.${selectedCase.report.status}`)}</span
            >
          </header>
          <dl>
            <div>
              <dt>{$t('moderation.category')}</dt>
              <dd>{$t(`reportCategory.${selectedCase.report.category}`)}</dd>
            </div>
            <div>
              <dt>{$t('moderation.reportedAt')}</dt>
              <dd>{formatTime(selectedCase.report.createdAt)}</dd>
            </div>
            <div>
              <dt>{$t('moderation.roomId')}</dt>
              <dd>{selectedCase.report.roomId}</dd>
            </div>
            <div>
              <dt>{$t('moderation.targetIdentity')}</dt>
              <dd>{selectedCase.report.targetIdentityId}</dd>
            </div>
          </dl>
          <article class="report-copy">
            <strong>{$t('moderation.reportDetails')}</strong>
            <p>{selectedCase.report.details}</p>
          </article>
          <details class="evidence" open>
            <summary>{$t('moderation.serverEvidence')}</summary>
            <pre>{JSON.stringify(selectedCase.report.evidence, null, 2)}</pre>
          </details>

          <section class="audit-log">
            <h3>{$t('moderation.auditHistory')}</h3>
            {#if selectedCase.actions.length === 0}
              <p class="empty">{$t('moderation.noActions')}</p>
            {:else}
              {#each selectedCase.actions as item (item.id)}
                <article>
                  <strong>{$t(`moderationAction.${item.action}`)}</strong>
                  <span>{item.operatorId} · {formatTime(item.createdAt)}</span>
                  <p>{item.reason}</p>
                  {#if item.expiresAt}<small
                      >{$t('moderation.expiresAt', { time: formatTime(item.expiresAt) })}</small
                    >{/if}
                  {#if item.reversesActionId}<small
                      >{$t('moderation.reversesAction', {
                        id: item.reversesActionId
                      })}</small
                    >{/if}
                </article>
              {/each}
            {/if}
          </section>

          <form
            class="action-form"
            onsubmit={(event) => {
              event.preventDefault();
              submitAction();
            }}
          >
            <h3><Ban size={15} /> {$t('moderation.recordAction')}</h3>
            <div class="action-fields">
              <label
                >{$t('moderation.action')}
                <select bind:value={action}>
                  <option value="WARN">{$t('moderationAction.WARN')}</option>
                  <option value="SUSPEND">{$t('moderationAction.SUSPEND')}</option>
                  <option value="BAN">{$t('moderationAction.BAN')}</option>
                  <option value="DISMISS">{$t('moderationAction.DISMISS')}</option>
                  <option value="REVERSE">{$t('moderationAction.REVERSE')}</option>
                </select></label
              >
              {#if action === 'SUSPEND'}
                <label
                  >{$t('moderation.suspensionHours')}
                  <input bind:value={durationHours} type="number" min="1" max="8760" /></label
                >
              {:else if action === 'REVERSE'}
                <label
                  >{$t('moderation.actionToReverse')}
                  <select bind:value={reversesActionId} required>
                    <option value="" disabled>{$t('moderation.selectAction')}</option>
                    {#each reversibleActions as item (item.id)}
                      <option value={item.id}
                        >{$t(`moderationAction.${item.action}`)} ·
                        {formatTime(item.createdAt)}</option
                      >
                    {/each}
                  </select></label
                >
              {/if}
            </div>
            <label
              >{$t('moderation.reason')}
              <textarea bind:value={reason} minlength="4" maxlength="1000" rows="3" required
              ></textarea>
            </label>
            <button
              class="primary-button"
              type="submit"
              disabled={acting || reason.trim().length < 4}
              >{#if action === 'REVERSE'}<RotateCcw size={14} />{:else}<ShieldAlert
                  size={14}
                />{/if}
              {acting ? $t('moderation.recording') : $t('moderation.confirmAction')}</button
            >
          </form>
        {:else}
          <p class="empty">{$t('moderation.selectCase')}</p>
        {/if}
      </section>
    </div>
    {#if notice}<p class="global-notice" role="status">{notice}</p>{/if}
  {/if}
</main>

<style>
  .moderation-page {
    display: grid;
    gap: 18px;
    padding-block: 42px 80px;
  }
  .page-head {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 20px;
  }
  .page-head > :global(svg) {
    color: var(--red-400);
  }
  .page-head p {
    max-width: 720px;
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
  .access-panel {
    display: grid;
    width: min(560px, 100%);
    gap: 16px;
    margin-inline: auto;
    padding: 28px;
  }
  .access-panel > :global(svg) {
    color: var(--cyan-300);
  }
  .access-panel h2,
  .access-panel p {
    margin: 0;
  }
  .access-panel p {
    color: var(--ink-400);
    font-size: 12px;
    line-height: 1.6;
  }
  label {
    display: grid;
    gap: 6px;
    color: var(--ink-400);
    font-size: 10px;
  }
  input,
  select,
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
  select:focus,
  textarea:focus {
    border-color: var(--line-hot);
    box-shadow: 0 0 0 3px rgba(40, 223, 232, 0.07);
  }
  .primary-button,
  .queue-tools button {
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
    font-family: var(--font-display);
    font-size: 10px;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .notice,
  .global-notice {
    margin: 0;
    color: var(--red-400);
    font-size: 10px;
  }
  .queue-tools {
    display: grid;
    grid-template-columns: 180px minmax(220px, 1fr) 120px;
    align-items: end;
    gap: 10px;
    padding: 14px;
  }
  .integrity-panel {
    padding: 14px;
  }
  .integrity-panel > summary {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--cyan-300);
    cursor: pointer;
    font-family: var(--font-display);
    font-size: 11px;
  }
  .integrity-panel > summary strong {
    margin-left: auto;
    color: var(--amber-500);
  }
  .integrity-tools {
    display: grid;
    grid-template-columns: minmax(180px, 260px) 1fr;
    align-items: end;
    gap: 14px;
    margin-top: 14px;
  }
  .integrity-tools p {
    margin: 0 0 9px;
    color: var(--ink-500);
    font-size: 9px;
  }
  .integrity-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 8px;
    margin-top: 12px;
  }
  .integrity-list > article {
    display: grid;
    gap: 5px;
    min-width: 0;
    padding: 11px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: rgba(3, 14, 22, 0.5);
  }
  .integrity-list article > small,
  .integrity-list article > p {
    overflow-wrap: anywhere;
    margin: 0;
    color: var(--ink-500);
    font-size: 8px;
  }
  .integrity-list details summary {
    color: var(--ink-400);
    cursor: pointer;
    font-size: 8px;
  }
  .integrity-list pre {
    max-height: 180px;
    overflow: auto;
    color: var(--ink-400);
    font-size: 8px;
    white-space: pre-wrap;
  }
  .signal-severity {
    width: fit-content;
    color: var(--red-400);
    font-family: var(--font-display);
    font-size: 8px;
  }
  .operations-grid {
    display: grid;
    grid-template-columns: minmax(280px, 0.72fr) minmax(0, 1.6fr);
    align-items: start;
    gap: 14px;
  }
  .case-list {
    display: grid;
    max-height: calc(100vh - 230px);
    overflow-y: auto;
  }
  .case-list > header {
    display: flex;
    justify-content: space-between;
    padding: 14px;
    border-bottom: 1px solid var(--line);
  }
  .case-list > header small {
    color: var(--ink-500);
  }
  .case-list > button {
    display: grid;
    gap: 5px;
    padding: 14px;
    border: 0;
    border-bottom: 1px solid var(--line);
    color: inherit;
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  .case-list > button:hover,
  .case-list > button.selected {
    background: rgba(40, 223, 232, 0.055);
  }
  .case-list button p {
    overflow: hidden;
    margin: 0;
    color: var(--ink-400);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .case-list button small {
    color: var(--ink-500);
    font-size: 8px;
  }
  .status {
    width: fit-content;
    padding: 3px 7px;
    border: 1px solid var(--line);
    border-radius: 999px;
    color: var(--amber-500);
    font-family: var(--font-display);
    font-size: 8px;
  }
  .status--actioned {
    color: var(--red-400);
  }
  .status--dismissed {
    color: var(--ink-500);
  }
  .status--reviewing {
    color: var(--cyan-300);
  }
  .load-more {
    align-items: center;
    justify-content: center;
    color: var(--cyan-300) !important;
  }
  .case-detail {
    display: grid;
    gap: 16px;
    padding: 20px;
  }
  .case-detail > header {
    display: flex;
    align-items: start;
    justify-content: space-between;
  }
  .case-detail h2,
  .case-detail h3 {
    margin: 0;
  }
  .case-detail dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin: 0;
  }
  .case-detail dl div {
    display: grid;
    gap: 3px;
    padding: 9px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  dt {
    color: var(--ink-500);
    font-size: 8px;
  }
  dd {
    overflow-wrap: anywhere;
    margin: 0;
    color: var(--ink-300);
    font-family: var(--font-mono);
    font-size: 9px;
  }
  .report-copy {
    padding: 12px;
    border-left: 2px solid var(--orange-400);
    background: rgba(255, 180, 60, 0.045);
  }
  .report-copy p {
    margin: 7px 0 0;
    color: var(--ink-300);
    font-size: 11px;
    line-height: 1.6;
  }
  .evidence {
    border: 1px solid var(--line);
    border-radius: 9px;
  }
  .evidence summary {
    padding: 10px;
    cursor: pointer;
    color: var(--cyan-300);
    font-size: 10px;
  }
  .evidence pre {
    max-height: 300px;
    overflow: auto;
    margin: 0;
    padding: 12px;
    border-top: 1px solid var(--line);
    color: var(--ink-400);
    background: rgba(1, 8, 13, 0.65);
    font-size: 9px;
    white-space: pre-wrap;
  }
  .audit-log {
    display: grid;
    gap: 8px;
  }
  .audit-log article {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 9px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .audit-log article strong {
    color: var(--amber-500);
    font-size: 9px;
  }
  .audit-log article span,
  .audit-log article small {
    color: var(--ink-500);
    font-size: 8px;
  }
  .audit-log article p {
    grid-column: 1 / -1;
    margin: 3px 0;
    color: var(--ink-300);
    font-size: 10px;
  }
  .audit-log article small {
    grid-column: 1 / -1;
  }
  .action-form {
    display: grid;
    gap: 10px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .action-form h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
  }
  .action-fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 9px;
  }
  .empty {
    padding: 22px;
    color: var(--ink-500);
    font-size: 10px;
    text-align: center;
  }
  @media (max-width: 840px) {
    .operations-grid {
      grid-template-columns: 1fr;
    }
    .case-list {
      max-height: 360px;
    }
  }
  @media (max-width: 600px) {
    .queue-tools,
    .case-detail dl,
    .action-fields,
    .integrity-tools {
      grid-template-columns: 1fr;
    }
    .page-head {
      align-items: start;
    }
  }
</style>
