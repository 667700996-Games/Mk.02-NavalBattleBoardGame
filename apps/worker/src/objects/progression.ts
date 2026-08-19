import { DurableObject } from "cloudflare:workers";
import {
  DomainError,
  type GameResult,
  type HistoryItem,
} from "../domain/protocol";
import {
  applyInactivityDecay,
  newRankedStanding,
  nextSeasonSeed,
  rankedProfile as profileForStanding,
  rankedTier as tier,
  recordRankedResult,
  RANKED_PLACEMENT_MATCHES,
  seasonRewardXp,
  type RankedStanding,
} from "../domain/ranked";
import {
  bodyObject,
  json,
  noContent,
  requireString,
  requireUuid,
} from "../http";
import type { WorkerEnv } from "../env";

interface ResultParticipant {
  identityId: string;
  accountId: string | null;
  playerId: string;
  handle: string;
}

interface RankedReward {
  sourceKind: "RANKED_MATCH" | "RANKED_PLACEMENT" | "RANKED_SEASON";
  sourceId: string;
  seasonId: string;
  xp: number;
  createdAt: string;
}

interface RankedMatchResult {
  roomId: string;
  accountId: string;
  seasonId: string;
  ratingBefore: number;
  ratingAfter: number;
  delta: number;
  won: boolean;
  finishedAt: string;
}

interface MissionReward {
  missionId: string;
  periodKey: string;
  xp: number;
  claimedAt: string;
}

interface ProgressionState {
  history: Record<string, Record<string, HistoryItem>>;
  handles: Record<string, string>;
  rewards: Record<string, Record<string, MissionReward>>;
  ranked: Record<string, RankedStanding>;
  leaderboardVisible: Record<string, boolean>;
  settledRankedRooms: Record<string, true | string[]>;
  rankedRewards: Record<string, RankedReward>;
  rankedMatchResults: Record<string, RankedMatchResult>;
  recentPlayers: Record<
    string,
    Record<string, { accountId: string; handle: string; lastPlayedAt: string }>
  >;
}

const EMPTY_STATE: ProgressionState = {
  history: {},
  handles: {},
  rewards: {},
  ranked: {},
  leaderboardVisible: {},
  settledRankedRooms: {},
  rankedRewards: {},
  rankedMatchResults: {},
  recentPlayers: {},
};
const CURRENT_SEASON = "FOUNDERS_SEASON";

