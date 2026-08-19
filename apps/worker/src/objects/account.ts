import { DurableObject } from "cloudflare:workers";
import {
  DomainError,
  type PlayerAccount,
  type SessionRecord,
} from "../domain/protocol";
import {
  bodyObject,
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
}

const EMPTY_STATE: AccountState = { sessions: {}, accounts: {}, handles: {} };

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
    });
    return noContent();
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
      response = {
        account: structuredClone(account),
        session: structuredClone(session),
      };
    });
    if (!response) throw new DomainError("INTERNAL_ERROR");
    return json(response);
  }

  private async login(input: Record<string, unknown>): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    const recoveryKeyHash = requireString(input.recoveryKeyHash);
    const session = sessionFromInput(input);
    await this.mutate((state) => {
      const account = state.accounts[accountId];
      if (!account || account.recoveryKeyHash !== recoveryKeyHash) {
        throw new DomainError("UNAUTHORIZED");
      }
      session.accountId = account.id;
      session.nickname = account.handle;
      state.sessions[session.tokenHash] = session;
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
    return (
      (await this.ctx.storage.get<AccountState>("state")) ??
      structuredClone(EMPTY_STATE)
    );
  }

  private async mutate(action: (state: AccountState) => void): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<AccountState>("state")) ??
        structuredClone(EMPTY_STATE);
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

function status(error: DomainError): number {
  if (error.code === "UNAUTHORIZED") return 401;
  if (error.code === "ACCOUNT_HANDLE_TAKEN") return 409;
  if (error.code === "INTERNAL_ERROR") return 500;
  return 400;
}
