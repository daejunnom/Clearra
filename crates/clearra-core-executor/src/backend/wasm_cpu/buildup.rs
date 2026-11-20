use std::collections::BTreeMap;
use std::sync::Arc;

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
    conditioned_local_relation::LocalRelationRowFrame,
    legal_board::{
        legal_board_negative_snapshot, CompletionCapability, LegalBoardNegativeOwner,
        LegalBoardNegativeQueryCache, LegalBoardQuery,
    },
    performance::{ExecutorSearchStage, SearchStageSpan},
    CorePathStep,
};

#[cfg(any(feature = "parallel", test))]
use super::{
    reachability::SharedReachabilityTemplates, standard_bag_coverage::StandardBagMemoAccounting,
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
    standard_bag_coverage::{
        SharedStandardBagRequest, StandardBagCoverage, StandardBagCoverageResult,
    },
    WasmExactSearchError,
};

// Keep the low-volume symbolic fast path intact, but recycle accelerator
// arenas before wasm32's fixed address space is exhausted on full searches.
const SYMBOLIC_CACHE_LIVE_LIMIT: usize = 256 * 1024 * 1024;
// Candidate subset count is bounded by MAX_BOARD64_PIECES (15). These tags
// reuse existing generation-stamped graph storage, never a second per-worker
// table, and cannot alias a real node index.
const LEGAL_BOARD_PENDING_NODE: u32 = u32::MAX - 1;
const LEGAL_BOARD_REJECTED_NODE: u32 = u32::MAX;

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GeometricEdgeReuseMetrics {
    /// Fresh grounding/collision and geometry checks when reuse is enabled.
    pub footprint_validations: u64,
    /// Subsequent equal masks for which no geometry check was performed.
    pub reused_validations: u64,
    pub line_clear_computations: u64,
    pub projection_validations: u64,
}

#[cfg(any(test, feature = "local-search-ab"))]
mod geometric_edge_reuse {
    use super::GeometricEdgeReuseMetrics;

    #[cfg(feature = "local-search-ab")]
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(test)]
    thread_local! {
        static MODE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
        static METRICS: std::cell::Cell<GeometricEdgeReuseMetrics> =
            std::cell::Cell::new(GeometricEdgeReuseMetrics::default());
    }

    #[cfg(feature = "local-search-ab")]
    static VALIDATIONS: AtomicU64 = AtomicU64::new(0);
    #[cfg(feature = "local-search-ab")]
    static REUSED: AtomicU64 = AtomicU64::new(0);
    #[cfg(feature = "local-search-ab")]
    static LINE_CLEARS: AtomicU64 = AtomicU64::new(0);
    #[cfg(feature = "local-search-ab")]
    static PROJECTIONS: AtomicU64 = AtomicU64::new(0);

    pub(super) fn enabled() -> bool {
        #[cfg(test)]
        if let Some(enabled) = MODE.with(std::cell::Cell::get) {
            return enabled;
        }
        #[cfg(feature = "local-search-ab")]
        {
            static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            *MODE.get_or_init(|| {
                std::env::var("CLEARRA_GEOMETRIC_EDGE_REUSE").is_ok_and(|value| value == "1")
            })
        }
        #[cfg(not(feature = "local-search-ab"))]
        {
            false
        }
    }

    pub(super) fn record(delta: GeometricEdgeReuseMetrics) {
        if delta.footprint_validations == 0 {
            return;
        }
        #[cfg(test)]
        METRICS.with(|slot| {
            let mut value = slot.get();
            value.footprint_validations += delta.footprint_validations;
            value.reused_validations += delta.reused_validations;
            value.line_clear_computations += delta.line_clear_computations;
            value.projection_validations += delta.projection_validations;
            slot.set(value);
        });
        #[cfg(feature = "local-search-ab")]
        {
            VALIDATIONS.fetch_add(delta.footprint_validations, Ordering::Relaxed);
            REUSED.fetch_add(delta.reused_validations, Ordering::Relaxed);
            LINE_CLEARS.fetch_add(delta.line_clear_computations, Ordering::Relaxed);
            PROJECTIONS.fetch_add(delta.projection_validations, Ordering::Relaxed);
        }
    }

    #[cfg(feature = "local-search-ab")]
    pub(super) fn totals() -> GeometricEdgeReuseMetrics {
        GeometricEdgeReuseMetrics {
            footprint_validations: VALIDATIONS.load(Ordering::Relaxed),
            reused_validations: REUSED.load(Ordering::Relaxed),
            line_clear_computations: LINE_CLEARS.load(Ordering::Relaxed),
            projection_validations: PROJECTIONS.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(super) fn with_mode<T>(
        enabled: bool,
        action: impl FnOnce() -> T,
    ) -> (T, GeometricEdgeReuseMetrics) {
        struct Restore(Option<bool>, GeometricEdgeReuseMetrics);
        impl Drop for Restore {
            fn drop(&mut self) {
                MODE.with(|slot| slot.set(self.0));
                METRICS.with(|slot| slot.set(self.1));
            }
        }
        let _restore = Restore(
            MODE.with(|slot| slot.replace(Some(enabled))),
            METRICS.with(|slot| slot.replace(GeometricEdgeReuseMetrics::default())),
        );
        let output = action();
        (output, METRICS.with(std::cell::Cell::get))
    }
}

/// Process-local totals for one opt-in, joined benchmark run.
#[cfg(feature = "local-search-ab")]
pub fn geometric_edge_reuse_experiment_totals() -> GeometricEdgeReuseMetrics {
    geometric_edge_reuse::totals()
}

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
    // Worker-local suffix-language identity, consumed before the owning
    // BuildUpWorkspace may recycle its language arena. Never exported as an ID.
    #[cfg(any(test, feature = "local-search-ab"))]
    canonical_language_root: Option<u32>,
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

#[cfg(any(feature = "parallel", test))]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BuildUpMemoryComponents {
    pub piece_language_bytes: usize,
    pub standard_bag_bytes: usize,
    pub reachability_bytes: usize,
    pub graph_projection_bytes: usize,
    pub other_bytes: usize,
}

#[derive(Default)]
pub(super) struct BuildUpWorkspace {
    realization_feasibility: RealizationFeasibilityWorkspace,
    piece_order_languages: PieceOrderLanguageCache,
    standard_bag_coverage: Option<StandardBagCoverage>,
    standard_bag_coverage_initialized: bool,
    shared_standard_bag_request: Option<Arc<SharedStandardBagRequest>>,
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
    legal_board: Option<LegalBoardNegativeOwner>,
    legal_board_profile: Option<clearra_rules::kicks::KickTableProfileId>,
    legal_board_enabled: Option<bool>,
    legal_board_policy_enabled: Option<bool>,
    legal_board_query_cache: LegalBoardNegativeQueryCache,
    legal_board_verified_negative_prunes: usize,
}

impl BuildUpWorkspace {
    #[cfg(any(feature = "parallel", test))]
    pub fn set_shared_reachability_templates(
        &mut self,
        templates: Arc<SharedReachabilityTemplates>,
    ) -> Result<(), WasmExactSearchError> {
        self.reachability
            .set_shared_templates(templates)
            .map_err(WasmExactSearchError::InvalidProblem)
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn set_shared_standard_bag_request(
        &mut self,
        request: Arc<SharedStandardBagRequest>,
    ) -> Result<(), WasmExactSearchError> {
        if self.standard_bag_coverage_initialized {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_shared_request_already_initialized",
            ));
        }
        self.shared_standard_bag_request = Some(request);
        Ok(())
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn shared_standard_bag_request_retained_bytes(&self) -> usize {
        if self.shared_standard_bag_request.is_none() {
            return 0;
        }
        self.standard_bag_coverage
            .as_ref()
            .map_or(0, StandardBagCoverage::shared_request_retained_bytes)
    }

    /// Final worker snapshot only; no timer or allocation on solver hot paths.
    #[cfg(any(feature = "parallel", test))]
    pub fn memory_components(&self) -> BuildUpMemoryComponents {
        let piece_language_bytes = self.piece_order_languages.retained_bytes();
        let standard_bag_bytes = self
            .standard_bag_coverage
            .as_ref()
            .map_or(0, StandardBagCoverage::retained_bytes);
        let reachability_bytes = self.reachability.private_retained_bytes();
        let graph_projection_bytes = self.graph_projection_retained_bytes();
        BuildUpMemoryComponents {
            piece_language_bytes,
            standard_bag_bytes,
            reachability_bytes,
            graph_projection_bytes,
            other_bytes: self.other_retained_bytes(),
        }
    }

    fn graph_projection_retained_bytes(&self) -> usize {
        self.graph_nodes.capacity() * core::mem::size_of::<BuildNode>()
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
    }

