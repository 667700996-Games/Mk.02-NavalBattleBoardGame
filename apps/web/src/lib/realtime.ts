import { browser } from '$app/environment';
import { get } from 'svelte/store';
import {
  chatMessages,
  chatHistoryLoaded,
  chatTyping,
  dismissHudNotification,
  gameError,
  gameSnapshot,
  hudNotifications,
  lastAttack,
  socketStatus,
  type SocketStatus
} from '$lib/stores';
import type { ClientEvent, ServerEvent } from '$lib/types';

class RealtimeClient {
  private socket: WebSocket | null = null;
  private retries = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private manuallyClosed = false;
  private queue: ClientEvent[] = [];
  private typingTimer: ReturnType<typeof setTimeout> | null = null;

  connect(): void {
    if (!browser || this.socket?.readyState === WebSocket.OPEN) return;
    if (this.socket?.readyState === WebSocket.CONNECTING) return;
    this.manuallyClosed = false;
    this.setStatus(this.retries ? 'reconnecting' : 'connecting');
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    this.socket = new WebSocket(`${protocol}//${location.host}/ws`);
    this.socket.addEventListener('open', () => this.onOpen());
    this.socket.addEventListener('message', (event) => this.onMessage(String(event.data)));
    this.socket.addEventListener('close', () => this.onClose());
    this.socket.addEventListener('error', () => this.socket?.close());
  }

  disconnect(): void {
    this.manuallyClosed = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    if (this.typingTimer) clearTimeout(this.typingTimer);
    this.socket?.close(1000, 'client navigation');
    this.socket = null;
    this.queue = [];
    this.setStatus('idle');
  }

