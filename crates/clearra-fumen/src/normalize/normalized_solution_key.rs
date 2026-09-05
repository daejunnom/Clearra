#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedSolutionKey {
    initial_board_mask: u64,
    final_board_mask: u64,
    piece_sequence: Vec<String>,
    hold_decision_sequence: Vec<String>,
    operation_sequence: Vec<String>,
    cleared_line_sequence: Vec<String>,
    mirror_policy: String,
    normalized_shape_key: String,
    normalized_tiling_key: String,
}

impl NormalizedSolutionKey {
    // The constructor mirrors the independent components of the canonical key contract.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_board_mask: u64,
        final_board_mask: u64,
        piece_sequence: Vec<String>,
        hold_decision_sequence: Vec<String>,
        operation_sequence: Vec<String>,
        cleared_line_sequence: Vec<String>,
        mirror_policy: String,
        normalized_shape_key: String,
        normalized_tiling_key: String,
    ) -> Self {
        Self {
            initial_board_mask,
            final_board_mask,
            piece_sequence,
            hold_decision_sequence,
            operation_sequence,
            cleared_line_sequence,
            mirror_policy,
            normalized_shape_key,
            normalized_tiling_key,
        }
    }
}
impl NormalizedSolutionKey {
    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }
}
impl NormalizedSolutionKey {
    pub const fn final_board_mask(&self) -> u64 {
        self.final_board_mask
    }
}
impl NormalizedSolutionKey {
    pub fn piece_sequence(&self) -> &[String] {
        &self.piece_sequence
    }
}
impl NormalizedSolutionKey {
    pub fn hold_decision_sequence(&self) -> &[String] {
        &self.hold_decision_sequence
    }
}
impl NormalizedSolutionKey {
    pub fn operation_sequence(&self) -> &[String] {
        &self.operation_sequence
    }
}
impl NormalizedSolutionKey {
    pub fn cleared_line_sequence(&self) -> &[String] {
        &self.cleared_line_sequence
    }
}
impl NormalizedSolutionKey {
    pub fn mirror_policy(&self) -> &str {
        &self.mirror_policy
    }
}
impl NormalizedSolutionKey {
    pub fn normalized_shape_key(&self) -> &str {
        &self.normalized_shape_key
    }
}
impl NormalizedSolutionKey {
    pub fn normalized_tiling_key(&self) -> &str {
        &self.normalized_tiling_key
    }
}
