export type RoomStatus =
  | 'WAITING_FOR_OPPONENT'
  | 'WAITING_FOR_READY'
  | 'READY_TO_START'
  | 'PLACEMENT'
  | 'PLAYING'
  | 'FINISHED'
  | 'CANCELLED';

export type RoomVisibility = 'PUBLIC' | 'PRIVATE';
export type GameMode = 'CLASSIC' | 'RAPID' | 'SALVO';
export type ConnectionState = 'ONLINE' | 'RECONNECTING' | 'OFFLINE';
export type PlayerReadyState = 'NOT_READY' | 'READY';
export type PlayerRole = 'HOST' | 'GUEST';
export type PlayerKind = 'HUMAN' | 'AI';
export type AiDifficulty = 'RECRUIT' | 'OFFICER' | 'ADMIRAL';
export type ShipKind = 'CARRIER' | 'BATTLESHIP' | 'CRUISER' | 'SUBMARINE' | 'DESTROYER';
export type Orientation = 'HORIZONTAL' | 'VERTICAL';
export type AttackOutcome = 'MISS' | 'HIT' | 'SUNK';
export type FinishReason =
  'FLEET_DESTROYED' | 'SURRENDER' | 'TURN_TIMEOUT' | 'DISCONNECT_TIMEOUT' | 'PLAYER_LEFT';
export type WinType = 'NORMAL_VICTORY' | 'SURRENDER' | 'DISCONNECT' | 'TIMEOUT';
export type ChatMessageType = 'TEXT' | 'QUICK_COMMAND' | 'EMOJI' | 'SYSTEM';
export type QuickCommandId =
  | 'GOOD_GAME'
  | 'WAIT_A_MOMENT'
  | 'READY'
  | 'NICE_SHOT'
  | 'LUCKY'
  | 'GO_FIRST'
  | 'REMATCH'
  | 'THANK_YOU';

export interface Coordinate {
  row: number;
  col: number;
}

export interface ShipPlacement {
  kind: ShipKind;
  origin: Coordinate;
  orientation: Orientation;
}

export interface Session {
  id: string;
  accountId: string | null;
  nickname: string;
  currentRoomId: string | null;
  expiresAt: string;
}

export interface PlayerAccount {
  id: string;
  handle: string;
  createdAt: string;
}

export interface AccountSession {
  id: string;
  nickname: string;
  createdAt: string;
  lastSeenAt: string;
  currentRoomId: string | null;
}

export interface AchievementProgress {
  id: string;
  title: string;
  description: string;
  progress: number;
  target: number;
  unlocked: boolean;
}

export interface MissionProgress {
  id: string;
  cadence: 'DAILY' | 'WEEKLY';
  title: string;
  description: string;
  progress: number;
  target: number;
  rewardXp: number;
  completed: boolean;
  claimed: boolean;
  claimable: boolean;
}

export interface PlayerProgression {
  accountId: string | null;
  handle: string;
  level: number;
  rankTitle: string;
  totalXp: number;
  levelXp: number;
  xpToNextLevel: number;
  gamesPlayed: number;
  wins: number;
  losses: number;
  totalShots: number;
  totalHits: number;
  totalShipsSunk: number;
  achievements: AchievementProgress[];
  missions: MissionProgress[];
  calculatedAt: string;
}

export interface SocialRelationship {
  targetIdentityId: string;
  targetNickname: string;
  muted: boolean;
  blocked: boolean;
  updatedAt: string;
}

export type ReportCategory = 'CHAT' | 'NAME' | 'CHEATING' | 'STALLING' | 'OTHER';

export interface PlayerReportReceipt {
  reportId: string;
  status: 'OPEN';
  createdAt: string;
}

export type ReportStatus = 'OPEN' | 'REVIEWING' | 'ACTIONED' | 'DISMISSED';
export type ModerationActionKind = 'WARN' | 'SUSPEND' | 'BAN' | 'DISMISS' | 'REVERSE';

export interface PlayerReport {
  id: string;
  reporterIdentityId: string;
  targetIdentityId: string;
  roomId: string;
  targetPlayerId: string;
  targetNickname: string;
  category: ReportCategory;
  details: string;
  evidence: Record<string, unknown>;
  status: ReportStatus;
  createdAt: string;
  updatedAt: string;
}

