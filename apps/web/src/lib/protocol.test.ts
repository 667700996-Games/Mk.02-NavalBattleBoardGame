import { describe, expect, it } from 'vitest';
import {
  acceptProtocolCompatibility,
  acceptProtocolHeaders,
  acceptWebsocketProtocol,
  GAME_PROTOCOL_VERSION,
  isCompatibleGameSnapshot,
  isCompatibleProtocolEnvelope,
  isCompatibleServerEvent,
  isCompatibleSpectatorSnapshot,
  legacyProtocolCompatibility,
  PROTOCOL_CAPABILITIES,
  PROTOCOL_CAPABILITIES_HEADER,
  PROTOCOL_MAX_VERSION_HEADER,
  PROTOCOL_MIN_VERSION_HEADER,
  PROTOCOL_VERSION_HEADER,
  serverProtocolMismatchMessage,
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

const currentSpectatorSnapshot = {
  protocolVersion: GAME_PROTOCOL_VERSION,
  balance: currentSnapshot.balance,
  room: { status: 'PLAYING' },
  phase: 'DELAYED',
  delaySeconds: 30,
  players: [{ displayName: 'Alpha' }, { displayName: 'Bravo' }],
  timeline: []
};

describe('game protocol compatibility', () => {
  it('accepts the current explicit-start snapshot contract', () => {
    expect(isCompatibleGameSnapshot(currentSnapshot)).toBe(true);
    expect(isCompatibleProtocolEnvelope({ protocolVersion: GAME_PROTOCOL_VERSION })).toBe(true);
  });

  it('negotiates HTTP and WebSocket V4 while retaining the headerless stable fallback', () => {
    expect(websocketProtocol()).toBe('mk01.v4');
    expect(acceptWebsocketProtocol('')).toBe(true);
    expect(acceptWebsocketProtocol('mk01.v4')).toBe(true);
    expect(acceptWebsocketProtocol('mk01.v3')).toBe(false);
    expect(acceptProtocolHeaders(new Headers())).toBe(true);

    const compatible = new Headers({
      [PROTOCOL_VERSION_HEADER]: '4',
      [PROTOCOL_MIN_VERSION_HEADER]: '3',
      [PROTOCOL_MAX_VERSION_HEADER]: '4',
      [PROTOCOL_CAPABILITIES_HEADER]: PROTOCOL_CAPABILITIES.join(',')
    });
    expect(acceptProtocolHeaders(compatible)).toBe(true);
    expect(supportsProtocolCapability('balance-pin-v1')).toBe(true);

    compatible.set(PROTOCOL_VERSION_HEADER, '5');
    expect(acceptProtocolHeaders(compatible)).toBe(false);
    compatible.set(PROTOCOL_VERSION_HEADER, 'invalid');
    expect(acceptProtocolHeaders(compatible)).toBe(false);
  });

  it('accepts a V4 server window and rejects a window that excludes V4', () => {
    expect(
      acceptProtocolCompatibility({
        currentVersion: 4,
        minimumSupportedVersion: 3,
        maximumSupportedVersion: 4,
        legacyDefaultVersion: 3,
        compatibilityWindowDays: 30,
        capabilities: [...PROTOCOL_CAPABILITIES]
      })
    ).toBe(true);
    expect(
      acceptProtocolCompatibility({
        currentVersion: 5,
        minimumSupportedVersion: 4,
        maximumSupportedVersion: 5,
        legacyDefaultVersion: 4,
        compatibilityWindowDays: 30,
        capabilities: [...PROTOCOL_CAPABILITIES, 'future-capability-v1']
      })
    ).toBe(true);
    expect(
      acceptProtocolCompatibility({
        currentVersion: 5,
        minimumSupportedVersion: 5,
        maximumSupportedVersion: 5,
        legacyDefaultVersion: 5,
        compatibilityWindowDays: 30,
        capabilities: []
      })
    ).toBe(false);
  });

  it('rejects every malformed compatibility window and returns an isolated legacy fallback', () => {
    const valid = {
      currentVersion: 4,
      minimumSupportedVersion: 3,
      maximumSupportedVersion: 4,
      legacyDefaultVersion: 3,
      compatibilityWindowDays: 30,
      capabilities: [...PROTOCOL_CAPABILITIES]
    };
    const invalid = [
      null,
      { ...valid, capabilities: 'not-an-array' },
      { ...valid, currentVersion: 1.5 },
      { ...valid, minimumSupportedVersion: 1.5 },
      { ...valid, maximumSupportedVersion: 4.5 },
      { ...valid, legacyDefaultVersion: 1.5 },
      { ...valid, compatibilityWindowDays: 30.5 },
      { ...valid, minimumSupportedVersion: 5, currentVersion: 5, maximumSupportedVersion: 5 },
      { ...valid, maximumSupportedVersion: 3 },
      { ...valid, currentVersion: 2 },
      { ...valid, currentVersion: 5 },
      { ...valid, legacyDefaultVersion: 2 },
      { ...valid, legacyDefaultVersion: 4 },
      { ...valid, compatibilityWindowDays: 29 },
      { ...valid, capabilities: [3] },
      { ...valid, capabilities: ['balance-pin-v1', 'balance-pin-v1'] }
    ];
    for (const candidate of invalid) expect(acceptProtocolCompatibility(candidate)).toBe(false);

    const first = legacyProtocolCompatibility();
    first.capabilities.length = 0;
    expect(legacyProtocolCompatibility().capabilities).toEqual(PROTOCOL_CAPABILITIES);
    expect(serverProtocolMismatchMessage()).toBeTruthy();
  });

  it('rejects malformed or incompatible explicit protocol headers independently', () => {
    const headers = (selected: string, minimum: string, maximum: string) =>
      new Headers({
        [PROTOCOL_VERSION_HEADER]: selected,
        [PROTOCOL_MIN_VERSION_HEADER]: minimum,
        [PROTOCOL_MAX_VERSION_HEADER]: maximum
      });
    expect(acceptProtocolHeaders(headers('4', '3', '4'))).toBe(true);
    expect(acceptProtocolHeaders(headers('invalid', '3', '3'))).toBe(false);
    expect(acceptProtocolHeaders(headers('3', 'invalid', '3'))).toBe(false);
    expect(acceptProtocolHeaders(headers('3', '3', 'invalid'))).toBe(false);
    expect(acceptProtocolHeaders(headers('3', '3', '4'))).toBe(false);
    expect(acceptProtocolHeaders(headers('4', '5', '5'))).toBe(false);
    expect(acceptProtocolHeaders(headers('4', '2', '3'))).toBe(false);
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

    const invalidBalances = [
      null,
      { ...currentSnapshot.balance, manifest: null },
      { ...currentSnapshot.balance, rulesetVersion: 1.5 },
      { ...currentSnapshot.balance, checksum: 3 },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, schemaVersion: 2 }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, boardSize: 9.5 }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, boardSize: 4 }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, boardSize: 21 }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, fleet: null }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, fleet: [] }
      },
      {
        ...currentSnapshot.balance,
        manifest: { ...currentSnapshot.balance.manifest, consecutiveTimeoutForfeit: 2.5 }
      }
    ];
    for (const balance of invalidBalances) {
      expect(isCompatibleGameSnapshot({ ...currentSnapshot, balance })).toBe(false);
    }
  });

  it('rejects malformed snapshot identity, state, and player fields', () => {
    const invalidSnapshots = [
      null,
      { ...currentSnapshot, protocolVersion: 3 },
      { ...currentSnapshot, room: null },
      { ...currentSnapshot, room: { status: 'UNKNOWN' } },
      { ...currentSnapshot, roomId: 3 },
      { ...currentSnapshot, roomState: 3 },
      { ...currentSnapshot, hostPlayerId: 3 },
      { ...currentSnapshot, gameId: 3 },
      { ...currentSnapshot, canStartGame: 'false' },
      { ...currentSnapshot, roomVersion: '1' },
      { ...currentSnapshot, players: null }
    ];
    for (const snapshot of invalidSnapshots) {
      expect(isCompatibleGameSnapshot(snapshot)).toBe(false);
    }
  });

  it('accepts only delayed, bounded spectator projections without hidden fleet fields', () => {
    expect(isCompatibleSpectatorSnapshot(currentSpectatorSnapshot)).toBe(true);
    const invalidSnapshots = [
      null,
      { ...currentSpectatorSnapshot, protocolVersion: 3 },
      { ...currentSpectatorSnapshot, room: null },
      { ...currentSpectatorSnapshot, room: { status: 'UNKNOWN' } },
      { ...currentSpectatorSnapshot, balance: null },
      { ...currentSpectatorSnapshot, phase: 'UNKNOWN' },
      { ...currentSpectatorSnapshot, delaySeconds: 15.5 },
      { ...currentSpectatorSnapshot, delaySeconds: 14 },
      { ...currentSpectatorSnapshot, players: null },
      { ...currentSpectatorSnapshot, players: [{}] },
      { ...currentSpectatorSnapshot, timeline: null },
      { ...currentSpectatorSnapshot, ownBoard: [] },
      { ...currentSpectatorSnapshot, targetBoard: [] },
      { ...currentSpectatorSnapshot, revealedBoard: [] },
      { ...currentSpectatorSnapshot, placement: [] },
      { ...currentSpectatorSnapshot, pendingPlacements: [] }
    ];
    for (const snapshot of invalidSnapshots) {
      expect(isCompatibleSpectatorSnapshot(snapshot)).toBe(false);
    }
  });

  it('rejects unknown WebSocket envelopes before state mutation', () => {
    expect(isCompatibleProtocolEnvelope(null)).toBe(false);
    expect(isCompatibleServerEvent(null)).toBe(false);
    expect(isCompatibleServerEvent({ type: 'root:override', payload: {} })).toBe(false);
    expect(isCompatibleServerEvent({ type: 'heartbeat' })).toBe(false);
    expect(isCompatibleServerEvent({ type: 'heartbeat', payload: null })).toBe(false);
    expect(isCompatibleServerEvent({ type: 'heartbeat', payload: { serverTime: 'now' } })).toBe(
      true
    );
    expect(isCompatibleServerEvent({ type: 'game:snapshot', payload: currentSnapshot })).toBe(true);
    expect(isCompatibleServerEvent({ type: 'game:snapshot', payload: {} })).toBe(false);
    expect(
      isCompatibleServerEvent({
        type: 'room:created',
        payload: { inviteUrl: '/join/ABC123', snapshot: currentSnapshot }
      })
    ).toBe(true);
    expect(
      isCompatibleServerEvent({
        type: 'room:created',
        payload: { inviteUrl: 3, snapshot: currentSnapshot }
      })
    ).toBe(false);
    expect(
      isCompatibleServerEvent({
        type: 'room:created',
        payload: { inviteUrl: '/join/ABC123', snapshot: {} }
      })
    ).toBe(false);
  });
});
