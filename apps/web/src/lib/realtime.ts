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
import type { ClientEvent, GameSnapshot, ServerEvent } from '$lib/types';
import { sounds } from '$lib/sound';
import {
  acceptWebsocketProtocol,
  isCompatibleGameSnapshot,
  isCompatibleServerEvent,
  SERVER_PROTOCOL_MISMATCH_CODE,
  SERVER_PROTOCOL_MISMATCH_MESSAGE,
  websocketProtocol
} from '$lib/protocol';

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
    const socket = new WebSocket(
      [protocol, '//', location.host, '/ws'].join(''),
      websocketProtocol()
    );
    this.socket = socket;
    socket.addEventListener('open', () => {
      if (this.socket === socket) this.onOpen();
    });
    socket.addEventListener('message', (event) => {
      if (this.socket === socket) this.onMessage(String(event.data));
    });
    socket.addEventListener('close', () => this.onClose(socket));
    socket.addEventListener('error', () => {
      if (this.socket === socket) socket.close();
    });
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
      event.type !== 'game:start' &&
      event.type !== 'player:ready' &&
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

  private applySnapshot(next: GameSnapshot): void {
    const current = get(gameSnapshot);
    if (current && current.room.id === next.room.id && next.version < current.version) return;
    gameSnapshot.set(next);
  }

  private onOpen(): void {
    if (this.socket && !acceptWebsocketProtocol(this.socket.protocol)) {
      gameError.set({
        code: SERVER_PROTOCOL_MISMATCH_CODE,
        message: SERVER_PROTOCOL_MISMATCH_MESSAGE,
        retryable: false
      });
      this.disconnect();
      return;
    }
    this.retries = 0;
    this.setStatus('online');
    for (const event of this.queue.splice(0)) this.send(event);
    this.heartbeatTimer = setInterval(() => {
      this.send({ type: 'heartbeat', payload: { clientTime: new Date().toISOString() } });
    }, 20_000);
    const roomId = get(gameSnapshot)?.room.id;
    if (roomId) this.sync(roomId);
  }

  private onClose(closedSocket: WebSocket): void {
    if (this.socket !== closedSocket) return;
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
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw) as unknown;
    } catch {
      return;
    }
    if (!isCompatibleServerEvent(parsed)) return;
    const event: ServerEvent = parsed;
    if (
      event.type === 'room:updated' ||
      event.type === 'player:joined' ||
      event.type === 'player:left' ||
      event.type === 'game:placement-started' ||
      event.type === 'placement:accepted' ||
      event.type === 'game:started' ||
      event.type === 'turn:changed' ||
      event.type === 'game:finished' ||
      event.type === 'player:disconnected' ||
      event.type === 'player:reconnected' ||
      event.type === 'game:snapshot'
    ) {
      if (!isCompatibleGameSnapshot(event.payload)) {
        gameError.set({
          code: SERVER_PROTOCOL_MISMATCH_CODE,
          message: SERVER_PROTOCOL_MISMATCH_MESSAGE,
          retryable: false
        });
        this.disconnect();
        return;
      }
      if (event.type === 'player:joined' || event.type === 'player:reconnected') {
        sounds.connected();
      }
      this.applySnapshot(event.payload);
      gameError.set(null);
    } else if (event.type === 'room:created') {
      if (!isCompatibleGameSnapshot(event.payload.snapshot)) {
        gameError.set({
          code: SERVER_PROTOCOL_MISMATCH_CODE,
          message: SERVER_PROTOCOL_MISMATCH_MESSAGE,
          retryable: false
        });
        this.disconnect();
        return;
      }
      this.applySnapshot(event.payload.snapshot);
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
        const current = get(gameSnapshot);
        if (
          event.payload.playerId &&
          event.payload.playerId !== current?.selfPlayerId &&
          event.payload.type !== 'SYSTEM'
        ) {
          sounds.chat();
        }
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
    } else if (event.type === 'player:ready:accepted' || event.type === 'player:unready:accepted') {
      const snapshot = get(gameSnapshot);
      if (snapshot?.room.id === event.payload.roomId) {
        const ready = event.payload.readyState === 'READY';
        const notification = {
          id: `ready-${event.payload.requestId}`,
          title: ready ? '작전 준비 완료' : '준비 상태 해제',
          message: ready
            ? '준비 상태가 서버에 반영되었습니다.'
            : '준비를 취소했습니다. 방장의 작전 시작 승인이 잠금 해제되었습니다.',
          tone: ready ? ('success' as const) : ('warning' as const)
        };
        hudNotifications.update((notifications) => [...notifications, notification].slice(-3));
        setTimeout(() => dismissHudNotification(notification.id), 1_800);
      }
    } else if (event.type === 'game:start:accepted') {
      const snapshot = get(gameSnapshot);
      if (snapshot?.room.id === event.payload.roomId) {
        const notification = {
          id: `start-${event.payload.requestId}`,
          title: '작전 개시 승인',
          message: '서버 검증이 완료되었습니다. 함선 배치 단계로 이동합니다.',
          tone: 'success' as const
        };
        hudNotifications.update((notifications) => [...notifications, notification].slice(-3));
        setTimeout(() => dismissHudNotification(notification.id), 1_800);
      }
    } else if (event.type === 'turn:started' || event.type === 'game:timer-sync') {
      gameSnapshot.update((snapshot) => {
        if (!snapshot || snapshot.room.id !== event.payload.roomId) return snapshot;
        if (event.payload.turnNumber < (snapshot.turnNumber ?? 0)) return snapshot;
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
      if (snapshot?.room.id === event.payload.roomId) {
        this.sync(event.payload.roomId);
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
      event.type === 'player:ready:rejected' ||
      event.type === 'player:unready:rejected' ||
      event.type === 'game:start:rejected' ||
      event.type === 'chat:rejected'
    ) {
      gameError.set(event.payload);
      if (
        event.payload.code === 'VERSION_CONFLICT' ||
        event.payload.code === 'TURN_CONFLICT' ||
        event.payload.code === 'TURN_EXPIRED' ||
        event.payload.code === 'STALE_ROOM_VERSION'
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
