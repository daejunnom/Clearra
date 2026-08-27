use std::collections::BTreeMap;

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_finesse::{
    ClassicInputAction, FinesseBoard, FinesseTarget, FrozenFinesseQuery, GeometryActionKey,
    PiecePose, TerminalEvidenceLabel,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_postprocess::BackToBackEdgePolicy;
use clearra_problem::SearchProblem;
use clearra_replay::{
    ExactScoringExecutionGraph, RotationRequest, ScoringExecutionEdge, ScoringExecutionNode,
    ScoringLockEvidence,
};
use clearra_supply::{
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    hold_automaton::HoldAutomatonState,
    pattern_universe::PatternPiecePositionIndex,
    piece_source::{PieceSourceId, PieceSourceKind},
};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    CorePathStep,
};

use super::{
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    exact_collections::ExactHashSet,
    geometry::{GeometryCandidate, TargetGroup},
    piece_order_language::{CoverageCacheLookup, PieceOrderLanguageCache},
    queue_observation_policy::{
        QueueObservationCoverage, QueueObservationPolicyEvaluator, RootedPieceLanguageUnion,
    },
    reachability::{checked_reachability_retained_upper_bound, ReachabilityWorkspace},
    realization_feasibility::{RealizationFeasibility, RealizationFeasibilityWorkspace},
    standard_bag_coverage::{StandardBagCoverage, StandardBagCoverageResult},
    WasmExactSearchError,
};

// Keep the low-volume symbolic fast path intact, but recycle accelerator
// arenas before wasm32's fixed address space is exhausted on full searches.
const SYMBOLIC_CACHE_LIVE_LIMIT: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct BuildEdge {
    pub to: u32,
    operation_index: u8,
    pub piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    cleared_lines: u8,
}

impl BuildEdge {
    pub(super) const fn canonical_key(&self) -> (u32, PieceKind, u8, RotationState, i8, i8, u8) {
        (
            self.to,
            self.piece,
            self.operation_index,
            self.rotation,
            self.x,
            self.y,
            self.cleared_lines,
        )
    }
}

#[derive(Debug)]
pub(super) struct BuildNode {
    edge_start: u32,
    edge_count: u32,
    piece_edge_start: u32,
    piece_edge_count: u32,
    pub depth: u8,
    pub live: bool,
    accepting: bool,
}

const _: () = assert!(core::mem::size_of::<BuildNode>() == 20);

impl BuildNode {
    pub fn accepting(&self) -> bool {
        self.accepting
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BuildOrderNodeSpec {
    pub edge_start: u32,
    pub edge_count: u32,
    pub depth: u8,
    pub accepting: bool,
}

#[derive(Debug)]
pub(super) struct BuildOrderGraph {
    pub nodes: Vec<BuildNode>,
    edges: Vec<BuildEdge>,
    piece_edges: Vec<BuildEdge>,
    piece_edges_share_operation_edges: bool,
    pub root: u32,
    reachability_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CoverageState {
    node: u32,
    cursor: u16,
    hold: Option<PieceKind>,
}

#[derive(Clone, Copy)]
struct FinessePathGuard<'a> {
    nodes: &'a [PreparedFinesseNode],
    spawn_profile: clearra_rules::spawn::SpawnProfile,
}

impl FinessePathGuard<'_> {
    fn current_piece_can_spawn(self, node_index: usize, piece: PieceKind) -> bool {
        self.nodes
            .get(node_index)
            .and_then(|node| node.source_board)
            .is_none_or(|board| board.piece_can_spawn(piece, self.spawn_profile))
    }
}

#[derive(Clone, Debug)]
pub(super) struct CandidateBuildResult {
    pub buildable: bool,
    pub covered_patterns: Option<PatternBitSet>,
    pub symbolic_coverage_root: Option<u32>,
    pub observation_language_root: Option<u32>,
    pub symbolic_covered_pattern_count: usize,
    pub witness_pattern_id: Option<u32>,
    pub build_variant_count: u128,
    pub count_complete: bool,
    pub representative_path: Vec<CorePathStep>,
    pub graph_nodes: usize,
    pub coverage_product_words: usize,
    pub coverage_product_states: usize,
    pub coverage_product_edge_checks: usize,
    pub feasibility_states: usize,
    pub feasibility_rejected: bool,
    pub reachability_states: usize,
    pub retained_bytes: usize,
    pub finesse_language: Option<PreparedFinesseLanguage>,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedFinesseNode {
    pub edge_start: u32,
    pub edge_count: u32,
    pub depth: u8,
    pub accepting: bool,
    pub source_board: Option<FinesseBoard>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedFinesseEdge {
    pub child: u32,
    pub piece: PieceKind,
    pub cost: u32,
    pub transition_order: u32,
    pub action_key: GeometryActionKey,
    pub terminal_evidence: Option<TerminalEvidenceLabel>,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedFinesseLanguage {
    pub nodes: Vec<PreparedFinesseNode>,
    pub edges: Vec<PreparedFinesseEdge>,
    pub root: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct FinesseAnnotationGroupKey {
    occupied: [u64; 4],
    piece: PieceKind,
}

impl FinesseAnnotationGroupKey {
    pub(super) fn new(board: FinesseBoard, piece: PieceKind) -> Self {
        Self {
            occupied: board.occupied().words(),
            piece,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GroupedFinesseTarget {
    edge_index: usize,
    target: FinesseTarget,
    terminal_evidence: Option<TerminalEvidenceLabel>,
    allowed: bool,
}

impl GroupedFinesseTarget {
    pub(super) const fn new(
        edge_index: usize,
        target: FinesseTarget,
        terminal_evidence: Option<TerminalEvidenceLabel>,
        allowed: bool,
    ) -> Self {
        Self {
            edge_index,
            target,
            terminal_evidence,
            allowed,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FrozenFinesseTargetGroup {
    board: FinesseBoard,
    piece: PieceKind,
    targets: Vec<GroupedFinesseTarget>,
}

pub(super) type FrozenFinesseTargetGroups =
    BTreeMap<FinesseAnnotationGroupKey, FrozenFinesseTargetGroup>;

pub(super) fn push_frozen_finesse_target(
    groups: &mut FrozenFinesseTargetGroups,
    board: FinesseBoard,
    piece: PieceKind,
    target: GroupedFinesseTarget,
) {
    // Spawn, kick rules, and the classic-input finesse profile are fixed by one
    // request. Board and piece are therefore the complete traversal key.
    groups
        .entry(FinesseAnnotationGroupKey::new(board, piece))
        .or_insert_with(|| FrozenFinesseTargetGroup {
            board,
            piece,
            targets: Vec::new(),
        })
        .targets
        .push(target);
}

pub(super) fn finesse_scoring_edge_requirement(
    spin_coverage_requested: bool,
    b2b_policy: Option<BackToBackEdgePolicy>,
    edge: ScoringExecutionEdge,
) -> (bool, bool) {
    let allowed = b2b_policy.is_none_or(|policy| policy.allows(edge));
    let exact_evidence_required = spin_coverage_requested
        || b2b_policy.is_some_and(|policy| policy.requires_recognized_spin(edge));
    (allowed, exact_evidence_required)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn annotate_frozen_finesse_target_groups(
    groups: FrozenFinesseTargetGroups,
    spawn: clearra_rules::spawn::SpawnProfile,
    kicks: &clearra_rules::kicks::KickTableProfile,
    edge_costs: &mut [Option<u32>],
    edge_terminal_evidence: &mut [Option<TerminalEvidenceLabel>],
    control: &ExecutionControl,
    movement_error: &'static str,
    scoring_error: &'static str,
) -> Result<usize, WasmExactSearchError> {
    let mut query_traversals = 0_usize;
    for group in groups.into_values() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        query_traversals += 1;
        let query = FrozenFinesseQuery::new(
            group.board,
            group.piece,
            spawn,
            kicks.clone(),
            group
                .targets
                .iter()
                .map(|target| target.target)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let exact_evidence_required = group
            .targets
            .iter()
            .any(|target| target.allowed && target.terminal_evidence.is_some());
        let costs = if exact_evidence_required {
            let evidence = group
                .targets
                .iter()
                .map(|target| target.allowed.then_some(target.terminal_evidence).flatten())
                .collect::<Vec<_>>();
            query
                .costs_for_terminal_evidence(&evidence)
                .map_err(|_| WasmExactSearchError::InvalidProblem(scoring_error))?
        } else {
            query
                .costs()
                .map_err(|_| WasmExactSearchError::InvalidProblem(movement_error))?
        };
        for (target, cost) in group.targets.iter().zip(costs.as_slice()) {
            let Some(edge_cost) = edge_costs.get_mut(target.edge_index) else {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_grouped_edge_index_invalid",
                ));
            };
            *edge_cost = target.allowed.then_some(*cost).flatten();
            let Some(edge_evidence) = edge_terminal_evidence.get_mut(target.edge_index) else {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_grouped_evidence_index_invalid",
                ));
            };
            *edge_evidence = target.allowed.then_some(target.terminal_evidence).flatten();
        }
    }
    Ok(query_traversals)
}

#[cfg(test)]
mod finesse_target_grouping_tests {
    use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
    use clearra_replay::ScoringLockEvidence;
    use clearra_rules::{kicks::NoKick, spawn::SpawnProfile};

    use super::*;

    #[test]
    fn compact_repeated_semantic_nodes_share_one_board_piece_query() {
        let layout = Board64Layout::new(BoardSize::new(10, 4).unwrap()).unwrap();
        let board = FinesseBoard::new(layout, 0).unwrap();
        let mut groups = FrozenFinesseTargetGroups::new();

        // These two records model distinct semantic nodes that project to the
        // same physical board. Insertion order and the duplicate target remain
        // one-for-one with their original edge indexes.
        for edge_index in [1, 0] {
            push_frozen_finesse_target(
                &mut groups,
                board,
                PieceKind::O,
                GroupedFinesseTarget::new(
                    edge_index,
                    FinesseTarget::new(RotationState::Zero, 4, 0),
                    None,
                    true,
                ),
            );
        }

        let mut edge_costs = vec![None; 2];
        let mut evidence = vec![None; 2];
        let query_count = annotate_frozen_finesse_target_groups(
            groups,
            SpawnProfile::new(4, 2),
            &NoKick::profile(),
            &mut edge_costs,
            &mut evidence,
            &ExecutionControl::default(),
            "compact_test_movement_failed",
            "compact_test_scoring_failed",
        )
        .unwrap();

        assert_eq!(query_count, 1);
        assert_eq!(edge_costs, vec![Some(1), Some(1)]);
        assert_eq!(evidence, vec![None, None]);
    }

    #[test]
    fn grouped_duplicate_targets_keep_per_edge_evidence_and_allowance() {
        let layout = Board64Layout::new(BoardSize::new(10, 4).unwrap()).unwrap();
        let board = FinesseBoard::new(layout, 0).unwrap();
        let target = FinesseTarget::new(RotationState::Zero, 4, 0);
        let mut groups = FrozenFinesseTargetGroups::new();
        push_frozen_finesse_target(
            &mut groups,
            board,
            PieceKind::O,
            GroupedFinesseTarget::new(2, target, None, false),
        );
        push_frozen_finesse_target(
            &mut groups,
            board,
            PieceKind::O,
            GroupedFinesseTarget::new(1, target, None, true),
        );
        push_frozen_finesse_target(
            &mut groups,
            board,
            PieceKind::O,
            GroupedFinesseTarget::new(0, target, Some(TerminalEvidenceLabel::NoRotation), true),
        );

        let mut edge_costs = vec![None; 3];
        let mut evidence = vec![None; 3];
        let query_count = annotate_frozen_finesse_target_groups(
            groups,
            SpawnProfile::new(4, 2),
            &NoKick::profile(),
            &mut edge_costs,
            &mut evidence,
            &ExecutionControl::default(),
            "compact_test_movement_failed",
            "compact_test_scoring_failed",
        )
        .unwrap();

        assert_eq!(query_count, 1);
        assert_eq!(edge_costs, vec![Some(1), Some(1), None]);
        assert_eq!(
            evidence,
            vec![Some(TerminalEvidenceLabel::NoRotation), None, None]
        );
    }

    #[test]
    fn b2b_only_requires_exact_evidence_for_spin_dependent_clears() {
        let policy = BackToBackEdgePolicy::new(SpinProfileSelection::TSpins);
        let edge = |cleared_lines, perfect_clear| {
            ScoringExecutionEdge::new(
                1,
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
            .with_perfect_clear(perfect_clear)
        };

        assert_eq!(
            finesse_scoring_edge_requirement(false, Some(policy), edge(0, false)),
            (true, false)
        );
        assert_eq!(
            finesse_scoring_edge_requirement(false, Some(policy), edge(4, false)),
            (true, false)
        );
        assert_eq!(
            finesse_scoring_edge_requirement(false, Some(policy), edge(1, true)),
            (true, false)
        );
        assert_eq!(
            finesse_scoring_edge_requirement(false, Some(policy), edge(1, false)),
            (false, true)
        );
        assert_eq!(
            finesse_scoring_edge_requirement(true, Some(policy), edge(0, false)),
            (true, true)
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildReachabilityMode {
    Existing,
    GeometryOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BuildCompletion {
    ClearToEmpty,
    ExactBoardAfterLineClears(u64),
}

impl BuildCompletion {
    pub(super) fn accepts(self, projection: &mut CandidateProjection, subset: usize) -> bool {
        let (board, _deleted_rows) = projection.state(subset);
        match self {
            Self::ClearToEmpty => board == 0,
            Self::ExactBoardAfterLineClears(expected) => board == expected,
        }
    }
}

#[derive(Default)]
pub(super) struct BuildUpWorkspace {
    realization_feasibility: RealizationFeasibilityWorkspace,
    piece_order_languages: PieceOrderLanguageCache,
    standard_bag_coverage: Option<StandardBagCoverage>,
    standard_bag_coverage_initialized: bool,
    observation_language_roots: Vec<u32>,
    reachability: ReachabilityWorkspace,
    graph_nodes: Vec<BuildNode>,
    graph_edges: Vec<BuildEdge>,
    graph_piece_edges: Vec<BuildEdge>,
    graph_edge_scratch: Vec<BuildEdge>,
    graph_reachable_generations: Vec<u32>,
    graph_subset_node_ids: Vec<u32>,
    graph_subset_queue: Vec<u16>,
    graph_generation: u32,
    projection_deleted_rows: Vec<u16>,
    projection_physical_boards: Vec<u64>,
    projection_state_generations: Vec<u32>,
    projection_generation: u32,
}

impl BuildUpWorkspace {
    pub fn retained_bytes(&self) -> usize {
        self.realization_feasibility.retained_bytes()
            + self.piece_order_languages.retained_bytes()
            + self
                .standard_bag_coverage
                .as_ref()
                .map_or(0, StandardBagCoverage::retained_bytes)
            + self.reachability.retained_bytes()
            + self.graph_nodes.capacity() * core::mem::size_of::<BuildNode>()
            + (self.graph_edges.capacity()
                + self.graph_piece_edges.capacity()
                + self.graph_edge_scratch.capacity())
                * core::mem::size_of::<BuildEdge>()
            + self.projection_physical_boards.capacity() * core::mem::size_of::<u64>()
            + self.projection_deleted_rows.capacity() * core::mem::size_of::<u16>()
            + self.projection_state_generations.capacity() * core::mem::size_of::<u32>()
            + self.graph_reachable_generations.capacity() * core::mem::size_of::<u32>()
            + self.graph_subset_node_ids.capacity() * core::mem::size_of::<u32>()
            + self.graph_subset_queue.capacity() * core::mem::size_of::<u16>()
            + self.observation_language_roots.capacity() * core::mem::size_of::<u32>()
    }

    pub const fn piece_language_coverage_hits(&self) -> usize {
        self.piece_order_languages.coverage_hits()
    }

    pub const fn piece_language_coverage_misses(&self) -> usize {
        self.piece_order_languages.coverage_misses()
    }

    pub const fn standard_bag_coverage_hits(&self) -> usize {
        match self.standard_bag_coverage.as_ref() {
            Some(coverage) => coverage.root_cache_hits(),
            None => 0,
        }
    }

    pub const fn standard_bag_coverage_misses(&self) -> usize {
        match self.standard_bag_coverage.as_ref() {
            Some(coverage) => coverage.root_cache_misses(),
            None => 0,
        }
    }

    pub const fn standard_bag_coverage_complete(&self) -> bool {
        match self.standard_bag_coverage.as_ref() {
            Some(coverage) => coverage.global_is_complete(),
            None => false,
        }
    }

    pub const fn reachability_metrics(&self) -> super::reachability::ReachabilityMetrics {
        self.reachability.metrics()
    }

    pub fn merge_standard_bag_coverage(&mut self, root: u32) -> Result<(), WasmExactSearchError> {
        {
            let coverage =
                self.standard_bag_coverage
                    .as_mut()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_coverage_not_initialized",
                    ))?;
            coverage.merge_global(root)?;
        }
        self.recycle_symbolic_caches_if_needed()
    }

    pub fn materialize_standard_bag_coverage(
        &mut self,
    ) -> Result<Option<PatternBitSet>, WasmExactSearchError> {
        self.standard_bag_coverage
            .as_mut()
            .map(StandardBagCoverage::materialize_global)
            .transpose()
    }

    pub fn materialize_standard_bag_root(
        &self,
        root: u32,
    ) -> Result<PatternBitSet, WasmExactSearchError> {
        self.standard_bag_coverage
            .as_ref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_not_initialized",
            ))?
            .materialize_root(root)
    }

    pub fn merge_observation_language(&mut self, root: u32) -> Result<(), WasmExactSearchError> {
        self.observation_language_roots
            .try_reserve(1)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_observation_language_root_storage_unavailable",
                )
            })?;
        self.observation_language_roots.push(root);
        Ok(())
    }

    pub fn evaluate_observation_language(
        &mut self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<Option<QueueObservationCoverage>, WasmExactSearchError> {
        if self.observation_language_roots.is_empty() {
            return Ok(None);
        }
        self.observation_language_roots.sort_unstable();
        self.observation_language_roots.dedup();
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let pattern_index = PatternPiecePositionIndex::compile(universe).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_pattern_index_compile_failed")
        })?;
        let mut evaluator = QueueObservationPolicyEvaluator::new(
            universe,
            &pattern_index,
            problem.queue_observation_policy(),
            problem.initial_hold().cursor(),
            problem.initial_hold().hold_piece(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            problem.supply().projects_standard_bag_lookahead(),
            None,
        )?;
        let language = RootedPieceLanguageUnion::new(
            &self.piece_order_languages,
            &self.observation_language_roots,
        )?;
        let mut result = evaluator.evaluate(&language, control)?;
        result.metrics.retained_bytes = result
            .metrics
            .retained_bytes
            .saturating_add(language.retained_bytes());
        Ok(Some(result))
    }

    fn cover_standard_bag_language(
        &mut self,
        problem: &SearchProblem,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<Option<StandardBagCoverageResult>, WasmExactSearchError> {
        if !self.standard_bag_coverage_initialized {
            let universe = problem.piece_source().materialized_universe().ok_or(
                WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
            )?;
            self.standard_bag_coverage = StandardBagCoverage::for_universe(
                universe,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                problem.supply().projects_unplaced_lookahead(),
            )?;
            self.standard_bag_coverage_initialized = true;
        }
        let Some(coverage) = self.standard_bag_coverage.as_mut() else {
            return Ok(None);
        };
        coverage
            .cover_language(&self.piece_order_languages, language_root, control)
            .map(Some)
    }

    fn recycle_symbolic_caches_if_needed(&mut self) -> Result<(), WasmExactSearchError> {
        let standard_live = self
            .standard_bag_coverage
            .as_ref()
            .map_or(0, StandardBagCoverage::local_live_bytes);
        if standard_live.saturating_add(self.piece_order_languages.local_live_bytes())
            <= SYMBOLIC_CACHE_LIVE_LIMIT
        {
            return Ok(());
        }
        if let Some(coverage) = self.standard_bag_coverage.as_mut() {
            coverage.flush_and_recycle_local_cache()?;
        }
        if !self.observation_language_roots.is_empty() {
            self.piece_order_languages.clear_coverage_caches();
        } else {
            self.piece_order_languages.clear_retain_capacity();
        }
        Ok(())
    }

    fn recycle_graph(&mut self, mut graph: BuildOrderGraph) {
        graph.nodes.clear();
        graph.edges.clear();
        graph.piece_edges.clear();
        self.graph_nodes = graph.nodes;
        self.graph_edges = graph.edges;
        self.graph_piece_edges = graph.piece_edges;
    }

    fn recycle_projection(&mut self, projection: CandidateProjection) {
        self.projection_deleted_rows = projection.deleted_rows;
        self.projection_physical_boards = projection.physical_boards;
        self.projection_state_generations = projection.state_generations;
    }

    fn begin_graph_generation(&mut self, state_count: usize) -> Result<u32, WasmExactSearchError> {
        if self.graph_reachable_generations.len() < state_count {
            self.graph_reachable_generations
                .try_reserve_exact(state_count - self.graph_reachable_generations.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_order_reachable_storage_unavailable",
                    )
                })?;
            self.graph_reachable_generations.resize(state_count, 0);
            self.graph_subset_node_ids
                .try_reserve_exact(state_count - self.graph_subset_node_ids.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_order_subset_index_storage_unavailable",
                    )
                })?;
            self.graph_subset_node_ids.resize(state_count, 0);
        }
        self.graph_generation = self.graph_generation.wrapping_add(1);
        if self.graph_generation == 0 {
            self.graph_reachable_generations.fill(0);
            self.graph_generation = 1;
        }
        Ok(self.graph_generation)
    }
}

pub(super) struct CandidateProjection {
    operation_cells: [u64; super::MAX_BOARD64_PIECES],
    operation_count: u8,
    row_contributors: [u16; 16],
    width: u8,
    height: u8,
    initial_board: u64,
    completed_target_rows: u16,
    deleted_rows: Vec<u16>,
    physical_boards: Vec<u64>,
    state_generations: Vec<u32>,
    generation: u32,
    pub all_placed: usize,
}

/// Conservative constructor/live-set peak for one serial candidate check.
///
/// The exact graph shape is not known before construction, so this deliberately
/// uses the full subset lattice and every operation edge, plus the complete
/// graph x hold-product state space. It is an upper bound, not an exact retained
/// claim. The final factor of two covers replacement/recycling moments where a
/// prior workspace allocation and its larger successor coexist.
pub(super) fn checked_candidate_verification_peak_upper_bound(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    retain_trace: bool,
) -> Option<u128> {
    let operation_count = candidate.row_ids().len();
    if operation_count == 0 || operation_count > super::MAX_BOARD64_PIECES {
        return None;
    }
    let subset_count = 1_u128.checked_shl(u32::try_from(operation_count).ok()?)?;
    let edge_count = subset_count.checked_mul(operation_count as u128)?;
    let product_state_count = subset_count
        .checked_mul(super::coverage_product::EXTRA_DRAW_STATE_COUNT as u128)?
        .checked_mul(super::coverage_product::HOLD_STATE_COUNT as u128)?;
    let universe = problem.piece_source().materialized_universe()?;
    let pattern_word_count = (universe.pattern_count() as u128).checked_add(63)? / 64;

    let projection_and_feasibility_per_subset = (core::mem::size_of::<u16>()
        + core::mem::size_of::<u64>()
        + core::mem::size_of::<u32>()
        + core::mem::size_of::<u32>() * 3
        + core::mem::size_of::<u16>() * 2) as u128;
    let graph_per_subset = (core::mem::size_of::<BuildNode>()
        + core::mem::size_of::<u32>() * 2
        + core::mem::size_of::<u16>()) as u128;
    let graph_edge_bytes = edge_count
        .checked_mul(core::mem::size_of::<BuildEdge>() as u128)?
        // operation, piece-deduplicated, scratch, and prepared copies
        .checked_mul(4)?;
    let product_bytes = product_state_count.checked_mul(
        (core::mem::size_of::<u64>()
            + core::mem::size_of::<u32>()
            + core::mem::size_of::<usize>()
            + core::mem::size_of::<u128>() * u64::BITS as usize) as u128,
    )?;
    let pattern_bytes = pattern_word_count
        .checked_mul(core::mem::size_of::<u64>() as u128)?
        .checked_mul(6)?;
    let language_cache_bytes = edge_count.checked_mul((core::mem::size_of::<u64>() * 8) as u128)?;
    let trace_bytes = if retain_trace {
        (operation_count as u128).checked_mul(core::mem::size_of::<CorePathStep>() as u128)?
    } else {
        0
    };
    let reachability_bytes = checked_reachability_retained_upper_bound(
        catalog.width(),
        catalog.height(),
        problem.kick_profile().profile_id(),
    )?;

    subset_count
        .checked_mul(projection_and_feasibility_per_subset)?
        .checked_add(subset_count.checked_mul(graph_per_subset)?)?
        .checked_add(graph_edge_bytes)?
        .checked_add(product_bytes)?
        .checked_add(pattern_bytes)?
        .checked_add(language_cache_bytes)?
        .checked_add(trace_bytes)?
        .checked_add(reachability_bytes)?
        .checked_add(core::mem::size_of::<CandidateBuildResult>() as u128)?
        .checked_mul(2)
}

impl CandidateProjection {
    pub fn compile(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        workspace: &mut BuildUpWorkspace,
        _completion: BuildCompletion,
    ) -> Result<Self, WasmExactSearchError> {
        let operation_count = candidate.row_ids().len();
        if operation_count == 0 || operation_count > super::MAX_BOARD64_PIECES {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_candidate_projection_operation_count_invalid",
            ));
        }
        let state_count = 1_usize << operation_count;
        let all_placed = state_count - 1;
        let row_bits = full_row_mask(catalog.width());
        let mut operation_cells = [0_u64; super::MAX_BOARD64_PIECES];
        let mut row_contributors = [0_u16; 16];
        for (operation_index, row_id) in candidate.row_ids().iter().copied().enumerate() {
            let cells = catalog.skeleton(row_id).cells;
            operation_cells[operation_index] = cells;
            let mut occupied = occupied_rows(catalog.width(), cells);
            while occupied != 0 {
                let row = occupied.trailing_zeros() as usize;
                occupied &= occupied - 1;
                row_contributors[row] |= 1_u16 << operation_index;
            }
        }
        for row in 0..catalog.height() as usize {
            let initial_row =
                (catalog.initial_board() >> (row * catalog.width() as usize)) & row_bits;
            if initial_row == row_bits && row_contributors[row] == 0 {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_initial_full_row_requires_preclear_normalization",
                ));
            }
        }
        let mut completed_target_rows = 0_u16;
        let mut final_board = catalog.initial_board();
        for &cells in operation_cells.iter().take(operation_count) {
            final_board |= cells;
        }
        for row in 0..catalog.height() as usize {
            let target_row = (final_board >> (row * catalog.width() as usize)) & row_bits;
            if target_row == row_bits {
                completed_target_rows |= 1_u16 << row;
            }
        }