export class ProgressionDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/results/record")
        return await this.recordResult(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/identities/migrate")
        return await this.migrateIdentity(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/history")
        return await this.history(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/recent")
        return await this.recent(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/profile")
        return await this.profile(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/missions/claim")
        return await this.claimMission(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/ranked/profile")
        return await this.rankedProfile(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/ranked/leaderboard")
        return await this.rankedLeaderboard(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/ranked/visibility")
        return await this.setVisibility(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/export")
        return await this.exportData(await bodyObject(request));
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
        resolved.code === "UNAUTHORIZED"
          ? 401
          : resolved.code === "INTERNAL_ERROR"
            ? 500
            : 400,
      );
    }
  }

  private async recordResult(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const roomId = requireUuid(input.roomId);
    const roomName = requireString(input.roomName);
    const result = resultFrom(input.result);
    const participants = participantsFrom(input.participants);
    const rankedMatch = rankedMatchFrom(input.rankedMatch);
    await this.mutate((state) => {
      for (const participant of participants) {
        state.history[participant.identityId] ??= {};
        state.history[participant.identityId][roomId] ??= {
          roomId,
          roomName,
          selfPlayerId: participant.playerId,
          balance: input.balance as HistoryItem["balance"],
          result,
        };
        state.handles[participant.identityId] = participant.handle;
        if (participant.accountId)
          state.handles[participant.accountId] = participant.handle;
      }
      if (
        rankedMatch &&
        !state.settledRankedRooms[roomId] &&
        participants.length === 2 &&
        participants.every((participant) => participant.accountId)
      ) {
        settleRanked(state, roomId, participants, result, rankedMatch.seasonId);
        state.settledRankedRooms[roomId] = participants.map(
          (participant) => participant.accountId!,
        );
      }
      if (
        participants.length === 2 &&
        participants.every((participant) => participant.accountId)
      ) {
        const [first, second] = participants;
        if (first?.accountId && second?.accountId) {
          state.recentPlayers[first.accountId] ??= {};
          state.recentPlayers[second.accountId] ??= {};
          state.recentPlayers[first.accountId][second.accountId] = {
            accountId: second.accountId,
            handle: second.handle,
            lastPlayedAt: result.finishedAt,
          };
          state.recentPlayers[second.accountId][first.accountId] = {
            accountId: first.accountId,
            handle: first.handle,
            lastPlayedAt: result.finishedAt,
          };
        }
      }
    });
    return noContent();
  }

  private async migrateIdentity(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const sourceId = requireUuid(input.sourceId);
    const accountId = requireUuid(input.accountId);
    const handle = requireString(input.handle);
    await this.mutate((state) => {
      const sourceHistory = state.history[sourceId] ?? {};
      state.history[accountId] = {
        ...sourceHistory,
        ...(state.history[accountId] ?? {}),
      };
      delete state.history[sourceId];
      const sourceRewards = state.rewards[sourceId] ?? {};
      state.rewards[accountId] = {
        ...sourceRewards,
        ...(state.rewards[accountId] ?? {}),
      };
      delete state.rewards[sourceId];
      delete state.handles[sourceId];
      state.handles[accountId] = handle;
    });
    return noContent();
  }

  private async history(input: Record<string, unknown>): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    const state = await this.read();
    return json({
      games: Object.values(state.history[identityId] ?? {})
        .sort((left, right) =>
          right.result.finishedAt.localeCompare(left.result.finishedAt),
        )
        .slice(0, 50),
    });
  }

  private async recent(input: Record<string, unknown>): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    const state = await this.read();
    return json({
      recentPlayers: Object.values(state.recentPlayers[accountId] ?? {})
        .sort((left, right) =>
          right.lastPlayedAt.localeCompare(left.lastPlayedAt),
        )
        .slice(0, 20),
    });
  }

  private async profile(input: Record<string, unknown>): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    const accountId =
      input.accountId === null ? null : requireUuid(input.accountId);
    const handle = requireString(input.handle);
    const now = requireString(input.now);
    const runtime = runtimeFrom(input);
    let response: unknown = null;
    await this.mutate((state) => {
      if (accountId)
        prepareRankedStanding(
          state,
          accountId,
          handle,
          runtime.liveContent.season.id,
          runtime.liveContent.season.startsAt,
          now,
        );
      response = buildProgression(
        state,
        identityId,
        accountId,
        handle,
        now,
        runtime,
      );
    });
    if (!response) throw new DomainError("INTERNAL_ERROR");
    return json(response);
  }

  private async claimMission(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    const accountId = requireUuid(input.accountId);
    const handle = requireString(input.handle);
    const missionId = requireString(input.missionId);
    const now = requireString(input.now);
    const runtime = runtimeFrom(input);
    let response: unknown = null;
    await this.mutate((state) => {
      prepareRankedStanding(
        state,
        accountId,
        handle,
        runtime.liveContent.season.id,
        runtime.liveContent.season.startsAt,
        now,
      );
      const progression = buildProgression(
        state,
        identityId,
        accountId,
        handle,
        now,
        runtime,
      );
      const mission = progression.missions.find(
        (candidate) => candidate.id === missionId,
      );
      if (!mission) throw new DomainError("INVALID_REQUEST");
      if (!mission.completed) throw new DomainError("INVALID_STATE");
      const periodKey = missionPeriodKey(mission.cadence, now);
      const rewardKey = `${missionId}:${periodKey}`;
      state.rewards[accountId] ??= {};
      state.rewards[accountId][rewardKey] ??= {
        missionId,
        periodKey,
        xp: mission.rewardXp,
        claimedAt: now,
      };
      response = buildProgression(
        state,
        identityId,
        accountId,
        handle,
        now,
        runtime,
      );
    });
    if (!response) throw new DomainError("INTERNAL_ERROR");
    return json(response);
  }

  private async rankedProfile(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    const handle = requireString(input.handle);
    const seasonId = requireString(input.seasonId);
    const seasonStartsAt = requireString(input.seasonStartsAt);
    const now = requireString(input.now);
    let response: unknown = null;
    await this.mutate((state) => {
      const standing = prepareRankedStanding(
        state,
        accountId,
        handle,
        seasonId,
        seasonStartsAt,
        now,
      );
      response = profileForStanding(
        standing,
        rankedRewardTotal(state, accountId),
      );
    });
    if (!response) throw new DomainError("INTERNAL_ERROR");
    return json(response);
  }

  private async rankedLeaderboard(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    const seasonId =
      input.seasonId === null || input.seasonId === undefined
        ? CURRENT_SEASON
        : requireString(input.seasonId);
    if (!/^[A-Z0-9_]{3,32}$/.test(seasonId))
      throw new DomainError("INVALID_REQUEST");
    const cursor =
      input.cursor === null || input.cursor === undefined
        ? null
        : requireUuid(input.cursor);
    const limit = Number(input.limit ?? 20);
    if (!Number.isInteger(limit) || limit < 1 || limit > 50)
      throw new DomainError("INVALID_REQUEST");
    const state = await this.read();
    const ordered = Object.values(state.ranked)
      .filter(
        (standing) =>
          standing.seasonId === seasonId &&
          standing.matchesPlayed >= RANKED_PLACEMENT_MATCHES &&
          state.leaderboardVisible[standing.accountId] !== false,
      )
      .sort(
        (left, right) =>
          right.rating - left.rating ||
          left.accountId.localeCompare(right.accountId),
      );
    const start = cursor
      ? Math.max(
          0,
          ordered.findIndex((standing) => standing.accountId === cursor) + 1,
        )
      : 0;
    const page = ordered.slice(start, start + limit);
    const next = ordered[start + limit]?.accountId ?? null;
    return json({
      seasonId,
      archived:
        Array.isArray(input.availableSeasons) &&
        input.availableSeasons.some(
          (season) =>
            season !== null &&
            typeof season === "object" &&
            (season as { seasonId?: unknown }).seasonId === seasonId &&
            (season as { archived?: unknown }).archived === true,
        ),
      generatedAt: requireString(input.now),
      entries: page.map((standing, index) => ({
        rank: start + index + 1,
        handle: standing.handle,
        rating: standing.rating,
        tier: tier(standing.rating, standing.matchesPlayed),
        matchesPlayed: standing.matchesPlayed,
        wins: standing.wins,
        losses: standing.losses,
        peakRating: standing.peakRating,
      })),
      nextCursor: next,
      availableSeasons: Array.isArray(input.availableSeasons)
        ? input.availableSeasons
        : [{ seasonId: CURRENT_SEASON, archived: false }],
      viewerVisible: state.leaderboardVisible[accountId] !== false,
    });
  }

  private async setVisibility(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const accountId = requireUuid(input.accountId);
    if (typeof input.visible !== "boolean")
      throw new DomainError("INVALID_REQUEST");
    await this.mutate((state) => {
      state.leaderboardVisible[accountId] = input.visible as boolean;
    });
    return json({ visible: input.visible });
  }

  private async exportData(input: Record<string, unknown>): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    const accountId = requireUuid(input.accountId);
    const state = await this.read();
    return json({
      gameHistory: Object.values(state.history[identityId] ?? {}),
      progressionRewards: Object.values(state.rewards[accountId] ?? {}),
      rankedRating: latestStanding(state, accountId)
        ? profileForStanding(
            latestStanding(state, accountId)!,
            rankedRewardTotal(state, accountId),
          )
        : null,
      rankedStandings: Object.values(state.ranked)
        .filter((standing) => standing.accountId === accountId)
        .map((standing) =>
          profileForStanding(standing, rankedRewardTotal(state, accountId)),
        ),
      rankedMatchResults: Object.values(state.rankedMatchResults).filter(
        (result) => result.accountId === accountId,
      ),
      rankedRewards: Object.entries(state.rankedRewards)
        .filter(([key]) => key.startsWith(`${accountId}:`))
        .map(([, reward]) => reward),
      leaderboardVisible: state.leaderboardVisible[accountId] !== false,
    });
  }

  private async deleteIdentity(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const identityId = requireUuid(input.identityId);
    const accountId = requireUuid(input.accountId);
    let rewardsDeleted = 0;
    let roomIds: string[] = [];
    await this.mutate((state) => {
      rewardsDeleted = Object.keys(state.rewards[accountId] ?? {}).length;
      roomIds = Object.keys(state.history[identityId] ?? {});
      delete state.history[identityId];
      delete state.rewards[accountId];
      delete state.handles[identityId];
      delete state.handles[accountId];
      delete state.leaderboardVisible[accountId];
      delete state.recentPlayers[accountId];
      for (const recent of Object.values(state.recentPlayers))
        delete recent[accountId];
      for (const key of Object.keys(state.ranked)) {
        if (state.ranked[key]?.accountId === accountId)
          delete state.ranked[key];
      }
      for (const key of Object.keys(state.rankedRewards)) {
        if (key.startsWith(`${accountId}:`)) {
          delete state.rankedRewards[key];
          rewardsDeleted += 1;
        }
      }
      for (const [key, result] of Object.entries(state.rankedMatchResults)) {
        if (result.accountId === accountId)
          delete state.rankedMatchResults[key];
      }
      for (const [roomId, participants] of Object.entries(
        state.settledRankedRooms,
      )) {
        if (!Array.isArray(participants)) continue;
        const remaining = participants.filter(
          (participant) => participant !== accountId,
        );
        if (remaining.length) state.settledRankedRooms[roomId] = remaining;
        else delete state.settledRankedRooms[roomId];
      }
    });
    return json({ rewardsDeleted, roomIds });
  }

  private async read(): Promise<ProgressionState> {
    const state =
      (await this.ctx.storage.get<ProgressionState>("state")) ??
      structuredClone(EMPTY_STATE);
    normalizeState(state);
    return state;
  }

  private async mutate(
    action: (state: ProgressionState) => void,
  ): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<ProgressionState>("state")) ??
        structuredClone(EMPTY_STATE);
      normalizeState(state);
      action(state);
      await transaction.put("state", state);
    });
  }
}