export interface ModerationAction {
  id: string;
  reportId: string;
  targetIdentityId: string;
  operatorId: string;
  action: ModerationActionKind;
  reason: string;
  expiresAt: string | null;
  reversesActionId: string | null;
  createdAt: string;
}

export interface ModerationCase {
  report: PlayerReport;
  actions: ModerationAction[];
}

export interface ModerationCasePage {
  cases: ModerationCase[];
  nextBefore: string | null;
}

export type IntegritySignalKind =
  | 'IMPOSSIBLE_ORDER'
  | 'AUTOMATION'
  | 'COLLUSION'
  | 'INTENTIONAL_STALLING';

export interface IntegritySignal {
  id: string;
  subjectIdentityId: string;
  roomId: string | null;
  kind: IntegritySignalKind;
  severity: number;
  confidence: number;
  evidence: Record<string, unknown>;
  occurrences: number;
  firstObservedAt: string;
  lastObservedAt: string;
}

export interface IntegritySignalPage {
  signals: IntegritySignal[];
  nextBefore: string | null;
}

export interface MatchRules {
  mode: GameMode;
  turnDurationSeconds: number | null;
}

export interface RoomSummary {
  id: string;
  code: string;
  name: string;
  status: RoomStatus;
  rules: MatchRules;
  hostPlayerId: string;
  gameId: string | null;
  version: number;
  playerCount: number;
  capacity: number;
  createdAt: string;
}

export interface PlayerPublic {
  id: string;
  nickname: string;
  kind: PlayerKind;
  role: PlayerRole;
  isHost: boolean;
  placementConfirmed: boolean;
  readyState: PlayerReadyState;
  joinedAt: string;
  readyAt: string | null;
  consecutiveTimeoutCount: number;
  totalTimeoutCount: number;
  connectionState: ConnectionState;
}

export interface OwnShipSnapshot {
  kind: ShipKind;
  cells: Coordinate[];
  hits: Coordinate[];
  sunk: boolean;
}

export interface CellAttackSnapshot {
  coordinate: Coordinate;
  outcome: AttackOutcome;
}

export interface OwnBoardSnapshot {
  ships: OwnShipSnapshot[];
  attacksReceived: CellAttackSnapshot[];
}

export interface TargetAttackSnapshot {
  coordinate: Coordinate;
  outcome: AttackOutcome;
  sunkShip: ShipKind | null;
}

export interface TargetBoardSnapshot {
  attacks: TargetAttackSnapshot[];
}

export interface PlayerStatistics {
  playerId: string;
  shots: number;
  hits: number;
  shipsSunk: number;
  accuracy: number;
  totalTimeouts: number;
}

export interface GameResult {
  winnerId: string;
  loserId: string;
  totalTurns: number;
  durationSeconds: number;
  finishedAt: string;
  players: PlayerStatistics[];
  finishReason: FinishReason;
  winType: WinType;
}

export interface ChatMessage {
  messageId: string;
  roomId: string;
  playerId: string | null;
  nickname: string;
  content: string;
  timestamp: string;
  type: ChatMessageType;
  commandId: QuickCommandId | null;
}

export interface ChatTypingEvent {
  roomId: string;
  playerId: string;
  nickname: string;
  isTyping: boolean;
}

export interface SurrenderRecord {
  roomId: string;
  surrenderedPlayerId: string;
  winnerId: string;
  nickname: string;
  timestamp: string;
}

export interface PlayerReadyRecord {
  requestId: string;
  roomId: string;
  playerId: string;
  readyState: PlayerReadyState;
  roomState: RoomStatus;
  version: number;
  acceptedAt: string;
}

export interface GameStartRecord {
  requestId: string;
  roomId: string;
  gameId: string;
  startedBy: string;
  version: number;
  startedAt: string;
}

export interface GameTimerState {
  roomId: string;
  gameId: string;
  turnNumber: number;
  activePlayerId: string;
  gameStartedAt: string;
  turnStartedAt: string | null;
  turnDeadlineAt: string | null;
  turnDurationSeconds: number;
  serverTimestamp: string;
}

