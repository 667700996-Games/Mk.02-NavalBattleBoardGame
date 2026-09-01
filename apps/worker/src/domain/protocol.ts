export const PROTOCOL_VERSION = 4;
export const MIN_PROTOCOL_VERSION = 3;
export const MAX_PROTOCOL_VERSION = 4;
export const PROTOCOL_CAPABILITIES = [
  "account-sessions-v1",
  "authoritative-room-v2",
  "balance-pin-v1",
  "explicit-lobby-readiness-v1",
  "ranked-seasons-v1",
  "safe-replay-analysis-v1",
  "tactical-skills-v1",
] as const;

export const PROTOCOL_HEADERS = {
  version: "x-mk01-protocol-version",
  minimum: "x-mk01-protocol-min-version",
  maximum: "x-mk01-protocol-max-version",
  capabilities: "x-mk01-protocol-capabilities",
} as const;

export const FLEET = [
  { kind: "CARRIER", cells: 5 },
  { kind: "BATTLESHIP", cells: 4 },
  { kind: "CRUISER", cells: 3 },
  { kind: "SUBMARINE", cells: 3 },
  { kind: "DESTROYER", cells: 2 },
] as const;

export type TacticalSkillKind =
  "RAPID_FIRE" | "CROSS_FIRE" | "AREA_ANNIHILATION";
export type TacticalSkillGrade = "C" | "B" | "A";
export type TacticalSkillTargetPattern =
  "TWO_TARGETS" | "ORTHOGONAL_CROSS" | "THREE_BY_THREE";

export interface TacticalSkillSpec {
  kind: TacticalSkillKind;
  grade: TacticalSkillGrade;
  usesPerMatch: number;
  maxCells: number;
  targetPattern: TacticalSkillTargetPattern;
}

export interface BalanceManifest {
  schemaVersion: number;
  rulesetVersion: number;
  label: string;
  boardSize: number;
  fleet: ReadonlyArray<{ kind: ShipKind; cells: number }>;
  classicShotsPerTurn: number;
  rapidTurnDurationSeconds: number;
  maximumTurnDurationSeconds: number;
  consecutiveTimeoutForfeit: number;
  salvoShotPolicy: "SURVIVING_SHIPS";
  turnAdvancePolicy: "AFTER_SHOT_ALLOWANCE";
  duplicateTargetPolicy: "REJECT";
  victoryCondition: "SINK_ALL_SHIPS";
  fleetRevealPolicy: "MATCH_COMPLETE";
  tacticalSkills?: {
    unlockTurn: number;
    maxSkillsPerTurn: number;
    skills: TacticalSkillSpec[];
  };
}

export interface BalancePin {
  rulesetVersion: number;
  checksum: string;
  manifest: BalanceManifest;
}

export const BALANCE_V1: BalancePin = {
  rulesetVersion: 1,
  checksum: "6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76",
  manifest: {
    schemaVersion: 1,
    rulesetVersion: 1,
    label: "Founders Fleet",
    boardSize: 10,
    fleet: FLEET,
    classicShotsPerTurn: 1,
    rapidTurnDurationSeconds: 30,
    maximumTurnDurationSeconds: 300,
    consecutiveTimeoutForfeit: 3,
    salvoShotPolicy: "SURVIVING_SHIPS",
    turnAdvancePolicy: "AFTER_SHOT_ALLOWANCE",
    duplicateTargetPolicy: "REJECT",
    victoryCondition: "SINK_ALL_SHIPS",
    fleetRevealPolicy: "MATCH_COMPLETE",
  },
};

