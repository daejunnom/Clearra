use clearra_core_domain::piece::piece_kind::PieceKind;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackToBackFilterMemoryProjection {
    pub output_retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackToBackFilterMemoryReport {
    pub projection: BackToBackFilterMemoryProjection,
    pub output_retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackToBackFilterError {
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed {
        required_output_bytes: u128,
    },
}

impl BackToBackExecutionFilter {
    pub fn checked_scoring_batch_memory_projection(
        batch: &ExactScoringExecutionBatch,
        spin_profile: SpinProfileId,
    ) -> Option<BackToBackFilterMemoryProjection> {
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let mut output_retained_bytes = core::mem::size_of::<ExactScoringExecutionBatch>() as u128;
        output_retained_bytes = output_retained_bytes.checked_add(
            (batch.patterns().len() as u128)
                .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?,
        )?;
        for pattern in batch.patterns() {
            output_retained_bytes = output_retained_bytes.checked_add(
                (pattern.len() as u128).checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
            )?;
        }
        output_retained_bytes = output_retained_bytes.checked_add(
            (batch.graphs().len() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionGraph>() as u128)?,
        )?;
        for graph in batch.graphs() {
            output_retained_bytes = output_retained_bytes.checked_add(
                (graph.node_count() as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?,
            )?;
            let allowed_edges = checked_allowed_edge_count(
                graph.node_count(),
                |index| graph.node(index),
                |node| graph.edges(node),
                policy,
            )?;
            output_retained_bytes = output_retained_bytes.checked_add(
                (allowed_edges as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )?;
        }
        Some(BackToBackFilterMemoryProjection {
            output_retained_bytes,
        })
    }

    pub fn checked_spin_batch_memory_projection(
        batch: &SpinCoverageExecutionBatch,
        spin_profile: SpinProfileId,
    ) -> Option<BackToBackFilterMemoryProjection> {
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let mut output_retained_bytes = core::mem::size_of::<SpinCoverageExecutionBatch>() as u128;
        output_retained_bytes = output_retained_bytes.checked_add(
            (batch.patterns().len() as u128)
                .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?,
        )?;
        for pattern in batch.patterns() {
            output_retained_bytes = output_retained_bytes.checked_add(
                (pattern.len() as u128).checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
            )?;
        }
        output_retained_bytes = output_retained_bytes.checked_add(
            (batch.graphs().len() as u128)
                .checked_mul(core::mem::size_of::<SpinCoverageExecutionGraph>() as u128)?,
        )?;
        for graph in batch.graphs() {
            output_retained_bytes = output_retained_bytes
                .checked_add(graph.candidate_key().len() as u128)?
                .checked_add(
                    (graph.node_count() as u128)
                        .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)?,
                )?;
            let allowed_edges = checked_allowed_edge_count(
                graph.node_count(),
                |index| graph.node(index),
                |node| graph.edges(node),
                policy,
            )?;
            output_retained_bytes = output_retained_bytes.checked_add(
                (allowed_edges as u128)
                    .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?,
            )?;
        }
        Some(BackToBackFilterMemoryProjection {
            output_retained_bytes,
        })
    }

    pub fn scoring_batch_with_memory_limit(
        batch: &ExactScoringExecutionBatch,
        spin_profile: SpinProfileId,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<(ExactScoringExecutionBatch, BackToBackFilterMemoryReport), BackToBackFilterError>
    {
        let projection = Self::checked_scoring_batch_memory_projection(batch, spin_profile)
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
        check_memory_limit(
            already_retained_bytes,
            projection.output_retained_bytes,
            max_memory_bytes,
        )?;
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let patterns = try_clone_patterns(batch.patterns(), projection.output_retained_bytes)?;
        let mut graphs = Vec::new();
        graphs
            .try_reserve_exact(batch.graphs().len())
            .map_err(|_| BackToBackFilterError::AllocationFailed {
                required_output_bytes: projection.output_retained_bytes,
            })?;
        for graph in batch.graphs() {
            graphs.push(try_filter_scoring_graph(
                graph,
                policy,
                projection.output_retained_bytes,
            )?);
        }
        let filtered = ExactScoringExecutionBatch::new(
            batch.layout(),
            batch.initial_occupied(),
            patterns,
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        );
        let output_retained_bytes = (core::mem::size_of::<ExactScoringExecutionBatch>() as u128)
            .checked_add(
                filtered
                    .checked_nested_retained_bytes()
                    .ok_or(BackToBackFilterError::ProjectionOverflow)?,
            )
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
        check_memory_limit(
            already_retained_bytes,
            output_retained_bytes,
            max_memory_bytes,
        )?;
        Ok((
            filtered,
            BackToBackFilterMemoryReport {
                projection,
                output_retained_bytes,
            },
        ))
    }

    pub fn spin_batch_with_memory_limit(
        batch: &SpinCoverageExecutionBatch,
        spin_profile: SpinProfileId,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<(SpinCoverageExecutionBatch, BackToBackFilterMemoryReport), BackToBackFilterError>
    {
        let projection = Self::checked_spin_batch_memory_projection(batch, spin_profile)
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
        check_memory_limit(
            already_retained_bytes,
            projection.output_retained_bytes,
            max_memory_bytes,
        )?;
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(spin_profile));
        let patterns = try_clone_patterns(batch.patterns(), projection.output_retained_bytes)?;
        let mut graphs = Vec::new();
        graphs
            .try_reserve_exact(batch.graphs().len())
            .map_err(|_| BackToBackFilterError::AllocationFailed {
                required_output_bytes: projection.output_retained_bytes,
            })?;
        for graph in batch.graphs() {
            graphs.push(try_filter_spin_graph(
                graph,
                policy,
                projection.output_retained_bytes,
            )?);
        }
        let filtered = SpinCoverageExecutionBatch::new(
            patterns,
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        );
        let output_retained_bytes = (core::mem::size_of::<SpinCoverageExecutionBatch>() as u128)
            .checked_add(
                filtered
                    .checked_nested_retained_bytes()
                    .ok_or(BackToBackFilterError::ProjectionOverflow)?,
            )
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
        check_memory_limit(
            already_retained_bytes,
            output_retained_bytes,
            max_memory_bytes,
        )?;
        Ok((
            filtered,
            BackToBackFilterMemoryReport {
                projection,
                output_retained_bytes,
            },
        ))
    }

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

fn check_memory_limit(
    already_retained_bytes: u128,
    output_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), BackToBackFilterError> {
    let required_memory_bytes = already_retained_bytes
        .checked_add(output_retained_bytes)
        .ok_or(BackToBackFilterError::ProjectionOverflow)?;
    if required_memory_bytes > max_memory_bytes {
        return Err(BackToBackFilterError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        });
    }
    Ok(())
}

fn try_clone_patterns(
    patterns: &[Vec<PieceKind>],
    required_output_bytes: u128,
) -> Result<Vec<Vec<PieceKind>>, BackToBackFilterError> {
    let allocation_error = || BackToBackFilterError::AllocationFailed {
        required_output_bytes,
    };
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(patterns.len())
        .map_err(|_| allocation_error())?;
    for pattern in patterns {
        let mut cloned_pattern = Vec::new();
        cloned_pattern
            .try_reserve_exact(pattern.len())
            .map_err(|_| allocation_error())?;
        cloned_pattern.extend_from_slice(pattern);
        cloned.push(cloned_pattern);
    }
    Ok(cloned)
}

fn try_filter_scoring_graph(
    graph: &ExactScoringExecutionGraph,
    policy: BackToBackPreservationPolicy,
    required_output_bytes: u128,
) -> Result<ExactScoringExecutionGraph, BackToBackFilterError> {
    let (nodes, edges) = try_filter_graph_edges(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
        policy,
        required_output_bytes,
    )?;
    Ok(ExactScoringExecutionGraph::new(
        graph.candidate_id(),
        graph.identity(),
        graph.root(),
        nodes,
        edges,
    ))
}

fn try_filter_spin_graph(
    graph: &SpinCoverageExecutionGraph,
    policy: BackToBackPreservationPolicy,
    required_output_bytes: u128,
) -> Result<SpinCoverageExecutionGraph, BackToBackFilterError> {
    let (nodes, edges) = try_filter_graph_edges(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
        policy,
        required_output_bytes,
    )?;
    let candidate_key = try_owned_string(graph.candidate_key(), required_output_bytes)?;
    Ok(SpinCoverageExecutionGraph::new(
        graph.candidate_id(),
        candidate_key,
        graph.root(),
        nodes,
        edges,
    ))
}

fn try_owned_string(
    value: &str,
    required_output_bytes: u128,
) -> Result<String, BackToBackFilterError> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| BackToBackFilterError::AllocationFailed {
            required_output_bytes,
        })?;
    owned.push_str(value);
    Ok(owned)
}

fn checked_allowed_edge_count<'a>(
    node_count: usize,
    mut node_at: impl FnMut(u32) -> Option<ScoringExecutionNode>,
    mut edges_for: impl FnMut(ScoringExecutionNode) -> &'a [ScoringExecutionEdge],
    policy: BackToBackPreservationPolicy,
) -> Option<usize> {
    let mut allowed = 0_usize;
    for index in 0..node_count {
        let node = node_at(u32::try_from(index).ok()?)?;
        allowed = allowed.checked_add(
            edges_for(node)
                .iter()
                .filter(|edge| policy.allows(**edge))
                .count(),
        )?;
    }
    Some(allowed)
}

fn try_filter_graph_edges<'a>(
    node_count: usize,
    mut node_at: impl FnMut(u32) -> Option<ScoringExecutionNode>,
    mut edges_for: impl FnMut(ScoringExecutionNode) -> &'a [ScoringExecutionEdge],
    policy: BackToBackPreservationPolicy,
    required_output_bytes: u128,
) -> Result<(Vec<ScoringExecutionNode>, Vec<ScoringExecutionEdge>), BackToBackFilterError> {
    let allowed_edge_count =
        checked_allowed_edge_count(node_count, &mut node_at, &mut edges_for, policy)
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
    let allocation_error = || BackToBackFilterError::AllocationFailed {
        required_output_bytes,
    };
    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(node_count)
        .map_err(|_| allocation_error())?;
    let mut edges = Vec::new();
    edges
        .try_reserve_exact(allowed_edge_count)
        .map_err(|_| allocation_error())?;
    for index in 0..node_count {
        let Some(node) =
            node_at(u32::try_from(index).map_err(|_| BackToBackFilterError::ProjectionOverflow)?)
        else {
            continue;
        };
        let edge_start =
            u32::try_from(edges.len()).map_err(|_| BackToBackFilterError::ProjectionOverflow)?;
        for edge in edges_for(node)
            .iter()
            .copied()
            .filter(|edge| policy.allows(*edge))
        {
            edges.push(edge);
        }
        let edge_count = edges
            .len()
            .checked_sub(edge_start as usize)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(BackToBackFilterError::ProjectionOverflow)?;
        nodes.push(ScoringExecutionNode::new(
            edge_start,
            edge_count,
            node.accepting(),
        ));
    }
    Ok((nodes, edges))
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

    use super::{
        check_memory_limit, BackToBackEdgePolicy, BackToBackExecutionFilter, BackToBackFilterError,
    };
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

    #[test]
    fn guarded_filter_accepts_exact_projection_and_rejects_peak_minus_one() {
        let batch = batch_with_edges(vec![edge(1, 1), edge(2, 0)]);
        let projection = BackToBackExecutionFilter::checked_scoring_batch_memory_projection(
            &batch,
            SpinProfileId::TSpins,
        )
        .expect("projection");
        let already_retained_bytes = 11_u128;
        let exact_cap = already_retained_bytes
            .checked_add(projection.output_retained_bytes)
            .expect("exact cap");
        let (filtered, report) = BackToBackExecutionFilter::scoring_batch_with_memory_limit(
            &batch,
            SpinProfileId::TSpins,
            already_retained_bytes,
            exact_cap,
        )
        .expect("exact projected cap");
        assert_eq!(
            filtered,
            BackToBackExecutionFilter::scoring_batch(&batch, SpinProfileId::TSpins)
        );
        assert!(report.output_retained_bytes <= projection.output_retained_bytes);

        assert!(matches!(
            BackToBackExecutionFilter::scoring_batch_with_memory_limit(
                &batch,
                SpinProfileId::TSpins,
                already_retained_bytes,
                exact_cap - 1,
            ),
            Err(BackToBackFilterError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            }) if required_memory_bytes == exact_cap && max_memory_bytes == exact_cap - 1
        ));
    }

    #[test]
    fn guarded_filter_external_addition_overflow_fails_closed() {
        assert_eq!(
            check_memory_limit(u128::MAX, 1, u128::MAX),
            Err(BackToBackFilterError::ProjectionOverflow)
        );
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
