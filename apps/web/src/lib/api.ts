import type {
  ApiErrorBody,
  GameSnapshot,
  HistoryItem,
  RoomCreatedResponse,
  RoomSummary,
  RoomVisibility,
  Session
} from '$lib/types';

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

export const api = {
  createSession: (nickname: string) =>
    request<Session>('/sessions', { method: 'POST', body: JSON.stringify({ nickname }) }),
  currentSession: () => request<Session>('/sessions/current'),
  listRooms: () => request<{ rooms: RoomSummary[]; serverTime: string }>('/rooms'),
  createRoom: (name: string, visibility: RoomVisibility) =>
    request<RoomCreatedResponse>('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, visibility })
    }),
  joinRoom: (code: string) =>
    request<GameSnapshot>('/rooms/join', {
      method: 'POST',
      body: JSON.stringify({ code: code.trim().toUpperCase() })
    }),
  room: (roomId: string) => request<GameSnapshot>(`/rooms/${roomId}`),
  leaveRoom: (roomId: string) => request<void>(`/rooms/${roomId}/leave`, { method: 'POST' }),
  recover: () => request<GameSnapshot | null>('/games/recover'),
  history: () => request<{ games: HistoryItem[] }>('/games/history'),
  enqueueMatchmaking: () =>
    request<{ queued: boolean; queuedAt: string | null; snapshot: GameSnapshot | null }>(
      '/matchmaking',
      { method: 'POST' }
    ),
  cancelMatchmaking: () => request<void>('/matchmaking', { method: 'DELETE' })
};
