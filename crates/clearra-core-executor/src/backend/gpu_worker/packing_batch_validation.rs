#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingBatchValidationError {
    ZeroBatchId,
    ZeroBoardDimension,
    BoardExceedsBoard64Limit {
        cell_count: u16,
    },
    ZeroActivePackingRows,
    ActivePackingRowsExceedBoardHeight {
        active_packing_rows: u8,
        board_height: u8,
    },
    GoalClearLinesHintExceedsBoardHeight {
        goal_clear_lines_hint: u8,
        board_height: u8,
    },
    InitialBoardMaskOutsideActivePackingRows,
    ZeroPieceWindow,
    ZeroPieceCount,
    PieceCountExceedsPieceWindow {
        piece_count: u8,
        piece_window: u8,
    },
    ExactPieceCountExceedsPieceWindow {
        exact_piece_count: u8,
        piece_window: u8,
    },
    UnknownPieceSourceKind {
        piece_source_kind: u8,
    },
    MissingPieceSourceId,
    InvalidPieceMultisetWindow,
    PieceCountDoesNotMatchMultiset {
        piece_count: u8,
        total_count: u8,
    },
    ExactPieceCountDoesNotMatchMultiset {
        exact_piece_count: u8,
        exact_count: u8,
    },
    MissingPieceMultisetWindow {
        piece_count: u8,
        stored_len: u16,
    },
    InvalidPiece {
        index: usize,
        piece: u8,
    },
    InactivePieceSlotNotEmpty {
        index: usize,
        piece: u8,
    },
    ZeroCandidateCapacity,
    ZeroMaxFrontierStates,
    ZeroPatternCount,
    MissingRuleProfileId,
    MissingKickProfileId,
    MissingPatternUniverseIdentity,
    MissingPatternWeightModelIdentity,
}