export interface TurnExpiredRecord {
  roomId: string;
  gameId: string;
  expiredTurnNumber: number;
  expiredPlayerId: string;
  nextPlayerId: string | null;
  consecutiveTimeoutCount: number;
  totalTimeoutCount: number;
  winnerId: string | null;
  expiredAt: string;
  serverTimestamp: string;
}

export interface GameSnapshot {
  protocolVersion: number;
  room: RoomSummary;
  roomId: string;
  roomState: RoomStatus;
  hostPlayerId: string;
  gameId: string | null;
  canStartGame: boolean;
  roomVersion: number;
  version: number;
  selfPlayerId: string;
  players: PlayerPublic[];
  practiceDifficulty: AiDifficulty | null;
  rules: MatchRules;
  ownBoard: OwnBoardSnapshot | null;
  targetBoard: TargetBoardSnapshot | null;
  revealedBoard: OwnBoardSnapshot | null;
  turnNumber: number | null;
  currentPlayerId: string | null;
  result: GameResult | null;
  reconnectDeadline: string | null;
  rematchRequestedBy: string[];
  placement: ShipPlacement[] | null;
  placementStartedAt: string | null;
  gameStartedAt: string | null;
  gameFinishedAt: string | null;
  turnStartedAt: string | null;
  turnDeadlineAt: string | null;
  turnDurationSeconds: number | null;
  shotsRemainingInTurn: number | null;
  serverTimestamp: string;
}

export interface AttackRecord {
  requestId: string;
  attackerId: string;
  targetId: string;
  coordinate: Coordinate;
  outcome: AttackOutcome;
  sunkShip: ShipKind | null;
  turnNumber: number;
  nextPlayerId: string | null;
  winnerId: string | null;
  shotsRemainingInTurn: number;
  resolvedVersion: number;
  createdAt: string;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  requestId?: string;
}

export interface ProtocolError extends ApiErrorBody {
  retryable: boolean;
}

export interface RoomCreatedResponse {
  snapshot: GameSnapshot;
  inviteUrl: string;
}

export interface HistoryItem {
  roomId: string;
  roomName: string;
  selfPlayerId: string;
  result: GameResult;
}

export interface ReplayShip {
  kind: ShipKind;
  cells: Coordinate[];
}

export interface ReplayPlayer {
  id: string;
  nickname: string;
  kind: PlayerKind;
  fleet: ReplayShip[];
}

export interface ReplayTurnExpiration {
  expiredTurnNumber: number;
  expiredPlayerId: string;
  nextPlayerId: string | null;
  consecutiveTimeoutCount: number;
  totalTimeoutCount: number;
  winnerId: string | null;
  expiredAt: string;
}

export type GameTimelineEvent =
  | { type: 'ATTACK'; payload: AttackRecord }
  | { type: 'TURN_EXPIRED'; payload: ReplayTurnExpiration };

export interface GameReplay {
  protocolVersion: number;
  rulesetVersion: number;
  roomId: string;
  roomName: string;
  gameId: string;
  firstPlayerId: string;
  startedAt: string;
  finishedAt: string;
  players: ReplayPlayer[];
  timeline: GameTimelineEvent[];
  result: GameResult;
}

export type ClientEvent =
  | { type: 'room:create'; payload: { name: string; visibility: RoomVisibility } }
  | { type: 'room:join'; payload: { code: string } }
  | { type: 'room:leave'; payload: { roomId: string } }
  | {
      type: 'player:ready';
      payload: { requestId: string; roomId: string; playerId: string };
    }
  | {
      type: 'ships:place';
      payload: { roomId: string; playerId: string; placements: ShipPlacement[] };
    }
  | {
      type: 'ships:confirm';
      payload: { roomId: string; playerId: string; placements: ShipPlacement[] };
    }
  | {
      type: 'player:unready';
      payload: { requestId: string; roomId: string; playerId: string };
    }
  | {
      type: 'game:start';
      payload: { requestId: string; roomId: string; playerId: string; roomVersion: number };
    }
  | {
      type: 'attack:fire';
      payload: {
        requestId: string;
        roomId: string;
        playerId: string;
        coordinate: Coordinate;
        expectedVersion: number;
        turnNumber: number;
      };
    }
  | { type: 'game:surrender'; payload: { roomId: string; playerId: string } }
  | {
      type: 'chat:send';
      payload: {
        roomId: string;
        clientMessageId: string;
        type: Exclude<ChatMessageType, 'SYSTEM'>;
        content: string | null;
        commandId: QuickCommandId | null;
      };
    }
  | { type: 'chat:typing'; payload: { roomId: string; isTyping: boolean } }
  | { type: 'game:rematch'; payload: { roomId: string } }
  | { type: 'game:sync'; payload: { roomId: string } }
  | { type: 'heartbeat'; payload: { clientTime: string } };