function normalizeState(state: ProgressionState): void {
  state.history ??= {};
  state.handles ??= {};
  state.rewards ??= {};
  state.ranked ??= {};
  state.leaderboardVisible ??= {};
  state.settledRankedRooms ??= {};
  state.rankedRewards ??= {};
  state.rankedMatchResults ??= {};
  state.recentPlayers ??= {};
  for (const standing of Object.values(state.ranked)) {
    const legacy = standing as RankedStanding & {
      rewardXpEarned?: number;
      seasonRewardIssuedAt?: string | null;
      createdAt?: string;
      updatedAt?: string;
    };
    standing.seasonRewardIssuedAt ??= null;
    standing.createdAt ??= standing.lastMatchAt ?? "2026-08-01T00:00:00.000Z";
    standing.updatedAt ??= standing.lastMatchAt ?? standing.createdAt;
    if ((legacy.rewardXpEarned ?? 0) > 0) {
      addRankedReward(state, standing.accountId, {
        sourceKind: "RANKED_SEASON",
        sourceId: `LEGACY_${standing.seasonId}`,
        seasonId: standing.seasonId,
        xp: legacy.rewardXpEarned!,
        createdAt: standing.updatedAt,
      });
      delete legacy.rewardXpEarned;
    }
  }
}

function participantsFrom(value: unknown): ResultParticipant[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 2)
    throw new DomainError("INVALID_REQUEST");
  return value.map((entry) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry))
      throw new DomainError("INVALID_REQUEST");
    const participant = entry as Record<string, unknown>;
    return {
      identityId: requireUuid(participant.identityId),
      accountId:
        participant.accountId === null
          ? null
          : requireUuid(participant.accountId),
      playerId: requireUuid(participant.playerId),
      handle: requireString(participant.handle),
    };
  });
}

