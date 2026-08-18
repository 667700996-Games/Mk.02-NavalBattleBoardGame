<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    Ban,
    ChevronDown,
    Flag,
    MessageSquare,
    Radio,
    Send,
    ShieldAlert,
    Smile,
    Volume2,
    VolumeX,
    X,
    Zap
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { chatHistoryLoaded, chatMessages, chatTyping } from '$lib/stores';
  import { Modal } from '$lib/ui';
  import InputPrompt from './InputPrompt.svelte';
  import {
    CHAT_EMOJIS,
    QUICK_COMMANDS,
    type ChatMessageType,
    type QuickCommandId,
    type ReportCategory
  } from '$lib/types';

  interface Props {
    roomId: string;
    selfPlayerId: string;
    online: boolean;
    readOnly?: boolean;
    targetPlayerId?: string;
    targetNickname?: string;
  }

  let {
    roomId,
    selfPlayerId,
    online,
    readOnly = false,
    targetPlayerId,
    targetNickname
  }: Props = $props();
  let open = $state(false);
  let unread = $state(0);
  let draft = $state('');
  let notice = $state('');
  let atBottom = $state(true);
  let messageList = $state<HTMLDivElement>();
  let messageInput = $state<HTMLTextAreaElement>();
  let drawerToggle = $state<HTMLButtonElement>();
  let previousLastMessageId: string | null = null;
  let historyReady = false;
  let typingSent = false;
  let typingTimer: ReturnType<typeof setTimeout> | null = null;
  let actionCooldownTimer: ReturnType<typeof setTimeout> | null = null;
  let showActions = $state(false);
  let actionTab = $state<'commands' | 'emoji'>('commands');
  let actionCooling = $state(false);
  let showSafety = $state(false);
  let muted = $state(false);
  let blocked = $state(false);
  let safetyBusy = $state(false);
  let safetyNotice = $state('');
  let reportCategory = $state<ReportCategory>('CHAT');
  let reportDetails = $state('');
  let reportSubmitted = $state(false);
  let visibleMessages = $derived(
    $chatMessages.filter((message) => !(muted && message.playerId === targetPlayerId))
  );
  let recentActions = $state<
    Array<{
      type: 'QUICK_COMMAND' | 'EMOJI';
      content: string | null;
      commandId: QuickCommandId | null;
      label: string;
    }>
  >([]);

  $effect(() => {
    const messages = visibleMessages;
    const lastMessageId = messages.at(-1)?.messageId ?? null;
    const loaded = $chatHistoryLoaded;
    if (!loaded) {
      previousLastMessageId = lastMessageId;
      return;
    }
    if (!historyReady) {
      historyReady = true;
      previousLastMessageId = lastMessageId;
      if (open) queueMicrotask(scrollToBottom);
      return;
    }
    if (!lastMessageId || lastMessageId === previousLastMessageId) return;
    const previousIndex = previousLastMessageId
      ? messages.findIndex((message) => message.messageId === previousLastMessageId)
      : -1;
    const added = previousIndex >= 0 ? messages.length - previousIndex - 1 : 1;
    previousLastMessageId = lastMessageId;
    if (open && atBottom) queueMicrotask(scrollToBottom);
    else unread += added;
  });

  function scrollToBottom() {
    if (!messageList) return;
    messageList.scrollTop = messageList.scrollHeight;
    atBottom = true;
    unread = 0;
  }

  function handleScroll() {
    if (!messageList) return;
    atBottom = messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight < 32;
    if (atBottom) unread = 0;
  }

  function toggleDrawer() {
    open = !open;
    if (open) {
      queueMicrotask(() => {
        scrollToBottom();
        messageInput?.focus();
      });
    } else {
      queueMicrotask(() => drawerToggle?.focus());
    }
  }

  function closeDrawer() {
    if (!open) return;
    open = false;
    queueMicrotask(() => drawerToggle?.focus());
  }

  async function openSafety() {
    if (!targetPlayerId || !targetNickname) return;
    showSafety = true;
    safetyNotice = '';
    reportSubmitted = false;
    try {
      const relationships = (await api.socialRelationships()).relationships;
      const relationship = relationships.find((item) => item.targetNickname === targetNickname);
      muted = relationship?.muted ?? false;
      blocked = relationship?.blocked ?? false;
    } catch (caught) {
      safetyNotice =
        caught instanceof ApiError ? caught.message : '안전 설정을 불러오지 못했습니다.';
    }
  }

  async function updateSafety(nextMuted: boolean, nextBlocked: boolean) {
    if (!targetPlayerId || safetyBusy) return;
    safetyBusy = true;
    safetyNotice = '';
    try {
      const relationship = await api.updateSocialRelationship(
        roomId,
        targetPlayerId,
        nextMuted,
        nextBlocked
      );
      muted = relationship.muted;
      blocked = relationship.blocked;
      realtime.sync(roomId);
    } catch (caught) {
      safetyNotice =
        caught instanceof ApiError ? caught.message : '안전 설정을 변경하지 못했습니다.';
    } finally {
      safetyBusy = false;
    }
  }

  async function submitReport() {
    if (!targetPlayerId || safetyBusy || reportDetails.trim().length < 4) return;
    safetyBusy = true;
    safetyNotice = '';
    try {
      const response = await api.reportPlayer(
        roomId,
        targetPlayerId,
        reportCategory,
        reportDetails.trim()
      );
      reportSubmitted = true;
      reportDetails = '';
      safetyNotice = `신고 ${response.report.reportId.slice(0, 8)} 접수 완료`;
    } catch (caught) {
      safetyNotice = caught instanceof ApiError ? caught.message : '신고를 접수하지 못했습니다.';
    } finally {
      safetyBusy = false;
    }
  }

  function sendTyping(isTyping: boolean) {
    if (!online || readOnly || typingSent === isTyping) return;
    typingSent = isTyping;
    realtime.send({ type: 'chat:typing', payload: { roomId, isTyping } });
  }

  function handleInput() {
    notice = '';
    const hasText = draft.trim().length > 0;
    sendTyping(hasText);
    if (typingTimer) clearTimeout(typingTimer);
    if (hasText) {
      typingTimer = setTimeout(() => sendTyping(false), 1_200);
    }
  }

  function sendMessage() {
    const message = draft.trim();
    if (!message || readOnly) return;
    if (message.length > 300) {
      notice = '메시지는 최대 300자까지 입력할 수 있습니다.';
      return;
    }
    if (message.includes('<') || message.includes('>')) {
      notice = 'HTML 문법은 채팅에 사용할 수 없습니다.';
      return;
    }
    if (
      !online ||
      !realtime.send({
        type: 'chat:send',
        payload: {
          roomId,
          clientMessageId: crypto.randomUUID(),
          type: 'TEXT',
          content: message,
          commandId: null
        }
      })
    ) {
      notice = '실시간 연결이 복구된 뒤 다시 전송해 주세요.';
      return;
    }
    draft = '';
    notice = '';
    sendTyping(false);
  }

  function openActions(tab: 'commands' | 'emoji') {
    actionTab = tab;
    showActions = true;
  }

  function sendAction(
    type: 'QUICK_COMMAND' | 'EMOJI',
    label: string,
    content: string | null,
    commandId: QuickCommandId | null
  ) {
    if (!online || readOnly || actionCooling) return;
    const sent = realtime.send({
      type: 'chat:send',
      payload: {
        roomId,
        clientMessageId: crypto.randomUUID(),
        type,
        content,
        commandId
      }
    });
    if (!sent) {
      notice = '실시간 연결이 복구된 뒤 다시 전송해 주세요.';
      return;
    }
    const next = { type, content, commandId, label };
    recentActions = [
      next,
      ...recentActions.filter((action) => action.type !== type || action.label !== label)
    ].slice(0, 4);
    showActions = false;
    actionCooling = true;
    if (actionCooldownTimer) clearTimeout(actionCooldownTimer);
    actionCooldownTimer = setTimeout(() => (actionCooling = false), 700);
  }

  const messageLabel = (type: ChatMessageType) =>
    type === 'QUICK_COMMAND' ? 'QUICK COMMAND' : type === 'EMOJI' ? 'TACTICAL SIGNAL' : '';

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  }

  function formatTime(timestamp: string): string {
    return new Date(timestamp).toLocaleTimeString('ko-KR', {
      hour: '2-digit',
      minute: '2-digit'
    });
  }

  onDestroy(() => {
    if (typingTimer) clearTimeout(typingTimer);
    if (actionCooldownTimer) clearTimeout(actionCooldownTimer);
    if (typingSent) {
      realtime.send({ type: 'chat:typing', payload: { roomId, isTyping: false } });
    }
  });
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && closeDrawer()} />

