use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_replay::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
    ScoringExecutionNode, SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
};
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy,
    profile::{SpinProfile, SpinProfileId},
};

use crate::score_profile_selection::spin_profile_id;

/// Post-processing-owned predicate used when a consumer needs the exact same
/// B2B edge decision without importing the scoring implementation itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackToBackEdgePolicy {
    policy: BackToBackPreservationPolicy,
}

impl BackToBackEdgePolicy {
    pub fn new(profile: SpinProfileSelection) -> Self {
        Self {
            policy: BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile_id(
                profile,
            ))),
        }
    }

    pub fn allows(self, edge: ScoringExecutionEdge) -> bool {
        self.policy.allows(edge)
    }

    /// Whether preserving B2B for this edge depends on recognized spin
    /// evidence. Zero-line clears, tetrises, and perfect clears are
    /// movement-evidence agnostic and may use their ordinary finesse route.
    pub const fn requires_recognized_spin(self, edge: ScoringExecutionEdge) -> bool {
        BackToBackPreservationPolicy::requires_recognized_spin(edge)
    }
}

pub struct BackToBackExecutionFilter;

impl BackToBackExecutionFilter {
    pub fn scoring_batch(
        batch: &ExactScoringExecutionBatch,
        spin_profile: SpinProfileId,
    ) -> ExactScoringExecutionBatch {
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let graphs = batch
            .graphs()
            .iter()
            .map(|graph| filter_scoring_graph(graph, policy))
            .collect();
        ExactScoringExecutionBatch::new(
            batch.layout(),
            batch.initial_occupied(),
            batch.patterns().to_vec(),
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        )
    }

    pub fn spin_batch(
        batch: &SpinCoverageExecutionBatch,
        spin_profile: SpinProfileId,
    ) -> SpinCoverageExecutionBatch {
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let graphs = batch
            .graphs()
            .iter()
            .map(|graph| filter_spin_graph(graph, policy))
            .collect();
        SpinCoverageExecutionBatch::new(
            batch.patterns().to_vec(),
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        )
    }
}

fn filter_scoring_graph(
    graph: &ExactScoringExecutionGraph,
    policy: BackToBackPreservationPolicy,
) -> ExactScoringExecutionGraph {
    let (nodes, edges) = filter_graph_edges(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
        policy,
    );
    ExactScoringExecutionGraph::new(
        graph.candidate_id(),
        graph.identity(),
        graph.root(),
        nodes,
        edges,
    )
}

fn filter_spin_graph(
    graph: &SpinCoverageExecutionGraph,
    policy: BackToBackPreservationPolicy,
) -> SpinCoverageExecutionGraph {
    let (nodes, edges) = filter_graph_edges(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
        policy,
    );
    SpinCoverageExecutionGraph::new(
        graph.candidate_id(),
        graph.candidate_key(),
        graph.root(),
        nodes,
        edges,
    )
}

fn filter_graph_edges<'a>(
    node_count: usize,
    mut node_at: impl FnMut(u32) -> Option<ScoringExecutionNode>,
    mut edges_for: impl FnMut(ScoringExecutionNode) -> &'a [ScoringExecutionEdge],
    policy: BackToBackPreservationPolicy,
) -> (Vec<ScoringExecutionNode>, Vec<ScoringExecutionEdge>) {
    let mut nodes = Vec::with_capacity(node_count);
    let mut edges = Vec::new();
    for index in 0..node_count {
        let Some(node) = node_at(index as u32) else {
            continue;
        };
        let edge_start = edges.len() as u32;
        edges.extend(
            edges_for(node)
                .iter()
                .copied()
                .filter(|edge| policy.allows(*edge)),
        );
        nodes.push(ScoringExecutionNode::new(
            edge_start,
            (edges.len() as u32).saturating_sub(edge_start),
            node.accepting(),
        ));
    }
    (nodes, edges)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{
        ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
        ScoringExecutionNode, ScoringLockEvidence,
    };
    use clearra_scoring::profile::SpinProfileId;

    use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;

    use super::{BackToBackEdgePolicy, BackToBackExecutionFilter};
    use crate::TSpinCoverageOnlyMaterializer;

    #[test]
    fn alternate_preserving_path_keeps_candidate_while_normal_clear_is_removed() {
        let batch = batch_with_edges(vec![edge(1, 1), edge(2, 0)]);
        let filtered = BackToBackExecutionFilter::scoring_batch(&batch, SpinProfileId::TSpins);
        let root = filtered.graphs()[0].node(0).expect("root");
        assert_eq!(filtered.graphs()[0].edges(root), &[edge(2, 0)]);

        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_paths(
            &filtered,
            0..1,
            &ExecutionControl::default(),
        )
        .expect("coverage");
        assert_eq!(materialized.candidate_keys().count(), 1);
        assert_eq!(materialized.covered_patterns().count_ones(), 1);
    }

    #[test]
    fn edge_policy_uses_the_same_authoritative_scoring_decision() {
        let normal_clear = edge(1, 1);
        let preserving = edge(2, 0);
        let policy = BackToBackEdgePolicy::new(SpinProfileSelection::TSpins);
        assert!(!policy.allows(normal_clear));
        assert!(policy.allows(preserving));
        assert!(policy.requires_recognized_spin(normal_clear));
        assert!(!policy.requires_recognized_spin(preserving));
    }

    #[test]
    fn candidate_without_preserving_execution_is_removed() {
        let batch = batch_with_edges(vec![edge(1, 1)]);
        let filtered = BackToBackExecutionFilter::scoring_batch(&batch, SpinProfileId::TSpins);
        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_paths(
            &filtered,
            0..1,
            &ExecutionControl::default(),
        )
        .expect("coverage");
        assert_eq!(materialized.candidate_keys().count(), 0);
        assert_eq!(materialized.covered_patterns().count_ones(), 0);
    }

    #[test]
    fn alternate_build_paths_count_as_one_candidate_pattern_witness() {
        let batch = batch_with_edges(vec![edge(1, 0), edge(2, 0)]);
        let filtered = BackToBackExecutionFilter::scoring_batch(&batch, SpinProfileId::TSpins);
        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_paths(
            &filtered,
            0..1,
            &ExecutionControl::default(),
        )
        .expect("coverage");

        assert_eq!(materialized.covered_patterns().count_ones(), 1);
        assert_eq!(materialized.candidate_keys().count(), 1);
        assert_eq!(materialized.witnessed_pattern_count(), 1);
    }

    fn batch_with_edges(edges: Vec<ScoringExecutionEdge>) -> ExactScoringExecutionBatch {
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("identity");
        let nodes = vec![
            ScoringExecutionNode::new(0, edges.len() as u32, false),
            ScoringExecutionNode::new(edges.len() as u32, 0, true),
            ScoringExecutionNode::new(edges.len() as u32, 0, true),
        ];
        ExactScoringExecutionBatch::new(
            Board64Layout::standard_10_by_lines(4).expect("layout"),
            0,
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![ExactScoringExecutionGraph::new(
                1, identity, 0, nodes, edges,
            )],
            true,
        )
    }

    fn edge(to: u32, cleared_lines: u8) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            to,
            0,
            PieceKind::I,
            RotationState::Zero,
            0,
            0,
            cleared_lines,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
    }
}