    fn other_retained_bytes(&self) -> usize {
        // Count private components directly, never subtract two snapshots of
        // a shared OnceLock which another worker may initialize in between.
        self.realization_feasibility.retained_bytes()
            + self.observation_language_roots.capacity() * core::mem::size_of::<u32>()
            + self.legal_board_query_cache.retained_bytes()
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn standard_bag_memo_storage_label(&self) -> &'static str {
        self.standard_bag_coverage
            .as_ref()
            .map_or("not-used", StandardBagCoverage::memo_storage_label)
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn standard_bag_memo_retained_payload_bytes(&self) -> usize {
        self.standard_bag_coverage
            .as_ref()
            .map_or(0, StandardBagCoverage::memo_retained_payload_bytes)
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn standard_bag_memo_accounting(&self) -> StandardBagMemoAccounting {
        self.standard_bag_coverage.as_ref().map_or_else(
            StandardBagMemoAccounting::default,
            StandardBagCoverage::memo_accounting,
        )
    }

    fn configure_legal_board(
        &mut self,
        profile: clearra_rules::kicks::KickTableProfileId,
        enabled: bool,
    ) {
        // A workspace is one execution-session owner. Snapshot the immutable
        // generation once so a concurrent install can never mix generations
        // between candidates in the same result.
        let policy_enabled = crate::search_prune_policy::legal_board_enabled();
        if self.legal_board_profile == Some(profile)
            && self.legal_board_enabled == Some(enabled)
            && self.legal_board_policy_enabled == Some(policy_enabled)
        {
            return;
        }
        self.legal_board = enabled
            .then(|| legal_board_negative_snapshot(profile))
            .flatten();
        self.legal_board_profile = Some(profile);
        self.legal_board_enabled = Some(enabled);
        self.legal_board_policy_enabled = Some(policy_enabled);
        self.legal_board_query_cache.reset(profile);
    }

    pub fn retained_bytes(&self) -> usize {
        self.other_retained_bytes()
            + self.piece_order_languages.retained_bytes()
            + self
                .standard_bag_coverage
                .as_ref()
                .map_or(0, StandardBagCoverage::retained_bytes)
            + self.reachability.retained_bytes()
            + self.graph_projection_retained_bytes()
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

    pub const fn legal_board_verified_negative_prunes(&self) -> usize {
        self.legal_board_verified_negative_prunes
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

    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn capture_p7p4_root(
        &mut self,
        root: u32,
    ) -> Result<Option<Arc<super::standard_bag_coverage::P7P4CoverageBlocks>>, WasmExactSearchError>
    {
        match self.standard_bag_coverage.as_mut() {
            Some(coverage) => coverage.capture_p7p4_root(root),
            None => Ok(None),
        }
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
            self.standard_bag_coverage = match self.shared_standard_bag_request.as_deref() {
                Some(shared) => StandardBagCoverage::for_universe_with_shared(
                    universe,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                    shared,
                )?,
                None => StandardBagCoverage::for_universe(
                    universe,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                )?,
            };
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

    /// A previously rejected child needs no realization or asset lookup. The
    /// candidate generation also prevents a prior candidate's tag from pruning.
    #[inline]
    fn legal_board_child_is_rejected(&self, child: usize, graph_generation: u32) -> bool {
        self.graph_reachable_generations[child] == graph_generation
            && self.graph_subset_node_ids[child] == LEGAL_BOARD_REJECTED_NODE
    }

    /// A child has one exact projected board regardless of its predecessor
    /// operation or concrete rotation. Query it only after finding a valid
    /// concrete edge, before exact lock traversal. Positive admission alone
    /// remains no evidence of reachability.
    fn legal_board_child_allows(
        &mut self,
        catalog: &GeometryCatalog,
        profile: clearra_rules::kicks::KickTableProfileId,
        projection: &mut CandidateProjection,
        child: usize,
        graph_generation: u32,
    ) -> bool {
        if self.graph_reachable_generations[child] == graph_generation {
            return self.graph_subset_node_ids[child] != LEGAL_BOARD_REJECTED_NODE;
        }
        let (board, deleted_rows) = projection.state(child);
        let owner = self.legal_board.as_ref();
        let allowed = self.legal_board_query_cache.allows(
            LegalBoardQuery {
                width: catalog.width(),
                height: catalog.height(),
                initial_board: catalog.initial_board(),
                kick_profile: profile,
                physical_board: board,
                deleted_original_rows: deleted_rows,
                placed_piece_count: child.count_ones() as usize,
                completion: CompletionCapability::ClearToEmpty,
            },
            || {
                crate::search_prune_policy::local_pc4_legal_board_allows(
                    owner,
                    catalog.width(),
                    catalog.height(),
                    catalog.initial_board(),
                    profile,
                    true,
                    board,
                    deleted_rows,
                    child.count_ones() as usize,
                )
            },
        );
        self.graph_reachable_generations[child] = graph_generation;
        self.graph_subset_node_ids[child] = if allowed {
            LEGAL_BOARD_PENDING_NODE
        } else {
            self.legal_board_verified_negative_prunes =
                self.legal_board_verified_negative_prunes.saturating_add(1);
            LEGAL_BOARD_REJECTED_NODE
        };
        allowed
    }
}

pub(super) struct CandidateProjection {
    #[cfg(any(test, feature = "local-search-ab"))]
    shared_row_conditions: Option<super::placement_conditions::GroupedClearConditions>,
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
    let standard_bag_cursor_bytes =
        StandardBagCoverage::checked_cursor_transition_retained_bytes(universe)?;
    #[cfg(any(test, feature = "local-search-ab"))]
    let row_condition_bytes = if super::placement_dependency::enabled() {
        core::mem::size_of::<Option<super::placement_conditions::GroupedClearConditions>>()
    } else {
        0
    };
    #[cfg(not(any(test, feature = "local-search-ab")))]
    let row_condition_bytes = 0;

    subset_count
        .checked_mul(projection_and_feasibility_per_subset)?
        .checked_add(subset_count.checked_mul(graph_per_subset)?)?
        .checked_add(graph_edge_bytes)?
        .checked_add(product_bytes)?
        .checked_add(pattern_bytes)?
        .checked_add(language_cache_bytes)?
        .checked_add(trace_bytes)?
        .checked_add(reachability_bytes)?
        .checked_add(standard_bag_cursor_bytes)?
        .checked_add(row_condition_bytes as u128)?
        .checked_add(crate::legal_board::NEGATIVE_QUERY_CACHE_BYTES as u128)?
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
        for (row, &contributors) in row_contributors
            .iter()
            .enumerate()
            .take(catalog.height() as usize)
        {
            let initial_row =
                (catalog.initial_board() >> (row * catalog.width() as usize)) & row_bits;
            if initial_row == row_bits && contributors == 0 {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_initial_full_row_requires_preclear_normalization",
                ));
            }
        }
        let mut completed_target_rows = 0_u16;
        let mut final_board = catalog.initial_board();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut group_admitted =
            super::placement_dependency::enabled() && catalog.placement_conditions().is_some();
        for &cells in operation_cells.iter().take(operation_count) {
            #[cfg(any(test, feature = "local-search-ab"))]
            if group_admitted && final_board & cells != 0 {
                // Keep the old specialization gate: duplicate or
                // overlapping operations cannot prove a row conjunction.
                group_admitted = false;
            }
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
        #[cfg(any(test, feature = "local-search-ab"))]
        let shared_row_conditions = if group_admitted {
            // These facts belong to this exact candidate and operation
            // order. Group them directly instead of looking up every row
            // again and retaining a second copy of the selected IDs.
            super::placement_conditions::GroupedClearConditions::from_completed_rows(
                &row_contributors[..usize::from(catalog.height())],
                completed_target_rows,
            )
        } else {
            None
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(conditions) = &shared_row_conditions {
            super::placement_dependency::record(|m| {
                m.row_conditions_specialized += 1;
                m.row_condition_groups += conditions.group_count() as u64;
                m.completed_row_events += u64::from(conditions.completed_rows().count_ones());
                m.atomic_row_groups += conditions.atomic_group_count() as u64;
                m.row_condition_peak_bytes = m
                    .row_condition_peak_bytes
                    .max(conditions.retained_bytes() as u64);
            });
        }
        Ok(Self {
            #[cfg(any(test, feature = "local-search-ab"))]
            shared_row_conditions,
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
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(conditions) = &self.shared_row_conditions {
            return conditions.deleted_rows(subset as u16);
        }
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

#[cfg(test)]
mod candidate_projection_clear_fact_reuse_tests {
    use super::*;

    #[test]
    fn candidate_projection_clear_fact_reuse_matches_concrete_subsets_and_scalar_rows() {
        let ((selected, owned_groups), metrics) =
            super::super::placement_dependency::with_mode(true, || {
                let initial = 0x0033;
                let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
                    4, 4, initial, 0xffcc,
                )
                .unwrap();
                let rows = [0x3300, 0x00cc, 0xcc00]
                    .map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap());
                let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
                let mut workspace = BuildUpWorkspace::default();
                let mut projection = CandidateProjection::compile(
                    &catalog,
                    &candidate,
                    &mut workspace,
                    BuildCompletion::ClearToEmpty,
                )
                .unwrap();
                let selected: Vec<_> = candidate
                    .row_ids()
                    .iter()
                    .map(|&row_id| catalog.skeleton(row_id).cells)
                    .collect();
                let grouped = projection.shared_row_conditions.as_ref().unwrap();
                let owned_groups = grouped.groups().collect::<Vec<_>>();
                let independent = catalog
                    .placement_conditions()
                    .unwrap()
                    .specialize(candidate.row_ids())
                    .unwrap();
                for subset in 0..(1 << selected.len()) {
                    let logical =
                        selected
                            .iter()
                            .enumerate()
                            .fold(initial, |board, (i, &cells)| {
                                if subset & (1 << i) == 0 {
                                    board
                                } else {
                                    board | cells
                                }
                            });
                    let mut deleted = 0_u16;
                    let mut physical = 0_u64;
                    let mut destination_row = 0;
                    for row in 0..4 {
                        let cells = (logical >> (row * 4)) & 0xf;
                        if cells == 0xf {
                            deleted |= 1 << row;
                        } else {
                            physical |= cells << (destination_row * 4);
                            destination_row += 1;
                        }
                    }
                    assert_eq!(independent.deleted_rows(subset as u16), deleted);
                    assert_eq!(projection.state(subset), (physical, deleted));
                }
                (selected, owned_groups)
            });
        let bottom = 1_u16 << selected.iter().position(|&cells| cells == 0x00cc).unwrap();
        assert_eq!(owned_groups, [(bottom, 0b0011), (0b111 ^ bottom, 0b1100)]);
        assert_eq!(metrics.row_conditions_specialized, 1);
        assert_eq!(metrics.atomic_row_groups, 2);
        assert_eq!(metrics.row_condition_peak_bytes, 0);
    }

    #[test]
    fn candidate_projection_clear_fact_reuse_still_rejects_initial_full_rows() {
        for enabled in [false, true] {
            super::super::placement_dependency::with_mode(enabled, || {
                let catalog =
                    GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0x000f, 0x0330)
                        .unwrap();
                let row = catalog.skeleton_id(PieceKind::O, 0x0330).unwrap();
                let candidate = GeometryCandidate::from_rows(&catalog, 0, &[row]).unwrap();
                let mut workspace = BuildUpWorkspace::default();
                assert!(matches!(
                    CandidateProjection::compile(
                        &catalog,
                        &candidate,
                        &mut workspace,
                        BuildCompletion::ClearToEmpty
                    ),
                    Err(WasmExactSearchError::InvalidProblem(
                        "wasm_initial_full_row_requires_preclear_normalization"
                    ))
                ));
            });
        }
    }

    #[test]
    fn candidate_projection_clear_fact_reuse_respects_catalog_admission() {
        let (catalog, _) = super::super::placement_dependency::with_mode(false, || {
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0x0033, 0x00cc).unwrap()
        });
        let (_, metrics) = super::super::placement_dependency::with_mode(true, || {
            let row = catalog.skeleton_id(PieceKind::O, 0x00cc).unwrap();
            let candidate = GeometryCandidate::from_rows(&catalog, 0, &[row]).unwrap();
            let mut workspace = BuildUpWorkspace::default();
            let mut projection = CandidateProjection::compile(
                &catalog,
                &candidate,
                &mut workspace,
                BuildCompletion::ClearToEmpty,
            )
            .unwrap();
            assert!(projection.shared_row_conditions.is_none());
            assert_eq!(projection.state(1), (0, 0b11));
        });
        assert_eq!(metrics.row_conditions_specialized, 0);
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
        coverage_already_known: impl FnOnce() -> bool,
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
        // Only this witness mode can use the answer. In particular, standard
        // bag targets without a position index never need the superset scan.
        if coverage_already_known() {
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

// The verifier keeps independent reusable workspaces explicit on its hot path.
#[allow(clippy::too_many_arguments)]
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
    let movement_proofs = CandidateMovementProofs::default();
    #[cfg(any(test, feature = "local-search-ab"))]
    let movement_proofs = {
        let eligible = problem.count_policy() == PcCountPolicy::CountUnique
            && matches!(completion, BuildCompletion::ClearToEmpty)
            && !retain_trace
            && !finesse_requested
            && !finesse_spin_coverage_requested
            && !witness_mode.enabled()
            && !problem.objective().execution_constraints().requested()
            && !problem
                .queue_observation_policy()
                .requires_observation_policy();
        let mut proofs = movement_proofs;
        proofs.enabled = eligible && super::structural_order::movement_clear_certificates_enabled();
        proofs.first_kick_enabled =
            eligible && super::structural_order::first_kick_intervals_enabled();
        proofs.ordered_kick_enabled =
            eligible && super::structural_order::ordered_kick_intervals_enabled();
        proofs.kick_guard_reuse_enabled = (proofs.first_kick_enabled
            || proofs.ordered_kick_enabled)
            && super::structural_order::ordered_kick_guard_reuse_enabled();
        if eligible && super::structural_order::enabled() {
            if let Some(result) = verify_certified_poset(
                problem,
                catalog,
                candidate,
                workspace,
                &mut proofs,
                control,
            )? {
                return Ok(result);
            }
        }
        if eligible && super::structural_order::conditional_order_enabled() {
            if let Some(result) = verify_conditional_order(
                problem,
                catalog,
                candidate,
                workspace,
                &mut proofs,
                control,
            )? {
                return Ok(result);
            }
        }
        proofs
    };
    workspace.reachability.configure(candidate.row_ids().len());
    workspace.reachability.configure_kick_profile(
        kick_profile_id,
        problem.backend_policy().conditioned_reachability_enabled(),
    );
    workspace
        .reachability
        .configure_conditioned_boolean_shortcuts(
            problem.count_policy() == PcCountPolicy::CountUnique
                && !retain_trace
                && !witness_mode.enabled()
                && !finesse_requested
                && !finesse_spin_coverage_requested
                && !problem.objective().execution_constraints().requested(),
        );
    workspace.configure_legal_board(
        kick_profile_id,
        problem.backend_policy().exact_legal_board_enabled(),
    );
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
        &movement_proofs,
        control,
    );
    workspace.recycle_projection(projection);
    result
}

// Evidence lives only until this candidate's fallback graph is constructed.
// The owning call fixes catalog, kick profile, shape and evidence capability.
// A full buffer is a cache miss, not failure or negative authority.
#[derive(Default)]
struct CandidateMovementProofs {
    #[cfg(any(test, feature = "local-search-ab"))]
    enabled: bool,
    #[cfg(any(test, feature = "local-search-ab"))]
    first_kick_enabled: bool,
    #[cfg(any(test, feature = "local-search-ab"))]
    ordered_kick_enabled: bool,
    #[cfg(any(test, feature = "local-search-ab"))]
    kick_guard_reuse_enabled: bool,
    #[cfg(any(test, feature = "local-search-ab"))]
    intervals: [Option<(
        super::placement_poset::MovementProofQuery,
        MovementProofSource,
    )>; 16],
    #[cfg(any(test, feature = "local-search-ab"))]
    len: usize,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MovementProofSource {
    Poset,
    ConditionalOrder,
}

#[cfg(any(test, feature = "local-search-ab"))]
enum PositiveEdgeProof {
    Reused,
    FirstKick,
    OrderedKick,
    ReusedKickGuard,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KickGuardKey {
    piece: PieceKind,
    realization: super::catalog::InstantiatedRealization,
    deleted: u16,
    order: super::reachability::KickWitnessOrder,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy)]
struct KickGuardEntry {
    key: KickGuardKey,
    guard: super::reachability::KickWitnessGuard,
}

// Keep this candidate's graph witnesses separate from certificates retained
// by partial poset/conditional admission. Neither can evict the other's proof.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Default)]
struct CandidateKickGuardLedger {
    entries: [Option<KickGuardEntry>; 16],
    len: usize,
    conflicted: bool,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl CandidateKickGuardLedger {
    fn proves(&self, key: KickGuardKey, board: u64) -> bool {
        !self.conflicted
            && self.entries[..self.len].iter().flatten().any(|entry| {
                entry.key == key
                    && entry.guard.minimum & !board == 0
                    && board & !entry.guard.maximum == 0
            })
    }

    fn remember(
        &mut self,
        entry: KickGuardEntry,
        source: &CandidateMovementProofs,
        board: u64,
    ) -> bool {
        use super::placement_poset::MovementProof;
        if self.conflicted {
            return false;
        }
        if entry.guard.minimum & !board != 0
            || board & !entry.guard.maximum != 0
            || source.intervals[..source.len]
                .iter()
                .flatten()
                .any(|(query, _)| {
                    query.piece == entry.key.piece
                        && query.realization == entry.key.realization
                        && query.deleted == entry.key.deleted
                        && matches!(query.proof, MovementProof::Closed)
                        // The negative holds on every superset of its minimum.
                        // There is a contradictory point if the joined minima
                        // fit wholly within the positive interval's maximum.
                        && (query.minimum | entry.guard.minimum) & !entry.guard.maximum == 0
                })
        {
            self.len = 0;
            self.conflicted = true;
            return false;
        }
        if self.len == self.entries.len()
            || self.entries[..self.len].iter().flatten().any(|prior| {
                prior.key == entry.key
                    && prior.guard.minimum & !entry.guard.minimum == 0
                    && entry.guard.maximum & !prior.guard.maximum == 0
            })
        {
            return false;
        }
        self.entries[self.len] = Some(entry);
        self.len += 1;
        true
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
#[allow(clippy::too_many_arguments)]
fn try_kick_interval_edge(
    reachability: &mut ReachabilityWorkspace,
    catalog: &GeometryCatalog,
    board: u64,
    piece: PieceKind,
    profile: clearra_rules::kicks::KickTableProfileId,
    realization: super::catalog::InstantiatedRealization,
    frame: LocalRelationRowFrame,
    order: super::reachability::KickWitnessOrder,
    source: &CandidateMovementProofs,
    ledger: &mut CandidateKickGuardLedger,
    axis_metrics: &mut super::structural_order::KickIntervalMetrics,
    guard_metrics: &mut super::structural_order::KickGuardReuseMetrics,
) -> Option<PositiveEdgeProof> {
    use super::reachability::KickWitnessOrder;

    let kind = match order {
        KickWitnessOrder::RawFirst => PositiveEdgeProof::FirstKick,
        KickWitnessOrder::LaterOnly => PositiveEdgeProof::OrderedKick,
        KickWitnessOrder::Any => return None,
    };
    if source.kick_guard_reuse_enabled {
        let key = KickGuardKey {
            piece,
            realization,
            deleted: u16::from(frame.deleted_original_rows()),
            order,
        };
        if ledger.proves(key, board) {
            return Some(PositiveEdgeProof::ReusedKickGuard);
        }
        axis_metrics.attempts += 1;
        let guard = reachability.try_kick_witness_guard(
            catalog,
            board,
            board,
            piece,
            profile,
            realization,
            Some(frame),
            order,
        )?;
        axis_metrics.proofs += 1;
        guard_metrics.constructed += 1;
        let stored = ledger.remember(KickGuardEntry { key, guard }, source, board);
        guard_metrics.stored += u64::from(stored);
        return (!ledger.conflicted).then_some(kind);
    }
    axis_metrics.attempts += 1;
    let proved = match order {
        KickWitnessOrder::RawFirst => reachability.try_prove_first_kick_lock_interval(
            catalog,
            board,
            board,
            piece,
            profile,
            realization,
            Some(frame),
        ),
        KickWitnessOrder::LaterOnly => reachability.try_prove_ordered_kick_lock_interval(
            catalog,
            board,
            board,
            piece,
            profile,
            realization,
            Some(frame),
        ),
        KickWitnessOrder::Any => return None,
    };
    axis_metrics.proofs += u64::from(proved);
    proved.then_some(kind)
}

#[cfg(any(test, feature = "local-search-ab"))]
impl CandidateMovementProofs {
    fn remember(&mut self, query: super::placement_poset::MovementProofQuery) {
        if self.enabled {
            self.insert(query, MovementProofSource::Poset);
        }
    }

    fn insert(
        &mut self,
        query: super::placement_poset::MovementProofQuery,
        source: MovementProofSource,
    ) {
        if self.len < self.intervals.len() {
            self.intervals[self.len] = Some((query, source));
            self.len += 1;
        }
    }

    fn answer(
        &self,
        piece: PieceKind,
        realization: super::catalog::InstantiatedRealization,
        deleted: u16,
        board: u64,
    ) -> Option<(bool, MovementProofSource)> {
        use super::placement_poset::MovementProof;

        self.intervals[..self.len]
            .iter()
            .flatten()
            .find_map(|(query, source)| {
                if query.piece != piece
                    || query.realization != realization
                    || query.deleted != deleted
                    || query.minimum & !board != 0
                {
                    return None;
                }
                match query.proof {
                    MovementProof::Reachable { maximum } if board & !maximum == 0 => {
                        Some((true, *source))
                    }
                    MovementProof::Closed => Some((false, *source)),
                    MovementProof::Reachable { .. } => None,
                }
            })
    }
}

/// For a small support-OR candidate, certify its executed-set language
/// before allocating the ordinary BuildOrderGraph. All physical facts come
/// from the same original-row quotient or a bounded analytic pose proof.
#[cfg(any(test, feature = "local-search-ab"))]
fn verify_conditional_order(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    movement_proofs: &mut CandidateMovementProofs,
    control: &ExecutionControl,
) -> Result<Option<CandidateBuildResult>, WasmExactSearchError> {
    use super::{
        piece_order_language::{ConditionalLanguageCompilation, ConditionalTransitionProof},
        placement_poset::CandidateGeometry,
        structural_order::{record_conditional_order, ConditionalOrderMetrics},
    };

    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let Some(universe) = problem.piece_source().materialized_universe() else {
        return Ok(None);
    };
    if !StandardBagCoverage::supports(universe, problem.initial_hold()) {
        return Ok(None);
    }
    let Some(geometry) = CandidateGeometry::for_conditional_order(catalog, candidate) else {
        return Ok(None);
    };
    let profile = problem.kick_profile().profile_id();
    workspace.reachability.configure_kick_profile(
        profile,
        problem.backend_policy().conditioned_reachability_enabled(),
    );
    let mut metrics = ConditionalOrderMetrics {
        attempted_candidates: 1,
        ..ConditionalOrderMetrics::default()
    };
    let mut current_state = None;
    let compiled = workspace
        .piece_order_languages
        .compile_conditional_language(geometry.pieces(), 64, 256, |executed, operation| {
            metrics.checked_transitions += 1;
            if control.is_cancelled() {
                return ConditionalTransitionProof::Unknown;
            }
            if current_state
                .as_ref()
                .is_none_or(|(mask, _)| *mask != executed)
            {
                current_state = Some((executed, geometry.conditional_state(executed)));
            }
            let state = &current_state.as_ref().expect("current frame initialized").1;
            prove_conditional_transition(
                catalog,
                &geometry,
                candidate.row_ids()[operation],
                operation,
                executed,
                state,
                profile,
                &mut workspace.reachability,
                movement_proofs,
            )
        })?;
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let language = match compiled {
        ConditionalLanguageCompilation::Certified(language) => {
            metrics.certified_candidates = 1;
            metrics.residual_states = language.residual_states as u64;
            debug_assert_eq!(
                metrics.checked_transitions,
                language.checked_transitions as u64
            );
            language
        }
        ConditionalLanguageCompilation::Unknown | ConditionalLanguageCompilation::Unsupported => {
            metrics.unknown_candidates = 1;
            record_conditional_order(metrics);
            return Ok(None);
        }
        ConditionalLanguageCompilation::BudgetExceeded => {
            metrics.budget_candidates = 1;
            record_conditional_order(metrics);
            return Ok(None);
        }
    };
    let coverage = if let Some(root) = language.root {
        let Some(coverage) = workspace.cover_standard_bag_language(problem, root, control)? else {
            record_conditional_order(metrics);
            return Ok(None);
        };
        Some(coverage)
    } else {
        metrics.empty_certificates = 1;
        None
    };
    metrics.graph_bypasses = 1;
    record_conditional_order(metrics);
    Ok(Some(certified_language_result(coverage)))
}

#[cfg(any(test, feature = "local-search-ab"))]
#[allow(clippy::too_many_arguments)]
fn prove_conditional_transition(
    catalog: &GeometryCatalog,
    geometry: &super::placement_poset::CandidateGeometry,
    row_id: u32,
    operation: usize,
    executed: u16,
    state: &super::placement_poset::ConditionalState,
    profile: clearra_rules::kicks::KickTableProfileId,
    reachability: &mut ReachabilityWorkspace,
    movement_proofs: &mut CandidateMovementProofs,
) -> super::piece_order_language::ConditionalTransitionProof {
    use super::{
        piece_order_language::ConditionalTransitionProof::{Allowed, Forbidden, Unknown},
        placement_poset::{MovementProof, MovementProofQuery},
    };

    let board = state.board;
    let deleted = state.deleted;
    let lock_mask = geometry.conditional_lock_mask(operation, state);
    let mut realizations = catalog.instantiations(row_id, deleted).peekable();
    if realizations.peek().is_none() {
        return Forbidden;
    }
    if lock_mask.count_ones() != 4 {
        return Unknown;
    }
    if board & lock_mask != 0 || !placement_is_grounded(catalog.width(), board, lock_mask) {
        // The physical support predicate is over actual executed cells, not
        // one mandatory predecessor: floor, initial, or *any* real support.
        return if realizations.all(|lock| lock.lock_mask == lock_mask) {
            Forbidden
        } else {
            Unknown
        };
    }
    let (next_board, current_clears, _) =
        place_and_clear(catalog.width(), catalog.height(), board | lock_mask);
    let expected = geometry.conditional_after_operation(state, executed, operation);
    if next_board != expected.0
        || merge_deleted_rows(catalog.height(), deleted, current_clears) != Some(expected.1)
    {
        return Unknown;
    }
    let frame = LocalRelationRowFrame::new(catalog.height(), deleted);
    let piece = geometry.pieces()[operation];
    let mut unknown = false;
    for realization in realizations {
        if realization.lock_mask != lock_mask {
            unknown = true;
            continue;
        }
        if reachability.lock_harddrop_reachable_instantiated(catalog.width(), board, realization) {
            return Allowed;
        }
        if let Some((reachable, _)) = movement_proofs.answer(piece, realization, deleted, board) {
            if reachable {
                return Allowed;
            }
            continue;
        }
        let exact_positive = reachability.try_prove_first_kick_lock_interval(
            catalog,
            board,
            board,
            piece,
            profile,
            realization,
            frame,
        ) || reachability.proves_lock_for_interval(
            catalog,
            board,
            board,
            piece,
            profile,
            realization,
            frame,
        );
        if exact_positive {
            movement_proofs.insert(
                MovementProofQuery {
                    piece,
                    realization,
                    deleted,
                    minimum: board,
                    proof: MovementProof::Reachable { maximum: board },
                },
                MovementProofSource::ConditionalOrder,
            );
            return Allowed;
        }
        if reachability.try_prove_lock_pose_unreachable_at_minimum(
            catalog,
            board,
            piece,
            profile,
            realization,
            frame,
        ) {
            movement_proofs.insert(
                MovementProofQuery {
                    piece,
                    realization,
                    deleted,
                    minimum: board,
                    proof: MovementProof::Closed,
                },
                MovementProofSource::ConditionalOrder,
            );
        } else {
            unknown = true;
        }
    }
    if unknown {
        Unknown
    } else {
        Forbidden
    }
}

/// Exact, proof-gated language production before allocating a candidate's
/// physical-board subset projection. Failed proofs retain the ordinary exact
/// graph path. Only a complete poset language may reach the supply union.
#[cfg(any(test, feature = "local-search-ab"))]
fn verify_certified_poset(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    movement_proofs: &mut CandidateMovementProofs,
    control: &ExecutionControl,
) -> Result<Option<CandidateBuildResult>, WasmExactSearchError> {
    use super::placement_poset::MovementProof;

    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let Some(universe) = problem.piece_source().materialized_universe() else {
        return Ok(None);
    };
    if !StandardBagCoverage::supports(universe, problem.initial_hold()) {
        return Ok(None);
    }
    let mut algebraic_attempts = 0;
    let mut algebraic_guards = 0;
    let mut algebraic_row_steps = 0;
    let mut translation_attempts = 0;
    let mut translation_guards = 0;
    let mut translation_row_steps = 0;
    let mut movement_metrics = super::structural_order::MovementClearMetrics::default();
    let profile = problem.kick_profile().profile_id();
    if movement_proofs.enabled {
        workspace.reachability.configure_kick_profile(
            profile,
            problem.backend_policy().conditioned_reachability_enabled(),
        );
    }
    let poset = super::placement_poset::compile_with_movement_guard(
        catalog,
        candidate,
        movement_proofs.enabled,
        |query| {
            let super::placement_poset::MovementProofQuery {
                piece,
                realization,
                deleted,
                minimum,
                proof,
            } = query;
            let frame = LocalRelationRowFrame::new(catalog.height(), deleted);
            let MovementProof::Reachable { maximum } = proof else {
                movement_metrics.frame_pose_attempts += 1;
                let closed = workspace
                    .reachability
                    .try_prove_lock_pose_unreachable_at_minimum(
                        catalog,
                        minimum,
                        piece,
                        profile,
                        realization,
                        frame,
                    );
                if closed {
                    movement_metrics.frame_pose_proofs += 1;
                    movement_proofs.remember(query);
                }
                return closed;
            };
            let o_attempt = workspace.reachability.try_monotone_lock_interval(
                catalog,
                minimum,
                maximum,
                piece,
                profile,
                realization,
                frame,
            );
            // An admitted O proof already solves this translation subgraph;
            // do not repeat it. Other pieces use a positive-only witness.
            let (attempt, translation) = match o_attempt {
                Some(attempt) => (Some(attempt), false),
                None => (
                    workspace.reachability.try_translation_lock_interval(
                        catalog,
                        minimum,
                        maximum,
                        piece,
                        profile,
                        realization,
                        frame,
                    ),
                    true,
                ),
            };
            if let Some((proven, row_steps)) = attempt {
                algebraic_attempts += 1;
                algebraic_row_steps += row_steps;
                if translation {
                    translation_attempts += 1;
                    translation_row_steps += row_steps;
                    translation_guards += usize::from(proven);
                }
                if proven {
                    algebraic_guards += 1;
                    movement_proofs.remember(query);
                    return true;
                }
            }
            let proven = workspace.reachability.proves_lock_for_interval(
                catalog,
                minimum,
                maximum,
                piece,
                profile,
                realization,
                frame,
            );
            if proven {
                movement_proofs.remember(query);
            }
            proven
        },
    );
    // Include failed candidates and failed interval attempts in this cost.
    // Aggregate once per candidate, never an atomic per row/primitive state.
    super::structural_order::record_algebraic_intervals(
        algebraic_attempts,
        algebraic_guards,
        algebraic_row_steps,
    );
    super::structural_order::record_translation_intervals(
        translation_attempts,
        translation_guards,
        translation_row_steps,
    );
    super::structural_order::record_movement_clear(movement_metrics);
    let Some(poset) = poset else {
        return Ok(None);
    };
    // A bounded count-vector traversal can still be exponential for a general
    // poset. Budget exhaustion means Unknown, never a rejected candidate.
    let Some(language) = workspace.piece_order_languages.compile_poset_language(
        poset.pieces(),
        poset.predecessors(),
        4096,
    )?
    else {
        return Ok(None);
    };
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    let Some(coverage) = workspace.cover_standard_bag_language(problem, language.root, control)?
    else {
        return Ok(None);
    };
    super::structural_order::record_bypass(
        language.residual_states,
        poset
            .pieces()
            .len()
            .saturating_sub(language.operation_classes),
        poset
            .interval_guard_count()
            .saturating_sub(algebraic_guards),
    );
    Ok(Some(certified_language_result(Some(coverage))))
}

#[cfg(any(test, feature = "local-search-ab"))]
fn certified_language_result(coverage: Option<StandardBagCoverageResult>) -> CandidateBuildResult {
    let buildable = coverage
        .as_ref()
        .is_some_and(|value| value.covers_any_pattern);
    CandidateBuildResult {
        buildable,
        covered_patterns: None,
        symbolic_coverage_root: coverage
            .as_ref()
            .filter(|value| value.covers_any_pattern)
            .map(|value| value.root),
        observation_language_root: None,
        symbolic_covered_pattern_count: coverage
            .as_ref()
            .map_or(0, |value| value.covered_pattern_count),
        witness_pattern_id: coverage.as_ref().and_then(|value| value.witness_pattern_id),
        build_variant_count: 0,
        count_complete: true,
        representative_path: Vec::new(),
        graph_nodes: 0,
        coverage_product_words: 0,
        coverage_product_states: coverage.as_ref().map_or(0, |value| value.product_states),
        coverage_product_edge_checks: coverage.as_ref().map_or(0, |value| value.edge_checks),
        feasibility_states: 0,
        feasibility_rejected: false,
        reachability_states: 0,
        retained_bytes: 0,
        finesse_language: None,
    }
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
    movement_proofs: &CandidateMovementProofs,
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
        movement_proofs,
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
            #[cfg(any(test, feature = "local-search-ab"))]
            let mut canonical_language_id = graph.canonical_language_root;
            #[cfg(not(any(test, feature = "local-search-ab")))]
            let mut canonical_language_id = None;
            if problem
                .queue_observation_policy()
                .requires_observation_policy()
            {
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

// Witness verification reuses caller-owned scratch and policy inputs without allocation.
#[allow(clippy::too_many_arguments)]
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

// Witness search reuses caller-owned scratch and policy inputs without allocation.
#[allow(clippy::too_many_arguments)]
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
        if !workspace.reachability.lock_reachable_instantiated_in_frame(
            catalog,
            board,
            row.piece,
            realization,
            LocalRelationRowFrame::new(catalog.height(), deleted_rows),
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
            #[cfg(any(test, feature = "local-search-ab"))]
            canonical_language_root: None,
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

    // Graph construction keeps admission, projection, and reachability policy explicit.
    #[allow(clippy::too_many_arguments)]
    fn build(
        problem: &SearchProblem,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        workspace: &mut BuildUpWorkspace,
        piece_language_projection_only: bool,
        completion: BuildCompletion,
        feasibility: Option<RealizationFeasibility>,
        reachability_mode: BuildReachabilityMode,
        movement_proofs: &CandidateMovementProofs,
    ) -> Result<Self, WasmExactSearchError> {
        let legal_board_eligible = catalog.width() == 10
            && catalog.height() == 4
            && catalog.initial_board() == 0
            && matches!(completion, BuildCompletion::ClearToEmpty)
            && (workspace.legal_board.is_some() || cfg!(feature = "local-search-ab"))
            && crate::search_prune_policy::legal_board_enabled();
        Self::build_with_legal_board_admission(
            problem,
            catalog,
            candidate,
            projection,
            workspace,
            piece_language_projection_only,
            completion,
            feasibility,
            reachability_mode,
            legal_board_eligible,
            movement_proofs,
        )
    }

    // The normal entry point owns eligibility. Bounded tests can exercise the
    // admission control flow without a global registry or a qualified asset.
    #[allow(clippy::too_many_arguments)]
    fn build_with_legal_board_admission(
        problem: &SearchProblem,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        workspace: &mut BuildUpWorkspace,
        piece_language_projection_only: bool,
        completion: BuildCompletion,
        feasibility: Option<RealizationFeasibility>,
        reachability_mode: BuildReachabilityMode,
        legal_board_eligible: bool,
        movement_proofs: &CandidateMovementProofs,
    ) -> Result<Self, WasmExactSearchError> {
        #[cfg(any(test, feature = "local-search-ab"))]
        let states_before = movement_proofs
            .kick_guard_reuse_enabled
            .then(|| workspace.reachability.generated_state_count());
        let graph = Self::build_pass(
            problem,
            catalog,
            candidate,
            projection,
            workspace,
            piece_language_projection_only,
            completion,
            feasibility,
            reachability_mode,
            legal_board_eligible,
            movement_proofs,
        )?;
        #[cfg(not(any(test, feature = "local-search-ab")))]
        {
            graph.ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_exact_build_pass_witness_conflict",
            ))
        }
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            if let Some(graph) = graph {
                return Ok(graph);
            }
            let states_before = states_before.ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_exact_build_pass_witness_conflict",
            ))?;
            // A conflict may invalidate an earlier edge. Start one fresh
            // graph generation without any candidate proof shortcut.
            super::structural_order::record_ordered_kick_guard_reuse(
                super::structural_order::KickGuardReuseMetrics {
                    exact_retries: 1,
                    ..super::structural_order::KickGuardReuseMetrics::default()
                },
            );
            let mut graph = Self::build_pass(
                problem,
                catalog,
                candidate,
                projection,
                workspace,
                piece_language_projection_only,
                completion,
                feasibility,
                reachability_mode,
                legal_board_eligible,
                &CandidateMovementProofs::default(),
            )?
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_exact_build_pass_witness_conflict",
            ))?;
            graph.reachability_states = workspace
                .reachability
                .generated_state_count()
                .saturating_sub(states_before);
            Ok(graph)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_pass(
        problem: &SearchProblem,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        workspace: &mut BuildUpWorkspace,
        piece_language_projection_only: bool,
        completion: BuildCompletion,
        feasibility: Option<RealizationFeasibility>,
        reachability_mode: BuildReachabilityMode,
        legal_board_eligible: bool,
        _movement_proofs: &CandidateMovementProofs,
    ) -> Result<Option<Self>, WasmExactSearchError> {
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
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut accepting_reached = false;
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut movement_metrics = super::structural_order::MovementClearMetrics::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut first_kick_metrics = super::structural_order::KickIntervalMetrics::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut ordered_kick_metrics = super::structural_order::KickIntervalMetrics::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut conditional_metrics = super::structural_order::ConditionalOrderMetrics::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut kick_guards = CandidateKickGuardLedger::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut kick_guard_metrics = super::structural_order::KickGuardReuseMetrics::default();
        #[cfg(any(test, feature = "local-search-ab"))]
        let reuse_geometric_edges = geometric_edge_reuse::enabled();
        #[cfg(any(test, feature = "local-search-ab"))]
        let mut geometric_reuse_metrics = GeometricEdgeReuseMetrics::default();
        let mut subset_cursor = 0usize;
        #[allow(unused_labels)]
        'build_subsets: while subset_cursor < subset_queue.len() {
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
                #[cfg(any(test, feature = "local-search-ab"))]
                {
                    accepting_reached |= nodes[node_index].accepting;
                }
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
                if legal_board_eligible
                    && workspace.legal_board_child_is_rejected(child, graph_generation)
                {
                    continue;
                }
                // Reuse the stamped decision across all realizations and
                // predecessors. Unseen children stay unseen when no concrete
                // placement survives cheap physical/projection checks.
                let mut legal_admission_checked = !legal_board_eligible
                    || workspace.graph_reachable_generations[child] == graph_generation;
                let row_id = candidate.row_ids()[operation_index];
                let row = catalog.skeleton(row_id);
                // This memo owns only this candidate, subset, operation and
                // original deleted-row frame. It never stores an edge pose or
                // substitutes one realization for another.
                #[cfg(any(test, feature = "local-search-ab"))]
                let mut prepared_footprint = None;
                #[allow(unused_mut)]
                let mut prepare_edge =
                    |projection: &mut CandidateProjection,
                     realization: super::catalog::InstantiatedRealization,
                     needs_grounding: bool| {
                        #[cfg(any(test, feature = "local-search-ab"))]
                        if reuse_geometric_edges {
                            return geometric_build_edge_with_reuse(
                                catalog,
                                projection,
                                child,
                                operation_index,
                                row.piece,
                                board,
                                deleted_rows,
                                realization,
                                needs_grounding,
                                &mut prepared_footprint,
                                &mut geometric_reuse_metrics,
                            );
                        }
                        if needs_grounding
                            && !placement_is_grounded(catalog.width(), board, realization.lock_mask)
                        {
                            return None;
                        }
                        geometric_build_edge(
                            catalog,
                            projection,
                            child,
                            operation_index,
                            row.piece,
                            board,
                            deleted_rows,
                            realization,
                        )
                    };
                if reachability_mode == BuildReachabilityMode::GeometryOnly {
                    for realization in catalog.instantiations(row_id, deleted_rows) {
                        if let Some(edge) = prepare_edge(projection, realization, false) {
                            if !legal_admission_checked {
                                if !workspace.legal_board_child_allows(
                                    catalog,
                                    problem.kick_profile().profile_id(),
                                    projection,
                                    child,
                                    graph_generation,
                                ) {
                                    break;
                                }
                                legal_admission_checked = true;
                            }
                            try_push_build_edge(&mut edge_scratch, edge)?;
                        }
                    }
                } else if piece_language_projection_only {
                    let scratch_start = edge_scratch.len();
                    let mut harddrop_edge = None;
                    #[cfg(any(test, feature = "local-search-ab"))]
                    // Retain the source until all hard-drop alternatives have
                    // been considered, so the experiments count actual use.
                    let mut certified_edge = None;
                    for realization in catalog.instantiations(row_id, deleted_rows) {
                        // A normal lock must touch the floor or an occupied
                        // cell directly below it. This is the same necessary
                        // physical predicate used by exact reachability; it
                        // does not apply to GeometryOnly/finesse candidates.
                        let Some(edge) = prepare_edge(projection, realization, true) else {
                            continue;
                        };
                        if !legal_admission_checked {
                            if !workspace.legal_board_child_allows(
                                catalog,
                                problem.kick_profile().profile_id(),
                                projection,
                                child,
                                graph_generation,
                            ) {
                                break;
                            }
                            legal_admission_checked = true;
                        }
                        try_push_build_edge(&mut edge_scratch, edge)?;
                        if workspace.reachability.lock_harddrop_reachable_instantiated(
                            catalog.width(),
                            board,
                            realization,
                        ) {
                            harddrop_edge = Some(edge);
                            break;
                        }
                        #[cfg(any(test, feature = "local-search-ab"))]
                        if (_movement_proofs.enabled
                            || _movement_proofs.first_kick_enabled
                            || _movement_proofs.ordered_kick_enabled
                            || _movement_proofs.len != 0)
                            && certified_edge.is_none()
                        {
                            match _movement_proofs.answer(
                                row.piece,
                                realization,
                                deleted_rows,
                                board,
                            ) {
                                Some((true, source)) => {
                                    match source {
                                        MovementProofSource::Poset => {
                                            movement_metrics.reused_positive_locks += 1;
                                        }
                                        MovementProofSource::ConditionalOrder => {
                                            conditional_metrics.reused_pose_proofs += 1;
                                        }
                                    }
                                    certified_edge = Some((edge, PositiveEdgeProof::Reused));
                                }
                                Some((false, source)) => {
                                    match source {
                                        MovementProofSource::Poset => {
                                            movement_metrics.reused_closed_locks += 1;
                                        }
                                        MovementProofSource::ConditionalOrder => {
                                            conditional_metrics.reused_pose_proofs += 1;
                                        }
                                    }
                                    let _ = edge_scratch.pop();
                                }
                                None => {
                                    let frame =
                                        LocalRelationRowFrame::new(catalog.height(), deleted_rows);
                                    let profile = problem.kick_profile().profile_id();
                                    if let Some(frame) = frame {
                                        use super::reachability::KickWitnessOrder;
                                        for (enabled, order, metrics) in [
                                            (
                                                _movement_proofs.first_kick_enabled,
                                                KickWitnessOrder::RawFirst,
                                                &mut first_kick_metrics,
                                            ),
                                            (
                                                _movement_proofs.ordered_kick_enabled,
                                                KickWitnessOrder::LaterOnly,
                                                &mut ordered_kick_metrics,
                                            ),
                                        ] {
                                            if !enabled {
                                                continue;
                                            }
                                            let proof = try_kick_interval_edge(
                                                &mut workspace.reachability,
                                                catalog,
                                                board,
                                                row.piece,
                                                profile,
                                                realization,
                                                frame,
                                                order,
                                                _movement_proofs,
                                                &mut kick_guards,
                                                metrics,
                                                &mut kick_guard_metrics,
                                            );
                                            if kick_guards.conflicted {
                                                break 'build_subsets;
                                            }
                                            if let Some(proof) = proof {
                                                certified_edge = Some((edge, proof));
                                                break;
                                            }
                                        }
                                    }
                                    if certified_edge.is_some() {
                                        continue;
                                    }
                                    if _movement_proofs.enabled && deleted_rows != 0 {
                                        movement_metrics.point_pose_attempts += 1;
                                        if workspace
                                            .reachability
                                            .try_prove_lock_pose_unreachable_at_minimum(
                                                catalog,
                                                board,
                                                row.piece,
                                                profile,
                                                realization,
                                                frame,
                                            )
                                        {
                                            movement_metrics.point_pose_proofs += 1;
                                            let _ = edge_scratch.pop();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Keep a valid hard-drop edge preferred. Otherwise a
                    // positive proof can supply this Boolean operation edge.
                    #[cfg(any(test, feature = "local-search-ab"))]
                    let harddrop_edge = harddrop_edge.or_else(|| {
                        certified_edge.map(|(edge, proof)| {
                            match proof {
                                PositiveEdgeProof::Reused => {}
                                PositiveEdgeProof::FirstKick => {
                                    first_kick_metrics.selected_locks += 1;
                                }
                                PositiveEdgeProof::OrderedKick => {
                                    ordered_kick_metrics.selected_locks += 1;
                                }
                                PositiveEdgeProof::ReusedKickGuard => {
                                    kick_guard_metrics.reused_selected += 1;
                                }
                            }
                            edge
                        })
                    });
                    let selected = if let Some(edge) = harddrop_edge {
                        Some(edge)
                    } else if edge_scratch.len() == scratch_start {
                        // No grounded concrete edge (or verified negative):
                        // do not initialize a template merely to visit none.
                        None
                    } else {
                        workspace.reachability.prepare_template(catalog, row.piece);
                        let mut reachable = None;
                        for edge in edge_scratch[scratch_start..].iter().copied() {
                            if workspace
                                .reachability
                                .lock_reachable_after_harddrop_miss_in_frame(
                                    catalog,
                                    board,
                                    row.piece,
                                    edge.rotation,
                                    edge.x,
                                    edge.y,
                                    LocalRelationRowFrame::new(catalog.height(), deleted_rows),
                                )
                            {
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
                        let Some(edge) = prepare_edge(projection, realization, true) else {
                            continue;
                        };
                        if !legal_admission_checked {
                            if !workspace.legal_board_child_allows(
                                catalog,
                                problem.kick_profile().profile_id(),
                                projection,
                                child,
                                graph_generation,
                            ) {
                                break;
                            }
                            legal_admission_checked = true;
                        }
                        if workspace.reachability.lock_reachable_instantiated_in_frame(
                            catalog,
                            board,
                            row.piece,
                            realization,
                            LocalRelationRowFrame::new(catalog.height(), deleted_rows),
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
                if workspace.graph_reachable_generations[child_subset] != graph_generation
                    || workspace.graph_subset_node_ids[child_subset] == LEGAL_BOARD_PENDING_NODE
                {
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
        #[cfg(any(test, feature = "local-search-ab"))]
        super::structural_order::record_movement_clear(movement_metrics);
        #[cfg(any(test, feature = "local-search-ab"))]
        super::structural_order::record_first_kick_interval(first_kick_metrics);
        #[cfg(any(test, feature = "local-search-ab"))]
        super::structural_order::record_ordered_kick_interval(ordered_kick_metrics);
        #[cfg(any(test, feature = "local-search-ab"))]
        super::structural_order::record_conditional_order(conditional_metrics);
        #[cfg(any(test, feature = "local-search-ab"))]
        geometric_edge_reuse::record(geometric_reuse_metrics);
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            if kick_guards.conflicted {
                // Recycle un-published graph scratch, but keep only exact
                // template/reachability caches and exact physical projection.
                workspace.graph_nodes = nodes;
                workspace.graph_edges = edges;
                workspace.graph_piece_edges = piece_edges;
                workspace.graph_subset_queue = subset_queue;
                workspace.graph_edge_scratch = edge_scratch;
                kick_guard_metrics.conflicts += 1;
                // Reuses in an aborted pass are not selected output edges.
                kick_guard_metrics.reused_selected = 0;
                super::structural_order::record_ordered_kick_guard_reuse(kick_guard_metrics);
                return Ok(None);
            }
            super::structural_order::record_ordered_kick_guard_reuse(kick_guard_metrics);
        }
        #[cfg(any(test, feature = "local-search-ab"))]
        let fuse_language = super::placement_dependency::enabled()
            && piece_language_projection_only
            && problem.count_policy() == PcCountPolicy::CountUnique
            && reachability_mode == BuildReachabilityMode::Existing;
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let fuse_language = false;
        if !fuse_language {
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
        }
        edge_scratch.clear();
        workspace.graph_edge_scratch = edge_scratch;
        subset_queue.clear();
        workspace.graph_subset_queue = subset_queue;
        let graph = Self {
            nodes,
            edges,
            piece_edges,
            piece_edges_share_operation_edges: share_piece_edges,
            root: 0,
            reachability_states: workspace
                .reachability
                .generated_state_count()
                .saturating_sub(reachability_states_before),
            #[cfg(any(test, feature = "local-search-ab"))]
            canonical_language_root: None,
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        let graph = if fuse_language && accepting_reached {
            let mut graph = graph;
            super::placement_dependency::record(|metrics| {
                metrics.fused_language_graphs += 1;
                metrics.fused_language_nodes += graph.nodes.len() as u64;
                metrics.fused_language_edges += if graph.piece_edges_share_operation_edges {
                    graph.edges.len()
                } else {
                    graph.piece_edges.len()
                } as u64;
            });
            graph.canonical_language_root = match workspace
                .piece_order_languages
                .canonicalize_and_mark_live(&mut graph)
            {
                Ok(root) => root,
                Err(error) => {
                    workspace.recycle_graph(graph);
                    return Err(error);
                }
            };
            graph
        } else {
            if fuse_language {
                // Every node starts nonlive, and only accepting terminals can
                // seed a live suffix. With no such terminal, the graph already
                // denotes the empty language; leave its root ID unset without
                // initializing classes or visiting edges again.
                super::placement_dependency::record(|metrics| {
                    metrics.skipped_dead_language_graphs += 1;
                    metrics.skipped_dead_language_nodes += graph.nodes.len() as u64;
                });
            }
            graph
        };
        Ok(Some(graph))
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
    #[cfg(any(test, feature = "local-search-ab"))]
    {
        // Finesse may replace the edge set, invalidating any earlier suffix ID.
        graph.canonical_language_root = None;
    }
    let target_grouping_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseTargetGrouping);
    let mut subsets = vec![usize::MAX; graph.nodes.len()];
    subsets[graph.root as usize] = 0;
    // Child entries are populated while earlier indices are traversed, so an iterator
    // borrow would incorrectly cover the same mutable work array for the whole loop.
    #[allow(clippy::needless_range_loop)]
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
    for (node_index, &subset) in subsets.iter().enumerate().take(graph.nodes.len()) {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
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
    workspace.reachability.configure_kick_profile(
        problem.kick_profile().profile_id(),
        problem.backend_policy().conditioned_reachability_enabled(),
    );
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
        &CandidateMovementProofs::default(),
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
    let (cleared_lines, _) =
        projected_line_clear(catalog, projection, child, board | lock_mask, deleted_rows);
    Some(instantiated_geometric_edge(
        child,
        operation_index,
        piece,
        realization,
        cleared_lines?,
    ))
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy)]
struct PreparedGeometricFootprint {
    lock_mask: u64,
    needs_grounding: bool,
    // None is the same failed geometry check as for the uncached pose.
    cleared_lines: Option<u8>,
}

#[cfg(any(test, feature = "local-search-ab"))]
#[allow(clippy::too_many_arguments)]
fn geometric_build_edge_with_reuse(
    catalog: &GeometryCatalog,
    projection: &mut CandidateProjection,
    child: usize,
    operation_index: usize,
    piece: PieceKind,
    board: u64,
    deleted_rows: u16,
    realization: super::catalog::InstantiatedRealization,
    needs_grounding: bool,
    prepared: &mut Option<PreparedGeometricFootprint>,
    metrics: &mut GeometricEdgeReuseMetrics,
) -> Option<BuildEdge> {
    let lock_mask = realization.lock_mask;
    let cleared_lines = match prepared {
        Some(previous)
            if previous.lock_mask == lock_mask && previous.needs_grounding == needs_grounding =>
        {
            metrics.reused_validations += 1;
            previous.cleared_lines
        }
        _ => {
            metrics.footprint_validations += 1;
            let cleared_lines = if (!needs_grounding
                || placement_is_grounded(catalog.width(), board, lock_mask))
                && board & lock_mask == 0
            {
                metrics.line_clear_computations += 1;
                let (lines, projection_checked) = projected_line_clear(
                    catalog,
                    projection,
                    child,
                    board | lock_mask,
                    deleted_rows,
                );
                metrics.projection_validations += u64::from(projection_checked);
                lines
            } else {
                None
            };
            // A different footprint, even for the same row, is not a hit.
            // Validate it exactly without replacing the one common fact.
            if prepared.is_none() {
                *prepared = Some(PreparedGeometricFootprint {
                    lock_mask,
                    needs_grounding,
                    cleared_lines,
                });
            }
            cleared_lines
        }
    }?;
    Some(instantiated_geometric_edge(
        child,
        operation_index,
        piece,
        realization,
        cleared_lines,
    ))
}

fn projected_line_clear(
    catalog: &GeometryCatalog,
    projection: &mut CandidateProjection,
    child: usize,
    placed_board: u64,
    deleted_rows: u16,
) -> (Option<u8>, bool) {
    let (next_board, cleared_current, cleared_lines) =
        place_and_clear(catalog.width(), catalog.height(), placed_board);
    let Some(next_deleted_rows) =
        merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)
    else {
        return (None, false);
    };
    let (expected_board, expected_deleted_rows) = projection.state(child);
    (
        (next_deleted_rows == expected_deleted_rows && next_board == expected_board)
            .then_some(cleared_lines),
        true,
    )
}

fn instantiated_geometric_edge(
    child: usize,
    operation_index: usize,
    piece: PieceKind,
    realization: super::catalog::InstantiatedRealization,
    cleared_lines: u8,
) -> BuildEdge {
    BuildEdge {
        to: child as u32,
        operation_index: operation_index as u8,
        piece,
        rotation: realization.rotation,
        x: realization.x,
        y: realization.lock_y,
        cleared_lines,
    }
}

// Path reconstruction consumes the same explicit supply-policy dimensions as search.
#[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::too_many_arguments)]
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

// Supply-state transition inputs remain scalar on this allocation-sensitive path.
#[allow(clippy::too_many_arguments)]
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

pub(super) fn compact_target_board(width: u8, height: u8, board: u64, deleted_rows: u16) -> u64 {
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
mod kick_guard_reuse_tests {
    use super::*;
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
    use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs};

    use super::super::{placement_dependency, structural_order};

    const BOARD: u64 = 0x3f0f_83f4;
    const PLACEMENTS: [(PieceKind, u64); 3] = [
        (PieceKind::I, 0x00f0_0000),
        (PieceKind::O, 0x0c03),
        (PieceKind::T, 0x7008),
    ];

    fn problem() -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(3, BOARD),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(3),
        )
        .with_rule(srs())
        .with_allow_hold(false)
        .with_exact_pieces(Some(3))
        .with_supply_window_size(SupplyWindowSize::new(3))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(1)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false)
                .with_max_memory_mib(Some(256)),
        );
        ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::CoverageSummary)
    }

    #[test]
    fn candidate_kick_guard_reuse_replaces_raw_attempts_without_changing_coverage() {
        let problem = problem();
        let run = |first, reuse| {
            structural_order::with_ordered_kick_guard_reuse_mode(reuse, || {
                structural_order::with_ordered_kick_interval_mode(false, || {
                    structural_order::with_first_kick_interval_mode(first, || {
                        structural_order::with_conditional_order_mode(false, || {
                            structural_order::with_movement_clear_mode(false, || {
                                structural_order::with_mode(false, || {
                                    placement_dependency::with_mode(false, || {
                                        crate::WasmCpuSearchBackend::execute_with_control(
                                            &problem,
                                            &ExecutionControl::default(),
                                        )
                                        .unwrap()
                                    })
                                    .0
                                })
                            })
                        })
                    })
                })
            })
        };
        let (_, reuse_alone) = run(false, true);
        let (baseline, before) = run(true, false);
        let (candidate, after) = run(true, true);
        assert_eq!(reuse_alone.ordered_kick_guard_reuse, Default::default());
        assert_eq!(before.ordered_kick_guard_reuse, Default::default());
        assert_eq!(after.ordered_kick_interval, Default::default());
        assert_eq!(
            baseline.coverage_pattern_words(),
            candidate.coverage_pattern_words()
        );
        assert!(!baseline.solution_coverages().is_empty());
        assert_eq!(
            baseline.solution_coverages(),
            candidate.solution_coverages()
        );
        let guard = after.ordered_kick_guard_reuse;
        assert!(guard.constructed >= guard.stored && guard.stored > 0);
        assert!(guard.reused_selected > 0);
        assert!(after.first_kick_interval.attempts < before.first_kick_interval.attempts);
        assert_eq!(guard.conflicts, 0);
        assert_eq!(guard.exact_retries, 0);
    }

    #[test]
    fn candidate_kick_guard_conflict_discards_the_provisional_graph_and_retries_once() {
        use super::super::{
            placement_poset::{MovementProof, MovementProofQuery},
            reachability::{KickWitnessGuard, KickWitnessOrder},
        };

        let profile = KickTableProfileId::Srs90;
        let problem = problem();
        let required = PLACEMENTS.iter().fold(0, |mask, &(_, cells)| mask | cells);
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 3, BOARD, required)
                .unwrap();
        let row_ids = PLACEMENTS.map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &row_ids).unwrap();
        let realization = catalog
            .instantiations(row_ids[2], 0)
            .find(|pose| pose.rotation == RotationState::Two)
            .unwrap();
        let mut checker = ReachabilityWorkspace::default();
        checker.configure_kick_profile(profile, false);
        let frame = LocalRelationRowFrame::new(3, 0);
        let guard = checker
            .try_kick_witness_guard(
                &catalog,
                BOARD,
                BOARD,
                PieceKind::T,
                profile,
                realization,
                frame,
                KickWitnessOrder::RawFirst,
            )
            .unwrap();
        let later_board = BOARD | PLACEMENTS[1].1;
        assert_eq!(guard.minimum & !later_board, 0);
        assert_eq!(later_board & !guard.maximum, 0);

        let mut ledger = CandidateKickGuardLedger::default();
        let key = KickGuardKey {
            piece: PieceKind::T,
            realization,
            deleted: 0,
            order: KickWitnessOrder::RawFirst,
        };
        assert!(ledger.remember(
            KickGuardEntry { key, guard },
            &CandidateMovementProofs::default(),
            BOARD,
        ));
        assert!(ledger.proves(key, later_board));
        assert!(!ledger.proves(KickGuardKey { deleted: 1, ..key }, later_board));
        assert!(!ledger.proves(
            KickGuardKey {
                order: KickWitnessOrder::LaterOnly,
                ..key
            },
            later_board
        ));
        assert!(!ledger.proves(key, guard.maximum | realization.lock_mask));
        assert!(!ledger.remember(
            KickGuardEntry {
                key,
                guard: KickWitnessGuard {
                    minimum: !BOARD,
                    maximum: BOARD
                }
            },
            &CandidateMovementProofs::default(),
            BOARD,
        ));
        assert!(ledger.conflicted && !ledger.proves(key, later_board));

        let mut disputed = CandidateMovementProofs {
            first_kick_enabled: true,
            kick_guard_reuse_enabled: true,
            ..CandidateMovementProofs::default()
        };
        // Inject an intentionally invalid, previously retained Closed fact
        // whose minimum does not match the initial point but intersects the
        // later board interval. Detect it when first constructing the guard,
        // discard any partially built edges, and rerun the ordinary graph.
        disputed.insert(
            MovementProofQuery {
                piece: PieceKind::T,
                realization,
                deleted: 0,
                minimum: PLACEMENTS[1].1,
                proof: MovementProof::Closed,
            },
            MovementProofSource::ConditionalOrder,
        );
        let run = |proofs| {
            structural_order::with_mode(false, || {
                placement_dependency::with_mode(false, || {
                    let mut workspace = BuildUpWorkspace::default();
                    workspace
                        .reachability
                        .configure_kick_profile(profile, false);
                    let mut projection = CandidateProjection::compile(
                        &catalog,
                        &candidate,
                        &mut workspace,
                        BuildCompletion::ClearToEmpty,
                    )
                    .unwrap();
                    let graph = BuildOrderGraph::build_with_legal_board_admission(
                        &problem,
                        &catalog,
                        &candidate,
                        &mut projection,
                        &mut workspace,
                        true,
                        BuildCompletion::ClearToEmpty,
                        None,
                        BuildReachabilityMode::Existing,
                        false,
                        proofs,
                    )
                    .unwrap();
                    assert!(graph.is_live());
                    (graph.nodes.len(), graph.edges)
                })
                .0
            })
        };
        let baseline = CandidateMovementProofs::default();
        let (expected, _) = run(&baseline);
        let (actual, observed) = run(&disputed);
        assert_eq!(actual, expected);
        let metrics = observed.ordered_kick_guard_reuse;
        assert_eq!(metrics.conflicts, 1);
        assert_eq!(metrics.exact_retries, 1);
        assert_eq!(metrics.reused_selected, 0);
    }
}

#[cfg(test)]
mod v081_legal_child_admission_tests {
    use super::*;
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcQueueInput};
    use clearra_problem::ProblemCompiler;
    use clearra_rules::kicks::KickTableProfileId;

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    fn fixture() -> (GeometryCatalog, GeometryCandidate) {
        o_fixture(10, 4)
    }

    fn o_fixture(width: u8, height: u8) -> (GeometryCatalog, GeometryCandidate) {
        let full = (1_u64 << (u32::from(width) * u32::from(height))) - 1;
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(width, height, 0, full)
                .unwrap();
        let mut rows = Vec::new();
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                let cells = 3_u64 << (y * width + x) | 3_u64 << ((y + 1) * width + x);
                rows.push(catalog.skeleton_id(PieceKind::O, cells).unwrap());
            }
        }
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        (catalog, candidate)
    }

    fn horizontal_i_fixture() -> (GeometryCatalog, GeometryCandidate) {
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, (1 << 16) - 1)
                .unwrap();
        let rows = (0..4)
            .map(|y| catalog.skeleton_id(PieceKind::I, 15 << (y * 4)).unwrap())
            .collect::<Vec<_>>();
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        (catalog, candidate)
    }

    fn problem_for(profile: KickTableProfileId) -> SearchProblem {
        use clearra_rules::profile::builtin_rules;
        let rule = match profile {
            KickTableProfileId::Srs90 => builtin_rules::srs(),
            KickTableProfileId::SrsPlus => builtin_rules::srs_plus(),
            KickTableProfileId::SrsX => builtin_rules::srs_x(),
            KickTableProfileId::Jstris180 => builtin_rules::jstris_180(),
            KickTableProfileId::NoKick => builtin_rules::no_kick(),
            _ => unreachable!(),
        };
        ProblemCompiler::compile_opening_pc(
            &OpeningPcSearchQuery::new(PcTarget::four_lines())
                .with_queue(PcQueueInput::standard_7_bag())
                .with_rule(rule),
        )
        .unwrap()
    }

    // Deliberately independent cell-wise physics; no product collision,
    // compiled transition, or harddrop-path helper is reused here.
    fn primitive_grounded(width: u8, board: u64, mut placement: u64) -> bool {
        while placement != 0 {
            let cell = placement.trailing_zeros();
            placement &= placement - 1;
            if cell < u32::from(width) || board & (1_u64 << (cell - u32::from(width))) != 0 {
                return true;
            }
        }
        false
    }

    fn primitive_harddrop(width: u8, height: u8, board: u64, placement: u64) -> bool {
        primitive_grounded(width, board, placement)
            && (1..=height).all(|distance| {
                board & (placement << (u32::from(distance) * u32::from(width))) == 0
            })
    }

    #[test]
    fn geometric_edge_reuse_matches_exhaustive_2l_and_4l_scalar_row_facts() {
        for height in [2_u8, 4] {
            let (catalog, candidate) = o_fixture(4, height);
            let operation_cells: Vec<_> = candidate
                .row_ids()
                .iter()
                .map(|&id| catalog.skeleton(id).cells)
                .collect();
            let occupied = |subset: usize| {
                operation_cells
                    .iter()
                    .enumerate()
                    .fold(0_u64, |board, (i, &cells)| {
                        if subset & (1 << i) != 0 {
                            board | cells
                        } else {
                            board
                        }
                    })
            };
            let complete = |board: u64| {
                (0..height).fold(0_u16, |deleted, row| {
                    deleted | (u16::from((board >> (row * 4)) & 15 == 15) << row)
                })
            };
            let compact = |board: u64, removed: u16| {
                let mut compacted = 0;
                let mut destination = 0;
                for row in 0..height {
                    if removed & (1 << row) == 0 {
                        compacted |= ((board >> (row * 4)) & 15) << destination;
                        destination += 4;
                    }
                }
                compacted
            };
            let mut workspace = BuildUpWorkspace::default();
            let mut projection = CandidateProjection::compile(
                &catalog,
                &candidate,
                &mut workspace,
                BuildCompletion::ClearToEmpty,
            )
            .unwrap();
            let mut total_reused = 0;
            let mut checked_two_line_clears = 0;
            let mut rejected_floating = 0;
            for subset in 0..(1 << operation_cells.len()) {
                let deleted_rows = complete(occupied(subset));
                let board = compact(occupied(subset), deleted_rows);
                assert_eq!(projection.state(subset), (board, deleted_rows));
                for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
                    let op_bit = 1 << operation;
                    if subset & op_bit != 0 {
                        continue;
                    }
                    let child = subset | op_bit;
                    let child_deleted = complete(occupied(child));
                    assert_eq!(
                        projection.state(child),
                        (compact(occupied(child), child_deleted), child_deleted),
                    );
                    let differing = (child_deleted & !deleted_rows).count_ones() as u8;
                    let physical = compact(operation_cells[operation], deleted_rows);
                    let grounded = primitive_grounded(4, board, physical);
                    for needs_grounding in [false, true] {
                        let mut prepared = None;
                        let mut metrics = GeometricEdgeReuseMetrics::default();
                        let mut poses = 0;
                        for realization in catalog.instantiations(row_id, deleted_rows) {
                            assert_eq!(realization.lock_mask, physical);
                            let edge = geometric_build_edge_with_reuse(
                                &catalog,
                                &mut projection,
                                child,
                                operation,
                                PieceKind::O,
                                board,
                                deleted_rows,
                                realization,
                                needs_grounding,
                                &mut prepared,
                                &mut metrics,
                            );
                            let expected = if needs_grounding && !grounded {
                                None
                            } else {
                                Some(differing)
                            };
                            assert_eq!(edge.as_ref().map(|edge| edge.cleared_lines), expected);
                            if let Some(edge) = edge {
                                assert_eq!(edge.rotation, realization.rotation);
                                assert_eq!((edge.x, edge.y), (realization.x, realization.lock_y));
                            }
                            poses += 1;
                        }
                        if poses != 0 {
                            assert_eq!(metrics.footprint_validations, 1);
                            assert_eq!(metrics.reused_validations, poses - 1);
                            let checked = u64::from(!needs_grounding || grounded);
                            assert_eq!(metrics.line_clear_computations, checked);
                            assert_eq!(metrics.projection_validations, checked);
                            total_reused += metrics.reused_validations;
                            checked_two_line_clears += u64::from(differing == 2) * checked;
                            rejected_floating += u64::from(needs_grounding && !grounded);
                        }
                    }
                }
            }
            assert!(
                total_reused > 0,
                "{height}L must visit several poses per footprint"
            );
            assert!(
                checked_two_line_clears > 0,
                "{height}L must clear two rows atomically"
            );
            if height == 4 {
                assert!(rejected_floating > 0);
            }
        }
    }

    #[test]
    fn geometric_edge_reuse_preserves_all_pose_edges_and_rejects_mask_mismatch() {
        let profile = KickTableProfileId::Srs90;
        let problem = problem_for(profile);
        for height in [2, 4] {
            let (catalog, candidate) = o_fixture(4, height);
            for (mode, unique) in [
                (BuildReachabilityMode::Existing, true),
                (BuildReachabilityMode::Existing, false),
                (BuildReachabilityMode::GeometryOnly, false),
            ] {
                let run = |enabled| {
                    geometric_edge_reuse::with_mode(enabled, || {
                        let mut workspace = BuildUpWorkspace::default();
                        workspace
                            .reachability
                            .configure_kick_profile(profile, false);
                        let mut projection = CandidateProjection::compile(
                            &catalog,
                            &candidate,
                            &mut workspace,
                            BuildCompletion::ClearToEmpty,
                        )
                        .unwrap();
                        let graph = BuildOrderGraph::build_with_legal_board_admission(
                            &problem,
                            &catalog,
                            &candidate,
                            &mut projection,
                            &mut workspace,
                            unique,
                            BuildCompletion::ClearToEmpty,
                            None,
                            mode,
                            false,
                            &CandidateMovementProofs::default(),
                        )
                        .unwrap();
                        assert!(graph.is_live());
                        let nodes: Vec<_> = graph
                            .nodes
                            .iter()
                            .map(|node| {
                                (
                                    node.depth,
                                    node.live,
                                    node.accepting,
                                    node.edge_count,
                                    node.piece_edge_count,
                                )
                            })
                            .collect();
                        (
                            nodes,
                            graph.edges,
                            graph.piece_edges,
                            graph.reachability_states,
                        )
                    })
                };
                let (baseline, off) = run(false);
                let (actual, on) = run(true);
                assert_eq!(actual, baseline, "height={height} {mode:?} unique={unique}");
                assert_eq!(off, GeometricEdgeReuseMetrics::default());
                assert!(on.footprint_validations > 0);
                if !unique {
                    assert!(on.reused_validations > 0);
                }
            }
            let mut workspace = BuildUpWorkspace::default();
            let mut projection = CandidateProjection::compile(
                &catalog,
                &candidate,
                &mut workspace,
                BuildCompletion::ClearToEmpty,
            )
            .unwrap();
            let realization = catalog
                .instantiations(candidate.row_ids()[0], 0)
                .next()
                .unwrap();
            let unrelated = catalog
                .instantiations(candidate.row_ids()[1], 0)
                .next()
                .unwrap();
            let mut facts = None;
            let mut metrics = GeometricEdgeReuseMetrics::default();
            let mut prepare = |pose| {
                geometric_build_edge_with_reuse(
                    &catalog,
                    &mut projection,
                    1,
                    0,
                    PieceKind::O,
                    0,
                    0,
                    pose,
                    false,
                    &mut facts,
                    &mut metrics,
                )
            };
            assert!(prepare(realization).is_some());
            assert!(
                prepare(unrelated).is_none(),
                "another footprint cannot validate this child"
            );
            assert!(prepare(realization).is_some());
            assert_eq!(metrics.footprint_validations, 2);
            assert_eq!(metrics.reused_validations, 1);
        }
    }

    #[test]
    fn v081_legal_child_admission_uses_existing_generation_storage_without_claiming_reachability() {
        let (catalog, candidate) = fixture();
        let mut workspace = BuildUpWorkspace::default();
        let mut projection = CandidateProjection::compile(
            &catalog,
            &candidate,
            &mut workspace,
            BuildCompletion::ClearToEmpty,
        )
        .unwrap();
        let generation = workspace
            .begin_graph_generation(projection.state_count())
            .unwrap();
        let table_bytes = workspace.graph_reachable_generations.capacity() * 4
            + workspace.graph_subset_node_ids.capacity() * 4;
        let profile = clearra_rules::kicks::KickTableProfileId::SrsPlus;
        workspace.legal_board_query_cache.reset(profile);
        assert!(workspace.legal_board_child_allows(
            &catalog,
            profile,
            &mut projection,
            1,
            generation,
        ));
        assert_eq!(workspace.graph_subset_node_ids[1], LEGAL_BOARD_PENDING_NODE);
        assert!(!workspace.legal_board_child_is_rejected(1, generation));
        assert!(workspace.graph_nodes.is_empty());
        assert!(workspace.graph_edges.is_empty());
        assert_eq!(workspace.reachability.generated_state_count(), 0);
        assert_eq!(
            workspace.graph_reachable_generations.capacity() * 4
                + workspace.graph_subset_node_ids.capacity() * 4,
            table_bytes
        );

        // The actual edge verifier, not admission, assigns a concrete node.
        workspace.graph_subset_node_ids[1] = 7;
        assert!(workspace.legal_board_child_allows(
            &catalog,
            profile,
            &mut projection,
            1,
            generation,
        ));
        assert_eq!(workspace.graph_subset_node_ids[1], 7);
    }

    #[test]
    fn v081_legal_child_negative_is_counted_once_and_cannot_cross_candidate_generations() {
        let (catalog, candidate) = fixture();
        let mut workspace = BuildUpWorkspace::default();
        let mut projection = CandidateProjection::compile(
            &catalog,
            &candidate,
            &mut workspace,
            BuildCompletion::ClearToEmpty,
        )
        .unwrap();
        let profile = clearra_rules::kicks::KickTableProfileId::SrsPlus;
        workspace.legal_board_query_cache.reset(profile);
        let generation = workspace
            .begin_graph_generation(projection.state_count())
            .unwrap();
        let (board, deleted_rows) = projection.state(1);
        // Only the memo's stamped control flow is under test; this synthetic
        // closure is not a production asset or a completeness certificate.
        assert!(!workspace.legal_board_query_cache.allows(
            LegalBoardQuery {
                width: 10,
                height: 4,
                initial_board: 0,
                kick_profile: profile,
                physical_board: board,
                deleted_original_rows: deleted_rows,
                placed_piece_count: 1,
                completion: CompletionCapability::ClearToEmpty,
            },
            || false,
        ));
        for _ in 0..2 {
            assert!(!workspace.legal_board_child_allows(
                &catalog,
                profile,
                &mut projection,
                1,
                generation,
            ));
        }
        assert_eq!(
            workspace.graph_subset_node_ids[1],
            LEGAL_BOARD_REJECTED_NODE
        );
        assert_eq!(workspace.legal_board_verified_negative_prunes(), 1);
        assert!(workspace.legal_board_child_is_rejected(1, generation));
        assert!(!workspace.legal_board_child_is_rejected(1, generation.wrapping_add(1)));
        assert_eq!(workspace.reachability.generated_state_count(), 0);
        workspace.legal_board_query_cache.reset(profile);
        let next_generation = workspace
            .begin_graph_generation(projection.state_count())
            .unwrap();
        assert!(workspace.legal_board_child_allows(
            &catalog,
            profile,
            &mut projection,
            1,
            next_generation,
        ));
        assert_eq!(workspace.graph_subset_node_ids[1], LEGAL_BOARD_PENDING_NODE);
        assert_eq!(workspace.legal_board_verified_negative_prunes(), 1);
    }

    #[test]
    fn v081_legal_admission_does_not_query_floating_children_but_keeps_geometry_only() {
        let (catalog, candidate) = fixture();
        let problem = problem_for(KickTableProfileId::SrsPlus);
        let floating_operation = candidate
            .row_ids()
            .iter()
            .position(|&row| catalog.skeleton(row).cells & full_row_mask(10) == 0)
            .unwrap();
        let grounded_operation = candidate
            .row_ids()
            .iter()
            .position(|&row| catalog.skeleton(row).cells & full_row_mask(10) != 0)
            .unwrap();
        for (mode, unique) in [
            (BuildReachabilityMode::Existing, true),
            (BuildReachabilityMode::Existing, false),
            (BuildReachabilityMode::GeometryOnly, true),
        ] {
            let mut workspace = BuildUpWorkspace::default();
            workspace
                .legal_board_query_cache
                .reset(KickTableProfileId::SrsPlus);
            let mut projection = CandidateProjection::compile(
                &catalog,
                &candidate,
                &mut workspace,
                BuildCompletion::ClearToEmpty,
            )
            .unwrap();
            // This is a deterministic scheduling seam, not asset qualification.
            // The missing asset must fail open for every queried concrete edge.
            let graph = BuildOrderGraph::build_with_legal_board_admission(
                &problem,
                &catalog,
                &candidate,
                &mut projection,
                &mut workspace,
                unique,
                BuildCompletion::ClearToEmpty,
                None,
                mode,
                true,
                &CandidateMovementProofs::default(),
            )
            .unwrap();
            let generation = workspace.graph_generation;
            assert_eq!(
                workspace.graph_reachable_generations[1 << grounded_operation],
                generation
            );
            assert_eq!(
                workspace.graph_reachable_generations[1 << floating_operation] == generation,
                mode == BuildReachabilityMode::GeometryOnly,
                "floating singleton must not cause a normal-lock asset query"
            );
            assert!(graph.is_live());
            assert_eq!(workspace.legal_board_verified_negative_prunes(), 0);
        }
    }

    #[test]
    fn v081_concrete_edge_negative_stops_before_harddrop_or_exact_template_work() {
        let (catalog, _) = fixture();
        let row = catalog.skeleton_id(PieceKind::O, 3 | (3 << 10)).unwrap();
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &[row]).unwrap();
        let profile = KickTableProfileId::SrsPlus;
        let problem = problem_for(profile);
        for (mode, unique) in [
            (BuildReachabilityMode::Existing, true),
            (BuildReachabilityMode::Existing, false),
            (BuildReachabilityMode::GeometryOnly, true),
        ] {
            let mut workspace = BuildUpWorkspace::default();
            workspace.legal_board_query_cache.reset(profile);
            let mut projection = CandidateProjection::compile(
                &catalog,
                &candidate,
                &mut workspace,
                BuildCompletion::ClearToEmpty,
            )
            .unwrap();
            let (physical_board, deleted_original_rows) = projection.state(1);
            // Synthetic memo control-flow only: this is neither a complete
            // PC candidate nor evidence that a legal-board asset rejects it.
            assert!(!workspace.legal_board_query_cache.allows(
                LegalBoardQuery {
                    width: 10,
                    height: 4,
                    initial_board: 0,
                    kick_profile: profile,
                    physical_board,
                    deleted_original_rows,
                    placed_piece_count: 1,
                    completion: CompletionCapability::ClearToEmpty,
                },
                || false,
            ));
            let graph = BuildOrderGraph::build_with_legal_board_admission(
                &problem,
                &catalog,
                &candidate,
                &mut projection,
                &mut workspace,
                unique,
                BuildCompletion::ClearToEmpty,
                None,
                mode,
                true,
                &CandidateMovementProofs::default(),
            )
            .unwrap();
            assert_eq!(graph.nodes.len(), 1);
            assert!(graph.edges(0).is_empty());
            assert!(workspace.legal_board_child_is_rejected(1, workspace.graph_generation));
            assert_eq!(workspace.legal_board_verified_negative_prunes(), 1);
            assert_eq!(workspace.reachability.generated_state_count(), 0);
            assert_eq!(workspace.reachability.metrics().harddrop_queries, 0);
            assert_eq!(workspace.reachability.metrics().lock_queries, 0);
        }
    }

    #[test]
    fn v081_concrete_edge_order_and_multiplicity_match_independent_primitive_locks() {
        use crate::reachability_reference::reference_spawn_lock_anchors;
        for (catalog, candidate) in [o_fixture(4, 4), horizontal_i_fixture()] {
            let piece = catalog.skeleton(candidate.row_ids()[0]).piece;
            for profile in PROFILES {
                let problem = problem_for(profile);
                for (mode, unique) in [
                    (BuildReachabilityMode::Existing, true),
                    (BuildReachabilityMode::Existing, false),
                    (BuildReachabilityMode::GeometryOnly, true),
                ] {
                    let mut workspace = BuildUpWorkspace::default();
                    workspace
                        .reachability
                        .configure_kick_profile(profile, false);
                    workspace.legal_board_query_cache.reset(profile);
                    let mut projection = CandidateProjection::compile(
                        &catalog,
                        &candidate,
                        &mut workspace,
                        BuildCompletion::ClearToEmpty,
                    )
                    .unwrap();
                    let graph = BuildOrderGraph::build_with_legal_board_admission(
                        &problem,
                        &catalog,
                        &candidate,
                        &mut projection,
                        &mut workspace,
                        unique,
                        BuildCompletion::ClearToEmpty,
                        None,
                        mode,
                        true,
                        &CandidateMovementProofs::default(),
                    )
                    .unwrap();
                    // Invert only reached IDs; pending admission is not a node.
                    let mut node_subsets = vec![0; graph.nodes.len()];
                    for subset in 0..projection.state_count() {
                        let node = workspace.graph_subset_node_ids[subset] as usize;
                        if workspace.graph_reachable_generations[subset]
                            == workspace.graph_generation
                            && node < graph.nodes.len()
                        {
                            node_subsets[node] = subset;
                        }
                    }
                    for (node, &subset) in node_subsets.iter().enumerate() {
                        let (board, deleted_rows) = projection.state(subset);
                        let mut expected = Vec::new();
                        let locks = reference_spawn_lock_anchors(4, 4, board, piece, profile)
                            .expect("bounded independent primitive BFS");
                        for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
                            let bit = 1 << operation;
                            if subset & bit != 0 {
                                continue;
                            }
                            let mut concrete = Vec::new();
                            for realization in catalog.instantiations(row_id, deleted_rows) {
                                if let Some(edge) = geometric_build_edge(
                                    &catalog,
                                    &mut projection,
                                    subset | bit,
                                    operation,
                                    piece,
                                    board,
                                    deleted_rows,
                                    realization,
                                ) {
                                    concrete.push((edge, realization));
                                }
                            }
                            let reference_reachable = |edge: &BuildEdge| {
                                let anchor = edge.y as usize * 4 + edge.x as usize;
                                locks[edge.rotation.quarter_turns() as usize] & (1_u64 << anchor)
                                    != 0
                            };
                            if mode == BuildReachabilityMode::GeometryOnly {
                                expected.extend(concrete.iter().map(|(edge, _)| *edge));
                            } else if unique {
                                let first = concrete.iter().find(|(_, realization)| {
                                    primitive_harddrop(4, 4, board, realization.lock_mask)
                                });
                                let selected = first.or_else(|| {
                                    concrete.iter().find(|(edge, _)| reference_reachable(edge))
                                });
                                if let Some((edge, _)) = selected {
                                    expected.push(*edge);
                                }
                            } else {
                                expected.extend(concrete.iter().filter_map(|(edge, _)| {
                                    reference_reachable(edge).then_some(*edge)
                                }));
                            }
                        }
                        let mut actual = graph.edges(node).to_vec();
                        for edge in &mut actual {
                            edge.to = node_subsets[edge.to as usize] as u32;
                        }
                        actual.sort_unstable_by_key(BuildEdge::canonical_key);
                        expected.sort_unstable_by_key(BuildEdge::canonical_key);
                        expected.dedup();
                        assert_eq!(
                            actual, expected,
                            "{profile:?} {piece:?} {mode:?} unique={unique} subset={subset}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn v081_grounded_guard_preserves_every_independent_primitive_lock_in_all_profiles() {
        use crate::reachability_reference::reference_spawn_lock_anchors;
        for height in [1, 4, 6] {
            let full = (1_u64 << (height * 4)) - 1;
            let catalog =
                GeometryCatalog::compile_for_required_cells_on_dimensions(4, height, 0, full)
                    .unwrap();
            for profile in PROFILES {
                for board in [0, 1, 1 << (height * 4 - 1)] {
                    for piece in PieceKind::STANDARD_TETROMINOES {
                        let locks = reference_spawn_lock_anchors(4, height, board, piece, profile)
                            .expect("bounded independent primitive BFS");
                        for row in 0..catalog.skeleton_count() as u32 {
                            if catalog.skeleton(row).piece != piece {
                                continue;
                            }
                            for realization in catalog.instantiations(row, 0) {
                                if board & realization.lock_mask != 0 {
                                    continue;
                                }
                                let grounded =
                                    placement_is_grounded(4, board, realization.lock_mask);
                                assert_eq!(
                                    grounded,
                                    primitive_grounded(4, board, realization.lock_mask)
                                );
                                let anchor =
                                    realization.lock_y as usize * 4 + realization.x as usize;
                                if locks[realization.rotation.quarter_turns() as usize]
                                    & (1_u64 << anchor)
                                    != 0
                                {
                                    assert!(
                                        grounded,
                                        "{profile:?} {piece:?} {height}L board={board:#x}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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
