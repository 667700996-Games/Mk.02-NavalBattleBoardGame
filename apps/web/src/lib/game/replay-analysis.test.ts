import { describe, expect, it } from 'vitest';
import type {
  AttackOutcome,
  AttackRecord,
  FinishReason,
  GameReplay,
  GameTimelineEvent,
  ShipKind
} from '$lib/types';
import { analyzeReplay } from './replay-analysis';

const alpha = '00000000-0000-4000-8000-000000000001';
const bravo = '00000000-0000-4000-8000-000000000002';
const timestamp = '2026-08-18T00:00:00Z';

function attack(
  attackerId: string,
  targetId: string,
  turnNumber: number,
  outcome: AttackOutcome,
  row: number,
  col: number,
  options: { sunkShip?: ShipKind; winnerId?: string } = {}
): GameTimelineEvent {
  const payload: AttackRecord = {
    requestId: `00000000-0000-4000-9000-${String(turnNumber).padStart(12, '0')}`,
    attackerId,
    targetId,
    coordinate: { row, col },
    outcome,
    sunkShip: options.sunkShip ?? null,
    turnNumber,
    nextPlayerId: options.winnerId ? null : targetId,
    winnerId: options.winnerId ?? null,
    shotsRemainingInTurn: options.winnerId ? 0 : 1,
    resolvedVersion: turnNumber,
    createdAt: timestamp
  };
  return { type: 'ATTACK', payload };
}

function timeout(
  expiredPlayerId: string,
  turnNumber: number,
  consecutiveTimeoutCount: number,
  winnerId: string | null = null
): GameTimelineEvent {
  return {
    type: 'TURN_EXPIRED',
    payload: {
      expiredTurnNumber: turnNumber,
      expiredPlayerId,
      nextPlayerId: winnerId ? null : expiredPlayerId === alpha ? bravo : alpha,
      consecutiveTimeoutCount,
      totalTimeoutCount: consecutiveTimeoutCount,
      winnerId,
      expiredAt: timestamp
    }
  };
}

function replay(
  timeline: GameTimelineEvent[],
  finishReason: FinishReason = 'FLEET_DESTROYED'
): GameReplay {
  return {
    protocolVersion: 2,
    rulesetVersion: 1,
    roomId: '00000000-0000-4000-8000-000000000010',
    roomName: 'After action fixture',
    gameId: '00000000-0000-4000-8000-000000000020',
    firstPlayerId: alpha,
    startedAt: timestamp,
    finishedAt: timestamp,
    players: [
      { id: alpha, nickname: 'Alpha', kind: 'HUMAN', fleet: [] },
      { id: bravo, nickname: 'Bravo', kind: 'HUMAN', fleet: [] }
    ],
    timeline,
    result: {
      winnerId: alpha,
      loserId: bravo,
      totalTurns: Math.max(
        1,
        ...timeline.map((event) =>
          event.type === 'ATTACK' ? event.payload.turnNumber : event.payload.expiredTurnNumber
        )
      ),
      durationSeconds: 300,
      finishedAt: timestamp,
      players: [
        { playerId: alpha, shots: 0, hits: 0, shipsSunk: 0, accuracy: 0, totalTimeouts: 0 },
        { playerId: bravo, shots: 0, hits: 0, shipsSunk: 0, accuracy: 0, totalTimeouts: 0 }
      ],
      finishReason,
      winType: finishReason === 'SURRENDER' ? 'SURRENDER' : 'NORMAL_VICTORY'
    }
  };
}