        let mut deleted_rows = core::mem::take(&mut workspace.projection_deleted_rows);
        let mut physical_boards = core::mem::take(&mut workspace.projection_physical_boards);
        let mut state_generations = core::mem::take(&mut workspace.projection_state_generations);
        reserve_state_storage(&mut deleted_rows, state_count, 0_u16)?;
        reserve_state_storage(&mut physical_boards, state_count, 0_u64)?;
        reserve_state_storage(&mut state_generations, state_count, 0_u32)?;
        workspace.projection_generation = workspace.projection_generation.wrapping_add(1);
        if workspace.projection_generation == 0 {
            state_generations.fill(0);
            workspace.projection_generation = 1;
        }
        Ok(Self {
            operation_cells,
            operation_count: operation_count as u8,
            row_contributors,
            width: catalog.width(),
            height: catalog.height(),
            initial_board: catalog.initial_board(),
            completed_target_rows,
            deleted_rows,
            physical_boards,
            state_generations,
            generation: workspace.projection_generation,
            all_placed,
        })
    }

    pub fn operation_count(&self) -> usize {
        usize::from(self.operation_count)
    }

    pub fn state_count(&self) -> usize {
        self.all_placed + 1
    }

    pub fn state(&mut self, subset: usize) -> (u64, u16) {
        debug_assert!(subset <= self.all_placed);
        if self.state_generations[subset] != self.generation {
            let deleted = self.expected_deleted_rows(subset);
            let mut logical_board = self.initial_board;
            let mut selected = subset;
            while selected != 0 {
                let operation_index = selected.trailing_zeros() as usize;
                selected &= selected - 1;
                logical_board |= self.operation_cells[operation_index];
            }
            self.deleted_rows[subset] = deleted;
            self.physical_boards[subset] =
                compact_target_board(self.width, self.height, logical_board, deleted);
            self.state_generations[subset] = self.generation;
        }
        (self.physical_boards[subset], self.deleted_rows[subset])
    }

    pub fn expected_deleted_rows(&self, subset: usize) -> u16 {
        let subset_bits = subset as u16;
        let mut deleted = 0_u16;
        for row in 0..usize::from(self.height) {
            if self.completed_target_rows & (1_u16 << row) == 0 {
                continue;
            }
            let contributors = self.row_contributors[row];
            if contributors != 0 && contributors & !subset_bits == 0 {
                deleted |= 1_u16 << row;
            }
        }
        deleted
    }

    pub fn confirm_transition(&mut self, subset: usize, board: u64, deleted_rows: u16) -> bool {
        debug_assert!(subset <= self.all_placed);
        if deleted_rows != self.expected_deleted_rows(subset) {
            return false;
        }
        if self.state_generations[subset] == self.generation {
            return self.deleted_rows[subset] == deleted_rows
                && self.physical_boards[subset] == board;
        }
        self.deleted_rows[subset] = deleted_rows;
        self.physical_boards[subset] = board;
        self.state_generations[subset] = self.generation;
        true
    }
}