<div class:chat-shell--open={open} class="chat-shell">
  {#if open}
    <section class="chat-drawer" aria-label="전술 채팅">
      <span class="chat-drawer__scan" aria-hidden="true"></span>
      <header>
        <span class="chat-drawer__icon"><MessageSquare size={16} /></span>
        <div>
          <small>SECURE ROOM CHANNEL</small>
          <h2>전술 채팅</h2>
        </div>
        <span class:offline={!online} class="chat-link-status"
          ><i></i>{online ? 'LIVE' : 'OFFLINE'}</span
        >
        {#if targetPlayerId}
          <button
            class="ui-icon-button"
            type="button"
            onclick={openSafety}
            aria-label="플레이어 안전 설정"
          >
            <ShieldAlert size={15} />
          </button>
        {/if}
        <button class="ui-icon-button" type="button" onclick={toggleDrawer} aria-label="채팅 닫기">
          <X size={15} />
        </button>
      </header>

      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Safari requires keyboard focus for scrollable logs) -->
      <div
        bind:this={messageList}
        class="chat-messages"
        role="log"
        tabindex="0"
        aria-label="채팅 기록"
        aria-live="polite"
        aria-relevant="additions"
        onscroll={handleScroll}
      >
        {#if !$chatHistoryLoaded}
          <div class="chat-loading"><Radio size={18} /><span>보안 채널 동기화 중…</span></div>
        {:else if visibleMessages.length === 0}
          <div class="chat-empty">
            <MessageSquare size={20} /><span>아직 전송된 메시지가 없습니다.</span>
          </div>
        {:else}
          {#each visibleMessages as item (item.messageId)}
            <article
              class:chat-message--self={item.playerId === selfPlayerId}
              class:chat-message--system={item.type === 'SYSTEM'}
              class:chat-message--quick={item.type === 'QUICK_COMMAND'}
              class:chat-message--emoji={item.type === 'EMOJI'}
              class="chat-message"
            >
              <div>
                <strong
                  >{item.type === 'SYSTEM'
                    ? 'SYSTEM'
                    : item.playerId === selfPlayerId
                      ? 'YOU'
                      : item.nickname}</strong
                >
                <time datetime={item.timestamp}>{formatTime(item.timestamp)}</time>
              </div>
              {#if messageLabel(item.type)}<small>{messageLabel(item.type)}</small>{/if}
              <p>{item.content}</p>
            </article>
          {/each}
        {/if}
      </div>

      {#if unread > 0 && !atBottom}
        <button class="new-message-button" type="button" onclick={scrollToBottom}>
          <ChevronDown size={13} /> 새 메시지 {unread}개
        </button>
      {/if}

      <div class="chat-composer">
        <div class="typing-line" aria-live="polite">
          {#if $chatTyping?.isTyping}<span><i></i>{$chatTyping.nickname} 입력 중…</span
            >{:else}<InputPrompt context="chat" compact />{/if}
          <em>{draft.length}/300</em>
        </div>
        <div class="chat-input-row">
          <button
            type="button"
            class="chat-action"
            aria-label="이모지 선택"
            disabled={readOnly || !online || actionCooling}
            onclick={() => openActions('emoji')}><Smile size={15} /></button
          >
          <button
            type="button"
            class="chat-action"
            aria-label="빠른 명령 선택"
            disabled={readOnly || !online || actionCooling}
            onclick={() => openActions('commands')}><Zap size={15} /></button
          >
          <textarea
            bind:this={messageInput}
            bind:value={draft}
            aria-label="채팅 메시지"
            maxlength="300"
            rows="1"
            placeholder={readOnly
              ? '작전 종료 · 기록 열람 전용'
              : online
                ? '메시지 입력…'
                : '연결 복구 대기 중…'}
            disabled={readOnly || !online}
            oninput={handleInput}
            onkeydown={handleKeydown}></textarea>
          <button
            type="button"
            class="chat-send"
            aria-label="채팅 전송"
            disabled={readOnly || !online || !draft.trim()}
            onclick={sendMessage}><Send size={15} /></button
          >
        </div>
        {#if notice}<p class="chat-notice" role="alert">{notice}</p>{/if}
      </div>
    </section>
  {:else}
    <button
      bind:this={drawerToggle}
      class="chat-toggle"
      type="button"
      onclick={toggleDrawer}
      aria-label="전술 채팅 열기"
    >
      <MessageSquare size={18} />
      <span>CHAT</span>
      {#if unread > 0}<strong aria-label={`읽지 않은 메시지 ${unread}개`}
          >{unread > 99 ? '99+' : unread}</strong
        >{/if}
    </button>
  {/if}
</div>

<Modal
  open={showActions}
  eyebrow="ROOM SIGNAL PROTOCOL"
  title="전술 신호 선택"
  description="선택한 신호는 현재 작전실에 즉시 전송됩니다."
  onclose={() => (showActions = false)}
>
  <div class="signal-picker">
    <div class="signal-tabs" role="tablist" aria-label="전술 신호 종류">
      <button
        type="button"
        role="tab"
        aria-selected={actionTab === 'commands'}
        class:active={actionTab === 'commands'}
        onclick={() => (actionTab = 'commands')}><Zap size={14} /> 빠른 명령</button
      >
      <button
        type="button"
        role="tab"
        aria-selected={actionTab === 'emoji'}
        class:active={actionTab === 'emoji'}
        onclick={() => (actionTab = 'emoji')}><Smile size={14} /> 이모지</button
      >
    </div>
    {#if recentActions.length}
      <section class="recent-signals" aria-label="최근 사용">
        <small>RECENT SIGNALS</small>
        <div>
          {#each recentActions as action (`${action.type}-${action.label}`)}
            <button
              type="button"
              disabled={actionCooling}
              onclick={() =>
                sendAction(action.type, action.label, action.content, action.commandId)}
              >{action.label}</button
            >
          {/each}
        </div>
      </section>
    {/if}
    {#if actionTab === 'commands'}
      <div class="command-grid" role="tabpanel">
        {#each QUICK_COMMANDS as command (command.id)}
          <button
            type="button"
            disabled={actionCooling}
            onclick={() => sendAction('QUICK_COMMAND', command.label, null, command.id)}
            ><Zap size={12} /><span>{command.label}</span><small>{command.id}</small></button
          >
        {/each}
      </div>
    {:else}
      <div class="emoji-grid" role="tabpanel">
        {#each CHAT_EMOJIS as emoji (emoji)}
          <button
            type="button"
            aria-label={`${emoji} 이모지 전송`}
            disabled={actionCooling}
            onclick={() => sendAction('EMOJI', emoji, emoji, null)}>{emoji}</button
          >
        {/each}
      </div>
    {/if}
    {#if actionCooling}<p class="signal-cooldown"><Radio size={12} /> SIGNAL COOLDOWN</p>{/if}
  </div>
</Modal>

<Modal
  open={showSafety}
  eyebrow="PLAYER SAFETY"
  title={`${targetNickname ?? '상대 플레이어'} 안전 설정`}
  description="음소거와 차단은 즉시 적용됩니다. 신고에는 현재 방 상태와 최근 대화가 증거로 함께 보존됩니다."
  onclose={() => (showSafety = false)}
>
  <div class="safety-panel">
    <section class="safety-controls" aria-label="플레이어 안전 제어">
      <button
        type="button"
        class:active={muted}
        disabled={safetyBusy || blocked}
        onclick={() => updateSafety(!muted, blocked)}
      >
        {#if muted}<Volume2 size={17} /> 음소거 해제{:else}<VolumeX size={17} /> 음소거{/if}
        <small
          >{blocked ? '차단 중에는 음소거가 유지됩니다.' : '채팅과 입력 알림을 숨깁니다.'}</small
        >
      </button>
      <button
        type="button"
        class:danger={blocked}
        disabled={safetyBusy}
        onclick={() => updateSafety(blocked ? muted : true, !blocked)}
      >
        <Ban size={17} />
        {blocked ? '차단 해제' : '플레이어 차단'}
        <small
          >{blocked
            ? '다시 같은 방과 매치에 참가할 수 있습니다.'
            : '통신과 재매칭을 막습니다.'}</small
        >
      </button>
    </section>

    <form
      class="safety-report"
      onsubmit={(event) => {
        event.preventDefault();
        submitReport();
      }}
    >
      <div class="safety-heading">
        <span><Flag size={15} /> 플레이어 신고</span>
        <small>4–1000자</small>
      </div>
      <label>
        신고 사유
        <select bind:value={reportCategory} disabled={safetyBusy}>
          <option value="CHAT">부적절한 채팅</option>
          <option value="NAME">부적절한 이름</option>
          <option value="CHEATING">치팅 의심</option>
          <option value="STALLING">고의 지연</option>
          <option value="OTHER">기타</option>
        </select>
      </label>
      <label>
        상세 내용
        <textarea
          bind:value={reportDetails}
          minlength="4"
          maxlength="1000"
          rows="4"
          disabled={safetyBusy}
          placeholder="발생한 상황을 구체적으로 적어 주세요."></textarea>
      </label>
      <button
        class="safety-submit"
        type="submit"
        disabled={safetyBusy || reportDetails.trim().length < 4}
        ><Flag size={14} /> {safetyBusy ? '처리 중…' : '증거와 함께 신고 접수'}</button
      >
    </form>

    {#if safetyNotice}
      <p
        class:success={reportSubmitted}
        class="safety-notice"
        role={reportSubmitted ? 'status' : 'alert'}
      >
        {safetyNotice}
      </p>
    {/if}
  </div>
</Modal>

<style>
  .chat-shell {
    position: fixed;
    z-index: 64;
    right: max(20px, env(safe-area-inset-right));
    bottom: max(20px, env(safe-area-inset-bottom));
  }
  .chat-toggle {
    position: relative;
    display: flex;
    height: 44px;
    align-items: center;
    gap: 8px;
    padding: 0 15px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    color: var(--cyan-300);
    background: rgba(5, 22, 33, 0.9);
    box-shadow: var(--shadow-sm), var(--glow-cyan);
    backdrop-filter: blur(18px);
    cursor: pointer;
    transition: 220ms var(--ease-out);
  }
  .chat-toggle:hover {
    border-color: var(--line-hot);
    transform: translateY(-2px);
  }
  .chat-toggle > span {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.16em;
  }
  .chat-toggle strong {
    position: absolute;
    top: -7px;
    right: -7px;
    display: grid;
    min-width: 20px;
    height: 20px;
    place-items: center;
    padding-inline: 5px;
    border: 2px solid var(--navy-950);
    border-radius: 10px;
    color: #071116;
    background: var(--orange-400);
    font-family: var(--font-display);
    font-size: 9px;
  }
  .chat-drawer {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    width: min(370px, calc(100vw - 32px));
    height: min(540px, calc(100vh - 112px));
    overflow: hidden;
    border: 1px solid var(--line-strong);
    border-radius: 18px;
    background:
      radial-gradient(circle at 80% 0%, rgba(40, 223, 232, 0.09), transparent 34%),
      rgba(4, 17, 27, 0.94);
    box-shadow:
      0 28px 80px rgba(0, 0, 0, 0.52),
      var(--glow-cyan);
    backdrop-filter: blur(24px) saturate(1.25);
    animation: chat-enter 260ms var(--ease-out);
  }
  .chat-drawer__scan {
    position: absolute;
    z-index: 0;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(transparent 49%, rgba(40, 223, 232, 0.025) 50%, transparent 51%);
    background-size: 100% 6px;
    opacity: 0.45;
  }
  .chat-drawer > header {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 10px;
    min-height: 64px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    background: rgba(8, 29, 41, 0.58);
  }
  .chat-drawer__icon {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid rgba(40, 223, 232, 0.22);
    border-radius: 9px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .chat-drawer header div {
    display: grid;
    gap: 2px;
  }
  .chat-drawer header small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.15em;
  }
  .chat-drawer header h2 {
    margin: 0;
    font-size: 13px;
  }
  .chat-link-status {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--green-400);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.08em;
  }
  .chat-link-status i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 7px currentColor;
  }
  .chat-link-status.offline {
    color: var(--red-400);
  }
  .chat-messages {
    position: relative;
    z-index: 1;
    display: grid;
    align-content: start;
    gap: 0;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: rgba(40, 223, 232, 0.24) transparent;
  }
  .chat-message {
    display: grid;
    gap: 5px;
    padding: 11px 14px;
    border-bottom: 1px solid rgba(132, 191, 211, 0.07);
    background: transparent;
  }
  .chat-message:hover {
    background: rgba(80, 173, 194, 0.035);
  }
  .chat-message > div {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chat-message strong {
    color: var(--orange-400);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.11em;
  }
  .chat-message--self strong {
    color: var(--cyan-300);
  }
  .chat-message--system {
    border-left: 2px solid rgba(79, 226, 173, 0.34);
    background: rgba(79, 226, 173, 0.025);
  }
  .chat-message--system strong {
    color: var(--green-400);
  }
  .chat-message time {
    color: var(--ink-600);
    font-family: var(--font-mono);
    font-size: 8px;
  }
  .chat-message p {
    margin: 0;
    color: var(--ink-200);
    font-size: 11px;
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .chat-message--system p {
    color: var(--ink-400);
    font-size: 10px;
  }
  .chat-message > small {
    color: var(--amber-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.14em;
  }
  .chat-message--quick {
    border-left: 2px solid rgba(255, 180, 60, 0.36);
    background: rgba(255, 180, 60, 0.025);
  }
  .chat-message--emoji p {
    font-size: 24px;
    line-height: 1.2;
  }
  .chat-loading,
  .chat-empty {
    display: grid;
    min-height: 210px;
    place-items: center;
    align-content: center;
    gap: 10px;
    color: var(--ink-500);
    font-size: 10px;
  }
  .chat-loading :global(svg) {
    color: var(--cyan-400);
    animation: pulse 1.2s infinite;
  }
  .new-message-button {
    position: absolute;
    z-index: 4;
    right: 50%;
    bottom: 102px;
    display: flex;
    min-height: 28px;
    align-items: center;
    gap: 5px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    color: var(--cyan-200);
    background: rgba(7, 30, 42, 0.96);
    box-shadow: var(--shadow-sm);
    transform: translateX(50%);
    cursor: pointer;
    font-size: 9px;
  }
  .chat-composer {
    position: relative;
    z-index: 2;
    padding: 9px 11px 11px;
    border-top: 1px solid var(--line);
    background: rgba(3, 14, 22, 0.86);
  }
  .typing-line {
    display: flex;
    min-height: 20px;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    color: var(--ink-600);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.08em;
  }
  .typing-line span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .typing-line span i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 6px var(--cyan-300);
    animation: pulse 700ms infinite;
  }
  .typing-line :global(.input-prompt) {
    min-width: 0;
    color: var(--ink-500);
  }
  .typing-line em {
    font-style: normal;
  }
  .chat-input-row {
    display: grid;
    grid-template-columns: 34px 34px minmax(0, 1fr) 38px;
    gap: 7px;
  }
  .chat-input-row textarea {
    width: 100%;
    min-height: 40px;
    max-height: 94px;
    resize: vertical;
    padding: 10px 11px;
    border: 1px solid var(--line);
    border-radius: 10px;
    outline: 0;
    color: var(--ink-100);
    background: rgba(3, 13, 20, 0.8);
    font: inherit;
    font-size: 11px;
    line-height: 1.45;
    transition: 180ms ease;
  }
  .chat-input-row textarea:focus {
    border-color: var(--line-hot);
    box-shadow: 0 0 0 3px rgba(40, 223, 232, 0.07);
  }
  .chat-input-row textarea:disabled {
    opacity: 0.55;
  }
  .chat-send {
    display: grid;
    height: 40px;
    place-items: center;
    border: 1px solid rgba(40, 223, 232, 0.38);
    border-radius: 10px;
    color: #031419;
    background: linear-gradient(135deg, var(--cyan-300), var(--cyan-500));
    box-shadow: 0 0 20px rgba(40, 223, 232, 0.09);
    cursor: pointer;
  }
  .chat-action {
    display: grid;
    height: 40px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 10px;
    color: var(--cyan-300);
    background: rgba(7, 29, 40, 0.72);
    cursor: pointer;
    transition: 180ms var(--ease-out);
  }
  .chat-action:hover:not(:disabled) {
    border-color: var(--line-hot);
    background: rgba(40, 223, 232, 0.08);
  }
  .chat-action:disabled {
    cursor: not-allowed;
    opacity: 0.35;
  }
  .chat-send:disabled {
    cursor: not-allowed;
    filter: saturate(0.2);
    opacity: 0.38;
  }
  .chat-notice {
    margin: 6px 2px 0;
    color: var(--red-400);
    font-size: 9px;
  }
  @keyframes chat-enter {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.98);
    }
  }
  .signal-picker {
    display: grid;
    gap: 16px;
    margin-top: 18px;
  }
  .signal-tabs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(2, 13, 21, 0.68);
  }
  .signal-tabs button {
    display: flex;
    min-height: 38px;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 0;
    border-radius: 8px;
    color: var(--ink-500);
    background: transparent;
    cursor: pointer;
    font-size: 10px;
  }
  .signal-tabs button.active {
    color: var(--cyan-200);
    background: rgba(40, 223, 232, 0.09);
    box-shadow: inset 0 0 0 1px rgba(40, 223, 232, 0.16);
  }
  .recent-signals {
    display: grid;
    gap: 8px;
  }
  .recent-signals > small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .recent-signals > div {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .recent-signals button {
    padding: 6px 9px;
    border: 1px solid var(--line);
    border-radius: 999px;
    color: var(--ink-300);
    background: rgba(6, 25, 35, 0.7);
    cursor: pointer;
    font-size: 9px;
  }
  .command-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
  }
  .command-grid button {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 7px;
    padding: 11px;
    border: 1px solid var(--line);
    border-radius: 10px;
    color: var(--amber-500);
    background: rgba(7, 26, 36, 0.66);
    cursor: pointer;
    text-align: left;
  }
  .command-grid button:hover {
    border-color: rgba(255, 180, 60, 0.28);
    transform: translateY(-1px);
  }
  .command-grid span {
    color: var(--ink-200);
    font-size: 10px;
  }
  .command-grid small {
    grid-column: 2;
    color: var(--ink-600);
    font-family: var(--font-display);
    font-size: 7px;
  }
  .emoji-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 7px;
  }
  .emoji-grid button {
    display: grid;
    min-height: 52px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: rgba(7, 26, 36, 0.66);
    cursor: pointer;
    font-size: 24px;
    transition: 180ms var(--ease-out);
  }
  .emoji-grid button:hover {
    border-color: var(--line-hot);
    transform: translateY(-2px) scale(1.03);
  }
  .signal-cooldown {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin: 0;
    color: var(--amber-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.12em;
  }
  .safety-panel {
    display: grid;
    gap: 16px;
    margin-top: 18px;
  }
  .safety-controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .safety-controls button {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 4px 8px;
    min-height: 66px;
    padding: 11px;
    border: 1px solid var(--line);
    border-radius: 11px;
    color: var(--ink-200);
    background: rgba(7, 26, 36, 0.66);
    cursor: pointer;
    text-align: left;
  }
  .safety-controls button :global(svg) {
    color: var(--cyan-300);
  }
  .safety-controls button.active {
    border-color: rgba(255, 180, 60, 0.34);
    background: rgba(255, 180, 60, 0.07);
  }
  .safety-controls button.danger {
    border-color: rgba(255, 92, 92, 0.4);
    background: rgba(255, 92, 92, 0.08);
  }
  .safety-controls button.danger :global(svg) {
    color: var(--red-400);
  }
  .safety-controls button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .safety-controls small {
    grid-column: 1 / -1;
    color: var(--ink-500);
    font-size: 8px;
    line-height: 1.45;
  }
  .safety-report {
    display: grid;
    gap: 10px;
    padding-top: 15px;
    border-top: 1px solid var(--line);
  }
  .safety-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--ink-200);
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.08em;
  }
  .safety-heading span {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .safety-heading :global(svg) {
    color: var(--red-400);
  }
  .safety-heading small {
    color: var(--ink-600);
    font-size: 8px;
  }
  .safety-report label {
    display: grid;
    gap: 6px;
    color: var(--ink-400);
    font-size: 9px;
  }
  .safety-report select,
  .safety-report textarea {
    width: 100%;
    padding: 10px 11px;
    border: 1px solid var(--line);
    border-radius: 9px;
    outline: 0;
    color: var(--ink-100);
    background: rgba(3, 13, 20, 0.8);
    font: inherit;
    font-size: 10px;
  }
  .safety-report textarea {
    resize: vertical;
    line-height: 1.5;
  }
  .safety-report select:focus,
  .safety-report textarea:focus {
    border-color: var(--line-hot);
    box-shadow: 0 0 0 3px rgba(40, 223, 232, 0.07);
  }
  .safety-submit {
    display: flex;
    min-height: 39px;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 1px solid rgba(255, 92, 92, 0.34);
    border-radius: 9px;
    color: var(--red-400);
    background: rgba(255, 92, 92, 0.08);
    cursor: pointer;
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.06em;
  }
  .safety-submit:disabled {
    cursor: not-allowed;
    filter: saturate(0.25);
    opacity: 0.42;
  }
  .safety-notice {
    margin: 0;
    color: var(--red-400);
    font-size: 9px;
  }
  .safety-notice.success {
    color: var(--green-400);
  }
  @media (max-width: 640px) {
    .chat-shell {
      right: 12px;
      bottom: max(12px, env(safe-area-inset-bottom));
      left: 12px;
    }
    .chat-shell:not(.chat-shell--open) {
      left: auto;
    }
    .chat-drawer {
      width: 100%;
      height: min(66vh, 560px);
      border-radius: 16px;
    }
    .chat-toggle > span {
      display: none;
    }
    .chat-toggle {
      width: 44px;
      justify-content: center;
      padding: 0;
    }
    .safety-controls {
      grid-template-columns: 1fr;
    }
  }
</style>