  send(event: ClientEvent): boolean {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(event));
      return true;
    }
    if (
      event.type !== 'attack:fire' &&
      event.type !== 'game:surrender' &&
      event.type !== 'player:unready' &&
      event.type !== 'chat:send' &&
      event.type !== 'chat:typing'
    ) {
      this.queue.push(event);
    }
    this.connect();
    return false;
  }

  sync(roomId: string): void {
    this.send({ type: 'game:sync', payload: { roomId } });
  }

  private onOpen(): void {
    this.retries = 0;
    this.setStatus('online');
    for (const event of this.queue.splice(0)) this.send(event);
    this.heartbeatTimer = setInterval(() => {
      this.send({ type: 'heartbeat', payload: { clientTime: new Date().toISOString() } });
    }, 20_000);
    const roomId = get(gameSnapshot)?.room.id;
    if (roomId) this.sync(roomId);
  }

  private onClose(): void {
    this.socket = null;
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    if (this.typingTimer) clearTimeout(this.typingTimer);
    this.typingTimer = null;
    chatTyping.set(null);
    if (this.manuallyClosed) return;
    this.setStatus('offline');
    const delay = Math.min(10_000, 600 * 2 ** this.retries) + Math.round(Math.random() * 250);
    this.retries += 1;
    this.reconnectTimer = setTimeout(() => this.connect(), delay);
  }

  private onMessage(raw: string): void {
    let event: ServerEvent;
    try {
      event = JSON.parse(raw) as ServerEvent;
    } catch {
      return;
    }
    if (
      event.type === 'room:updated' ||
      event.type === 'player:joined' ||
      event.type === 'player:left' ||
      event.type === 'placement:accepted' ||
      event.type === 'game:started' ||
      event.type === 'turn:changed' ||
      event.type === 'game:finished' ||
      event.type === 'player:disconnected' ||
      event.type === 'player:reconnected' ||
      event.type === 'game:snapshot'
    ) {
      gameSnapshot.set(event.payload);
      gameError.set(null);
    } else if (event.type === 'room:created') {
      gameSnapshot.set(event.payload.snapshot);
    } else if (event.type === 'attack:result' || event.type === 'ship:sunk') {
      lastAttack.set(event.payload);
    } else if (event.type === 'chat:history') {
      const roomId = get(gameSnapshot)?.room.id;
      if (roomId === event.payload.roomId) {
        chatMessages.set(
          event.payload.messages.filter((message) => message.roomId === roomId).slice(-100)
        );
        chatHistoryLoaded.set(true);
      }
    } else if (event.type === 'chat:message') {
      const roomId = get(gameSnapshot)?.room.id;
      if (roomId === event.payload.roomId) {
        chatMessages.update((messages) => {
          if (messages.some((message) => message.messageId === event.payload.messageId)) {
            return messages;
          }
          return [...messages, event.payload].slice(-100);
        });
      }
    } else if (event.type === 'chat:typing') {
      const roomId = get(gameSnapshot)?.room.id;
      if (roomId === event.payload.roomId && event.payload.isTyping) {
        chatTyping.set(event.payload);
        if (this.typingTimer) clearTimeout(this.typingTimer);
        this.typingTimer = setTimeout(() => chatTyping.set(null), 2_500);
      } else {
        chatTyping.set(null);
      }
    } else if (event.type === 'game:surrendered') {
      const snapshot = get(gameSnapshot);
      if (snapshot?.room.id === event.payload.roomId) {
        const surrenderedSelf = snapshot.selfPlayerId === event.payload.surrenderedPlayerId;
        const notification = {
          id: `surrender-${event.payload.timestamp}-${snapshot.selfPlayerId}`,
          title: surrenderedSelf ? '작전 포기 승인' : '적군 항복',
          message: surrenderedSelf
            ? '기권이 승인되어 즉시 패배 처리되었습니다.'
            : '상대가 작전을 포기했습니다.',
          tone: surrenderedSelf ? ('danger' as const) : ('success' as const)
        };
        hudNotifications.update((notifications) => [...notifications, notification].slice(-3));
        setTimeout(() => dismissHudNotification(notification.id), 6_000);
      }
    } else if (event.type === 'player:unready:accepted') {
      const snapshot = get(gameSnapshot);
      if (snapshot?.room.id === event.payload.roomId) {
        const notification = {
          id: `unready-${event.payload.requestId}`,
          title: '함대 배치 잠금 해제',
          message: '준비 상태를 해제했습니다. 함선 배치를 다시 수정할 수 있습니다.',
          tone: 'warning' as const
        };
        hudNotifications.update((notifications) => [...notifications, notification].slice(-3));
        setTimeout(() => dismissHudNotification(notification.id), 5_000);
      }
    } else if (event.type === 'turn:started' || event.type === 'game:timer-sync') {
      gameSnapshot.update((snapshot) => {
        if (!snapshot || snapshot.room.id !== event.payload.gameId) return snapshot;
        return {
          ...snapshot,
          turnNumber: event.payload.turnNumber,
          currentPlayerId: event.payload.activePlayerId,
          gameStartedAt: event.payload.gameStartedAt,
          turnStartedAt: event.payload.turnStartedAt,
          turnDeadlineAt: event.payload.turnDeadlineAt,
          turnDurationSeconds: event.payload.turnDurationSeconds,
          serverTimestamp: event.payload.serverTimestamp
        };
      });
    } else if (event.type === 'turn:expired') {
      const snapshot = get(gameSnapshot);
      if (snapshot?.room.id === event.payload.gameId) {
        const expiredSelf = snapshot.selfPlayerId === event.payload.expiredPlayerId;
        const automaticDefeat = event.payload.winnerId !== null;
        const notification = {
          id: `timeout-${event.payload.gameId}-${event.payload.expiredTurnNumber}`,
          title: automaticDefeat
            ? expiredSelf
              ? '시간 초과 자동 패배'
              : '적 작전 지연 종료'
            : expiredSelf
              ? '작전 시간 만료'
              : '상대 시간 만료',
          message: automaticDefeat
            ? expiredSelf
              ? '3회 연속 시간 초과로 자동 기권 처리되었습니다.'
              : '상대 지휘관이 3회 연속 시간 초과로 패배했습니다.'
            : expiredSelf
              ? '공격 기회가 소멸되어 상대 턴으로 전환됩니다.'
              : '상대 지휘관의 공격 기회가 소멸했습니다.',
          tone: automaticDefeat || expiredSelf ? ('danger' as const) : ('warning' as const)
        };
        hudNotifications.update((notifications) => [...notifications, notification].slice(-3));
        setTimeout(() => dismissHudNotification(notification.id), 6_000);
      }
    } else if (
      event.type === 'error' ||
      event.type === 'placement:rejected' ||
      event.type === 'player:unready:rejected' ||
      event.type === 'chat:rejected'
    ) {
      gameError.set(event.payload);
      if (
        event.payload.code === 'VERSION_CONFLICT' ||
        event.payload.code === 'TURN_CONFLICT' ||
        event.payload.code === 'TURN_EXPIRED'
      ) {
        const roomId = get(gameSnapshot)?.room.id;
        if (roomId) this.sync(roomId);
      }
    }
  }

  private setStatus(status: SocketStatus): void {
    socketStatus.set(status);
  }
}

export const realtime = new RealtimeClient();