fn reserve_state_storage<T: Copy>(
    storage: &mut Vec<T>,
    state_count: usize,
    empty: T,
) -> Result<(), WasmExactSearchError> {
    if storage.len() < state_count {
        storage
            .try_reserve_exact(state_count - storage.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_projection_storage_unavailable")
            })?;
        storage.resize(state_count, empty);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CachedWitnessTransition {
    Unknown,
    Impossible,
    Legal(BuildEdge),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct WitnessProductState {
    subset: u16,
    extra_draw: u8,
    hold_code: u8,
    terminal_projection_consumed: bool,
    active_patterns: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FixedWitnessProductState {
    subset: u16,
    extra_draw: u8,
    hold_code: u8,
    terminal_projection_consumed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CandidateWitnessMode {
    Disabled,
    MembershipOnly,
    ExactSinglePatternCoverage,
}

impl CandidateWitnessMode {
    pub(super) fn for_candidate(
        problem: &SearchProblem,
        target: &TargetGroup,
        coverage_already_known: bool,
        solution_coverage_required: bool,
    ) -> Self {
        if solution_coverage_required
            || problem.objective().execution_constraints().requested()
            || problem
                .queue_observation_policy()
                .requires_observation_policy()
            || problem.count_policy() != PcCountPolicy::CountUnique
            || !target.single_pattern_witness_is_exact()
        {
            return Self::Disabled;
        }
        if coverage_already_known {
            return Self::MembershipOnly;
        }
        if problem.piece_source().fixed_sequence().is_some() {
            return Self::ExactSinglePatternCoverage;
        }
        Self::Disabled
    }

    const fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    const fn returns_coverage(self) -> bool {
        matches!(self, Self::ExactSinglePatternCoverage)
    }
}

#[derive(Clone, Copy)]
struct FixedWitnessBranch {
    desired_piece: u8,
    next_extra_draw: u8,
    next_hold_code: u8,
    terminal_projection_consumed: bool,
    hold_kind: &'static str,
}

const EMPTY_FIXED_WITNESS_BRANCH: FixedWitnessBranch = FixedWitnessBranch {
    desired_piece: 0,
    next_extra_draw: 0,
    next_hold_code: 0,
    terminal_projection_consumed: false,
    hold_kind: "",
};

struct FixedWitnessBranches {
    values: [FixedWitnessBranch; 3],
    len: usize,
}

impl FixedWitnessBranches {
    fn push(&mut self, branch: FixedWitnessBranch) {
        self.values[self.len] = branch;
        self.len += 1;
    }

    fn iter(&self) -> impl Iterator<Item = FixedWitnessBranch> + '_ {
        self.values[..self.len].iter().copied()
    }
}

#[derive(Clone, Copy)]
struct WitnessStep {
    edge: BuildEdge,
    hold_kind: &'static str,
}

struct CandidateWitness {
    global_pattern_index: usize,
    path: Vec<WitnessStep>,
    visited_product_states: usize,
    retained_bytes: usize,
}

pub(super) fn verify_candidate(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    profile_scale: u64,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    verify_candidate_for_completion(
        problem,
        catalog,
        candidate,
        target,
        workspace,
        evaluator,
        witness_mode,
        retain_trace,
        profile_scale,
        BuildCompletion::ClearToEmpty,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_candidate_for_completion(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    profile_scale: u64,
    completion: BuildCompletion,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    verify_candidate_for_completion_mode(
        problem,
        catalog,
        candidate,
        target,
        workspace,
        evaluator,
        witness_mode,
        retain_trace,
        profile_scale,
        completion,
        false,
        false,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_candidate_for_completion_with_finesse(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    profile_scale: u64,
    completion: BuildCompletion,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    verify_candidate_for_completion_mode(
        problem,
        catalog,
        candidate,
        target,
        workspace,
        evaluator,
        witness_mode,
        retain_trace,
        profile_scale,
        completion,
        true,
        spin_coverage_requested,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_candidate_for_completion_mode(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    profile_scale: u64,
    completion: BuildCompletion,
    finesse_requested: bool,
    finesse_spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    let kick_profile_id = problem.kick_profile().profile_id();
    if super::kick_profiles::builtin_kick_profile(kick_profile_id).is_none() {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_exact_backend_requires_connected_kick_profile",
        ));
    }
    if RealizationFeasibilityWorkspace::dependency_relaxation_is_infeasible(catalog, candidate) {
        return Ok(infeasible_candidate_result(0));
    }
    workspace.reachability.configure(candidate.row_ids().len());
    workspace
        .reachability
        .configure_kick_profile(kick_profile_id);
    let projection_span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::WasmCandidateProjection, profile_scale);
    let mut projection = CandidateProjection::compile(catalog, candidate, workspace, completion)?;
    projection_span.finish(projection.state_count() as u64);
    let result = verify_candidate_with_projection(
        problem,
        catalog,
        candidate,
        target,
        &mut projection,
        workspace,
        evaluator,
        witness_mode,
        retain_trace,
        profile_scale,
        completion,
        finesse_requested,
        finesse_spin_coverage_requested,
        control,
    );
    workspace.recycle_projection(projection);
    result
}

#[allow(clippy::too_many_arguments)]
fn verify_candidate_with_projection(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    projection: &mut CandidateProjection,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    profile_scale: u64,
    completion: BuildCompletion,
    finesse_requested: bool,
    finesse_spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    let universe = problem.piece_source().materialized_universe().ok_or(
        WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
    )?;
    let feasibility_span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::WasmCandidateFeasibility, profile_scale);
    let feasibility = workspace.realization_feasibility.analyze(
        catalog,
        candidate,
        projection,
        completion,
        problem.backend_policy().precompute_build_dependencies(),
        control,
    )?;
    let feasibility_states = feasibility.explored_states();
    feasibility_span.finish(feasibility_states as u64);
    if feasibility.is_infeasible() {
        return Ok(infeasible_candidate_result(feasibility_states));
    }
    if !finesse_requested
        && witness_mode.enabled()
        && problem.count_policy() == PcCountPolicy::CountUnique
        && target.pattern_index.is_some()
    {
        return verify_first_pattern_witness(
            problem,
            catalog,
            candidate,
            target,
            projection,
            workspace,
            witness_mode,
            retain_trace,
            feasibility_states,
            profile_scale,
            completion,
            feasibility,
            control,
        );
    }
    let graph_span = SearchStageSpan::begin_scaled(
        ExecutorSearchStage::WasmBuildOrderReachability,
        profile_scale,
    );
    let finesse_geometry_span =
        finesse_requested.then(|| SearchStageSpan::begin(ExecutorSearchStage::FinesseGeometry));
    let mut graph = BuildOrderGraph::build(
        problem,
        catalog,
        candidate,
        projection,
        workspace,
        problem.count_policy() == PcCountPolicy::CountUnique,
        completion,
        Some(feasibility),
        if finesse_requested {
            BuildReachabilityMode::GeometryOnly
        } else {
            BuildReachabilityMode::Existing
        },
    )?;
    graph_span.finish(graph.nodes.len() as u64);
    if let Some(span) = finesse_geometry_span {
        span.finish(graph.nodes.len() as u64);
    }
    let finesse_language = if finesse_requested {
        match annotate_and_prune_finesse_graph(
            problem,
            catalog,
            projection,
            &mut graph,
            &mut workspace.reachability,
            finesse_spin_coverage_requested,
            control,
        ) {
            Ok(language) => Some(language),
            Err(error) => {
                workspace.recycle_graph(graph);
                return Err(error);
            }
        }
    } else {
        None
    };
    let mut covered_patterns = None;
    let mut symbolic_coverage_root = None;
    let mut observation_language_root = None;
    let mut symbolic_covered_pattern_count = 0usize;
    let mut witness_pattern_id = None;
    let mut build_variant_count = 0_u128;
    let mut count_complete = true;
    let mut representative_path = Vec::new();
    let mut coverage_product_words = 0usize;
    let mut coverage_product_states = 0usize;
    let mut coverage_product_edge_checks = 0usize;
    let coverage_span = SearchStageSpan::begin_scaled(
        ExecutorSearchStage::WasmCoverageLanguageProduct,
        profile_scale,
    );
    if graph.nodes[graph.root as usize].live {
        let count_paths = problem.count_policy() == PcCountPolicy::CountAll;
        if finesse_requested {
            let Some(pattern_index) = target.pattern_index.as_deref() else {
                workspace.recycle_graph(graph);
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_concrete_pattern_index_not_compiled",
                ));
            };
            let prepared = finesse_language
                .as_ref()
                .expect("requested finesse annotation produced a language");
            let product = match evaluator.evaluate_with_finesse(
                &graph,
                pattern_index,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                problem.supply().projects_unplaced_lookahead(),
                problem.supply().projects_standard_bag_lookahead(),
                count_paths,
                false,
                &prepared.nodes,
                problem.spawn_profile(),
                control,
            ) {
                Ok(product) => product,
                Err(error) => {
                    workspace.recycle_graph(graph);
                    return Err(error);
                }
            };
            if !product.coverage_bits.is_empty() {
                witness_pattern_id = product
                    .coverage_bits
                    .first_pattern()
                    .map(|pattern| pattern.index() as u32);
                covered_patterns = Some(product.coverage_bits);
            }
            build_variant_count = product.path_count;
            count_complete = product.count_complete;
            coverage_product_words = product.processed_words;
            coverage_product_states = product.active_states;
            coverage_product_edge_checks = product.edge_checks;
        } else {
            let mut canonical_language_id = None;
            if problem
                .queue_observation_policy()
                .requires_observation_policy()
            {
                let language_id = match workspace.piece_order_languages.canonicalize(&graph) {
                    Ok(language_id) => language_id,
                    Err(error) => {
                        workspace.recycle_graph(graph);
                        return Err(error);
                    }
                };
                canonical_language_id = Some(language_id);
                observation_language_root = Some(language_id);
            }
            if count_paths {
                let Some(pattern_index) = target.pattern_index.as_deref() else {
                    workspace.recycle_graph(graph);
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_concrete_pattern_index_not_compiled",
                    ));
                };
                let product = match evaluator.evaluate(
                    &graph,
                    pattern_index,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                    problem.supply().projects_standard_bag_lookahead(),
                    true,
                    false,
                    control,
                ) {
                    Ok(product) => product,
                    Err(error) => {
                        workspace.recycle_graph(graph);
                        return Err(error);
                    }
                };
                if !product.coverage_bits.is_empty() {
                    covered_patterns = Some(product.coverage_bits);
                }
                build_variant_count = product.path_count;
                count_complete = product.count_complete;
                coverage_product_words = product.processed_words;
                coverage_product_states = product.active_states;
                coverage_product_edge_checks = product.edge_checks;
            } else {
                let language_id = if let Some(language_id) = canonical_language_id {
                    language_id
                } else {
                    match workspace.piece_order_languages.canonicalize(&graph) {
                        Ok(language_id) => language_id,
                        Err(error) => {
                            workspace.recycle_graph(graph);
                            return Err(error);
                        }
                    }
                };
                let symbolic =
                    match workspace.cover_standard_bag_language(problem, language_id, control) {
                        Ok(symbolic) => symbolic,
                        Err(error) => {
                            workspace.recycle_graph(graph);
                            return Err(error);
                        }
                    };
                if let Some(symbolic) = symbolic {
                    coverage_product_states = symbolic.product_states;
                    coverage_product_edge_checks = symbolic.edge_checks;
                    if symbolic.covers_any_pattern {
                        symbolic_coverage_root = Some(symbolic.root);
                        symbolic_covered_pattern_count = symbolic.covered_pattern_count;
                        witness_pattern_id = symbolic.witness_pattern_id;
                    }
                } else {
                    let Some(pattern_index) = target.pattern_index.as_deref() else {
                        workspace.recycle_graph(graph);
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_concrete_pattern_index_not_compiled",
                        ));
                    };
                    let cache_lookup = match workspace
                        .piece_order_languages
                        .coverage(language_id, target.pattern_index_id)
                    {
                        Ok(lookup) => lookup,
                        Err(error) => {
                            workspace.recycle_graph(graph);
                            return Err(error);
                        }
                    };
                    let coverage = match cache_lookup {
                        CoverageCacheLookup::Hit(local_words) => {
                            match target
                                .pattern_index
                                .as_deref()
                                .expect("generic coverage requires its compiled pattern index")
                                .expand_coverage_words(local_words.as_ref())
                            {
                                Ok(coverage) => coverage,
                                Err(_) => {
                                    workspace.recycle_graph(graph);
                                    return Err(WasmExactSearchError::InvalidProblem(
                                        "wasm_cached_coverage_expansion_failed",
                                    ));
                                }
                            }
                        }
                        CoverageCacheLookup::Miss {
                            admit_after_compute,
                        } => {
                            let product = match evaluator.evaluate(
                                &graph,
                                pattern_index,
                                problem.initial_hold(),
                                problem.supply().hold_enabled(),
                                problem.supply().projects_unplaced_lookahead(),
                                problem.supply().projects_standard_bag_lookahead(),
                                false,
                                false,
                                control,
                            ) {
                                Ok(product) => product,
                                Err(error) => {
                                    workspace.recycle_graph(graph);
                                    return Err(error);
                                }
                            };
                            coverage_product_words = product.processed_words;
                            coverage_product_states = product.active_states;
                            coverage_product_edge_checks = product.edge_checks;
                            if admit_after_compute {
                                if let Err(error) = workspace.piece_order_languages.insert_coverage(
                                    language_id,
                                    target.pattern_index_id,
                                    evaluator.local_coverage_words(),
                                ) {
                                    workspace.recycle_graph(graph);
                                    return Err(error);
                                }
                            }
                            product.coverage_bits
                        }
                    };
                    if !coverage.is_empty() {
                        witness_pattern_id = coverage
                            .first_pattern()
                            .map(|pattern| pattern.index() as u32);
                        covered_patterns = Some(coverage);
                    }
                }
            }
        }
        if retain_trace && representative_path.is_empty() {
            let pattern_id = witness_pattern_id
                .map(|pattern| {
                    clearra_coverage::pattern::pattern_id::PatternId::new(pattern as usize)
                })
                .or_else(|| {
                    covered_patterns
                        .as_ref()
                        .and_then(PatternBitSet::first_pattern)
                });
            if let Some(pattern_id) = pattern_id {
                let sequence = universe.sequence(pattern_id);
                representative_path = first_pattern_path(
                    &graph,
                    sequence.as_ref(),
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                    problem.supply().projects_standard_bag_lookahead(),
                    problem.initial_hold(),
                    CoverageState {
                        node: graph.root,
                        cursor: problem.initial_hold().cursor(),
                        hold: problem.initial_hold().hold_piece(),
                    },
                    finesse_language.as_ref().map(|language| FinessePathGuard {
                        nodes: &language.nodes,
                        spawn_profile: problem.spawn_profile(),
                    }),
                );
            }
        }
    }
    coverage_span.finish(coverage_product_edge_checks as u64);

    let graph_nodes = graph.nodes.len();
    let reachability_states = graph.reachability_states;
    workspace.recycle_graph(graph);
    Ok(CandidateBuildResult {
        buildable: covered_patterns.is_some() || symbolic_coverage_root.is_some(),
        witness_pattern_id: witness_pattern_id.or_else(|| {
            covered_patterns
                .as_ref()
                .and_then(PatternBitSet::first_pattern)
                .map(|pattern| pattern.index() as u32)
        }),
        covered_patterns,
        symbolic_coverage_root,
        observation_language_root,
        symbolic_covered_pattern_count,
        build_variant_count,
        count_complete,
        representative_path,
        graph_nodes,
        coverage_product_words,
        coverage_product_states,
        coverage_product_edge_checks,
        feasibility_states,
        feasibility_rejected: false,
        reachability_states,
        retained_bytes: 0,
        finesse_language,
    })
}

