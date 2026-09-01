import {
  BALANCE,
  BALANCE_V1,
  DomainError,
  FLEET,
  PROTOCOL_VERSION,
  type AttackOutcome,
  type AttackRecord,
  type BalancePin,
  type AiDifficulty,
  type ChatMessage,
  type ChatMessageType,
  type Coordinate,
  type GameResult,
  type GameTimelineEvent,
  type GameStartRecord,
  type InternalBoard,
  type InternalGame,
  type InternalRoom,
  type MatchRules,
  type PlayerReadyRecord,
  type QuickCommandId,
  type RoomPlayer,
  type RoomSummary,
  type RoomVisibility,
  type SessionRecord,
  type ShipKind,
  type ShipPlacement,
  type SurrenderRecord,
  type TacticalSkillCellResult,
  type TacticalSkillInventory,
  type TacticalSkillKind,
  type TacticalSkillUseRecord,
} from "./protocol";

const SPECTATOR_DELAY_SECONDS = 30;

const ALLOWED_EMOJIS = new Set([
  "👍",
  "👏",
  "😅",
  "😮",
  "🔥",
  "🎯",
  "🚢",
  "💥",
  "🫡",
  "🤝",
]);
const QUICK_COMMAND_LABELS: Record<QuickCommandId, string> = {
  GOOD_GAME: "굿게임",
  WAIT_A_MOMENT: "잠시만요",
  READY: "교전 준비 완료",
  NICE_SHOT: "나이스 샷",
  LUCKY: "운이 좋았군요",
  GO_FIRST: "제가 먼저 가겠습니다",
  THANK_YOU: "감사합니다",
};

export interface CreateRoomCommand {
  roomId: string;
  code: string;
  name: string;
  visibility: RoomVisibility;
  rules?: Partial<MatchRules> | null;
  session: SessionRecord;
  playerId: string;
  now: string;
}

function balanceFor(room: InternalRoom): BalancePin {
  return room.balance ?? room.game?.balance ?? BALANCE_V1;
}

function gameBalance(game: InternalGame): BalancePin {
  return game.balance ?? BALANCE_V1;
}

function inventoryFor(
  skills: ReadonlyArray<{ kind: TacticalSkillKind; usesPerMatch: number }>,
): TacticalSkillInventory {
  const inventory: TacticalSkillInventory = {
    rapidFire: 0,
    crossFire: 0,
    areaAnnihilation: 0,
  };
  for (const skill of skills) {
    setRemainingUses(inventory, skill.kind, skill.usesPerMatch);
  }
  return inventory;
}

function remainingUses(
  inventory: TacticalSkillInventory,
  skill: TacticalSkillKind,
): number {
  if (skill === "RAPID_FIRE") return inventory.rapidFire;
  if (skill === "CROSS_FIRE") return inventory.crossFire;
  return inventory.areaAnnihilation;
}

function setRemainingUses(
  inventory: TacticalSkillInventory,
  skill: TacticalSkillKind,
  value: number,
): void {
  if (skill === "RAPID_FIRE") inventory.rapidFire = value;
  else if (skill === "CROSS_FIRE") inventory.crossFire = value;
  else inventory.areaAnnihilation = value;
}

const PRACTICE_FLEET: ShipPlacement[] = [
  {
    kind: "CARRIER",
    origin: { row: 0, col: 0 },
    orientation: "HORIZONTAL",
  },
  {
    kind: "BATTLESHIP",
    origin: { row: 2, col: 0 },
    orientation: "HORIZONTAL",
  },
  {
    kind: "CRUISER",
    origin: { row: 4, col: 0 },
    orientation: "HORIZONTAL",
  },
  {
    kind: "SUBMARINE",
    origin: { row: 6, col: 0 },
    orientation: "HORIZONTAL",
  },
  {
    kind: "DESTROYER",
    origin: { row: 8, col: 0 },
    orientation: "HORIZONTAL",
  },
];

export function validateNickname(nickname: unknown): string {
  if (typeof nickname !== "string") throw new DomainError("INVALID_NICKNAME");
  const trimmed = nickname.trim();
  if (
    [...trimmed].length < 2 ||
    [...trimmed].length > 16 ||
    !/^[\p{L}\p{N} _-]+$/u.test(trimmed)
  ) {
    throw new DomainError("INVALID_NICKNAME");
  }
  return trimmed;
}

export function validateHandle(handle: unknown): string {
  if (typeof handle !== "string") throw new DomainError("INVALID_REQUEST");
  const normalized = handle.trim();
  if (!/^[\p{L}\p{N}_-]{3,20}$/u.test(normalized)) {
    throw new DomainError("INVALID_REQUEST");
  }
  return normalized;
}

function validatedRules(input?: Partial<MatchRules> | null): MatchRules {
  const mode = input?.mode ?? "CLASSIC";
  const turnDurationSeconds = input?.turnDurationSeconds ?? null;
  const tacticalSkillsEnabled = input?.tacticalSkillsEnabled ?? false;
  if (!["CLASSIC", "RAPID", "SALVO"].includes(mode)) {
    throw new DomainError("INVALID_REQUEST");
  }
  if (
    turnDurationSeconds !== null &&
    (!Number.isInteger(turnDurationSeconds) ||
      turnDurationSeconds < 0 ||
      turnDurationSeconds > BALANCE.manifest.maximumTurnDurationSeconds)
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  if (typeof tacticalSkillsEnabled !== "boolean") {
    throw new DomainError("INVALID_REQUEST");
  }
  return { mode, turnDurationSeconds, tacticalSkillsEnabled };
}

function newPlayer(
  session: SessionRecord,
  playerId: string,
  isHost: boolean,
  now: string,
): RoomPlayer {
  return {
    id: playerId,
    sessionId: session.id,
    accountId: session.accountId,
    nickname: session.nickname,
    kind: "HUMAN",
    role: isHost ? "HOST" : "GUEST",
    isHost,
    placementConfirmed: false,
    readyState: "NOT_READY",
    connectionState: "ONLINE",
    joinedAt: now,
    readyAt: null,
  };
}

export function createRoom(command: CreateRoomCommand): InternalRoom {
  const name = command.name.trim();
  if ([...name].length < 2 || [...name].length > 32) {
    throw new DomainError("INVALID_ROOM_NAME");
  }
  if (!["PUBLIC", "PRIVATE"].includes(command.visibility)) {
    throw new DomainError("INVALID_REQUEST");
  }
  const host = newPlayer(command.session, command.playerId, true, command.now);
  const room: InternalRoom = {
    id: command.roomId,
    code: command.code,
    name,
    visibility: command.visibility,
    rules: validatedRules(command.rules),
    balance: structuredClone(BALANCE),
    status: "WAITING_FOR_OPPONENT",
    hostPlayerId: host.id,
    players: [host],
    pendingPlacements: {},
    gameId: null,
    game: null,
    version: 1,
    createdAt: command.now,
    updatedAt: command.now,
    placementStartedAt: null,
    disconnectedDeadlines: {},
    chatMessages: [],
    readyResolutions: {},
    startResolutions: {},
    chatRateWindows: {},
    chatBlockedUntil: {},
    lastQuickCommands: {},
    practiceDifficulty: null,
    matchmakingQuality: null,
    rankedMatch: null,
    resultProjectionPending: false,
  };
  pushSystemMessage(
    room,
    `${host.nickname} 지휘관이 작전실에 입장했습니다.`,
    command.now,
  );
  return room;
}

export function createPracticeRoom(
  command: CreateRoomCommand,
  difficulty: AiDifficulty,
  aiSession: SessionRecord,
  aiPlayerId: string,
): InternalRoom {
  if (!["RECRUIT", "OFFICER", "ADMIRAL"].includes(difficulty)) {
    throw new DomainError("INVALID_REQUEST");
  }
  const room = createRoom(command);
  joinRoom(room, aiSession, aiPlayerId, command.now);
  const ai = playerForSession(room, aiSession.id);
  ai.kind = "AI";
  room.practiceDifficulty = difficulty;
  setLobbyReady(
    room,
    command.session.id,
    crypto.randomUUID(),
    room.hostPlayerId,
    true,
    command.now,
  );
  setLobbyReady(
    room,
    aiSession.id,
    crypto.randomUUID(),
    ai.id,
    true,
    command.now,
  );
  startPlacement(
    room,
    command.session.id,
    crypto.randomUUID(),
    room.hostPlayerId,
    room.version,
    crypto.randomUUID(),
    command.now,
  );
  placeShips(room, aiSession.id, PRACTICE_FLEET, command.now);
  confirmPlacement(
    room,
    aiSession.id,
    PRACTICE_FLEET,
    60,
    room.hostPlayerId,
    command.now,
  );
  pushSystemMessage(
    room,
    `AI training opponent connected at ${difficulty} difficulty.`,
    command.now,
  );
  return room;
}

export function joinRoom(
  room: InternalRoom,
  session: SessionRecord,
  playerId: string,
  now: string,
): string {
  if (room.status !== "WAITING_FOR_OPPONENT") {
    throw new DomainError(
      room.players.length >= 2 ? "ROOM_FULL" : "ROOM_ALREADY_STARTED",
    );
  }
  if (room.players.length >= 2) throw new DomainError("ROOM_FULL");
  if (room.players.some((player) => player.sessionId === session.id)) {
    throw new DomainError("ALREADY_JOINED");
  }
  if (
    room.players.some(
      (player) =>
        player.nickname.toLocaleLowerCase() ===
        session.nickname.toLocaleLowerCase(),
    )
  ) {
    throw new DomainError("DUPLICATE_NICKNAME");
  }
  room.players.push(newPlayer(session, playerId, false, now));
  room.status = "WAITING_FOR_READY";
  bump(room, now);
  pushSystemMessage(
    room,
    `${session.nickname} 지휘관이 작전실에 입장했습니다.`,
    now,
  );
  return playerId;
}

export function playerForSession(
  room: InternalRoom,
  sessionId: string,
): RoomPlayer {
  const player = room.players.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!player) throw new DomainError("NOT_ROOM_MEMBER");
  return player;
}

