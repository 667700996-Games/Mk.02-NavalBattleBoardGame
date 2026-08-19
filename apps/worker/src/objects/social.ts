import { DurableObject } from "cloudflare:workers";
import { DomainError } from "../domain/protocol";
import {
  bodyObject,
  internalRequest,
  json,
  noContent,
  requireString,
  requireUuid,
} from "../http";
import type { WorkerEnv } from "../env";

type FriendState = "NONE" | "OUTGOING" | "INCOMING" | "FRIEND";
type PartyState =
  "NONE" | "OUTGOING_INVITE" | "INCOMING_INVITE" | "OWNER" | "MEMBER";

interface GameInvite {
  id: string;
  direction: "OUTGOING" | "INCOMING";
  roomId: string;
  roomCode: string;
  roomName: string;
  expiresAt: string;
}

interface Relationship {
  targetIdentityId: string;
  targetNickname: string;
  muted: boolean;
  blocked: boolean;
  friendState: FriendState;
  friendRequestId: string | null;
  partyState: PartyState;
  partyId: string | null;
  gameInvite: GameInvite | null;
  updatedAt: string;
}

interface Privacy {
  allowFriendRequests: boolean;
  showPresence: boolean;
  allowGameInvites: boolean;
  updatedAt: string;
}

interface SocialState {
  privacy: Record<string, Privacy>;
  relationships: Record<string, Record<string, Relationship>>;
}

const EMPTY_STATE: SocialState = { privacy: {}, relationships: {} };

