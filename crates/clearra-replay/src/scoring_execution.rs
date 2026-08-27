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

    pub const fn edge_start(self) -> u32 {
        self.edge_start
    }

    pub const fn edge_count(self) -> u32 {
        self.edge_count
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

    /// Heap storage retained by this graph, excluding the inline graph value.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.nodes.capacity() as u128)
            .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?
            .checked_add(
                (self.edges.capacity() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )
    }

    /// Heap storage requested by `Clone` while the source graph remains live.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.nodes.len() as u128)
            .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?
            .checked_add(
                (self.edges.len() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )
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
    projects_standard_bag_lookahead: bool,
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
        projects_standard_bag_lookahead: bool,
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
            projects_standard_bag_lookahead,
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

    pub const fn projects_standard_bag_lookahead(&self) -> bool {
        self.projects_standard_bag_lookahead
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

    /// Heap storage retained by this batch, excluding the inline batch value.
    /// Outer vector slots and every nested pattern/graph allocation are counted
    /// exactly once from their owning capacities.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (self.patterns.capacity() as u128)
            .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?;
        for pattern in &self.patterns {
            bytes = bytes.checked_add(
                (pattern.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(
            (self.graphs.capacity() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionGraph>() as u128)?,
        )?;
        for graph in &self.graphs {
            bytes = bytes.checked_add(graph.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

    /// Heap storage requested by `Clone`, excluding the cloned inline batch.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = (self.patterns.len() as u128)
            .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?;
        for pattern in &self.patterns {
            bytes = bytes.checked_add(
                (pattern.len() as u128).checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(
            (self.graphs.len() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionGraph>() as u128)?,
        )?;
        for graph in &self.graphs {
            bytes = bytes.checked_add(graph.checked_clone_nested_bytes()?)?;
        }
        Some(bytes)
    }

    /// Peak bytes while this batch and a fieldwise clone coexist.
    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.checked_nested_retained_bytes()?)?
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

#[cfg(test)]
mod retained_memory_projection_tests {
    use super::*;

    #[test]
    fn scoring_batch_projection_counts_owner_capacities_and_clone_lengths() {
        let mut pattern = Vec::with_capacity(5);
        pattern.extend([PieceKind::I, PieceKind::O]);
        let pattern_capacity = pattern.capacity();
        let mut patterns = Vec::with_capacity(3);
        patterns.push(pattern);
        let patterns_capacity = patterns.capacity();

        let mut nodes = Vec::with_capacity(7);
        nodes.push(ScoringExecutionNode::new(0, 0, true));
        let nodes_capacity = nodes.capacity();
        let edges = Vec::with_capacity(11);
        let edges_capacity = edges.capacity();
        let graph = ExactScoringExecutionGraph::new(
            1,
            StandardBoard64TilingIdentity::from_placements(0, []).expect("empty tiling identity"),
            0,
            nodes,
            edges,
        );
        let graph_retained = nodes_capacity as u128
            * core::mem::size_of::<ScoringExecutionNode>() as u128
            + edges_capacity as u128 * core::mem::size_of::<ScoringExecutionEdge>() as u128;
        assert_eq!(graph.checked_nested_retained_bytes(), Some(graph_retained));

        let mut graphs = Vec::with_capacity(4);
        graphs.push(graph);
        let graphs_capacity = graphs.capacity();
        let batch = ExactScoringExecutionBatch::new(
            Board64Layout::standard_10_by_lines(4).expect("layout"),
            0,
            patterns,
            0,
            None,
            true,
            false,
            false,
            1,
            1,
            graphs,
            true,
        );
        let retained = patterns_capacity as u128 * core::mem::size_of::<Vec<PieceKind>>() as u128
            + pattern_capacity as u128 * core::mem::size_of::<PieceKind>() as u128
            + graphs_capacity as u128 * core::mem::size_of::<ExactScoringExecutionGraph>() as u128
            + graph_retained;
        assert_eq!(batch.checked_nested_retained_bytes(), Some(retained));

        let clone_nested = core::mem::size_of::<Vec<PieceKind>>() as u128
            + 2_u128 * core::mem::size_of::<PieceKind>() as u128
            + core::mem::size_of::<ExactScoringExecutionGraph>() as u128
            + core::mem::size_of::<ScoringExecutionNode>() as u128;
        assert_eq!(batch.checked_clone_nested_bytes(), Some(clone_nested));
        assert_eq!(
            batch.checked_clone_peak_bytes(),
            Some(
                retained
                    + clone_nested
                    + 2_u128 * core::mem::size_of::<ExactScoringExecutionBatch>() as u128
            )
        );
    }
}
