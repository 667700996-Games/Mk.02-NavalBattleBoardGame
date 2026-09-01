import type {
  AttackRecord,
  FinishReason,
  GameReplay,
  GameTimelineEvent,
  ReplayPlayer
} from '$lib/types';
import { message, shipMessageKey, type MessageKey, type Translator } from '$lib/i18n';

export interface AccuracyPhase {
  id: 'OPENING' | 'MIDGAME' | 'ENDGAME';
  label: string;
  shots: number;
  hits: number;
  accuracy: number;
}

export interface ReplayPlayerAnalysis {
  playerId: string;
  nickname: string;
  won: boolean;
  shots: number;
  hits: number;
  misses: number;
  shipsSunk: number;
  timeouts: number;
  accuracy: number;
  maxHitStreak: number;
  maxMissStreak: number;
  adjacentFollowUps: number;
  followUpOpportunities: number;
  followUpRate: number | null;
  phases: AccuracyPhase[];
  tips: string[];
}

export interface DecisiveMoment {
  eventIndex: number | null;
  turnNumber: number;
  playerId: string | null;
  impact: 'CRITICAL' | 'HIGH' | 'MEDIUM';
  title: string;
  detail: string;
}

export interface ReplayAnalysis {
  players: ReplayPlayerAnalysis[];
  decisiveMoments: DecisiveMoment[];
}

interface RankedMoment extends DecisiveMoment {
  score: number;
}

const phaseDefinitions: Array<{ id: AccuracyPhase['id']; key: MessageKey }> = [
  { id: 'OPENING', key: 'replayAnalysis.phaseOpening' },
  { id: 'MIDGAME', key: 'replayAnalysis.phaseMidgame' },
  { id: 'ENDGAME', key: 'replayAnalysis.phaseEndgame' }
];

const finishLabelKeys: Record<FinishReason, { title: MessageKey; action: MessageKey }> = {
  FLEET_DESTROYED: {
    title: 'replayAnalysis.finishFleetDestroyedTitle',
    action: 'replayAnalysis.finishFleetDestroyedAction'
  },
  SURRENDER: {
    title: 'replayAnalysis.finishSurrenderTitle',
    action: 'replayAnalysis.finishSurrenderAction'
  },
  TURN_TIMEOUT: {
    title: 'replayAnalysis.finishTurnTimeoutTitle',
    action: 'replayAnalysis.finishTurnTimeoutAction'
  },
  DISCONNECT_TIMEOUT: {
    title: 'replayAnalysis.finishDisconnectTitle',
    action: 'replayAnalysis.finishDisconnectAction'
  },
  PLAYER_LEFT: {
    title: 'replayAnalysis.finishPlayerLeftTitle',
    action: 'replayAnalysis.finishPlayerLeftAction'
  }
};

function attacksByPlayer(timeline: GameTimelineEvent[], playerId: string): AttackRecord[] {
  return timeline.flatMap((event) =>
    event.type !== 'TURN_EXPIRED' && event.payload.attackerId === playerId
      ? attacksFromEvent(event)
      : []
  );
}

function attacksFromEvent(
  event: Exclude<GameTimelineEvent, { type: 'TURN_EXPIRED' }>
): AttackRecord[] {
  if (event.type === 'ATTACK') return [event.payload];
  return event.payload.cells.map((cell, index) => ({
    requestId: `${event.payload.requestId}:${index}`,
    attackerId: event.payload.attackerId,
    targetId: event.payload.targetId,
    coordinate: cell.coordinate,
    outcome: cell.outcome,
    sunkShip: cell.sunkShip,
    turnNumber: event.payload.turnNumber,
    nextPlayerId: event.payload.nextPlayerId,
    winnerId: index === event.payload.cells.length - 1 ? event.payload.winnerId : null,
    shotsRemainingInTurn: event.payload.shotsRemainingInTurn,
    resolvedVersion: event.payload.resolvedVersion,
    createdAt: event.payload.createdAt
  }));
}

function accuracy(hits: number, shots: number): number {
  return shots === 0 ? 0 : hits / shots;
}

function phaseAccuracy(attacks: AttackRecord[], translate: Translator): AccuracyPhase[] {
  const buckets = phaseDefinitions.map((phase) => ({
    id: phase.id,
    label: translate(phase.key),
    shots: 0,
    hits: 0,
    accuracy: 0
  }));
  for (const [index, attack] of attacks.entries()) {
    const phaseIndex = Math.min(2, Math.floor((index * 3) / Math.max(attacks.length, 1)));
    const phase = buckets[phaseIndex];
    phase.shots += 1;
    if (attack.outcome !== 'MISS') phase.hits += 1;
  }
  for (const phase of buckets) phase.accuracy = accuracy(phase.hits, phase.shots);
  return buckets;
}

