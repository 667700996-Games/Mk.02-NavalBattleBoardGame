import { describe, expect, it } from 'vitest';
import {
  acceptProtocolCompatibility,
  acceptProtocolHeaders,
  acceptWebsocketProtocol,
  GAME_PROTOCOL_VERSION,
  isCompatibleGameSnapshot,
  isCompatibleProtocolEnvelope,
  isCompatibleServerEvent,
  PROTOCOL_CAPABILITIES,
  PROTOCOL_CAPABILITIES_HEADER,
  PROTOCOL_MAX_VERSION_HEADER,
  PROTOCOL_MIN_VERSION_HEADER,
  PROTOCOL_VERSION_HEADER,
  supportsProtocolCapability,
  websocketProtocol
} from './protocol';

const currentSnapshot = {
  protocolVersion: GAME_PROTOCOL_VERSION,
  balance: {
    rulesetVersion: 1,
    checksum: 'a'.repeat(64),
    manifest: {
      schemaVersion: 1,
      rulesetVersion: 1,
      boardSize: 10,
      fleet: [{ kind: 'CARRIER', cells: 5 }],
      consecutiveTimeoutForfeit: 3
    }
  },
  roomId: 'room-id',
  roomState: 'WAITING_FOR_OPPONENT',
  hostPlayerId: 'host-id',
  gameId: null,
  canStartGame: false,
  roomVersion: 1,
  players: [],
  room: { status: 'WAITING_FOR_OPPONENT' }
};

describe('game protocol compatibility', () => {
  it('accepts the current explicit-start snapshot contract', () => {
    expect(isCompatibleGameSnapshot(currentSnapshot)).toBe(true);
    expect(isCompatibleProtocolEnvelope({ protocolVersion: GAME_PROTOCOL_VERSION })).toBe(true);
  });

  it('negotiates HTTP and WebSocket V2 while retaining the headerless stable fallback', () => {
    expect(websocketProtocol()).toBe('mk01.v2');
    expect(acceptWebsocketProtocol('')).toBe(true);
    expect(acceptWebsocketProtocol('mk01.v2')).toBe(true);
    expect(acceptWebsocketProtocol('mk01.v3')).toBe(false);
    expect(acceptProtocolHeaders(new Headers())).toBe(true);

    const compatible = new Headers({
      [PROTOCOL_VERSION_HEADER]: '2',
      [PROTOCOL_MIN_VERSION_HEADER]: '2',
      [PROTOCOL_MAX_VERSION_HEADER]: '2',
      [PROTOCOL_CAPABILITIES_HEADER]: PROTOCOL_CAPABILITIES.join(',')
    });
    expect(acceptProtocolHeaders(compatible)).toBe(true);
    expect(supportsProtocolCapability('balance-pin-v1')).toBe(true);

    compatible.set(PROTOCOL_VERSION_HEADER, '3');
    expect(acceptProtocolHeaders(compatible)).toBe(false);
    compatible.set(PROTOCOL_VERSION_HEADER, 'invalid');
    expect(acceptProtocolHeaders(compatible)).toBe(false);
  });

  it('accepts a newer server that retains V2 and rejects a V3-only window', () => {
    expect(
      acceptProtocolCompatibility({
        currentVersion: 2,
        minimumSupportedVersion: 2,
        maximumSupportedVersion: 2,
        legacyDefaultVersion: 2,
        compatibilityWindowDays: 30,
        capabilities: [...PROTOCOL_CAPABILITIES]
      })
    ).toBe(true);
    expect(
      acceptProtocolCompatibility({
        currentVersion: 3,
        minimumSupportedVersion: 2,
        maximumSupportedVersion: 3,
        legacyDefaultVersion: 2,
        compatibilityWindowDays: 30,
        capabilities: [...PROTOCOL_CAPABILITIES, 'future-capability-v1']
      })
    ).toBe(true);
    expect(
      acceptProtocolCompatibility({
        currentVersion: 3,
        minimumSupportedVersion: 3,
        maximumSupportedVersion: 3,
        legacyDefaultVersion: 3,
        compatibilityWindowDays: 30,
        capabilities: []
      })
    ).toBe(false);
  });

  it('rejects legacy WAITING and auto-PLACEMENT snapshots', () => {
    expect(
      isCompatibleGameSnapshot({
        ...currentSnapshot,
        protocolVersion: undefined,
        roomState: undefined,
        room: { status: 'WAITING' }
      })
    ).toBe(false);
    expect(
      isCompatibleGameSnapshot({
        ...currentSnapshot,
        protocolVersion: undefined,
        roomState: undefined,
        room: { status: 'PLACEMENT' }
      })
    ).toBe(false);
    expect(isCompatibleProtocolEnvelope({ status: 'ok' })).toBe(false);
  });

  it('rejects mismatched room summary and authoritative room state', () => {
    expect(
      isCompatibleGameSnapshot({
        ...currentSnapshot,
        roomState: 'READY_TO_START',
        room: { status: 'WAITING_FOR_READY' }
      })
    ).toBe(false);
  });

  it('rejects snapshots without an intact balance interpretation pin', () => {
    expect(isCompatibleGameSnapshot({ ...currentSnapshot, balance: undefined })).toBe(false);
    expect(
      isCompatibleGameSnapshot({
        ...currentSnapshot,
        balance: { ...currentSnapshot.balance, checksum: 'tampered' }
      })
    ).toBe(false);
    expect(
      isCompatibleGameSnapshot({
        ...currentSnapshot,
        balance: {
          ...currentSnapshot.balance,
          rulesetVersion: 2
        }
      })
    ).toBe(false);
  });

  it('rejects unknown WebSocket envelopes before state mutation', () => {
    expect(isCompatibleServerEvent({ type: 'root:override', payload: {} })).toBe(false);
    expect(isCompatibleServerEvent({ type: 'heartbeat', payload: { serverTime: 'now' } })).toBe(
      true
    );
    expect(isCompatibleServerEvent({ type: 'game:snapshot', payload: currentSnapshot })).toBe(true);
  });
});
