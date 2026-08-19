import type { GameSnapshot, RoomStatus, ServerEvent, SpectatorSnapshot } from '$lib/types';
import { message } from '$lib/i18n';

export const GAME_PROTOCOL_VERSION = 3;
export const LEGACY_DEFAULT_PROTOCOL_VERSION = 3;
export const MIN_SUPPORTED_PROTOCOL_VERSION = 3;
export const MAX_SUPPORTED_PROTOCOL_VERSION = GAME_PROTOCOL_VERSION;
export const PROTOCOL_COMPATIBILITY_WINDOW_DAYS = 30;
export const PROTOCOL_VERSION_HEADER = 'x-mk01-protocol-version';
export const PROTOCOL_MIN_VERSION_HEADER = 'x-mk01-protocol-min-version';
export const PROTOCOL_MAX_VERSION_HEADER = 'x-mk01-protocol-max-version';
export const PROTOCOL_CAPABILITIES_HEADER = 'x-mk01-protocol-capabilities';
export const PROTOCOL_CAPABILITIES = [
  'account-sessions-v1',
  'authoritative-room-v2',
  'balance-pin-v1',
  'explicit-lobby-readiness-v1',
  'ranked-seasons-v1',
  'safe-replay-analysis-v1'
] as const;
export type ProtocolCapability = (typeof PROTOCOL_CAPABILITIES)[number];

export interface ProtocolCompatibility {
  currentVersion: number;
  minimumSupportedVersion: number;
  maximumSupportedVersion: number;
  legacyDefaultVersion: number;
  compatibilityWindowDays: number;
  capabilities: string[];
}

const baselineCompatibility: ProtocolCompatibility = {
  currentVersion: GAME_PROTOCOL_VERSION,
  minimumSupportedVersion: MIN_SUPPORTED_PROTOCOL_VERSION,
  maximumSupportedVersion: MAX_SUPPORTED_PROTOCOL_VERSION,
  legacyDefaultVersion: LEGACY_DEFAULT_PROTOCOL_VERSION,
  compatibilityWindowDays: PROTOCOL_COMPATIBILITY_WINDOW_DAYS,
  capabilities: [...PROTOCOL_CAPABILITIES]
};
let negotiatedCompatibility = baselineCompatibility;

export function websocketProtocol(): string {
  return `mk01.v${GAME_PROTOCOL_VERSION}`;
}

export function acceptWebsocketProtocol(selected: string): boolean {
  return selected === '' || selected === websocketProtocol();
}

export function acceptProtocolHeaders(headers: Pick<Headers, 'get'>): boolean {
  const selectedRaw = headers.get(PROTOCOL_VERSION_HEADER);
  const minimumRaw = headers.get(PROTOCOL_MIN_VERSION_HEADER);
  const maximumRaw = headers.get(PROTOCOL_MAX_VERSION_HEADER);
  if (selectedRaw === null && minimumRaw === null && maximumRaw === null) {
    return true;
  }
  const selected = Number(selectedRaw);
  const minimum = Number(minimumRaw);
  const maximum = Number(maximumRaw);
  if (
    !Number.isInteger(selected) ||
    !Number.isInteger(minimum) ||
    !Number.isInteger(maximum) ||
    selected !== GAME_PROTOCOL_VERSION ||
    minimum > GAME_PROTOCOL_VERSION ||
    maximum < GAME_PROTOCOL_VERSION
  ) {
    return false;
  }
  const capabilities = headers
    .get(PROTOCOL_CAPABILITIES_HEADER)
    ?.split(',')
    .map((value) => value.trim())
    .filter(Boolean);
  if (capabilities) {
    negotiatedCompatibility = { ...negotiatedCompatibility, capabilities };
  }
  return true;
}

