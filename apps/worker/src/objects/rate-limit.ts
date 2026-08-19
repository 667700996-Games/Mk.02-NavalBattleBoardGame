import { DurableObject } from "cloudflare:workers";
import { DomainError } from "../domain/protocol";
import { bodyObject, json, noContent } from "../http";
import type { WorkerEnv } from "../env";

const WINDOW_KEY = "window-v1";

export class EdgeRateLimitDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      if (
        request.method !== "POST" ||
        new URL(request.url).pathname !== "/check"
      ) {
        return json({ code: "NOT_FOUND" }, 404);
      }
      const input = await bodyObject(request);
      const limit = integer(input.limit, 1, 10_000);
      const windowMs = integer(input.windowMs, 1_000, 3_600_000);
      const now = integer(input.now, 0, Number.MAX_SAFE_INTEGER);
      const previous = (await this.ctx.storage.get<number[]>(WINDOW_KEY)) ?? [];
      const active = previous.filter((timestamp) => now - timestamp < windowMs);
      if (active.length >= limit) throw new DomainError("RATE_LIMITED");
      active.push(now);
      await this.ctx.storage.put(WINDOW_KEY, active);
      await this.ctx.storage.setAlarm(now + windowMs);
      return noContent();
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INVALID_REQUEST");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "RATE_LIMITED" ? 429 : 400,
      );
    }
  }

  async alarm(): Promise<void> {
    await this.ctx.storage.delete(WINDOW_KEY);
  }
}

function integer(value: unknown, minimum: number, maximum: number): number {
  if (
    !Number.isSafeInteger(value) ||
    (value as number) < minimum ||
    (value as number) > maximum
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  return value as number;
}
