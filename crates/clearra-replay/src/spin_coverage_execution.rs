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
        usize::try_from(self.checked_nested_retained_bytes().unwrap_or(u128::MAX))
            .unwrap_or(usize::MAX)
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.candidate_key.capacity() as u128)
            .checked_add(
                (self.nodes.capacity() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?,
            )?
            .checked_add(
                (self.edges.capacity() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.candidate_key.len() as u128)
            .checked_add(
                (self.nodes.len() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?,
            )?
            .checked_add(
                (self.edges.len() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
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
        usize::try_from(self.checked_nested_retained_bytes().unwrap_or(u128::MAX))
            .unwrap_or(usize::MAX)
    }

    /// Complete nested backing owned by this batch. In particular, the two
    /// outer `Vec` buffers use their actual capacities rather than logical
    /// lengths, so spare-capacity constructors cannot understate retention.
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
                .checked_mul(core::mem::size_of::<SpinCoverageExecutionGraph>() as u128)?,
        )?;
        for graph in &self.graphs {
            bytes = bytes.checked_add(graph.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

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
                .checked_mul(core::mem::size_of::<SpinCoverageExecutionGraph>() as u128)?,
        )?;
        for graph in &self.graphs {
            bytes = bytes.checked_add(graph.checked_clone_nested_bytes()?)?;
        }
        Some(bytes)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

#[cfg(test)]
mod memory_projection_tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::{SpinCoverageExecutionBatch, SpinCoverageExecutionGraph};

    fn reserved(value: &str, capacity: usize) -> String {
        let mut result = String::with_capacity(capacity);
        result.push_str(value);
        result
    }

    #[test]
    fn batch_projection_uses_outer_and_nested_capacities() {
        let mut pattern = Vec::with_capacity(7);
        pattern.push(PieceKind::T);
        let mut patterns = Vec::with_capacity(5);
        patterns.push(pattern);

        let graph = SpinCoverageExecutionGraph {
            candidate_id: 1,
            candidate_key: reserved("candidate", 43),
            root: 0,
            nodes: Vec::with_capacity(3),
            edges: Vec::with_capacity(11),
        };
        let mut graphs = Vec::with_capacity(4);
        graphs.push(graph);
        let batch = SpinCoverageExecutionBatch::new(
            patterns, 0, None, true, false, false, 1, 1, graphs, true,
        );

        let expected = 5 * core::mem::size_of::<Vec<PieceKind>>()
            + 7 * core::mem::size_of::<PieceKind>()
            + 4 * core::mem::size_of::<SpinCoverageExecutionGraph>()
            + 43
            + 3 * core::mem::size_of::<crate::ScoringExecutionNode>()
            + 11 * core::mem::size_of::<crate::ScoringExecutionEdge>();
        assert_eq!(
            batch.checked_nested_retained_bytes(),
            Some(expected as u128)
        );
        assert_eq!(batch.retained_bytes(), expected);

        let clone_bytes = core::mem::size_of::<Vec<PieceKind>>()
            + core::mem::size_of::<PieceKind>()
            + core::mem::size_of::<SpinCoverageExecutionGraph>()
            + "candidate".len();
        assert_eq!(
            batch.checked_clone_nested_bytes(),
            Some(clone_bytes as u128)
        );
        assert_eq!(
            batch.checked_clone_peak_bytes(),
            Some((expected + clone_bytes) as u128)
        );
    }
}