function refreshLobbyStatus(room: InternalRoom): void {
  room.status =
    room.players.length < 2
      ? "WAITING_FOR_OPPONENT"
      : room.players.every((player) => player.readyState === "READY")
        ? "READY_TO_START"
        : "WAITING_FOR_READY";
}

export function setLobbyReady(
  room: InternalRoom,
  sessionId: string,
  requestId: string,
  claimedPlayerId: string,
  ready: boolean,
  now: string,
): { record: PlayerReadyRecord; duplicate: boolean } {
  const player = playerForSession(room, sessionId);
  if (player.id !== claimedPlayerId) throw new DomainError("UNAUTHORIZED");
  const previous = room.readyResolutions[requestId];
  const nextState = ready ? "READY" : "NOT_READY";
  if (previous) {
    if (previous.playerId === player.id && previous.readyState === nextState) {
      return { record: previous, duplicate: true };
    }
    throw new DomainError("UNAUTHORIZED");
  }
  if (
    room.gameId ||
    !["WAITING_FOR_OPPONENT", "WAITING_FOR_READY", "READY_TO_START"].includes(
      room.status,
    )
  ) {
    throw new DomainError("GAME_ALREADY_STARTED");
  }
  if (player.readyState !== nextState) {
    player.readyState = nextState;
    player.readyAt = ready ? now : null;
    refreshLobbyStatus(room);
    bump(room, now);
    pushSystemMessage(
      room,
      ready
        ? `${player.nickname} 지휘관이 준비를 완료했습니다.`
        : `${player.nickname} 지휘관이 준비를 취소했습니다.`,
      now,
    );
    if (ready && room.status === "READY_TO_START") {
      pushSystemMessage(room, "모든 지휘관의 준비가 완료되었습니다.", now);
    }
  }
  const record: PlayerReadyRecord = {
    requestId,
    roomId: room.id,
    playerId: player.id,
    readyState: nextState,
    roomState: room.status,
    version: room.version,
    acceptedAt: now,
  };
  rememberResolution(room.readyResolutions, record, 128, "acceptedAt");
  return { record, duplicate: false };
}

export function startPlacement(
  room: InternalRoom,
  sessionId: string,
  requestId: string,
  claimedPlayerId: string,
  expectedVersion: number,
  gameId: string,
  now: string,
): { record: GameStartRecord; duplicate: boolean } {
  const player = playerForSession(room, sessionId);
  if (player.id !== claimedPlayerId) throw new DomainError("UNAUTHORIZED");
  const previous = room.startResolutions[requestId];
  if (previous) {
    if (previous.startedBy === player.id)
      return { record: previous, duplicate: true };
    throw new DomainError("UNAUTHORIZED");
  }
  if (player.id !== room.hostPlayerId) throw new DomainError("NOT_HOST");
  if (room.gameId || ["PLACEMENT", "PLAYING"].includes(room.status)) {
    throw new DomainError("GAME_ALREADY_STARTED");
  }
  if (room.version !== expectedVersion)
    throw new DomainError("STALE_ROOM_VERSION");
  if (room.players.length !== 2) throw new DomainError("PLAYER_COUNT_INVALID");
  if (!room.players.every((candidate) => candidate.readyState === "READY")) {
    throw new DomainError("PLAYERS_NOT_READY");
  }
  if (
    !room.players.every((candidate) => candidate.connectionState === "ONLINE")
  ) {
    throw new DomainError("PLAYER_DISCONNECTED");
  }
  if (room.status !== "READY_TO_START")
    throw new DomainError("ROOM_STATE_INVALID");

  room.status = "PLACEMENT";
  room.gameId = gameId;
  room.placementStartedAt = now;
  room.pendingPlacements = {};
  for (const candidate of room.players) candidate.placementConfirmed = false;
  bump(room, now);
  pushSystemMessage(
    room,
    "방장이 작전을 시작했습니다. 함선 배치 채널을 개방합니다.",
    now,
  );
  const record: GameStartRecord = {
    requestId,
    roomId: room.id,
    gameId,
    startedBy: player.id,
    version: room.version,
    startedAt: now,
  };
  rememberResolution(room.startResolutions, record, 64, "startedAt");
  return { record, duplicate: false };
}

export function placeShips(
  room: InternalRoom,
  sessionId: string,
  placements: ShipPlacement[],
  now: string,
): void {
  if (room.status !== "PLACEMENT") throw new DomainError("INVALID_STATE");
  const player = playerForSession(room, sessionId);
  if (player.placementConfirmed) throw new DomainError("PLACEMENT_LOCKED");
  boardFromPlacements(placements, balanceFor(room));
  room.pendingPlacements[player.id] = structuredClone(placements);
  bump(room, now);
}

