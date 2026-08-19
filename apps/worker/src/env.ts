import type {
  AccountDurableObject,
  GameRoomDurableObject,
  LobbyDurableObject,
  EdgeRateLimitDurableObject,
  ProgressionDurableObject,
  MatchmakingDurableObject,
  SocialDurableObject,
  OperationsDurableObject,
  ContentDurableObject,
} from "./objects";

export interface WorkerEnv {
  ASSETS: Fetcher;
  ACCOUNTS: DurableObjectNamespace<AccountDurableObject>;
  LOBBY: DurableObjectNamespace<LobbyDurableObject>;
  GAME_ROOMS: DurableObjectNamespace<GameRoomDurableObject>;
  RATE_LIMITS: DurableObjectNamespace<EdgeRateLimitDurableObject>;
  PROGRESSION: DurableObjectNamespace<ProgressionDurableObject>;
  MATCHMAKING: DurableObjectNamespace<MatchmakingDurableObject>;
  SOCIAL: DurableObjectNamespace<SocialDurableObject>;
  OPERATIONS: DurableObjectNamespace<OperationsDurableObject>;
  CONTENT: DurableObjectNamespace<ContentDurableObject>;
  ADMIN_TOKEN?: string;
}
