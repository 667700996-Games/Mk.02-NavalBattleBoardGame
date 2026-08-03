import { browser } from '$app/environment';
import { get } from 'svelte/store';
import {
  gameError,
  gameSnapshot,
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
    if (event.type !== 'attack:fire') this.queue.push(event);
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
    } else if (event.type === 'error' || event.type === 'placement:rejected') {
      gameError.set(event.payload);
      if (event.payload.code === 'VERSION_CONFLICT' || event.payload.code === 'TURN_CONFLICT') {
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

