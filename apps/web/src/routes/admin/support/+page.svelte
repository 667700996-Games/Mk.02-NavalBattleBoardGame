<script lang="ts">
  import { Headphones, KeyRound, LockKeyhole, Search, ShieldCheck } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { api, ApiError } from '$lib/api';
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

  const formatTime = (value: string) => new Date(value).toLocaleString('ko-KR');
  const errorMessage = (error: unknown, fallback: string) =>
    error instanceof ApiError ? `${error.message} (${error.code})` : fallback;

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
      notice = errorMessage(caught, '지원 계정을 조회하지 못했습니다.');
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
      notice = `${response.action.affectedSessionIds.length}개 세션을 회수하고 감사 이력에 기록했습니다.`;
      result = await api.supportAccount(token.trim(), result.account.id);
      reason = '';
      confirmation = '';
      selectedSessionId = '';
    } catch (caught) {
      notice = errorMessage(caught, '세션을 회수하지 못했습니다.');
    } finally {
      acting = false;
    }
  }
</script>

<svelte:head><title>플레이어 지원 운영 · Mk.01</title></svelte:head>

<main class="support-page shell">
  <header class="page-head">
    <div>
      <p class="eyebrow">PLAYER SUPPORT OPERATIONS</p>
      <h1 class="page-title">계정 지원 센터</h1>
      <p>정확한 계정 식별자를 조회하고 침해되거나 분실된 세션을 감사 가능한 방식으로 회수합니다.</p>
    </div>
    <Headphones size={34} aria-hidden="true" />
  </header>

  <nav class="admin-nav" aria-label="운영 도구">
    <a aria-current="page" href={resolve('/admin/support')}>고객지원</a>
    <a href={resolve('/admin/moderation')}>신뢰 및 안전</a>
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
        <h2>운영자 인증 및 정확 검색</h2>
        <p>토큰은 저장되지 않습니다. 계정 UUID 또는 핸들이 완전히 일치할 때만 결과를 표시합니다.</p>
      </div>
    </header>
    <label>운영자 ID <input bind:value={operatorId} minlength="2" maxlength="64" required /></label>
    <label>
      관리 토큰
      <input bind:value={token} type="password" minlength="32" autocomplete="off" required />
    </label>
    <label>
      계정 UUID 또는 정확한 핸들
      <input bind:value={query} minlength="3" maxlength="64" autocomplete="off" required />
    </label>
    <button type="submit" disabled={loading || query.trim().length < 3}>
      <Search size={15} aria-hidden="true" />
      {loading ? '조회 중…' : '지원 계정 조회'}
    </button>
  </form>

  {#if result}
    <section class="identity panel" aria-labelledby="support-account-heading">
      <header>
        <div>
          <p class="eyebrow">VERIFIED ACCOUNT</p>
          <h2 id="support-account-heading">{result.account.handle}</h2>
        </div>
        <ShieldCheck size={26} aria-label="정확 일치 확인" />
      </header>
      <dl>
        <div>
          <dt>계정 ID</dt>
          <dd>{result.account.id}</dd>
        </div>
        <div>
          <dt>생성 시각</dt>
          <dd>{formatTime(result.account.createdAt)}</dd>
        </div>
        <div>
          <dt>활성 세션</dt>
          <dd>{result.sessions.length}개</dd>
        </div>
      </dl>
    </section>

    <div class="support-grid">
      <section class="sessions panel">
        <header>
          <h2>세션 보안 조치</h2>
          <KeyRound size={19} aria-hidden="true" />
        </header>
        {#if result.sessions.length === 0}
          <p class="empty">현재 회수할 활성 세션이 없습니다.</p>
        {:else}
          <fieldset>
            <legend>회수 범위</legend>
            <label class="session-choice">
              <input bind:group={selectedSessionId} type="radio" value="" />
              <span><strong>모든 활성 세션</strong><small>계정 침해 대응에 사용</small></span>
            </label>
            {#each result.sessions as session (session.id)}
              <label class="session-choice">
                <input bind:group={selectedSessionId} type="radio" value={session.id} />
                <span>
                  <strong>{session.nickname}</strong>
                  <small>마지막 활동 {formatTime(session.lastSeenAt)}</small>
                  <small
                    >{session.currentRoomId
                      ? `활성 방 ${session.currentRoomId}`
                      : '활성 방 없음'}</small
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
              확인된 지원 사유
              <textarea bind:value={reason} minlength="8" maxlength="500" rows="3" required
              ></textarea>
            </label>
            <label>
              위험 동작 확인: <strong>{result.account.handle}</strong> 입력
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
              {acting ? '감사 기록 중…' : selectedSessionId ? '선택 세션 회수' : '모든 세션 회수'}
            </button>
          </form>
        {/if}
      </section>

      <section class="audit panel" aria-live="polite">
        <header>
          <h2>지원 감사 이력</h2>
          <span>{result.actions.length}</span>
        </header>
        {#if result.actions.length === 0}
          <p class="empty">이 계정에 기록된 지원 조치가 없습니다.</p>
        {:else}
          {#each result.actions as action (action.id)}
            <article>
              <strong>{action.action}</strong>
              <small>{action.operatorId} · {formatTime(action.createdAt)}</small>
              <p>{action.reason}</p>
              <small>{action.affectedSessionIds.length}개 세션 회수</small>
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
