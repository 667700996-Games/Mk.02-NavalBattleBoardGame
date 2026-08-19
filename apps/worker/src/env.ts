import type {
  AccountDurableObject,
  GameRoomDurableObject,
  LobbyDurableObject,
  EdgeRateLimitDurableObject,
  ProgressionDurableObject,
  MatchmakingDurableObject,
} from "./objects";

export interface WorkerEnv {
  ASSETS: Fetcher;
  ACCOUNTS: DurableObjectNamespace<AccountDurableObject>;
  LOBBY: DurableObjectNamespace<LobbyDurableObject>;
  GAME_ROOMS: DurableObjectNamespace<GameRoomDurableObject>;
  RATE_LIMITS: DurableObjectNamespace<EdgeRateLimitDurableObject>;
  PROGRESSION: DurableObjectNamespace<ProgressionDurableObject>;
  MATCHMAKING: DurableObjectNamespace<MatchmakingDurableObject>;
}