function streaks(attacks: AttackRecord[]): { maxHitStreak: number; maxMissStreak: number } {
  let hitStreak = 0;
  let missStreak = 0;
  let maxHitStreak = 0;
  let maxMissStreak = 0;
  for (const attack of attacks) {
    if (attack.outcome === 'MISS') {
      missStreak += 1;
      hitStreak = 0;
      maxMissStreak = Math.max(maxMissStreak, missStreak);
    } else {
      hitStreak += 1;
      missStreak = 0;
      maxHitStreak = Math.max(maxHitStreak, hitStreak);
    }
  }
  return { maxHitStreak, maxMissStreak };
}

function followUps(attacks: AttackRecord[]): {
  adjacentFollowUps: number;
  followUpOpportunities: number;
  followUpRate: number | null;
} {
  let adjacentFollowUps = 0;
  let followUpOpportunities = 0;
  for (let index = 0; index < attacks.length - 1; index += 1) {
    const attack = attacks[index];
    if (attack.outcome !== 'HIT') continue;
    followUpOpportunities += 1;
    const next = attacks[index + 1];
    const distance =
      Math.abs(next.coordinate.row - attack.coordinate.row) +
      Math.abs(next.coordinate.col - attack.coordinate.col);
    if (distance === 1) adjacentFollowUps += 1;
  }
  return {
    adjacentFollowUps,
    followUpOpportunities,
    followUpRate: followUpOpportunities === 0 ? null : adjacentFollowUps / followUpOpportunities
  };
}

function playerTips(
  analysis: Omit<ReplayPlayerAnalysis, 'tips'>,
  replay: GameReplay,
  translate: Translator
): string[] {
  const tips: string[] = [];
  const percentage = Math.round(analysis.accuracy * 100);
  if (analysis.shots >= 6 && analysis.accuracy < 0.35) {
    tips.push(translate('replayAnalysis.tipLowAccuracy', { percentage }));
  }
  if (analysis.maxMissStreak >= 4) {
    tips.push(translate('replayAnalysis.tipMissStreak', { count: analysis.maxMissStreak }));
  }
  if (
    analysis.followUpRate !== null &&
    analysis.followUpOpportunities >= 2 &&
    analysis.followUpRate < 0.6
  ) {
    tips.push(
      translate('replayAnalysis.tipFollowUp', {
        adjacent: analysis.adjacentFollowUps,
        opportunities: analysis.followUpOpportunities
      })
    );
  }
  if (analysis.timeouts > 0) {
    tips.push(translate('replayAnalysis.tipTimeout', { count: analysis.timeouts }));
  }
  const opening = analysis.phases[0];
  const endgame = analysis.phases[2];
  if (opening.shots >= 2 && endgame.shots >= 2 && opening.accuracy - endgame.accuracy >= 0.2) {
    tips.push(
      translate('replayAnalysis.tipLateDrop', {
        difference: Math.round((opening.accuracy - endgame.accuracy) * 100)
      })
    );
  }
  if (tips.length === 0) {
    tips.push(
      analysis.won
        ? translate('replayAnalysis.tipWinStable', {
            percentage,
            streak: analysis.maxHitStreak
          })
        : translate('replayAnalysis.tipLossStable', { turns: replay.result.totalTurns })
    );
  }
  return tips.slice(0, 4);
}

function analyzePlayer(
  player: ReplayPlayer,
  replay: GameReplay,
  translate: Translator
): ReplayPlayerAnalysis {
  const attacks = attacksByPlayer(replay.timeline, player.id);
  const hits = attacks.filter((attack) => attack.outcome !== 'MISS').length;
  const shipsSunk = attacks.filter((attack) => attack.outcome === 'SUNK').length;
  const timeouts = replay.timeline.filter(
    (event) => event.type === 'TURN_EXPIRED' && event.payload.expiredPlayerId === player.id
  ).length;
  const base: Omit<ReplayPlayerAnalysis, 'tips'> = {
    playerId: player.id,
    nickname: player.nickname,
    won: replay.result.winnerId === player.id,
    shots: attacks.length,
    hits,
    misses: attacks.length - hits,
    shipsSunk,
    timeouts,
    accuracy: accuracy(hits, attacks.length),
    ...streaks(attacks),
    ...followUps(attacks),
    phases: phaseAccuracy(attacks, translate)
  };
  return { ...base, tips: playerTips(base, replay, translate) };
}

function coordinateLabel(attack: AttackRecord): string {
  return `${String.fromCharCode(65 + attack.coordinate.row)}${attack.coordinate.col + 1}`;
}

function playerName(replay: GameReplay, playerId: string | null, translate: Translator): string {
  return (
    replay.players.find((player) => player.id === playerId)?.nickname ??
    translate('common.commander')
  );
}

