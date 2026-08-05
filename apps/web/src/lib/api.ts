import type {
  ApiErrorBody,
  GameSnapshot,
  HistoryItem,
  RoomCreatedResponse,
  RoomSummary,
  RoomVisibility,
  Session
} from '$lib/types';
import {
  isCompatibleGameSnapshot,
  isCompatibleProtocolEnvelope,
  SERVER_PROTOCOL_MISMATCH_CODE,
  SERVER_PROTOCOL_MISMATCH_MESSAGE
} from '$lib/protocol';

export class ApiError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly status: number,
    public readonly requestId?: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      Accept: 'application/json',
      ...(init.body ? { 'Content-Type': 'application/json' } : {}),
      ...init.headers
    }
  });
  if (!response.ok) {
    let body: ApiErrorBody = {
      code: 'NETWORK_ERROR',
      message: '통신 상태를 확인한 뒤 다시 시도해 주세요.'
    };
    try {
      body = (await response.json()) as ApiErrorBody;
    } catch {
      // Keep the safe user-facing fallback.
    }
    throw new ApiError(body.code, body.message, response.status, body.requestId);
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

function compatibleSnapshot(value: unknown): GameSnapshot {
  if (!isCompatibleGameSnapshot(value)) {
    throw new ApiError(SERVER_PROTOCOL_MISMATCH_CODE, SERVER_PROTOCOL_MISMATCH_MESSAGE, 426);
  }
  return value;
}

async function roomList(): Promise<{ rooms: RoomSummary[]; serverTime: string }> {
  const response = await request<{
    rooms: RoomSummary[];
    serverTime: string;
    protocolVersion?: number;
  }>('/rooms');
  if (!isCompatibleProtocolEnvelope(response)) {
    throw new ApiError(SERVER_PROTOCOL_MISMATCH_CODE, SERVER_PROTOCOL_MISMATCH_MESSAGE, 426);
  }
  return response;
}

export const api = {
  createSession: (nickname: string) =>
    request<Session>('/sessions', { method: 'POST', body: JSON.stringify({ nickname }) }),
  currentSession: () => request<Session>('/sessions/current'),
  listRooms: roomList,
  createRoom: async (name: string, visibility: RoomVisibility) => {
    const response = await request<RoomCreatedResponse>('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, visibility })
    });
    return { ...response, snapshot: compatibleSnapshot(response.snapshot) };
  },
  joinRoom: async (code: string) => {
    const snapshot = await request<unknown>('/rooms/join', {
      method: 'POST',
      body: JSON.stringify({ code: code.trim().toUpperCase() })
    });
    return compatibleSnapshot(snapshot);
  },
  room: async (roomId: string) => compatibleSnapshot(await request<unknown>(`/rooms/${roomId}`)),
  leaveRoom: (roomId: string) => request<void>(`/rooms/${roomId}/leave`, { method: 'POST' }),
  recover: async () => {
    const snapshot = await request<unknown | null>('/games/recover');
    return snapshot === null ? null : compatibleSnapshot(snapshot);
  },
  history: () => request<{ games: HistoryItem[] }>('/games/history'),
  enqueueMatchmaking: async () => {
    const response = await request<{
      queued: boolean;
      queuedAt: string | null;
      snapshot: unknown | null;
    }>('/matchmaking', { method: 'POST' });
    return {
      ...response,
      snapshot: response.snapshot === null ? null : compatibleSnapshot(response.snapshot)
    };
  },
  cancelMatchmaking: () => request<void>('/matchmaking', { method: 'DELETE' })
};