export type ServerEvent =
  | { type: 'room:created'; payload: RoomCreatedResponse }
  | {
      type:
        | 'room:updated'
        | 'player:joined'
        | 'player:left'
        | 'game:placement-started'
        | 'placement:accepted'
        | 'game:started'
        | 'turn:changed'
        | 'game:finished'
        | 'player:disconnected'
        | 'player:reconnected'
        | 'game:snapshot';
      payload: GameSnapshot;
    }
  | { type: 'placement:rejected' | 'error'; payload: ProtocolError }
  | { type: 'player:ready:accepted' | 'player:unready:accepted'; payload: PlayerReadyRecord }
  | { type: 'game:start:accepted'; payload: GameStartRecord }
  | {
      type:
        | 'player:ready:rejected'
        | 'player:unready:rejected'
        | 'game:start:rejected'
        | 'chat:rejected';
      payload: ProtocolError;
    }
  | { type: 'attack:result' | 'ship:sunk'; payload: AttackRecord }
  | { type: 'game:surrendered'; payload: SurrenderRecord }
  | { type: 'chat:message'; payload: ChatMessage }
  | { type: 'chat:history'; payload: { roomId: string; messages: ChatMessage[] } }
  | { type: 'chat:typing'; payload: ChatTypingEvent }
  | { type: 'turn:started' | 'game:timer-sync'; payload: GameTimerState }
  | { type: 'turn:expired'; payload: TurnExpiredRecord }
  | {
      type: 'matchmaking:queued' | 'matchmaking:cancelled';
      payload: { queued: boolean; queuedAt: string | null };
    }
  | { type: 'heartbeat'; payload: { serverTime: string } };

export const QUICK_COMMANDS: ReadonlyArray<{ id: QuickCommandId; label: string }> = [
  { id: 'GOOD_GAME', label: '굿게임' },
  { id: 'WAIT_A_MOMENT', label: '잠시만요' },
  { id: 'READY', label: '교전 준비 완료' },
  { id: 'NICE_SHOT', label: '나이스 샷' },
  { id: 'LUCKY', label: '운이 좋았군요' },
  { id: 'GO_FIRST', label: '제가 먼저 가겠습니다' },
  { id: 'REMATCH', label: '다시 한 판?' },
  { id: 'THANK_YOU', label: '감사합니다' }
] as const;

export const CHAT_EMOJIS = ['👍', '👏', '😅', '😮', '🔥', '🎯', '🚢', '💥', '🫡', '🤝'] as const;

export const FLEET: ReadonlyArray<{ kind: ShipKind; size: number; name: string }> = [
  { kind: 'CARRIER', size: 5, name: '항공모함' },
  { kind: 'BATTLESHIP', size: 4, name: '전함' },
  { kind: 'CRUISER', size: 3, name: '순양함' },
  { kind: 'SUBMARINE', size: 3, name: '잠수함' },
  { kind: 'DESTROYER', size: 2, name: '구축함' }
] as const;

export const ROW_LABELS = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J'] as const;

export function coordinateKey(coordinate: Coordinate): string {
  return `${coordinate.row}:${coordinate.col}`;
}

export function coordinateLabel(coordinate: Coordinate): string {
  return `${ROW_LABELS[coordinate.row] ?? '?'}${coordinate.col + 1}`;
}

export function shipName(kind: ShipKind): string {
  return FLEET.find((ship) => ship.kind === kind)?.name ?? kind;
}