export function confirmPlacement(
  room: InternalRoom,
  sessionId: string,
  submittedPlacements: ShipPlacement[],
  fallbackTurnDurationSeconds: number,
  firstPlayerId: string,
  now: string,
): boolean {
  if (room.status !== "PLACEMENT" || !room.gameId)
    throw new DomainError("INVALID_STATE");
  const player = playerForSession(room, sessionId);
  const stored = room.pendingPlacements[player.id];
  if (!stored) throw new DomainError("INCOMPLETE_FLEET");
  boardFromPlacements(submittedPlacements, balanceFor(room));
  if (JSON.stringify(stored) !== JSON.stringify(submittedPlacements)) {
    throw new DomainError("PLACEMENT_MISMATCH");
  }
  if (player.placementConfirmed) return false;
  player.placementConfirmed = true;
  bump(room, now);

  if (
    room.players.length !== 2 ||
    !room.players.every((candidate) => candidate.placementConfirmed)
  ) {
    return false;
  }
  if (!room.players.some((candidate) => candidate.id === firstPlayerId)) {
    throw new DomainError("INVALID_STATE");
  }
  const boards: Record<string, InternalBoard> = {};
  for (const candidate of room.players) {
    const placements = room.pendingPlacements[candidate.id];
    if (!placements) throw new DomainError("INCOMPLETE_FLEET");
    boards[candidate.id] = boardFromPlacements(placements, balanceFor(room));
  }
  const balance = balanceFor(room);
  const duration = resolvedTurnDuration(
    room.rules,
    fallbackTurnDurationSeconds,
    balance,
  );
  const skillInventories: Record<string, TacticalSkillInventory> = {};
  if (room.rules.tacticalSkillsEnabled) {
    const skillRules = balance.manifest.tacticalSkills;
    if (!skillRules) throw new DomainError("INVALID_STATE");
    for (const candidate of room.players) {
      skillInventories[candidate.id] = inventoryFor(skillRules.skills);
    }
  }
  room.game = {
    balance: structuredClone(balance),
    boards,
    attacks: [],
    skillUses: [],
    timeline: [],
    firstPlayerId,
    mode: room.rules.mode,
    tacticalSkillsEnabled: room.rules.tacticalSkillsEnabled,
    skillInventories,
    skillUsedTurns: {},
    shotsRemainingInTurn: shotsFor(
      boards,
      firstPlayerId,
      room.rules.mode,
      balance,
    ),
    currentPlayerId: firstPlayerId,
    turnNumber: 1,
    startedAt: now,
    turnDurationSeconds: duration,
    turnStartedAt: now,
    turnDeadlineAt: deadline(now, duration),
    consecutiveTimeoutCounts: {},
    totalTimeoutCounts: {},
    result: null,
  };
  room.status = "PLAYING";
  room.pendingPlacements = {};
  bump(room, now);
  pushSystemMessage(
    room,
    "게임이 시작되었습니다. 전투 채널을 개방합니다.",
    now,
  );
  return true;
}

export function fire(
  room: InternalRoom,
  sessionId: string,
  requestId: string,
  claimedPlayerId: string,
  coordinate: Coordinate,
  expectedVersion: number,
  expectedTurn: number,
  now: string,
): { record: AttackRecord; duplicate: boolean } {
  const player = playerForSession(room, sessionId);
  if (player.id !== claimedPlayerId) throw new DomainError("UNAUTHORIZED");
  const previous = room.game?.attacks.find(
    (attack) =>
      attack.requestId === requestId && attack.attackerId === player.id,
  );
  if (previous) return { record: previous, duplicate: true };
  if (room.status !== "PLAYING" || !room.game)
    throw new DomainError("INVALID_STATE");
  if (room.version !== expectedVersion)
    throw new DomainError("VERSION_CONFLICT");
  const game = room.game;
  if (game.result) throw new DomainError("INVALID_STATE");
  if (game.currentPlayerId !== player.id)
    throw new DomainError("NOT_YOUR_TURN");
  if (game.turnNumber !== expectedTurn) throw new DomainError("TURN_CONFLICT");
  if (
    game.turnDeadlineAt &&
    Date.parse(now) >= Date.parse(game.turnDeadlineAt)
  ) {
    throw new DomainError("TURN_EXPIRED");
  }
  validateCoordinate(coordinate, gameBalance(game));
  const target = room.players.find((candidate) => candidate.id !== player.id);
  if (!target) throw new DomainError("INVALID_STATE");
  const board = game.boards[target.id];
  if (!board) throw new DomainError("INVALID_STATE");
  if (
    board.attacksReceived.some((attack) =>
      sameCoordinate(attack.coordinate, coordinate),
    )
  ) {
    throw new DomainError("COORDINATE_ALREADY_ATTACKED");
  }
  let outcome: AttackOutcome = "MISS";
  let sunkShip: ShipKind | null = null;
  const ship = board.ships.find((candidate) =>
    candidate.cells.some((cell) => sameCoordinate(cell, coordinate)),
  );
  if (ship) {
    ship.hits.push({ ...coordinate });
    if (
      ship.cells.every((cell) =>
        ship.hits.some((hit) => sameCoordinate(hit, cell)),
      )
    ) {
      outcome = "SUNK";
      sunkShip = ship.kind;
    } else {
      outcome = "HIT";
    }
  }
  board.attacksReceived.push({ coordinate: { ...coordinate }, outcome });
  const allSunk = board.ships.every((candidate) =>
    candidate.cells.every((cell) =>
      candidate.hits.some((hit) => sameCoordinate(hit, cell)),
    ),
  );
  const winnerId = allSunk ? player.id : null;
  const continuesSalvo =
    !winnerId && game.mode === "SALVO" && game.shotsRemainingInTurn > 1;
  const nextPlayerId = winnerId ? null : continuesSalvo ? player.id : target.id;
  const shotsRemainingInTurn = winnerId
    ? 0
    : continuesSalvo
      ? game.shotsRemainingInTurn - 1
      : shotsFor(game.boards, target.id, game.mode, gameBalance(game));
  const record: AttackRecord = {
    requestId,
    attackerId: player.id,
    targetId: target.id,
    coordinate: { ...coordinate },
    outcome,
    sunkShip,
    turnNumber: game.turnNumber,
    nextPlayerId,
    winnerId,
    shotsRemainingInTurn,
    resolvedVersion: room.version + 1,
    createdAt: now,
  };
  game.attacks.push(record);
  game.timeline ??= [];
  game.timeline.push({ type: "ATTACK", payload: structuredClone(record) });
  game.consecutiveTimeoutCounts[player.id] = 0;
  if (winnerId) {
    finishGame(
      room,
      winnerId,
      target.id,
      "FLEET_DESTROYED",
      "NORMAL_VICTORY",
      now,
    );
    game.shotsRemainingInTurn = 0;
    room.status = "FINISHED";
  } else if (continuesSalvo) {
    game.shotsRemainingInTurn -= 1;
  } else {
    game.currentPlayerId = target.id;
    game.turnNumber += 1;
    startTurn(game, now);
  }
  bump(room, now);
  if (winnerId) {
    pushSystemMessage(
      room,
      `게임이 종료되었습니다. ${player.nickname} 지휘관이 적 함대를 전멸시켰습니다.`,
      now,
    );
  }
  return { record, duplicate: false };
}

