import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import type { MatchmakingTicket, RoomSummary } from '$lib/types';
import LobbyCommandDashboard from './LobbyCommandDashboard.svelte';
import LobbyRoomOperations from './LobbyRoomOperations.svelte';

const ticket: MatchmakingTicket = {
  pool: 'RANKED',
  region: 'KOREA',
  reportedLatencyMs: 24,
  rating: 1_542,
  partySize: 1,
  searchWindow: {
    phase: 'REGIONAL',
    ratingDelta: 150,
    maxLatencyMs: 90,
    elapsedSeconds: 17
  }
};

const room: RoomSummary = {
  id: '00000000-0000-4000-8000-000000000001',
  code: 'FLEET1',
  name: '북해 호송 작전',
  status: 'WAITING_FOR_OPPONENT',
  rules: { mode: 'SALVO', turnDurationSeconds: 60 },
  hostPlayerId: '00000000-0000-4000-8000-000000000002',
  gameId: null,
  version: 1,
  playerCount: 1,
  capacity: 2,
  createdAt: new Date(Date.now() - 120_000).toISOString()
};

describe('LobbyCommandDashboard', () => {
  it('renders an actionable casual idle state and every practice difficulty', () => {
    const { body } = render(LobbyCommandDashboard, {
      props: {
        matching: false,
        elapsed: 0,
        matchPool: 'CASUAL',
        rankedRegion: 'AUTO',
        measuredLatency: null,
        matchmakingTicket: null,
        practicing: false,
        socketStatus: 'online',
        toggleMatchmaking: vi.fn(),
        measureLatency: vi.fn(),
        startPractice: vi.fn()
      }
    });

    expect(body).toContain('빠른 교전');
    expect(body).toContain('상대 찾기');
    expect(body).toContain('실시간 동기화 중');
    expect(body).toContain('RECRUIT');
    expect(body).toContain('OFFICER');
    expect(body).toContain('ADMIRAL');
    expect(body).toContain('aria-label="매칭 유형"');
  });

  it('renders ranked search telemetry and locks mutable matchmaking controls', () => {
    const { body } = render(LobbyCommandDashboard, {
      props: {
        matching: true,
        elapsed: 17,
        matchPool: 'RANKED',
        rankedRegion: 'KOREA',
        measuredLatency: 24,
        matchmakingTicket: ticket,
        practicing: true,
        socketStatus: 'reconnecting',
        toggleMatchmaking: vi.fn(),
        measureLatency: vi.fn(),
        startPractice: vi.fn()
      }
    });

    expect(body).toContain('상대 지휘관 탐색 중');
    expect(body).toContain('17초 경과 · REGIONAL');
    expect(body).toContain('RATING 1542');
    expect(body).toContain('매칭 취소');
    expect(body).toContain('RECONNECTING');
    expect(body.match(/disabled/g)?.length).toBeGreaterThanOrEqual(6);
  });
});

const roomProps = {
  submitting: false,
  openCreate: false,
  openJoin: false,
  roomName: '',
  visibility: 'PUBLIC' as const,
  gameMode: 'CLASSIC' as const,
  turnDurationSeconds: 60,
  roomCode: '',
  loadRooms: vi.fn(),
  createRoom: vi.fn(),
  joinRoom: vi.fn()
};

describe('LobbyRoomOperations', () => {
  it('distinguishes loading, empty, and populated room states', () => {
    const loading = render(LobbyRoomOperations, {
      props: { ...roomProps, rooms: [], loading: true }
    }).body;
    expect(loading.match(/ui-skeleton/g)?.length).toBeGreaterThanOrEqual(3);

    const empty = render(LobbyRoomOperations, {
      props: { ...roomProps, rooms: [], loading: false }
    }).body;
    expect(empty).toContain('NO ACTIVE OPERATIONS DETECTED');
    expect(empty).toContain('첫 채널 편성');

    const populated = render(LobbyRoomOperations, {
      props: { ...roomProps, rooms: [room], loading: false }
    }).body;
    expect(populated).toContain('북해 호송 작전');
    expect(populated).toContain('FLEET1');
    expect(populated).toContain('SALVO');
    expect(populated).toContain('1 / 2 지휘관');
    expect(populated).toContain('채널 참가');
  });

  it('renders create and secure-join dialogs with their validation contracts', () => {
    const createDialog = render(LobbyRoomOperations, {
      props: { ...roomProps, rooms: [], loading: false, openCreate: true }
    }).body;
    expect(createDialog).toContain('role="dialog"');
    expect(createDialog).toContain('새 작전실 편성');
    expect(createDialog).toContain('minlength="2"');
    expect(createDialog).toContain('value="SALVO"');
    expect(createDialog).toContain('id="turn-duration"');

    const joinDialog = render(LobbyRoomOperations, {
      props: { ...roomProps, rooms: [], loading: false, openJoin: true, roomCode: 'ABC' }
    }).body;
    expect(joinDialog).toContain('보안 코드로 참가');
    expect(joinDialog).toContain('minlength="6"');
    expect(joinDialog).toContain('maxlength="6"');
    expect(joinDialog).toMatch(/채널 접속[\s\S]*disabled|disabled[\s\S]*채널 접속/);
  });
});
