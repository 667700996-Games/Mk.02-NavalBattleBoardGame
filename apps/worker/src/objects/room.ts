import { DurableObject } from "cloudflare:workers";
import {
  confirmPlacement,
  createPracticeRoom,
  createRoom,
  disconnect,
  expireDisconnects,
  expireTurn,
  fire,
  joinRoom,
  leaveRoom,
  placeShips,
  playerForSession,
  reconnect,
  replayFor,
  roomSummary,
  sendChat,
  selectAiCoordinate,
  setLobbyReady,
  snapshotFor,
  startPlacement,
  spectatorSnapshot,
  surrender,
  timerState,
} from "../domain/game";
import {
  BALANCE,
  DomainError,
  protocolError,
  type ChatMessageType,
  type ClientEvent,
  type Coordinate,
  type InternalRoom,
  type QuickCommandId,
  type ServerEvent,
  type SessionRecord,
  type ShipPlacement,
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

interface SocketAttachment {
  sessionId: string;
  playerId: string;
  eventTimes: number[];
}

const ROOM_KEY = "room-v1";
const RECONNECT_GRACE_SECONDS = 90;
const TURN_DURATION_SECONDS = 60;
const COMPLETED_ROOM_RETENTION_MS = 90 * 86_400_000;

export class GameRoomDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/create") {
        return await this.create(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/create-practice") {
        return await this.createPractice(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/join") {
        return await this.join(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/matchmaking") {
        return await this.configureMatchmaking(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/leave") {
        return await this.leave(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/snapshot") {
        return await this.snapshot(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/replay") {
        return await this.replay(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/spectate") {
        return await this.spectate(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/invite-info") {
        return await this.inviteInfo(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/safety-context") {
        return await this.safetyContext(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/disconnect-session") {
        return await this.disconnectSession(await bodyObject(request));
      }
      if (request.method === "POST" && url.pathname === "/anonymize-account") {
        return await this.anonymizeAccount(await bodyObject(request));
      }
      if (request.method === "GET" && url.pathname === "/websocket") {
        return await this.websocket(request);
      }
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "ROOM_NOT_FOUND"
          ? 404
          : resolved.code === "UNAUTHORIZED"
            ? 401
            : 409,
      );
    }
  }

  async webSocketMessage(
    socket: WebSocket,
    message: string | ArrayBuffer,
  ): Promise<void> {
    const attachment =
      socket.deserializeAttachment() as SocketAttachment | null;
    if (!attachment) {
      socket.close(1011, "missing connection identity");
      return;
    }
    const nowMs = Date.now();
    attachment.eventTimes = attachment.eventTimes.filter(
      (time) => nowMs - time < 1_000,
    );
    if (attachment.eventTimes.length >= 60) {
      await this.recordIntegritySignal(attachment, "AUTOMATION", 3, 0.88, {
        detector: "WEBSOCKET_EVENT_BURST",
        eventsPerSecondLimit: 60,
        protocolVersion: 3,
      });
      this.send(socket, {
        type: "error",
        payload: protocolError(new DomainError("RATE_LIMITED")),
      });
      return;
    }
    attachment.eventTimes.push(nowMs);
    socket.serializeAttachment(attachment);
    if (
      typeof message !== "string" ||
      new TextEncoder().encode(message).byteLength > 64 * 1024
    ) {
      this.send(socket, {
        type: "error",
        payload: protocolError(new DomainError("INVALID_REQUEST")),
      });
      return;
    }
    let event: ClientEvent;
    try {
      const parsed: unknown = JSON.parse(message);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new DomainError("INVALID_REQUEST");
      }
      event = parsed as ClientEvent;
      await this.handleEvent(socket, attachment, event);
    } catch (error) {
      const eventType = typeof event!?.type === "string" ? event.type : "";
      const payload = eventPayload(event!);
      const requestId =
        typeof payload.requestId === "string" ? payload.requestId : undefined;
      const errorEvent =
        eventType === "player:ready"
          ? "player:ready:rejected"
          : eventType === "player:unready"
            ? "player:unready:rejected"
            : eventType === "game:start"
              ? "game:start:rejected"
              : eventType.startsWith("chat:")
                ? "chat:rejected"
                : eventType.startsWith("ships:")
                  ? "placement:rejected"
                  : "error";
      if (
        error instanceof DomainError &&
        [
          "NOT_YOUR_TURN",
          "COORDINATE_ALREADY_ATTACKED",
          "TURN_CONFLICT",
          "STALE_ROOM_VERSION",
          "PLACEMENT_MISMATCH",
          "UNAUTHORIZED",
        ].includes(error.code)
      ) {
        await this.recordIntegritySignal(
          attachment,
          "IMPOSSIBLE_ORDER",
          error.code === "UNAUTHORIZED" ? 4 : 2,
          error.code === "UNAUTHORIZED" ? 0.96 : 0.72,
          {
            detector: "AUTHORITATIVE_COMMAND_REJECTION",
            event: eventType || "unknown",
            errorCode: error.code,
            requestId: requestId ?? null,
            protocolVersion: 3,
          },
        );
      }
      this.send(socket, {
        type: errorEvent,
        payload: protocolError(error, requestId),
      });
    }
  }

  async webSocketClose(
    socket: WebSocket,
    code: number,
    reason: string,
  ): Promise<void> {
    const attachment =
      socket.deserializeAttachment() as SocketAttachment | null;
    if (!attachment) return;
    const others = this.ctx
      .getWebSockets(`session:${attachment.sessionId}`)
      .some(
        (candidate) =>
          candidate !== socket && candidate.readyState === WebSocket.OPEN,
      );
    if (!others) {
      const room = await this.load();
      const now = new Date().toISOString();
      const chatCount = room.chatMessages.length;
      if (
        disconnect(room, attachment.sessionId, RECONNECT_GRACE_SECONDS, now)
      ) {
        await this.persist(room);
        await this.broadcastNewChat(room, chatCount);
        this.broadcastSnapshots(room, "player:disconnected", now);
      }
    }
    if (socket.readyState !== WebSocket.CLOSED) socket.close(code, reason);
  }

  async webSocketError(socket: WebSocket): Promise<void> {
    socket.close(1011, "websocket error");
  }

  async alarm(): Promise<void> {
    const room = await this.load();
    await this.flushResultProjection(room);
    const now = new Date().toISOString();
    if (
      !room.resultProjectionPending &&
      ["FINISHED", "CANCELLED"].includes(room.status) &&
      Date.parse(now) >= completedRoomExpiry(room)
    ) {
      await this.expireCompletedRoom(room);
      return;
    }
    const chatCount = room.chatMessages.length;
    const beforeStatus = room.status;
    const disconnected = expireDisconnects(room, now);
    const expiration = expireTurn(room, now);
    if (disconnected || expiration) {
      await this.persist(room);
      await this.broadcastNewChat(room, chatCount);
      if (expiration && room.gameId) {
        this.broadcast({
          type: "turn:expired",
          payload: {
            roomId: room.id,
            gameId: room.gameId,
            ...expiration,
            serverTimestamp: now,
          },
        });
      }
      this.broadcastSnapshots(
        room,
        room.status === "FINISHED" ? "game:finished" : "turn:changed",
        now,
      );
      const timer = timerState(room, now);
      if (timer) this.broadcast({ type: "turn:started", payload: timer });
      await this.runAiTurns(room);
    } else if (beforeStatus === room.status) {
      await this.scheduleAlarm(room);
    }
  }

  private async create(input: Record<string, unknown>): Promise<Response> {
    if (await this.ctx.storage.get(ROOM_KEY))
      throw new DomainError("INVALID_STATE");
    const session = input.session as SessionRecord;
    const now = requireString(input.now);
    const room = createRoom({
      roomId: requireUuid(input.roomId),
      code: requireString(input.code).toUpperCase(),
      name: requireString(input.name),
      visibility: input.visibility === "PRIVATE" ? "PRIVATE" : "PUBLIC",
      rules: (input.rules ?? null) as never,
      session,
      playerId: requireUuid(input.playerId),
      now,
    });
    await this.register(room);
    return json(
      {
        snapshot: snapshotFor(room, session.id, now),
        summary: roomSummary(room),
        visibility: room.visibility,
      },
      201,
    );
  }

  private async createPractice(
    input: Record<string, unknown>,
  ): Promise<Response> {
    if (await this.ctx.storage.get(ROOM_KEY))
      throw new DomainError("INVALID_STATE");
    const session = input.session as SessionRecord;
    const aiSession = input.aiSession as SessionRecord;
    const now = requireString(input.now);
    const difficulty = requireString(input.difficulty);
    if (!["RECRUIT", "OFFICER", "ADMIRAL"].includes(difficulty))
      throw new DomainError("INVALID_REQUEST");
    const room = createPracticeRoom(
      {
        roomId: requireUuid(input.roomId),
        code: requireString(input.code).toUpperCase(),
        name: "AI 전술 훈련",
        visibility: "PRIVATE",
        session,
        playerId: requireUuid(input.playerId),
        now,
      },
      difficulty as "RECRUIT" | "OFFICER" | "ADMIRAL",
      aiSession,
      requireUuid(input.aiPlayerId),
    );
    await this.register(room);
    return json({ snapshot: snapshotFor(room, session.id, now) }, 201);
  }

  private async join(input: Record<string, unknown>): Promise<Response> {
    const room = await this.load();
    const session = input.session as SessionRecord;
    const now = requireString(input.now);
    await this.ensureNotBlocked(room, session);
    joinRoom(room, session, requireUuid(input.playerId), now);
    await this.persist(room);
    await this.broadcastNewChat(
      room,
      Math.max(0, room.chatMessages.length - 1),
    );
    this.broadcastSnapshots(room, "player:joined", now);
    return json({
      snapshot: snapshotFor(room, session.id, now),
      summary: roomSummary(room),
      visibility: room.visibility,
    });
  }

  private async configureMatchmaking(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const room = await this.load();
    if (
      room.players.length !== 2 ||
      room.gameId ||
      room.matchmakingQuality !== null
    ) {
      throw new DomainError("INVALID_STATE");
    }
    if (
      !input.quality ||
      typeof input.quality !== "object" ||
      Array.isArray(input.quality)
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    const quality = input.quality as Record<string, unknown>;
    if (
      !["CASUAL", "RANKED"].includes(String(quality.pool)) ||
      !["EXACT", "REGIONAL", "GLOBAL"].includes(String(quality.phase))
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    room.matchmakingQuality =
      input.quality as InternalRoom["matchmakingQuality"];
    if (input.rankedMatch !== null) {
      if (
        !input.rankedMatch ||
        typeof input.rankedMatch !== "object" ||
        Array.isArray(input.rankedMatch)
      ) {
        throw new DomainError("INVALID_REQUEST");
      }
      const ranked = input.rankedMatch as Record<string, unknown>;
      const contentRevision = Number(ranked.contentRevision);
      if (!Number.isInteger(contentRevision) || contentRevision < 0)
        throw new DomainError("INVALID_REQUEST");
      room.rankedMatch = {
        seasonId: requireString(ranked.seasonId),
        contentRevision,
      };
    }
    room.updatedAt = requireString(input.now);
    room.version += 1;
    await this.persist(room);
    return noContent();
  }

  private async leave(input: Record<string, unknown>): Promise<Response> {
    const room = await this.load();
    const sessionId = requireUuid(input.sessionId);
    const now = requireString(input.now);
    const chatCount = room.chatMessages.length;
    leaveRoom(room, sessionId, now);
    await this.persist(room);
    await this.broadcastNewChat(room, chatCount);
    this.broadcastSnapshots(
      room,
      room.status === "FINISHED" ? "game:finished" : "player:left",
      now,
    );
    return noContent();
  }

  private async snapshot(input: Record<string, unknown>): Promise<Response> {
    const room = await this.load();
    return json(
      snapshotFor(room, requireUuid(input.sessionId), requireString(input.now)),
    );
  }

  private async replay(input: Record<string, unknown>): Promise<Response> {
    return json(replayFor(await this.load(), requireUuid(input.sessionId)));
  }

  private async spectate(input: Record<string, unknown>): Promise<Response> {
    return json(spectatorSnapshot(await this.load(), requireString(input.now)));
  }

  private async inviteInfo(input: Record<string, unknown>): Promise<Response> {
    const room = await this.load();
    playerForSession(room, requireUuid(input.sessionId));
    if (room.status !== "WAITING_FOR_OPPONENT" || room.players.length !== 1)
      throw new DomainError("INVALID_STATE");
    return json({
      roomId: room.id,
      roomCode: room.code,
      roomName: room.name,
    });
  }

  private async safetyContext(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const room = await this.load();
    const actor = playerForSession(room, requireUuid(input.sessionId));
    const targetPlayerId = requireUuid(input.targetPlayerId);
    const target = room.players.find((player) => player.id === targetPlayerId);
    if (!target || target.id === actor.id || target.kind === "AI")
      throw new DomainError("INVALID_REQUEST");
    return json({
      roomId: room.id,
      roomVersion: room.version,
      roomState: room.status,
      actor: {
        playerId: actor.id,
        sessionId: actor.sessionId,
        accountId: actor.accountId ?? null,
        nickname: actor.nickname,
      },
      target: {
        playerId: target.id,
        sessionId: target.sessionId,
        accountId: target.accountId ?? null,
        nickname: target.nickname,
      },
      messages: room.chatMessages.slice(-20).reverse(),
      recentAttacks: (room.game?.attacks ?? []).slice(-20).reverse(),
    });
  }

  private async disconnectSession(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const sessionId = requireUuid(input.sessionId);
    const now = requireString(input.now);
    const room = await this.load();
    const chatCount = room.chatMessages.length;
    let changed = false;
    try {
      changed = Boolean(
        disconnect(room, sessionId, RECONNECT_GRACE_SECONDS, now),
      );
    } catch (error) {
      if (!(error instanceof DomainError) || error.code !== "NOT_ROOM_MEMBER") {
        throw error;
      }
    }
    if (changed) {
      await this.persist(room);
      await this.broadcastNewChat(room, chatCount);
      this.broadcastSnapshots(room, "player:disconnected", now);
    }
    for (const socket of this.ctx.getWebSockets(`session:${sessionId}`)) {
      socket.close(4001, "session revoked");
    }
    return noContent();
  }

  private async anonymizeAccount(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    if (!Array.isArray(input.sessionIds))
      throw new DomainError("INVALID_REQUEST");
    const sessionIds = new Set(input.sessionIds.map(requireUuid));
    const room = await this.load();
    if (!["FINISHED", "CANCELLED"].includes(room.status))
      throw new DomainError("INVALID_STATE");
    const deletedPlayers = room.players.filter(
      (player) =>
        player.accountId === accountId || sessionIds.has(player.sessionId),
    );
    if (deletedPlayers.length === 0) return noContent();
    const deletedPlayerIds = new Set(deletedPlayers.map((player) => player.id));
    const deletedNames = deletedPlayers.map((player) => player.nickname);
    for (const player of deletedPlayers) {
      player.sessionId = crypto.randomUUID();
      player.accountId = null;
      player.nickname = "Deleted Commander";
    }
    for (const message of room.chatMessages) {
      for (const name of deletedNames) {
        message.content = message.content.replaceAll(name, "Deleted Commander");
      }
      if (
        (message.playerId && deletedPlayerIds.has(message.playerId)) ||
        deletedNames.includes(message.nickname)
      ) {
        message.nickname = "Deleted Commander";
        if (message.type === "TEXT") message.content = "[deleted]";
      }
    }
    room.name = "Archived Operation";
    room.updatedAt = requireString(input.now);
    room.version += 1;
    await this.persist(room);
    return noContent();
  }

  private async websocket(request: Request): Promise<Response> {
    if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
      return json({ code: "UPGRADE_REQUIRED" }, 426);
    }
    const protocolHeader = request.headers.get("sec-websocket-protocol");
    const offered = (protocolHeader ?? "")
      .split(",")
      .map((value) => value.trim());
    if (protocolHeader !== null && !offered.includes("mk01.v3")) {
      return json(
        protocolError(new DomainError("SERVER_PROTOCOL_MISMATCH")),
        426,
      );
    }
    const sessionId = requireUuid(request.headers.get("x-mk01-session-id"));
    const room = await this.load();
    const player = playerForSession(room, sessionId);
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    const attachment: SocketAttachment = {
      sessionId,
      playerId: player.id,
      eventTimes: [],
    };
    server.serializeAttachment(attachment);
    this.ctx.acceptWebSocket(server, [
      `session:${sessionId}`,
      `player:${player.id}`,
    ]);
    const now = new Date().toISOString();
    const chatCount = room.chatMessages.length;
    if (reconnect(room, sessionId, now)) {
      await this.persist(room);
      await this.broadcastNewChat(room, chatCount);
      this.broadcastSnapshots(room, "player:reconnected", now);
    }
    this.send(server, {
      type: "game:snapshot",
      payload: snapshotFor(room, sessionId, now),
    });
    this.send(server, {
      type: "chat:history",
      payload: {
        roomId: room.id,
        messages: await this.chatHistoryFor(room, sessionId),
      },
    });
    const timer = timerState(room, now);
    if (timer) this.send(server, { type: "game:timer-sync", payload: timer });
    const responseHeaders = new Headers();
    if (protocolHeader !== null)
      responseHeaders.set("sec-websocket-protocol", "mk01.v3");
    return new Response(null, {
      status: 101,
      webSocket: client,
      headers: responseHeaders,
    });
  }

  private async handleEvent(
    socket: WebSocket,
    attachment: SocketAttachment,
    event: ClientEvent,
  ): Promise<void> {
    if (typeof event.type !== "string")
      throw new DomainError("INVALID_REQUEST");
    const payload = eventPayload(event);
    if (event.type === "heartbeat") {
      this.send(socket, {
        type: "heartbeat",
        payload: { serverTime: new Date().toISOString() },
      });
      return;
    }
    const room = await this.load();
    const now = new Date().toISOString();
    const chatCount = room.chatMessages.length;
    if (event.type !== "game:sync") requireRoom(room, payload.roomId);
    switch (event.type) {
      case "player:ready":
      case "player:unready": {
        const ready = event.type === "player:ready";
        const result = setLobbyReady(
          room,
          attachment.sessionId,
          requireUuid(payload.requestId),
          requireUuid(payload.playerId),
          ready,
          now,
        );
        if (!result.duplicate) await this.persist(room);
        this.send(socket, {
          type: ready ? "player:ready:accepted" : "player:unready:accepted",
          payload: result.record,
        });
        if (!result.duplicate) {
          await this.broadcastNewChat(room, chatCount);
          this.broadcastSnapshots(room, "room:updated", now);
        }
        return;
      }
      case "game:start": {
        const result = startPlacement(
          room,
          attachment.sessionId,
          requireUuid(payload.requestId),
          requireUuid(payload.playerId),
          requireNumber(payload.roomVersion),
          crypto.randomUUID(),
          now,
        );
        if (!result.duplicate) await this.persist(room);
        this.send(socket, {
          type: "game:start:accepted",
          payload: result.record,
        });
        if (!result.duplicate) {
          await this.broadcastNewChat(room, chatCount);
          this.broadcastSnapshots(room, "game:placement-started", now);
        }
        return;
      }
      case "ships:place": {
        assertClaimedPlayer(room, attachment.sessionId, payload.playerId);
        placeShips(
          room,
          attachment.sessionId,
          placements(payload.placements),
          now,
        );
        await this.persist(room);
        this.send(socket, {
          type: "placement:accepted",
          payload: snapshotFor(room, attachment.sessionId, now),
        });
        return;
      }
      case "ships:confirm": {
        assertClaimedPlayer(room, attachment.sessionId, payload.playerId);
        const randomByte = crypto.getRandomValues(new Uint8Array(1))[0] ?? 0;
        const firstPlayer = room.players[randomByte % room.players.length];
        if (!firstPlayer) throw new DomainError("INVALID_STATE");
        const started = confirmPlacement(
          room,
          attachment.sessionId,
          placements(payload.placements),
          TURN_DURATION_SECONDS,
          firstPlayer.id,
          now,
        );
        await this.persist(room);
        this.send(socket, {
          type: "placement:accepted",
          payload: snapshotFor(room, attachment.sessionId, now),
        });
        if (started) {
          await this.broadcastNewChat(room, chatCount);
          this.broadcastSnapshots(room, "game:started", now);
          const timer = timerState(room, now);
          if (timer) this.broadcast({ type: "turn:started", payload: timer });
          await this.runAiTurns(room);
        } else {
          this.broadcastSnapshots(room, "room:updated", now);
        }
        return;
      }
      case "attack:fire": {
        const result = fire(
          room,
          attachment.sessionId,
          requireUuid(payload.requestId),
          requireUuid(payload.playerId),
          coordinate(payload.coordinate),
          requireNumber(payload.expectedVersion),
          requireNumber(payload.turnNumber),
          now,
        );
        if (!result.duplicate) await this.persist(room);
        if (result.duplicate) {
          this.send(socket, { type: "attack:result", payload: result.record });
          this.send(socket, {
            type: "game:snapshot",
            payload: snapshotFor(room, attachment.sessionId, now),
          });
        } else {
          this.broadcast({ type: "attack:result", payload: result.record });
          if (result.record.sunkShip)
            this.broadcast({ type: "ship:sunk", payload: result.record });
          await this.broadcastNewChat(room, chatCount);
          this.broadcastSnapshots(
            room,
            result.record.winnerId ? "game:finished" : "turn:changed",
            now,
          );
          const timer = timerState(room, now);
          if (timer) this.broadcast({ type: "turn:started", payload: timer });
          await this.runAiTurns(room);
        }
        return;
      }
      case "game:surrender": {
        const record = surrender(
          room,
          attachment.sessionId,
          requireUuid(payload.playerId),
          now,
        );
        await this.persist(room);
        this.broadcast({ type: "game:surrendered", payload: record });
        await this.broadcastNewChat(room, chatCount);
        this.broadcastSnapshots(room, "game:finished", now);
        return;
      }
      case "chat:send": {
        const result = sendChat(
          room,
          attachment.sessionId,
          requireUuid(payload.clientMessageId),
          chatType(payload.type),
          payload.content === null ? null : requireString(payload.content),
          payload.commandId === null ? null : quickCommand(payload.commandId),
          now,
        );
        if (!result.duplicate) await this.persist(room);
        if (result.duplicate)
          this.send(socket, { type: "chat:message", payload: result.message });
        else await this.broadcastChatMessage(room, result.message);
        return;
      }
      case "chat:typing": {
        const player = playerForSession(room, attachment.sessionId);
        if (typeof payload.isTyping !== "boolean")
          throw new DomainError("INVALID_REQUEST");
        for (const recipientSocket of this.ctx.getWebSockets()) {
          const recipientAttachment =
            recipientSocket.deserializeAttachment() as SocketAttachment | null;
          const recipient = room.players.find(
            (candidate) => candidate.id === recipientAttachment?.playerId,
          );
          if (
            !recipient ||
            recipient.id === player.id ||
            (await this.communicationSuppressed(recipient, player))
          ) {
            continue;
          }
          this.send(recipientSocket, {
            type: "chat:typing",
            payload: {
              roomId: room.id,
              playerId: player.id,
              nickname: player.nickname,
              isTyping: payload.isTyping,
            },
          });
        }
        return;
      }
      case "game:sync": {
        requireRoom(room, payload.roomId);
        this.send(socket, {
          type: "game:snapshot",
          payload: snapshotFor(room, attachment.sessionId, now),
        });
        this.send(socket, {
          type: "chat:history",
          payload: {
            roomId: room.id,
            messages: await this.chatHistoryFor(room, attachment.sessionId),
          },
        });
        const timer = timerState(room, now);
        if (timer)
          this.send(socket, { type: "game:timer-sync", payload: timer });
        return;
      }
      case "room:leave": {
        leaveRoom(room, attachment.sessionId, now);
        await this.persist(room);
        await this.clearSessionRoom(attachment.sessionId);
        await this.broadcastNewChat(room, chatCount);
        this.broadcastSnapshots(room, "player:left", now);
        return;
      }
      default:
        throw new DomainError("INVALID_REQUEST");
    }
  }

  private async load(): Promise<InternalRoom> {
    const room = await this.ctx.storage.get<InternalRoom>(ROOM_KEY);
    if (!room) throw new DomainError("ROOM_NOT_FOUND");
    return room;
  }

  private async persist(room: InternalRoom): Promise<void> {
    await this.ctx.storage.put(ROOM_KEY, room);
    await this.scheduleAlarm(room);
    const lobby = this.env.LOBBY.get(this.env.LOBBY.idFromName("global-v1"));
    const response = await lobby.fetch(
      internalRequest("/rooms/upsert", {
        summary: roomSummary(room),
        visibility: room.visibility,
      }),
    );
    if (!response.ok) throw new DomainError("INTERNAL_ERROR");
    await this.flushResultProjection(room);
  }

  private async runAiTurns(room: InternalRoom): Promise<void> {
    let safety = 0;
    while (
      room.status === "PLAYING" &&
      room.game &&
      !room.game.result &&
      safety < BALANCE.manifest.boardSize
    ) {
      const ai = room.players.find(
        (player) =>
          player.kind === "AI" && player.id === room.game?.currentPlayerId,
      );
      if (!ai) return;
      const coordinate = selectAiCoordinate(room, ai.id);
      if (!coordinate) return;
      const now = new Date().toISOString();
      const chatCount = room.chatMessages.length;
      const result = fire(
        room,
        ai.sessionId,
        crypto.randomUUID(),
        ai.id,
        coordinate,
        room.version,
        room.game.turnNumber,
        now,
      );
      await this.persist(room);
      this.broadcast({ type: "attack:result", payload: result.record });
      if (result.record.sunkShip)
        this.broadcast({ type: "ship:sunk", payload: result.record });
      await this.broadcastNewChat(room, chatCount);
      this.broadcastSnapshots(
        room,
        result.record.winnerId ? "game:finished" : "turn:changed",
        now,
      );
      const timer = timerState(room, now);
      if (timer) this.broadcast({ type: "turn:started", payload: timer });
      safety += 1;
    }
  }

  private async flushResultProjection(room: InternalRoom): Promise<void> {
    if (!room.resultProjectionPending || !room.game?.result) return;
    try {
      const accounts = this.env.ACCOUNTS.get(
        this.env.ACCOUNTS.idFromName("global-v1"),
      );
      const identityResponse = await accounts.fetch(
        internalRequest("/sessions/identities", {
          sessionIds: room.players.map((player) => player.sessionId),
        }),
      );
      if (!identityResponse.ok) throw new DomainError("INTERNAL_ERROR");
      const identityPayload = (await identityResponse.json()) as {
        identities: Array<{
          sessionId: string;
          accountId: string | null;
          nickname: string;
        }>;
      };
      const progression = this.env.PROGRESSION.get(
        this.env.PROGRESSION.idFromName("global-v1"),
      );
      const participants = room.players
        .filter((player) => player.kind === "HUMAN")
        .map((player) => {
          const identity = identityPayload.identities.find(
            (candidate) => candidate.sessionId === player.sessionId,
          );
          const accountId = identity?.accountId ?? player.accountId ?? null;
          return {
            identityId: accountId ?? player.sessionId,
            accountId,
            playerId: player.id,
            handle: identity?.nickname ?? player.nickname,
          };
        });
      const response = await progression.fetch(
        internalRequest("/results/record", {
          roomId: room.id,
          roomName: room.name,
          balance: BALANCE,
          result: room.game.result,
          rankedMatch: room.rankedMatch ?? null,
          participants,
        }),
      );
      if (!response.ok) throw new DomainError("INTERNAL_ERROR");
      if (!room.gameId) throw new DomainError("INTERNAL_ERROR");
      const operations = this.env.OPERATIONS.get(
        this.env.OPERATIONS.idFromName("global-v1"),
      );
      const assessment = await operations.fetch(
        internalRequest("/integrity/assess", {
          roomId: room.id,
          gameId: room.gameId,
          result: room.game.result,
          participants,
          now: new Date().toISOString(),
        }),
      );
      if (!assessment.ok) throw new DomainError("INTERNAL_ERROR");
      room.resultProjectionPending = false;
      await this.ctx.storage.put(ROOM_KEY, room);
    } catch {
      await this.ctx.storage.setAlarm(Date.now() + 1_000);
    }
  }

  private async register(room: InternalRoom): Promise<void> {
    await this.ctx.storage.put(ROOM_KEY, room);
    await this.scheduleAlarm(room);
    const lobby = this.env.LOBBY.get(this.env.LOBBY.idFromName("global-v1"));
    const response = await lobby.fetch(
      internalRequest("/rooms/register", {
        summary: roomSummary(room),
        visibility: room.visibility,
      }),
    );
    if (response.ok) return;
    await this.ctx.storage.delete(ROOM_KEY);
    await this.ctx.storage.deleteAlarm();
    throw new DomainError(
      response.status === 409 ? "INVALID_STATE" : "INTERNAL_ERROR",
    );
  }

  private async clearSessionRoom(sessionId: string): Promise<void> {
    const accounts = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    const response = await accounts.fetch(
      internalRequest("/sessions/room-by-id", { sessionId, roomId: null }),
    );
    if (!response.ok) throw new DomainError("INTERNAL_ERROR");
  }

  private async expireCompletedRoom(room: InternalRoom): Promise<void> {
    for (const socket of this.ctx.getWebSockets())
      socket.close(1001, "archived room retention expired");
    const accounts = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    await Promise.all(
      room.players
        .filter((player) => player.kind === "HUMAN")
        .map((player) =>
          accounts.fetch(
            internalRequest("/sessions/room-by-id", {
              sessionId: player.sessionId,
              roomId: null,
            }),
          ),
        ),
    );
    const lobby = this.env.LOBBY.get(this.env.LOBBY.idFromName("global-v1"));
    await lobby.fetch(internalRequest("/rooms/remove", { roomId: room.id }));
    await this.ctx.storage.delete(ROOM_KEY);
    await this.ctx.storage.deleteAlarm();
  }

  private async ensureNotBlocked(
    room: InternalRoom,
    joining: SessionRecord,
  ): Promise<void> {
    const existing = room.players.find((player) => player.kind === "HUMAN");
    if (!existing) return;
    const accounts = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    const response = await accounts.fetch(
      internalRequest("/sessions/identities", {
        sessionIds: [existing.sessionId, joining.id],
      }),
    );
    if (!response.ok) throw new DomainError("INTERNAL_ERROR");
    const payload = (await response.json()) as {
      identities: Array<{ sessionId: string; accountId: string | null }>;
    };
    const existingIdentity = payload.identities.find(
      (identity) => identity.sessionId === existing.sessionId,
    );
    const joiningIdentity = payload.identities.find(
      (identity) => identity.sessionId === joining.id,
    );
    const social = this.env.SOCIAL.get(this.env.SOCIAL.idFromName("global-v1"));
    const blocked = await social.fetch(
      internalRequest("/blocked", {
        firstIdentityId: existingIdentity?.accountId ?? existing.sessionId,
        secondIdentityId:
          joiningIdentity?.accountId ?? joining.accountId ?? joining.id,
      }),
    );
    if (!blocked.ok) throw new DomainError("INTERNAL_ERROR");
    if (((await blocked.json()) as { blocked: boolean }).blocked)
      throw new DomainError("PLAYER_BLOCKED");
  }

  private async recordIntegritySignal(
    attachment: SocketAttachment,
    kind: "IMPOSSIBLE_ORDER" | "AUTOMATION",
    severity: number,
    confidence: number,
    evidence: Record<string, unknown>,
  ): Promise<void> {
    try {
      const room = await this.load();
      const player = playerForSession(room, attachment.sessionId);
      const accounts = this.env.ACCOUNTS.get(
        this.env.ACCOUNTS.idFromName("global-v1"),
      );
      const identityResponse = await accounts.fetch(
        internalRequest("/sessions/identities", {
          sessionIds: [attachment.sessionId],
        }),
      );
      if (!identityResponse.ok) return;
      const identity = (
        (await identityResponse.json()) as {
          identities: Array<{ accountId: string | null }>;
        }
      ).identities[0];
      const operations = this.env.OPERATIONS.get(
        this.env.OPERATIONS.idFromName("global-v1"),
      );
      await operations.fetch(
        internalRequest("/integrity/record", {
          subjectIdentityId:
            identity?.accountId ?? player.accountId ?? attachment.sessionId,
          roomId: room.id,
          kind,
          severity,
          confidence,
          evidence,
          now: new Date().toISOString(),
        }),
      );
    } catch {
      // Integrity telemetry must not replace the authoritative command response.
    }
  }

  private async scheduleAlarm(room: InternalRoom): Promise<void> {
    const candidates = [
      room.resultProjectionPending
        ? new Date(Date.now() + 1_000).toISOString()
        : null,
      ["FINISHED", "CANCELLED"].includes(room.status)
        ? new Date(completedRoomExpiry(room)).toISOString()
        : null,
      room.game?.result ? null : room.game?.turnDeadlineAt,
      ...Object.values(room.disconnectedDeadlines),
    ]
      .filter((value): value is string => Boolean(value))
      .map(Date.parse)
      .filter(Number.isFinite)
      .sort((left, right) => left - right);
    if (candidates[0] !== undefined)
      await this.ctx.storage.setAlarm(candidates[0]);
    else await this.ctx.storage.deleteAlarm();
  }

  private send(socket: WebSocket, event: ServerEvent): void {
    if (socket.readyState === WebSocket.OPEN)
      socket.send(JSON.stringify(event));
  }

  private broadcast(event: ServerEvent): void {
    for (const socket of this.ctx.getWebSockets()) this.send(socket, event);
  }

  private broadcastSnapshots(
    room: InternalRoom,
    type: string,
    now: string,
  ): void {
    for (const socket of this.ctx.getWebSockets()) {
      const attachment =
        socket.deserializeAttachment() as SocketAttachment | null;
      if (!attachment) continue;
      try {
        this.send(socket, {
          type,
          payload: snapshotFor(room, attachment.sessionId, now),
        });
      } catch {
        socket.close(1008, "room membership ended");
      }
    }
  }

  private async broadcastNewChat(
    room: InternalRoom,
    previousCount: number,
  ): Promise<void> {
    for (const message of room.chatMessages.slice(previousCount)) {
      await this.broadcastChatMessage(room, message);
    }
  }

  private async broadcastChatMessage(
    room: InternalRoom,
    message: InternalRoom["chatMessages"][number],
  ): Promise<void> {
    const sender = message.playerId
      ? room.players.find((player) => player.id === message.playerId)
      : null;
    for (const socket of this.ctx.getWebSockets()) {
      const attachment =
        socket.deserializeAttachment() as SocketAttachment | null;
      if (!attachment) continue;
      const recipient = room.players.find(
        (player) => player.id === attachment.playerId,
      );
      if (
        sender &&
        recipient &&
        sender.id !== recipient.id &&
        (await this.communicationSuppressed(recipient, sender))
      ) {
        continue;
      }
      this.send(socket, { type: "chat:message", payload: message });
    }
  }

  private async chatHistoryFor(
    room: InternalRoom,
    recipientSessionId: string,
  ): Promise<InternalRoom["chatMessages"]> {
    const recipient = playerForSession(room, recipientSessionId);
    const messages: InternalRoom["chatMessages"] = [];
    for (const message of room.chatMessages) {
      const sender = message.playerId
        ? room.players.find((player) => player.id === message.playerId)
        : null;
      if (
        sender &&
        sender.id !== recipient.id &&
        (await this.communicationSuppressed(recipient, sender))
      ) {
        continue;
      }
      messages.push(message);
    }
    return messages;
  }

  private async communicationSuppressed(
    recipient: InternalRoom["players"][number],
    sender: InternalRoom["players"][number],
  ): Promise<boolean> {
    const accounts = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    const identitiesResponse = await accounts.fetch(
      internalRequest("/sessions/identities", {
        sessionIds: [recipient.sessionId, sender.sessionId],
      }),
    );
    if (!identitiesResponse.ok) return true;
    const identities = (await identitiesResponse.json()) as {
      identities: Array<{
        sessionId: string;
        accountId: string | null;
      }>;
    };
    const recipientIdentity = identities.identities.find(
      (identity) => identity.sessionId === recipient.sessionId,
    );
    const senderIdentity = identities.identities.find(
      (identity) => identity.sessionId === sender.sessionId,
    );
    const social = this.env.SOCIAL.get(this.env.SOCIAL.idFromName("global-v1"));
    const response = await social.fetch(
      internalRequest("/suppressed", {
        recipientIdentityId:
          recipientIdentity?.accountId ??
          recipient.accountId ??
          recipient.sessionId,
        senderIdentityId:
          senderIdentity?.accountId ?? sender.accountId ?? sender.sessionId,
      }),
    );
    if (!response.ok) return true;
    return ((await response.json()) as { suppressed: boolean }).suppressed;
  }
}

function eventPayload(event: ClientEvent | undefined): Record<string, unknown> {
  if (
    !event ||
    !event.payload ||
    typeof event.payload !== "object" ||
    Array.isArray(event.payload)
  ) {
    return {};
  }
  return event.payload as Record<string, unknown>;
}

function requireRoom(room: InternalRoom, value: unknown): void {
  if (requireUuid(value) !== room.id) throw new DomainError("ROOM_NOT_FOUND");
}

function completedRoomExpiry(room: InternalRoom): number {
  const completedAt = room.game?.result?.finishedAt ?? room.updatedAt;
  return Date.parse(completedAt) + COMPLETED_ROOM_RETENTION_MS;
}

function requireNumber(value: unknown): number {
  if (!Number.isInteger(value) || (value as number) < 0)
    throw new DomainError("INVALID_REQUEST");
  return value as number;
}

function coordinate(value: unknown): Coordinate {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new DomainError("INVALID_REQUEST");
  }
  const candidate = value as Record<string, unknown>;
  return {
    row: requireNumber(candidate.row),
    col: requireNumber(candidate.col),
  };
}

function placements(value: unknown): ShipPlacement[] {
  if (!Array.isArray(value)) throw new DomainError("INVALID_REQUEST");
  return value as ShipPlacement[];
}

function assertClaimedPlayer(
  room: InternalRoom,
  sessionId: string,
  value: unknown,
): void {
  if (playerForSession(room, sessionId).id !== requireUuid(value)) {
    throw new DomainError("UNAUTHORIZED");
  }
}

function chatType(value: unknown): ChatMessageType {
  if (!["TEXT", "QUICK_COMMAND", "EMOJI"].includes(String(value))) {
    throw new DomainError("INVALID_CHAT_MESSAGE");
  }
  return value as ChatMessageType;
}

function quickCommand(value: unknown): QuickCommandId {
  const commands = [
    "GOOD_GAME",
    "WAIT_A_MOMENT",
    "READY",
    "NICE_SHOT",
    "LUCKY",
    "GO_FIRST",
    "THANK_YOU",
  ];
  if (!commands.includes(String(value)))
    throw new DomainError("INVALID_QUICK_COMMAND");
  return value as QuickCommandId;
}
