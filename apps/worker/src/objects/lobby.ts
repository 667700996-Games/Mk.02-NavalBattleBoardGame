import { DurableObject } from "cloudflare:workers";
import { DomainError, type RoomSummary } from "../domain/protocol";
import {
  bodyObject,
  json,
  noContent,
  requireString,
  requireUuid,
} from "../http";
import type { WorkerEnv } from "../env";

interface LobbyState {
  rooms: Record<string, RoomSummary & { visibility: "PUBLIC" | "PRIVATE" }>;
  codes: Record<string, string>;
}

const EMPTY_STATE: LobbyState = { rooms: {}, codes: {} };

export class LobbyDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/rooms/register") {
        return await this.register(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/rooms/upsert") {
        return await this.upsert(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/rooms/lookup") {
        return await this.lookup(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/rooms/remove") {
        return await this.remove(await bodyObject(request));
      }
      if (request.method === "GET" && url.pathname === "/rooms")
        return await this.list();
      if (request.method === "GET" && url.pathname === "/rooms/spectatable")
        return await this.spectatable();
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "ROOM_NOT_FOUND" ? 404 : 409,
      );
    }
  }

  private async register(input: Record<string, unknown>): Promise<Response> {
    const summary = roomFromInput(input);
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<LobbyState>("state")) ??
        structuredClone(EMPTY_STATE);
      const existing = state.codes[summary.code];
      if (existing && existing !== summary.id)
        throw new DomainError("INVALID_STATE");
      state.rooms[summary.id] = summary;
      state.codes[summary.code] = summary.id;
      await transaction.put("state", state);
    });
    return noContent();
  }

  private async upsert(input: Record<string, unknown>): Promise<Response> {
    const summary = roomFromInput(input);
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<LobbyState>("state")) ??
        structuredClone(EMPTY_STATE);
      state.rooms[summary.id] = summary;
      state.codes[summary.code] = summary.id;
      await transaction.put("state", state);
    });
    return noContent();
  }

  private async lookup(input: Record<string, unknown>): Promise<Response> {
    const code = requireString(input.code).trim().toUpperCase();
    const state =
      (await this.ctx.storage.get<LobbyState>("state")) ?? EMPTY_STATE;
    const roomId = state.codes[code];
    if (!roomId) throw new DomainError("ROOM_NOT_FOUND");
    return json({ roomId });
  }

  private async remove(input: Record<string, unknown>): Promise<Response> {
    const roomId = requireUuid(input.roomId);
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<LobbyState>("state")) ??
        structuredClone(EMPTY_STATE);
      const room = state.rooms[roomId];
      if (room) delete state.codes[room.code];
      delete state.rooms[roomId];
      await transaction.put("state", state);
    });
    return noContent();
  }

  private async list(): Promise<Response> {
    const state =
      (await this.ctx.storage.get<LobbyState>("state")) ?? EMPTY_STATE;
    const rooms = Object.values(state.rooms)
      .filter(
        (room) =>
          room.visibility === "PUBLIC" &&
          !["FINISHED", "CANCELLED"].includes(room.status) &&
          room.playerCount < room.capacity,
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
      .map(({ visibility: _visibility, ...summary }) => summary);
    return json({ rooms });
  }

  private async spectatable(): Promise<Response> {
    const state =
      (await this.ctx.storage.get<LobbyState>("state")) ?? EMPTY_STATE;
    const rooms = Object.values(state.rooms)
      .filter(
        (room) =>
          room.visibility === "PUBLIC" &&
          room.gameId !== null &&
          room.status === "PLAYING",
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
      .slice(0, 100)
      .map(({ visibility: _visibility, ...summary }) => summary);
    return json({ rooms, delaySeconds: 30 });
  }
}

function roomFromInput(input: Record<string, unknown>): RoomSummary & {
  visibility: "PUBLIC" | "PRIVATE";
} {
  const summary = input.summary;
  if (!summary || typeof summary !== "object" || Array.isArray(summary)) {
    throw new DomainError("INVALID_REQUEST");
  }
  const room = summary as RoomSummary;
  return {
    ...room,
    id: requireUuid(room.id),
    code: requireString(room.code).toUpperCase(),
    visibility: input.visibility === "PRIVATE" ? "PRIVATE" : "PUBLIC",
  };
}