fn infeasible_candidate_result(feasibility_states: usize) -> CandidateBuildResult {
    CandidateBuildResult {
        buildable: false,
        covered_patterns: None,
        symbolic_coverage_root: None,
        observation_language_root: None,
        symbolic_covered_pattern_count: 0,
        witness_pattern_id: None,
        build_variant_count: 0,
        count_complete: true,
        representative_path: Vec::new(),
        graph_nodes: 0,
        coverage_product_words: 0,
        coverage_product_states: 0,
        coverage_product_edge_checks: 0,
        feasibility_states,
        feasibility_rejected: true,
        reachability_states: 0,
        retained_bytes: 0,
        finesse_language: None,
    }
}

fn verify_first_pattern_witness(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    projection: &mut CandidateProjection,
    workspace: &mut BuildUpWorkspace,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    feasibility_states: usize,
    profile_scale: u64,
    completion: BuildCompletion,
    feasibility: RealizationFeasibility,
    control: &ExecutionControl,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    let reachability_states_before = workspace.reachability.generated_state_count();
    let witness_span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::WasmWitnessSearch, profile_scale);
    let witness = find_first_pattern_witness(
        problem,
        catalog,
        candidate,
        target,
        projection,
        workspace,
        completion,
        feasibility,
        control,
    )?;
    let reachability_states = workspace
        .reachability
        .generated_state_count()
        .saturating_sub(reachability_states_before);
    witness_span.finish(reachability_states as u64);
    let Some(witness) = witness else {
        return Ok(CandidateBuildResult {
            buildable: false,
            covered_patterns: None,
            symbolic_coverage_root: None,
            observation_language_root: None,
            symbolic_covered_pattern_count: 0,
            witness_pattern_id: None,
            build_variant_count: 0,
            count_complete: true,
            representative_path: Vec::new(),
            graph_nodes: 0,
            coverage_product_words: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            feasibility_states,
            feasibility_rejected: false,
            reachability_states,
            retained_bytes: 0,
            finesse_language: None,
        });
    };
    let representative_path = if retain_trace {
        witness
            .path
            .iter()
            .map(|step| {
                CorePathStep::new(
                    step.edge.piece,
                    step.edge.rotation.quarter_turns(),
                    i32::from(step.edge.x),
                    i32::from(step.edge.y),
                    step.hold_kind,
                    step.edge.cleared_lines,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let covered_patterns = witness_mode
        .returns_coverage()
        .then(|| target.possible_patterns.as_ref().clone());
    Ok(CandidateBuildResult {
        buildable: true,
        covered_patterns,
        symbolic_coverage_root: None,
        observation_language_root: None,
        symbolic_covered_pattern_count: 0,
        witness_pattern_id: Some(witness.global_pattern_index as u32),
        build_variant_count: 0,
        count_complete: true,
        representative_path,
        graph_nodes: witness.visited_product_states,
        coverage_product_words: 0,
        coverage_product_states: witness.visited_product_states,
        coverage_product_edge_checks: 0,
        feasibility_states,
        feasibility_rejected: false,
        reachability_states,
        retained_bytes: witness.retained_bytes,
        finesse_language: None,
    })
}

fn find_first_pattern_witness(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    projection: &mut CandidateProjection,
    workspace: &mut BuildUpWorkspace,
    completion: BuildCompletion,
    feasibility: RealizationFeasibility,
    control: &ExecutionControl,
) -> Result<Option<CandidateWitness>, WasmExactSearchError> {
    let pattern_index =
        target
            .pattern_index
            .as_deref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_witness_pattern_index_not_compiled",
            ))?;
    let initial_hold_code = match (
        problem.initial_hold().hold_empty(),
        problem.initial_hold().hold_piece(),
    ) {
        (true, None) => 0,
        (false, Some(piece)) => witness_piece_code(piece),
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_initial_hold_state_invalid",
            ));
        }
    };
    let transition_count = projection
        .state_count()
        .checked_mul(projection.operation_count())
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_witness_transition_storage_overflow",
        ))?;
    let mut transition_cache = vec![CachedWitnessTransition::Unknown; transition_count];
    if let Some(fixed_sequence) = problem
        .piece_source()
        .fixed_sequence()
        .filter(|_| !problem.supply().projects_standard_bag_lookahead())
        .filter(|_| pattern_index.local_pattern_count() == 1)
    {
        let mut operation_masks_by_piece = [0_u16; 8];
        for (operation_index, row_id) in candidate.row_ids().iter().copied().enumerate() {
            let piece_code = usize::from(witness_piece_code(catalog.skeleton(row_id).piece));
            operation_masks_by_piece[piece_code] |= 1_u16 << operation_index;
        }
        let mut failed = ExactHashSet::default();
        let mut path = Vec::with_capacity(projection.operation_count());
        let mut visited_product_states = 0usize;
        let root = FixedWitnessProductState {
            subset: 0,
            extra_draw: 0,
            hold_code: initial_hold_code,
            terminal_projection_consumed: false,
        };
        let accepted = visit_fixed_witness_state(
            problem,
            catalog,
            candidate,
            projection,
            fixed_sequence.pieces(),
            &operation_masks_by_piece,
            root,
            workspace,
            &mut transition_cache,
            &mut failed,
            &mut path,
            &mut visited_product_states,
            completion,
            feasibility,
            control,
        )?;
        if !accepted {
            return Ok(None);
        }
        let global_pattern_index =
            pattern_index
                .global_pattern_index(0)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_witness_pattern_index_missing",
                ))?;
        return Ok(Some(CandidateWitness {
            global_pattern_index,
            path,
            visited_product_states,
            retained_bytes: transition_cache.capacity()
                * core::mem::size_of::<CachedWitnessTransition>()
                + failed.capacity() * core::mem::size_of::<FixedWitnessProductState>(),
        }));
    }
    let mut failed = ExactHashSet::default();
    let mut path = Vec::with_capacity(projection.operation_count());
    let mut visited_product_states = 0usize;

    for word_index in 0..pattern_index.word_count() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let active_patterns = pattern_index.active_word(word_index);
        if active_patterns == 0 {
            continue;
        }
        failed.clear();
        path.clear();
        let root = WitnessProductState {
            subset: 0,
            extra_draw: 0,
            hold_code: initial_hold_code,
            terminal_projection_consumed: false,
            active_patterns,
        };
        if let Some(accepted_bits) = visit_witness_state(
            problem,
            catalog,
            candidate,
            projection,
            pattern_index,
            word_index,
            root,
            workspace,
            &mut transition_cache,
            &mut failed,
            &mut path,
            &mut visited_product_states,
            completion,
            feasibility,
            control,
        )? {
            let local_pattern_index =
                word_index * u64::BITS as usize + accepted_bits.trailing_zeros() as usize;
            let global_pattern_index = pattern_index
                .global_pattern_index(local_pattern_index)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_witness_pattern_index_missing",
                ))?;
            return Ok(Some(CandidateWitness {
                global_pattern_index,
                path,
                visited_product_states,
                retained_bytes: transition_cache.capacity()
                    * core::mem::size_of::<CachedWitnessTransition>()
                    + failed.capacity() * core::mem::size_of::<WitnessProductState>(),
            }));
        }
    }
    Ok(None)
}

fn fixed_witness_branches(
    sequence: &[PieceKind],
    queue_position: usize,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    terminal_step: bool,
    state: FixedWitnessProductState,
) -> FixedWitnessBranches {
    let mut branches = FixedWitnessBranches {
        values: [EMPTY_FIXED_WITNESS_BRANCH; 3],
        len: 0,
    };
    let Some(current_piece) = sequence.get(queue_position).copied() else {
        if super::terminal_hold_projection::finite_terminal_release_allowed(
            sequence.len(),
            queue_position,
            hold_enabled,
            projects_unplaced_lookahead,
            state.terminal_projection_consumed,
            state.hold_code != 0,
            terminal_step,
        ) {
            branches.push(FixedWitnessBranch {
                desired_piece: state.hold_code,
                next_extra_draw: state.extra_draw,
                next_hold_code: 0,
                terminal_projection_consumed: true,
                hold_kind: "release-held-at-terminal",
            });
        }
        return branches;
    };
    let current_code = witness_piece_code(current_piece);
    let Some(cursor) = u16::try_from(queue_position).ok() else {
        return branches;
    };
    if state.hold_code > 7 {
        return branches;
    }
    let hold_piece = witness_piece_from_code(state.hold_code);
    let identity = fixed_witness_supply_identity(sequence, cursor, hold_piece, hold_enabled);
    if let Some(step) = sequence_supply_transition(
        identity,
        cursor,
        hold_piece,
        hold_enabled,
        SupplyBranchKind::Current,
        current_piece,
        None,
    ) {
        if let Some(next_extra_draw) =
            projected_witness_extra_draw(state.extra_draw, step.evidence.queue_advances)
        {
            branches.push(FixedWitnessBranch {
                desired_piece: witness_piece_code(step.used_piece),
                next_extra_draw,
                next_hold_code: step.next_state.hold_piece.map_or(0, witness_piece_code),
                terminal_projection_consumed: state.terminal_projection_consumed,
                hold_kind: "use-current",
            });
        }
    }
    if !hold_enabled {
        return branches;
    }
    if state.hold_code != 0 {
        // Swapping equal pieces reaches the exact same semantic state as using
        // current, so it is not a second search branch.
        if state.hold_code != current_code {
            if let Some(step) = sequence_supply_transition(
                identity,
                cursor,
                hold_piece,
                hold_enabled,
                SupplyBranchKind::SwapHeld,
                current_piece,
                None,
            ) {
                if let Some(next_extra_draw) =
                    projected_witness_extra_draw(state.extra_draw, step.evidence.queue_advances)
                {
                    branches.push(FixedWitnessBranch {
                        desired_piece: witness_piece_code(step.used_piece),
                        next_extra_draw,
                        next_hold_code: step.next_state.hold_piece.map_or(0, witness_piece_code),
                        terminal_projection_consumed: state.terminal_projection_consumed,
                        hold_kind: "swap-held",
                    });
                }
            }
        }
    } else if state.extra_draw == 0 {
        if let Some(next_piece) = queue_position
            .checked_add(1)
            .and_then(|position| sequence.get(position))
            .copied()
        {
            if let Some(step) = sequence_supply_transition(
                identity,
                cursor,
                hold_piece,
                hold_enabled,
                SupplyBranchKind::StoreCurrent,
                current_piece,
                Some(next_piece),
            ) {
                if let Some(next_extra_draw) =
                    projected_witness_extra_draw(state.extra_draw, step.evidence.queue_advances)
                {
                    branches.push(FixedWitnessBranch {
                        desired_piece: witness_piece_code(step.used_piece),
                        next_extra_draw,
                        next_hold_code: step.next_state.hold_piece.map_or(0, witness_piece_code),
                        terminal_projection_consumed: state.terminal_projection_consumed,
                        hold_kind: "store-current-use-next",
                    });
                }
            }
        }
    }
    branches
}

fn fixed_witness_supply_identity(
    sequence: &[PieceKind],
    cursor: u16,
    hold_piece: Option<PieceKind>,
    hold_enabled: bool,
) -> HoldAutomatonState {
    let mut identity = 0xcbf2_9ce4_8422_2325_u64;
    for piece in sequence.iter().copied() {
        identity ^= u64::from(witness_piece_code(piece));
        identity = identity.wrapping_mul(0x0000_0100_0000_01b3);
    }
    SupplyExecutionState::with_contract(
        PieceSourceId::new(identity),
        PieceSourceKind::FixedQueue,
        cursor,
        hold_piece,
        if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(identity),
    )
}

fn projected_witness_extra_draw(extra_draw: u8, queue_advances: u8) -> Option<u8> {
    extra_draw.checked_add(queue_advances.checked_sub(1)?)
}