export function acceptProtocolCompatibility(value: unknown): value is ProtocolCompatibility {
  if (!isRecord(value) || !Array.isArray(value.capabilities)) return false;
  const current = Number(value.currentVersion);
  const minimum = Number(value.minimumSupportedVersion);
  const maximum = Number(value.maximumSupportedVersion);
  const legacyDefault = Number(value.legacyDefaultVersion);
  const compatibilityWindowDays = Number(value.compatibilityWindowDays);
  const capabilities = value.capabilities;
  if (
    !Number.isInteger(current) ||
    !Number.isInteger(minimum) ||
    !Number.isInteger(maximum) ||
    !Number.isInteger(legacyDefault) ||
    !Number.isInteger(compatibilityWindowDays) ||
    minimum > GAME_PROTOCOL_VERSION ||
    maximum < GAME_PROTOCOL_VERSION ||
    current < minimum ||
    current > maximum ||
    legacyDefault < minimum ||
    legacyDefault > maximum ||
    compatibilityWindowDays < PROTOCOL_COMPATIBILITY_WINDOW_DAYS ||
    !capabilities.every((capability) => typeof capability === 'string') ||
    new Set(capabilities).size !== capabilities.length
  ) {
    return false;
  }
  negotiatedCompatibility = {
    currentVersion: current,
    minimumSupportedVersion: minimum,
    maximumSupportedVersion: maximum,
    legacyDefaultVersion: legacyDefault,
    compatibilityWindowDays,
    capabilities: capabilities as string[]
  };
  return true;
}

export function supportsProtocolCapability(capability: ProtocolCapability): boolean {
  return negotiatedCompatibility.capabilities.includes(capability);
}

export function legacyProtocolCompatibility(): ProtocolCompatibility {
  return { ...baselineCompatibility, capabilities: [...baselineCompatibility.capabilities] };
}

export const SERVER_PROTOCOL_MISMATCH_CODE = 'SERVER_PROTOCOL_MISMATCH';
export const serverProtocolMismatchMessage = () => message('protocol.versionMismatch');

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

function isCompatibleBalancePin(value: unknown): boolean {
  if (!isRecord(value) || !isRecord(value.manifest)) return false;
  const manifest = value.manifest;
  return (
    Number.isInteger(value.rulesetVersion) &&
    value.rulesetVersion === manifest.rulesetVersion &&
    typeof value.checksum === 'string' &&
    /^[0-9a-f]{64}$/.test(value.checksum) &&
    manifest.schemaVersion === 1 &&
    Number.isInteger(manifest.boardSize) &&
    Number(manifest.boardSize) >= 5 &&
    Number(manifest.boardSize) <= 20 &&
    Array.isArray(manifest.fleet) &&
    manifest.fleet.length > 0 &&
    Number.isInteger(manifest.consecutiveTimeoutForfeit)
  );
}

export function isCompatibleGameSnapshot(value: unknown): value is GameSnapshot {
  if (!isRecord(value) || value.protocolVersion !== GAME_PROTOCOL_VERSION) return false;
  if (!isRecord(value.room) || !ROOM_STATES.has(value.room.status as RoomStatus)) return false;
  if (!isCompatibleBalancePin(value.balance)) return false;

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

export function isCompatibleSpectatorSnapshot(value: unknown): value is SpectatorSnapshot {
  if (!isRecord(value) || value.protocolVersion !== GAME_PROTOCOL_VERSION) return false;
  if (!isRecord(value.room) || !ROOM_STATES.has(value.room.status as RoomStatus)) return false;
  if (!isCompatibleBalancePin(value.balance)) return false;
  if (
    !['DELAYED', 'LIVE', 'FINISHED'].includes(String(value.phase)) ||
    !Number.isInteger(value.delaySeconds) ||
    Number(value.delaySeconds) < 15 ||
    !Array.isArray(value.players) ||
    value.players.length !== 2 ||
    !Array.isArray(value.timeline)
  ) {
    return false;
  }
  const forbidden = ['ownBoard', 'targetBoard', 'revealedBoard', 'placement', 'pendingPlacements'];
  return forbidden.every((field) => !(field in value));
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
