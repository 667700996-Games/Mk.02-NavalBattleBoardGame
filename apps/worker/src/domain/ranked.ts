export interface RankedStanding {
  seasonId: string;
  accountId: string;
  handle: string;
  rating: number;
  matchesPlayed: number;
  wins: number;
  losses: number;
  peakRating: number;
  lastMatchAt: string | null;
  decayPointsApplied: number;
  seasonRewardIssuedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export const RANKED_PLACEMENT_MATCHES = 5;
const RANKED_DECAY_GRACE_DAYS = 14;
const RANKED_DECAY_INTERVAL_DAYS = 7;
const RANKED_DECAY_POINTS = 25;
const RANKED_DECAY_THRESHOLD = 2_100;
const RANKED_DECAY_FLOOR = 1_800;

export function newRankedStanding(
  accountId: string,
  handle: string,
  seasonId: string,
  seedRating = 1_500,
  now = new Date().toISOString(),
): RankedStanding {
  const rating = Math.max(0, Math.min(4_000, seedRating));
  return {
    seasonId,
    accountId,
    handle,
    rating,
    matchesPlayed: 0,
    wins: 0,
    losses: 0,
    peakRating: rating,
    lastMatchAt: null,
    decayPointsApplied: 0,
    seasonRewardIssuedAt: null,
    createdAt: now,
    updatedAt: now,
  };
}

export function recordRankedResult(
  standing: RankedStanding,
  opponentRating: number,
  won: boolean,
  finishedAt: string,
): {
  ratingBefore: number;
  ratingAfter: number;
  delta: number;
  placementCompleted: boolean;
} {
  const before = standing.rating;
  const expected = 1 / (1 + 10 ** ((opponentRating - standing.rating) / 400));
  const k = standing.matchesPlayed < RANKED_PLACEMENT_MATCHES ? 64 : 32;
  let delta = Math.round(k * ((won ? 1 : 0) - expected));
  delta = won ? Math.max(1, delta) : Math.min(-1, delta);
  standing.rating = Math.max(0, Math.min(4_000, standing.rating + delta));
  delta = standing.rating - before;
  standing.matchesPlayed += 1;
  if (won) standing.wins += 1;
  else standing.losses += 1;
  standing.peakRating = Math.max(standing.peakRating, standing.rating);
  standing.lastMatchAt = finishedAt;
  standing.decayPointsApplied = 0;
  standing.updatedAt = finishedAt;
  return {
    ratingBefore: before,
    ratingAfter: standing.rating,
    delta,
    placementCompleted: standing.matchesPlayed === RANKED_PLACEMENT_MATCHES,
  };
}

export function applyInactivityDecay(
  standing: RankedStanding,
  now: string,
): number {
  if (
    standing.matchesPlayed < RANKED_PLACEMENT_MATCHES ||
    standing.rating < RANKED_DECAY_THRESHOLD ||
    !standing.lastMatchAt
  )
    return 0;
  const inactiveDays = Math.floor(
    (Date.parse(now) - Date.parse(standing.lastMatchAt)) / 86_400_000,
  );
  if (inactiveDays < RANKED_DECAY_GRACE_DAYS) return 0;
  const dueSteps =
    1 +
    Math.floor(
      (inactiveDays - RANKED_DECAY_GRACE_DAYS) / RANKED_DECAY_INTERVAL_DAYS,
    );
  const appliedSteps = Math.floor(
    standing.decayPointsApplied / RANKED_DECAY_POINTS,
  );
  const newSteps = Math.max(0, dueSteps - appliedSteps);
  if (!newSteps) return 0;
  const before = standing.rating;
  standing.rating = Math.max(
    RANKED_DECAY_FLOOR,
    standing.rating - newSteps * RANKED_DECAY_POINTS,
  );
  standing.decayPointsApplied = dueSteps * RANKED_DECAY_POINTS;
  standing.updatedAt = now;
  return standing.rating - before;
}

export function nextDecayAt(standing: RankedStanding): string | null {
  if (
    standing.matchesPlayed < RANKED_PLACEMENT_MATCHES ||
    standing.rating < RANKED_DECAY_THRESHOLD ||
    !standing.lastMatchAt
  )
    return null;
  const appliedSteps = Math.floor(
    standing.decayPointsApplied / RANKED_DECAY_POINTS,
  );
  return new Date(
    Date.parse(standing.lastMatchAt) +
      (RANKED_DECAY_GRACE_DAYS + appliedSteps * RANKED_DECAY_INTERVAL_DAYS) *
        86_400_000,
  ).toISOString();
}

export function rankedTier(rating: number, matchesPlayed: number) {
  if (matchesPlayed < RANKED_PLACEMENT_MATCHES) return "PROVISIONAL" as const;
  if (rating <= 1_199) return "BRONZE" as const;
  if (rating <= 1_499) return "SILVER" as const;
  if (rating <= 1_799) return "GOLD" as const;
  if (rating <= 2_099) return "PLATINUM" as const;
  if (rating <= 2_399) return "DIAMOND" as const;
  return "ADMIRAL" as const;
}

export function seasonRewardXp(tier: ReturnType<typeof rankedTier>): number {
  if (tier === "BRONZE") return 500;
  if (tier === "SILVER") return 750;
  if (tier === "GOLD") return 1_000;
  if (tier === "PLATINUM") return 1_500;
  if (tier === "DIAMOND") return 2_000;
  if (tier === "ADMIRAL") return 3_000;
  return 0;
}

export function nextSeasonSeed(previousRating: number | null): number {
  if (previousRating === null) return 1_500;
  return Math.max(
    1_000,
    Math.min(2_000, 1_500 + Math.trunc((previousRating - 1_500) / 2)),
  );
}

export function rankedProfile(standing: RankedStanding, rewardXpEarned = 0) {
  return {
    seasonId: standing.seasonId,
    rating: standing.rating,
    matchesPlayed: standing.matchesPlayed,
    wins: standing.wins,
    losses: standing.losses,
    peakRating: standing.peakRating,
    tier: rankedTier(standing.rating, standing.matchesPlayed),
    placementMatchesRemaining: Math.max(
      0,
      RANKED_PLACEMENT_MATCHES - standing.matchesPlayed,
    ),
    lastMatchAt: standing.lastMatchAt,
    nextDecayAt: nextDecayAt(standing),
    decayPointsApplied: standing.decayPointsApplied,
    rewardXpEarned,
  };
}