export function fireSkill(
  room: InternalRoom,
  sessionId: string,
  requestId: string,
  claimedPlayerId: string,
  skill: TacticalSkillKind,
  targets: Coordinate[],
  expectedVersion: number,
  expectedTurn: number,
  now: string,
): { record: TacticalSkillUseRecord; duplicate: boolean } {
  const player = playerForSession(room, sessionId);
  if (player.id !== claimedPlayerId) throw new DomainError("UNAUTHORIZED");
  const previous = room.game?.skillUses?.find(
    (record) =>
      record.requestId === requestId && record.attackerId === player.id,
  );
  if (previous) return { record: previous, duplicate: true };
  if (room.status !== "PLAYING" || !room.game)
    throw new DomainError("INVALID_STATE");
  if (room.version !== expectedVersion)
    throw new DomainError("VERSION_CONFLICT");
  const game = room.game;
  if (!game.tacticalSkillsEnabled || !room.rules.tacticalSkillsEnabled) {
    throw new DomainError("TACTICAL_SKILLS_DISABLED");
  }
  if (game.result) throw new DomainError("INVALID_STATE");
  if (game.currentPlayerId !== player.id)
    throw new DomainError("NOT_YOUR_TURN");
  if (game.turnNumber !== expectedTurn) throw new DomainError("TURN_CONFLICT");
  if (
    game.turnDeadlineAt &&
    Date.parse(now) >= Date.parse(game.turnDeadlineAt)
  ) {
    throw new DomainError("TURN_EXPIRED");
  }
  const balance = gameBalance(game);
  const skillRules = balance.manifest.tacticalSkills;
  if (!skillRules) throw new DomainError("INVALID_STATE");
  if (game.turnNumber < skillRules.unlockTurn) {
    throw new DomainError("TACTICAL_SKILL_LOCKED");
  }
  game.skillUsedTurns ??= {};
  if (game.skillUsedTurns[player.id] === game.turnNumber) {
    throw new DomainError("TACTICAL_SKILL_ALREADY_USED");
  }
  const spec = skillRules.skills.find((candidate) => candidate.kind === skill);
  if (!spec) throw new DomainError("INVALID_REQUEST");
  const inventory = game.skillInventories?.[player.id];
  if (!inventory) throw new DomainError("INVALID_STATE");
  if (remainingUses(inventory, skill) <= 0) {
    throw new DomainError("TACTICAL_SKILL_EXHAUSTED");
  }
  const target = room.players.find((candidate) => candidate.id !== player.id);
  if (!target) throw new DomainError("INVALID_STATE");
  const board = game.boards[target.id];
  if (!board) throw new DomainError("INVALID_STATE");
  let coordinates = skillCoordinates(skill, targets, balance);
  if (
    skill === "RAPID_FIRE" &&
    coordinates.some((coordinate) => wasAttacked(board, coordinate))
  ) {
    throw new DomainError("INVALID_TACTICAL_SKILL_TARGETS");
  }
  coordinates = coordinates.filter(
    (coordinate) => !wasAttacked(board, coordinate),
  );
  const cells = coordinates.map((coordinate) =>
    attackBoardCell(board, coordinate),
  );
  const winnerId = board.ships.every((candidate) =>
    candidate.cells.every((cell) =>
      candidate.hits.some((hit) => sameCoordinate(hit, cell)),
    ),
  )
    ? player.id
    : null;
  const uses = remainingUses(inventory, skill) - 1;
  setRemainingUses(inventory, skill, uses);
  game.skillUsedTurns[player.id] = game.turnNumber;
  const continuesSalvo =
    !winnerId && game.mode === "SALVO" && game.shotsRemainingInTurn > 1;
  const nextPlayerId = winnerId ? null : continuesSalvo ? player.id : target.id;
  const shotsRemainingInTurn = winnerId
    ? 0
    : continuesSalvo
      ? game.shotsRemainingInTurn - 1
      : shotsFor(game.boards, target.id, game.mode, balance);
  const record: TacticalSkillUseRecord = {
    requestId,
    attackerId: player.id,
    targetId: target.id,
    skill,
    grade: spec.grade,
    cells,
    turnNumber: game.turnNumber,
    nextPlayerId,
    winnerId,
    shotsRemainingInTurn,
    remainingUses: uses,
    resolvedVersion: room.version + 1,
    createdAt: now,
  };
  game.skillUses ??= [];
  game.skillUses.push(record);
  game.timeline ??= [];
  game.timeline.push({
    type: "SKILL_ATTACK",
    payload: structuredClone(record),
  });
  game.consecutiveTimeoutCounts[player.id] = 0;
  if (winnerId) {
    finishGame(
      room,
      winnerId,
      target.id,
      "FLEET_DESTROYED",
      "NORMAL_VICTORY",
      now,
    );
    game.shotsRemainingInTurn = 0;
    room.status = "FINISHED";
  } else if (continuesSalvo) {
    game.shotsRemainingInTurn -= 1;
  } else {
    game.currentPlayerId = target.id;
    game.turnNumber += 1;
    startTurn(game, now);
  }
  bump(room, now);
  if (winnerId) {
    pushSystemMessage(
      room,
      `게임이 종료되었습니다. ${player.nickname} 지휘관이 적 함대를 전멸시켰습니다.`,
      now,
    );
  }
  return { record, duplicate: false };
}

export function selectAiCoordinate(
  room: InternalRoom,
  aiPlayerId: string,
): Coordinate | null {
  const game = room.game;
  if (!game) return null;
  const balance = gameBalance(game);
  const used = new Set(
    game.attacks
      .filter((attack) => attack.attackerId === aiPlayerId)
      .map((attack) => `${attack.coordinate.row}:${attack.coordinate.col}`),
  );
  const difficulty = room.practiceDifficulty ?? "RECRUIT";
  if (difficulty !== "RECRUIT") {
    for (const attack of [...game.attacks].reverse()) {
      if (attack.attackerId !== aiPlayerId || attack.outcome !== "HIT")
        continue;
      for (const [rowOffset, colOffset] of [
        [-1, 0],
        [0, 1],
        [1, 0],
        [0, -1],
      ] as const) {
        const row = attack.coordinate.row + rowOffset;
        const col = attack.coordinate.col + colOffset;
        if (
          row >= 0 &&
          row < balance.manifest.boardSize &&
          col >= 0 &&
          col < balance.manifest.boardSize &&
          !used.has(`${row}:${col}`)
        ) {
          return { row, col };
        }
      }
    }
  }
  let candidates: Coordinate[] = [];
  for (let row = 0; row < balance.manifest.boardSize; row += 1) {
    for (let col = 0; col < balance.manifest.boardSize; col += 1) {
      if (!used.has(`${row}:${col}`)) candidates.push({ row, col });
    }
  }
  if (difficulty === "ADMIRAL") {
    const parity = candidates.filter(({ row, col }) => (row + col) % 2 === 0);
    if (parity.length) candidates = parity;
  }
  if (!candidates.length) return null;
  const roomSeed = [...room.id].reduce(
    (seed, character) => (seed * 33 + character.charCodeAt(0)) >>> 0,
    5381,
  );
  const index = (roomSeed + Math.imul(game.turnNumber, 2_654_435_761)) >>> 0;
  return candidates[index % candidates.length] ?? null;
}