#[allow(clippy::too_many_arguments)]
fn visit_fixed_witness_state(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    projection: &mut CandidateProjection,
    sequence: &[PieceKind],
    operation_masks_by_piece: &[u16; 8],
    state: FixedWitnessProductState,
    workspace: &mut BuildUpWorkspace,
    transition_cache: &mut [CachedWitnessTransition],
    failed: &mut ExactHashSet<FixedWitnessProductState>,
    path: &mut Vec<WitnessStep>,
    visited_product_states: &mut usize,
    completion: BuildCompletion,
    feasibility: RealizationFeasibility,
    control: &ExecutionControl,
) -> Result<bool, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    if failed.contains(&state) {
        return Ok(false);
    }
    *visited_product_states = visited_product_states.saturating_add(1);
    let subset = usize::from(state.subset);
    if subset == projection.all_placed {
        return Ok(completion.accepts(projection, subset));
    }
    let depth = subset.count_ones() as usize;
    let queue_position = problem
        .initial_hold()
        .cursor()
        .checked_add(depth as u16)
        .and_then(|position| position.checked_add(u16::from(state.extra_draw)))
        .map(usize::from)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_witness_queue_position_overflow",
        ))?;
    let permitted_operations = workspace
        .realization_feasibility
        .permitted_operation_mask(feasibility, subset);
    let branches = fixed_witness_branches(
        sequence,
        queue_position,
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
        depth.saturating_add(1) == projection.operation_count(),
        state,
    );
    for branch in branches.iter() {
        let mut operations =
            permitted_operations & operation_masks_by_piece[usize::from(branch.desired_piece)];
        while operations != 0 {
            let operation_index = operations.trailing_zeros() as usize;
            operations &= operations - 1;
            let Some(edge) = witness_transition(
                problem,
                catalog,
                candidate,
                projection,
                subset,
                operation_index,
                workspace,
                transition_cache,
            ) else {
                continue;
            };
            path.push(WitnessStep {
                edge,
                hold_kind: branch.hold_kind,
            });
            let next = FixedWitnessProductState {
                subset: edge.to as u16,
                extra_draw: branch.next_extra_draw,
                hold_code: branch.next_hold_code,
                terminal_projection_consumed: branch.terminal_projection_consumed,
            };
            if visit_fixed_witness_state(
                problem,
                catalog,
                candidate,
                projection,
                sequence,
                operation_masks_by_piece,
                next,
                workspace,
                transition_cache,
                failed,
                path,
                visited_product_states,
                completion,
                feasibility,
                control,
            )? {
                return Ok(true);
            }
            path.pop();
        }
    }
    failed.insert(state);
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn visit_witness_state(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    projection: &mut CandidateProjection,
    pattern_index: &PatternPiecePositionIndex,
    word_index: usize,
    state: WitnessProductState,
    workspace: &mut BuildUpWorkspace,
    transition_cache: &mut [CachedWitnessTransition],
    failed: &mut ExactHashSet<WitnessProductState>,
    path: &mut Vec<WitnessStep>,
    visited_product_states: &mut usize,
    completion: BuildCompletion,
    feasibility: RealizationFeasibility,
    control: &ExecutionControl,
) -> Result<Option<u64>, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    if state.active_patterns == 0 || failed.contains(&state) {
        return Ok(None);
    }
    *visited_product_states = visited_product_states.saturating_add(1);
    let subset = usize::from(state.subset);
    if subset == projection.all_placed {
        return Ok(completion
            .accepts(projection, subset)
            .then_some(state.active_patterns));
    }
    let depth = subset.count_ones() as usize;
    let queue_position = problem
        .initial_hold()
        .cursor()
        .checked_add(depth as u16)
        .and_then(|position| position.checked_add(u16::from(state.extra_draw)))
        .map(usize::from)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_witness_queue_position_overflow",
        ))?;
    let mut permitted_operations = workspace
        .realization_feasibility
        .permitted_operation_mask(feasibility, subset);
    while permitted_operations != 0 {
        let operation_index = permitted_operations.trailing_zeros() as usize;
        permitted_operations &= permitted_operations - 1;
        let Some(edge) = witness_transition(
            problem,
            catalog,
            candidate,
            projection,
            subset,
            operation_index,
            workspace,
            transition_cache,
        ) else {
            continue;
        };
        let desired_piece = witness_piece_code(edge.piece);
        let Some(supply_cursor) = u16::try_from(queue_position).ok() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_witness_queue_position_overflow",
            ));
        };
        if state.hold_code > 7 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_witness_hold_state_invalid",
            ));
        }
        let hold_piece = witness_piece_from_code(state.hold_code);
        if super::terminal_hold_projection::finite_terminal_release_allowed(
            pattern_index.sequence_len(),
            queue_position,
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            state.terminal_projection_consumed,
            state.hold_code != 0,
            edge.to as usize == projection.all_placed,
        ) && state.hold_code == desired_piece
        {
            let terminal_patterns = super::terminal_hold_projection::terminal_release_pattern_word(
                pattern_index,
                queue_position,
                word_index,
                state.active_patterns,
                problem.supply().projects_standard_bag_lookahead(),
            );
            if terminal_patterns != 0 {
                path.push(WitnessStep {
                    edge,
                    hold_kind: "release-held-at-terminal",
                });
                let next = WitnessProductState {
                    subset: edge.to as u16,
                    hold_code: 0,
                    terminal_projection_consumed: true,
                    active_patterns: terminal_patterns,
                    ..state
                };
                if let Some(accepted) = visit_witness_state(
                    problem,
                    catalog,
                    candidate,
                    projection,
                    pattern_index,
                    word_index,
                    next,
                    workspace,
                    transition_cache,
                    failed,
                    path,
                    visited_product_states,
                    completion,
                    feasibility,
                    control,
                )? {
                    return Ok(Some(accepted));
                }
                path.pop();
            }
        }
        let use_current = state.active_patterns
            & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                queue_position,
                desired_piece,
                word_index,
                problem.supply().projects_standard_bag_lookahead(),
            );
        if use_current != 0 {
            let Some(supply_step) = sequence_supply_transition(
                problem.initial_hold(),
                supply_cursor,
                hold_piece,
                problem.supply().hold_enabled(),
                SupplyBranchKind::Current,
                edge.piece,
                None,
            ) else {
                continue;
            };
            path.push(WitnessStep {
                edge,
                hold_kind: "use-current",
            });
            let next = WitnessProductState {
                subset: edge.to as u16,
                extra_draw: projected_witness_extra_draw(
                    state.extra_draw,
                    supply_step.evidence.queue_advances,
                )
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_witness_extra_draw_overflow",
                ))?,
                hold_code: supply_step
                    .next_state
                    .hold_piece
                    .map_or(0, witness_piece_code),
                active_patterns: use_current,
                ..state
            };
            if let Some(accepted) = visit_witness_state(
                problem,
                catalog,
                candidate,
                projection,
                pattern_index,
                word_index,
                next,
                workspace,
                transition_cache,
                failed,
                path,
                visited_product_states,
                completion,
                feasibility,
                control,
            )? {
                return Ok(Some(accepted));
            }
            path.pop();
        }
        if !problem.supply().hold_enabled() {
            continue;
        }
        if state.hold_code != 0 && state.hold_code == desired_piece {
            for current_piece in 1..=7 {
                let swap_bits = state.active_patterns
                    & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                        queue_position,
                        current_piece,
                        word_index,
                        problem.supply().projects_standard_bag_lookahead(),
                    );
                if swap_bits == 0 {
                    continue;
                }
                let Some(supply_step) = sequence_supply_transition(
                    problem.initial_hold(),
                    supply_cursor,
                    hold_piece,
                    true,
                    SupplyBranchKind::SwapHeld,
                    PieceKind::STANDARD_TETROMINOES[usize::from(current_piece - 1)],
                    None,
                ) else {
                    continue;
                };
                path.push(WitnessStep {
                    edge,
                    hold_kind: "swap-held",
                });
                let next = WitnessProductState {
                    subset: edge.to as u16,
                    extra_draw: projected_witness_extra_draw(
                        state.extra_draw,
                        supply_step.evidence.queue_advances,
                    )
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_witness_extra_draw_overflow",
                    ))?,
                    hold_code: supply_step
                        .next_state
                        .hold_piece
                        .map_or(0, witness_piece_code),
                    active_patterns: swap_bits,
                    ..state
                };
                if let Some(accepted) = visit_witness_state(
                    problem,
                    catalog,
                    candidate,
                    projection,
                    pattern_index,
                    word_index,
                    next,
                    workspace,
                    transition_cache,
                    failed,
                    path,
                    visited_product_states,
                    completion,
                    feasibility,
                    control,
                )? {
                    return Ok(Some(accepted));
                }
                path.pop();
            }
        } else if state.hold_code == 0 && state.extra_draw == 0 {
            let Some(next_queue_position) = queue_position.checked_add(1) else {
                continue;
            };
            let desired_next = pattern_index.piece_word_with_projected_standard_bag_lookahead(
                next_queue_position,
                desired_piece,
                word_index,
                problem.supply().projects_standard_bag_lookahead(),
            );
            for current_piece in 1..=7 {
                let store_bits = state.active_patterns
                    & desired_next
                    & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                        queue_position,
                        current_piece,
                        word_index,
                        problem.supply().projects_standard_bag_lookahead(),
                    );
                if store_bits == 0 {
                    continue;
                }
                let Some(supply_step) = sequence_supply_transition(
                    problem.initial_hold(),
                    supply_cursor,
                    hold_piece,
                    true,
                    SupplyBranchKind::StoreCurrent,
                    PieceKind::STANDARD_TETROMINOES[usize::from(current_piece - 1)],
                    Some(edge.piece),
                ) else {
                    continue;
                };
                path.push(WitnessStep {
                    edge,
                    hold_kind: "store-current-use-next",
                });
                let next = WitnessProductState {
                    subset: edge.to as u16,
                    extra_draw: projected_witness_extra_draw(
                        state.extra_draw,
                        supply_step.evidence.queue_advances,
                    )
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_witness_extra_draw_overflow",
                    ))?,
                    hold_code: supply_step
                        .next_state
                        .hold_piece
                        .map_or(0, witness_piece_code),
                    terminal_projection_consumed: state.terminal_projection_consumed,
                    active_patterns: store_bits,
                };
                if let Some(accepted) = visit_witness_state(
                    problem,
                    catalog,
                    candidate,
                    projection,
                    pattern_index,
                    word_index,
                    next,
                    workspace,
                    transition_cache,
                    failed,
                    path,
                    visited_product_states,
                    completion,
                    feasibility,
                    control,
                )? {
                    return Ok(Some(accepted));
                }
                path.pop();
            }
        }
    }
    failed.insert(state);
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn witness_transition(
    _problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    projection: &mut CandidateProjection,
    subset: usize,
    operation_index: usize,
    workspace: &mut BuildUpWorkspace,
    transition_cache: &mut [CachedWitnessTransition],
) -> Option<BuildEdge> {
    let cache_index = subset * projection.operation_count() + operation_index;
    match transition_cache[cache_index] {
        CachedWitnessTransition::Impossible => return None,
        CachedWitnessTransition::Legal(edge) => return Some(edge),
        CachedWitnessTransition::Unknown => {}
    }
    let row_id = candidate.row_ids()[operation_index];
    let row = catalog.skeleton(row_id);
    let (board, deleted_rows) = projection.state(subset);
    for realization in catalog.instantiations(row_id, deleted_rows) {
        let lock_mask = realization.lock_mask;
        let lock_y = realization.lock_y;
        if board & lock_mask != 0 {
            continue;
        }
        if !workspace.reachability.lock_reachable_instantiated(
            catalog,
            board,
            row.piece,
            realization,
        ) {
            continue;
        }
        let (next_board, cleared_current, cleared_lines) =
            place_and_clear(catalog.width(), catalog.height(), board | lock_mask);
        let next_deleted_rows =
            merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)?;
        let child = subset | (1_usize << operation_index);
        let (expected_board, expected_deleted_rows) = projection.state(child);
        if next_deleted_rows != expected_deleted_rows || next_board != expected_board {
            continue;
        }
        let edge = BuildEdge {
            to: child as u32,
            operation_index: operation_index as u8,
            piece: row.piece,
            rotation: realization.rotation,
            x: realization.x,
            y: lock_y,
            cleared_lines,
        };
        transition_cache[cache_index] = CachedWitnessTransition::Legal(edge);
        return Some(edge);
    }
    transition_cache[cache_index] = CachedWitnessTransition::Impossible;
    None
}