export const BALANCE: BalancePin = {
  rulesetVersion: 2,
  checksum: "b73b72f6dfdba8020f21b86065aefd26c81645a8669932a38fcaa2abe976b8cd",
  manifest: {
    schemaVersion: 1,
    rulesetVersion: 2,
    label: "Tactical Fleet",
    boardSize: 10,
    fleet: FLEET,
    classicShotsPerTurn: 1,
    rapidTurnDurationSeconds: 30,
    maximumTurnDurationSeconds: 300,
    consecutiveTimeoutForfeit: 3,
    salvoShotPolicy: "SURVIVING_SHIPS",
    turnAdvancePolicy: "AFTER_SHOT_ALLOWANCE",
    duplicateTargetPolicy: "REJECT",
    victoryCondition: "SINK_ALL_SHIPS",
    fleetRevealPolicy: "MATCH_COMPLETE",
    tacticalSkills: {
      unlockTurn: 3,
      maxSkillsPerTurn: 1,
      skills: [
        {
          kind: "RAPID_FIRE",
          grade: "C",
          usesPerMatch: 3,
          maxCells: 2,
          targetPattern: "TWO_TARGETS",
        },
        {
          kind: "CROSS_FIRE",
          grade: "B",
          usesPerMatch: 2,
          maxCells: 5,
          targetPattern: "ORTHOGONAL_CROSS",
        },
        {
          kind: "AREA_ANNIHILATION",
          grade: "A",
          usesPerMatch: 1,
          maxCells: 9,
          targetPattern: "THREE_BY_THREE",
        },
      ],
    },
  },
};

export type RoomStatus =
  | "WAITING_FOR_OPPONENT"
  | "WAITING_FOR_READY"
  | "READY_TO_START"
  | "PLACEMENT"
  | "PLAYING"
  | "FINISHED"
  | "CANCELLED";
export type RoomVisibility = "PUBLIC" | "PRIVATE";
export type GameMode = "CLASSIC" | "RAPID" | "SALVO";
export type AiDifficulty = "RECRUIT" | "OFFICER" | "ADMIRAL";
export type MatchmakingPool = "CASUAL" | "RANKED";
export type MatchmakingRegion =
  | "AUTO"
  | "KOREA"
  | "JAPAN"
  | "SOUTHEAST_ASIA"
  | "NORTH_AMERICA_WEST"
  | "NORTH_AMERICA_EAST"
  | "EUROPE";
export type MatchmakingSearchPhase = "EXACT" | "REGIONAL" | "GLOBAL";
export type ConnectionState = "ONLINE" | "RECONNECTING" | "OFFLINE";
export type PlayerReadyState = "NOT_READY" | "READY";
export type ShipKind = (typeof FLEET)[number]["kind"];
export type Orientation = "HORIZONTAL" | "VERTICAL";
export type AttackOutcome = "MISS" | "HIT" | "SUNK";
export type ChatMessageType = "TEXT" | "QUICK_COMMAND" | "EMOJI" | "SYSTEM";
export type QuickCommandId =
  | "GOOD_GAME"
  | "WAIT_A_MOMENT"
  | "READY"
  | "NICE_SHOT"
  | "LUCKY"
  | "GO_FIRST"
  | "THANK_YOU";

export interface Coordinate {
  row: number;
  col: number;
}

export interface ShipPlacement {
  kind: ShipKind;
  origin: Coordinate;
  orientation: Orientation;
}

export interface MatchRules {
  mode: GameMode;
  turnDurationSeconds: number | null;
  tacticalSkillsEnabled: boolean;
}

export interface SessionRecord {
  id: string;
  accountId: string | null;
  nickname: string;
  tokenHash: string;
  createdAt: string;
  lastSeenAt: string;
  currentRoomId: string | null;
  expiresAt: string;
}

export interface PlayerAccount {
  id: string;
  handle: string;
  recoveryKeyHash: string;
  createdAt: string;
}

export interface RoomPlayer {
  id: string;
  sessionId: string;
  accountId?: string | null;
  nickname: string;
  kind: "HUMAN" | "AI";
  role: "HOST" | "GUEST";
  isHost: boolean;
  placementConfirmed: boolean;
  readyState: PlayerReadyState;
  connectionState: ConnectionState;
  joinedAt: string;
  readyAt: string | null;
}

export interface InternalShip {
  kind: ShipKind;
  cells: Coordinate[];
  hits: Coordinate[];
}

