import { describe, expect, it } from "vitest";
import {
  applyInactivityDecay,
  newRankedStanding,
  nextSeasonSeed,
  rankedTier,
  recordRankedResult,
  seasonRewardXp,
} from "./ranked";

describe("ranked competition", () => {
  it("uses placement K-factor then reveals the established tier", () => {
    const standing = newRankedStanding(
      crypto.randomUUID(),
      "Alpha",
      "S1",
      1_500,
      "2026-01-01T00:00:00.000Z",
    );
    for (let match = 0; match < 4; match += 1) {
      const change = recordRankedResult(
        standing,
        1_500,
        true,
        "2026-01-02T00:00:00.000Z",
      );
      expect(change.placementCompleted).toBe(false);
      expect(rankedTier(standing.rating, standing.matchesPlayed)).toBe(
        "PROVISIONAL",
      );
    }
    const placement = recordRankedResult(
      standing,
      1_500,
      true,
      "2026-01-03T00:00:00.000Z",
    );
    expect(placement.placementCompleted).toBe(true);
    expect(rankedTier(standing.rating, standing.matchesPlayed)).not.toBe(
      "PROVISIONAL",
    );
  });

  it("applies inactivity decay once and stops at the floor", () => {
    const standing = newRankedStanding(
      crypto.randomUUID(),
      "Alpha",
      "S1",
      2_150,
      "2025-12-01T00:00:00.000Z",
    );
    standing.matchesPlayed = 20;
    standing.lastMatchAt = "2026-01-01T00:00:00.000Z";
    expect(applyInactivityDecay(standing, "2026-01-29T00:00:00.000Z")).toBe(
      -75,
    );
    expect(applyInactivityDecay(standing, "2026-01-29T00:00:00.000Z")).toBe(0);
    expect(standing.rating).toBe(2_075);

    standing.rating = 2_150;
    standing.lastMatchAt = "2025-01-01T00:00:00.000Z";
    standing.decayPointsApplied = 0;
    expect(applyInactivityDecay(standing, "2026-02-05T00:00:00.000Z")).toBe(
      -350,
    );
    expect(standing.rating).toBe(1_800);
  });

  it("soft-resets new seasons and maps established tiers to rewards", () => {
    expect(nextSeasonSeed(null)).toBe(1_500);
    expect(nextSeasonSeed(2_500)).toBe(2_000);
    expect(nextSeasonSeed(900)).toBe(1_200);
    expect(seasonRewardXp(rankedTier(2_450, 5))).toBe(3_000);
    expect(seasonRewardXp(rankedTier(1_500, 4))).toBe(0);
  });
});