const fn witness_piece_code(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

const fn witness_piece_from_code(code: u8) -> Option<PieceKind> {
    match code {
        0 => None,
        1 => Some(PieceKind::I),
        2 => Some(PieceKind::O),
        3 => Some(PieceKind::T),
        4 => Some(PieceKind::S),
        5 => Some(PieceKind::Z),
        6 => Some(PieceKind::J),
        7 => Some(PieceKind::L),
        _ => None,
    }
}

impl BuildOrderGraph {
    pub(super) fn from_topological_parts(
        specs: Vec<BuildOrderNodeSpec>,
        edges: Vec<BuildEdge>,
        root: u32,
        reachability_states: usize,
        piece_transitions_are_unique: bool,
    ) -> Result<Self, WasmExactSearchError> {
        if specs.is_empty() || root as usize >= specs.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_order_graph_root_invalid",
            ));
        }

        let mut nodes = Vec::new();
        nodes.try_reserve_exact(specs.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_build_order_node_storage_unavailable")
        })?;
        let mut piece_edges = Vec::new();
        if !piece_transitions_are_unique {
            piece_edges.try_reserve(edges.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_build_order_edge_storage_unavailable")
            })?;
        }

        for (node_index, spec) in specs.iter().copied().enumerate() {
            let start = spec.edge_start as usize;
            let end = start.checked_add(spec.edge_count as usize).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_build_order_edge_range_overflow"),
            )?;
            let source_edges =
                edges
                    .get(start..end)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_order_edge_range_invalid",
                    ))?;
            if spec.accepting && !source_edges.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_order_accepting_node_has_edges",
                ));
            }
            for edge in source_edges {
                let Some(target) = specs.get(edge.to as usize) else {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_build_order_edge_target_invalid",
                    ));
                };
                if edge.to as usize <= node_index || target.depth != spec.depth.saturating_add(1) {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_build_order_graph_not_topological",
                    ));
                }
            }

            let piece_edge_start = if piece_transitions_are_unique {
                spec.edge_start
            } else {
                piece_edges.len() as u32
            };
            let mut previous = None;
            for edge in source_edges {
                let key = (edge.to, edge.piece);
                if previous == Some(key) {
                    if piece_transitions_are_unique {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_build_order_piece_transition_not_unique",
                        ));
                    }
                    continue;
                }
                if !piece_transitions_are_unique {
                    piece_edges.push(*edge);
                }
                previous = Some(key);
            }
            nodes.push(BuildNode {
                edge_start: spec.edge_start,
                edge_count: spec.edge_count,
                piece_edge_start,
                piece_edge_count: if piece_transitions_are_unique {
                    spec.edge_count
                } else {
                    piece_edges.len() as u32 - piece_edge_start
                },
                depth: spec.depth,
                live: spec.accepting,
                accepting: spec.accepting,
            });
        }

        for index in (0..nodes.len()).rev() {
            if nodes[index].live {
                continue;
            }
            let start = nodes[index].edge_start as usize;
            let end = start + nodes[index].edge_count as usize;
            nodes[index].live = edges[start..end]
                .iter()
                .any(|edge| nodes[edge.to as usize].live);
        }

        Ok(Self {
            nodes,
            edges,
            piece_edges,
            piece_edges_share_operation_edges: piece_transitions_are_unique,
            root,
            reachability_states,
        })
    }

    pub(super) const fn edge(
        to: u32,
        operation_index: u8,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        cleared_lines: u8,
    ) -> BuildEdge {
        BuildEdge {
            to,
            operation_index,
            piece,
            rotation,
            x,
            y,
            cleared_lines,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<BuildNode>()
            + (self.edges.capacity() + self.piece_edges.capacity())
                * core::mem::size_of::<BuildEdge>()
    }

    pub(super) fn is_live(&self) -> bool {
        self.nodes[self.root as usize].live
    }

    fn build(
        _problem: &SearchProblem,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        workspace: &mut BuildUpWorkspace,
        piece_language_projection_only: bool,
        completion: BuildCompletion,
        feasibility: Option<RealizationFeasibility>,
        reachability_mode: BuildReachabilityMode,
    ) -> Result<Self, WasmExactSearchError> {
        if candidate.row_ids().len() > 15 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_buildup_operation_count_exceeds_bitset",
            ));
        }
        // In target-frame semantics, the placed-operation subset uniquely
        // determines both deleted logical rows and the compacted physical board.
        // This is an exact quotient, not a hash: every contributor of a logical
        // row must have been placed before that row can clear.
        let operation_count = projection.operation_count();
        let state_count = projection.state_count();
        let all_placed = projection.all_placed;
        let share_piece_edges =
            piece_language_projection_only && matches!(completion, BuildCompletion::ClearToEmpty);
        let graph_generation = workspace.begin_graph_generation(state_count)?;
        workspace.graph_reachable_generations[0] = graph_generation;
        workspace.graph_subset_node_ids[0] = 0;
        let mut nodes = core::mem::take(&mut workspace.graph_nodes);
        nodes.clear();
        nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_build_order_node_storage_unavailable")
        })?;
        nodes.push(BuildNode {
            edge_start: 0,
            edge_count: 0,
            piece_edge_start: 0,
            piece_edge_count: 0,
            depth: 0,
            live: false,
            accepting: false,
        });
        let mut subset_queue = core::mem::take(&mut workspace.graph_subset_queue);
        subset_queue.clear();
        subset_queue.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_build_order_queue_storage_unavailable")
        })?;
        subset_queue.push(0);
        let mut edges = core::mem::take(&mut workspace.graph_edges);
        edges.clear();
        let mut piece_edges = core::mem::take(&mut workspace.graph_piece_edges);
        piece_edges.clear();
        let mut edge_scratch = core::mem::take(&mut workspace.graph_edge_scratch);
        edge_scratch.clear();
        let reachability_states_before = workspace.reachability.generated_state_count();
        let all_operations = (1_u16 << operation_count) - 1;
        let partial_dependency_graph_active = feasibility.is_some_and(|proof| {
            workspace
                .realization_feasibility
                .has_current_partial_dependency_graph(proof)
        });
        let mut subset_cursor = 0usize;
        while subset_cursor < subset_queue.len() {
            let subset = usize::from(subset_queue[subset_cursor]);
            subset_cursor += 1;
            let node_index = workspace.graph_subset_node_ids[subset] as usize;
            nodes[node_index].edge_start = edges.len() as u32;
            nodes[node_index].piece_edge_start = if share_piece_edges {
                edges.len() as u32
            } else {
                piece_edges.len() as u32
            };
            if subset == all_placed {
                nodes[node_index].accepting = completion.accepts(projection, subset);
                nodes[node_index].live = nodes[node_index].accepting;
                continue;
            }
            edge_scratch.clear();
            let (board, deleted_rows) = projection.state(subset);
            let mut permitted_operations =
                feasibility.map_or(all_operations & !(subset as u16), |proof| {
                    workspace
                        .realization_feasibility
                        .permitted_operation_mask(proof, subset)
                });
            while permitted_operations != 0 {
                let operation_index = permitted_operations.trailing_zeros() as usize;
                permitted_operations &= permitted_operations - 1;
                let operation_bit = 1_usize << operation_index;
                let child = subset | operation_bit;
                if !partial_dependency_graph_active
                    && feasibility.is_some_and(|proof| {
                        workspace
                            .realization_feasibility
                            .proves_subset_infeasible(proof, child)
                    })
                {
                    continue;
                }
                let row_id = candidate.row_ids()[operation_index];
                let row = catalog.skeleton(row_id);
                if reachability_mode == BuildReachabilityMode::GeometryOnly {
                    for realization in catalog.instantiations(row_id, deleted_rows) {
                        if let Some(edge) = geometric_build_edge(
                            catalog,
                            projection,
                            child,
                            operation_index,
                            row.piece,
                            board,
                            deleted_rows,
                            realization,
                        ) {
                            try_push_build_edge(&mut edge_scratch, edge)?;
                        }
                    }
                } else if piece_language_projection_only {
                    let scratch_start = edge_scratch.len();
                    let mut harddrop_edge = None;
                    for realization in catalog.instantiations(row_id, deleted_rows) {
                        let Some(edge) = geometric_build_edge(
                            catalog,
                            projection,
                            child,
                            operation_index,
                            row.piece,
                            board,
                            deleted_rows,
                            realization,
                        ) else {
                            continue;
                        };
                        try_push_build_edge(&mut edge_scratch, edge)?;
                        if workspace.reachability.lock_harddrop_reachable_instantiated(
                            catalog.width(),
                            board,
                            realization,
                        ) {
                            harddrop_edge = Some(edge);
                            break;
                        }
                    }
                    let selected = if let Some(edge) = harddrop_edge {
                        Some(edge)
                    } else {
                        workspace.reachability.prepare_template(catalog, row.piece);
                        let mut reachable = None;
                        for edge in edge_scratch[scratch_start..].iter().copied() {
                            if workspace.reachability.lock_reachable_after_harddrop_miss(
                                catalog,
                                board,
                                row.piece,
                                edge.rotation,
                                edge.x,
                                edge.y,
                            ) {
                                reachable = Some(edge);
                                break;
                            }
                        }
                        reachable
                    };
                    edge_scratch.truncate(scratch_start);
                    if let Some(edge) = selected {
                        try_push_build_edge(&mut edge_scratch, edge)?;
                    }
                } else {
                    for realization in catalog.instantiations(row_id, deleted_rows) {
                        let Some(edge) = geometric_build_edge(
                            catalog,
                            projection,
                            child,
                            operation_index,
                            row.piece,
                            board,
                            deleted_rows,
                            realization,
                        ) else {
                            continue;
                        };
                        if workspace.reachability.lock_reachable_instantiated(
                            catalog,
                            board,
                            row.piece,
                            realization,
                        ) {
                            try_push_build_edge(&mut edge_scratch, edge)?;
                        }
                    }
                }
            }
            edge_scratch.sort_unstable_by_key(|edge| {
                (
                    edge.to,
                    edge.operation_index,
                    edge.piece,
                    edge.rotation,
                    edge.x,
                    edge.y,
                    edge.cleared_lines,
                )
            });
            edge_scratch.dedup();
            for edge in &mut edge_scratch {
                let child_subset = edge.to as usize;
                if workspace.graph_reachable_generations[child_subset] != graph_generation {
                    workspace.graph_reachable_generations[child_subset] = graph_generation;
                    let child_node = u32::try_from(nodes.len()).map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_build_order_node_index_overflow")
                    })?;
                    workspace.graph_subset_node_ids[child_subset] = child_node;
                    nodes.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_build_order_node_storage_unavailable",
                        )
                    })?;
                    subset_queue.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_build_order_queue_storage_unavailable",
                        )
                    })?;
                    nodes.push(BuildNode {
                        edge_start: 0,
                        edge_count: 0,
                        piece_edge_start: 0,
                        piece_edge_count: 0,
                        depth: child_subset.count_ones() as u8,
                        live: false,
                        accepting: false,
                    });
                    subset_queue.push(child_subset as u16);
                }
                edge.to = workspace.graph_subset_node_ids[child_subset];
            }
            if !share_piece_edges {
                let mut previous_piece_transition = None;
                for edge in &edge_scratch {
                    let key = (edge.to, edge.piece);
                    if previous_piece_transition != Some(key) {
                        piece_edges.try_reserve(1).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "wasm_build_order_piece_edge_storage_unavailable",
                            )
                        })?;
                        piece_edges.push(*edge);
                        previous_piece_transition = Some(key);
                    }
                }
            }
            edges.try_reserve(edge_scratch.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_build_order_edge_storage_unavailable")
            })?;
            edges.extend_from_slice(&edge_scratch);
            nodes[node_index].edge_count = edge_scratch.len() as u32;
            nodes[node_index].piece_edge_count = if share_piece_edges {
                nodes[node_index].edge_count
            } else {
                piece_edges.len() as u32 - nodes[node_index].piece_edge_start
            };
        }
        for index in (0..nodes.len()).rev() {
            if nodes[index].live {
                continue;
            }
            let start = nodes[index].edge_start as usize;
            let end = start + nodes[index].edge_count as usize;
            nodes[index].live = edges[start..end]
                .iter()
                .any(|edge| nodes[edge.to as usize].live);
        }
        edge_scratch.clear();
        workspace.graph_edge_scratch = edge_scratch;
        subset_queue.clear();
        workspace.graph_subset_queue = subset_queue;
        Ok(Self {
            nodes,
            edges,
            piece_edges,
            piece_edges_share_operation_edges: share_piece_edges,
            root: 0,
            reachability_states: workspace
                .reachability
                .generated_state_count()
                .saturating_sub(reachability_states_before),
        })
    }

    pub fn max_depth(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| usize::from(node.depth))
            .max()
            .unwrap_or(0)
    }

    pub fn edges(&self, node_index: usize) -> &[BuildEdge] {
        let node = &self.nodes[node_index];
        let start = node.edge_start as usize;
        &self.edges[start..start + node.edge_count as usize]
    }

    pub fn piece_edges(&self, node_index: usize) -> &[BuildEdge] {
        let node = &self.nodes[node_index];
        let start = node.piece_edge_start as usize;
        let edges = if self.piece_edges_share_operation_edges {
            &self.edges
        } else {
            &self.piece_edges
        };
        &edges[start..start + node.piece_edge_count as usize]
    }
}

fn try_push_build_edge(
    storage: &mut Vec<BuildEdge>,
    edge: BuildEdge,
) -> Result<(), WasmExactSearchError> {
    storage.try_reserve(1).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_build_order_edge_scratch_storage_unavailable")
    })?;
    storage.push(edge);
    Ok(())
}

fn annotate_and_prune_finesse_graph(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    projection: &mut CandidateProjection,
    graph: &mut BuildOrderGraph,
    reachability: &mut ReachabilityWorkspace,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<PreparedFinesseLanguage, WasmExactSearchError> {
    annotate_and_prune_finesse_graph_with_query_count(
        problem,
        catalog,
        projection,
        graph,
        reachability,
        spin_coverage_requested,
        control,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn annotate_and_prune_finesse_graph_with_query_count(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    projection: &mut CandidateProjection,
    graph: &mut BuildOrderGraph,
    reachability: &mut ReachabilityWorkspace,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
    query_count: Option<&mut usize>,
) -> Result<PreparedFinesseLanguage, WasmExactSearchError> {
    let target_grouping_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseTargetGrouping);
    let mut subsets = vec![usize::MAX; graph.nodes.len()];
    subsets[graph.root as usize] = 0;
    for node_index in 0..graph.nodes.len() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let subset = subsets[node_index];
        if subset == usize::MAX {
            continue;
        }
        for edge in graph.edges(node_index) {
            let child_subset = subset | (1_usize << edge.operation_index);
            let child = &mut subsets[edge.to as usize];
            if *child == usize::MAX {
                *child = child_subset;
            } else if *child != child_subset {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_build_order_subset_mismatch",
                ));
            }
        }
    }
    let size = BoardSize::new(u16::from(catalog.width()), u16::from(catalog.height()))
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_board_size_invalid"))?;
    let layout = Board64Layout::new(size).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_compact_board_layout_invalid")
    })?;
    let kick_profile =
        super::kick_profiles::builtin_kick_profile(problem.kick_profile().profile_id())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_kick_profile_unavailable",
            ))?
            .clone();
    let mut edge_costs = vec![None; graph.edges.len()];
    let mut edge_terminal_evidence = vec![None; graph.edges.len()];
    let b2b_policy = finesse_b2b_policy(problem);
    let grouped_target_count = graph.edges.len();
    let mut groups = FrozenFinesseTargetGroups::new();
    for node_index in 0..graph.nodes.len() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let subset = subsets[node_index];
        if subset == usize::MAX {
            continue;
        }
        let board = projection.state(subset).0;
        let node = &graph.nodes[node_index];
        let edge_start = node.edge_start as usize;
        let finesse_board = FinesseBoard::new(layout, board)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_board_invalid"))?;
        for (local_index, edge) in graph.edges(node_index).iter().copied().enumerate() {
            let edge_index = edge_start + local_index;
            let (terminal_evidence, allowed) = if spin_coverage_requested || b2b_policy.is_some() {
                let lock_evidence = reachability.scoring_lock_evidence(
                    catalog,
                    board,
                    edge.piece,
                    edge.rotation,
                    edge.x,
                    edge.y,
                );
                let child_subset = subset | (1_usize << edge.operation_index);
                let perfect_clear = edge.cleared_lines > 0 && projection.state(child_subset).0 == 0;
                let (blocked_t_corners, blocked_t_front_corners) = if edge.piece == PieceKind::T {
                    t_corner_evidence(
                        catalog.width(),
                        catalog.height(),
                        board,
                        edge.rotation,
                        edge.x,
                        edge.y,
                    )
                } else {
                    (0, 0)
                };
                let scoring_edge = ScoringExecutionEdge::new(
                    edge.to,
                    edge.operation_index,
                    edge.piece,
                    edge.rotation,
                    edge.x,
                    edge.y,
                    edge.cleared_lines,
                    blocked_t_corners,
                    blocked_t_front_corners,
                    lock_evidence,
                )
                .with_perfect_clear(perfect_clear);
                let (allowed, exact_evidence_required) = finesse_scoring_edge_requirement(
                    spin_coverage_requested,
                    b2b_policy,
                    scoring_edge,
                );
                (
                    (allowed && exact_evidence_required)
                        .then(|| finesse_terminal_label(lock_evidence, edge.rotation))
                        .transpose()?,
                    allowed,
                )
            } else {
                (None, true)
            };
            push_frozen_finesse_target(
                &mut groups,
                finesse_board,
                edge.piece,
                GroupedFinesseTarget::new(
                    edge_index,
                    FinesseTarget::new(edge.rotation, i16::from(edge.x), i16::from(edge.y)),
                    terminal_evidence,
                    allowed,
                ),
            );
        }
    }
    target_grouping_span.finish(grouped_target_count as u64);

    let movement_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseMovementBfs);
    let query_traversals = annotate_frozen_finesse_target_groups(
        groups,
        problem.spawn_profile(),
        &kick_profile,
        &mut edge_costs,
        &mut edge_terminal_evidence,
        control,
        "wasm_finesse_movement_search_failed",
        "wasm_finesse_scoring_evidence_search_failed",
    )?;
    if let Some(query_count) = query_count {
        *query_count = query_traversals;
    }
    movement_span.finish(grouped_target_count as u64);

    let prune_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAnnotationPrune);
    let mut live = graph
        .nodes
        .iter()
        .map(BuildNode::accepting)
        .collect::<Vec<_>>();
    for node_index in (0..graph.nodes.len()).rev() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if live[node_index] {
            continue;
        }
        let node = &graph.nodes[node_index];
        let start = node.edge_start as usize;
        live[node_index] = graph
            .edges(node_index)
            .iter()
            .enumerate()
            .any(|(offset, edge)| edge_costs[start + offset].is_some() && live[edge.to as usize]);
    }

    let old_edges = core::mem::take(&mut graph.edges);
    let mut new_edges = Vec::with_capacity(old_edges.len());
    let mut new_piece_edges = Vec::with_capacity(old_edges.len());
    let mut prepared_nodes = Vec::with_capacity(graph.nodes.len());
    let mut prepared_edges = Vec::with_capacity(old_edges.len());
    for (node_index, node) in graph.nodes.iter_mut().enumerate() {
        let old_start = node.edge_start as usize;
        let old_end = old_start + node.edge_count as usize;
        node.live = live[node_index];
        node.edge_start = new_edges.len() as u32;
        node.piece_edge_start = new_piece_edges.len() as u32;
        let prepared_start = prepared_edges.len() as u32;
        if node.live {
            for (offset, edge) in old_edges[old_start..old_end].iter().copied().enumerate() {
                let original_index = old_start + offset;
                let Some(cost) = edge_costs[original_index] else {
                    continue;
                };
                if !live[edge.to as usize] {
                    continue;
                }
                new_edges.push(edge);
                prepared_edges.push(PreparedFinesseEdge {
                    child: edge.to,
                    piece: edge.piece,
                    cost,
                    transition_order: u32::try_from(original_index).unwrap_or(u32::MAX),
                    action_key: GeometryActionKey::new(
                        edge.piece,
                        edge.rotation,
                        i16::from(edge.x),
                        i16::from(edge.y),
                    ),
                    terminal_evidence: edge_terminal_evidence[original_index],
                });
            }
        }
        node.edge_count = new_edges.len() as u32 - node.edge_start;
        let mut previous = None;
        for edge in &new_edges[node.edge_start as usize..] {
            let key = (edge.to, edge.piece);
            if previous != Some(key) {
                new_piece_edges.push(*edge);
                previous = Some(key);
            }
        }
        node.piece_edge_count = new_piece_edges.len() as u32 - node.piece_edge_start;
        prepared_nodes.push(PreparedFinesseNode {
            edge_start: prepared_start,
            edge_count: prepared_edges.len() as u32 - prepared_start,
            depth: node.depth,
            accepting: node.accepting,
            source_board: (subsets[node_index] != usize::MAX)
                .then(|| FinesseBoard::new(layout, projection.state(subsets[node_index]).0))
                .transpose()
                .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_board_invalid"))?,
        });
    }
    graph.edges = new_edges;
    graph.piece_edges = new_piece_edges;
    graph.piece_edges_share_operation_edges = false;
    prune_span.finish(prepared_edges.len() as u64);
    Ok(PreparedFinesseLanguage {
        nodes: prepared_nodes,
        edges: prepared_edges,
        root: graph.root,
    })
}