describe('analyzeReplay', () => {
  it('derives accuracy, streaks, phases, follow-ups, and decisive turns from the timeline', () => {
    const analysis = analyzeReplay(
      replay([
        attack(alpha, bravo, 1, 'MISS', 9, 9),
        attack(bravo, alpha, 2, 'MISS', 0, 0),
        attack(alpha, bravo, 3, 'HIT', 0, 0),
        attack(bravo, alpha, 4, 'MISS', 2, 2),
        attack(alpha, bravo, 5, 'HIT', 0, 1),
        attack(bravo, alpha, 6, 'MISS', 4, 4),
        attack(alpha, bravo, 7, 'SUNK', 0, 2, { sunkShip: 'SUBMARINE' }),
        timeout(bravo, 8, 2),
        attack(bravo, alpha, 9, 'MISS', 6, 6),
        attack(alpha, bravo, 10, 'SUNK', 8, 8, {
          sunkShip: 'DESTROYER',
          winnerId: alpha
        })
      ])
    );

    const alphaAnalysis = analysis.players.find((player) => player.playerId === alpha)!;
    expect(alphaAnalysis).toMatchObject({
      shots: 5,
      hits: 4,
      misses: 1,
      shipsSunk: 2,
      accuracy: 0.8,
      maxHitStreak: 4,
      maxMissStreak: 1,
      adjacentFollowUps: 2,
      followUpOpportunities: 2,
      followUpRate: 1
    });
    expect(alphaAnalysis.phases.map((phase) => [phase.shots, phase.hits])).toEqual([
      [2, 1],
      [2, 2],
      [1, 1]
    ]);
    expect(analysis.decisiveMoments.map((moment) => moment.title)).toEqual([
      '승부를 끝낸 일격',
      '전력 균형을 바꾼 격침',
      '누적된 시간 압박'
    ]);
    expect(analysis.decisiveMoments[0]).toMatchObject({
      eventIndex: 9,
      turnNumber: 10,
      playerId: alpha,
      impact: 'CRITICAL'
    });
  });

  it('turns observed low accuracy, miss streaks, poor follow-up, and timeouts into bounded tips', () => {
    const timeline: GameTimelineEvent[] = [
      attack(bravo, alpha, 1, 'MISS', 0, 0),
      attack(bravo, alpha, 2, 'MISS', 1, 2),
      attack(bravo, alpha, 3, 'MISS', 2, 4),
      attack(bravo, alpha, 4, 'MISS', 3, 6),
      attack(bravo, alpha, 5, 'HIT', 5, 5),
      attack(bravo, alpha, 6, 'MISS', 9, 9),
      attack(bravo, alpha, 7, 'HIT', 7, 7),
      attack(bravo, alpha, 8, 'MISS', 0, 9),
      timeout(bravo, 9, 1),
      attack(alpha, bravo, 10, 'SUNK', 0, 0, { winnerId: alpha, sunkShip: 'CARRIER' })
    ];

    const bravoAnalysis = analyzeReplay(replay(timeline)).players.find(
      (player) => player.playerId === bravo
    )!;
    expect(bravoAnalysis.accuracy).toBe(0.25);
    expect(bravoAnalysis.maxMissStreak).toBe(4);
    expect(bravoAnalysis.followUpRate).toBe(0);
    expect(bravoAnalysis.timeouts).toBe(1);
    expect(bravoAnalysis.tips).toHaveLength(4);
    expect(bravoAnalysis.tips.join(' ')).toContain('명중률 25%');
    expect(bravoAnalysis.tips.join(' ')).toContain('4연속 빗나감');
    expect(bravoAnalysis.tips.join(' ')).toContain('명중 뒤 인접 추적');
    expect(bravoAnalysis.tips.join(' ')).toContain('시간 초과가 1회');
  });

  it('explains a result that ended outside the attack timeline and handles no-shot players', () => {
    const analysis = analyzeReplay(replay([], 'SURRENDER'));
    expect(analysis.players.every((player) => player.shots === 0 && player.accuracy === 0)).toBe(
      true
    );
    expect(analysis.decisiveMoments).toEqual([
      expect.objectContaining({
        eventIndex: null,
        playerId: alpha,
        impact: 'CRITICAL',
        title: '기권으로 확정된 승부'
      })
    ]);
  });

  it('records a third timeout as the critical finishing moment without a synthetic duplicate', () => {
    const fixture = replay([timeout(bravo, 8, 3, alpha)], 'TURN_TIMEOUT');
    fixture.result.winType = 'TIMEOUT';
    const analysis = analyzeReplay(fixture);
    expect(analysis.decisiveMoments).toHaveLength(1);
    expect(analysis.decisiveMoments[0]).toMatchObject({
      eventIndex: 0,
      impact: 'CRITICAL',
      title: '시간 초과로 확정된 승부'
    });
  });
});