function resultFrom(value: unknown): GameResult {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  const result = value as GameResult;
  requireUuid(result.winnerId);
  requireUuid(result.loserId);
  requireString(result.finishedAt);
  return structuredClone(result);
}

function rankedMatchFrom(
  value: unknown,
): { seasonId: string; contentRevision: number } | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  const match = value as Record<string, unknown>;
  const contentRevision = Number(match.contentRevision);
  if (!Number.isInteger(contentRevision) || contentRevision < 0)
    throw new DomainError("INVALID_REQUEST");
  return {
    seasonId: requireString(match.seasonId),
    contentRevision,
  };
}

function settleRanked(
  state: ProgressionState,
  roomId: string,
  participants: ResultParticipant[],
  result: GameResult,
  seasonId: string,
): void {
  const [first, second] = participants;
  if (!first?.accountId || !second?.accountId)
    throw new DomainError("INVALID_REQUEST");
  const firstKey = rankedKey(seasonId, first.accountId);
  const secondKey = rankedKey(seasonId, second.accountId);
  const firstStanding = standingOrSeed(
    state,
    first.accountId,
    first.handle,
    seasonId,
    result.finishedAt,
  );
  const secondStanding = standingOrSeed(
    state,
    second.accountId,
    second.handle,
    seasonId,
    result.finishedAt,
  );
  const firstBefore = firstStanding.rating;
  const secondBefore = secondStanding.rating;
  const firstChange = recordRankedResult(
    firstStanding,
    secondBefore,
    result.winnerId === first.playerId,
    result.finishedAt,
  );
  const secondChange = recordRankedResult(
    secondStanding,
    firstBefore,
    result.winnerId === second.playerId,
    result.finishedAt,
  );
  state.ranked[firstKey] = firstStanding;
  state.ranked[secondKey] = secondStanding;
  for (const [participant, change] of [
    [first, firstChange],
    [second, secondChange],
  ] as const) {
    const accountId = participant.accountId!;
    const won = result.winnerId === participant.playerId;
    state.rankedMatchResults[`${roomId}:${accountId}`] = {
      roomId,
      accountId,
      seasonId,
      ratingBefore: change.ratingBefore,
      ratingAfter: change.ratingAfter,
      delta: change.delta,
      won,
      finishedAt: result.finishedAt,
    };
    addRankedReward(state, accountId, {
      sourceKind: "RANKED_MATCH",
      sourceId: roomId,
      seasonId,
      xp: won ? 100 : 40,
      createdAt: result.finishedAt,
    });
    if (change.placementCompleted)
      addRankedReward(state, accountId, {
        sourceKind: "RANKED_PLACEMENT",
        sourceId: seasonId,
        seasonId,
        xp: 500,
        createdAt: result.finishedAt,
      });
  }
}