export interface InternalBoard {
  ships: InternalShip[];
  attacksReceived: Array<{ coordinate: Coordinate; outcome: AttackOutcome }>;
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

export interface TacticalSkillInventory {
  rapidFire: number;
  crossFire: number;
  areaAnnihilation: number;
}

export interface TacticalSkillCellResult {
  coordinate: Coordinate;
  outcome: AttackOutcome;
  sunkShip: ShipKind | null;
}

export interface TacticalSkillUseRecord {
  requestId: string;
  attackerId: string;
  targetId: string;
  skill: TacticalSkillKind;
  grade: TacticalSkillGrade;
  cells: TacticalSkillCellResult[];
  turnNumber: number;
  nextPlayerId: string | null;
  winnerId: string | null;
  shotsRemainingInTurn: number;
  remainingUses: number;
  resolvedVersion: number;
  createdAt: string;
}

export interface GameResult {
  winnerId: string;
  loserId: string;
  totalTurns: number;
  durationSeconds: number;
  finishedAt: string;
  players: Array<{
    playerId: string;
    shots: number;
    hits: number;
    shipsSunk: number;
    accuracy: number;
    totalTimeouts: number;
  }>;
  finishReason:
    | "FLEET_DESTROYED"
    | "SURRENDER"
    | "TURN_TIMEOUT"
    | "DISCONNECT_TIMEOUT"
    | "PLAYER_LEFT";
  winType: "NORMAL_VICTORY" | "SURRENDER" | "DISCONNECT" | "TIMEOUT";
}

export interface InternalGame {
  balance?: BalancePin;
  boards: Record<string, InternalBoard>;
  attacks: AttackRecord[];
  skillUses?: TacticalSkillUseRecord[];
  timeline?: GameTimelineEvent[];
  firstPlayerId: string;
  mode: GameMode;
  tacticalSkillsEnabled?: boolean;
  skillInventories?: Record<string, TacticalSkillInventory>;
  skillUsedTurns?: Record<string, number>;
  shotsRemainingInTurn: number;
  currentPlayerId: string;
  turnNumber: number;
  startedAt: string;
  turnDurationSeconds: number;
  turnStartedAt: string | null;
  turnDeadlineAt: string | null;
  consecutiveTimeoutCounts: Record<string, number>;
  totalTimeoutCounts: Record<string, number>;
  result: GameResult | null;
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

export interface InternalRoom {
  id: string;
  code: string;
  name: string;
  visibility: RoomVisibility;
  rules: MatchRules;
  balance?: BalancePin;
  status: RoomStatus;
  hostPlayerId: string;
  players: RoomPlayer[];
  pendingPlacements: Record<string, ShipPlacement[]>;
  gameId: string | null;
  game: InternalGame | null;
  version: number;
  createdAt: string;
  updatedAt: string;
  placementStartedAt: string | null;
  disconnectedDeadlines: Record<string, string>;
  chatMessages: ChatMessage[];
  readyResolutions: Record<string, PlayerReadyRecord>;
  startResolutions: Record<string, GameStartRecord>;
  chatRateWindows: Record<string, string[]>;
  chatBlockedUntil: Record<string, string>;
  lastQuickCommands: Record<
    string,
    { commandId: QuickCommandId; sentAt: string }
  >;
  practiceDifficulty?: AiDifficulty | null;
  matchmakingQuality?: MatchmakingQuality | null;
  rankedMatch?: { seasonId: string; contentRevision: number } | null;
  resultProjectionPending?: boolean;
}

export interface MatchmakingQuality {
  pool: MatchmakingPool;
  phase: MatchmakingSearchPhase;
  ratingDelta: number;
  maxReportedLatencyMs: number;
  partySize: number;
  recentPairings: number;
  rematchRelaxed: boolean;
  sharedWaitSeconds: number;
  waitSkewSeconds: number;
}

export interface MatchmakingSearchWindow {
  phase: MatchmakingSearchPhase;
  ratingDelta: number;
  maxLatencyMs: number;
  elapsedSeconds: number;
}

export interface MatchmakingTicket {
  pool: MatchmakingPool;
  region: MatchmakingRegion;
  reportedLatencyMs: number;
  rating: number | null;
  partySize: number;
  searchWindow: MatchmakingSearchWindow;
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
  | { type: "ATTACK"; payload: AttackRecord }
  | { type: "SKILL_ATTACK"; payload: TacticalSkillUseRecord }
  | { type: "TURN_EXPIRED"; payload: ReplayTurnExpiration };

export interface HistoryItem {
  roomId: string;
  roomName: string;
  selfPlayerId: string;
  balance: BalancePin;
  result: GameResult;
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

export interface SurrenderRecord {
  roomId: string;
  surrenderedPlayerId: string;
  winnerId: string;
  nickname: string;
  timestamp: string;
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

export type ClientEvent = { type: string; payload?: unknown };
export interface ServerEvent {
  type: string;
  payload: unknown;
}

const ERROR_MESSAGES = {
  INVALID_COORDINATE: "좌표는 A1부터 J10 사이여야 합니다.",
  PLACEMENT_OUT_OF_BOUNDS: "함선이 보드 경계를 벗어났습니다.",
  SHIPS_OVERLAP: "함선은 서로 겹칠 수 없습니다.",
  INCOMPLETE_FLEET: "모든 함선을 한 척씩 배치해 주세요.",
  INVALID_FLEET_COMPOSITION: "함선 구성이 올바르지 않습니다.",
  COORDINATE_ALREADY_ATTACKED: "이미 공격한 좌표입니다.",
  INVALID_STATE: "지금은 이 요청을 처리할 수 없는 게임 상태입니다.",
  NOT_YOUR_TURN: "현재 당신의 턴이 아닙니다.",
  VERSION_CONFLICT: "게임 상태가 갱신되었습니다. 최신 상태를 불러왔습니다.",
  TURN_CONFLICT: "턴 번호가 일치하지 않습니다.",
  TURN_EXPIRED: "현재 턴의 제한 시간이 이미 만료되었습니다.",
  TACTICAL_SKILLS_DISABLED: "이 방에서는 전술 스킬이 비활성화되어 있습니다.",
  TACTICAL_SKILL_LOCKED:
    "양쪽의 첫 공격 기회가 끝난 뒤에 전술 스킬을 사용할 수 있습니다.",
  TACTICAL_SKILL_EXHAUSTED: "해당 전술 스킬의 사용 횟수를 모두 소진했습니다.",
  TACTICAL_SKILL_ALREADY_USED: "이미 이번 턴에 전술 스킬을 사용했습니다.",
  INVALID_TACTICAL_SKILL_TARGETS: "전술 스킬의 표적 좌표가 올바르지 않습니다.",
  ROOM_NOT_FOUND: "방을 찾을 수 없습니다.",
  ROOM_FULL: "이미 두 명이 참가한 방입니다.",
  ROOM_ALREADY_STARTED: "이미 시작된 방에는 참가할 수 없습니다.",
  ALREADY_JOINED: "같은 세션으로 이 방에 중복 참가할 수 없습니다.",
  NOT_ROOM_MEMBER: "이 방의 플레이어가 아닙니다.",
  NOT_HOST: "방장만 게임을 시작할 수 있습니다.",
  PLAYERS_NOT_READY: "두 플레이어가 모두 준비를 완료해야 합니다.",
  PLAYER_COUNT_INVALID: "게임을 시작하려면 정확히 두 명이 필요합니다.",
  PLAYER_DISCONNECTED: "연결이 끊긴 플레이어가 있어 게임을 시작할 수 없습니다.",
  STALE_ROOM_VERSION: "방 상태가 변경되었습니다. 최신 상태를 확인해 주세요.",
  ROOM_STATE_INVALID: "현재 방 상태에서는 게임을 시작할 수 없습니다.",
  PLACEMENT_LOCKED: "배치를 확정한 뒤에는 함선을 변경할 수 없습니다.",
  PLACEMENT_MISMATCH:
    "제출한 함선 배치가 서버에 저장된 배치와 일치하지 않습니다.",
  GAME_ALREADY_STARTED: "게임이 이미 시작되었습니다.",
  INVALID_NICKNAME:
    "닉네임은 2~16자의 문자, 숫자, 공백, 밑줄 또는 하이픈만 사용할 수 있습니다.",
  INVALID_ROOM_NAME: "방 이름은 2~32자로 입력해 주세요.",
  DUPLICATE_NICKNAME: "같은 닉네임을 사용 중인 플레이어가 있습니다.",
  ACCOUNT_HANDLE_TAKEN: "이 계정 핸들은 이미 사용 중입니다.",
  UNAUTHORIZED: "인증 세션이 없거나 만료되었습니다.",
  RANKED_ACCOUNT_REQUIRED:
    "랭크 매칭은 계정으로 로그인한 지휘관만 이용할 수 있습니다.",
  PLAYER_BLOCKED: "차단된 지휘관과는 이 작업을 수행할 수 없습니다.",
  ACCOUNT_SUSPENDED: "이 계정은 일시 정지되었습니다.",
  ACCOUNT_BANNED: "이 계정은 이용이 금지되었습니다.",
  REPORT_NOT_FOUND: "신고 사례를 찾을 수 없습니다.",
  SUPPORT_ACCOUNT_NOT_FOUND: "지원 도구에서 계정을 찾을 수 없습니다.",
  LIVE_CONTENT_REVISION_NOT_FOUND: "라이브 콘텐츠 리비전을 찾을 수 없습니다.",
  LIVE_CONTENT_REVISION_CONFLICT:
    "라이브 콘텐츠가 다른 운영자에 의해 갱신되었습니다.",
  SERVER_PROTOCOL_MISMATCH:
    "이 클라이언트 프로토콜 버전은 현재 릴리스 창에서 지원되지 않습니다.",
  INVALID_REQUEST: "요청 형식이 올바르지 않습니다.",
  RATE_LIMITED: "요청이 너무 잦습니다. 잠시 후 다시 시도해 주세요.",
  INVALID_CHAT_MESSAGE: "채팅 메시지는 1~300자의 일반 텍스트로 입력해 주세요.",
  INVALID_QUICK_COMMAND: "허용되지 않은 빠른 명령입니다.",
  INVALID_EMOJI: "허용되지 않은 이모지입니다.",
  INTERNAL_ERROR: "서버에서 요청을 처리하지 못했습니다.",
} as const;

export type ErrorCode = keyof typeof ERROR_MESSAGES;

export class DomainError extends Error {
  constructor(public readonly code: ErrorCode) {
    super(ERROR_MESSAGES[code]);
    this.name = "DomainError";
  }
}

export function protocolError(
  error: unknown,
  requestId: string = crypto.randomUUID(),
) {
  const resolved =
    error instanceof DomainError ? error : new DomainError("INTERNAL_ERROR");
  return {
    code: resolved.code,
    message: resolved.message,
    retryable: [
      "VERSION_CONFLICT",
      "TURN_CONFLICT",
      "TURN_EXPIRED",
      "STALE_ROOM_VERSION",
    ].includes(resolved.code),
    requestId,
  };
}

export function statusForError(code: ErrorCode): number {
  if (code === "UNAUTHORIZED") return 401;
  if (
    [
      "RANKED_ACCOUNT_REQUIRED",
      "PLAYER_BLOCKED",
      "ACCOUNT_SUSPENDED",
      "ACCOUNT_BANNED",
    ].includes(code)
  )
    return 403;
  if (code === "SERVER_PROTOCOL_MISMATCH") return 426;
  if (code === "ROOM_NOT_FOUND") return 404;
  if (
    [
      "ROOM_FULL",
      "ROOM_ALREADY_STARTED",
      "ALREADY_JOINED",
      "DUPLICATE_NICKNAME",
      "ACCOUNT_HANDLE_TAKEN",
      "COORDINATE_ALREADY_ATTACKED",
      "VERSION_CONFLICT",
      "STALE_ROOM_VERSION",
      "TURN_CONFLICT",
      "TURN_EXPIRED",
      "PLACEMENT_LOCKED",
      "LIVE_CONTENT_REVISION_CONFLICT",
    ].includes(code)
  )
    return 409;
  if (code === "RATE_LIMITED") return 429;
  if (code === "INTERNAL_ERROR") return 500;
  return 400;
}
