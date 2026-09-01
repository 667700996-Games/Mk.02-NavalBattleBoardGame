use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GameError {
    #[error("좌표는 A1부터 J10 사이여야 합니다.")]
    InvalidCoordinate,
    #[error("함선이 보드 경계를 벗어났습니다.")]
    PlacementOutOfBounds,
    #[error("함선은 서로 겹칠 수 없습니다.")]
    ShipsOverlap,
    #[error("모든 함선을 한 척씩 배치해 주세요.")]
    IncompleteFleet,
    #[error("함선 구성이 올바르지 않습니다.")]
    InvalidFleetComposition,
    #[error("이미 공격한 좌표입니다.")]
    CoordinateAlreadyAttacked,
    #[error("지금은 이 요청을 처리할 수 없는 게임 상태입니다.")]
    InvalidState,
    #[error("현재 당신의 턴이 아닙니다.")]
    NotYourTurn,
    #[error("게임 상태가 갱신되었습니다. 최신 상태를 불러왔습니다.")]
    VersionConflict,
    #[error("턴 번호가 일치하지 않습니다.")]
    TurnConflict,
    #[error("현재 턴의 제한 시간이 이미 만료되었습니다.")]
    TurnExpired,
    #[error("이 방에서는 전술 스킬이 비활성화되어 있습니다.")]
    TacticalSkillsDisabled,
    #[error("양쪽의 첫 공격 기회가 끝난 뒤에 전술 스킬을 사용할 수 있습니다.")]
    TacticalSkillLocked,
    #[error("해당 전술 스킬의 사용 횟수를 모두 소진했습니다.")]
    TacticalSkillExhausted,
    #[error("이미 이번 턴에 전술 스킬을 사용했습니다.")]
    TacticalSkillAlreadyUsed,
    #[error("전술 스킬의 표적 좌표가 올바르지 않습니다.")]
    InvalidTacticalSkillTargets,
    #[error("방을 찾을 수 없습니다.")]
    RoomNotFound,
    #[error("이미 두 명이 참가한 방입니다.")]
    RoomFull,
    #[error("이미 시작된 방에는 참가할 수 없습니다.")]
    RoomAlreadyStarted,
    #[error("같은 세션으로 이 방에 중복 참가할 수 없습니다.")]
    AlreadyJoined,
    #[error("이 방의 플레이어가 아닙니다.")]
    NotRoomMember,
    #[error("방장만 게임을 시작할 수 있습니다.")]
    NotHost,
    #[error("두 플레이어가 모두 준비를 완료해야 합니다.")]
    PlayersNotReady,
    #[error("게임을 시작하려면 정확히 두 명의 플레이어가 필요합니다.")]
    PlayerCountInvalid,
    #[error("연결이 끊긴 플레이어가 있어 게임을 시작할 수 없습니다.")]
    PlayerDisconnected,
    #[error("방 상태가 변경되었습니다. 최신 상태를 확인해 주세요.")]
    StaleRoomVersion,
    #[error("현재 방 상태에서는 게임을 시작할 수 없습니다.")]
    RoomStateInvalid,
    #[error("배치를 확정한 뒤에는 함선을 변경할 수 없습니다.")]
    PlacementLocked,
    #[error("제출한 함선 배치가 서버에 저장된 배치와 일치하지 않습니다.")]
    PlacementMismatch,
    #[error("게임이 이미 시작되었습니다.")]
    GameAlreadyStarted,
    #[error("현재 준비 완료 상태가 아닙니다.")]
    PlayerNotReady,
    #[error("닉네임은 2~16자의 문자, 숫자, 공백, 밑줄 또는 하이픈만 사용할 수 있습니다.")]
    InvalidNickname,
    #[error("방 이름은 2~32자로 입력해 주세요.")]
    InvalidRoomName,
    #[error("같은 닉네임을 사용 중인 플레이어가 있습니다.")]
    DuplicateNickname,
    #[error("이 계정 핸들은 이미 사용 중입니다.")]
    AccountHandleTaken,
    #[error("인증 세션이 없거나 만료되었습니다.")]
    Unauthorized,
    #[error("랭크 매칭은 계정으로 로그인한 지휘관만 이용할 수 있습니다.")]
    RankedAccountRequired,
    #[error("현재 랭크 시즌이 시작되지 않거나 이미 종료되었습니다.")]
    RankedSeasonUnavailable,
    #[error("허용되지 않은 출처의 연결입니다.")]
    OriginNotAllowed,
    #[error("이 클라이언트 프로토콜 버전은 현재 릴리스 창에서 지원되지 않습니다.")]
    ProtocolVersionMismatch,
    #[error("요청 형식이 올바르지 않습니다.")]
    InvalidRequest,
    #[error("요청이 너무 잦습니다. 잠시 후 다시 시도해 주세요.")]
    RateLimited,
    #[error("현재 서비스 정원이 가득 찼습니다. 잠시 후 다시 시도해 주세요.")]
    CapacityReached,
    #[error("서로 차단된 플레이어와는 매칭하거나 같은 방에 참가할 수 없습니다.")]
    PlayerBlocked,
    #[error("신고 사건을 찾을 수 없습니다.")]
    ReportNotFound,
    #[error("고객지원 조회 조건과 일치하는 계정을 찾을 수 없습니다.")]
    SupportAccountNotFound,
    #[error("회수할 수 있는 계정 세션을 찾을 수 없습니다.")]
    SupportSessionNotFound,
    #[error("라이브 콘텐츠 리비전을 찾을 수 없습니다.")]
    LiveContentRevisionNotFound,
    #[error(
        "라이브 콘텐츠가 다른 운영자에 의해 갱신되었습니다. 최신 리비전으로 다시 검증해 주세요."
    )]
    LiveContentRevisionConflict,
    #[error("계정 이용이 일시 정지되었습니다.")]
    AccountSuspended,
    #[error("계정 이용이 영구 제한되었습니다.")]
    AccountBanned,
    #[error("채팅 메시지는 1~300자의 일반 텍스트로 입력해 주세요.")]
    InvalidChatMessage,
    #[error("허용되지 않은 빠른 명령입니다.")]
    InvalidQuickCommand,
    #[error("허용되지 않은 이모지입니다.")]
    InvalidEmoji,
    #[error("데이터 저장소에 일시적인 문제가 발생했습니다.")]
    StorageUnavailable,
    #[error("서버에서 요청을 처리하지 못했습니다.")]
    Internal,
}

