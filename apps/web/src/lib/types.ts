export type RoomStatus =
  'WAITING' | 'PLACEMENT' | 'READY' | 'PLAYING' | 'DISCONNECTED' | 'FINISHED' | 'CANCELLED';

export type RoomVisibility = 'PUBLIC' | 'PRIVATE';
export type ConnectionState = 'ONLINE' | 'RECONNECTING' | 'OFFLINE';
export type ShipKind = 'CARRIER' | 'BATTLESHIP' | 'CRUISER' | 'SUBMARINE' | 'DESTROYER';
export type Orientation = 'HORIZONTAL' | 'VERTICAL';
export type AttackOutcome = 'MISS' | 'HIT' | 'SUNK';
export type FinishReason = 'FLEET_DESTROYED' | 'DISCONNECT_TIMEOUT' | 'PLAYER_LEFT';

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
  nickname: string;
  currentRoomId: string | null;
  expiresAt: string;
}

export interface RoomSummary {
  id: string;
  code: string;
  name: string;
  status: RoomStatus;
  playerCount: number;
  capacity: number;
  createdAt: string;
}

export interface PlayerPublic {
  id: string;
  nickname: string;
  isHost: boolean;
  isReady: boolean;
  placementConfirmed: boolean;
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
}

export interface GameResult {
  winnerId: string;
  loserId: string;
  totalTurns: number;
  durationSeconds: number;
  finishedAt: string;
  players: PlayerStatistics[];
  finishReason: FinishReason;
}

export interface GameSnapshot {
  room: RoomSummary;
  version: number;
  selfPlayerId: string;
  players: PlayerPublic[];
  ownBoard: OwnBoardSnapshot | null;
  targetBoard: TargetBoardSnapshot | null;
  turnNumber: number | null;
  currentPlayerId: string | null;
  result: GameResult | null;
  reconnectDeadline: string | null;
  rematchRequestedBy: string[];
  placement: ShipPlacement[] | null;
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

export type ClientEvent =
  | { type: 'room:create'; payload: { name: string; visibility: RoomVisibility } }
  | { type: 'room:join'; payload: { code: string } }
  | { type: 'room:leave'; payload: { roomId: string } }
  | {
      type: 'player:ready';
      payload: { roomId: string; playerId: string; ready: boolean };
    }
  | {
      type: 'ships:place';
      payload: { roomId: string; playerId: string; placements: ShipPlacement[] };
    }
  | { type: 'ships:confirm'; payload: { roomId: string; playerId: string } }
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
  | { type: 'attack:result' | 'ship:sunk'; payload: AttackRecord }
  | { type: 'heartbeat'; payload: { serverTime: string } };

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