export function surrender(
  room: InternalRoom,
  sessionId: string,
  claimedPlayerId: string,
  now: string,
): SurrenderRecord {
  const player = playerForSession(room, sessionId);
  if (player.id !== claimedPlayerId) throw new DomainError("UNAUTHORIZED");
  if (room.status !== "PLAYING" || !room.game || room.game.result) {
    throw new DomainError("INVALID_STATE");
  }
  const winner = room.players.find((candidate) => candidate.id !== player.id);
  if (!winner) throw new DomainError("INVALID_STATE");
  finishGame(room, winner.id, player.id, "SURRENDER", "SURRENDER", now);
  room.status = "FINISHED";
  room.disconnectedDeadlines = {};
  bump(room, now);
  pushSystemMessage(
    room,
    `Commander ${player.nickname} surrendered. ${winner.nickname} 지휘관이 승리했습니다.`,
    now,
  );
  return {
    roomId: room.id,
    surrenderedPlayerId: player.id,
    winnerId: winner.id,
    nickname: player.nickname,
    timestamp: now,
  };
}

export function sendChat(
  room: InternalRoom,
  sessionId: string,
  clientMessageId: string,
  messageType: ChatMessageType,
  content: string | null,
  commandId: QuickCommandId | null,
  now: string,
): { message: ChatMessage; duplicate: boolean } {
  if (
    ![
      "WAITING_FOR_OPPONENT",
      "WAITING_FOR_READY",
      "READY_TO_START",
      "PLACEMENT",
      "PLAYING",
      "FINISHED",
    ].includes(room.status)
  ) {
    throw new DomainError("INVALID_STATE");
  }
  const player = playerForSession(room, sessionId);
  const previous = room.chatMessages.find(
    (message) => message.messageId === clientMessageId,
  );
  if (previous) {
    if (previous.playerId === player.id)
      return { message: previous, duplicate: true };
    throw new DomainError("UNAUTHORIZED");
  }
  let normalized: string;
  let resolvedCommand: QuickCommandId | null = null;
  if (messageType === "TEXT") {
    if (typeof content !== "string")
      throw new DomainError("INVALID_CHAT_MESSAGE");
    normalized = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n").trim();
    if (
      [...normalized].length < 1 ||
      [...normalized].length > 300 ||
      /[<>]/.test(normalized) ||
      [...normalized].some((character) =>
        /[\u0000-\u0009\u000b-\u001f\u007f]/.test(character),
      )
    ) {
      throw new DomainError("INVALID_CHAT_MESSAGE");
    }
  } else if (messageType === "EMOJI") {
    if (
      commandId ||
      typeof content !== "string" ||
      !ALLOWED_EMOJIS.has(content)
    ) {
      throw new DomainError("INVALID_EMOJI");
    }
    normalized = content;
  } else if (messageType === "QUICK_COMMAND") {
    if (
      content !== null ||
      !commandId ||
      !(commandId in QUICK_COMMAND_LABELS)
    ) {
      throw new DomainError("INVALID_QUICK_COMMAND");
    }
    const previousQuick = room.lastQuickCommands[player.id];
    if (
      previousQuick?.commandId === commandId &&
      Date.parse(now) - Date.parse(previousQuick.sentAt) < 2_000
    ) {
      throw new DomainError("RATE_LIMITED");
    }
    normalized = QUICK_COMMAND_LABELS[commandId];
    resolvedCommand = commandId;
  } else {
    throw new DomainError("INVALID_CHAT_MESSAGE");
  }
  if (Date.parse(room.chatBlockedUntil[player.id] ?? "") > Date.parse(now)) {
    throw new DomainError("RATE_LIMITED");
  }
  const window = (room.chatRateWindows[player.id] ?? []).filter(
    (sentAt) => Date.parse(now) - Date.parse(sentAt) < 10_000,
  );
  const recent = window.filter(
    (sentAt) => Date.parse(now) - Date.parse(sentAt) < 2_000,
  ).length;
  if (window.length >= 8 || recent >= 3) {
    room.chatBlockedUntil[player.id] = new Date(
      Date.parse(now) + 3_000,
    ).toISOString();
    throw new DomainError("RATE_LIMITED");
  }
  window.push(now);
  room.chatRateWindows[player.id] = window;
  if (resolvedCommand) {
    room.lastQuickCommands[player.id] = {
      commandId: resolvedCommand,
      sentAt: now,
    };
  }
  const message: ChatMessage = {
    messageId: clientMessageId,
    roomId: room.id,
    playerId: player.id,
    nickname: player.nickname,
    content: normalized,
    timestamp: now,
    type: messageType,
    commandId: resolvedCommand,
  };
  appendChat(room, message, now);
  return { message, duplicate: false };
}

export function reconnect(
  room: InternalRoom,
  sessionId: string,
  now: string,
): boolean {
  const player = playerForSession(room, sessionId);
  const changed =
    player.connectionState !== "ONLINE" ||
    room.disconnectedDeadlines[player.id] !== undefined;
  if (!changed) return false;
  player.connectionState = "ONLINE";
  delete room.disconnectedDeadlines[player.id];
  bump(room, now);
  pushSystemMessage(
    room,
    `${player.nickname} 지휘관이 전투 채널에 재접속했습니다.`,
    now,
  );
  return true;
}

export function disconnect(
  room: InternalRoom,
  sessionId: string,
  graceSeconds: number,
  now: string,
): string | null {
  if (["FINISHED", "CANCELLED"].includes(room.status)) return null;
  const player = playerForSession(room, sessionId);
  if (room.disconnectedDeadlines[player.id]) return null;
  player.connectionState = "RECONNECTING";
  const reconnectDeadline = new Date(
    Date.parse(now) + graceSeconds * 1_000,
  ).toISOString();
  room.disconnectedDeadlines[player.id] = reconnectDeadline;
  bump(room, now);
  pushSystemMessage(
    room,
    `${player.nickname} 지휘관의 연결이 끊겼습니다. 재접속을 기다립니다.`,
    now,
  );
  return reconnectDeadline;
}

export function expireDisconnects(room: InternalRoom, now: string): boolean {
  let changed = false;
  for (const [playerId, expiresAt] of Object.entries(
    room.disconnectedDeadlines,
  )) {
    if (Date.parse(expiresAt) > Date.parse(now)) continue;
    const player = room.players.find((candidate) => candidate.id === playerId);
    if (!player) {
      delete room.disconnectedDeadlines[playerId];
      continue;
    }
    player.connectionState = "OFFLINE";
    const opponent = room.players.find(
      (candidate) => candidate.id !== playerId,
    );
    const lobby =
      ["WAITING_FOR_OPPONENT", "WAITING_FOR_READY", "READY_TO_START"].includes(
        room.status,
      ) && !room.gameId;
    if (lobby && playerId !== room.hostPlayerId) {
      room.players = room.players.filter(
        (candidate) => candidate.id !== playerId,
      );
      resetLobby(room);
    } else if (lobby) {
      room.status = "CANCELLED";
    } else if (room.game && opponent && !room.game.result) {
      finishGame(
        room,
        opponent.id,
        playerId,
        "DISCONNECT_TIMEOUT",
        "DISCONNECT",
        now,
      );
      room.status = "FINISHED";
    } else if (!["FINISHED", "CANCELLED"].includes(room.status)) {
      room.status = "CANCELLED";
    }
    delete room.disconnectedDeadlines[playerId];
    bump(room, now);
    pushSystemMessage(
      room,
      `${player.nickname} 지휘관의 재접속 시간이 만료되었습니다.`,
      now,
    );
    changed = true;
  }
  return changed;
}