export class SocialDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/overview")
        return json(await this.overview(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/relationships")
        return json({
          relationships: await this.relationships(await bodyObject(request)),
        });
      if (request.method === "POST" && url.pathname === "/privacy")
        return json(await this.updatePrivacy(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/actions")
        return json(await this.applyAction(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/relationship")
        return json(await this.updateRelationship(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/blocked")
        return json(await this.blocked(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/suppressed")
        return json(await this.suppressed(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/export")
        return json(await this.exportData(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/delete")
        return await this.deleteIdentity(await bodyObject(request));
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "PLAYER_BLOCKED"
          ? 403
          : resolved.code === "INTERNAL_ERROR"
            ? 500
            : 400,
      );
    }
  }

  private async overview(input: Record<string, unknown>) {
    const accountId = requireUuid(input.accountId);
    const now = requireString(input.now);
    const state = await this.read();
    pruneInvites(state, now);
    const stored = Object.values(state.relationships[accountId] ?? {});
    const visibleTargets = stored
      .filter(
        (relationship) =>
          relationship.friendState === "FRIEND" &&
          !relationship.blocked &&
          privacyFor(state, relationship.targetIdentityId, now).showPresence,
      )
      .map((relationship) => relationship.targetIdentityId);
    const presence = visibleTargets.length
      ? await this.accountJson<{
          presences: Array<{
            accountId: string;
            presence: "OFFLINE" | "ONLINE" | "IN_GAME";
            currentRoomId: string | null;
          }>;
        }>("/accounts/presence", { accountIds: visibleTargets, now })
      : { presences: [] };
    const relationships = stored.map((relationship) => {
      const targetPresence = presence.presences.find(
        (candidate) => candidate.accountId === relationship.targetIdentityId,
      );
      return {
        ...relationship,
        presence: targetPresence?.presence ?? "OFFLINE",
        currentRoomId: targetPresence?.currentRoomId ?? null,
      };
    });
    const recent = await this.progressionJson<{
      recentPlayers: Array<{
        accountId: string;
        handle: string;
        lastPlayedAt: string;
      }>;
    }>("/recent", { accountId });
    return {
      privacy: privacyFor(state, accountId, now),
      relationships,
      recentPlayers: recent.recentPlayers.map((player) => {
        const relationship = state.relationships[accountId]?.[player.accountId];
        return {
          ...player,
          friend: relationship?.friendState === "FRIEND",
          muted: relationship?.muted ?? false,
          blocked: relationship?.blocked ?? false,
        };
      }),
    };
  }

  private async relationships(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    const now = requireString(input.now);
    const state = await this.read();
    pruneInvites(state, now);
    return Object.values(state.relationships[identityId] ?? {}).map(
      (relationship) => ({
        ...relationship,
        presence: "OFFLINE",
        currentRoomId: null,
      }),
    );
  }

  private async updatePrivacy(input: Record<string, unknown>) {
    const accountId = requireUuid(input.accountId);
    const now = requireString(input.now);
    for (const field of [
      "allowFriendRequests",
      "showPresence",
      "allowGameInvites",
    ]) {
      if (typeof input[field] !== "boolean")
        throw new DomainError("INVALID_REQUEST");
    }
    await this.mutate((state) => {
      state.privacy[accountId] = {
        allowFriendRequests: input.allowFriendRequests as boolean,
        showPresence: input.showPresence as boolean,
        allowGameInvites: input.allowGameInvites as boolean,
        updatedAt: now,
      };
    });
    return this.overview({ accountId, now });
  }

  private async applyAction(input: Record<string, unknown>) {
    const actorId = requireUuid(input.actorId);
    const actorHandle = requireString(input.actorHandle);
    const now = requireString(input.now);
    const action = requireString(input.action);
    let target: { id: string; handle: string };
    if (action === "FRIEND_REQUEST") {
      target = await this.accountJson("/accounts/lookup", {
        handle: requireString(input.targetHandle),
      });
    } else {
      target = await this.accountJson("/accounts/lookup", {
        accountId: requireUuid(input.targetAccountId),
      });
    }
    if (target.id === actorId) throw new DomainError("INVALID_REQUEST");
    let joinCode: string | undefined;
    await this.mutate((state) => {
      pruneInvites(state, now);
      const actor = relationshipFor(
        state,
        actorId,
        target.id,
        target.handle,
        now,
      );
      const reverse = relationshipFor(
        state,
        target.id,
        actorId,
        actorHandle,
        now,
      );
      if (actor.blocked || reverse.blocked)
        throw new DomainError("PLAYER_BLOCKED");
      switch (action) {
        case "FRIEND_REQUEST": {
          if (actor.friendState === "FRIEND") break;
          if (
            !privacyFor(state, target.id, now).allowFriendRequests ||
            actor.friendState !== "NONE" ||
            reverse.friendState !== "NONE"
          ) {
            throw new DomainError("INVALID_STATE");
          }
          const requestId = crypto.randomUUID();
          actor.friendState = "OUTGOING";
          reverse.friendState = "INCOMING";
          actor.friendRequestId = requestId;
          reverse.friendRequestId = requestId;
          break;
        }
        case "FRIEND_RESPOND": {
          const requestId = requireUuid(input.requestId);
          if (
            actor.friendState !== "INCOMING" ||
            reverse.friendState !== "OUTGOING" ||
            actor.friendRequestId !== requestId ||
            reverse.friendRequestId !== requestId
          ) {
            throw new DomainError("INVALID_STATE");
          }
          const accepted = input.accept === true;
          actor.friendState = accepted ? "FRIEND" : "NONE";
          reverse.friendState = actor.friendState;
          actor.friendRequestId = null;
          reverse.friendRequestId = null;
          break;
        }
        case "FRIEND_REMOVE":
          clearSocial(actor);
          clearSocial(reverse);
          break;
        case "PARTY_INVITE": {
          if (
            actor.friendState !== "FRIEND" ||
            reverse.friendState !== "FRIEND" ||
            actor.partyState !== "NONE" ||
            reverse.partyState !== "NONE" ||
            hasParty(state, actorId) ||
            hasParty(state, target.id)
          ) {
            throw new DomainError("INVALID_STATE");
          }
          const partyId = crypto.randomUUID();
          actor.partyState = "OUTGOING_INVITE";
          reverse.partyState = "INCOMING_INVITE";
          actor.partyId = partyId;
          reverse.partyId = partyId;
          break;
        }
        case "PARTY_RESPOND": {
          const partyId = requireUuid(input.partyId);
          if (
            actor.partyState !== "INCOMING_INVITE" ||
            reverse.partyState !== "OUTGOING_INVITE" ||
            actor.partyId !== partyId ||
            reverse.partyId !== partyId
          ) {
            throw new DomainError("INVALID_STATE");
          }
          if (input.accept === true) {
            actor.partyState = "MEMBER";
            reverse.partyState = "OWNER";
          } else {
            clearParty(actor);
            clearParty(reverse);
          }
          break;
        }
        case "PARTY_LEAVE":
          if (actor.partyState === "NONE")
            throw new DomainError("INVALID_STATE");
          clearParty(actor);
          clearParty(reverse);
          break;
        case "GAME_INVITE": {
          if (
            actor.friendState !== "FRIEND" ||
            !privacyFor(state, target.id, now).allowGameInvites
          ) {
            throw new DomainError("INVALID_STATE");
          }
          const room = roomInfoFrom(input.roomInfo);
          if (room.roomId !== requireUuid(input.roomId))
            throw new DomainError("INVALID_REQUEST");
          const inviteId = crypto.randomUUID();
          const expiresAt = new Date(
            Date.parse(now) + 15 * 60 * 1_000,
          ).toISOString();
          actor.gameInvite = {
            id: inviteId,
            direction: "OUTGOING",
            ...room,
            expiresAt,
          };
          reverse.gameInvite = {
            id: inviteId,
            direction: "INCOMING",
            ...room,
            expiresAt,
          };
          break;
        }
        case "GAME_INVITE_RESPOND": {
          const inviteId = requireUuid(input.inviteId);
          if (
            actor.gameInvite?.id !== inviteId ||
            actor.gameInvite.direction !== "INCOMING" ||
            reverse.gameInvite?.id !== inviteId ||
            reverse.gameInvite.direction !== "OUTGOING"
          ) {
            throw new DomainError("INVALID_STATE");
          }
          if (input.accept === true) joinCode = actor.gameInvite.roomCode;
          actor.gameInvite = null;
          reverse.gameInvite = null;
          break;
        }
        default:
          throw new DomainError("INVALID_REQUEST");
      }
      actor.updatedAt = now;
      reverse.updatedAt = now;
    });
    return {
      overview: await this.overview({ accountId: actorId, now }),
      ...(joinCode ? { joinCode } : {}),
    };
  }

  private async updateRelationship(input: Record<string, unknown>) {
    const actorId = requireUuid(input.actorIdentityId);
    const targetId = requireUuid(input.targetIdentityId);
    const actorNickname = requireString(input.actorNickname);
    const targetNickname = requireString(input.targetNickname);
    const now = requireString(input.now);
    if (
      actorId === targetId ||
      typeof input.muted !== "boolean" ||
      typeof input.blocked !== "boolean"
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    let result: Relationship | undefined;
    await this.mutate((state) => {
      const relationship = relationshipFor(
        state,
        actorId,
        targetId,
        targetNickname,
        now,
      );
      relationship.muted = input.muted as boolean;
      relationship.blocked = input.blocked as boolean;
      relationship.updatedAt = now;
      if (relationship.blocked) {
        clearSocial(relationship);
        const reverse = relationshipFor(
          state,
          targetId,
          actorId,
          actorNickname,
          now,
        );
        clearSocial(reverse);
        reverse.updatedAt = now;
      }
      result = structuredClone(relationship);
    });
    if (!result) throw new DomainError("INTERNAL_ERROR");
    return {
      ...(result as Relationship),
      presence: "OFFLINE",
      currentRoomId: null,
    };
  }

  private async blocked(input: Record<string, unknown>) {
    const first = requireUuid(input.firstIdentityId);
    const second = requireUuid(input.secondIdentityId);
    const state = await this.read();
    return {
      blocked:
        Boolean(state.relationships[first]?.[second]?.blocked) ||
        Boolean(state.relationships[second]?.[first]?.blocked),
    };
  }

  private async suppressed(input: Record<string, unknown>) {
    const recipient = requireUuid(input.recipientIdentityId);
    const sender = requireUuid(input.senderIdentityId);
    const state = await this.read();
    return {
      suppressed:
        Boolean(state.relationships[recipient]?.[sender]?.muted) ||
        Boolean(state.relationships[recipient]?.[sender]?.blocked) ||
        Boolean(state.relationships[sender]?.[recipient]?.blocked),
    };
  }

  private async exportData(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    const state = await this.read();
    return {
      relationships: Object.values(state.relationships[identityId] ?? {}),
      privacy: state.privacy[identityId] ?? null,
    };
  }

  private async deleteIdentity(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    let deleted = 0;
    await this.mutate((state) => {
      deleted += Object.keys(state.relationships[identityId] ?? {}).length;
      delete state.relationships[identityId];
      delete state.privacy[identityId];
      for (const relationships of Object.values(state.relationships)) {
        if (relationships[identityId]) {
          delete relationships[identityId];
          deleted += 1;
        }
      }
    });
    return json({ relationshipsDeleted: deleted });
  }

  private async accountJson<T = { id: string; handle: string }>(
    path: string,
    body: unknown,
  ): Promise<T> {
    const object = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    return fetchJson<T>(object.fetch(internalRequest(path, body)));
  }

  private async progressionJson<T>(path: string, body: unknown): Promise<T> {
    const object = this.env.PROGRESSION.get(
      this.env.PROGRESSION.idFromName("global-v1"),
    );
    return fetchJson<T>(object.fetch(internalRequest(path, body)));
  }

  private async read(): Promise<SocialState> {
    const state =
      (await this.ctx.storage.get<SocialState>("state")) ??
      structuredClone(EMPTY_STATE);
    state.privacy ??= {};
    state.relationships ??= {};
    return state;
  }

  private async mutate(action: (state: SocialState) => void): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<SocialState>("state")) ??
        structuredClone(EMPTY_STATE);
      state.privacy ??= {};
      state.relationships ??= {};
      action(state);
      await transaction.put("state", state);
    });
  }
}

function relationshipFor(
  state: SocialState,
  ownerId: string,
  targetId: string,
  targetNickname: string,
  now: string,
): Relationship {
  state.relationships[ownerId] ??= {};
  state.relationships[ownerId][targetId] ??= {
    targetIdentityId: targetId,
    targetNickname,
    muted: false,
    blocked: false,
    friendState: "NONE",
    friendRequestId: null,
    partyState: "NONE",
    partyId: null,
    gameInvite: null,
    updatedAt: now,
  };
  state.relationships[ownerId][targetId].targetNickname = targetNickname;
  return state.relationships[ownerId][targetId];
}

function privacyFor(
  state: SocialState,
  accountId: string,
  now: string,
): Privacy {
  return (
    state.privacy[accountId] ?? {
      allowFriendRequests: true,
      showPresence: true,
      allowGameInvites: true,
      updatedAt: now,
    }
  );
}

function clearParty(relationship: Relationship): void {
  relationship.partyState = "NONE";
  relationship.partyId = null;
}

function clearSocial(relationship: Relationship): void {
  relationship.friendState = "NONE";
  relationship.friendRequestId = null;
  clearParty(relationship);
  relationship.gameInvite = null;
}

function hasParty(state: SocialState, ownerId: string): boolean {
  return Object.values(state.relationships[ownerId] ?? {}).some(
    (relationship) => relationship.partyState !== "NONE",
  );
}

function pruneInvites(state: SocialState, now: string): void {
  for (const relationships of Object.values(state.relationships)) {
    for (const relationship of Object.values(relationships)) {
      if (relationship.gameInvite && relationship.gameInvite.expiresAt <= now) {
        relationship.gameInvite = null;
      }
    }
  }
}

function roomInfoFrom(value: unknown): {
  roomId: string;
  roomCode: string;
  roomName: string;
} {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  const room = value as Record<string, unknown>;
  return {
    roomId: requireUuid(room.roomId),
    roomCode: requireString(room.roomCode),
    roomName: requireString(room.roomName),
  };
}

async function fetchJson<T>(responsePromise: Promise<Response>): Promise<T> {
  const response = await responsePromise;
  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as {
      code?: string;
    } | null;
    if (payload?.code === "INVALID_REQUEST")
      throw new DomainError("INVALID_REQUEST");
    throw new DomainError("INTERNAL_ERROR");
  }
  return response.json() as Promise<T>;
}
