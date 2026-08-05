import { describe, expect, it } from 'vitest';
import {
  GAME_PROTOCOL_VERSION,
  isCompatibleGameSnapshot,
  isCompatibleProtocolEnvelope
} from './protocol';

const currentSnapshot = {
  protocolVersion: GAME_PROTOCOL_VERSION,
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
});