export function expireTurn(
  room: InternalRoom,
  now: string,
): null | {
  expiredTurnNumber: number;
  expiredPlayerId: string;
  nextPlayerId: string | null;
  consecutiveTimeoutCount: number;
  totalTimeoutCount: number;
  winnerId: string | null;
  expiredAt: string;
} {
  const game = room.game;
  if (
    room.status !== "PLAYING" ||
    !game ||
    game.result ||
    !game.turnDeadlineAt ||
    Date.parse(now) < Date.parse(game.turnDeadlineAt)
  ) {
    return null;
  }
  const expiredPlayerId = game.currentPlayerId;
  const next = room.players.find((player) => player.id !== expiredPlayerId);
  if (!next) throw new DomainError("INVALID_STATE");
  const consecutive = (game.consecutiveTimeoutCounts[expiredPlayerId] ?? 0) + 1;
  const total = (game.totalTimeoutCounts[expiredPlayerId] ?? 0) + 1;
  game.consecutiveTimeoutCounts[expiredPlayerId] = consecutive;
  game.totalTimeoutCounts[expiredPlayerId] = total;
  const winnerId =
    consecutive >= gameBalance(game).manifest.consecutiveTimeoutForfeit
      ? next.id
      : null;
  const expiredTurnNumber = game.turnNumber;
  if (winnerId) {
    finishGame(room, winnerId, expiredPlayerId, "TURN_TIMEOUT", "TIMEOUT", now);
    room.status = "FINISHED";
    room.disconnectedDeadlines = {};
  } else {
    game.currentPlayerId = next.id;
    game.turnNumber += 1;
    startTurn(game, now);
  }
  bump(room, now);
  pushSystemMessage(
    room,
    `${playerName(room, expiredPlayerId)} 지휘관의 작전 시간이 만료되었습니다.`,
    now,
  );
  const expiration = {
    expiredTurnNumber,
    expiredPlayerId,
    nextPlayerId: winnerId ? null : next.id,
    consecutiveTimeoutCount: consecutive,
    totalTimeoutCount: total,
    winnerId,
    expiredAt: now,
  };
  game.timeline ??= [];
  game.timeline.push({
    type: "TURN_EXPIRED",
    payload: structuredClone(expiration),
  });
  return expiration;
}

export function leaveRoom(
  room: InternalRoom,
  sessionId: string,
  now: string,
): void {
  const player = playerForSession(room, sessionId);
  const lobby =
    ["WAITING_FOR_OPPONENT", "WAITING_FOR_READY", "READY_TO_START"].includes(
      room.status,
    ) && !room.gameId;
  if (lobby) {
    if (player.id === room.hostPlayerId) {
      room.status = "CANCELLED";
      player.connectionState = "OFFLINE";
    } else {
      room.players = room.players.filter(
        (candidate) => candidate.id !== player.id,
      );
      resetLobby(room);
    }
  } else {
    const opponent = room.players.find(
      (candidate) => candidate.id !== player.id,
    );
    if (
      room.status === "PLAYING" &&
      room.game &&
      !room.game.result &&
      opponent
    ) {
      finishGame(
        room,
        opponent.id,
        player.id,
        "PLAYER_LEFT",
        "DISCONNECT",
        now,
      );
      room.status = "FINISHED";
    } else if (!["FINISHED", "CANCELLED"].includes(room.status)) {
      room.status = "CANCELLED";
    }
    player.connectionState = "OFFLINE";
  }
  bump(room, now);
  pushSystemMessage(
    room,
    `${player.nickname} 지휘관이 작전실에서 퇴장했습니다.`,
    now,
  );
}

export function roomSummary(room: InternalRoom): RoomSummary {
  return {
    id: room.id,
    code: room.code,
    name: room.name,
    status: room.status,
    rules: room.rules,
    hostPlayerId: room.hostPlayerId,
    gameId: room.gameId,
    version: room.version,
    playerCount: room.players.length,
    capacity: 2,
    createdAt: room.createdAt,
  };
}

export function snapshotFor(
  room: InternalRoom,
  sessionId: string,
  now: string,
) {
  const me = playerForSession(room, sessionId);
  const game = room.game;
  const ownBoard = game?.boards[me.id];
  const opponent = room.players.find((player) => player.id !== me.id);
  const opponentBoard = opponent && game?.boards[opponent.id];
  const reconnectDeadline =
    Object.values(room.disconnectedDeadlines).sort()[0] ?? null;
  return {
    protocolVersion: PROTOCOL_VERSION,
    balance: balanceFor(room),
    room: roomSummary(room),
    roomId: room.id,
    roomState: room.status,
    hostPlayerId: room.hostPlayerId,
    gameId: room.gameId,
    canStartGame:
      me.id === room.hostPlayerId &&
      room.status === "READY_TO_START" &&
      !room.gameId &&
      room.players.length === 2 &&
      room.players.every(
        (player) =>
          player.readyState === "READY" && player.connectionState === "ONLINE",
      ),
    roomVersion: room.version,
    version: room.version,
    selfPlayerId: me.id,
    players: room.players.map((player) => ({
      id: player.id,
      nickname: player.nickname,
      kind: player.kind,
      role: player.role,
      isHost: player.isHost,
      placementConfirmed: player.placementConfirmed,
      readyState: player.readyState,
      joinedAt: player.joinedAt,
      readyAt: player.readyAt,
      consecutiveTimeoutCount: game?.consecutiveTimeoutCounts[player.id] ?? 0,
      totalTimeoutCount: game?.totalTimeoutCounts[player.id] ?? 0,
      connectionState: player.connectionState,
    })),
    practiceDifficulty: room.practiceDifficulty ?? null,
    matchmakingQuality: room.matchmakingQuality ?? null,
    rankedMatch: room.rankedMatch ?? null,
    rules: room.rules,
    ownBoard: ownBoard ? projectOwnBoard(ownBoard) : null,
    targetBoard: game
      ? {
          attacks: targetAttacksFor(game, me.id),
        }
      : null,
    revealedBoard:
      room.status === "FINISHED" && opponentBoard
        ? projectOwnBoard(opponentBoard)
        : null,
    turnNumber: game?.turnNumber ?? null,
    currentPlayerId: game?.currentPlayerId ?? null,
    result: game?.result ?? null,
    reconnectDeadline,
    placement: room.pendingPlacements[me.id] ?? null,
    placementStartedAt: room.placementStartedAt,
    gameStartedAt: game?.startedAt ?? null,
    gameFinishedAt: game?.result?.finishedAt ?? null,
    turnStartedAt: game?.turnStartedAt ?? null,
    turnDeadlineAt: game?.turnDeadlineAt ?? null,
    turnDurationSeconds: game?.turnDurationSeconds ?? null,
    shotsRemainingInTurn: game?.shotsRemainingInTurn ?? null,
    skillInventories: game?.skillInventories ?? {},
    skillUsedThisTurn: game?.skillUsedTurns?.[me.id] === game?.turnNumber,
    skillUnlockTurn:
      balanceFor(room).manifest.tacticalSkills?.unlockTurn ?? null,
    serverTimestamp: now,
  };
}

export function timerState(room: InternalRoom, now: string) {
  const game = room.game;
  if (!game || game.result || !room.gameId) return null;
  return {
    roomId: room.id,
    gameId: room.gameId,
    turnNumber: game.turnNumber,
    activePlayerId: game.currentPlayerId,
    gameStartedAt: game.startedAt,
    turnStartedAt: game.turnStartedAt,
    turnDeadlineAt: game.turnDeadlineAt,
    turnDurationSeconds: game.turnDurationSeconds,
    serverTimestamp: now,
  };
}

