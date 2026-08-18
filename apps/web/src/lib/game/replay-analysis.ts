import type {
  AttackRecord,
  FinishReason,
  GameReplay,
  GameTimelineEvent,
  ReplayPlayer,
  ShipKind
} from '$lib/types';

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

const phaseDefinitions: Array<Pick<AccuracyPhase, 'id' | 'label'>> = [
  { id: 'OPENING', label: '초반' },
  { id: 'MIDGAME', label: '중반' },
  { id: 'ENDGAME', label: '후반' }
];

const shipLabels: Record<ShipKind, string> = {
  CARRIER: '항공모함',
  BATTLESHIP: '전함',
  CRUISER: '순양함',
  SUBMARINE: '잠수함',
  DESTROYER: '구축함'
};

const finishLabels: Record<FinishReason, { title: string; action: string }> = {
  FLEET_DESTROYED: { title: '함대 전멸로 확정된 승부', action: '마지막 함선을 제거했습니다.' },
  SURRENDER: { title: '기권으로 확정된 승부', action: '상대의 기권으로 승리가 확정됐습니다.' },
  TURN_TIMEOUT: {
    title: '누적 시간 초과로 확정된 승부',
    action: '상대가 세 번째 연속 시간 제한을 넘겼습니다.'
  },
  DISCONNECT_TIMEOUT: {
    title: '재접속 유예 종료로 확정된 승부',
    action: '상대가 재접속 제한 시간 안에 복귀하지 못했습니다.'
  },
  PLAYER_LEFT: { title: '상대 이탈로 확정된 승부', action: '상대가 작전을 이탈했습니다.' }
};

function attacksByPlayer(timeline: GameTimelineEvent[], playerId: string): AttackRecord[] {
  return timeline.flatMap((event) =>
    event.type === 'ATTACK' && event.payload.attackerId === playerId ? [event.payload] : []
  );
}

function accuracy(hits: number, shots: number): number {
  return shots === 0 ? 0 : hits / shots;
}

function phaseAccuracy(attacks: AttackRecord[]): AccuracyPhase[] {
  const buckets = phaseDefinitions.map((phase) => ({ ...phase, shots: 0, hits: 0, accuracy: 0 }));
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

function playerTips(analysis: Omit<ReplayPlayerAnalysis, 'tips'>, replay: GameReplay): string[] {
  const tips: string[] = [];
  const percentage = Math.round(analysis.accuracy * 100);
  if (analysis.shots >= 6 && analysis.accuracy < 0.35) {
    tips.push(
      `명중률 ${percentage}%입니다. 초반에는 한 칸씩 흩어 쏘기보다 일정한 간격의 탐색 격자를 유지해 후보 해역을 줄이세요.`
    );
  }
  if (analysis.maxMissStreak >= 4) {
    tips.push(
      `${analysis.maxMissStreak}연속 빗나감이 있었습니다. 이미 비어 있다고 판정된 줄 주변보다 아직 넓게 남은 해역으로 탐색축을 옮기세요.`
    );
  }
  if (
    analysis.followUpRate !== null &&
    analysis.followUpOpportunities >= 2 &&
    analysis.followUpRate < 0.6
  ) {
    tips.push(
      `명중 뒤 인접 추적은 ${analysis.adjacentFollowUps}/${analysis.followUpOpportunities}회였습니다. 격침 전에는 직교 인접 칸으로 함선 방향을 먼저 확인하세요.`
    );
  }
  if (analysis.timeouts > 0) {
    tips.push(
      `시간 초과가 ${analysis.timeouts}회 있었습니다. 상대 턴 동안 다음 탐색 좌표와 명중 시 후속 좌표를 함께 준비하세요.`
    );
  }
  const opening = analysis.phases[0];
  const endgame = analysis.phases[2];
  if (opening.shots >= 2 && endgame.shots >= 2 && opening.accuracy - endgame.accuracy >= 0.2) {
    tips.push(
      `후반 명중률이 초반보다 ${Math.round((opening.accuracy - endgame.accuracy) * 100)}%p 낮았습니다. 남은 함선 길이와 배제된 행·열을 다시 계산한 뒤 발사하세요.`
    );
  }
  if (tips.length === 0) {
    tips.push(
      analysis.won
        ? `명중률 ${percentage}%와 최대 ${analysis.maxHitStreak}연속 명중의 리듬을 유지했습니다. 다음 교전에서도 탐색과 추적 단계를 분리하세요.`
        : `큰 전술 누수는 보이지 않았습니다. ${replay.result.totalTurns}턴 기록에서 상대보다 먼저 후보 해역을 줄일 수 있었던 선택을 비교하세요.`
    );
  }
  return tips.slice(0, 4);
}

function analyzePlayer(player: ReplayPlayer, replay: GameReplay): ReplayPlayerAnalysis {
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
    phases: phaseAccuracy(attacks)
  };
  return { ...base, tips: playerTips(base, replay) };
}

