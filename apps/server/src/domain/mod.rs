mod board;
mod chat;
mod game;
mod integrity;
mod live_content;
mod matchmaking;
mod progression;
mod room;
mod session;
mod social;

pub use board::*;
pub use chat::*;
pub use game::*;
pub use integrity::*;
pub use live_content::*;
pub use matchmaking::*;
pub use progression::*;
pub use room::*;
pub use session::*;
pub use social::*;

use serde::{Deserialize, Serialize};

pub const BOARD_SIZE: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coordinate {
    pub row: u8,
    pub col: u8,
}

impl Coordinate {
    pub fn new(row: u8, col: u8) -> Result<Self, crate::error::GameError> {
        if row >= BOARD_SIZE || col >= BOARD_SIZE {
            return Err(crate::error::GameError::InvalidCoordinate);
        }
        Ok(Self { row, col })
    }

    pub fn label(self) -> String {
        format!("{}{}", (b'A' + self.row) as char, self.col + 1)
    }
}
