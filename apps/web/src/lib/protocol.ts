import type { GameSnapshot, RoomStatus } from '$lib/types';

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
