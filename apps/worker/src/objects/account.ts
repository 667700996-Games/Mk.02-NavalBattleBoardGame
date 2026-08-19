import { DurableObject } from "cloudflare:workers";
import {
  DomainError,
  type PlayerAccount,
  type SessionRecord,
} from "../domain/protocol";
import {
  bodyObject,
  internalRequest,
  json,
  noContent,
  requireString,
  requireUuid,
} from "../http";
import type { WorkerEnv } from "../env";

interface AccountState {
  sessions: Record<string, SessionRecord>;
  accounts: Record<string, PlayerAccount>;
  handles: Record<string, string>;
  identities: Record<
    string,
    {
      accountId: string | null;
      nickname: string;
      currentRoomId: string | null;
    }
  >;
}

const EMPTY_STATE: AccountState = {
  sessions: {},
  accounts: {},
  handles: {},
  identities: {},
};

export class AccountDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/sessions/create") {
        return await this.createSession(await bodyObject(request));
      }
      if (
        request.method === "POST" &&
        url.pathname === "/sessions/authenticate"
      ) {
        return await this.authenticate(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/sessions/delete") {
        return await this.deleteSession(await bodyObject(request));
      }
      if (
        request.method === "POST" &&
        url.pathname === "/sessions/identities"
      ) {
        return await this.sessionIdentities(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/sessions/room") {
        return await this.updateRoom(await bodyObject(request));
      }
      if (
        request.method === "POST" &&
        url.pathname === "/sessions/room-by-id"
      ) {
        return await this.updateRoomBySessionId(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/upgrade") {
        return await this.upgrade(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/login") {
        return await this.login(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/lookup") {
        return await this.lookupAccount(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/presence") {
        return await this.accountPresence(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/export") {
        return await this.exportCore(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/verify") {
        return await this.verifyAccount(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/delete") {
        return await this.deleteAccount(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/accounts/sessions") {
        return await this.accountSessions(await bodyObject(request));
      }
      if (
        request.method === "POST" &&
        url.pathname === "/accounts/sessions/revoke"
      ) {
        return await this.revokeAccountSession(await bodyObject(request));
      }
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        status(resolved),
      );
    }
  }

  private async createSession(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const session = sessionFromInput(input);
    await this.mutate((state) => {
      state.sessions[session.tokenHash] = session;
      state.identities[session.id] = {
        accountId: session.accountId,
        nickname: session.nickname,
        currentRoomId: session.currentRoomId,
      };
    });
    return json(session, 201);
  }

  private async authenticate(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const now = requireString(input.now);
    let found: SessionRecord | null = null;
    await this.mutate((state) => {
      const session = state.sessions[tokenHash];
      if (!session || Date.parse(session.expiresAt) <= Date.parse(now)) {
        delete state.sessions[tokenHash];
        return;
      }
      session.lastSeenAt = now;
      found = structuredClone(session);
    });
    if (!found) throw new DomainError("UNAUTHORIZED");
    return json(found);
  }

  private async deleteSession(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    let revoked: { sessionId: string; roomId: string | null } | null = null;
    await this.mutate((state) => {
      const session = state.sessions[tokenHash];
      if (!session) throw new DomainError("UNAUTHORIZED");
      revoked = { sessionId: session.id, roomId: session.currentRoomId };
      delete state.sessions[tokenHash];
    });
    if (!revoked) throw new DomainError("INTERNAL_ERROR");
    return json(revoked);
  }

  private async updateRoom(input: Record<string, unknown>): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const roomId = input.roomId === null ? null : requireUuid(input.roomId);
    await this.mutate((state) => {
      const session = state.sessions[tokenHash];
      if (!session) throw new DomainError("UNAUTHORIZED");
      session.currentRoomId = roomId;
      state.identities[session.id] = {
        accountId: session.accountId,
        nickname: session.nickname,
        currentRoomId: roomId,
      };
    });
    return noContent();
  }

  private async sessionIdentities(
    input: Record<string, unknown>,
  ): Promise<Response> {
    if (!Array.isArray(input.sessionIds))
      throw new DomainError("INVALID_REQUEST");
    const sessionIds = input.sessionIds.map(requireUuid);
    const state = await this.read();
    return json({
      identities: sessionIds.map((sessionId) => ({
        sessionId,
        accountId: state.identities[sessionId]?.accountId ?? null,
        nickname: state.identities[sessionId]?.nickname ?? "Deleted Commander",
        currentRoomId: state.identities[sessionId]?.currentRoomId ?? null,
      })),
    });
  }

  private async updateRoomBySessionId(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const sessionId = requireUuid(input.sessionId);
    const roomId = input.roomId === null ? null : requireUuid(input.roomId);
    await this.mutate((state) => {
      const session = Object.values(state.sessions).find(
        (candidate) => candidate.id === sessionId,
      );
      if (!session) throw new DomainError("UNAUTHORIZED");
      session.currentRoomId = roomId;
      state.identities[session.id] = {
        accountId: session.accountId,
        nickname: session.nickname,
        currentRoomId: roomId,
      };
    });
    return noContent();
  }

  private async upgrade(input: Record<string, unknown>): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const nextTokenHash = requireString(input.nextTokenHash);
    const handle = requireString(input.handle);
    const accountId = requireUuid(input.accountId);
    const recoveryKeyHash = requireString(input.recoveryKeyHash);
    const now = requireString(input.now);
    let response: { account: PlayerAccount; session: SessionRecord } | null =
      null;
    await this.mutate((state) => {
      const session = state.sessions[tokenHash];
      if (!session) throw new DomainError("UNAUTHORIZED");
      if (state.handles[handle.toLocaleLowerCase()]) {
        throw new DomainError("ACCOUNT_HANDLE_TAKEN");
      }
      const account: PlayerAccount = {
        id: accountId,
        handle,
        recoveryKeyHash,
        createdAt: now,
      };
      session.accountId = accountId;
      session.nickname = handle;
      session.tokenHash = nextTokenHash;
      delete state.sessions[tokenHash];
      state.sessions[nextTokenHash] = session;
      state.accounts[accountId] = account;
      state.handles[handle.toLocaleLowerCase()] = accountId;
      state.identities[session.id] = {
        accountId,
        nickname: handle,
        currentRoomId: session.currentRoomId,
      };
      response = {
        account: structuredClone(account),
        session: structuredClone(session),
      };
    });
    if (!response) throw new DomainError("INTERNAL_ERROR");
    return json(response);
  }

  private async lookupAccount(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const state = await this.read();
    const accountId =
      input.accountId === undefined
        ? state.handles[requireString(input.handle).trim().toLocaleLowerCase()]
        : requireUuid(input.accountId);
    const account = accountId ? state.accounts[accountId] : undefined;
    if (!account) throw new DomainError("INVALID_REQUEST");
    return json({
      id: account.id,
      handle: account.handle,
      createdAt: account.createdAt,
    });
  }

  private async accountPresence(
    input: Record<string, unknown>,
  ): Promise<Response> {
    if (!Array.isArray(input.accountIds))
      throw new DomainError("INVALID_REQUEST");
    const accountIds = input.accountIds.map(requireUuid);
    const now = requireString(input.now);
    const state = await this.read();
    return json({
      presences: accountIds.map((accountId) => {
        const sessions = Object.values(state.sessions).filter(
          (session) =>
            session.accountId === accountId &&
            Date.parse(session.expiresAt) > Date.parse(now),
        );
        const currentRoomId =
          sessions.find((session) => session.currentRoomId)?.currentRoomId ??
          null;
        return {
          accountId,
          presence: currentRoomId
            ? "IN_GAME"
            : sessions.length
              ? "ONLINE"
              : "OFFLINE",
          currentRoomId,
        };
      }),
    });
  }

  private async login(input: Record<string, unknown>): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    const recoveryKeyHash = requireString(input.recoveryKeyHash);
    const session = sessionFromInput(input);
    await this.mutate((state) => {
      const account = state.accounts[accountId];
      if (
        !account ||
        !constantTimeEqual(account.recoveryKeyHash, recoveryKeyHash)
      ) {
        throw new DomainError("UNAUTHORIZED");
      }
      session.accountId = account.id;
      session.nickname = account.handle;
      state.sessions[session.tokenHash] = session;
      state.identities[session.id] = {
        accountId: account.id,
        nickname: account.handle,
        currentRoomId: session.currentRoomId,
      };
    });
    return json(session, 201);
  }

  private async accountSessions(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const state = await this.read();
    const current = state.sessions[tokenHash];
    if (!current?.accountId) throw new DomainError("UNAUTHORIZED");
    return json({
      currentSessionId: current.id,
      sessions: Object.values(state.sessions)
        .filter((session) => session.accountId === current.accountId)
        .map((session) => ({
          id: session.id,
          nickname: session.nickname,
          createdAt: session.createdAt,
          lastSeenAt: session.lastSeenAt,
          currentRoomId: session.currentRoomId,
        })),
    });
  }

  private async exportCore(input: Record<string, unknown>): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const state = await this.read();
    const current = state.sessions[tokenHash];
    if (!current?.accountId) throw new DomainError("UNAUTHORIZED");
    const account = state.accounts[current.accountId];
    if (!account) throw new DomainError("UNAUTHORIZED");
    const progression = this.env.PROGRESSION.get(
      this.env.PROGRESSION.idFromName("global-v1"),
    );
    const progressionData = await responseJson<{
      gameHistory: unknown[];
      progressionRewards: unknown[];
      rankedRating: unknown;
      rankedStandings: unknown[];
      rankedMatchResults: unknown[];
      rankedRewards: unknown[];
      leaderboardVisible: boolean;
    }>(
      progression.fetch(
        internalRequest("/export", {
          identityId: account.id,
          accountId: account.id,
        }),
      ),
    );
    return json({
      formatVersion: 1,
      requestId: requireUuid(input.requestId),
      generatedAt: requireString(input.generatedAt),
      account: {
        id: account.id,
        handle: account.handle,
        createdAt: account.createdAt,
      },
      sessions: accountSessionViews(state, account.id),
      ...progressionData,
      socialRelationships: [],
      moderationReports: [],
      moderationActions: [],
      integritySignals: [],
      supportActions: [],
      cacheCopies: "SQLite-backed Durable Objects storage only",
      credentialsExcluded: true,
    });
  }

  private async verifyAccount(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const recoveryKeyHash = requireString(input.recoveryKeyHash);
    const state = await this.read();
    const current = state.sessions[tokenHash];
    if (!current?.accountId) throw new DomainError("UNAUTHORIZED");
    const account = state.accounts[current.accountId];
    if (
      !account ||
      !constantTimeEqual(account.recoveryKeyHash, recoveryKeyHash)
    ) {
      throw new DomainError("UNAUTHORIZED");
    }
    return json({
      accountId: account.id,
      sessions: accountSessionViews(state, account.id),
    });
  }

  private async deleteAccount(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const recoveryKeyHash = requireString(input.recoveryKeyHash);
    if (input.confirmation !== "DELETE")
      throw new DomainError("INVALID_REQUEST");
    const requestId = requireUuid(input.requestId);
    const deletedAt = requireString(input.deletedAt);
    const verifiedState = await this.read();
    const verifiedSession = verifiedState.sessions[tokenHash];
    const verifiedAccount = verifiedSession?.accountId
      ? verifiedState.accounts[verifiedSession.accountId]
      : null;
    if (
      !verifiedAccount ||
      !constantTimeEqual(verifiedAccount.recoveryKeyHash, recoveryKeyHash)
    ) {
      throw new DomainError("UNAUTHORIZED");
    }
    return this.ctx.blockConcurrencyWhile(async () => {
      const state = await this.read();
      const current = state.sessions[tokenHash];
      if (!current?.accountId) throw new DomainError("UNAUTHORIZED");
      const account = state.accounts[current.accountId];
      if (
        !account ||
        !constantTimeEqual(account.recoveryKeyHash, recoveryKeyHash)
      ) {
        throw new DomainError("UNAUTHORIZED");
      }
      const sessions = Object.values(state.sessions).filter(
        (session) => session.accountId === account.id,
      );
      const identitySessions = Object.entries(state.identities)
        .filter(([, identity]) => identity.accountId === account.id)
        .map(([id, identity]) => ({
          id,
          currentRoomId: identity.currentRoomId ?? null,
        }));
      const progression = this.env.PROGRESSION.get(
        this.env.PROGRESSION.idFromName("global-v1"),
      );
      const exported = await responseJson<{
        gameHistory: Array<{ roomId: string }>;
      }>(
        progression.fetch(
          internalRequest("/export", {
            identityId: account.id,
            accountId: account.id,
          }),
        ),
      );
      const roomIds = new Set([
        ...identitySessions.flatMap((session) =>
          session.currentRoomId ? [session.currentRoomId] : [],
        ),
        ...exported.gameHistory.map((item) => requireUuid(item.roomId)),
      ]);
      for (const session of identitySessions) {
        if (!session.currentRoomId) continue;
        const gameRoom = this.env.GAME_ROOMS.get(
          this.env.GAME_ROOMS.idFromName(session.currentRoomId),
        );
        await acceptedResponse(
          gameRoom.fetch(
            internalRequest("/disconnect-session", {
              sessionId: session.id,
              now: deletedAt,
            }),
          ),
        );
        await acceptedResponse(
          gameRoom.fetch(
            internalRequest("/leave", {
              sessionId: session.id,
              now: deletedAt,
            }),
          ),
        );
      }
      let roomsAnonymized = 0;
      for (const roomId of roomIds) {
        const gameRoom = this.env.GAME_ROOMS.get(
          this.env.GAME_ROOMS.idFromName(roomId),
        );
        const response = await gameRoom.fetch(
          internalRequest("/anonymize-account", {
            accountId: account.id,
            sessionIds: identitySessions.map((session) => session.id),
            now: deletedAt,
          }),
        );
        if (response.ok) roomsAnonymized += 1;
        else if (response.status !== 404)
          throw new DomainError("INTERNAL_ERROR");
      }
      const progressionDeletion = await responseJson<{
        rewardsDeleted: number;
      }>(
        progression.fetch(
          internalRequest("/delete", {
            identityId: account.id,
            accountId: account.id,
          }),
        ),
      );
      await this.mutate((latest) => {
        const stillCurrent = latest.sessions[tokenHash];
        const stillAccount = stillCurrent?.accountId
          ? latest.accounts[stillCurrent.accountId]
          : null;
        if (!stillCurrent?.accountId || !stillAccount)
          throw new DomainError("UNAUTHORIZED");
        for (const [hash, session] of Object.entries(latest.sessions)) {
          if (session.accountId !== account.id) continue;
          delete latest.sessions[hash];
        }
        for (const [sessionId, identity] of Object.entries(latest.identities)) {
          if (identity.accountId === account.id)
            delete latest.identities[sessionId];
        }
        delete latest.handles[account.handle.toLocaleLowerCase()];
        delete latest.accounts[account.id];
      });
      return json({
        requestId,
        deletedAt,
        stats: {
          sessionsDeleted: sessions.length,
          rewardsDeleted: progressionDeletion.rewardsDeleted,
          relationshipsDeleted: 0,
          reportsDeleted: 0,
          integritySignalsDeleted: 0,
          roomsAnonymized,
        },
      });
    });
  }

  private async revokeAccountSession(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const tokenHash = requireString(input.tokenHash);
    const sessionId = requireUuid(input.sessionId);
    let revoked: { sessionId: string; roomId: string | null } | null = null;
    await this.mutate((state) => {
      const current = state.sessions[tokenHash];
      if (!current?.accountId || current.id === sessionId)
        throw new DomainError("UNAUTHORIZED");
      const entry = Object.entries(state.sessions).find(
        ([, session]) =>
          session.id === sessionId && session.accountId === current.accountId,
      );
      if (entry) {
        revoked = {
          sessionId: entry[1].id,
          roomId: entry[1].currentRoomId,
        };
        delete state.sessions[entry[0]];
      }
    });
    if (!revoked) throw new DomainError("UNAUTHORIZED");
    return json(revoked);
  }

  private async read(): Promise<AccountState> {
    const state =
      (await this.ctx.storage.get<AccountState>("state")) ??
      structuredClone(EMPTY_STATE);
    state.identities ??= {};
    return state;
  }

  private async mutate(action: (state: AccountState) => void): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<AccountState>("state")) ??
        structuredClone(EMPTY_STATE);
      state.identities ??= {};
      action(state);
      await transaction.put("state", state);
    });
  }
}