pub(super) fn finesse_b2b_policy(problem: &SearchProblem) -> Option<BackToBackEdgePolicy> {
    let constraints = problem.objective().execution_constraints();
    constraints
        .preserves_back_to_back()
        .then(|| BackToBackEdgePolicy::new(constraints.spin_profile()))
}

pub(super) fn finesse_terminal_label(
    evidence: ScoringLockEvidence,
    to: RotationState,
) -> Result<TerminalEvidenceLabel, WasmExactSearchError> {
    if !evidence.last_action_was_rotation() {
        return Ok(TerminalEvidenceLabel::NoRotation);
    }
    let request = match evidence.rotation_request() {
        RotationRequest::Clockwise => ClassicInputAction::RotateClockwise,
        RotationRequest::CounterClockwise => ClassicInputAction::RotateCounterClockwise,
        RotationRequest::HalfTurn => ClassicInputAction::Rotate180,
        RotationRequest::None => {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_scoring_rotation_request_missing",
            ));
        }
    };
    let (predecessor_x, predecessor_y) = evidence.predecessor();
    Ok(TerminalEvidenceLabel::Rotation {
        from: evidence.from_rotation(),
        to,
        request,
        kick_index: evidence.kick_index(),
        kick_dx: evidence.kick_dx(),
        kick_dy: evidence.kick_dy(),
        predecessor: PiecePose::new(
            evidence.from_rotation(),
            i16::from(predecessor_x),
            i16::from(predecessor_y),
        ),
    })
}

pub(super) fn exact_scoring_execution_graph(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    identity: StandardBoard64TilingIdentity,
    candidate_id: u64,
    workspace: &mut BuildUpWorkspace,
) -> Result<Option<ExactScoringExecutionGraph>, WasmExactSearchError> {
    exact_scoring_execution_graph_for_completion(
        problem,
        catalog,
        identity,
        candidate_id,
        workspace,
        BuildCompletion::ClearToEmpty,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ExactScoringGraphMemoryProjection {
    /// Peak additional storage while the reusable Build workspace, its
    /// topological graph, the scoring conversion scratch, and the retained
    /// scoring graph coexist.
    pub peak_additional_bytes: u128,
    /// Heap storage retained by the returned graph, excluding its outer slot.
    pub retained_graph_nested_bytes: u128,
}

/// Checked conservative upper bound for one exact scoring-graph replay.
///
/// A candidate with `n` placements has at most `2^n` subset nodes. Every
/// operation is absent from exactly half of those subsets, and one raw catalog
/// realization can yield at most one build edge for that operation/subset.
/// This therefore bounds both build-edge stores and the final scoring edges
/// without claiming that the bound is an exact allocation measurement.
pub(super) fn exact_scoring_execution_graph_memory_projection(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    identity: StandardBoard64TilingIdentity,
) -> Result<ExactScoringGraphMemoryProjection, WasmExactSearchError> {
    let placement_count = identity.placement_count();
    if placement_count == 0 || placement_count > super::MAX_BOARD64_PIECES {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_placement_count_invalid",
        ));
    }
    let state_count =
        1_u128
            .checked_shl(placement_count as u32)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_projection_state_count_overflow",
            ))?;
    let mut realization_sum = 0_u128;
    for index in 0..placement_count {
        let placement = identity
            .placement(index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_identity_placement_missing",
            ))?;
        let row_id = catalog
            .skeleton_id(placement.piece(), placement.cells_mask())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_identity_not_in_geometry_catalog",
            ))?;
        realization_sum = realization_sum
            .checked_add(catalog.realizations(row_id).len() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_projection_realization_count_overflow",
            ))?;
    }
    let edge_bound = state_count
        .checked_div(2)
        .and_then(|half| half.checked_mul(realization_sum))
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_edge_count_overflow",
        ))?;
    let state_workspace_bytes = state_count
        .checked_mul(
            (core::mem::size_of::<BuildNode>()
                + core::mem::size_of::<u16>()
                + core::mem::size_of::<u32>() * 2
                + core::mem::size_of::<u16>()
                + core::mem::size_of::<u64>()
                + core::mem::size_of::<u32>()) as u128,
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_workspace_overflow",
        ))?;
    let build_edge_bytes = edge_bound
        .checked_mul((core::mem::size_of::<BuildEdge>() * 2) as u128)
        .and_then(|bytes| {
            realization_sum
                .checked_mul(core::mem::size_of::<BuildEdge>() as u128)
                .and_then(|scratch| bytes.checked_add(scratch))
        })
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_build_edge_overflow",
        ))?;
    let retained_graph_nested_bytes = state_count
        .checked_mul(core::mem::size_of::<ScoringExecutionNode>() as u128)
        .and_then(|nodes| {
            edge_bound
                .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)
                .and_then(|edges| nodes.checked_add(edges))
        })
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_retained_graph_overflow",
        ))?;
    let conversion_scratch_bytes = state_count
        .checked_mul(core::mem::size_of::<usize>() as u128)
        .and_then(|subsets| {
            (placement_count as u128)
                .checked_mul(core::mem::size_of::<u32>() as u128)
                .and_then(|rows| subsets.checked_add(rows))
        })
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_conversion_overflow",
        ))?;
    let reachability_bytes = super::reachability::checked_reachability_retained_upper_bound(
        catalog.width(),
        catalog.height(),
        problem.kick_profile().profile_id(),
    )
    .ok_or(WasmExactSearchError::InvalidProblem(
        "wasm_scoring_projection_reachability_overflow",
    ))?;
    let peak_additional_bytes = state_workspace_bytes
        .checked_add(build_edge_bytes)
        .and_then(|bytes| bytes.checked_add(retained_graph_nested_bytes))
        .and_then(|bytes| bytes.checked_add(conversion_scratch_bytes))
        .and_then(|bytes| bytes.checked_add(reachability_bytes))
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<ExactScoringExecutionGraph>() as u128)
        })
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_projection_peak_overflow",
        ))?;
    Ok(ExactScoringGraphMemoryProjection {
        peak_additional_bytes,
        retained_graph_nested_bytes,
    })
}

pub(super) fn exact_scoring_execution_graph_for_completion(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    identity: StandardBoard64TilingIdentity,
    candidate_id: u64,
    workspace: &mut BuildUpWorkspace,
    completion: BuildCompletion,
) -> Result<Option<ExactScoringExecutionGraph>, WasmExactSearchError> {
    let mut row_ids = Vec::new();
    row_ids
        .try_reserve_exact(identity.placement_count())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_row_storage_unavailable")
        })?;
    for index in 0..identity.placement_count() {
        let placement = identity
            .placement(index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_identity_placement_missing",
            ))?;
        let row_id = catalog
            .skeleton_id(placement.piece(), placement.cells_mask())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_scoring_identity_not_in_geometry_catalog",
            ))?;
        row_ids.push(row_id);
    }
    let candidate = GeometryCandidate::from_rows(catalog, 0, &row_ids).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_scoring_candidate_reconstruction_failed"),
    )?;
    if candidate.identity != identity {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_scoring_candidate_identity_mismatch",
        ));
    }

    workspace.reachability.configure(candidate.row_ids().len());
    workspace
        .reachability
        .configure_kick_profile(problem.kick_profile().profile_id());
    let mut projection = CandidateProjection::compile(catalog, &candidate, workspace, completion)?;
    let graph = match BuildOrderGraph::build(
        problem,
        catalog,
        &candidate,
        &mut projection,
        workspace,
        false,
        completion,
        None,
        BuildReachabilityMode::Existing,
    ) {
        Ok(graph) => graph,
        Err(error) => {
            workspace.recycle_projection(projection);
            return Err(error);
        }
    };
    if !graph.nodes[graph.root as usize].live {
        workspace.recycle_graph(graph);
        workspace.recycle_projection(projection);
        return Ok(None);
    }

    let mut subsets = Vec::new();
    subsets.try_reserve_exact(graph.nodes.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_scoring_subset_storage_unavailable")
    })?;
    subsets.resize(graph.nodes.len(), usize::MAX);
    subsets[graph.root as usize] = 0;
    for node_index in 0..graph.nodes.len() {
        let subset = subsets[node_index];
        if subset == usize::MAX {
            continue;
        }
        for edge in graph.edges(node_index) {
            let child_subset = subset | (1_usize << edge.operation_index);
            let child = &mut subsets[edge.to as usize];
            if *child == usize::MAX {
                *child = child_subset;
            } else if *child != child_subset {
                workspace.recycle_graph(graph);
                workspace.recycle_projection(projection);
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_scoring_build_order_subset_mismatch",
                ));
            }
        }
    }

    let mut scoring_nodes = Vec::new();
    scoring_nodes
        .try_reserve_exact(graph.nodes.len())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_node_storage_unavailable")
        })?;
    let mut scoring_edges = Vec::new();
    scoring_edges
        .try_reserve_exact(graph.edges.len())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_edge_storage_unavailable")
        })?;
    for (node_index, node) in graph.nodes.iter().enumerate() {
        let edge_start = scoring_edges.len() as u32;
        if node.live {
            let subset = subsets[node_index];
            let board = projection.state(subset).0;
            for edge in graph
                .edges(node_index)
                .iter()
                .filter(|edge| graph.nodes[edge.to as usize].live)
            {
                let lock_evidence = workspace.reachability.scoring_lock_evidence(
                    catalog,
                    board,
                    edge.piece,
                    edge.rotation,
                    edge.x,
                    edge.y,
                );
                let (blocked_t_corners, blocked_t_front_corners) = if edge.piece == PieceKind::T {
                    t_corner_evidence(
                        catalog.width(),
                        catalog.height(),
                        board,
                        edge.rotation,
                        edge.x,
                        edge.y,
                    )
                } else {
                    (0, 0)
                };
                let child_subset = subset | (1_usize << edge.operation_index);
                let perfect_clear = edge.cleared_lines > 0 && projection.state(child_subset).0 == 0;
                scoring_edges.push(
                    ScoringExecutionEdge::new(
                        edge.to,
                        edge.operation_index,
                        edge.piece,
                        edge.rotation,
                        edge.x,
                        edge.y,
                        edge.cleared_lines,
                        blocked_t_corners,
                        blocked_t_front_corners,
                        lock_evidence,
                    )
                    .with_perfect_clear(perfect_clear),
                );
            }
        }
        scoring_nodes.push(ScoringExecutionNode::new(
            edge_start,
            scoring_edges.len() as u32 - edge_start,
            node.accepting(),
        ));
    }
    let result = ExactScoringExecutionGraph::new(
        candidate_id,
        identity,
        graph.root,
        scoring_nodes,
        scoring_edges,
    );
    workspace.recycle_graph(graph);
    workspace.recycle_projection(projection);
    Ok(Some(result))
}

fn t_corner_evidence(
    width: u8,
    height: u8,
    board_before: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> (u8, u8) {
    let x = i32::from(x);
    let y = i32::from(y);
    let (center_x, center_y) = match rotation.quarter_turns() {
        0 => (x + 1, y),
        1 => (x, y + 1),
        2 | 3 => (x + 1, y + 1),
        _ => return (0, 0),
    };
    let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
    let front = match rotation.quarter_turns() {
        0 => [(-1, 1), (1, 1)],
        1 => [(1, -1), (1, 1)],
        2 => [(-1, -1), (1, -1)],
        3 => [(-1, -1), (-1, 1)],
        _ => return (0, 0),
    };
    let blocked = |(dx, dy): (i32, i32)| {
        let cell_x = center_x + dx;
        let cell_y = center_y + dy;
        if cell_x < 0 || cell_y < 0 || cell_x >= i32::from(width) {
            return true;
        }
        if cell_y >= i32::from(height) {
            return false;
        }
        let index = cell_y as u64 * u64::from(width) + cell_x as u64;
        board_before & (1_u64 << index) != 0
    };
    (
        corners
            .into_iter()
            .filter(|corner| blocked(*corner))
            .count() as u8,
        front.into_iter().filter(|corner| blocked(*corner)).count() as u8,
    )
}

#[allow(clippy::too_many_arguments)]
fn geometric_build_edge(
    catalog: &GeometryCatalog,
    projection: &mut CandidateProjection,
    child: usize,
    operation_index: usize,
    piece: PieceKind,
    board: u64,
    deleted_rows: u16,
    realization: super::catalog::InstantiatedRealization,
) -> Option<BuildEdge> {
    let lock_mask = realization.lock_mask;
    if board & lock_mask != 0 {
        return None;
    }
    let (next_board, cleared_current, cleared_lines) =
        place_and_clear(catalog.width(), catalog.height(), board | lock_mask);
    let next_deleted_rows = merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)?;
    let (expected_board, expected_deleted_rows) = projection.state(child);
    if next_deleted_rows != expected_deleted_rows || next_board != expected_board {
        return None;
    }
    Some(BuildEdge {
        to: child as u32,
        operation_index: operation_index as u8,
        piece,
        rotation: realization.rotation,
        x: realization.x,
        y: realization.lock_y,
        cleared_lines,
    })
}

