use clearra_core_domain::{
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_geometry::layout::board64_layout::Board64Layout;

use crate::RotationRequest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoringLockEvidence {
    last_action_was_rotation: bool,
    used_kick: bool,
    used_180: bool,
    from_rotation: RotationState,
    rotation_request: RotationRequest,
    kick_index: u8,
    kick_dx: i8,
    kick_dy: i8,
    predecessor_x: i8,
    predecessor_y: i8,
    first_success_confirmed: bool,
    immobile_before_clear: bool,
}

impl ScoringLockEvidence {
    pub const fn no_rotation(rotation: RotationState) -> Self {
        Self {
            last_action_was_rotation: false,
            used_kick: false,
            used_180: false,
            from_rotation: rotation,
            rotation_request: RotationRequest::None,
            kick_index: 0,
            kick_dx: 0,
            kick_dy: 0,
            predecessor_x: 0,
            predecessor_y: 0,
            first_success_confirmed: false,
            immobile_before_clear: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn rotation(
        from_rotation: RotationState,
        rotation_request: RotationRequest,
        kick_index: u8,
        kick_dx: i8,
        kick_dy: i8,
        predecessor_x: i8,
        predecessor_y: i8,
    ) -> Self {
        Self {
            last_action_was_rotation: true,
            used_kick: kick_index != 0 || kick_dx != 0 || kick_dy != 0,
            used_180: matches!(rotation_request, RotationRequest::HalfTurn),
            from_rotation,
            rotation_request,
            kick_index,
            kick_dx,
            kick_dy,
            predecessor_x,
            predecessor_y,
            first_success_confirmed: true,
            immobile_before_clear: false,
        }
    }

    pub const fn with_immobile_before_clear(mut self, immobile: bool) -> Self {
        self.immobile_before_clear = immobile;
        self
    }

    pub const fn last_action_was_rotation(self) -> bool {
        self.last_action_was_rotation
    }

    pub const fn used_kick(self) -> bool {
        self.used_kick
    }

    pub const fn used_180(self) -> bool {
        self.used_180
    }

    pub const fn from_rotation(self) -> RotationState {
        self.from_rotation
    }

    pub const fn rotation_request(self) -> RotationRequest {
        self.rotation_request
    }

    pub const fn kick_index(self) -> u8 {
        self.kick_index
    }

    pub const fn kick_dx(self) -> i8 {
        self.kick_dx
    }

    pub const fn kick_dy(self) -> i8 {
        self.kick_dy
    }

    pub const fn predecessor(self) -> (i8, i8) {
        (self.predecessor_x, self.predecessor_y)
    }

    pub const fn first_success_confirmed(self) -> bool {
        self.first_success_confirmed
    }

    pub const fn immobile_before_clear(self) -> bool {
        self.immobile_before_clear
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoringExecutionEdge {
    to: u32,
    operation_index: u8,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    cleared_lines: u8,
    blocked_t_corners: u8,
    blocked_t_front_corners: u8,
    lock_evidence: ScoringLockEvidence,
    perfect_clear: bool,
}

impl ScoringExecutionEdge {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        to: u32,
        operation_index: u8,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        cleared_lines: u8,
        blocked_t_corners: u8,
        blocked_t_front_corners: u8,
        lock_evidence: ScoringLockEvidence,
    ) -> Self {
        Self {
            to,
            operation_index,
            piece,
            rotation,
            x,
            y,
            cleared_lines,
            blocked_t_corners,
            blocked_t_front_corners,
            lock_evidence,
            perfect_clear: false,
        }
    }

    pub const fn with_perfect_clear(mut self, perfect_clear: bool) -> Self {
        self.perfect_clear = perfect_clear;
        self
    }

    pub const fn to(self) -> u32 {
        self.to
    }

    pub const fn operation_index(self) -> u8 {
        self.operation_index
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn rotation(self) -> RotationState {
        self.rotation
    }

    pub const fn x(self) -> i8 {
        self.x
    }

    pub const fn y(self) -> i8 {
        self.y
    }

    pub const fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }

    pub const fn blocked_t_corners(self) -> u8 {
        self.blocked_t_corners
    }

    pub const fn blocked_t_front_corners(self) -> u8 {
        self.blocked_t_front_corners
    }

    pub const fn lock_evidence(self) -> ScoringLockEvidence {
        self.lock_evidence
    }

    pub const fn perfect_clear(self) -> bool {
        self.perfect_clear
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoringExecutionNode {
    edge_start: u32,
    edge_count: u32,
    accepting: bool,
}

impl ScoringExecutionNode {
    pub const fn new(edge_start: u32, edge_count: u32, accepting: bool) -> Self {
        Self {
            edge_start,
            edge_count,
            accepting,
        }
    }

    pub const fn accepting(self) -> bool {
        self.accepting
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactScoringExecutionGraph {
    candidate_id: u64,
    identity: StandardBoard64TilingIdentity,
    root: u32,
    nodes: Vec<ScoringExecutionNode>,
    edges: Vec<ScoringExecutionEdge>,
}

impl ExactScoringExecutionGraph {
    pub fn new(
        candidate_id: u64,
        identity: StandardBoard64TilingIdentity,
        root: u32,
        nodes: Vec<ScoringExecutionNode>,
        edges: Vec<ScoringExecutionEdge>,
    ) -> Self {
        Self {
            candidate_id,
            identity,
            root,
            nodes,
            edges,
        }
    }

    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn identity(&self) -> StandardBoard64TilingIdentity {
        self.identity
    }

    pub const fn root(&self) -> u32 {
        self.root
    }

    pub fn node(&self, index: u32) -> Option<ScoringExecutionNode> {
        self.nodes.get(index as usize).copied()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edges(&self, node: ScoringExecutionNode) -> &[ScoringExecutionEdge] {
        let start = node.edge_start as usize;
        &self.edges[start..start + node.edge_count as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactScoringExecutionBatch {
    layout: Board64Layout,
    initial_occupied: u64,
    patterns: Vec<Vec<PieceKind>>,
    initial_cursor: u16,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    kick_table_id: u64,
    rule_profile_id: u64,
    graphs: Vec<ExactScoringExecutionGraph>,
    complete: bool,
}

impl ExactScoringExecutionBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout: Board64Layout,
        initial_occupied: u64,
        patterns: Vec<Vec<PieceKind>>,
        initial_cursor: u16,
        initial_hold: Option<PieceKind>,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        kick_table_id: u64,
        rule_profile_id: u64,
        graphs: Vec<ExactScoringExecutionGraph>,
        complete: bool,
    ) -> Self {
        assert!(
            kick_table_id != 0,
            "scoring kick-table identity must be nonzero"
        );
        assert!(
            rule_profile_id != 0,
            "scoring rule-profile identity must be nonzero"
        );
        Self {
            layout,
            initial_occupied,
            patterns,
            initial_cursor,
            initial_hold,
            hold_enabled,
            projects_unplaced_lookahead,
            kick_table_id,
            rule_profile_id,
            graphs,
            complete,
        }
    }

    pub const fn layout(&self) -> Board64Layout {
        self.layout
    }

    pub const fn initial_occupied(&self) -> u64 {
        self.initial_occupied
    }

    pub fn patterns(&self) -> &[Vec<PieceKind>] {
        &self.patterns
    }

    pub const fn initial_cursor(&self) -> u16 {
        self.initial_cursor
    }

    pub const fn initial_hold(&self) -> Option<PieceKind> {
        self.initial_hold
    }

    pub const fn hold_enabled(&self) -> bool {
        self.hold_enabled
    }

    pub const fn projects_unplaced_lookahead(&self) -> bool {
        self.projects_unplaced_lookahead
    }

    pub const fn kick_table_id(&self) -> u64 {
        self.kick_table_id
    }

    pub const fn rule_profile_id(&self) -> u64 {
        self.rule_profile_id
    }

    pub fn graphs(&self) -> &[ExactScoringExecutionGraph] {
        &self.graphs
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
}