function rankedMoments(replay: GameReplay, translate: Translator): RankedMoment[] {
  const moments: RankedMoment[] = [];
  const hitStreaks = new Map<string, number>();
  let hasRecordedFinish = false;
  for (const [eventIndex, event] of replay.timeline.entries()) {
    if (event.type === 'TURN_EXPIRED') {
      if (event.payload.winnerId) {
        hasRecordedFinish = true;
        moments.push({
          eventIndex,
          turnNumber: event.payload.expiredTurnNumber,
          playerId: event.payload.winnerId,
          impact: 'CRITICAL',
          score: 100,
          title: translate('replayAnalysis.timeoutFinishTitle'),
          detail: translate('replayAnalysis.timeoutFinishDetail', {
            name: playerName(replay, event.payload.expiredPlayerId, translate),
            count: event.payload.consecutiveTimeoutCount
          })
        });
      } else if (event.payload.consecutiveTimeoutCount >= 2) {
        moments.push({
          eventIndex,
          turnNumber: event.payload.expiredTurnNumber,
          playerId: event.payload.expiredPlayerId,
          impact: 'HIGH',
          score: 35,
          title: translate('replayAnalysis.timeoutPressureTitle'),
          detail: translate('replayAnalysis.timeoutPressureDetail', {
            name: playerName(replay, event.payload.expiredPlayerId, translate),
            count: event.payload.consecutiveTimeoutCount
          })
        });
      }
      continue;
    }

    for (const attack of attacksFromEvent(event)) {
      const nextStreak =
        attack.outcome === 'MISS' ? 0 : (hitStreaks.get(attack.attackerId) ?? 0) + 1;
      hitStreaks.set(attack.attackerId, nextStreak);
      if (attack.winnerId) {
        hasRecordedFinish = true;
        moments.push({
          eventIndex,
          turnNumber: attack.turnNumber,
          playerId: attack.attackerId,
          impact: 'CRITICAL',
          score: 100,
          title: translate('replayAnalysis.finishingStrikeTitle'),
          detail: translate('replayAnalysis.finishingStrikeDetail', {
            name: playerName(replay, attack.attackerId, translate),
            coordinate: coordinateLabel(attack),
            ship: attack.sunkShip
              ? translate(shipMessageKey(attack.sunkShip))
              : translate('replayAnalysis.lastShip')
          })
        });
      } else if (attack.outcome === 'SUNK') {
        moments.push({
          eventIndex,
          turnNumber: attack.turnNumber,
          playerId: attack.attackerId,
          impact: 'HIGH',
          score: 50 + attack.turnNumber / Math.max(replay.result.totalTurns, 1),
          title: translate('replayAnalysis.sunkShiftTitle'),
          detail: translate('replayAnalysis.sunkShiftDetail', {
            name: playerName(replay, attack.attackerId, translate),
            coordinate: coordinateLabel(attack),
            ship: attack.sunkShip
              ? translate(shipMessageKey(attack.sunkShip))
              : translate('replayAnalysis.ship')
          })
        });
      } else if (nextStreak === 3) {
        moments.push({
          eventIndex,
          turnNumber: attack.turnNumber,
          playerId: attack.attackerId,
          impact: 'MEDIUM',
          score: 25,
          title: translate('replayAnalysis.hitStreakTitle'),
          detail: translate('replayAnalysis.hitStreakDetail', {
            name: playerName(replay, attack.attackerId, translate),
            coordinate: coordinateLabel(attack)
          })
        });
      }
    }
  }

  if (!hasRecordedFinish) {
    const finish = finishLabelKeys[replay.result.finishReason];
    moments.push({
      eventIndex: null,
      turnNumber: replay.result.totalTurns,
      playerId: replay.result.winnerId,
      impact: 'CRITICAL',
      score: 100,
      title: translate(finish.title),
      detail: translate('replayAnalysis.winnerDetail', {
        name: playerName(replay, replay.result.winnerId, translate),
        action: translate(finish.action)
      })
    });
  }

  return moments;
}

export function analyzeReplay(replay: GameReplay, translate: Translator = message): ReplayAnalysis {
  const decisiveMoments = rankedMoments(replay, translate)
    .sort((left, right) => right.score - left.score || right.turnNumber - left.turnNumber)
    .slice(0, 3)
    .map((moment) => ({
      eventIndex: moment.eventIndex,
      turnNumber: moment.turnNumber,
      playerId: moment.playerId,
      impact: moment.impact,
      title: moment.title,
      detail: moment.detail
    }));
  return {
    players: replay.players.map((player) => analyzePlayer(player, replay, translate)),
    decisiveMoments
  };
}