function rankedKey(seasonId: string, accountId: string): string {
  return `${seasonId}:${accountId}`;
}

function prepareRankedStanding(
  state: ProgressionState,
  accountId: string,
  handle: string,
  seasonId: string,
  seasonStartsAt: string,
  now: string,
): RankedStanding {
  if (
    !/^[A-Z0-9_]{3,32}$/.test(seasonId) ||
    !Number.isFinite(Date.parse(seasonStartsAt)) ||
    !Number.isFinite(Date.parse(now))
  )
    throw new DomainError("INVALID_REQUEST");
  issuePriorSeasonRewards(state, accountId, seasonId, seasonStartsAt, now);
  const standing = standingOrSeed(state, accountId, handle, seasonId, now);
  applyInactivityDecay(standing, now);
  standing.handle = handle;
  state.ranked[rankedKey(seasonId, accountId)] = standing;
  return standing;
}

function standingOrSeed(
  state: ProgressionState,
  accountId: string,
  handle: string,
  seasonId: string,
  now: string,
): RankedStanding {
  const existing = state.ranked[rankedKey(seasonId, accountId)];
  if (existing) return existing;
  const previous = latestStanding(state, accountId);
  return newRankedStanding(
    accountId,
    handle,
    seasonId,
    nextSeasonSeed(previous?.rating ?? null),
    now,
  );
}

