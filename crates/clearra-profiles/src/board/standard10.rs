use clearra_core_domain::board::board_size::{BoardSize, BoardSizeError};

use super::board_profile::{BoardProfile, BoardProfileId};

pub const STANDARD_10_WIDTH: u16 = 10;
pub const STANDARD_VISIBLE_HEIGHT: u16 = 20;

pub fn standard_10_board_profile() -> BoardProfile {
    BoardProfile::new(BoardProfileId::Standard10, BoardSize::standard_10x20())
}

pub fn standard_10_analysis_size(lines: u8) -> Result<BoardSize, BoardSizeError> {
    BoardSize::new(STANDARD_10_WIDTH, u16::from(lines))
}

#[cfg(test)]
#[path = "standard10_tests.rs"]
mod tests;