export function replayFor(room: InternalRoom, sessionId: string) {
  playerForSession(room, sessionId);
  if (room.status !== "FINISHED" || !room.game?.result || !room.gameId) {
    throw new DomainError("INVALID_STATE");
  }
  return {
    protocolVersion: PROTOCOL_VERSION,
    rulesetVersion: balanceFor(room).rulesetVersion,
    balance: balanceFor(room),
    roomId: room.id,
    roomName: room.name,
    gameId: room.gameId,
    firstPlayerId: room.game.firstPlayerId,
    startedAt: room.game.startedAt,
    finishedAt: room.game.result.finishedAt,
    players: room.players.map((player) => {
      const board = room.game?.boards[player.id];
      if (!board) throw new DomainError("INVALID_STATE");
      return {
        id: player.id,
        nickname: player.nickname,
        kind: player.kind,
        fleet: board.ships.map((ship) => ({
          kind: ship.kind,
          cells: ship.cells,
        })),
      };
    }),
    timeline: timelineFor(room.game),
    result: room.game.result,
  };
}

export function spectatorSnapshot(room: InternalRoom, now: string) {
  if (room.visibility !== "PUBLIC") throw new DomainError("ROOM_NOT_FOUND");
  if (room.status !== "PLAYING") throw new DomainError("ROOM_NOT_FOUND");
  if (!room.game || !room.gameId) throw new DomainError("INVALID_STATE");
  const visibleThrough = new Date(
    Date.parse(now) - SPECTATOR_DELAY_SECONDS * 1_000,
  ).toISOString();
  const timeline = timelineFor(room.game).filter(
    (event) => eventTimestamp(event) <= visibleThrough,
  );
  const visibleResult =
    room.game.result && room.game.result.finishedAt <= visibleThrough
      ? room.game.result
      : null;
  const phase =
    room.game.startedAt > visibleThrough
      ? "DELAYED"
      : visibleResult
        ? "FINISHED"
        : "LIVE";
  const currentPlayerId =
    phase === "LIVE"
      ? (timeline.at(-1)?.payload.nextPlayerId ?? room.game.firstPlayerId)
      : null;
  return {
    protocolVersion: PROTOCOL_VERSION,
    delaySeconds: SPECTATOR_DELAY_SECONDS,
    visibleThrough,
    room: {
      ...roomSummary(room),
      status:
        phase === "DELAYED"
          ? "PLACEMENT"
          : phase === "FINISHED"
            ? "FINISHED"
            : "PLAYING",
    },
    gameId: room.gameId,
    phase,
    players: room.players.map((player) => ({
      id: player.id,
      nickname: player.nickname,
      kind: player.kind,
    })),
    balance: balanceFor(room),
    rules: room.rules,
    timeline,
    currentPlayerId,
    result: visibleResult,
    serverTimestamp: now,
  };
}

function timelineFor(game: InternalGame): GameTimelineEvent[] {
  return game.timeline?.length
    ? structuredClone(game.timeline)
    : game.attacks.map((attack) => ({
        type: "ATTACK" as const,
        payload: structuredClone(attack),
      }));
}

function targetAttacksFor(game: InternalGame, playerId: string) {
  if (!game.timeline?.length) {
    return game.attacks
      .filter((attack) => attack.attackerId === playerId)
      .map((attack) => ({
        coordinate: attack.coordinate,
        outcome: attack.outcome,
        sunkShip: attack.sunkShip,
      }));
  }
  return game.timeline.flatMap((event) => {
    if (event.type === "ATTACK" && event.payload.attackerId === playerId) {
      return [
        {
          coordinate: event.payload.coordinate,
          outcome: event.payload.outcome,
          sunkShip: event.payload.sunkShip,
        },
      ];
    }
    if (
      event.type === "SKILL_ATTACK" &&
      event.payload.attackerId === playerId
    ) {
      return event.payload.cells.map((cell) => ({
        coordinate: cell.coordinate,
        outcome: cell.outcome,
        sunkShip: cell.sunkShip,
      }));
    }
    return [];
  });
}

function eventTimestamp(event: GameTimelineEvent): string {
  return event.type === "TURN_EXPIRED"
    ? event.payload.expiredAt
    : event.payload.createdAt;
}

function projectOwnBoard(board: InternalBoard) {
  return {
    ships: board.ships.map((ship) => ({
      kind: ship.kind,
      cells: ship.cells,
      hits: ship.hits,
      sunk: ship.cells.every((cell) =>
        ship.hits.some((hit) => sameCoordinate(hit, cell)),
      ),
    })),
    attacksReceived: board.attacksReceived,
  };
}

function boardFromPlacements(
  placements: ShipPlacement[],
  balance: BalancePin = BALANCE,
): InternalBoard {
  if (!Array.isArray(placements) || placements.length !== FLEET.length) {
    throw new DomainError("INCOMPLETE_FLEET");
  }
  const expected = new Map(
    balance.manifest.fleet.map((ship) => [ship.kind, ship.cells]),
  );
  const seen = new Set<ShipKind>();
  const occupied = new Set<string>();
  const ships = placements.map((placement) => {
    if (!expected.has(placement.kind) || seen.has(placement.kind)) {
      throw new DomainError("INVALID_FLEET_COMPOSITION");
    }
    seen.add(placement.kind);
    if (!["HORIZONTAL", "VERTICAL"].includes(placement.orientation)) {
      throw new DomainError("INVALID_REQUEST");
    }
    validateCoordinate(placement.origin, balance);
    const cells: Coordinate[] = [];
    for (
      let offset = 0;
      offset < (expected.get(placement.kind) ?? 0);
      offset += 1
    ) {
      const cell = {
        row:
          placement.origin.row +
          (placement.orientation === "VERTICAL" ? offset : 0),
        col:
          placement.origin.col +
          (placement.orientation === "HORIZONTAL" ? offset : 0),
      };
      if (
        cell.row >= balance.manifest.boardSize ||
        cell.col >= balance.manifest.boardSize
      ) {
        throw new DomainError("PLACEMENT_OUT_OF_BOUNDS");
      }
      const key = `${cell.row}:${cell.col}`;
      if (occupied.has(key)) throw new DomainError("SHIPS_OVERLAP");
      occupied.add(key);
      cells.push(cell);
    }
    return { kind: placement.kind, cells, hits: [] };
  });
  if (seen.size !== FLEET.length)
    throw new DomainError("INVALID_FLEET_COMPOSITION");
  return { ships, attacksReceived: [] };
}

function validateCoordinate(
  coordinate: Coordinate,
  balance: BalancePin = BALANCE,
): void {
  if (
    !coordinate ||
    !Number.isInteger(coordinate.row) ||
    !Number.isInteger(coordinate.col) ||
    coordinate.row < 0 ||
    coordinate.col < 0 ||
    coordinate.row >= balance.manifest.boardSize ||
    coordinate.col >= balance.manifest.boardSize
  ) {
    throw new DomainError("INVALID_COORDINATE");
  }
}

function sameCoordinate(left: Coordinate, right: Coordinate): boolean {
  return left.row === right.row && left.col === right.col;
}

function wasAttacked(board: InternalBoard, coordinate: Coordinate): boolean {
  return board.attacksReceived.some((attack) =>
    sameCoordinate(attack.coordinate, coordinate),
  );
}

