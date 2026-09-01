import { describe, expect, it } from "vitest";
import {
  confirmPlacement,
  createRoom,
  expireTurn,
  fire,
  fireSkill,
  joinRoom,
  placeShips,
  reconnect,
  replayFor,
  disconnect,
  sendChat,
  setLobbyReady,
  snapshotFor,
  startPlacement,
  spectatorSnapshot,
  surrender,
} from "./game";
import {
  DomainError,
  type InternalRoom,
  type SessionRecord,
  type ShipPlacement,
} from "./protocol";

const HOST_SESSION = "00000000-0000-4000-8000-000000000001";
const GUEST_SESSION = "00000000-0000-4000-8000-000000000002";
const HOST_PLAYER = "00000000-0000-4000-8000-000000000011";
const GUEST_PLAYER = "00000000-0000-4000-8000-000000000012";
const ROOM_ID = "00000000-0000-4000-8000-000000000021";
const GAME_ID = "00000000-0000-4000-8000-000000000031";
const T0 = "2026-08-19T00:00:00.000Z";

function session(id: string, nickname: string): SessionRecord {
  return {
    id,
    accountId: null,
    nickname,
    tokenHash: `hash-${id}`,
    createdAt: T0,
    lastSeenAt: T0,
    currentRoomId: null,
    expiresAt: "2026-09-19T00:00:00.000Z",
  };
}

function fleet(rowOffset: number): ShipPlacement[] {
  return [
    ["CARRIER", 0],
    ["BATTLESHIP", 1],
    ["CRUISER", 2],
    ["SUBMARINE", 3],
    ["DESTROYER", 4],
  ].map(([kind, offset]) => ({
    kind: kind as ShipPlacement["kind"],
    origin: { row: rowOffset + Number(offset), col: 0 },
    orientation: "HORIZONTAL",
  }));
}

function readyRoom(
  mode: "CLASSIC" | "SALVO" = "CLASSIC",
  tacticalSkillsEnabled = false,
): InternalRoom {
  const room = createRoom({
    roomId: ROOM_ID,
    code: "ABC123",
    name: "North Sea",
    visibility: "PRIVATE",
    rules: { mode, turnDurationSeconds: 60, tacticalSkillsEnabled },
    session: session(HOST_SESSION, "Alpha"),
    playerId: HOST_PLAYER,
    now: T0,
  });
  joinRoom(
    room,
    session(GUEST_SESSION, "Bravo"),
    GUEST_PLAYER,
    "2026-08-19T00:00:01.000Z",
  );
  setLobbyReady(
    room,
    HOST_SESSION,
    "00000000-0000-4000-8000-000000000101",
    HOST_PLAYER,
    true,
    "2026-08-19T00:00:02.000Z",
  );
  setLobbyReady(
    room,
    GUEST_SESSION,
    "00000000-0000-4000-8000-000000000102",
    GUEST_PLAYER,
    true,
    "2026-08-19T00:00:03.000Z",
  );
  startPlacement(
    room,
    HOST_SESSION,
    "00000000-0000-4000-8000-000000000103",
    HOST_PLAYER,
    room.version,
    GAME_ID,
    "2026-08-19T00:00:04.000Z",
  );
  placeShips(room, HOST_SESSION, fleet(0), "2026-08-19T00:00:05.000Z");
  placeShips(room, GUEST_SESSION, fleet(5), "2026-08-19T00:00:06.000Z");
  confirmPlacement(
    room,
    HOST_SESSION,
    fleet(0),
    60,
    HOST_PLAYER,
    "2026-08-19T00:00:07.000Z",
  );
  confirmPlacement(
    room,
    GUEST_SESSION,
    fleet(5),
    60,
    HOST_PLAYER,
    "2026-08-19T00:00:08.000Z",
  );
  return room;
}