fn first_pattern_path(
    graph: &BuildOrderGraph,
    sequence: &[PieceKind],
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    supply_identity: HoldAutomatonState,
    initial: CoverageState,
    finesse_guard: Option<FinessePathGuard<'_>>,
) -> Vec<CorePathStep> {
    fn visit(
        graph: &BuildOrderGraph,
        sequence: &[PieceKind],
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        projects_standard_bag_lookahead: bool,
        supply_identity: HoldAutomatonState,
        state: CoverageState,
        finesse_guard: Option<FinessePathGuard<'_>>,
        seen: &mut ExactHashSet<CoverageState>,
        path: &mut Vec<CorePathStep>,
    ) -> bool {
        if !seen.insert(state) {
            return false;
        }
        let node = &graph.nodes[state.node as usize];
        if node.accepting() {
            let projected_terminal_cursor = sequence.len().checked_add(1);
            return !projects_unplaced_lookahead
                || (state.cursor as usize == sequence.len() && state.hold.is_none())
                || (Some(state.cursor as usize) == projected_terminal_cursor
                    && state.hold.is_some());
        }
        for edge in graph
            .edges(state.node as usize)
            .iter()
            .filter(|edge| graph.nodes[edge.to as usize].live)
        {
            if projects_unplaced_lookahead
                && hold_enabled
                && state.cursor as usize == sequence.len()
                && state.hold == Some(edge.piece)
                && graph.nodes[edge.to as usize].accepting()
                && (!projects_standard_bag_lookahead
                    || first_standard_bag_lookahead(sequence).is_none())
            {
                path.push(CorePathStep::new(
                    edge.piece,
                    edge.rotation.quarter_turns(),
                    i32::from(edge.x),
                    i32::from(edge.y),
                    "release-held-at-terminal",
                    edge.cleared_lines,
                ));
                return true;
            }
            for (hold_kind, next) in hold_successors(
                sequence,
                hold_enabled,
                projects_unplaced_lookahead,
                projects_standard_bag_lookahead,
                supply_identity,
                state,
                edge.piece,
                finesse_guard,
            ) {
                path.push(CorePathStep::new(
                    edge.piece,
                    edge.rotation.quarter_turns(),
                    i32::from(edge.x),
                    i32::from(edge.y),
                    hold_kind,
                    edge.cleared_lines,
                ));
                if visit(
                    graph,
                    sequence,
                    hold_enabled,
                    projects_unplaced_lookahead,
                    projects_standard_bag_lookahead,
                    supply_identity,
                    CoverageState {
                        node: edge.to,
                        ..next
                    },
                    finesse_guard,
                    seen,
                    path,
                ) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }

    let mut path = Vec::new();
    let mut seen = ExactHashSet::default();
    let _ = visit(
        graph,
        sequence,
        hold_enabled,
        projects_unplaced_lookahead,
        projects_standard_bag_lookahead,
        supply_identity,
        initial,
        finesse_guard,
        &mut seen,
        &mut path,
    );
    path
}

pub(super) fn representative_pattern_path(
    problem: &SearchProblem,
    graph: &BuildOrderGraph,
    sequence: &[PieceKind],
    finesse_nodes: Option<&[PreparedFinesseNode]>,
) -> Vec<CorePathStep> {
    first_pattern_path(
        graph,
        sequence,
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
        problem.supply().projects_standard_bag_lookahead(),
        problem.initial_hold(),
        CoverageState {
            node: graph.root,
            cursor: problem.initial_hold().cursor(),
            hold: problem.initial_hold().hold_piece(),
        },
        finesse_nodes.map(|nodes| FinessePathGuard {
            nodes,
            spawn_profile: problem.spawn_profile(),
        }),
    )
}

fn hold_successors(
    sequence: &[PieceKind],
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    supply_identity: HoldAutomatonState,
    state: CoverageState,
    required_piece: PieceKind,
    finesse_guard: Option<FinessePathGuard<'_>>,
) -> Vec<(&'static str, CoverageState)> {
    let cursor = usize::from(state.cursor);
    let Some(current) = projected_supply_piece(
        sequence,
        hold_enabled,
        projects_unplaced_lookahead,
        projects_standard_bag_lookahead,
        cursor,
    ) else {
        return Vec::new();
    };
    let mut successors = Vec::with_capacity(3);
    if current == required_piece {
        if let Some(step) = sequence_supply_transition(
            supply_identity,
            state.cursor,
            state.hold,
            hold_enabled,
            SupplyBranchKind::Current,
            current,
            None,
        ) {
            successors.push((
                if cursor < sequence.len() {
                    "use-current"
                } else {
                    "use-unplaced-lookahead"
                },
                CoverageState {
                    cursor: step.next_state.cursor,
                    hold: step.next_state.hold_piece,
                    ..state
                },
            ));
        }
    }
    if !hold_enabled {
        return successors;
    }
    if finesse_guard
        .is_some_and(|guard| !guard.current_piece_can_spawn(state.node as usize, current))
    {
        return successors;
    }
    if state.hold == Some(required_piece) {
        if let Some(step) = sequence_supply_transition(
            supply_identity,
            state.cursor,
            state.hold,
            hold_enabled,
            SupplyBranchKind::SwapHeld,
            current,
            None,
        ) {
            successors.push((
                if cursor < sequence.len() {
                    "swap-held"
                } else {
                    "swap-held-with-unplaced-lookahead"
                },
                CoverageState {
                    cursor: step.next_state.cursor,
                    hold: step.next_state.hold_piece,
                    ..state
                },
            ));
        }
    }
    if state.hold.is_none() {
        let next_index = cursor.checked_add(1);
        let next_piece = next_index.and_then(|index| {
            projected_supply_piece(
                sequence,
                hold_enabled,
                projects_unplaced_lookahead,
                projects_standard_bag_lookahead,
                index,
            )
        });
        if next_piece == Some(required_piece) {
            if let Some(step) = sequence_supply_transition(
                supply_identity,
                state.cursor,
                state.hold,
                hold_enabled,
                SupplyBranchKind::StoreCurrent,
                current,
                next_piece,
            ) {
                successors.push((
                    if next_index.is_some_and(|index| index < sequence.len()) {
                        "store-current-use-next"
                    } else {
                        "store-current-use-unplaced-lookahead"
                    },
                    CoverageState {
                        cursor: step.next_state.cursor,
                        hold: step.next_state.hold_piece,
                        ..state
                    },
                ));
            }
        }
    }
    successors
}

fn sequence_supply_transition(
    identity: HoldAutomatonState,
    cursor: u16,
    hold_piece: Option<PieceKind>,
    hold_enabled: bool,
    branch_kind: SupplyBranchKind,
    current_piece: PieceKind,
    next_piece: Option<PieceKind>,
) -> Option<clearra_supply::SupplyExecutionStep> {
    let state = SupplyExecutionState {
        cursor,
        hold_piece,
        hold_empty: hold_piece.is_none(),
        hold_policy: if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        ..identity
    };
    SupplyExecutionAutomaton::sequence()
        .transition(state, branch_kind, current_piece, next_piece)
        .ok()
}

fn projected_supply_piece(
    sequence: &[PieceKind],
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    index: usize,
) -> Option<PieceKind> {
    sequence.get(index).copied().or_else(|| {
        (hold_enabled
            && projects_unplaced_lookahead
            && projects_standard_bag_lookahead
            && index == sequence.len())
        .then(|| first_standard_bag_lookahead(sequence))
        .flatten()
    })
}

fn first_standard_bag_lookahead(sequence: &[PieceKind]) -> Option<PieceKind> {
    let used_in_current_bag = sequence.len() % PieceKind::STANDARD_TETROMINOES.len();
    if used_in_current_bag != PieceKind::STANDARD_TETROMINOES.len() - 1 {
        return None;
    }
    let current_bag_start = sequence.len() - used_in_current_bag;
    let mut missing = PieceKind::STANDARD_TETROMINOES.into_iter().filter(|piece| {
        !sequence[current_bag_start..]
            .iter()
            .any(|used| used == piece)
    });
    let piece = missing.next()?;
    missing.next().is_none().then_some(piece)
}

fn target_row_for_current_row(height: u8, deleted_rows: u16, current_row: u8) -> Option<u8> {
    let mut visible_row = 0_u8;
    for target_row in 0..height {
        if deleted_rows & (1_u16 << target_row) != 0 {
            continue;
        }
        if visible_row == current_row {
            return Some(target_row);
        }
        visible_row += 1;
    }
    None
}

pub(super) fn merge_deleted_rows(height: u8, previous: u16, current: u16) -> Option<u16> {
    let mut original = 0_u16;
    for current_row in 0..height {
        if current & (1_u16 << current_row) == 0 {
            continue;
        }
        original |= 1_u16 << target_row_for_current_row(height, previous, current_row)?;
    }
    Some(previous | original)
}

pub(super) fn place_and_clear(width: u8, height: u8, board: u64) -> (u64, u16, u8) {
    let row_bits = full_row_mask(width);
    let mut cleared = 0_u16;
    let mut compacted = 0_u64;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = (board >> (input_row as usize * width as usize)) & row_bits;
        if row == row_bits {
            cleared |= 1_u16 << input_row;
        } else {
            compacted |= row << (output_row as usize * width as usize);
            output_row += 1;
        }
    }
    (compacted, cleared, cleared.count_ones() as u8)
}

pub(super) fn placement_is_grounded(width: u8, board: u64, placement: u64) -> bool {
    let floor = full_row_mask(width);
    placement & floor != 0 || ((placement >> width) & board) != 0
}

const fn full_row_mask(width: u8) -> u64 {
    if width == u64::BITS as u8 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

fn compact_target_board(width: u8, height: u8, board: u64, deleted_rows: u16) -> u64 {
    let row_bits = full_row_mask(width);
    let mut compacted = 0_u64;
    let mut output_row = 0_u8;
    for target_row in 0..height {
        if deleted_rows & (1_u16 << target_row) != 0 {
            continue;
        }
        let row = (board >> (target_row as usize * width as usize)) & row_bits;
        compacted |= row << (output_row as usize * width as usize);
        output_row += 1;
    }
    compacted
}

fn occupied_rows(width: u8, mut cells: u64) -> u16 {
    let mut rows = 0_u16;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        rows |= 1_u16 << (cell / width as usize);
    }
    rows
}

#[cfg(test)]
mod fixed_witness_tests {
    use super::{fixed_witness_branches, witness_piece_code, FixedWitnessProductState};
    use clearra_core_domain::piece::piece_kind::PieceKind;

    fn state(extra_draw: u8, hold: Option<PieceKind>) -> FixedWitnessProductState {
        FixedWitnessProductState {
            subset: 0,
            extra_draw,
            hold_code: hold.map_or(0, witness_piece_code),
            terminal_projection_consumed: false,
        }
    }

    #[test]
    fn fixed_queue_without_hold_only_uses_current_piece() {
        let branches = fixed_witness_branches(
            &[PieceKind::I, PieceKind::O],
            0,
            false,
            false,
            false,
            state(0, None),
        );
        let actual = branches
            .iter()
            .map(|branch| (branch.desired_piece, branch.hold_kind))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![(witness_piece_code(PieceKind::I), "use-current")]
        );
    }

    #[test]
    fn fixed_queue_occupied_hold_can_swap_with_current() {
        let branches = fixed_witness_branches(
            &[PieceKind::I],
            0,
            true,
            false,
            false,
            state(0, Some(PieceKind::O)),
        );
        let actual = branches
            .iter()
            .map(|branch| {
                (
                    branch.desired_piece,
                    branch.next_hold_code,
                    branch.hold_kind,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (
                    witness_piece_code(PieceKind::I),
                    witness_piece_code(PieceKind::O),
                    "use-current",
                ),
                (
                    witness_piece_code(PieceKind::O),
                    witness_piece_code(PieceKind::I),
                    "swap-held",
                ),
            ]
        );
    }

    #[test]
    fn fixed_queue_equal_current_and_hold_has_one_semantic_branch() {
        let branches = fixed_witness_branches(
            &[PieceKind::I],
            0,
            true,
            false,
            false,
            state(0, Some(PieceKind::I)),
        );
        assert_eq!(branches.iter().count(), 1);
    }

    #[test]
    fn fixed_queue_empty_hold_can_store_current_and_use_next() {
        let branches = fixed_witness_branches(
            &[PieceKind::I, PieceKind::O],
            0,
            true,
            false,
            false,
            state(0, None),
        );
        let stored = branches
            .iter()
            .find(|branch| branch.hold_kind == "store-current-use-next")
            .expect("empty hold branch");
        assert_eq!(stored.desired_piece, witness_piece_code(PieceKind::O));
        assert_eq!(stored.next_hold_code, witness_piece_code(PieceKind::I));
        assert_eq!(stored.next_extra_draw, 1);
    }

    #[test]
    fn fixed_queue_releases_hold_once_at_the_projected_terminal_step() {
        let branches = fixed_witness_branches(
            &[PieceKind::I],
            1,
            true,
            true,
            true,
            state(0, Some(PieceKind::O)),
        );
        let branch = branches.iter().next().expect("terminal release branch");
        assert_eq!(branches.iter().count(), 1);
        assert_eq!(branch.desired_piece, witness_piece_code(PieceKind::O));
        assert_eq!(branch.next_hold_code, 0);
        assert!(branch.terminal_projection_consumed);
        assert_eq!(branch.hold_kind, "release-held-at-terminal");
    }

    #[test]
    fn fixed_queue_terminal_release_is_fail_closed_outside_the_exact_terminal_state() {
        let mut consumed = state(0, Some(PieceKind::O));
        consumed.terminal_projection_consumed = true;
        for branches in [
            fixed_witness_branches(
                &[PieceKind::I],
                1,
                true,
                false,
                true,
                state(0, Some(PieceKind::O)),
            ),
            fixed_witness_branches(
                &[PieceKind::I],
                1,
                true,
                true,
                false,
                state(0, Some(PieceKind::O)),
            ),
            fixed_witness_branches(&[PieceKind::I], 1, true, true, true, state(0, None)),
            fixed_witness_branches(&[PieceKind::I], 1, true, true, true, consumed),
        ] {
            assert_eq!(branches.iter().count(), 0);
        }
    }
}
// SRP rationale: this module has one behavior-level change reason: exact WASM BuildUp state expansion and trace preservation.
