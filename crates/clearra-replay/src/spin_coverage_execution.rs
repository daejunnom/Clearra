use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{ScoringExecutionEdge, ScoringExecutionNode};

/// Board-size-independent execution graph used by exact spin coverage.
///
/// The canonical candidate key remains the equality authority. `candidate_id`
/// is only an accelerator and diagnostic identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinCoverageExecutionGraph {
    candidate_id: u64,
    candidate_key: String,
    root: u32,
    nodes: Vec<ScoringExecutionNode>,
    edges: Vec<ScoringExecutionEdge>,
}

impl SpinCoverageExecutionGraph {
    pub fn new(
        candidate_id: u64,
        candidate_key: impl Into<String>,
        root: u32,
        nodes: Vec<ScoringExecutionNode>,
        edges: Vec<ScoringExecutionEdge>,
    ) -> Self {
        let candidate_key = candidate_key.into();
        assert!(
            !candidate_key.is_empty(),
            "spin coverage candidate key must be nonempty"
        );
        Self {
            candidate_id,
            candidate_key,
            root,
            nodes,
            edges,
        }
    }

    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
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
        let start = node.edge_start() as usize;
        &self.edges[start..start + node.edge_count() as usize]
    }

    pub fn retained_bytes(&self) -> usize {
        self.candidate_key.capacity()
            + self.nodes.capacity() * core::mem::size_of::<ScoringExecutionNode>()
            + self.edges.capacity() * core::mem::size_of::<ScoringExecutionEdge>()
    }
}

/// Supply and execution evidence needed by spin coverage, without a Board64
/// rendering or replay dependency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinCoverageExecutionBatch {
    patterns: Vec<Vec<PieceKind>>,
    initial_cursor: u16,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    kick_table_id: u64,
    rule_profile_id: u64,
    graphs: Vec<SpinCoverageExecutionGraph>,
    complete: bool,
}

impl SpinCoverageExecutionBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        patterns: Vec<Vec<PieceKind>>,
        initial_cursor: u16,
        initial_hold: Option<PieceKind>,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        projects_standard_bag_lookahead: bool,
        kick_table_id: u64,
        rule_profile_id: u64,
        graphs: Vec<SpinCoverageExecutionGraph>,
        complete: bool,
    ) -> Self {
        assert!(
            kick_table_id != 0,
            "spin coverage kick-table identity must be nonzero"
        );
        assert!(
            rule_profile_id != 0,
            "spin coverage rule-profile identity must be nonzero"
        );
        Self {
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

    pub fn graphs(&self) -> &[SpinCoverageExecutionGraph] {
        &self.graphs
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn retained_bytes(&self) -> usize {
        self.patterns
            .iter()
            .map(|pattern| pattern.capacity() * core::mem::size_of::<PieceKind>())
            .sum::<usize>()
            + self
                .graphs
                .iter()
                .map(SpinCoverageExecutionGraph::retained_bytes)
                .sum::<usize>()
    }
}
