import type {
  AiDifficulty,
  AccountSession,
  ApiErrorBody,
  GameSnapshot,
  GameReplay,
  HistoryItem,
  IntegritySignalKind,
  IntegritySignalPage,
  MatchRules,
  ModerationAction,
  ModerationActionKind,
  ModerationCasePage,
  PlayerAccount,
  PlayerProgression,
  PlayerReportReceipt,
  ReportCategory,
  ReportStatus,
  RoomCreatedResponse,
  RoomSummary,
  RoomVisibility,
  Session,
  SocialRelationship
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
  deleteCurrentSession: () => request<void>('/sessions/current', { method: 'DELETE' }),
  upgradeAccount: (handle: string) =>
    request<{ account: PlayerAccount; recoveryKey: string }>('/accounts/upgrade', {
      method: 'POST',
      body: JSON.stringify({ handle })
    }),
  loginAccount: (accountId: string, recoveryKey: string) =>
    request<Session>('/accounts/login', {
      method: 'POST',
      body: JSON.stringify({ accountId, recoveryKey })
    }),
  accountSessions: () =>
    request<{ currentSessionId: string; sessions: AccountSession[] }>('/accounts/sessions'),
  revokeAccountSession: (sessionId: string) =>
    request<void>(`/accounts/sessions/${sessionId}`, { method: 'DELETE' }),
  profile: () => request<PlayerProgression>('/profile'),
  claimMission: (missionId: string) =>
    request<PlayerProgression>(`/profile/missions/${encodeURIComponent(missionId)}/claim`, {
      method: 'POST'
    }),
  socialRelationships: () =>
    request<{ relationships: SocialRelationship[] }>('/social/relationships'),
  updateSocialRelationship: (
    roomId: string,
    targetPlayerId: string,
    muted: boolean,
    blocked: boolean
  ) =>
    request<SocialRelationship>('/social/relationships', {
      method: 'POST',
      body: JSON.stringify({ roomId, targetPlayerId, muted, blocked })
    }),
  reportPlayer: (
    roomId: string,
    targetPlayerId: string,
    category: ReportCategory,
    details: string
  ) =>
    request<{ report: PlayerReportReceipt }>('/reports', {
      method: 'POST',
      body: JSON.stringify({ roomId, targetPlayerId, category, details })
    }),
  moderationCases: (
    token: string,
    filters: { status?: ReportStatus; search?: string; before?: string; limit?: number } = {}
  ) => {
    const query = new URLSearchParams();
    if (filters.status) query.set('status', filters.status);
    if (filters.search) query.set('search', filters.search);
    if (filters.before) query.set('before', filters.before);
    if (filters.limit) query.set('limit', String(filters.limit));
    return request<ModerationCasePage>(
      `/admin/moderation/reports${query.size ? `?${query}` : ''}`,
      { headers: { Authorization: `Bearer ${token}` } }
    );
  },
  integritySignals: (
    token: string,
    filters: { kind?: IntegritySignalKind; search?: string; before?: string; limit?: number } = {}
  ) => {
    const query = new URLSearchParams();
    if (filters.kind) query.set('kind', filters.kind);
    if (filters.search) query.set('search', filters.search);
    if (filters.before) query.set('before', filters.before);
    if (filters.limit) query.set('limit', String(filters.limit));
    return request<IntegritySignalPage>(
      `/admin/integrity/signals${query.size ? `?${query}` : ''}`,
      { headers: { Authorization: `Bearer ${token}` } }
    );
  },
  moderateReport: (
    token: string,
    operatorId: string,
    reportId: string,
    action: ModerationActionKind,
    reason: string,
    durationHours?: number,
    reversesActionId?: string
  ) =>
    request<{ action: ModerationAction }>(
      `/admin/moderation/reports/${encodeURIComponent(reportId)}/actions`,
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'X-Operator-Id': operatorId },
        body: JSON.stringify({ action, reason, durationHours, reversesActionId })
      }
    ),
  listRooms: roomList,
  createRoom: async (name: string, visibility: RoomVisibility, rules?: MatchRules) => {
    const response = await request<RoomCreatedResponse>('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, visibility, rules })
    });
    return { ...response, snapshot: compatibleSnapshot(response.snapshot) };
  },
  createPractice: async (difficulty: AiDifficulty) =>
    compatibleSnapshot(
      await request<unknown>('/practice', {
        method: 'POST',
        body: JSON.stringify({ difficulty })
      })
    ),
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
  replay: (roomId: string) => request<GameReplay>(`/games/${roomId}/replay`),
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
