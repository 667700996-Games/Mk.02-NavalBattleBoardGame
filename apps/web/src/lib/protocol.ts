import type { GameSnapshot, RoomStatus, ServerEvent } from '$lib/types';

export const GAME_PROTOCOL_VERSION = 2;
export const SERVER_PROTOCOL_MISMATCH_CODE = 'SERVER_PROTOCOL_MISMATCH';
export const SERVER_PROTOCOL_MISMATCH_MESSAGE =
  '실행 중인 게임 서버가 이전 버전입니다. 기존 개발 서버를 완전히 종료한 뒤 `npm run dev`로 다시 시작해 주세요.';

const ROOM_STATES = new Set<RoomStatus>([
  'WAITING_FOR_OPPONENT',
  'WAITING_FOR_READY',
  'READY_TO_START',
  'PLACEMENT',
  'PLAYING',
  'FINISHED',
  'CANCELLED'
]);

const SNAPSHOT_EVENTS = new Set([
  'room:updated',
  'player:joined',
  'player:left',
  'game:placement-started',
  'placement:accepted',
  'game:started',
  'turn:changed',
  'game:finished',
  'player:disconnected',
  'player:reconnected',
  'game:snapshot'
]);

const SERVER_EVENTS = new Set([
  'room:created',
  ...SNAPSHOT_EVENTS,
  'placement:rejected',
  'error',
  'player:ready:accepted',
  'player:unready:accepted',
  'game:start:accepted',
  'player:ready:rejected',
  'player:unready:rejected',
  'game:start:rejected',
  'chat:rejected',
  'attack:result',
  'ship:sunk',
  'game:surrendered',
  'chat:message',
  'chat:history',
  'chat:typing',
  'turn:started',
  'game:timer-sync',
  'turn:expired',
  'matchmaking:queued',
  'matchmaking:cancelled',
  'heartbeat'
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function isCompatibleGameSnapshot(value: unknown): value is GameSnapshot {
  if (!isRecord(value) || value.protocolVersion !== GAME_PROTOCOL_VERSION) return false;
  if (!isRecord(value.room) || !ROOM_STATES.has(value.room.status as RoomStatus)) return false;

  return (
    typeof value.roomId === 'string' &&
    typeof value.roomState === 'string' &&
    value.room.status === value.roomState &&
    typeof value.hostPlayerId === 'string' &&
    (typeof value.gameId === 'string' || value.gameId === null) &&
    typeof value.canStartGame === 'boolean' &&
    typeof value.roomVersion === 'number' &&
    Array.isArray(value.players)
  );
}

export function isCompatibleProtocolEnvelope(value: unknown): value is {
  protocolVersion: number;
} {
  return isRecord(value) && value.protocolVersion === GAME_PROTOCOL_VERSION;
}

export function isCompatibleServerEvent(value: unknown): value is ServerEvent {
  if (!isRecord(value) || typeof value.type !== 'string' || !SERVER_EVENTS.has(value.type)) {
    return false;
  }
  if (!('payload' in value) || !isRecord(value.payload)) return false;
  if (SNAPSHOT_EVENTS.has(value.type)) return isCompatibleGameSnapshot(value.payload);
  if (value.type === 'room:created') {
    return (
      typeof value.payload.inviteUrl === 'string' &&
      isCompatibleGameSnapshot(value.payload.snapshot)
    );
  }
  return true;
}
