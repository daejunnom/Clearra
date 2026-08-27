use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ForwardSpinGroup {
    T,
    Other,
    Integrated,
}

impl ForwardSpinGroup {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::T => "t",
            Self::Other => "other",
            Self::Integrated => "integrated",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ForwardPathStep {
    piece: PieceKind,
    rotation: RotationState,
    placement_rotation: RotationState,
    x: i8,
    y: i8,
    hold_decision: &'static str,
    cleared_lines: u8,
    spin: Option<(char, bool)>,
    damage: u32,
    total_damage: u32,
    placement_mask: [u64; 4],
    cleared_row_mask: u32,
    board_after: [u64; 4],
}

impl ForwardPathStep {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        piece: PieceKind,
        rotation: RotationState,
        placement_rotation: RotationState,
        x: i8,
        y: i8,
        hold_decision: &'static str,
        cleared_lines: u8,
        spin: Option<(char, bool)>,
        damage: u32,
        total_damage: u32,
        placement_mask: [u64; 4],
        cleared_row_mask: u32,
        board_after: [u64; 4],
    ) -> Self {
        Self {
            piece,
            rotation,
            placement_rotation,
            x,
            y,
            hold_decision,
            cleared_lines,
            spin,
            damage,
            total_damage,
            placement_mask,
            cleared_row_mask,
            board_after,
        }
    }

    pub const fn piece(&self) -> PieceKind {
        self.piece
    }
    pub const fn rotation(&self) -> RotationState {
        self.rotation
    }
    pub(crate) const fn placement_rotation(&self) -> RotationState {
        self.placement_rotation
    }
    pub const fn x(&self) -> i8 {
        self.x
    }
    pub const fn y(&self) -> i8 {
        self.y
    }
    pub const fn hold_decision(&self) -> &'static str {
        self.hold_decision
    }
    pub const fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
    pub const fn spin(&self) -> Option<(char, bool)> {
        self.spin
    }
    pub const fn damage(&self) -> u32 {
        self.damage
    }
    pub const fn total_damage(&self) -> u32 {
        self.total_damage
    }
    pub const fn placement_mask(&self) -> [u64; 4] {
        self.placement_mask
    }
    pub const fn cleared_row_mask(&self) -> u32 {
        self.cleared_row_mask
    }
    pub const fn board_after(&self) -> [u64; 4] {
        self.board_after
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardSearchOutcome {
    id: u64,
    source_pattern_index: u32,
    source_queue: Vec<PieceKind>,
    group: Option<ForwardSpinGroup>,
    final_board: [u64; 4],
    spin_piece: Option<PieceKind>,
    spin_mini: bool,
    spin_lines: u8,
    ren_count: Option<u8>,
    total_damage: u32,
    evidence_path_count: String,
    evidence_complete: bool,
    path: Vec<ForwardPathStep>,
}

impl ForwardSearchOutcome {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: u64,
        source_pattern_index: u32,
        source_queue: Vec<PieceKind>,
        group: Option<ForwardSpinGroup>,
        final_board: [u64; 4],
        spin_piece: Option<PieceKind>,
        spin_mini: bool,
        spin_lines: u8,
        ren_count: Option<u8>,
        total_damage: u32,
        path: Vec<ForwardPathStep>,
    ) -> Self {
        Self {
            id,
            source_pattern_index,
            source_queue,
            group,
            final_board,
            spin_piece,
            spin_mini,
            spin_lines,
            ren_count,
            total_damage,
            evidence_path_count: "1".to_owned(),
            evidence_complete: true,
            path,
        }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }
    pub const fn source_pattern_index(&self) -> u32 {
        self.source_pattern_index
    }
    pub fn source_queue(&self) -> &[PieceKind] {
        &self.source_queue
    }
    pub const fn group(&self) -> Option<ForwardSpinGroup> {
        self.group
    }
    pub const fn final_board(&self) -> [u64; 4] {
        self.final_board
    }
    pub const fn spin_piece(&self) -> Option<PieceKind> {
        self.spin_piece
    }
    pub const fn spin_mini(&self) -> bool {
        self.spin_mini
    }
    pub const fn spin_lines(&self) -> u8 {
        self.spin_lines
    }
    pub const fn ren_count(&self) -> Option<u8> {
        self.ren_count
    }
    pub const fn total_damage(&self) -> u32 {
        self.total_damage
    }
    /// Exact decimal count of distinct placement-path witnesses folded into this outcome.
    ///
    /// Damage and REN witnesses remain one public outcome per path, so their count is one.
    /// Forward-spin outcomes use terminal spin identity and retain every equivalent path in the
    /// search trace DAG; this field reports the exact folded witness count without materializing
    /// an exponentially duplicated result vector.
    pub fn evidence_path_count(&self) -> &str {
        &self.evidence_path_count
    }
    pub const fn evidence_complete(&self) -> bool {
        self.evidence_complete
    }
    pub fn path(&self) -> &[ForwardPathStep] {
        &self.path
    }

    pub(crate) fn with_evidence_path_count(mut self, count: String) -> Self {
        debug_assert!(!count.is_empty());
        debug_assert!(count.bytes().all(|byte| byte.is_ascii_digit()));
        debug_assert!(count != "0");
        debug_assert!(!count.starts_with('0'));
        self.evidence_path_count = count;
        self
    }

    pub(crate) fn assign_id(&mut self, id: u64) {
        self.id = id;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardSearchReport {
    complete: bool,
    initial_board: [u64; 4],
    workers_used: usize,
    visited_states: u64,
    generated_locks: u64,
    peak_frontier: usize,
    outcomes: Vec<ForwardSearchOutcome>,
}

impl ForwardSearchReport {
    pub(crate) fn new(
        complete: bool,
        initial_board: [u64; 4],
        workers_used: usize,
        visited_states: u64,
        generated_locks: u64,
        peak_frontier: usize,
        outcomes: Vec<ForwardSearchOutcome>,
    ) -> Self {
        Self {
            complete,
            initial_board,
            workers_used: workers_used.max(1),
            visited_states,
            generated_locks,
            peak_frontier,
            outcomes,
        }
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub const fn initial_board(&self) -> [u64; 4] {
        self.initial_board
    }
    pub const fn workers_used(&self) -> usize {
        self.workers_used
    }
    pub const fn visited_states(&self) -> u64 {
        self.visited_states
    }
    pub const fn generated_locks(&self) -> u64 {
        self.generated_locks
    }
    pub const fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }
    pub fn outcomes(&self) -> &[ForwardSearchOutcome] {
        &self.outcomes
    }

    pub(crate) fn outcomes_mut(&mut self) -> &mut Vec<ForwardSearchOutcome> {
        &mut self.outcomes
    }
    pub fn maximum_damage(&self) -> Option<u32> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.ren_count().is_none())
            .map(ForwardSearchOutcome::total_damage)
            .max()
    }

    pub fn maximum_ren(&self) -> Option<u8> {
        self.outcomes
            .iter()
            .filter_map(ForwardSearchOutcome::ren_count)
            .max()
    }

    pub(crate) fn with_workers_used(mut self, workers_used: usize) -> Self {
        self.workers_used = workers_used.max(1);
        self
    }
}
