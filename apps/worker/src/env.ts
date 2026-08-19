import type {
  AccountDurableObject,
  GameRoomDurableObject,
  LobbyDurableObject,
  EdgeRateLimitDurableObject,
} from "./objects";

export interface WorkerEnv {
  ASSETS: Fetcher;
  ACCOUNTS: DurableObjectNamespace<AccountDurableObject>;
  LOBBY: DurableObjectNamespace<LobbyDurableObject>;
  GAME_ROOMS: DurableObjectNamespace<GameRoomDurableObject>;
  RATE_LIMITS: DurableObjectNamespace<EdgeRateLimitDurableObject>;
}
