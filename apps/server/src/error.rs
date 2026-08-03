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
    #[error("방장만 이 작업을 수행할 수 있습니다.")]
    NotHost,
    #[error("배치를 확정한 뒤에는 함선을 변경할 수 없습니다.")]
    PlacementLocked,
    #[error("닉네임은 2~16자의 문자, 숫자, 공백, 밑줄 또는 하이픈만 사용할 수 있습니다.")]
    InvalidNickname,
    #[error("방 이름은 2~32자로 입력해 주세요.")]
    InvalidRoomName,
    #[error("같은 닉네임을 사용 중인 플레이어가 있습니다.")]
    DuplicateNickname,
    #[error("인증 세션이 없거나 만료되었습니다.")]
    Unauthorized,
    #[error("허용되지 않은 출처의 연결입니다.")]
    OriginNotAllowed,
    #[error("요청 형식이 올바르지 않습니다.")]
    InvalidRequest,
    #[error("요청이 너무 잦습니다. 잠시 후 다시 시도해 주세요.")]
    RateLimited,
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
            Self::RoomNotFound => "ROOM_NOT_FOUND",
            Self::RoomFull => "ROOM_FULL",
            Self::RoomAlreadyStarted => "ROOM_ALREADY_STARTED",
            Self::AlreadyJoined => "ALREADY_JOINED",
            Self::NotRoomMember => "NOT_ROOM_MEMBER",
            Self::NotHost => "NOT_HOST",
            Self::PlacementLocked => "PLACEMENT_LOCKED",
            Self::InvalidNickname => "INVALID_NICKNAME",
            Self::InvalidRoomName => "INVALID_ROOM_NAME",
            Self::DuplicateNickname => "DUPLICATE_NICKNAME",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::OriginNotAllowed => "ORIGIN_NOT_ALLOWED",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::RateLimited => "RATE_LIMITED",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::OriginNotAllowed => StatusCode::FORBIDDEN,
            Self::RoomNotFound => StatusCode::NOT_FOUND,
            Self::RoomFull
            | Self::RoomAlreadyStarted
            | Self::AlreadyJoined
            | Self::DuplicateNickname
            | Self::CoordinateAlreadyAttacked
            | Self::VersionConflict
            | Self::TurnConflict
            | Self::PlacementLocked => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::StorageUnavailable => StatusCode::SERVICE_UNAVAILABLE,
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
