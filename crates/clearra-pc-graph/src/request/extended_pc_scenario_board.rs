use clearra_core_domain::board::standard_pc_board::{
    Board256Mask, StandardPcBoard, StandardPcBoardError, StandardPcBoardMask,
    StandardPcBoardStorageKind, BOARD256_WORD_COUNT, STANDARD_PC_EXTENDED_MIN_LINES,
    STANDARD_PC_MAX_LINES,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtendedPcScenarioBoard {
    board: StandardPcBoard,
}

impl ExtendedPcScenarioBoard {
    pub fn standard_10_from_words(
        visible_height: u8,
        occupied_words: [u64; BOARD256_WORD_COUNT],
    ) -> Result<Self, ExtendedPcScenarioBoardError> {
        validate_extended_height(visible_height)?;
        let board = StandardPcBoard::from_words(visible_height, occupied_words)
            .map_err(ExtendedPcScenarioBoardError::Board)?;
        Ok(Self { board })
    }

    pub fn standard_10(
        visible_height: u8,
        occupied: Board256Mask,
    ) -> Result<Self, ExtendedPcScenarioBoardError> {
        Self::standard_10_from_words(visible_height, occupied.words())
    }

    pub const fn width(self) -> u16 {
        self.board.width()
    }

    pub const fn visible_height(self) -> u8 {
        self.board.lines()
    }

    pub const fn occupied(self) -> StandardPcBoardMask {
        self.board.occupied()
    }

    pub const fn occupied_words(self) -> [u64; BOARD256_WORD_COUNT] {
        self.board.occupied().words()
    }

    pub const fn storage_kind(self) -> StandardPcBoardStorageKind {
        self.board.occupied().storage_kind()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtendedPcScenarioBoardError {
    CompactBoardMustUseLegacyContract { lines: u8 },
    TooManyLines { lines: u8, maximum: u8 },
    Board(StandardPcBoardError),
}

fn validate_extended_height(lines: u8) -> Result<(), ExtendedPcScenarioBoardError> {
    if lines < STANDARD_PC_EXTENDED_MIN_LINES {
        return Err(ExtendedPcScenarioBoardError::CompactBoardMustUseLegacyContract { lines });
    }
    if lines > STANDARD_PC_MAX_LINES {
        return Err(ExtendedPcScenarioBoardError::TooManyLines {
            lines,
            maximum: STANDARD_PC_MAX_LINES,
        });
    }
    Ok(())
}
