import { DurableObject } from "cloudflare:workers";
import {
  DomainError,
  type MatchmakingPool,
  type MatchmakingQuality,
  type MatchmakingRegion,
  type MatchmakingSearchWindow,
  type MatchmakingTicket,
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

interface Criteria {
  pool: MatchmakingPool;
  region: MatchmakingRegion;
  latencyMs: number;
  rating: number | null;
  seasonId: string | null;
  contentRevision: number | null;
  partyId: string;
}

interface QueueEntry {
  session: SessionRecord;
  criteria: Criteria;
  queuedAt: string;
}

interface MatchReference {
  roomId: string;
  createdAt: string;
}

interface MatchmakingState {
  queue: Record<string, QueueEntry>;
  matches: Record<string, MatchReference>;
  recentPairings: Record<string, string[]>;
}

const EMPTY_STATE: MatchmakingState = {
  queue: {},
  matches: {},
  recentPairings: {},
};
const QUEUE_TTL_MS = 10 * 60 * 1_000;
const MATCH_REFERENCE_TTL_MS = 10 * 60 * 1_000;
const RECENT_PAIRING_TTL_MS = 30 * 60 * 1_000;

export class MatchmakingDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/enqueue")
        return await this.enqueue(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/cancel")
        return await this.cancel(await bodyObject(request));
      if (request.method === "GET" && url.pathname === "/stats")
        return await this.stats();
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

  private async enqueue(input: Record<string, unknown>): Promise<Response> {
    const session = sessionFrom(input.session);
    const now = requireString(input.now);
    const nowMs = Date.parse(now);
    if (!Number.isFinite(nowMs)) throw new DomainError("INVALID_REQUEST");
    const criteria = criteriaFrom(input, session);
    const state = await this.read();
    prune(state, nowMs);

    const matched = state.matches[session.id];
    if (matched) {
      const snapshot = await this.snapshot(matched.roomId, session.id, now);
      delete state.matches[session.id];
      await this.write(state);
      return json(responseFor(criteria, null, null, snapshot, now));
    }

    const existing = state.queue[session.id];
    if (existing && !sameCriteria(existing.criteria, criteria)) {
      throw new DomainError("INVALID_STATE");
    }
    const own: QueueEntry = existing ?? { session, criteria, queuedAt: now };
    own.session = session;
    const unblocked = await Promise.all(
      Object.values(state.queue).map(async (candidate) => ({
        candidate,
        blocked: await this.pairBlocked(
          own.criteria.partyId,
          candidate.criteria.partyId,
        ),
      })),
    );
    const candidates = unblocked
      .filter(({ blocked }) => !blocked)
      .map(({ candidate }) => candidate)
      .filter(
        (candidate) =>
          candidate.session.id !== session.id &&
          candidate.session.nickname.toLocaleLowerCase() !==
            session.nickname.toLocaleLowerCase(),
      )
      .map((candidate) => {
        const pairKey = pairingKey(
          own.criteria.partyId,
          candidate.criteria.partyId,
        );
        const recentPairings = (state.recentPairings[pairKey] ?? []).length;
        return {
          candidate,
          quality: qualityFor(own, candidate, nowMs, recentPairings),
        };
      })
      .filter(
        (
          candidate,
        ): candidate is {
          candidate: QueueEntry;
          quality: MatchmakingQuality;
        } => candidate.quality !== null,
      )
      .sort(
        (left, right) =>
          rematchPriority(left.quality) - rematchPriority(right.quality) ||
          left.quality.ratingDelta - right.quality.ratingDelta ||
          left.candidate.queuedAt.localeCompare(right.candidate.queuedAt),
      );

    const selected = candidates[0];
    if (!selected) {
      state.queue[session.id] = own;
      await this.write(state);
      return json(responseFor(criteria, own.queuedAt, null, null, now));
    }

    const room = await this.createMatch(
      selected.candidate.session,
      session,
      selected.quality,
      criteria,
      now,
    );
    delete state.queue[selected.candidate.session.id];
    delete state.queue[session.id];
    state.matches[selected.candidate.session.id] = {
      roomId: room.roomId,
      createdAt: now,
    };
    const pairKey = pairingKey(
      own.criteria.partyId,
      selected.candidate.criteria.partyId,
    );
    state.recentPairings[pairKey] ??= [];
    state.recentPairings[pairKey].push(now);
    await this.write(state);
    return json(
      responseFor(criteria, null, selected.quality, room.snapshot, now),
    );
  }

  private async cancel(input: Record<string, unknown>): Promise<Response> {
    const sessionId = requireUuid(input.sessionId);
    const state = await this.read();
    delete state.queue[sessionId];
    await this.write(state);
    return noContent();
  }

  private async stats(): Promise<Response> {
    const state = await this.read();
    const now = Date.now();
    prune(state, now);
    await this.write(state);
    const entries = Object.values(state.queue);
    return json({
      queued: entries.length,
      rankedQueued: entries.filter((entry) => entry.criteria.pool === "RANKED")
        .length,
      oldestWaitSeconds: entries.length
        ? Math.max(
            0,
            Math.floor(
              (now -
                Math.min(
                  ...entries.map((entry) => Date.parse(entry.queuedAt)),
                )) /
                1_000,
            ),
          )
        : 0,
    });
  }

  private async createMatch(
    host: SessionRecord,
    guest: SessionRecord,
    quality: MatchmakingQuality,
    criteria: Criteria,
    now: string,
  ): Promise<{ roomId: string; snapshot: unknown }> {
    const roomId = crypto.randomUUID();
    let created = false;
    for (let attempt = 0; attempt < 10 && !created; attempt += 1) {
      const response = await this.room(roomId).fetch(
        internalRequest("/create", {
          roomId,
          code: inviteCode(),
          name: criteria.pool === "RANKED" ? "랭크 교전" : "신속 교전",
          visibility: "PRIVATE",
          session: host,
          playerId: crypto.randomUUID(),
          now,
        }),
      );
      if (response.status === 201) created = true;
      else if (response.status !== 409) throw new DomainError("INTERNAL_ERROR");
    }
    if (!created) throw new DomainError("INTERNAL_ERROR");
    const joined = await this.room(roomId).fetch(
      internalRequest("/join", {
        session: guest,
        playerId: crypto.randomUUID(),
        now,
      }),
    );
    if (!joined.ok) throw await responseError(joined);
    const joinPayload = (await joined.json()) as { snapshot: unknown };
    const metadata = await this.room(roomId).fetch(
      internalRequest("/matchmaking", {
        quality,
        rankedMatch:
          criteria.pool === "RANKED"
            ? {
                seasonId: criteria.seasonId,
                contentRevision: criteria.contentRevision,
              }
            : null,
        now,
      }),
    );
    if (metadata.status !== 204) throw await responseError(metadata);
    const accounts = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName("global-v1"),
    );
    for (const session of [host, guest]) {
      const response = await accounts.fetch(
        internalRequest("/sessions/room-by-id", {
          sessionId: session.id,
          roomId,
        }),
      );
      if (response.status !== 204) throw await responseError(response);
    }
    return {
      roomId,
      snapshot: await this.snapshot(roomId, guest.id, now),
    };
  }

  private async snapshot(
    roomId: string,
    sessionId: string,
    now: string,
  ): Promise<unknown> {
    const response = await this.room(roomId).fetch(
      internalRequest("/snapshot", { sessionId, now }),
    );
    if (!response.ok) throw await responseError(response);
    return response.json();
  }

  private room(roomId: string) {
    return this.env.GAME_ROOMS.get(this.env.GAME_ROOMS.idFromName(roomId));
  }

  private async pairBlocked(firstIdentityId: string, secondIdentityId: string) {
    if (firstIdentityId === secondIdentityId) return true;
    const safety = this.env.SAFETY.get(this.env.SAFETY.idFromName("global-v1"));
    const response = await safety.fetch(
      internalRequest("/blocked", { firstIdentityId, secondIdentityId }),
    );
    if (!response.ok) throw new DomainError("INTERNAL_ERROR");
    return ((await response.json()) as { blocked: boolean }).blocked;
  }

  private async read(): Promise<MatchmakingState> {
    const state =
      (await this.ctx.storage.get<MatchmakingState>("state")) ??
      structuredClone(EMPTY_STATE);
    state.queue ??= {};
    state.matches ??= {};
    state.recentPairings ??= {};
    return state;
  }

  private async write(state: MatchmakingState): Promise<void> {
    await this.ctx.storage.put("state", state);
  }
}

