<script lang="ts">
  import {
    Ban,
    ChevronDown,
    FileSearch,
    LockKeyhole,
    RotateCcw,
    ShieldAlert
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
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

  function message(error: unknown, fallback: string) {
    return error instanceof ApiError ? `${error.message} (${error.code})` : fallback;
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
      notice = message(caught, '운영 큐를 불러오지 못했습니다.');
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
      notice = message(caught, '무결성 신호를 불러오지 못했습니다.');
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
      notice = `${selectedCase.report.targetNickname} 사건에 ${action} 조치를 기록했습니다.`;
      await load(true);
    } catch (caught) {
      notice = message(caught, '운영 조치를 기록하지 못했습니다.');
    } finally {
      acting = false;
    }
  }

  const formatTime = (value: string) => new Date(value).toLocaleString('ko-KR');
</script>

<svelte:head><title>신뢰 및 안전 운영 · Mk.01</title></svelte:head>

<main class="moderation-page shell">
  <header class="page-head">
    <div>
      <p class="eyebrow">TRUST & SAFETY OPERATIONS</p>
      <h1 class="page-title">신고 검토 센터</h1>
      <p>플레이어 신고 증거를 검색하고 모든 운영 조치를 감사 이력으로 남깁니다.</p>
    </div>
    <ShieldAlert size={34} aria-hidden="true" />
  </header>

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
        <h2>운영자 인증</h2>
        <p>토큰은 브라우저 저장소에 보관되지 않으며 이 화면의 메모리에서만 유지됩니다.</p>
      </div>
      <label
        >운영자 ID <input bind:value={operatorId} minlength="2" maxlength="64" required /></label
      >
      <label
        >관리 토큰 <input
          bind:value={token}
          type="password"
          minlength="32"
          autocomplete="off"
          required
        /></label
      >
      <button class="primary-button" type="submit" disabled={loading}
        >{loading ? '검증 중…' : '안전 운영 큐 열기'}</button
      >
      {#if notice}<p class="notice" role="alert">{notice}</p>{/if}
    </form>
  {:else}
    <section class="queue-tools panel" aria-label="신고 검색">
      <label>
        상태
        <select bind:value={status}>
          <option value="">전체</option>
          <option value="OPEN">미검토</option>
          <option value="REVIEWING">재검토</option>
          <option value="ACTIONED">조치 완료</option>
          <option value="DISMISSED">기각</option>
        </select>
      </label>
      <label>
        증거 검색
        <input bind:value={search} maxlength="128" placeholder="닉네임, 설명, 채팅 증거" />
      </label>
      <button type="button" onclick={() => load(true)} disabled={loading}
        ><FileSearch size={15} /> 검색</button
      >
    </section>

    <details class="integrity-panel panel">
      <summary
        ><ShieldAlert size={15} /> 게임 무결성 탐지 신호 <strong>{signals.length}</strong></summary
      >
      <div class="integrity-tools">
        <label
          >탐지 종류
          <select bind:value={signalKind} onchange={loadSignals}>
            <option value="">전체</option>
            <option value="IMPOSSIBLE_ORDER">불가능한 명령 순서</option>
            <option value="AUTOMATION">자동화 이벤트 폭주</option>
            <option value="COLLUSION">담합 의심</option>
            <option value="INTENTIONAL_STALLING">고의 지연</option>
          </select></label
        >
        <p>신호는 자동 제재가 아니라 운영자 검토를 위한 위험 근거입니다.</p>
      </div>
      <div class="integrity-list">
        {#if signals.length === 0}
          <p class="empty">검색 조건에 해당하는 무결성 신호가 없습니다.</p>
        {:else}
          {#each signals as signal (signal.id)}
            <article>
              <span class="signal-severity">SEV {signal.severity}</span>
              <strong>{signal.kind}</strong>
              <small>{signal.subjectIdentityId} · {formatTime(signal.lastObservedAt)}</small>
              <p>신뢰도 {Math.round(signal.confidence * 100)}% · {signal.occurrences}회 관측</p>
              <details
                ><summary>탐지 근거</summary><pre
                  >{JSON.stringify(signal.evidence, null, 2)}</pre
                ></details
              >
            </article>
          {/each}
        {/if}
      </div>
    </details>

    <div class="operations-grid">
      <section class="case-list panel" aria-label="신고 사건 목록">
        <header><strong>사건 큐</strong><small>{cases.length}건 표시</small></header>
        {#if loading && cases.length === 0}
          <p class="empty">운영 큐 동기화 중…</p>
        {:else if cases.length === 0}
          <p class="empty">조건에 해당하는 신고가 없습니다.</p>
        {:else}
          {#each cases as item (item.report.id)}
            <button
              type="button"
              class:selected={selectedId === item.report.id}
              onclick={() => (selectedId = item.report.id)}
            >
              <span class={`status status--${item.report.status.toLowerCase()}`}
                >{item.report.status}</span
              >
              <strong>{item.report.targetNickname}</strong>
              <small>{item.report.category} · {formatTime(item.report.createdAt)}</small>
              <p>{item.report.details}</p>
            </button>
          {/each}
          {#if nextBefore}
            <button class="load-more" type="button" onclick={() => load(false)} disabled={loading}
              ><ChevronDown size={14} /> 이전 사건 더 보기</button
            >
          {/if}
        {/if}
      </section>

      <section class="case-detail panel" aria-live="polite">
        {#if selectedCase}
          <header>
            <div>
              <p class="eyebrow">CASE {selectedCase.report.id.slice(0, 8)}</p>
              <h2>{selectedCase.report.targetNickname}</h2>
            </div>
            <span class={`status status--${selectedCase.report.status.toLowerCase()}`}
              >{selectedCase.report.status}</span
            >
          </header>
          <dl>
            <div>
              <dt>분류</dt>
              <dd>{selectedCase.report.category}</dd>
            </div>
            <div>
              <dt>신고 시각</dt>
              <dd>{formatTime(selectedCase.report.createdAt)}</dd>
            </div>
            <div>
              <dt>방 ID</dt>
              <dd>{selectedCase.report.roomId}</dd>
            </div>
            <div>
              <dt>대상 신원</dt>
              <dd>{selectedCase.report.targetIdentityId}</dd>
            </div>
          </dl>
          <article class="report-copy">
            <strong>신고 내용</strong>
            <p>{selectedCase.report.details}</p>
          </article>
          <details class="evidence" open>
            <summary>서버 보존 증거</summary>
            <pre>{JSON.stringify(selectedCase.report.evidence, null, 2)}</pre>
          </details>

          <section class="audit-log">
            <h3>감사 이력</h3>
            {#if selectedCase.actions.length === 0}
              <p class="empty">아직 기록된 조치가 없습니다.</p>
            {:else}
              {#each selectedCase.actions as item (item.id)}
                <article>
                  <strong>{item.action}</strong>
                  <span>{item.operatorId} · {formatTime(item.createdAt)}</span>
                  <p>{item.reason}</p>
                  {#if item.expiresAt}<small>만료 {formatTime(item.expiresAt)}</small>{/if}
                  {#if item.reversesActionId}<small>취소 대상 {item.reversesActionId}</small>{/if}
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
            <h3><Ban size={15} /> 운영 조치 기록</h3>
            <div class="action-fields">
              <label
                >조치
                <select bind:value={action}>
                  <option value="WARN">경고</option>
                  <option value="SUSPEND">기간 정지</option>
                  <option value="BAN">영구 제한</option>
                  <option value="DISMISS">신고 기각</option>
                  <option value="REVERSE">기존 조치 취소</option>
                </select></label
              >
              {#if action === 'SUSPEND'}
                <label
                  >정지 시간
                  <input bind:value={durationHours} type="number" min="1" max="8760" /></label
                >
              {:else if action === 'REVERSE'}
                <label
                  >취소할 조치
                  <select bind:value={reversesActionId} required>
                    <option value="" disabled>조치 선택</option>
                    {#each reversibleActions as item (item.id)}
                      <option value={item.id}>{item.action} · {formatTime(item.createdAt)}</option>
                    {/each}
                  </select></label
                >
              {/if}
            </div>
            <label
              >판단 근거
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
              {acting ? '기록 중…' : '감사 이력에 조치 확정'}</button
            >
          </form>
        {:else}
          <p class="empty">검토할 사건을 선택하십시오.</p>
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
    .action-fields {
      grid-template-columns: 1fr;
    }
    .page-head {
      align-items: start;
    }
  }
</style>