function sessionFromInput(input: Record<string, unknown>): SessionRecord {
  return {
    id: requireUuid(input.sessionId),
    accountId:
      input.accountId === null || input.accountId === undefined
        ? null
        : requireUuid(input.accountId),
    nickname: requireString(input.nickname),
    tokenHash: requireString(input.tokenHash),
    createdAt: requireString(input.createdAt),
    lastSeenAt: requireString(input.lastSeenAt),
    currentRoomId:
      input.currentRoomId === null || input.currentRoomId === undefined
        ? null
        : requireUuid(input.currentRoomId),
    expiresAt: requireString(input.expiresAt),
  };
}

function accountSessionViews(state: AccountState, accountId: string) {
  return Object.values(state.sessions)
    .filter((session) => session.accountId === accountId)
    .map((session) => ({
      id: session.id,
      nickname: session.nickname,
      createdAt: session.createdAt,
      lastSeenAt: session.lastSeenAt,
      currentRoomId: session.currentRoomId,
    }));
}

function constantTimeEqual(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0;
}

async function responseJson<T>(responsePromise: Promise<Response>): Promise<T> {
  const response = await responsePromise;
  if (!response.ok) throw new DomainError("INTERNAL_ERROR");
  return (await response.json()) as T;
}

async function acceptedResponse(
  responsePromise: Promise<Response>,
): Promise<void> {
  const response = await responsePromise;
  if (!response.ok && response.status !== 404)
    throw new DomainError("INTERNAL_ERROR");
}

function status(error: DomainError): number {
  if (error.code === "UNAUTHORIZED") return 401;
  if (error.code === "ACCOUNT_HANDLE_TAKEN") return 409;
  if (error.code === "INTERNAL_ERROR") return 500;
  return 400;
}