function coordinateLabel(attack: AttackRecord): string {
  return `${String.fromCharCode(65 + attack.coordinate.row)}${attack.coordinate.col + 1}`;
}

function playerName(replay: GameReplay, playerId: string | null): string {
  return replay.players.find((player) => player.id === playerId)?.nickname ?? '지휘관';
}

function rankedMoments(replay: GameReplay): RankedMoment[] {
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
          title: '시간 초과로 확정된 승부',
          detail: `${playerName(replay, event.payload.expiredPlayerId)} 지휘관의 ${event.payload.consecutiveTimeoutCount}번째 연속 시간 초과로 승부가 끝났습니다.`
        });
      } else if (event.payload.consecutiveTimeoutCount >= 2) {
        moments.push({
          eventIndex,
          turnNumber: event.payload.expiredTurnNumber,
          playerId: event.payload.expiredPlayerId,
          impact: 'HIGH',
          score: 35,
          title: '누적된 시간 압박',
          detail: `${playerName(replay, event.payload.expiredPlayerId)} 지휘관이 ${event.payload.consecutiveTimeoutCount}회 연속 공격 기회를 잃어 다음 시간 초과가 패배 조건이 됐습니다.`
        });
      }
      continue;
    }

    const attack = event.payload;
    const nextStreak = attack.outcome === 'MISS' ? 0 : (hitStreaks.get(attack.attackerId) ?? 0) + 1;
    hitStreaks.set(attack.attackerId, nextStreak);
    if (attack.winnerId) {
      hasRecordedFinish = true;
      moments.push({
        eventIndex,
        turnNumber: attack.turnNumber,
        playerId: attack.attackerId,
        impact: 'CRITICAL',
        score: 100,
        title: '승부를 끝낸 일격',
        detail: `${playerName(replay, attack.attackerId)} 지휘관이 ${coordinateLabel(attack)}에서 ${attack.sunkShip ? shipLabels[attack.sunkShip] : '마지막 함선'}을 격침했습니다.`
      });
    } else if (attack.outcome === 'SUNK') {
      moments.push({
        eventIndex,
        turnNumber: attack.turnNumber,
        playerId: attack.attackerId,
        impact: 'HIGH',
        score: 50 + attack.turnNumber / Math.max(replay.result.totalTurns, 1),
        title: '전력 균형을 바꾼 격침',
        detail: `${playerName(replay, attack.attackerId)} 지휘관이 ${coordinateLabel(attack)}에서 ${attack.sunkShip ? shipLabels[attack.sunkShip] : '함선'}을 제거했습니다.`
      });
    } else if (nextStreak === 3) {
      moments.push({
        eventIndex,
        turnNumber: attack.turnNumber,
        playerId: attack.attackerId,
        impact: 'MEDIUM',
        score: 25,
        title: '3연속 명중으로 확보한 주도권',
        detail: `${playerName(replay, attack.attackerId)} 지휘관이 ${coordinateLabel(attack)}까지 세 번 연속 명중해 탐색을 확정 추적으로 전환했습니다.`
      });
    }
  }

  if (!hasRecordedFinish) {
    const finish = finishLabels[replay.result.finishReason];
    moments.push({
      eventIndex: null,
      turnNumber: replay.result.totalTurns,
      playerId: replay.result.winnerId,
      impact: 'CRITICAL',
      score: 100,
      title: finish.title,
      detail: `${playerName(replay, replay.result.winnerId)} 지휘관의 승리입니다. ${finish.action}`
    });
  }

  return moments;
}

export function analyzeReplay(replay: GameReplay): ReplayAnalysis {
  const decisiveMoments = rankedMoments(replay)
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
    players: replay.players.map((player) => analyzePlayer(player, replay)),
    decisiveMoments
  };
}