impl GameError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCoordinate => "INVALID_COORDINATE",
            Self::PlacementOutOfBounds => "PLACEMENT_OUT_OF_BOUNDS",
            Self::ShipsOverlap => "SHIPS_OVERLAP",
            Self::IncompleteFleet => "INCOMPLETE_FLEET",
            Self::InvalidFleetComposition => "INVALID_FLEET_COMPOSITION",
            Self::CoordinateAlreadyAttacked => "COORDINATE_ALREADY_ATTACKED",
            Self::InvalidState => "INVALID_STATE",
            Self::NotYourTurn => "NOT_YOUR_TURN",
            Self::VersionConflict => "VERSION_CONFLICT",
            Self::TurnConflict => "TURN_CONFLICT",
            Self::TurnExpired => "TURN_EXPIRED",
            Self::TacticalSkillsDisabled => "TACTICAL_SKILLS_DISABLED",
            Self::TacticalSkillLocked => "TACTICAL_SKILL_LOCKED",
            Self::TacticalSkillExhausted => "TACTICAL_SKILL_EXHAUSTED",
            Self::TacticalSkillAlreadyUsed => "TACTICAL_SKILL_ALREADY_USED",
            Self::InvalidTacticalSkillTargets => "INVALID_TACTICAL_SKILL_TARGETS",
            Self::RoomNotFound => "ROOM_NOT_FOUND",
            Self::RoomFull => "ROOM_FULL",
            Self::RoomAlreadyStarted => "ROOM_ALREADY_STARTED",
            Self::AlreadyJoined => "ALREADY_JOINED",
            Self::NotRoomMember => "NOT_ROOM_MEMBER",
            Self::NotHost => "NOT_HOST",
            Self::PlayersNotReady => "PLAYERS_NOT_READY",
            Self::PlayerCountInvalid => "PLAYER_COUNT_INVALID",
            Self::PlayerDisconnected => "PLAYER_DISCONNECTED",
            Self::StaleRoomVersion => "STALE_ROOM_VERSION",
            Self::RoomStateInvalid => "ROOM_STATE_INVALID",
            Self::PlacementLocked => "PLACEMENT_LOCKED",
            Self::PlacementMismatch => "PLACEMENT_MISMATCH",
            Self::GameAlreadyStarted => "GAME_ALREADY_STARTED",
            Self::PlayerNotReady => "PLAYER_NOT_READY",
            Self::InvalidNickname => "INVALID_NICKNAME",
            Self::InvalidRoomName => "INVALID_ROOM_NAME",
            Self::DuplicateNickname => "DUPLICATE_NICKNAME",
            Self::AccountHandleTaken => "ACCOUNT_HANDLE_TAKEN",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::RankedAccountRequired => "RANKED_ACCOUNT_REQUIRED",
            Self::RankedSeasonUnavailable => "RANKED_SEASON_UNAVAILABLE",
            Self::OriginNotAllowed => "ORIGIN_NOT_ALLOWED",
            Self::ProtocolVersionMismatch => "SERVER_PROTOCOL_MISMATCH",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::RateLimited => "RATE_LIMITED",
            Self::CapacityReached => "CAPACITY_REACHED",
            Self::PlayerBlocked => "PLAYER_BLOCKED",
            Self::ReportNotFound => "REPORT_NOT_FOUND",
            Self::SupportAccountNotFound => "SUPPORT_ACCOUNT_NOT_FOUND",
            Self::SupportSessionNotFound => "SUPPORT_SESSION_NOT_FOUND",
            Self::LiveContentRevisionNotFound => "LIVE_CONTENT_REVISION_NOT_FOUND",
            Self::LiveContentRevisionConflict => "LIVE_CONTENT_REVISION_CONFLICT",
            Self::AccountSuspended => "ACCOUNT_SUSPENDED",
            Self::AccountBanned => "ACCOUNT_BANNED",
            Self::InvalidChatMessage => "INVALID_CHAT_MESSAGE",
            Self::InvalidQuickCommand => "INVALID_QUICK_COMMAND",
            Self::InvalidEmoji => "INVALID_EMOJI",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::ProtocolVersionMismatch => StatusCode::UPGRADE_REQUIRED,
            Self::OriginNotAllowed
            | Self::PlayerBlocked
            | Self::RankedAccountRequired
            | Self::AccountSuspended
            | Self::AccountBanned => StatusCode::FORBIDDEN,
            Self::RoomNotFound
            | Self::ReportNotFound
            | Self::SupportAccountNotFound
            | Self::SupportSessionNotFound
            | Self::LiveContentRevisionNotFound => StatusCode::NOT_FOUND,
            Self::RoomFull
            | Self::RoomAlreadyStarted
            | Self::AlreadyJoined
            | Self::DuplicateNickname
            | Self::AccountHandleTaken
            | Self::CoordinateAlreadyAttacked
            | Self::VersionConflict
            | Self::StaleRoomVersion
            | Self::LiveContentRevisionConflict
            | Self::TurnConflict
            | Self::TurnExpired
            | Self::TacticalSkillLocked
            | Self::TacticalSkillExhausted
            | Self::TacticalSkillAlreadyUsed
            | Self::PlacementLocked => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::StorageUnavailable | Self::CapacityReached | Self::RankedSeasonUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: String,
    request_id: Uuid,
}

impl IntoResponse for GameError {
    fn into_response(self) -> Response {
        let request_id = Uuid::new_v4();
        if matches!(self, Self::StorageUnavailable | Self::Internal) {
            error!(%request_id, error_code = self.code(), "request failed");
        }
        let status = self.status();
        let body = ErrorBody {
            code: self.code(),
            message: self.to_string(),
            request_id,
        };
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for GameError {
    fn from(error: sqlx::Error) -> Self {
        error!(error = %error, "database operation failed");
        Self::StorageUnavailable
    }
}

impl From<redis::RedisError> for GameError {
    fn from(error: redis::RedisError) -> Self {
        error!(error = %error, "cache operation failed");
        Self::StorageUnavailable
    }
}