function attackBoardCell(
  board: InternalBoard,
  coordinate: Coordinate,
): TacticalSkillCellResult {
  let outcome: AttackOutcome = "MISS";
  let sunkShip: ShipKind | null = null;
  const ship = board.ships.find((candidate) =>
    candidate.cells.some((cell) => sameCoordinate(cell, coordinate)),
  );
  if (ship) {
    ship.hits.push({ ...coordinate });
    if (
      ship.cells.every((cell) =>
        ship.hits.some((hit) => sameCoordinate(hit, cell)),
      )
    ) {
      outcome = "SUNK";
      sunkShip = ship.kind;
    } else {
      outcome = "HIT";
    }
  }
  board.attacksReceived.push({ coordinate: { ...coordinate }, outcome });
  return { coordinate: { ...coordinate }, outcome, sunkShip };
}

function skillCoordinates(
  skill: TacticalSkillKind,
  targets: Coordinate[],
  balance: BalancePin,
): Coordinate[] {
  if (
    !Array.isArray(targets) ||
    (skill === "RAPID_FIRE" ? targets.length !== 2 : targets.length !== 1)
  ) {
    throw new DomainError("INVALID_TACTICAL_SKILL_TARGETS");
  }
  for (const coordinate of targets) {
    try {
      validateCoordinate(coordinate, balance);
    } catch {
      throw new DomainError("INVALID_TACTICAL_SKILL_TARGETS");
    }
  }
  if (
    skill === "RAPID_FIRE" &&
    sameCoordinate(targets[0]!, targets[1]!)
  ) {
    throw new DomainError("INVALID_TACTICAL_SKILL_TARGETS");
  }
  if (skill === "RAPID_FIRE") {
    return targets
      .map((coordinate) => ({ ...coordinate }))
      .sort((left, right) => left.row - right.row || left.col - right.col);
  }
  const center = targets[0]!;
  const offsets =
    skill === "CROSS_FIRE"
      ? [
          [0, 0],
          [-1, 0],
          [0, -1],
          [0, 1],
          [1, 0],
        ]
      : [
          [-1, -1],
          [-1, 0],
          [-1, 1],
          [0, -1],
          [0, 0],
          [0, 1],
          [1, -1],
          [1, 0],
          [1, 1],
        ];
  return offsets
    .map(([rowOffset, colOffset]) => ({
      row: center.row + (rowOffset ?? 0),
      col: center.col + (colOffset ?? 0),
    }))
    .filter(
      (coordinate) =>
        coordinate.row >= 0 &&
        coordinate.col >= 0 &&
        coordinate.row < balance.manifest.boardSize &&
        coordinate.col < balance.manifest.boardSize,
    )
    .sort((left, right) => left.row - right.row || left.col - right.col);
}

function shotsFor(
  boards: Record<string, InternalBoard>,
  playerId: string,
  mode: MatchRules["mode"],
  balance: BalancePin = BALANCE,
): number {
  if (mode !== "SALVO") return balance.manifest.classicShotsPerTurn;
  return Math.max(
    1,
    boards[playerId]?.ships.filter(
      (ship) =>
        !ship.cells.every((cell) =>
          ship.hits.some((hit) => sameCoordinate(hit, cell)),
        ),
    ).length ?? 1,
  );
}

function resolvedTurnDuration(
  rules: MatchRules,
  fallback: number,
  balance: BalancePin = BALANCE,
): number {
  return rules.mode === "RAPID"
    ? balance.manifest.rapidTurnDurationSeconds
    : Math.min(
        rules.turnDurationSeconds ?? fallback,
        balance.manifest.maximumTurnDurationSeconds,
      );
}

function startTurn(game: InternalGame, now: string): void {
  game.shotsRemainingInTurn = shotsFor(
    game.boards,
    game.currentPlayerId,
    game.mode,
    gameBalance(game),
  );
  game.turnStartedAt = now;
  game.turnDeadlineAt = deadline(now, game.turnDurationSeconds);
}

function deadline(now: string, seconds: number): string | null {
  return seconds > 0
    ? new Date(Date.parse(now) + seconds * 1_000).toISOString()
    : null;
}

function finishGame(
  room: InternalRoom,
  winnerId: string,
  loserId: string,
  finishReason: GameResult["finishReason"],
  winType: GameResult["winType"],
  now: string,
): void {
  const game = room.game;
  if (!game || game.result) throw new DomainError("INVALID_STATE");
  game.turnDeadlineAt = null;
  game.result = {
    winnerId,
    loserId,
    totalTurns: game.turnNumber,
    durationSeconds: Math.max(
      0,
      Math.floor((Date.parse(now) - Date.parse(game.startedAt)) / 1_000),
    ),
    finishedAt: now,
    players: room.players.map((player) => {
      const attacks = game.attacks.filter(
        (attack) => attack.attackerId === player.id,
      );
      const skillCells = (game.skillUses ?? [])
        .filter((record) => record.attackerId === player.id)
        .flatMap((record) => record.cells);
      const shots = attacks.length + skillCells.length;
      const hits =
        attacks.filter((attack) => attack.outcome !== "MISS").length +
        skillCells.filter((cell) => cell.outcome !== "MISS").length;
      return {
        playerId: player.id,
        shots,
        hits,
        shipsSunk:
          attacks.filter((attack) => attack.sunkShip).length +
          skillCells.filter((cell) => cell.sunkShip).length,
        accuracy: shots ? hits / shots : 0,
        totalTimeouts: game.totalTimeoutCounts[player.id] ?? 0,
      };
    }),
    finishReason,
    winType,
  };
  room.resultProjectionPending = true;
}

function appendChat(
  room: InternalRoom,
  message: ChatMessage,
  now: string,
): void {
  room.chatMessages.push(message);
  room.chatMessages = room.chatMessages.slice(-100);
  room.updatedAt = now;
}

function pushSystemMessage(
  room: InternalRoom,
  content: string,
  now: string,
): void {
  appendChat(
    room,
    {
      messageId: crypto.randomUUID(),
      roomId: room.id,
      playerId: null,
      nickname: "SYSTEM",
      content,
      timestamp: now,
      type: "SYSTEM",
      commandId: null,
    },
    now,
  );
}

function bump(room: InternalRoom, now: string): void {
  room.version += 1;
  room.updatedAt = now;
}

function resetLobby(room: InternalRoom): void {
  room.pendingPlacements = {};
  room.game = null;
  room.gameId = null;
  room.placementStartedAt = null;
  room.readyResolutions = {};
  room.startResolutions = {};
  room.disconnectedDeadlines = {};
  for (const player of room.players) {
    player.readyState = "NOT_READY";
    player.readyAt = null;
    player.placementConfirmed = false;
  }
  room.status = "WAITING_FOR_OPPONENT";
}

function rememberResolution<T extends { requestId: string }>(
  collection: Record<string, T>,
  record: T,
  maximum: number,
  dateKey: keyof T,
): void {
  const entries = Object.entries(collection);
  if (entries.length >= maximum) {
    entries.sort((left, right) =>
      String(left[1][dateKey]).localeCompare(String(right[1][dateKey])),
    );
    if (entries[0]) delete collection[entries[0][0]];
  }
  collection[record.requestId] = record;
}

function playerName(room: InternalRoom, playerId: string): string {
  return (
    room.players.find((player) => player.id === playerId)?.nickname ?? "상대"
  );
}
