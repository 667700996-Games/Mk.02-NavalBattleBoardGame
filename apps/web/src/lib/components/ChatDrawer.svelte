<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ChevronDown, MessageSquare, Radio, Send, X } from '@lucide/svelte';
  import { realtime } from '$lib/realtime';
  import { chatHistoryLoaded, chatMessages, chatTyping } from '$lib/stores';

  interface Props {
    roomId: string;
    selfPlayerId: string;
    online: boolean;
    readOnly?: boolean;
  }

  let { roomId, selfPlayerId, online, readOnly = false }: Props = $props();
  let open = $state(false);
  let unread = $state(0);
  let draft = $state('');
  let notice = $state('');
  let atBottom = $state(true);
  let messageList = $state<HTMLDivElement>();
  let previousLastMessageId: string | null = null;
  let historyReady = false;
  let typingSent = false;
  let typingTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const messages = $chatMessages;
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
    if (open) queueMicrotask(scrollToBottom);
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
    if (!online || !realtime.send({ type: 'chat:send', payload: { roomId, message } })) {
      notice = '실시간 연결이 복구된 뒤 다시 전송해 주세요.';
      return;
    }
    draft = '';
    notice = '';
    sendTyping(false);
  }

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
    if (typingSent) {
      realtime.send({ type: 'chat:typing', payload: { roomId, isTyping: false } });
    }
  });
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && open && (open = false)} />

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
        <button class="ui-icon-button" type="button" onclick={toggleDrawer} aria-label="채팅 닫기">
          <X size={15} />
        </button>
      </header>

      <div
        bind:this={messageList}
        class="chat-messages"
        role="log"
        aria-live="polite"
        aria-relevant="additions"
        onscroll={handleScroll}
      >
        {#if !$chatHistoryLoaded}
          <div class="chat-loading"><Radio size={18} /><span>보안 채널 동기화 중…</span></div>
        {:else if $chatMessages.length === 0}
          <div class="chat-empty">
            <MessageSquare size={20} /><span>아직 전송된 메시지가 없습니다.</span>
          </div>
        {:else}
          {#each $chatMessages as item (item.messageId)}
            <article
              class:chat-message--self={item.playerId === selfPlayerId}
              class:chat-message--system={item.kind === 'SYSTEM'}
              class="chat-message"
            >
              <div>
                <strong
                  >{item.kind === 'SYSTEM'
                    ? 'SYSTEM'
                    : item.playerId === selfPlayerId
                      ? 'YOU'
                      : item.nickname}</strong
                >
                <time datetime={item.timestamp}>{formatTime(item.timestamp)}</time>
              </div>
              <p>{item.message}</p>
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
          {#if $chatTyping?.isTyping}<span><i></i>{$chatTyping.nickname} 입력 중…</span>{:else}<span
              >ENTER 전송 · SHIFT+ENTER 줄바꿈</span
            >{/if}
          <em>{draft.length}/300</em>
        </div>
        <div class="chat-input-row">
          <textarea
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
    <button class="chat-toggle" type="button" onclick={toggleDrawer} aria-label="전술 채팅 열기">
      <MessageSquare size={18} />
      <span>CHAT</span>
      {#if unread > 0}<strong aria-label={`읽지 않은 메시지 ${unread}개`}
          >{unread > 99 ? '99+' : unread}</strong
        >{/if}
    </button>
  {/if}
</div>

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
  .typing-line em {
    font-style: normal;
  }
  .chat-input-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 38px;
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
  }
</style>
