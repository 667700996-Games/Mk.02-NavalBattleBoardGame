import { DurableObject } from "cloudflare:workers";
import { DomainError } from "../domain/protocol";
import { bodyObject, json, requireString, requireUuid } from "../http";
import type { WorkerEnv } from "../env";

interface SafetyRelationship {
  targetIdentityId: string;
  targetNickname: string;
  muted: boolean;
  blocked: boolean;
  updatedAt: string;
}

interface SafetyState {
  relationships: Record<string, Record<string, SafetyRelationship>>;
}

const EMPTY_STATE: SafetyState = { relationships: {} };

export class SafetyDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/relationships") {
        return json({
          relationships: await this.relationships(await bodyObject(request)),
        });
      }
      if (request.method === "POST" && url.pathname === "/relationship") {
        return json(await this.updateRelationship(await bodyObject(request)));
      }
      if (request.method === "POST" && url.pathname === "/blocked") {
        return json(await this.blocked(await bodyObject(request)));
      }
      if (request.method === "POST" && url.pathname === "/suppressed") {
        return json(await this.suppressed(await bodyObject(request)));
      }
      if (request.method === "POST" && url.pathname === "/export") {
        return json(await this.exportData(await bodyObject(request)));
      }
      if (request.method === "POST" && url.pathname === "/delete") {
        return this.deleteIdentity(await bodyObject(request));
      }
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "INTERNAL_ERROR" ? 500 : 400,
      );
    }
  }

  private async relationships(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    const state = await this.read();
    return Object.values(state.relationships[identityId] ?? {});
  }

  private async updateRelationship(input: Record<string, unknown>) {
    const actorId = requireUuid(input.actorIdentityId);
    const targetId = requireUuid(input.targetIdentityId);
    const targetNickname = requireString(input.targetNickname);
    const now = requireString(input.now);
    if (
      actorId === targetId ||
      typeof input.muted !== "boolean" ||
      typeof input.blocked !== "boolean"
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    let result: SafetyRelationship | undefined;
    await this.mutate((state) => {
      state.relationships[actorId] ??= {};
      const key = state.relationships[actorId];
      if (input.muted || input.blocked) {
        result = {
          targetIdentityId: targetId,
          targetNickname,
          muted: input.muted as boolean,
          blocked: input.blocked as boolean,
          updatedAt: now,
        };
        key[targetId] = result;
      } else {
        delete key[targetId];
        result = {
          targetIdentityId: targetId,
          targetNickname,
          muted: false,
          blocked: false,
          updatedAt: now,
        };
      }
      if (Object.keys(key).length === 0) delete state.relationships[actorId];
    });
    if (!result) throw new DomainError("INTERNAL_ERROR");
    return result;
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
      for (const [ownerId, relationships] of Object.entries(
        state.relationships,
      )) {
        if (relationships[identityId]) {
          delete relationships[identityId];
          deleted += 1;
        }
        if (Object.keys(relationships).length === 0) {
          delete state.relationships[ownerId];
        }
      }
    });
    return json({ relationshipsDeleted: deleted });
  }

  private async read(): Promise<SafetyState> {
    const state =
      (await this.ctx.storage.get<SafetyState>("state")) ??
      structuredClone(EMPTY_STATE);
    state.relationships ??= {};
    return state;
  }

  private async mutate(action: (state: SafetyState) => void): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<SafetyState>("state")) ??
        structuredClone(EMPTY_STATE);
      state.relationships ??= {};
      action(state);
      await transaction.put("state", state);
    });
  }
}