function latestStanding(
  state: ProgressionState,
  accountId: string,
): RankedStanding | null {
  return (
    Object.values(state.ranked)
      .filter((standing) => standing.accountId === accountId)
      .sort((left, right) =>
        (right.lastMatchAt ?? right.updatedAt).localeCompare(
          left.lastMatchAt ?? left.updatedAt,
        ),
      )[0] ?? null
  );
}

function issuePriorSeasonRewards(
  state: ProgressionState,
  accountId: string,
  currentSeasonId: string,
  seasonStartsAt: string,
  now: string,
): void {
  for (const standing of Object.values(state.ranked)) {
    if (
      standing.accountId !== accountId ||
      standing.seasonId === currentSeasonId ||
      standing.matchesPlayed < RANKED_PLACEMENT_MATCHES ||
      standing.seasonRewardIssuedAt !== null ||
      !standing.lastMatchAt ||
      standing.lastMatchAt >= seasonStartsAt
    )
      continue;
    const xp = seasonRewardXp(tier(standing.rating, standing.matchesPlayed));
    if (xp > 0)
      addRankedReward(state, accountId, {
        sourceKind: "RANKED_SEASON",
        sourceId: standing.seasonId,
        seasonId: standing.seasonId,
        xp,
        createdAt: now,
      });
    standing.seasonRewardIssuedAt = now;
    standing.updatedAt = now;
  }
}

function addRankedReward(
  state: ProgressionState,
  accountId: string,
  reward: RankedReward,
): void {
  const key = `${accountId}:${reward.sourceKind}:${reward.sourceId}:${reward.seasonId}`;
  state.rankedRewards[key] ??= reward;
}

function rankedRewardTotal(state: ProgressionState, accountId: string): number {
  return Object.entries(state.rankedRewards)
    .filter(([key]) => key.startsWith(`${accountId}:`))
    .reduce((total, [, reward]) => total + reward.xp, 0);
}

interface ProgressionRuntime {
  liveContent: ReturnType<typeof baselineLiveContentView>;
  tuning: {
    dailyDeploymentRewardXp: number;
    dailyAccuracyRewardXp: number;
    weeklySupremacyRewardXp: number;
  };
}