function criteriaFrom(
  input: Record<string, unknown>,
  session: SessionRecord,
): Criteria {
  const pool = input.pool === "RANKED" ? "RANKED" : "CASUAL";
  if (pool === "CASUAL") {
    if (
      input.region !== "AUTO" ||
      Number(input.latencyMs) !== 0 ||
      input.rating !== null ||
      input.seasonId !== null ||
      input.contentRevision !== null
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    return {
      pool,
      region: "AUTO",
      latencyMs: 0,
      rating: null,
      seasonId: null,
      contentRevision: null,
      partyId: session.accountId ?? session.id,
    };
  }
  if (!session.accountId) throw new DomainError("RANKED_ACCOUNT_REQUIRED");
  const region = requireString(input.region) as MatchmakingRegion;
  const latencyMs = Number(input.latencyMs);
  const rating = Number(input.rating);
  const contentRevision = Number(input.contentRevision);
  if (
    ![
      "KOREA",
      "JAPAN",
      "SOUTHEAST_ASIA",
      "NORTH_AMERICA_WEST",
      "NORTH_AMERICA_EAST",
      "EUROPE",
    ].includes(region) ||
    !Number.isInteger(latencyMs) ||
    latencyMs < 1 ||
    latencyMs > 300 ||
    !Number.isInteger(rating) ||
    rating < 0 ||
    rating > 4_000 ||
    !Number.isInteger(contentRevision) ||
    contentRevision < 0
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  return {
    pool,
    region,
    latencyMs,
    rating,
    seasonId: requireString(input.seasonId),
    contentRevision,
    partyId: session.accountId,
  };
}

function qualityFor(
  first: QueueEntry,
  second: QueueEntry,
  now: number,
  recentPairings: number,
): MatchmakingQuality | null {
  if (
    first.criteria.pool !== second.criteria.pool ||
    first.criteria.partyId === second.criteria.partyId
  ) {
    return null;
  }
  if (first.criteria.pool === "CASUAL") {
    return {
      pool: "CASUAL",
      phase: "EXACT",
      ratingDelta: 0,
      maxReportedLatencyMs: 0,
      partySize: 1,
      recentPairings: 0,
      rematchRelaxed: false,
      sharedWaitSeconds: 0,
      waitSkewSeconds: 0,
    };
  }
  if (first.criteria.seasonId !== second.criteria.seasonId) return null;
  const firstWindow = searchWindow(first.queuedAt, now);
  const secondWindow = searchWindow(second.queuedAt, now);
  const sharedWaitSeconds = Math.min(
    firstWindow.elapsedSeconds,
    secondWindow.elapsedSeconds,
  );
  if (recentPairings > 0 && sharedWaitSeconds < 90) return null;
  if (
    first.criteria.latencyMs > firstWindow.maxLatencyMs ||
    second.criteria.latencyMs > secondWindow.maxLatencyMs
  ) {
    return null;
  }
  const ratingDelta = Math.abs(
    (first.criteria.rating ?? 0) - (second.criteria.rating ?? 0),
  );
  if (ratingDelta > Math.min(firstWindow.ratingDelta, secondWindow.ratingDelta))
    return null;
  const firstPhase = phaseValue(firstWindow.phase);
  const secondPhase = phaseValue(secondWindow.phase);
  const sameRegion = first.criteria.region === second.criteria.region;
  const sameGroup =
    regionGroup(first.criteria.region) !== null &&
    regionGroup(first.criteria.region) === regionGroup(second.criteria.region);
  if (
    !sameRegion &&
    !(sameGroup && firstPhase >= 1 && secondPhase >= 1) &&
    !(firstPhase === 2 && secondPhase === 2)
  ) {
    return null;
  }
  return {
    pool: "RANKED",
    phase: firstPhase <= secondPhase ? firstWindow.phase : secondWindow.phase,
    ratingDelta,
    maxReportedLatencyMs: Math.max(
      first.criteria.latencyMs,
      second.criteria.latencyMs,
    ),
    partySize: 1,
    recentPairings,
    rematchRelaxed: recentPairings > 0,
    sharedWaitSeconds,
    waitSkewSeconds: Math.abs(
      firstWindow.elapsedSeconds - secondWindow.elapsedSeconds,
    ),
  };
}

function responseFor(
  criteria: Criteria,
  queuedAt: string | null,
  quality: MatchmakingQuality | null,
  snapshot: unknown | null,
  now: string,
) {
  const ticket: MatchmakingTicket = {
    pool: criteria.pool,
    region: criteria.region,
    reportedLatencyMs: criteria.latencyMs,
    rating: criteria.rating,
    partySize: 1,
    searchWindow: searchWindow(queuedAt ?? now, Date.parse(now)),
  };
  return {
    queued: snapshot === null,
    queuedAt,
    ticket,
    matchQuality: quality,
    snapshot,
  };
}

function searchWindow(queuedAt: string, now: number): MatchmakingSearchWindow {
  const elapsedSeconds = Math.max(
    0,
    Math.floor((now - Date.parse(queuedAt)) / 1_000),
  );
  if (elapsedSeconds <= 29)
    return {
      phase: "EXACT",
      ratingDelta: 100,
      maxLatencyMs: 120,
      elapsedSeconds,
    };
  if (elapsedSeconds <= 89)
    return {
      phase: "REGIONAL",
      ratingDelta: 250,
      maxLatencyMs: 200,
      elapsedSeconds,
    };
  return {
    phase: "GLOBAL",
    ratingDelta: 500,
    maxLatencyMs: 300,
    elapsedSeconds,
  };
}

function regionGroup(region: MatchmakingRegion): string | null {
  if (["KOREA", "JAPAN", "SOUTHEAST_ASIA"].includes(region)) return "APAC";
  if (["NORTH_AMERICA_WEST", "NORTH_AMERICA_EAST"].includes(region))
    return "NA";
  if (region === "EUROPE") return "EU";
  return null;
}

function phaseValue(phase: MatchmakingSearchWindow["phase"]): number {
  return phase === "EXACT" ? 0 : phase === "REGIONAL" ? 1 : 2;
}

function rematchPriority(quality: MatchmakingQuality): number {
  return quality.sharedWaitSeconds >= 180 ? 0 : quality.recentPairings;
}

function pairingKey(first: string, second: string): string {
  return [first, second].sort().join(":");
}

function sameCriteria(first: Criteria, second: Criteria): boolean {
  return JSON.stringify(first) === JSON.stringify(second);
}

function prune(state: MatchmakingState, now: number): void {
  for (const [sessionId, entry] of Object.entries(state.queue)) {
    if (now - Date.parse(entry.queuedAt) >= QUEUE_TTL_MS)
      delete state.queue[sessionId];
  }
  for (const [sessionId, match] of Object.entries(state.matches)) {
    if (now - Date.parse(match.createdAt) >= MATCH_REFERENCE_TTL_MS)
      delete state.matches[sessionId];
  }
  for (const [key, timestamps] of Object.entries(state.recentPairings)) {
    state.recentPairings[key] = timestamps.filter(
      (timestamp) => now - Date.parse(timestamp) < RECENT_PAIRING_TTL_MS,
    );
    if (!state.recentPairings[key].length) delete state.recentPairings[key];
  }
}

function sessionFrom(value: unknown): SessionRecord {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  const session = value as SessionRecord;
  requireUuid(session.id);
  if (session.accountId !== null) requireUuid(session.accountId);
  requireString(session.nickname);
  return structuredClone(session);
}

async function responseError(response: Response): Promise<DomainError> {
  const payload = (await response.json().catch(() => null)) as {
    code?: string;
  } | null;
  return new DomainError(
    payload?.code && isErrorCode(payload.code)
      ? (payload.code as ConstructorParameters<typeof DomainError>[0])
      : "INTERNAL_ERROR",
  );
}

function isErrorCode(value: string): boolean {
  return [
    "INVALID_STATE",
    "INVALID_REQUEST",
    "DUPLICATE_NICKNAME",
    "INTERNAL_ERROR",
  ].includes(value);
}

function inviteCode(): string {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  const bytes = crypto.getRandomValues(new Uint8Array(6));
  return [...bytes].map((byte) => alphabet[byte % alphabet.length]).join("");
}