describe("Cloudflare authoritative room domain", () => {
  it("preserves the explicit ready/start/placement transitions and private projections", () => {
    const room = readyRoom();
    expect(room.status).toBe("PLAYING");
    expect(room.game?.currentPlayerId).toBe(HOST_PLAYER);
    const host = snapshotFor(room, HOST_SESSION, "2026-08-19T00:00:09.000Z");
    const guest = snapshotFor(room, GUEST_SESSION, "2026-08-19T00:00:09.000Z");
    expect(host.protocolVersion).toBe(4);
    expect(host.ownBoard?.ships).toHaveLength(5);
    expect(host.targetBoard?.attacks).toEqual([]);
    expect(host.revealedBoard).toBeNull();
    expect(JSON.stringify(host)).not.toContain(HOST_SESSION);
    expect(JSON.stringify(host)).not.toContain("sessionId");
    expect(JSON.stringify(host)).not.toContain(JSON.stringify(guest.ownBoard));
  });

  it("rejects forged turns and resolves a complete two-player victory on the server", () => {
    const room = readyRoom();
    room.visibility = "PUBLIC";
    expect(() =>
      fire(
        room,
        GUEST_SESSION,
        crypto.randomUUID(),
        GUEST_PLAYER,
        { row: 9, col: 9 },
        room.version,
        1,
        "2026-08-19T00:00:09.000Z",
      ),
    ).toThrowError(new DomainError("NOT_YOUR_TURN"));
    const activeSpectator = spectatorSnapshot(room, "2026-08-19T00:00:39.000Z");
    expect(activeSpectator.phase).toBe("LIVE");
    expect(JSON.stringify(activeSpectator)).not.toContain('"ships"');
    expect(JSON.stringify(activeSpectator)).not.toContain("sessionId");

    const targets = fleet(5).flatMap((placement) => {
      const size = {
        CARRIER: 5,
        BATTLESHIP: 4,
        CRUISER: 3,
        SUBMARINE: 3,
        DESTROYER: 2,
      }[placement.kind];
      return Array.from({ length: size }, (_, col) => ({
        row: placement.origin.row,
        col,
      }));
    });
    for (let index = 0; index < targets.length; index += 1) {
      const result = fire(
        room,
        HOST_SESSION,
        `00000000-0000-4000-8001-${String(index).padStart(12, "0")}`,
        HOST_PLAYER,
        targets[index],
        room.version,
        room.game!.turnNumber,
        new Date(
          Date.parse("2026-08-19T00:00:09.000Z") + index * 2_000,
        ).toISOString(),
      );
      if (index < targets.length - 1) {
        fire(
          room,
          GUEST_SESSION,
          `00000000-0000-4000-8002-${String(index).padStart(12, "0")}`,
          GUEST_PLAYER,
          { row: 5 + Math.floor(index / 10), col: index % 10 },
          room.version,
          room.game!.turnNumber,
          new Date(
            Date.parse("2026-08-19T00:00:10.000Z") + index * 2_000,
          ).toISOString(),
        );
      } else {
        expect(result.record.winnerId).toBe(HOST_PLAYER);
      }
    }
    expect(room.status).toBe("FINISHED");
    expect(room.game?.result?.finishReason).toBe("FLEET_DESTROYED");
    expect(
      snapshotFor(room, HOST_SESSION, "2026-08-19T00:01:00.000Z").revealedBoard
        ?.ships,
    ).toHaveLength(5);
    const replay = replayFor(room, HOST_SESSION);
    expect(replay.players.every((player) => player.fleet.length === 5)).toBe(
      true,
    );
    expect(replay.timeline).toHaveLength(33);
    expect(() => replayFor(room, crypto.randomUUID())).toThrowError(
      new DomainError("NOT_ROOM_MEMBER"),
    );
    expect(() =>
      spectatorSnapshot(room, "2026-08-19T00:01:10.000Z"),
    ).toThrowError(new DomainError("ROOM_NOT_FOUND"));
    expect(() =>
      spectatorSnapshot(room, "2026-08-19T00:01:11.000Z"),
    ).toThrowError(new DomainError("ROOM_NOT_FOUND"));
  });

  it("uses salvo shot allowance, turn alarms, and reconnect deadlines without client authority", () => {
    const room = readyRoom("SALVO");
    const attack = fire(
      room,
      HOST_SESSION,
      crypto.randomUUID(),
      HOST_PLAYER,
      { row: 9, col: 9 },
      room.version,
      1,
      "2026-08-19T00:00:09.000Z",
    );
    expect(attack.record.nextPlayerId).toBe(HOST_PLAYER);
    expect(attack.record.shotsRemainingInTurn).toBe(4);
    const deadline = disconnect(
      room,
      GUEST_SESSION,
      90,
      "2026-08-19T00:00:10.000Z",
    );
    expect(deadline).toBe("2026-08-19T00:01:40.000Z");
    const chatCount = room.chatMessages.length;
    expect(
      disconnect(room, GUEST_SESSION, 90, "2026-08-19T00:00:10.500Z"),
    ).toBeNull();
    expect(room.chatMessages).toHaveLength(chatCount);
    expect(reconnect(room, GUEST_SESSION, "2026-08-19T00:00:11.000Z")).toBe(
      true,
    );

    room.game!.turnDeadlineAt = "2026-08-19T00:00:12.000Z";
    const expired = expireTurn(room, "2026-08-19T00:00:12.000Z");
    expect(expired?.expiredPlayerId).toBe(HOST_PLAYER);
    expect(room.game?.currentPlayerId).toBe(GUEST_PLAYER);
  });

  it("resolves tactical patterns server-side with inventory, lock, and idempotency", () => {
    const room = readyRoom("CLASSIC", true);
    expect(() =>
      fireSkill(
        room,
        HOST_SESSION,
        "00000000-0000-4000-8000-000000000301",
        HOST_PLAYER,
        "CROSS_FIRE",
        [{ row: 5, col: 2 }],
        room.version,
        1,
        "2026-08-19T00:00:09.000Z",
      ),
    ).toThrowError(new DomainError("TACTICAL_SKILL_LOCKED"));

    room.game!.turnNumber = 3;
    const requestId = "00000000-0000-4000-8000-000000000302";
    const resolved = fireSkill(
      room,
      HOST_SESSION,
      requestId,
      HOST_PLAYER,
      "CROSS_FIRE",
      [{ row: 5, col: 2 }],
      room.version,
      3,
      "2026-08-19T00:00:10.000Z",
    );
    expect(resolved.record.cells.map((cell) => cell.coordinate)).toEqual([
      { row: 4, col: 2 },
      { row: 5, col: 1 },
      { row: 5, col: 2 },
      { row: 5, col: 3 },
      { row: 6, col: 2 },
    ]);
    expect(resolved.record.remainingUses).toBe(1);
    expect(resolved.record.nextPlayerId).toBe(GUEST_PLAYER);
    expect(room.game?.timeline?.at(-1)?.type).toBe("SKILL_ATTACK");
    expect(
      fireSkill(
        room,
        HOST_SESSION,
        requestId,
        HOST_PLAYER,
        "CROSS_FIRE",
        [{ row: 0, col: 0 }],
        0,
        0,
        "2026-08-19T00:00:11.000Z",
      ).duplicate,
    ).toBe(true);
  });

  it("spends one salvo shell and forbids a second skill in the same turn", () => {
    const room = readyRoom("SALVO", true);
    room.game!.turnNumber = 3;
    const resolved = fireSkill(
      room,
      HOST_SESSION,
      "00000000-0000-4000-8000-000000000303",
      HOST_PLAYER,
      "RAPID_FIRE",
      [
        { row: 8, col: 9 },
        { row: 9, col: 9 },
      ],
      room.version,
      3,
      "2026-08-19T00:00:09.000Z",
    );
    expect(resolved.record.shotsRemainingInTurn).toBe(4);
    expect(resolved.record.nextPlayerId).toBe(HOST_PLAYER);
    expect(() =>
      fireSkill(
        room,
        HOST_SESSION,
        "00000000-0000-4000-8000-000000000304",
        HOST_PLAYER,
        "AREA_ANNIHILATION",
        [{ row: 0, col: 0 }],
        room.version,
        3,
        "2026-08-19T00:00:10.000Z",
      ),
    ).toThrowError(new DomainError("TACTICAL_SKILL_ALREADY_USED"));
  });

  it("persists bounded chat and finishes surrender with the same wire semantics", () => {
    const room = readyRoom();
    const chat = sendChat(
      room,
      HOST_SESSION,
      "00000000-0000-4000-8000-000000000201",
      "TEXT",
      "Hold position\nSector C4",
      null,
      "2026-08-19T00:00:09.000Z",
    );
    expect(chat.message.content).toBe("Hold position\nSector C4");
    expect(
      sendChat(
        room,
        HOST_SESSION,
        chat.message.messageId,
        "TEXT",
        "ignored retry content",
        null,
        "2026-08-19T00:00:10.000Z",
      ).duplicate,
    ).toBe(true);
    const record = surrender(
      room,
      HOST_SESSION,
      HOST_PLAYER,
      "2026-08-19T00:00:11.000Z",
    );
    expect(record.winnerId).toBe(GUEST_PLAYER);
    expect(room.game?.result).toMatchObject({
      winnerId: GUEST_PLAYER,
      loserId: HOST_PLAYER,
      finishReason: "SURRENDER",
      winType: "SURRENDER",
    });
  });
});