function runtimeFrom(input: Record<string, unknown>): ProgressionRuntime {
  if (
    !input.liveContent ||
    typeof input.liveContent !== "object" ||
    Array.isArray(input.liveContent) ||
    !input.tuning ||
    typeof input.tuning !== "object" ||
    Array.isArray(input.tuning)
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  const tuning = input.tuning as Record<string, unknown>;
  const values = {
    dailyDeploymentRewardXp: Number(tuning.dailyDeploymentRewardXp),
    dailyAccuracyRewardXp: Number(tuning.dailyAccuracyRewardXp),
    weeklySupremacyRewardXp: Number(tuning.weeklySupremacyRewardXp),
  };
  if (Object.values(values).some((value) => !Number.isInteger(value)))
    throw new DomainError("INVALID_REQUEST");
  return {
    liveContent: structuredClone(
      input.liveContent,
    ) as ProgressionRuntime["liveContent"],
    tuning: values,
  };
}

function defaultRuntime(now: string): ProgressionRuntime {
  return {
    liveContent: baselineLiveContentView(now),
    tuning: {
      dailyDeploymentRewardXp: 100,
      dailyAccuracyRewardXp: 150,
      weeklySupremacyRewardXp: 400,
    },
  };
}

function buildProgression(
  state: ProgressionState,
  identityId: string,
  accountId: string | null,
  handle: string,
  now: string,
  runtime: ProgressionRuntime = defaultRuntime(now),
) {
  const history = Object.values(state.history[identityId] ?? {});
  let wins = 0;
  let shots = 0;
  let hits = 0;
  let shipsSunk = 0;
  let dailyGames = 0;
  let dailyHits = 0;
  let weeklyWins = 0;
  for (const item of history) {
    const won = item.result.winnerId === item.selfPlayerId;
    if (won) wins += 1;
    const statistics = item.result.players.find(
      (player) => player.playerId === item.selfPlayerId,
    );
    shots += statistics?.shots ?? 0;
    hits += statistics?.hits ?? 0;
    shipsSunk += statistics?.shipsSunk ?? 0;
    if (item.result.finishedAt.slice(0, 10) === now.slice(0, 10)) {
      dailyGames += 1;
      dailyHits += statistics?.hits ?? 0;
    }
    if (isoWeek(item.result.finishedAt) === isoWeek(now) && won)
      weeklyWins += 1;
  }
  const rewards = Object.values(state.rewards[accountId ?? identityId] ?? {});
  const rankedStanding = accountId
    ? (state.ranked[rankedKey(runtime.liveContent.season.id, accountId)] ??
      newRankedStanding(
        accountId,
        handle,
        runtime.liveContent.season.id,
        1_500,
        now,
      ))
    : null;
  const resultXp =
    history.length * 100 + wins * 100 + hits * 3 + shipsSunk * 15;
  const totalXp =
    resultXp +
    rewards.reduce((total, reward) => total + reward.xp, 0) +
    (accountId ? rankedRewardTotal(state, accountId) : 0);
  const level = Math.min(100, Math.floor(totalXp / 500) + 1);
  const levelXp = level === 100 ? 500 : totalXp % 500;
  const accuracy = shots ? Math.floor((hits * 100) / shots) : 0;
  const missions = runtime.liveContent.featureFlags.missionsEnabled
    ? [
        mission(
          state,
          accountId,
          "DAILY_DEPLOYMENT",
          "DAILY",
          "오늘의 출항",
          "오늘 교전 1회를 완료하십시오.",
          dailyGames,
          1,
          runtime.tuning.dailyDeploymentRewardXp,
          now,
        ),
        mission(
          state,
          accountId,
          "DAILY_ACCURACY",
          "DAILY",
          "정밀 포격",
          "오늘 적 함선 칸 10개를 명중시키십시오.",
          dailyHits,
          10,
          runtime.tuning.dailyAccuracyRewardXp,
          now,
        ),
        mission(
          state,
          accountId,
          "WEEKLY_SUPREMACY",
          "WEEKLY",
          "주간 제해권",
          "이번 주 교전 3회에서 승리하십시오.",
          weeklyWins,
          3,
          runtime.tuning.weeklySupremacyRewardXp,
          now,
        ),
      ]
    : [];
  return {
    accountId,
    handle,
    level,
    rankTitle:
      level <= 4
        ? "CADET"
        : level <= 14
          ? "LIEUTENANT"
          : level <= 29
            ? "COMMANDER"
            : level <= 49
              ? "CAPTAIN"
              : level <= 74
                ? "COMMODORE"
                : "ADMIRAL",
    totalXp,
    levelXp,
    xpToNextLevel: level === 100 ? 0 : 500 - levelXp,
    gamesPlayed: history.length,
    wins,
    losses: history.length - wins,
    totalShots: shots,
    totalHits: hits,
    totalShipsSunk: shipsSunk,
    ranked: rankedStanding
      ? profileForStanding(
          rankedStanding,
          accountId ? rankedRewardTotal(state, accountId) : 0,
        )
      : null,
    achievements: [
      achievement(
        "FIRST_CONTACT",
        "첫 접촉",
        "첫 번째 교전을 완료했습니다.",
        history.length,
        1,
        history.length >= 1,
      ),
      achievement(
        "FIRST_VICTORY",
        "첫 승전보",
        "첫 번째 승리를 기록했습니다.",
        wins,
        1,
        wins >= 1,
      ),
      achievement(
        "FLEET_BREAKER",
        "함대 파쇄자",
        "적 함선 25척을 격침했습니다.",
        shipsSunk,
        25,
        shipsSunk >= 25,
      ),
      achievement(
        "SHARPSHOOTER",
        "명사수",
        "20발 이상 사격하고 누적 명중률 60%를 달성했습니다.",
        accuracy,
        60,
        shots >= 20 && accuracy >= 60,
      ),
      achievement(
        "VETERAN",
        "베테랑 지휘관",
        "교전 25회를 완료했습니다.",
        history.length,
        25,
        history.length >= 25,
      ),
    ],
    missions,
    liveContent: runtime.liveContent,
    calculatedAt: now,
  };
}

function achievement(
  id: string,
  title: string,
  description: string,
  progress: number,
  target: number,
  unlocked: boolean,
) {
  return { id, title, description, progress, target, unlocked };
}

function mission(
  state: ProgressionState,
  accountId: string | null,
  id: string,
  cadence: "DAILY" | "WEEKLY",
  title: string,
  description: string,
  progress: number,
  target: number,
  rewardXp: number,
  now: string,
) {
  const periodKey = missionPeriodKey(cadence, now);
  const claimed = accountId
    ? Boolean(state.rewards[accountId]?.[`${id}:${periodKey}`])
    : false;
  return {
    id,
    cadence,
    title,
    description,
    progress,
    target,
    rewardXp,
    completed: progress >= target,
    claimed,
    claimable: Boolean(accountId) && progress >= target && !claimed,
  };
}

function missionPeriodKey(cadence: "DAILY" | "WEEKLY", now: string): string {
  return cadence === "DAILY" ? now.slice(0, 10) : isoWeek(now);
}

function isoWeek(value: string): string {
  const date = new Date(value);
  const utc = new Date(
    Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()),
  );
  const day = utc.getUTCDay() || 7;
  utc.setUTCDate(utc.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(utc.getUTCFullYear(), 0, 1));
  const week = Math.ceil(
    ((utc.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7,
  );
  return `${utc.getUTCFullYear()}-W${String(week).padStart(2, "0")}`;
}

export function baselineLiveContentView(now: string) {
  const season = {
    id: CURRENT_SEASON,
    title: "창립 함대 시즌",
    description: "정식 함대 지휘 체계를 확립하고 첫 시즌 전공을 기록하십시오.",
    startsAt: "2026-08-01T00:00:00.000Z",
    endsAt: "2026-10-31T23:59:59.000Z",
  };
  const event = {
    id: "COMMANDER_MUSTER",
    title: "지휘관 소집령",
    description:
      "일일·주간 임무를 완수해 창립 시즌 함대의 작전 기록을 확장하십시오.",
    startsAt: "2026-08-18T00:00:00.000Z",
    endsAt: "2026-09-01T00:00:00.000Z",
  };
  return {
    revision: 0,
    season: { ...season, status: temporalStatus(season, now) },
    events:
      temporalStatus(event, now) === "ENDED"
        ? []
        : [{ ...event, status: temporalStatus(event, now) }],
    featureFlags: { missionsEnabled: true, eventBannerEnabled: true },
    serverTime: now,
  };
}

function temporalStatus(
  value: { startsAt: string; endsAt: string },
  now: string,
) {
  if (now < value.startsAt) return "UPCOMING";
  if (now < value.endsAt) return "ACTIVE";
  return "ENDED";
}
