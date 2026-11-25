use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[path = "extended_pc_family_parallel.rs"]
mod pc_parallel;

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
pub(super) use pc_parallel::cooperative::{DryRunOwner, ParallelTerminalRetention, SerialHandoff};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, objective::objective_kind::ObjectiveKind,
    piece::piece_kind::PieceKind, solution::ExtendedTilingSolutionKey,
};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    reducer::pattern_coverage_aggregation::{
        PatternCoverageAggregation, PatternCoverageCompleteness,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
};
use clearra_finesse::{
    CostedGeometryEdge, CostedGeometryLanguage, GeometryLanguageNode, GeometryNodeId,
};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, PcChanceEvidencePolicy, SearchOutputPolicy,
    SearchProblem,
};
use clearra_replay::{
    FullHeightExecutionBatch, FullHeightReplayProjector, SpinCoverageExecutionBatch,
    SpinCoverageExecutionGraph,
};
use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;
use clearra_supply::pattern_universe::{
    PackingMultisetFamily, PackingPatternMembershipKind, PieceMultisetKey,
};
use clearra_supply::QueueObservationPolicy;

use crate::{
    performance::{profile_sample_scale, ExecutorSearchStage, SearchStageSpan},
    resource::{ExecutionMemoryBound, ExecutionMemoryOwner, SharedExecutionNodeBudget},
    CoreExecutionResult, CorePathStep, NormalizedSolutionCoverage, PcChanceCoverageEvidence,
    PcFullHeightReplayEvidence, PcScoreProblemEvidence,
};

#[path = "extended_pc_distributed.rs"]
pub(super) mod pc_distributed;

use super::{
    build_probability::{
        build_pattern_coverage_aggregation, costed_finesse_language, exact_bool_field,
        exact_solution_probabilities_requested, exact_usize_field, mark_build_constraints_filtered,
        solution_coverage_union_matches_global, validate_distributed_coverage_aggregation_surface,
        validate_distributed_coverage_authority, validate_worker_partial_probability_surface,
        BuildProbabilityAdvance, FinesseSearchMaterial,
    },
    buildup::{representative_pattern_path, BuildOrderGraph, PreparedFinesseLanguage},
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedBackendExecution,
        WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    extended_board::{compact_logical_board, words_hex},
    extended_buildup::{
        build_extended_order_graph, build_extended_order_graph_with_constraints,
        build_extended_order_graph_with_finesse, ExtendedBuildOrderResult,
        ExtendedBuildOrderWorkspace, ExtendedTilingKey,
    },
    extended_geometry::{
        ExtendedGeometryAdvance, ExtendedGeometrySearch, ExtendedGeometrySharedTargets,
    },
    extended_inverse_catalog::ExtendedInverseCatalog,
    kick_profiles::replay_profile_ids,
    mix_digest,
    piece_order_language::PieceOrderLanguageCache,
    standard_bag_coverage::StandardBagCoverage,
    WasmExactSearchError,
};

pub(super) struct ExtendedBuildProbabilitySession {
    problem: Arc<SearchProblem>,
    purpose: ExtendedFamilyPurpose,
    aggregation: BuildProbabilityAggregation,
    field: BuildProbabilityField,
    catalog: Arc<ExtendedInverseCatalog>,
    // Root accounting owns the complete immutable input/catalog allocation.
    // Native verifier workers borrow those exact owners and account only their
    // private workspace/reducer state, never Arc::strong_count snapshots.
    shared_immutable_owners: bool,
    shared_build_preparation: Option<ExtendedBuildSharedPreparation>,
    geometry: ExtendedGeometrySearch,
    build_order_workspace: ExtendedBuildOrderWorkspace,
    coverage_evaluator: CoverageProductEvaluator,
    piece_order_languages: PieceOrderLanguageCache,
    standard_bag_coverage: Option<StandardBagCoverage>,
    full_draw_required_piece_counts: Option<[u8; 7]>,
    covered_patterns: PatternBitSet,
    buildable_tilings: HashSet<ExtendedTilingKey>,
    solution_coverage: Option<HashMap<String, PatternBitSet>>,
    spin_execution_graphs: Vec<SpinCoverageExecutionGraph>,
    distributed_solution_keys: HashSet<String>,
    candidate_digest: u64,
    processed_candidate_count: usize,
    searched_build_nodes: usize,
    reachability_states: usize,
    coverage_product_states: usize,
    coverage_product_edge_checks: usize,
    coverage_product_words: usize,
    peak_build_order_nodes: usize,
    total_build_order_nodes: usize,
    peak_build_scratch_bytes: usize,
    witnessed_pattern_count: u128,
    pc_build_variant_count: u128,
    pc_build_variant_count_complete: bool,
    representative_path: Vec<CorePathStep>,
    representative_pattern_id: Option<u32>,
    representative_rank: Option<u64>,
    truncated_reason: Option<&'static str>,
    supply_projection_complete: bool,
    distributed_count_complete: bool,
    distributed_probability_complete: bool,
    trivial_target: bool,
    external_geometry: bool,
    workers_used: usize,
    parallel_active_workers: usize,
    parallel_minimum_worker_candidates: usize,
    parallel_maximum_worker_candidates: usize,
    distributed_worker_memory_bytes: usize,
    native_parallel_peak_bytes: u128,
    distributed_execution_constraint_materialized: bool,
    finesse_requested: bool,
    finesse_languages: Vec<(String, PreparedFinesseLanguage)>,
    memory_bound: ExecutionMemoryBound,
    request_node_budget: Option<Arc<SharedExecutionNodeBudget>>,
    private_memory_owner: Option<ExecutionMemoryOwner>,
    pc_observation: Option<super::extended_pc_observation::ExtendedPcObservationLanguage>,
    coexisting_retained_bytes: u128,
    finished: bool,
}

/// The geometry/reachability/language engine is shared, not its product result.
/// PC completion has its own finalizer and cannot acquire Build evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExtendedFamilyPurpose {
    Build,
    Pc,
}

/// A native original-pass capability, constructed only from the already
/// admitted Build producer. No caller-supplied catalog/source IDs can create
/// or substitute its immutable inputs.
#[derive(Clone)]
pub(super) struct ExtendedBuildSharedPreparation(Arc<ExtendedBuildPreparedInputs>);

struct ExtendedBuildPreparedInputs {
    problem: Arc<SearchProblem>,
    catalog: Arc<ExtendedInverseCatalog>,
    targets: ExtendedGeometrySharedTargets,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    solution_coverage_requested: bool,
    supply_projection_complete: bool,
}

impl ExtendedBuildSharedPreparation {
    pub(super) fn problem(&self) -> &SearchProblem {
        &self.0.problem
    }

    pub(super) fn matches_pass(
        &self,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> bool {
        self.0.field == field && self.0.aggregation == aggregation
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        // Input/catalog/index payloads remain charged to the original session.
        // This counts the new allocation even when every verifier pins it.
        self.0
            .targets
            .checked_retained_bytes()?
            .checked_add(core::mem::size_of::<ExtendedBuildPreparedInputs>() as u128 + 32)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExtendedDistributedPartial {
    count_complete: bool,
    probability_complete: bool,
    resource_truncated: bool,
    coverage_source_row_count: usize,
}

/// Output of the exact native canonical cursor, not a decoded or
/// user-supplied solution family. The cursor validates every row and records
/// its retained text capacity while ordering and hashing it.
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
struct SealedPcFamily {
    keys: Vec<String>,
    hash: String,
    key_text_bytes: u128,
}

// Browser workers call the distributed entrypoints; native builds retain the same
// session surface so compact and extended execution stay contract-compatible.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl ExtendedBuildProbabilitySession {
    /// The physical graph remains the execution authority. Only its Boolean
    /// kind-word language is composed with the standard-bag source here.
    /// Keep this gate shared with admission so an omitted index is never
    /// charged as if it existed, or selected for a different evaluator.
    pub(super) fn uses_standard_bag_coverage(
        problem: &SearchProblem,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
    ) -> Result<bool, WasmExactSearchError> {
        #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
        match std::env::var("CLEARRA_EXTENDED_STANDARD_BAG_COVERAGE") {
            Ok(value) if value == "0" => return Ok(false),
            Ok(value) if value == "1" => {}
            Err(std::env::VarError::NotPresent) => {}
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_standard_bag_mode_invalid",
                ))
            }
        }
        let Some(universe) = problem.piece_source().materialized_universe() else {
            return Ok(false);
        };
        let source_draws = problem.supply().source_sequence_length();
        let Some(UniformCompactPatternSource::Standard7Bag {
            sequence_len: actual_len,
            pattern_count: actual_count,
        }) = universe.uniform_compact_source()
        else {
            return Ok(false);
        };
        let expected_count = (0..actual_len).try_fold(1_u128, |count, index| {
            count.checked_mul((7 - index % 7) as u128)
        });
        let held = usize::from(!problem.initial_hold().hold_empty());
        let projected = problem.supply().projects_unplaced_lookahead();
        let supported_evidence = problem.pc_chance_evidence_policy()
            == PcChanceEvidencePolicy::Disabled
            || (problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::PcProbabilityV2
                && problem.output_policy() == SearchOutputPolicy::CoverageSummary
                && problem.objective().kind() == ObjectiveKind::Unique);
        Ok(aggregation == BuildProbabilityAggregation::Buildability
            && !finesse_requested
            && !problem.solution_probability_policy().requested()
            && supported_evidence
            && !problem.objective().score().requested()
            && problem.count_policy() == PcCountPolicy::CountUnique
            && problem.queue_observation_policy() == QueueObservationPolicy::FullQueueOracle
            && universe.complete()
            && universe.truncation_reason().is_none()
            && universe.pattern_count() as u128 == universe.total_possible_pattern_count()
            && universe.pattern_count() != 0
            && actual_len == source_draws
            && actual_count == universe.pattern_count()
            && expected_count == Some(actual_count as u128)
            && actual_count.checked_sub(1).is_some_and(|last| u32::try_from(last).is_ok())
            && StandardBagCoverage::supports(universe, problem.initial_hold())
            // The canonical projected window includes every lock, including
            // the initially held piece. The standard-bag terminal authority
            // must accompany that projection (not an arbitrary manual flag).
            && (!projected
                || (problem.supply().projects_standard_bag_lookahead()
                    && source_draws.checked_add(held) == problem.exact_pieces())))
    }

    fn new_standard_bag_coverage(
        problem: &SearchProblem,
        purpose: ExtendedFamilyPurpose,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Option<StandardBagCoverage>, WasmExactSearchError> {
        if (purpose == ExtendedFamilyPurpose::Build
            && problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::Disabled)
            || !Self::uses_standard_bag_coverage(problem, aggregation, finesse_requested)?
        {
            return Ok(None);
        }
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("admitted source");
        let future = StandardBagCoverage::checked_creation_future_bytes(universe).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_symbolic_creation_projection_overflow",
            ),
        )?;
        memory_bound
            .ensure(coexisting_retained_bytes, future)
            .map_err(WasmExactSearchError::resource_admission)?;
        StandardBagCoverage::for_universe(
            universe,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
        )
    }

    fn new_full_draw_required_piece_counts(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        standard_bag_coverage: Option<&StandardBagCoverage>,
    ) -> Result<Option<[u8; 7]>, WasmExactSearchError> {
        if standard_bag_coverage.is_none() {
            return Ok(None);
        }
        #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
        match std::env::var("CLEARRA_EXTENDED_FULL_DRAW_MULTISET") {
            Ok(value) if value == "0" => return Ok(None),
            Ok(value) if value == "1" => {}
            Err(std::env::VarError::NotPresent) => {}
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_full_draw_multiset_mode_invalid",
                ))
            }
        }
        // The existing symbolic owner certifies the complete, fresh, uniform
        // Standard7Bag source. It supports cursor zero only. At a complete bag
        // boundary the unknown next draw may trigger only the final held-piece
        // release; it never becomes a locked operation. Hence n = N + held
        // consumes exactly the original N pieces plus the initial held piece.
        // A partial final bag is deliberately excluded: its uniquely known
        // projected next piece can itself become a lock in the existing product.
        let draws = problem.supply().source_sequence_length();
        let held = problem.initial_hold().hold_piece();
        let expected_locks = draws.checked_add(usize::from(held.is_some()));
        if problem.initial_hold().cursor() != 0
            || draws % 7 != 0
            || expected_locks != Some(field.target_piece_count())
            || expected_locks != problem.exact_pieces()
        {
            return Ok(None);
        }
        let per_kind = u8::try_from(draws / 7).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_full_draw_multiset_overflow")
        })?;
        let mut required = [per_kind; 7];
        if let Some(piece) = held {
            let count = &mut required[super::piece_index(piece)];
            *count = count
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_full_draw_multiset_overflow",
                ))?;
        }
        Ok(Some(required))
    }

    // Browser product execution always supplies a finite-memory boundary; this
    // unbounded convenience adapter remains for native embeddings and tests.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_memory_bound(problem, field, aggregation, memory_bound)
    }

    pub(super) fn new_with_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_mode(
            problem,
            field,
            aggregation,
            false,
            memory_bound,
            0,
            ExtendedFamilyPurpose::Build,
        )
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_finesse_and_memory_bound(problem, field, aggregation, memory_bound)
    }

    pub(super) fn new_with_finesse_and_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_mode(
            problem,
            field,
            aggregation,
            true,
            memory_bound,
            0,
            ExtendedFamilyPurpose::Build,
        )
    }

    pub(super) fn new_with_memory_bound_and_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_mode(
            problem,
            field,
            aggregation,
            finesse_requested,
            memory_bound,
            coexisting_retained_bytes,
            ExtendedFamilyPurpose::Build,
        )
    }

    /// Full-height PC family producer, including typed minimum source coverage.
    /// Score/replay evidence is a distinct contract; no Build result is relabelled.
    pub(super) fn new_pc_family(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        super::extended_pc_search::validate_pc_family_problem(problem)?;
        let initial = clearra_core_domain::board::standard_pc_board::Board256Mask::from_words(
            problem.initial_board().occupied_words(),
        );
        let full = clearra_core_domain::board::standard_pc_board::Board256Mask::all_cells(
            problem.visible_height() * 10,
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_height_invalid"))?;
        if u16::from(field.height()) != problem.visible_height()
            || field.base_words() != initial.words()
            || field.target_words() != full.without(initial).words()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_field_binding_mismatch",
            ));
        }
        Self::new_mode(
            problem,
            field,
            BuildProbabilityAggregation::Buildability,
            false,
            memory_bound,
            coexisting_retained_bytes,
            ExtendedFamilyPurpose::Pc,
        )
    }

    /// Borrow immutable source/catalog owners; BuildUp, language and reducer
    /// state remain private. Native and local WASM coordinators use this exact
    /// constructor instead of cloning full graph assets into a new session.
    pub(super) fn new_shared_pc_verifier_with_bound(
        &self,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        if self.purpose != ExtendedFamilyPurpose::Pc {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_shared_verifier_requires_pc",
            ));
        }
        self.new_shared_family_verifier_with_resources(memory_bound, None)
    }

    fn shared_family_verifier_creation_bytes(&self) -> Result<(u128, u128), WasmExactSearchError> {
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let pattern_count = universe.pattern_count();
        let bitmap_creation = (pattern_count.div_ceil(64) as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .and_then(|bytes| bytes.checked_add(128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_verifier_creation_projection_overflow",
            ))?;
        let symbolic_creation = if self.standard_bag_coverage.is_some() {
            StandardBagCoverage::checked_creation_future_bytes(universe).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "extended_build_verifier_creation_projection_overflow",
                ),
            )?
        } else {
            0
        };
        let creation = bitmap_creation.checked_add(symbolic_creation).ok_or(
            WasmExactSearchError::InvalidProblem(
                "extended_build_verifier_creation_projection_overflow",
            ),
        )?;
        Ok((bitmap_creation, creation))
    }

    fn new_shared_family_verifier_with_resources(
        &self,
        memory_bound: ExecutionMemoryBound,
        private_memory_owner: Option<ExecutionMemoryOwner>,
    ) -> Result<Self, WasmExactSearchError> {
        let (bitmap_creation, creation) = self.shared_family_verifier_creation_bytes()?;
        let pattern_count = self
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_source_not_materialized",
            ))?
            .pattern_count();
        if let Some(owner) = private_memory_owner.as_ref() {
            owner
                .ensure(creation)
                .map_err(WasmExactSearchError::resource_admission)?;
        } else {
            memory_bound
                .ensure(0, creation)
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let standard_bag_coverage = Self::new_standard_bag_coverage(
            &self.problem,
            self.purpose,
            self.aggregation,
            self.finesse_requested,
            memory_bound,
            bitmap_creation,
        )?;
        let verifier = Self {
            problem: Arc::clone(&self.problem),
            purpose: self.purpose,
            aggregation: self.aggregation,
            field: self.field,
            catalog: Arc::clone(&self.catalog),
            shared_immutable_owners: true,
            shared_build_preparation: None,
            geometry: ExtendedGeometrySearch::private_verifier_placeholder(),
            build_order_workspace: ExtendedBuildOrderWorkspace::new(
                self.field.width(),
                self.field.height(),
                self.problem.kick_profile().profile_id(),
            ),
            coverage_evaluator: CoverageProductEvaluator::default(),
            piece_order_languages: PieceOrderLanguageCache::default(),
            standard_bag_coverage,
            full_draw_required_piece_counts: self.full_draw_required_piece_counts,
            covered_patterns: PatternBitSet::new(pattern_count),
            buildable_tilings: HashSet::new(),
            solution_coverage: self.solution_coverage.as_ref().map(|_| HashMap::new()),
            spin_execution_graphs: Vec::new(),
            distributed_solution_keys: HashSet::new(),
            candidate_digest: 0,
            processed_candidate_count: 0,
            searched_build_nodes: 0,
            reachability_states: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            coverage_product_words: 0,
            peak_build_order_nodes: 0,
            total_build_order_nodes: 0,
            peak_build_scratch_bytes: 0,
            witnessed_pattern_count: 0,
            pc_build_variant_count: 0,
            pc_build_variant_count_complete: true,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            truncated_reason: None,
            supply_projection_complete: self.supply_projection_complete,
            distributed_count_complete: true,
            distributed_probability_complete: true,
            trivial_target: false,
            external_geometry: true,
            workers_used: 1,
            parallel_active_workers: 0,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            distributed_worker_memory_bytes: 0,
            native_parallel_peak_bytes: 0,
            distributed_execution_constraint_materialized: false,
            finesse_requested: self.finesse_requested,
            finesse_languages: Vec::new(),
            memory_bound,
            request_node_budget: self.request_node_budget.clone(),
            private_memory_owner,
            pc_observation: (self.purpose == ExtendedFamilyPurpose::Pc)
                .then(|| {
                    super::extended_pc_observation::ExtendedPcObservationLanguage::for_problem(
                        &self.problem,
                    )
                })
                .flatten(),
            coexisting_retained_bytes: 0,
            finished: false,
        };
        verifier.ensure_memory_bound(0)?;
        Ok(verifier)
    }

    fn new_shared_pc_verifier_with_resources(
        &self,
        memory_bound: ExecutionMemoryBound,
        private_memory_owner: Option<ExecutionMemoryOwner>,
    ) -> Result<Self, WasmExactSearchError> {
        if self.purpose != ExtendedFamilyPurpose::Pc {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_shared_verifier_requires_pc",
            ));
        }
        self.new_shared_family_verifier_with_resources(memory_bound, private_memory_owner)
    }

    /// Export the original native pass once. Unsupported cases retain their
    /// existing constructors, including finite node-credit delegation,
    /// mirrors, empty targets, and finesse preparation.
    pub(super) fn shared_build_preparation(
        &mut self,
    ) -> Result<Option<ExtendedBuildSharedPreparation>, WasmExactSearchError> {
        if let Some(prepared) = self.shared_build_preparation.as_ref() {
            return Ok(Some(prepared.clone()));
        }
        if self.purpose != ExtendedFamilyPurpose::Build
            || self.shared_immutable_owners
            || self.external_geometry
            || self.finished
            || self.trivial_target
            || self.finesse_requested
            || self.field.includes_applicable_horizontal_mirror()
            || self.problem.backend_request().max_nodes() != 0
        {
            return Ok(None);
        }
        let projection_error = || {
            WasmExactSearchError::InvalidProblem(
                "extended_build_shared_preparation_projection_unavailable",
            )
        };
        let future = self
            .geometry
            .checked_external_verifier_future_bytes()
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<ExtendedBuildPreparedInputs>() as u128 + 32)
            })
            .ok_or_else(projection_error)?;
        self.ensure_memory_bound(future)?;
        let targets =
            self.geometry
                .shared_verifier_targets()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "extended_build_shared_preparation_geometry_not_initial",
                ))?;
        let prepared = ExtendedBuildSharedPreparation(Arc::new(ExtendedBuildPreparedInputs {
            problem: Arc::clone(&self.problem),
            catalog: Arc::clone(&self.catalog),
            targets,
            field: self.field,
            aggregation: self.aggregation,
            solution_coverage_requested: self.solution_coverage.is_some(),
            supply_projection_complete: self.supply_projection_complete,
        }));
        self.shared_build_preparation = Some(prepared.clone());
        self.ensure_memory_bound(0)?;
        Ok(Some(prepared))
    }

    /// Reuse the exact source/catalog/target owners, keeping all mutable
    /// coverage, language, reachability and result state private. This accepts
    /// no independent SearchProblem: the capability is the request authority.
    pub(super) fn new_shared_build_verifier(
        prepared: &ExtendedBuildSharedPreparation,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        if !prepared.matches_pass(field, aggregation) {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_build_shared_preparation_request_mismatch",
            ));
        }
        let prepared = &prepared.0;
        let pattern_count = prepared
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_source_not_materialized",
            ))?
            .pattern_count();
        let creation = (pattern_count.div_ceil(64) as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .and_then(|bytes| bytes.checked_add(128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_build_verifier_creation_projection_overflow",
            ))?;
        memory_bound
            .ensure(coexisting_retained_bytes, creation)
            .map_err(WasmExactSearchError::resource_admission)?;
        let standard_bag_coverage = Self::new_standard_bag_coverage(
            &prepared.problem,
            ExtendedFamilyPurpose::Build,
            aggregation,
            false,
            memory_bound,
            coexisting_retained_bytes.checked_add(creation).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "extended_build_verifier_creation_projection_overflow",
                ),
            )?,
        )?;
        let full_draw_required_piece_counts = Self::new_full_draw_required_piece_counts(
            &prepared.problem,
            field,
            standard_bag_coverage.as_ref(),
        )?;
        let verifier = Self {
            problem: Arc::clone(&prepared.problem),
            purpose: ExtendedFamilyPurpose::Build,
            aggregation,
            field,
            catalog: Arc::clone(&prepared.catalog),
            shared_immutable_owners: true,
            shared_build_preparation: None,
            geometry: ExtendedGeometrySearch::from_shared_verifier_targets(&prepared.targets),
            build_order_workspace: ExtendedBuildOrderWorkspace::new(
                field.width(),
                field.height(),
                prepared.problem.kick_profile().profile_id(),
            ),
            coverage_evaluator: CoverageProductEvaluator::default(),
            piece_order_languages: PieceOrderLanguageCache::default(),
            standard_bag_coverage,
            full_draw_required_piece_counts,
            covered_patterns: PatternBitSet::new(pattern_count),
            buildable_tilings: HashSet::new(),
            solution_coverage: prepared.solution_coverage_requested.then(HashMap::new),
            spin_execution_graphs: Vec::new(),
            distributed_solution_keys: HashSet::new(),
            candidate_digest: 0,
            processed_candidate_count: 0,
            searched_build_nodes: 0,
            reachability_states: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            coverage_product_words: 0,
            peak_build_order_nodes: 0,
            total_build_order_nodes: 0,
            peak_build_scratch_bytes: 0,
            witnessed_pattern_count: 0,
            pc_build_variant_count: 0,
            pc_build_variant_count_complete: true,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            truncated_reason: None,
            supply_projection_complete: prepared.supply_projection_complete,
            distributed_count_complete: true,
            distributed_probability_complete: true,
            trivial_target: false,
            external_geometry: true,
            workers_used: 1,
            parallel_active_workers: 0,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            distributed_worker_memory_bytes: 0,
            native_parallel_peak_bytes: 0,
            distributed_execution_constraint_materialized: false,
            finesse_requested: false,
            finesse_languages: Vec::new(),
            memory_bound,
            request_node_budget: None,
            private_memory_owner: None,
            pc_observation: None,
            coexisting_retained_bytes,
            finished: false,
        };
        verifier.ensure_memory_bound(0)?;
        Ok(verifier)
    }

    #[allow(clippy::too_many_arguments)]
    fn new_mode(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
        purpose: ExtendedFamilyPurpose,
    ) -> Result<Self, WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
        if aggregation.is_tiling_only() && problem.solution_probability_policy().requested() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probabilities_unavailable_with_tiling",
            ));
        }
        if field.is_compact() || !(7..=24).contains(&field.height()) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_height_invalid",
            ));
        }
        if usize::from(problem.visible_height()) != usize::from(field.height()) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_layout_mismatch",
            ));
        }
        if problem.allowed_colored_solution_identities().is_some() {
            // A Board64 colored allow-list cannot authorize a full-height
            // result. Reject rather than returning an unfiltered family.
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_colored_identity_filter_unsupported",
            ));
        }
        let target_piece_count = field.target_piece_count();
        if target_piece_count > 60 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_piece_count_exceeds_60",
            ));
        }
        if problem
            .exact_pieces()
            .is_some_and(|count| count != target_piece_count)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_piece_count_mismatch",
            ));
        }
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let initial_other = if purpose == ExtendedFamilyPurpose::Build {
            super::build_probability::checked_build_probability_problem_nested_retained_bytes(
                problem,
            )
            .and_then(|bytes| bytes.checked_add(coexisting_retained_bytes))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128 + 32))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_symbolic_creation_projection_overflow",
            ))?
        } else {
            coexisting_retained_bytes
        };
        let standard_bag_coverage = Self::new_standard_bag_coverage(
            problem,
            purpose,
            aggregation,
            finesse_requested,
            memory_bound,
            initial_other,
        )?;
        let full_draw_required_piece_counts = Self::new_full_draw_required_piece_counts(
            problem,
            field,
            standard_bag_coverage.as_ref(),
        )?;
        let supply_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionSupplyCompile);
        let family = universe.packing_multiset_family_for_execution(
            target_piece_count,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            super::packing_hold_projection(problem),
        );
        supply_span.finish(family.len() as u64);
        if target_piece_count != 0 && family.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_supply_has_no_reachable_piece_multiset",
            ));
        }
        let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionCatalogCompile);
        let catalog = if purpose == ExtendedFamilyPurpose::Pc {
            // Reject catalog growth at allocation time rather than discovering
            // it after an unbounded compile. This is the same conservative
            // realization credit used by the full-height Tiling producer.
            let fixed = coexisting_retained_bytes
                .checked_add(
                    super::extended_pc_search::checked_pc_family_problem_nested_retained_bytes(
                        problem,
                    )
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_memory_projection_unavailable",
                    ))?,
                )
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<Self>() as u128 + 32 * 1024)
                })
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "extended_pc_memory_projection_unavailable",
                ))?;
            memory_bound
                .ensure(fixed, 0)
                .map_err(WasmExactSearchError::resource_admission)?;
            let credit = memory_bound.cap_bytes() - fixed;
            let max_realizations = usize::try_from(credit / 2048).unwrap_or(usize::MAX);
            ExtendedInverseCatalog::compile_bounded(field, max_realizations).map_err(|error| {
                if error.reason() == "extended_catalog_memory_budget_exceeded" {
                    WasmExactSearchError::resource_admission(
                        memory_bound
                            .ensure(fixed, credit.saturating_add(2048))
                            .expect_err("catalog exceeded its finite realization credit"),
                    )
                } else {
                    error
                }
            })?
        } else {
            ExtendedInverseCatalog::compile(field)?
        };
        catalog_span.finish(1);
        let supply_projection_complete = universe.complete()
            || family.membership_kind() == PackingPatternMembershipKind::ExactSymbolicStandardBag;
        let geometry_other = coexisting_retained_bytes
            .checked_add(core::mem::size_of::<Self>() as u128)
            .and_then(|bytes| bytes.checked_add(catalog.retained_bytes() as u128))
            .and_then(|bytes| {
                bytes.checked_add(
                    standard_bag_coverage
                        .as_ref()
                        .map_or(0, StandardBagCoverage::retained_bytes) as u128,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<ExtendedInverseCatalog>() as u128 + 32)
            })
            .and_then(|bytes| {
                // Build's supply shell may use Tiling + Trace without owning
                // typed PC tiling authority. Charge the actual purpose's
                // retained owners, as the live session projection does.
                let problem_bytes = match purpose {
                    ExtendedFamilyPurpose::Build => {
                        super::build_probability::checked_build_probability_problem_nested_retained_bytes(
                            problem,
                        )
                    }
                    ExtendedFamilyPurpose::Pc => {
                        super::extended_pc_search::checked_pc_family_problem_nested_retained_bytes(
                            problem,
                        )
                    }
                }?;
                bytes.checked_add(problem_bytes)
            })
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_geometry_memory_projection_unavailable",
            ))?;
        let geometry_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionGeometryPrepare);
        let geometry = ExtendedGeometrySearch::new_bounded_with_pattern_indexes(
            universe,
            &family,
            &catalog,
            memory_bound,
            geometry_other,
            standard_bag_coverage.is_none(),
        )?;
        geometry_span.finish(family.len() as u64);
        let build_order_workspace = ExtendedBuildOrderWorkspace::new(
            field.width(),
            field.height(),
            problem.kick_profile().profile_id(),
        );
        let covered_patterns = if target_piece_count == 0 {
            PatternBitSet::all(universe.pattern_count())
        } else {
            PatternBitSet::new(universe.pattern_count())
        };
        let session = Self {
            problem: Arc::new(problem.clone()),
            purpose,
            aggregation,
            field,
            catalog: Arc::new(catalog),
            shared_immutable_owners: false,
            shared_build_preparation: None,
            geometry,
            build_order_workspace,
            coverage_evaluator: CoverageProductEvaluator::default(),
            piece_order_languages: PieceOrderLanguageCache::default(),
            standard_bag_coverage,
            full_draw_required_piece_counts,
            covered_patterns,
            buildable_tilings: HashSet::new(),
            solution_coverage: (problem.solution_probability_policy().requested()
                || problem.objective().score().requested()
                || ((purpose == ExtendedFamilyPurpose::Pc
                    || aggregation != BuildProbabilityAggregation::Buildability)
                    && problem.objective().execution_constraints().requested())
                || matches!(
                    problem.pc_chance_evidence_policy(),
                    PcChanceEvidencePolicy::PcMinimumCoverV2
                        | PcChanceEvidencePolicy::PcProbabilityV2
                        | PcChanceEvidencePolicy::PcFailedQueueV2 { .. }
                ))
            .then(HashMap::new),
            spin_execution_graphs: Vec::new(),
            distributed_solution_keys: HashSet::new(),
            candidate_digest: 0,
            processed_candidate_count: 0,
            searched_build_nodes: 0,
            reachability_states: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            coverage_product_words: 0,
            peak_build_order_nodes: 0,
            total_build_order_nodes: 0,
            peak_build_scratch_bytes: 0,
            witnessed_pattern_count: 0,
            pc_build_variant_count: 0,
            pc_build_variant_count_complete: true,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            truncated_reason: None,
            supply_projection_complete,
            distributed_count_complete: true,
            distributed_probability_complete: true,
            trivial_target: target_piece_count == 0,
            external_geometry: false,
            workers_used: 1,
            parallel_active_workers: 0,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            distributed_worker_memory_bytes: 0,
            native_parallel_peak_bytes: 0,
            distributed_execution_constraint_materialized: false,
            finesse_requested,
            finesse_languages: Vec::new(),
            memory_bound,
            coexisting_retained_bytes,
            request_node_budget: (problem.backend_request().max_nodes() != 0).then(|| {
                Arc::new(SharedExecutionNodeBudget::new(
                    problem.backend_request().max_nodes(),
                ))
            }),
            private_memory_owner: None,
            pc_observation: (purpose == ExtendedFamilyPurpose::Pc)
                .then(|| {
                    super::extended_pc_observation::ExtendedPcObservationLanguage::for_problem(
                        problem,
                    )
                })
                .flatten(),
            finished: false,
        };
        session.ensure_memory_bound(0)?;
        Ok(session)
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_external_geometry_with_memory_bound(problem, field, aggregation, memory_bound)
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_external_geometry_with_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            memory_bound,
            0,
        )
    }

    pub(super) fn new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        let mut session = Self::new_mode(
            problem,
            field,
            aggregation,
            false,
            memory_bound,
            coexisting_retained_bytes,
            ExtendedFamilyPurpose::Build,
        )?;
        if !session.geometry.prepare_external() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_external_geometry_prepare_failed",
            ));
        }
        session.external_geometry = true;
        session.ensure_memory_bound(0)?;
        Ok(session)
    }

    pub(super) fn distributed_progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.geometry.expanded_nodes(),
            candidates: if self.external_geometry {
                self.processed_candidate_count
            } else {
                self.geometry.candidate_count()
            },
            candidate_family_count: self.geometry.candidate_family_count(),
            build_nodes: self.searched_build_nodes,
            coverage_checks: self.coverage_product_edge_checks,
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_session_already_finished",
            ));
        }
        if control.is_cancelled() {
            return Ok(BuildProbabilityAdvance::Cancelled);
        }
        self.ensure_memory_bound(0)?;
        if self.trivial_target {
            return self.complete_with_control(control);
        }

        let mut work = 0usize;
        while work < work_budget.max(1) {
            if control.is_cancelled() {
                return Ok(BuildProbabilityAdvance::Cancelled);
            }
            if self.node_budget_exhausted() {
                self.truncated_reason = Some("node_budget_exceeded");
                return self.complete_with_control(control);
            }
            self.ensure_memory_bound(0)?;
            let max_candidates = self.problem.backend_request().max_candidates();
            if max_candidates != 0 && self.geometry.candidate_count() >= max_candidates {
                self.truncated_reason = Some("candidate_budget_exceeded");
                return self.complete_with_control(control);
            }
            match self.advance_geometry_bounded()? {
                ExtendedGeometryAdvance::Pending => work += 1,
                ExtendedGeometryAdvance::Candidate(candidate) => {
                    let ordinal = self.geometry.candidate_count().saturating_sub(1) as u64;
                    self.process_candidate(candidate, Some(ordinal), control)?;
                    work += 1;
                    if self.truncated_reason.is_some() {
                        return self.complete_with_control(control);
                    }
                }
                ExtendedGeometryAdvance::Complete => return self.complete_with_control(control),
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    self.truncated_reason = Some(reason);
                    return self.complete_with_control(control);
                }
            }
        }
        Ok(BuildProbabilityAdvance::Pending)
    }

    fn process_candidate(
        &mut self,
        candidate: super::extended_geometry::ExtendedGeometryCandidate,
        external_ordinal: Option<u64>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        // Candidate identities and their optional score/spin string clones
        // coexist with the graph builder. Keep their bounded credit present
        // until the complete candidate has transferred or dropped them.
        let local = (candidate.row_ids().len() as u128)
            .checked_mul(256)
            .and_then(|bytes| bytes.checked_add(512))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        self.ensure_memory_bound(local)?;
        let original_coexisting = self.coexisting_retained_bytes;
        self.coexisting_retained_bytes =
            original_coexisting
                .checked_add(local)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "extended_pc_memory_projection_unavailable",
                ))?;
        let result =
            self.process_candidate_with_identity_credit(candidate, external_ordinal, control);
        self.coexisting_retained_bytes = original_coexisting;
        result
    }

    fn process_candidate_with_identity_credit(
        &mut self,
        candidate: super::extended_geometry::ExtendedGeometryCandidate,
        external_ordinal: Option<u64>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.processed_candidate_count = self.processed_candidate_count.saturating_add(1);
        let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
        let tiling_digest = tiling.digest();
        self.candidate_digest = mix_digest(self.candidate_digest, tiling_digest);
        if let Some(required) = self.full_draw_required_piece_counts {
            if candidate.row_ids().len() != self.field.target_piece_count() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_full_draw_candidate_depth_mismatch",
                ));
            }
            let mut actual = [0_u8; 7];
            for &row in candidate.row_ids() {
                let count = &mut actual[super::piece_index(self.catalog.skeleton(row).piece)];
                *count = count
                    .checked_add(1)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_full_draw_multiset_overflow",
                    ))?;
            }
            if actual != required {
                // This exact necessary condition changes neither the family
                // inventory above nor any successful physical/supply witness.
                // Keep cancellation observable when the graph walk is skipped.
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                return Ok(());
            }
        }
        if self.aggregation.is_tiling_only() {
            let growth = checked_hash_growth_future_bytes(
                self.buildable_tilings.capacity(),
                self.buildable_tilings.len(),
                core::mem::size_of::<ExtendedTilingKey>(),
            )
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
            self.ensure_memory_bound(growth)?;
            self.buildable_tilings.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_build_probability_solution_storage_unavailable",
                )
            })?;
            if !self.finesse_requested {
                self.buildable_tilings.insert(tiling);
                return Ok(());
            }
            self.buildable_tilings.insert(tiling.clone());
        }
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().score().requested()
            || (self.problem.objective().execution_constraints().requested()
                && ((self.purpose == ExtendedFamilyPurpose::Build
                    && !self.build_constraints_materialized())
                    || self.finesse_requested))
            || (self.purpose == ExtendedFamilyPurpose::Pc
                && self
                    .problem
                    .pc_chance_evidence_policy()
                    .retains_pc_path_v2_evidence());
        let candidate_key = (execution_evidence_requested
            || self.finesse_requested
            || self.solution_coverage.is_some())
        .then(|| tiling.canonical_key(self.catalog.initial_board(), self.field.height()));
        let spin_candidate = execution_evidence_requested.then(|| {
            (
                tiling_digest,
                candidate_key
                    .as_ref()
                    .expect("requested execution evidence has a candidate key")
                    .clone(),
            )
        });
        let node_limit = self.remaining_node_budget();
        if self.problem.backend_request().max_nodes() != 0 && node_limit == 0 {
            self.truncated_reason = Some("node_budget_exceeded");
            return Ok(());
        }
        self.prepare_build_order_resources()?;
        let graph_span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmBuildOrderReachability,
            profile_sample_scale(self.processed_candidate_count),
        );
        let build_order = if self.finesse_requested {
            build_extended_order_graph_with_finesse(
                &self.problem,
                &self.catalog,
                &candidate,
                &mut self.build_order_workspace,
                node_limit,
                spin_candidate,
                self.aggregation.requests_spin_coverage(),
                control,
            )
        } else if self.purpose == ExtendedFamilyPurpose::Pc || self.build_constraints_materialized()
        {
            build_extended_order_graph_with_constraints(
                &self.problem,
                &self.catalog,
                &candidate,
                &mut self.build_order_workspace,
                node_limit,
                spin_candidate,
                control,
            )
        } else {
            build_extended_order_graph(
                &self.catalog,
                &candidate,
                &mut self.build_order_workspace,
                node_limit,
                spin_candidate,
                control,
            )
        }?;
        graph_span.finish(match &build_order {
            ExtendedBuildOrderResult::Incomplete { searched_nodes, .. }
            | ExtendedBuildOrderResult::Complete { searched_nodes, .. } => *searched_nodes as u64,
        });
        self.apply_build_order_result(
            &candidate,
            tiling,
            candidate_key,
            external_ordinal,
            build_order,
            control,
        )
    }

    fn apply_build_order_result(
        &mut self,
        candidate: &super::extended_geometry::ExtendedGeometryCandidate,
        tiling: ExtendedTilingKey,
        candidate_key: Option<String>,
        external_ordinal: Option<u64>,
        build_order: ExtendedBuildOrderResult,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        match build_order {
            ExtendedBuildOrderResult::Incomplete {
                searched_nodes,
                reachability_states,
                scratch_bytes,
            } => {
                self.searched_build_nodes =
                    self.searched_build_nodes.saturating_add(searched_nodes);
                self.reachability_states =
                    self.reachability_states.saturating_add(reachability_states);
                self.peak_build_scratch_bytes = self.peak_build_scratch_bytes.max(scratch_bytes);
                self.truncated_reason = Some("node_budget_exceeded");
                Ok(())
            }
            ExtendedBuildOrderResult::Complete {
                graph,
                finesse_language,
                spin_graph,
                searched_nodes,
                reachability_states,
                scratch_bytes,
            } => {
                self.searched_build_nodes =
                    self.searched_build_nodes.saturating_add(searched_nodes);
                self.reachability_states =
                    self.reachability_states.saturating_add(reachability_states);
                self.peak_build_scratch_bytes = self.peak_build_scratch_bytes.max(scratch_bytes);
                self.peak_build_order_nodes = self.peak_build_order_nodes.max(graph.nodes.len());
                self.total_build_order_nodes = self
                    .total_build_order_nodes
                    .saturating_add(graph.nodes.len());
                if !graph.is_live() {
                    return Ok(());
                }
                if self.standard_bag_coverage.is_some() {
                    if finesse_language.is_some()
                        || spin_graph.is_some()
                        || self.pc_observation.is_some()
                        || (self.purpose == ExtendedFamilyPurpose::Build
                            && self.solution_coverage.is_some())
                    {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_extended_symbolic_boolean_scope_mismatch",
                        ));
                    }
                    return self.apply_symbolic_build_order(
                        candidate,
                        tiling,
                        candidate_key.as_deref(),
                        external_ordinal,
                        &graph,
                        control,
                    );
                }
                if let Some(observation) = self.pc_observation.as_ref() {
                    let future = observation
                        .checked_graph_insert_future_bytes(&graph)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "extended_pc_observation_memory_projection_overflow",
                        ))?;
                    self.ensure_memory_bound(future)?;
                    self.pc_observation
                        .as_mut()
                        .expect("observation owner remains bound")
                        .insert_graph(&graph)?;
                }
                let count_pc_paths = self.count_pc_build_paths();
                let pattern_index = candidate.pattern_index.as_deref().ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_extended_pattern_index_missing"),
                )?;
                let coverage_future = self
                    .coverage_evaluator
                    .checked_evaluation_future_bytes(
                        &graph,
                        pattern_index.word_count(),
                        pattern_index.global_pattern_count(),
                        count_pc_paths,
                    )
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_coverage_memory_projection_overflow",
                    ))?;
                self.ensure_memory_bound(coverage_future)?;
                let coverage_span = SearchStageSpan::begin_scaled(
                    ExecutorSearchStage::WasmCoverageLanguageProduct,
                    profile_sample_scale(self.processed_candidate_count),
                );
                let product = if let Some(language) = finesse_language.as_ref() {
                    self.coverage_evaluator.evaluate_with_finesse(
                        &graph,
                        pattern_index,
                        self.problem.initial_hold(),
                        self.problem.supply().hold_enabled(),
                        self.problem.supply().projects_unplaced_lookahead(),
                        self.problem.supply().projects_standard_bag_lookahead(),
                        count_pc_paths,
                        false,
                        &language.nodes,
                        self.problem.spawn_profile(),
                        control,
                    )?
                } else {
                    self.coverage_evaluator.evaluate(
                        &graph,
                        pattern_index,
                        self.problem.initial_hold(),
                        self.problem.supply().hold_enabled(),
                        self.problem.supply().projects_unplaced_lookahead(),
                        self.problem.supply().projects_standard_bag_lookahead(),
                        count_pc_paths,
                        false,
                        control,
                    )?
                };
                coverage_span.finish(product.edge_checks as u64);
                self.coverage_product_states = self
                    .coverage_product_states
                    .saturating_add(product.active_states);
                self.coverage_product_words = self
                    .coverage_product_words
                    .saturating_add(product.processed_words);
                self.coverage_product_edge_checks = self
                    .coverage_product_edge_checks
                    .saturating_add(product.edge_checks);
                if product.coverage_bits.is_empty() {
                    return Ok(());
                }
                self.witnessed_pattern_count = self
                    .witnessed_pattern_count
                    .saturating_add(u128::from(product.coverage_bits.count_ones()));
                if self.count_pc_build_paths() {
                    let next = self.pc_build_variant_count.checked_add(product.path_count);
                    self.pc_build_variant_count = next.unwrap_or(u128::MAX);
                    self.pc_build_variant_count_complete &=
                        next.is_some() && product.count_complete;
                }
                let rank = external_ordinal
                    .unwrap_or_else(|| self.processed_candidate_count.saturating_sub(1) as u64);
                if self
                    .representative_rank
                    .is_none_or(|current| rank < current)
                {
                    let pattern_id = product.coverage_bits.first_pattern().ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_coverage_witness_missing",
                        ),
                    )?;
                    let universe = self
                        .problem
                        .piece_source()
                        .materialized_universe()
                        .expect("extended build probability requires a materialized universe");
                    let sequence = universe.sequence(pattern_id);
                    let path = representative_pattern_path(
                        &self.problem,
                        &graph,
                        sequence.as_ref(),
                        finesse_language
                            .as_ref()
                            .map(|language| language.nodes.as_slice()),
                    );
                    if path.len() != candidate.row_ids().len() {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_extended_coverage_witness_path_missing",
                        ));
                    }
                    self.representative_path = path;
                    self.representative_pattern_id = Some(pattern_id.index() as u32);
                    self.representative_rank = Some(rank);
                }
                self.covered_patterns
                    .union_with(&product.coverage_bits)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_coverage_universe_mismatch",
                        )
                    })?;
                if self.solution_coverage.is_some() {
                    let candidate_key =
                        candidate_key
                            .as_ref()
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_extended_solution_coverage_key_missing",
                            ))?;
                    self.merge_solution_coverage(candidate_key, &product.coverage_bits)?;
                }
                if let Some(language) = finesse_language {
                    self.finesse_languages.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_finesse_language_storage_unavailable",
                        )
                    })?;
                    self.finesse_languages.push((
                        candidate_key
                            .as_ref()
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_extended_finesse_solution_key_missing",
                            ))?
                            .clone(),
                        language,
                    ));
                }
                self.retain_buildable_tiling(tiling)?;
                if let Some(spin_graph) = spin_graph.filter(|_| {
                    self.aggregation.requests_spin_coverage()
                        || !self.build_constraints_materialized()
                }) {
                    // The returned graph and source scratch coexist until
                    // transfer. Charge both before growing its retained owner.
                    self.ensure_memory_bound(
                        (spin_graph.retained_bytes() as u128)
                            .checked_add(
                                ((self.spin_execution_graphs.len() + 1)
                                    .saturating_mul(2)
                                    .max(4) as u128)
                                    * core::mem::size_of::<SpinCoverageExecutionGraph>() as u128,
                            )
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "extended_pc_score_graph_projection_overflow",
                            ))?,
                    )?;
                    self.spin_execution_graphs.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_spin_graph_storage_unavailable",
                        )
                    })?;
                    self.spin_execution_graphs.push(spin_graph);
                }
                Ok(())
            }
        }
    }

    fn apply_symbolic_build_order(
        &mut self,
        candidate: &super::extended_geometry::ExtendedGeometryCandidate,
        tiling: ExtendedTilingKey,
        candidate_key: Option<&str>,
        external_ordinal: Option<u64>,
        graph: &BuildOrderGraph,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmCoverageLanguageProduct,
            profile_sample_scale(self.processed_candidate_count),
        );
        let language_future = self
            .piece_order_languages
            .checked_canonicalize_future_bytes(graph)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_language_projection_overflow",
            ))?;
        self.ensure_memory_bound(language_future)?;
        let root = self.piece_order_languages.canonicalize(graph)?;
        self.prepare_standard_bag_resources()?;
        let product = self
            .standard_bag_coverage
            .as_mut()
            .expect("symbolic owner")
            .cover_language(&self.piece_order_languages, root, control)?;
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(product.product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(product.edge_checks);
        if product.covers_any_pattern {
            let pattern_id = product
                .witness_pattern_id
                .filter(|_| product.covered_pattern_count != 0)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_symbolic_witness_missing",
                ))?;
            // Typed Chance and constrained PC retain exact original-source
            // rows. Materialize this candidate root while it is live, before
            // global union is allowed to recycle the symbolic arena.
            if self.solution_coverage.is_some() {
                let key = candidate_key.ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_key_missing",
                ))?;
                self.ensure_symbolic_materialization_bound()?;
                self.prepare_standard_bag_resources()?;
                let row = self
                    .standard_bag_coverage
                    .as_ref()
                    .expect("symbolic owner")
                    .materialize_root(product.root)?;
                self.merge_solution_coverage(key, &row)?;
            }
            self.witnessed_pattern_count = self
                .witnessed_pattern_count
                .saturating_add(product.covered_pattern_count as u128);
            let rank = external_ordinal
                .unwrap_or_else(|| self.processed_candidate_count.saturating_sub(1) as u64);
            if self
                .representative_rank
                .is_none_or(|current| rank < current)
            {
                let universe = self
                    .problem
                    .piece_source()
                    .materialized_universe()
                    .expect("admitted source");
                let sequence = universe.sequence(
                    clearra_coverage::pattern::pattern_id::PatternId::new(pattern_id as usize),
                );
                let path =
                    representative_pattern_path(&self.problem, graph, sequence.as_ref(), None);
                if path.len() != candidate.row_ids().len() {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_coverage_witness_path_missing",
                    ));
                }
                self.representative_path = path;
                self.representative_pattern_id = Some(pattern_id);
                self.representative_rank = Some(rank);
            }
            self.prepare_standard_bag_resources()?;
            self.standard_bag_coverage
                .as_mut()
                .expect("symbolic owner")
                .merge_global(product.root)?;
            self.retain_buildable_tiling(tiling)?;
        }
        // This is an exact cache policy: commit every accepted original ID
        // before reusing either numeric arena. It never limits candidates,
        // source draws, physical paths, or the geometry completion criterion.
        const CACHE_LIVE_BYTES: usize = 32 * 1024 * 1024;
        if self
            .piece_order_languages
            .local_live_bytes()
            .saturating_add(
                self.standard_bag_coverage
                    .as_ref()
                    .expect("symbolic owner")
                    .local_live_bytes(),
            )
            > CACHE_LIVE_BYTES
        {
            self.ensure_symbolic_materialization_bound()?;
            self.prepare_standard_bag_resources()?;
            self.standard_bag_coverage
                .as_mut()
                .expect("symbolic owner")
                .flush_and_recycle_local_cache()?;
            self.piece_order_languages.clear_retain_capacity();
        }
        let result = self.ensure_memory_bound(0);
        span.finish(product.edge_checks as u64);
        result
    }

    fn prepare_standard_bag_resources(&mut self) -> Result<(), WasmExactSearchError> {
        let Some(coverage) = self.standard_bag_coverage.as_ref() else {
            return Ok(());
        };
        let outside = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_sub(coverage.retained_bytes() as u128))
            .and_then(|bytes| bytes.checked_add(self.coexisting_retained_bytes))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_symbolic_owner_projection_overflow",
            ))?;
        self.standard_bag_coverage
            .as_mut()
            .expect("symbolic owner")
            .set_resource_boundary(self.memory_bound, outside);
        self.standard_bag_coverage
            .as_mut()
            .expect("symbolic owner")
            .set_resource_owner(self.private_memory_owner.clone());
        Ok(())
    }

    fn ensure_symbolic_materialization_bound(&self) -> Result<(), WasmExactSearchError> {
        let pattern_count = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("admitted source")
            .pattern_count();
        let future =
            PatternBitSet::checked_external_words_materialize_union_future_bytes(pattern_count)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_symbolic_coverage_projection_overflow",
                ))?;
        self.ensure_memory_bound(future)
    }

    fn finalize_standard_bag_coverage(&mut self) -> Result<(), WasmExactSearchError> {
        if self.standard_bag_coverage.is_none() {
            return Ok(());
        }
        self.ensure_symbolic_materialization_bound()?;
        self.prepare_standard_bag_resources()?;
        let coverage = self
            .standard_bag_coverage
            .as_mut()
            .expect("symbolic owner")
            .materialize_global()?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_coverage_universe_mismatch")
        })?;
        drop(coverage);
        self.ensure_memory_bound(0)
    }

    fn retain_buildable_tiling(
        &mut self,
        tiling: ExtendedTilingKey,
    ) -> Result<(), WasmExactSearchError> {
        if self.buildable_tilings.contains(&tiling) {
            return Ok(());
        }
        let future = checked_hash_growth_future_bytes(
            self.buildable_tilings.capacity(),
            self.buildable_tilings.len(),
            core::mem::size_of::<ExtendedTilingKey>(),
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "extended_pc_memory_projection_unavailable",
        ))?;
        self.ensure_memory_bound(future)?;
        if self.buildable_tilings.try_reserve(1).is_err() {
            self.truncated_reason = Some("solution_storage_capacity_exceeded");
            return Ok(());
        }
        self.buildable_tilings.insert(tiling);
        Ok(())
    }

    fn merge_solution_coverage(
        &mut self,
        candidate_key: &str,
        coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let retained_map =
            self.solution_coverage
                .as_ref()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_not_requested",
                ))?;
        if !retained_map.contains_key(candidate_key) {
            let future = checked_hash_growth_future_bytes(
                retained_map.capacity(),
                retained_map.len(),
                core::mem::size_of::<(String, PatternBitSet)>(),
            )
            .and_then(|bytes| bytes.checked_add(candidate_key.len() as u128))
            .and_then(|bytes| bytes.checked_add(2 * coverage.retained_bytes() as u128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
            self.ensure_memory_bound(future)?;
        }
        let solution_coverage =
            self.solution_coverage
                .as_mut()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_not_requested",
                ))?;
        if !solution_coverage.contains_key(candidate_key) {
            solution_coverage.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_storage_unavailable",
                )
            })?;
        }
        solution_coverage
            .entry(candidate_key.to_owned())
            .or_insert_with(|| PatternBitSet::new(coverage.pattern_count()))
            .union_with(coverage)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_universe_mismatch",
                )
            })
    }

    // The browser coordinator uses the external-memory-guard form so retained
    // worker payloads participate in admission.
    pub fn advance_distributed_geometry(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.advance_distributed_geometry_with_candidate_memory_guard(
            pass_index,
            control,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_candidate_row_storage_projection_overflow",
                    ))?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_candidate_row_storage_projection_overflow",
                    ))?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    pub(super) fn advance_distributed_geometry_with_candidate_memory_guard(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_geometry_state_invalid",
            ));
        }
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        if self.trivial_target {
            return Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(None),
            ));
        }
        let max_candidates = self.problem.backend_request().max_candidates();
        if max_candidates != 0 && self.geometry.candidate_count() >= max_candidates {
            self.truncated_reason = Some("candidate_budget_exceeded");
            return Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(self.truncated_reason),
            ));
        }
        match self.advance_geometry_bounded()? {
            ExtendedGeometryAdvance::Pending => Ok(WasmCandidateProducerAdvance::Pending),
            ExtendedGeometryAdvance::Candidate(candidate) => {
                let row_ids = self.try_copy_distributed_candidate_row_ids_with_memory_guard(
                    candidate.row_ids(),
                    &mut memory_guard,
                )?;
                let ordinal = self.geometry.candidate_count().saturating_sub(1) as u64;
                let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
                self.candidate_digest = mix_digest(self.candidate_digest, tiling.digest());
                Ok(WasmCandidateProducerAdvance::Candidate(
                    WasmCandidatePacket::for_extended_pass(ordinal, pass_index, row_ids),
                ))
            }
            ExtendedGeometryAdvance::Complete => Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(None),
            )),
            ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                self.truncated_reason = Some(reason);
                Ok(WasmCandidateProducerAdvance::Completed(
                    self.distributed_geometry_summary(Some(reason)),
                ))
            }
        }
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn try_copy_distributed_candidate_row_ids(
        &self,
        source: &[u32],
    ) -> Result<Vec<u32>, WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.try_copy_distributed_candidate_row_ids_with_memory_guard(
            source,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_candidate_row_storage_projection_overflow",
                    ))?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_candidate_row_storage_projection_overflow",
                    ))?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    fn try_copy_distributed_candidate_row_ids_with_memory_guard(
        &self,
        source: &[u32],
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<Vec<u32>, WasmExactSearchError> {
        let requested_bytes = (source.len() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_candidate_row_storage_projection_overflow",
            ))?;
        memory_guard(self, 0, requested_bytes)?;

        let mut row_ids = Vec::new();
        row_ids.try_reserve_exact(source.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_candidate_row_storage_unavailable")
        })?;
        let actual_bytes = (row_ids.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_candidate_row_storage_projection_overflow",
            ))?;
        memory_guard(self, actual_bytes, 0)?;
        row_ids.extend_from_slice(source);
        Ok(row_ids)
    }

    fn distributed_geometry_summary(
        &self,
        truncated_reason: Option<&'static str>,
    ) -> WasmDistributedGeometrySummary {
        WasmDistributedGeometrySummary {
            candidate_count: self.geometry.candidate_count(),
            candidate_digest: self.candidate_digest,
            candidate_family_count: self.geometry.candidate_family_count(),
            expanded_nodes: self.geometry.expanded_nodes(),
            peak_frontier: self.geometry.peak_frontier(),
            domain_pruned_states: self.geometry.domain_pruned_states(),
            hall_pruned_states: self.geometry.hall_pruned_states(),
            column_pruned_states: self.geometry.column_pruned_states(),
            component_compositions: self.geometry.component_compositions(),
            truncated_reason,
            backend_execution: WasmDistributedBackendExecution::Cpu,
        }
    }

    pub fn prepare_distributed_finalizer(&mut self) {
        self.workers_used = 1;
        self.parallel_active_workers = 0;
        self.parallel_minimum_worker_candidates = usize::MAX;
        self.parallel_maximum_worker_candidates = 0;
        self.distributed_worker_memory_bytes = 0;
        self.distributed_execution_constraint_materialized =
            self.problem.objective().execution_constraints().requested();
    }

    pub fn process_external_candidate(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if !self.external_geometry || self.finished || !candidate.is_extended() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_candidate_kind_invalid",
            ));
        }
        let geometry = self
            .geometry
            .external_candidate(&self.catalog, candidate.row_ids())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_candidate_invalid",
            ))?;
        self.process_candidate(geometry, Some(candidate.ordinal()), control)?;
        self.ensure_memory_bound(0)
    }

    pub fn complete_distributed_worker(
        &mut self,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if !self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_verifier_state_invalid",
            ));
        }
        self.ensure_result_materialization_bound()?;
        self.finished = true;
        let result = self.build_result()?;
        self.ensure_materialized_result_bound(&result)?;
        Ok(result)
    }

    // Browser merging uses the coordinator-owned memory guard; retain this
    // convenience adapter for native/parity callers.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.absorb_distributed_result_with_memory_guard(
            result,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or_else(|| {
                        WasmExactSearchError::resource_admission(
                            memory_bound.ensure(u128::MAX, 1).expect_err(
                                "checked extended absorb storage overflow is unavailable",
                            ),
                        )
                    })?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or_else(|| {
                        WasmExactSearchError::resource_admission(
                            memory_bound.ensure(u128::MAX, 1).expect_err(
                                "checked extended absorb future overflow is unavailable",
                            ),
                        )
                    })?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    pub(super) fn absorb_distributed_result_with_memory_guard(
        &mut self,
        result: &CoreExecutionResult,
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        memory_guard(self, 0, 0)?;
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_merger_state_invalid",
            ));
        }
        let pattern_count = exact_usize_field(
            result,
            "coverage_pattern_count",
            "wasm_extended_distributed_pattern_count_invalid",
        )?;
        let partial = self.validate_distributed_solution_surface(result, pattern_count)?;
        let coverage_future =
            PatternBitSet::checked_external_words_materialize_union_future_bytes(pattern_count)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_coverage_projection_overflow",
                ))?;
        memory_guard(self, 0, coverage_future)?;
        let mut coverage_words = Vec::new();
        coverage_words
            .try_reserve_exact(result.coverage_pattern_words().len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_coverage_storage_unavailable",
                )
            })?;
        let coverage_word_bytes = (coverage_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_coverage_projection_overflow",
            ))?;
        memory_guard(self, coverage_word_bytes, coverage_future)?;
        coverage_words.extend_from_slice(result.coverage_pattern_words());
        let coverage = PatternBitSet::from_words(pattern_count, coverage_words).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_distributed_coverage_invalid")
        })?;
        let coverage_retained_bytes = coverage.checked_storage_retained_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_coverage_projection_overflow",
            ),
        )?;
        memory_guard(self, coverage_retained_bytes, coverage_future)?;
        if result
            .coverage_pattern_words()
            .iter()
            .copied()
            .enumerate()
            .any(|(word_index, word)| coverage.word_at(word_index) != word)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_coverage_invalid",
            ));
        }
        validate_distributed_coverage_aggregation_surface(
            &self.problem,
            self.aggregation,
            result,
            &coverage,
            partial.coverage_source_row_count,
            partial.probability_complete,
        )?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_distributed_coverage_mismatch")
        })?;
        memory_guard(self, coverage_retained_bytes, 0)?;
        drop(coverage);
        memory_guard(self, 0, 0)?;
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &= result
                .bool_field("execution_constraint_materialized")
                .unwrap_or(false);
        }

        for source_key in result.normalized_solution_keys() {
            if self.distributed_solution_keys.contains(source_key) {
                continue;
            }
            let requested_key_bytes = (core::mem::size_of::<String>() as u128)
                .checked_add(source_key.len() as u128)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_projection_overflow",
                ))?;
            memory_guard(self, 0, requested_key_bytes)?;
            let mut key = String::new();
            key.try_reserve_exact(source_key.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_storage_unavailable",
                )
            })?;
            let actual_key_bytes = key.capacity() as u128;
            memory_guard(
                self,
                actual_key_bytes,
                core::mem::size_of::<String>() as u128,
            )?;
            key.push_str(source_key);
            self.distributed_solution_keys.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_storage_unavailable",
                )
            })?;
            memory_guard(self, actual_key_bytes, 0)?;
            self.distributed_solution_keys.insert(key);
            memory_guard(self, 0, 0)?;
        }
        if self.solution_coverage.is_some() {
            for coverage in result.normalized_solution_coverages() {
                self.merge_solution_coverage_with_memory_guard(
                    coverage.solution_key(),
                    coverage.covered_patterns(),
                    &mut memory_guard,
                )?;
            }
        }

        let worker_candidates = result.usize_field("packing_candidate_count").unwrap_or(0);
        if worker_candidates != 0 {
            self.parallel_active_workers = self.parallel_active_workers.saturating_add(1);
            self.parallel_minimum_worker_candidates = self
                .parallel_minimum_worker_candidates
                .min(worker_candidates);
            self.parallel_maximum_worker_candidates = self
                .parallel_maximum_worker_candidates
                .max(worker_candidates);
        }
        self.searched_build_nodes = self
            .searched_build_nodes
            .saturating_add(result.usize_field("buildup_searched_nodes").unwrap_or(0));
        self.reachability_states = self
            .reachability_states
            .saturating_add(result.usize_field("total_reachability_states").unwrap_or(0));
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(result.usize_field("coverage_product_states").unwrap_or(0));
        self.coverage_product_edge_checks = self.coverage_product_edge_checks.saturating_add(
            result
                .usize_field("coverage_product_edge_checks")
                .unwrap_or(0),
        );
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(result.usize_field("coverage_product_words").unwrap_or(0));
        self.peak_build_order_nodes = self
            .peak_build_order_nodes
            .max(result.usize_field("peak_build_order_nodes").unwrap_or(0));
        self.total_build_order_nodes = self
            .total_build_order_nodes
            .saturating_add(result.usize_field("total_build_order_nodes").unwrap_or(0));
        self.witnessed_pattern_count = self.witnessed_pattern_count.saturating_add(
            result
                .field("witnessed_pattern_count")
                .and_then(|value| value.parse::<u128>().ok())
                .unwrap_or(0),
        );
        self.distributed_worker_memory_bytes = self
            .distributed_worker_memory_bytes
            .saturating_add(result.usize_field("resource_peak_cpu_bytes").unwrap_or(0));
        if let Some(rank) = result
            .field("representative_candidate_ordinal")
            .and_then(|value| value.parse::<u64>().ok())
        {
            if self
                .representative_rank
                .is_none_or(|current| rank < current)
            {
                self.representative_rank = Some(rank);
                self.representative_pattern_id = result
                    .field("representative_pattern_id")
                    .and_then(|value| value.parse::<u32>().ok());
                let requested_path_bytes = (result.path_steps().len() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_distributed_path_projection_overflow",
                    ))?;
                memory_guard(self, 0, requested_path_bytes)?;
                let mut representative_path = Vec::new();
                representative_path
                    .try_reserve_exact(result.path_steps().len())
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_distributed_path_storage_unavailable",
                        )
                    })?;
                let actual_path_bytes = (representative_path.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_distributed_path_projection_overflow",
                    ))?;
                memory_guard(self, actual_path_bytes, 0)?;
                representative_path.extend_from_slice(result.path_steps());
                memory_guard(self, actual_path_bytes, 0)?;
                self.representative_path = representative_path;
                memory_guard(self, 0, 0)?;
            }
        }
        self.distributed_count_complete &= partial.count_complete;
        self.distributed_probability_complete &= partial.probability_complete;
        if partial.resource_truncated {
            self.truncated_reason = Some("distributed_worker_incomplete");
            self.distributed_count_complete = false;
            self.distributed_probability_complete = false;
        }
        memory_guard(self, 0, 0)
    }

    fn merge_solution_coverage_with_memory_guard(
        &mut self,
        candidate_key: &str,
        coverage: &PatternBitSet,
        memory_guard: &mut impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        let is_new = !self
            .solution_coverage
            .as_ref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_solution_coverage_not_requested",
            ))?
            .contains_key(candidate_key);
        let union_future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            coverage.pattern_count(),
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_extended_solution_coverage_projection_overflow",
        ))?;
        let mut requested_future = union_future;
        if is_new {
            requested_future = requested_future
                .checked_add(
                    (core::mem::size_of::<String>() + core::mem::size_of::<PatternBitSet>())
                        as u128,
                )
                .and_then(|bytes| bytes.checked_add(candidate_key.len() as u128))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_projection_overflow",
                ))?;
        }
        memory_guard(self, 0, requested_future)?;

        if is_new {
            let mut owned_key = String::new();
            owned_key
                .try_reserve_exact(candidate_key.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_solution_coverage_storage_unavailable",
                    )
                })?;
            let actual_key_bytes = owned_key.capacity() as u128;
            memory_guard(
                self,
                actual_key_bytes,
                (core::mem::size_of::<String>() + core::mem::size_of::<PatternBitSet>()) as u128,
            )?;
            owned_key.push_str(candidate_key);
            {
                let map = self
                    .solution_coverage
                    .as_mut()
                    .expect("the requested solution coverage map exists");
                map.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_solution_coverage_storage_unavailable",
                    )
                })?;
            }
            memory_guard(self, actual_key_bytes, union_future)?;
            self.solution_coverage
                .as_mut()
                .expect("the requested solution coverage map exists")
                .insert(owned_key, PatternBitSet::new(coverage.pattern_count()));
            memory_guard(self, 0, union_future)?;
        }
        self.solution_coverage
            .as_mut()
            .expect("the requested solution coverage map exists")
            .get_mut(candidate_key)
            .expect("the requested solution coverage entry exists")
            .union_with(coverage)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_solution_coverage_universe_mismatch",
                )
            })?;
        memory_guard(self, 0, 0)
    }

    fn validate_distributed_solution_surface(
        &self,
        result: &CoreExecutionResult,
        pattern_count: usize,
    ) -> Result<ExtendedDistributedPartial, WasmExactSearchError> {
        if pattern_count != self.covered_patterns.pattern_count() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_pattern_count_mismatch",
            ));
        }
        let requested = exact_solution_probabilities_requested(result)?;
        if requested != self.problem.solution_probability_policy().requested() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probability_policy_mismatch",
            ));
        }
        let count_complete = exact_bool_field(
            result,
            "count_complete",
            "wasm_extended_distributed_count_complete_invalid",
        )?;
        let probability_complete = exact_bool_field(
            result,
            "probability_complete",
            "wasm_extended_distributed_probability_complete_invalid",
        )?;
        let resource_truncated = exact_bool_field(
            result,
            "resource_truncated",
            "wasm_extended_distributed_resource_truncated_invalid",
        )?;
        let worker_solution_count = exact_usize_field(
            result,
            "unique_solution_count",
            "wasm_extended_distributed_solution_count_invalid",
        )?;
        if self.build_constraints_materialized()
            && (result.unique_field("search_kind") != Some("build-probability")
                || result.unique_field("execution_constraint_preserve_b2b") != Some("true")
                || result.unique_field("execution_constraint_materialized") != Some("true")
                || result.unique_field("b2b_preservation_evaluation_basis")
                    != Some("filtered-build-order-language")
                || result.unique_field("b2b_preservation_selection") != Some("existential")
                || result.unique_field("b2b_preservation_denominator_semantics")
                    != Some("original-materialized-queue")
                || result.unique_field("b2b_preservation_path_multiplicity_counted")
                    != Some("false")
                || result.unique_field("execution_constraint_spin_profile")
                    != Some(
                        self.problem
                            .objective()
                            .execution_constraints()
                            .spin_profile()
                            .as_str(),
                    )
                || exact_usize_field(
                    result,
                    "b2b_preserving_solution_count",
                    "wasm_extended_distributed_filtered_constraint_count_invalid",
                )? != worker_solution_count
                || exact_usize_field(
                    result,
                    "b2b_preservation_pattern_universe_count",
                    "wasm_extended_distributed_filtered_constraint_universe_invalid",
                )? != pattern_count
                || exact_bool_field(
                    result,
                    "b2b_preservation_count_complete",
                    "wasm_extended_distributed_filtered_constraint_complete_invalid",
                )? != count_complete
                || exact_bool_field(
                    result,
                    "b2b_preservation_probability_complete",
                    "wasm_extended_distributed_filtered_constraint_probability_invalid",
                )? != probability_complete
                || result.unique_field("b2b_preserving_pattern_count")
                    != result.unique_field("covered_pattern_count")
                || result.unique_field("b2b_preservation_probability")
                    != result.unique_field("coverage_probability"))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_filtered_constraint_mismatch",
            ));
        }
        let coverage_source_row_count = validate_distributed_coverage_authority(
            &self.problem,
            self.aggregation,
            result,
            pattern_count,
            probability_complete,
            self.supply_projection_complete && count_complete && !resource_truncated,
        )?;
        if !result.normalized_solution_identities().is_empty()
            || !result.solution_coverages().is_empty()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_compact_solution_surface_forbidden",
            ));
        }

        let keys = result.normalized_solution_keys();
        if worker_solution_count != keys.len()
            || keys.iter().any(String::is_empty)
            || !keys.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_keys_incomplete",
            ));
        }
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let family = universe.packing_multiset_family_for_execution(
            self.field.target_piece_count(),
            self.problem.initial_hold(),
            self.problem.supply().hold_enabled(),
            super::packing_hold_projection(&self.problem),
        );
        for key in keys {
            self.validate_distributed_solution_key(key, &family)?;
        }

        let coverages = result.normalized_solution_coverages();
        let solution_coverage_required = self.solution_coverage.is_some();
        if !coverages
            .windows(2)
            .all(|pair| pair[0].solution_key() < pair[1].solution_key())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_coverage_not_canonical",
            ));
        }
        if solution_coverage_required && coverages.len() != worker_solution_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_coverage_incomplete",
            ));
        }
        if !solution_coverage_required && !coverages.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_unexpected_solution_coverage",
            ));
        }
        for coverage in coverages {
            if keys
                .binary_search_by(|key| key.as_str().cmp(coverage.solution_key()))
                .is_err()
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_coverage_foreign_key",
                ));
            }
            if coverage.covered_patterns().pattern_count() != pattern_count {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_coverage_shape_mismatch",
                ));
            }
        }
        if solution_coverage_required
            && !solution_coverage_union_matches_global(
                pattern_count,
                result.coverage_pattern_words(),
                coverages,
                NormalizedSolutionCoverage::covered_patterns,
            )
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_coverage_union_mismatch",
            ));
        }
        validate_worker_partial_probability_surface(result)?;
        if coverage_source_row_count != worker_solution_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_source_count_mismatch",
            ));
        }

        Ok(ExtendedDistributedPartial {
            count_complete,
            probability_complete,
            resource_truncated,
            coverage_source_row_count,
        })
    }

    fn validate_distributed_solution_key(
        &self,
        key: &str,
        family: &PackingMultisetFamily,
    ) -> Result<(), WasmExactSearchError> {
        let identity = ExtendedTilingSolutionKey::parse_canonical(key).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_encoding_invalid",
            )
        })?;
        if identity.height() != self.field.height() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_header_invalid",
            ));
        }
        if identity.initial_board().words() != self.catalog.initial_board().words() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_initial_board_mismatch",
            ));
        }

        let mut covered = super::extended_board::ExtendedBoard::EMPTY;
        let mut piece_counts = [0_u8; 7];
        let mut piece_count = 0_usize;
        for placement in identity.placements() {
            let piece = placement.piece();
            let cells = super::extended_board::ExtendedBoard::from_words(placement.cells().words());
            let mut matches = self
                .catalog
                .skeletons()
                .iter()
                .filter(|row| row.piece == piece && row.cells == cells);
            let row = matches
                .next()
                .copied()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_placement_not_in_catalog",
                ))?;
            if matches.next().is_some() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_catalog_collision",
                ));
            }
            if covered.intersects(row.cells)
                || !row.cells.is_subset_of(self.catalog.required_cells())
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_key_overlap_or_domain_mismatch",
                ));
            }
            covered = covered.union(row.cells);
            let count = piece_counts
                .get_mut(super::piece_index(piece))
                .expect("every standard tetromino has a count slot");
            *count = count
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_key_piece_count_mismatch",
                ))?;
            piece_count =
                piece_count
                    .checked_add(1)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_distributed_solution_key_piece_count_mismatch",
                    ))?;
        }
        if piece_count != self.field.target_piece_count() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_key_piece_count_mismatch",
            ));
        }
        if covered != self.catalog.required_cells() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_key_target_mismatch",
            ));
        }
        let multiset = PieceMultisetKey::from_counts(piece_counts);
        if self.field.target_piece_count() != 0 && family.pattern_bits(multiset).is_none() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_key_supply_mismatch",
            ));
        }
        Ok(())
    }

    pub fn complete_distributed_geometry(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if self.external_geometry
            || self.finished
            || summary.candidate_count != self.geometry.candidate_count()
            || summary.candidate_digest != self.candidate_digest
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_geometry_summary_mismatch",
            ));
        }
        self.workers_used = workers_used.max(1);
        if self.parallel_minimum_worker_candidates == usize::MAX {
            self.parallel_minimum_worker_candidates = 0;
        }
        if let Some(reason) = summary.truncated_reason {
            self.truncated_reason = Some(reason);
        }
        self.complete()
    }

    pub(super) fn annotate_distributed_finesse(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.rebuild_distributed_solution_evidence(control, true)
    }

    pub(super) fn annotate_distributed_execution_constraints(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.build_constraints_materialized() {
            // Every verifier already filtered the physical lock language before
            // coverage. Rebuilding from keys would repeat that work and restore
            // the source-sized indexes the symbolic producer deliberately omits.
            return if control.is_cancelled() {
                Err(WasmExactSearchError::Cancelled)
            } else {
                Ok(())
            };
        }
        self.rebuild_distributed_solution_evidence(control, false)
    }

    fn build_constraints_materialized(&self) -> bool {
        self.purpose == ExtendedFamilyPurpose::Build
            && self.aggregation == BuildProbabilityAggregation::Buildability
            && self.problem.objective().execution_constraints().requested()
    }

    // A raw worker's oracle coverage cannot authorize VisibleSeven. Rebuild
    // only the observation language over accepted canonical keys. Existing
    // path/coverage/count evidence is retained and must not be tallied twice.
    pub(super) fn rebuild_distributed_observation_language(
        &mut self,
        reconstruction: &ExtendedGeometrySearch,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.purpose != ExtendedFamilyPurpose::Pc || self.pc_observation.is_none() {
            return Ok(());
        }
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_reconstruction_state_invalid",
            ));
        }
        let mut future = (self.distributed_solution_keys.len() as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        for key in &self.distributed_solution_keys {
            future = future.checked_add(key.len() as u128).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "extended_pc_observation_memory_projection_overflow",
                ),
            )?;
        }
        self.ensure_memory_bound(future)?;
        let mut keys = Vec::new();
        keys.try_reserve_exact(self.distributed_solution_keys.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "extended_pc_observation_key_storage_unavailable",
                )
            })?;
        for key in &self.distributed_solution_keys {
            let mut owned = String::new();
            owned.try_reserve_exact(key.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "extended_pc_observation_key_storage_unavailable",
                )
            })?;
            owned.push_str(key);
            keys.push(owned);
        }
        keys.sort_unstable();
        let scratch = keys
            .iter()
            .try_fold(
                keys.capacity() as u128 * core::mem::size_of::<String>() as u128,
                |bytes, key| bytes.checked_add(key.capacity() as u128),
            )
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        let original_coexisting = self.coexisting_retained_bytes;
        self.coexisting_retained_bytes = original_coexisting.checked_add(scratch).ok_or(
            WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ),
        )?;
        self.pc_observation
            .as_mut()
            .expect("observation owner exists")
            .reset();
        let result = (|| {
            for key in keys {
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                // Parsing a canonical key owns placement and row vectors.
                // Credit their coexistence before parsing, not after it.
                let identity_future = (self.field.target_piece_count() as u128)
                    .checked_mul(128)
                    .and_then(|bytes| bytes.checked_add(2 * key.len() as u128))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_memory_projection_overflow",
                    ))?;
                self.ensure_memory_bound(identity_future)?;
                let rows =
                    extended_row_ids_from_canonical_key(&key, self.field.height(), &self.catalog)?;
                let candidate = reconstruction
                    .external_candidate(&self.catalog, &rows)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_reconstruction_not_in_family",
                    ))?;
                let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
                if tiling.canonical_key(self.catalog.initial_board(), self.field.height()) != key {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_reconstruction_identity_mismatch",
                    ));
                }
                let local = (rows.capacity() as u128)
                    .checked_mul(core::mem::size_of::<u32>() as u128)
                    .and_then(|bytes| bytes.checked_add(tiling.retained_bytes() as u128))
                    .and_then(|bytes| bytes.checked_add(key.capacity() as u128))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_memory_projection_overflow",
                    ))?;
                self.coexisting_retained_bytes = original_coexisting
                    .checked_add(scratch)
                    .and_then(|bytes| bytes.checked_add(local))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_memory_projection_overflow",
                    ))?;
                // Boolean coverage still uses exactly the original PC/B2B
                // language, not an unconstrained generic Build graph.
                self.prepare_build_order_resources()?;
                let graph = match build_extended_order_graph_with_constraints(
                    &self.problem,
                    &self.catalog,
                    &candidate,
                    &mut self.build_order_workspace,
                    0,
                    None,
                    control,
                )? {
                    ExtendedBuildOrderResult::Complete { graph, .. } if graph.is_live() => graph,
                    _ => {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "extended_pc_observation_reconstruction_incomplete",
                        ))
                    }
                };
                let insert_future = self
                    .pc_observation
                    .as_ref()
                    .expect("observation owner exists")
                    .checked_graph_insert_future_bytes(&graph)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_memory_projection_overflow",
                    ))?;
                self.ensure_memory_bound(
                    (graph.retained_bytes() as u128)
                        .checked_add(insert_future)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "extended_pc_observation_memory_projection_overflow",
                        ))?,
                )?;
                self.pc_observation
                    .as_mut()
                    .expect("observation owner exists")
                    .insert_graph(&graph)?;
                self.coexisting_retained_bytes = original_coexisting.checked_add(scratch).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "extended_pc_observation_memory_projection_overflow",
                    ),
                )?;
            }
            Ok(())
        })();
        self.coexisting_retained_bytes = original_coexisting;
        result
    }

    fn rebuild_distributed_solution_evidence(
        &mut self,
        control: &ExecutionControl,
        collect_finesse: bool,
    ) -> Result<(), WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_evidence_annotation_state_invalid",
            ));
        }
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let family = universe.packing_multiset_family_for_execution(
            self.field.target_piece_count(),
            self.problem.initial_hold(),
            self.problem.supply().hold_enabled(),
            super::packing_hold_projection(&self.problem),
        );
        let reconstruction_other = self.checked_retained_bytes_with_coexisting_owners().ok_or(
            WasmExactSearchError::InvalidProblem("extended_geometry_memory_projection_unavailable"),
        )?;
        let mut reconstruction = ExtendedGeometrySearch::new_bounded(
            universe,
            &family,
            &self.catalog,
            self.memory_bound,
            reconstruction_other,
        )?;
        if !reconstruction.prepare_external() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_reconstruction_targets_unavailable",
            ));
        }
        let key_future = self
            .distributed_solution_keys
            .iter()
            .try_fold(
                self.distributed_solution_keys.len() as u128
                    * core::mem::size_of::<String>() as u128,
                |bytes, key| bytes.checked_add(key.len() as u128),
            )
            .and_then(|bytes| bytes.checked_add(reconstruction.retained_bytes() as u128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_geometry_memory_projection_unavailable",
            ))?;
        self.ensure_memory_bound(key_future)?;
        let mut solution_keys = Vec::new();
        solution_keys
            .try_reserve_exact(self.distributed_solution_keys.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_storage_unavailable",
                )
            })?;
        for key in &self.distributed_solution_keys {
            let mut owned = String::new();
            owned.try_reserve_exact(key.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_storage_unavailable",
                )
            })?;
            owned.push_str(key);
            solution_keys.push(owned);
        }
        solution_keys.sort_unstable();
        let scratch = solution_keys
            .iter()
            .try_fold(
                solution_keys.capacity() as u128 * core::mem::size_of::<String>() as u128,
                |bytes, key| bytes.checked_add(key.capacity() as u128),
            )
            .and_then(|bytes| bytes.checked_add(reconstruction.retained_bytes() as u128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_geometry_memory_projection_unavailable",
            ))?;
        let original_coexisting = self.coexisting_retained_bytes;
        self.coexisting_retained_bytes = original_coexisting.checked_add(scratch).ok_or(
            WasmExactSearchError::InvalidProblem("extended_geometry_memory_projection_unavailable"),
        )?;
        self.reset_distributed_evidence_aggregation(collect_finesse);
        let result = (|| {
            for (ordinal, solution_key) in solution_keys.into_iter().enumerate() {
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                let identity_future = (self.field.target_piece_count() as u128)
                    .checked_mul(128)
                    .and_then(|bytes| bytes.checked_add(2 * solution_key.len() as u128))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_geometry_memory_projection_unavailable",
                    ))?;
                self.ensure_memory_bound(identity_future)?;
                let row_ids = extended_row_ids_from_canonical_key(
                    &solution_key,
                    self.field.height(),
                    &self.catalog,
                )?;
                let candidate = reconstruction
                    .external_candidate(&self.catalog, &row_ids)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_finesse_solution_key_not_in_catalog",
                    ))?;
                let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
                if tiling.canonical_key(self.catalog.initial_board(), self.field.height())
                    != solution_key
                {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_finesse_solution_key_reconstruction_mismatch",
                    ));
                }
                let spin_candidate = (self.aggregation.requests_spin_coverage()
                    || (self.problem.objective().execution_constraints().requested()
                        && (!self.build_constraints_materialized() || collect_finesse)))
                    .then(|| (tiling.digest(), solution_key.clone()));
                let local = row_ids.capacity() as u128 * core::mem::size_of::<u32>() as u128
                    + tiling.retained_bytes() as u128
                    + 2 * solution_key.capacity() as u128;
                self.coexisting_retained_bytes = original_coexisting
                    .checked_add(scratch)
                    .and_then(|bytes| bytes.checked_add(local))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_geometry_memory_projection_unavailable",
                    ))?;
                self.prepare_build_order_resources()?;
                let build = if collect_finesse {
                    build_extended_order_graph_with_finesse(
                        &self.problem,
                        &self.catalog,
                        &candidate,
                        &mut self.build_order_workspace,
                        usize::MAX,
                        spin_candidate,
                        self.aggregation.requests_spin_coverage(),
                        control,
                    )
                } else {
                    build_extended_order_graph_with_constraints(
                        &self.problem,
                        &self.catalog,
                        &candidate,
                        &mut self.build_order_workspace,
                        usize::MAX,
                        spin_candidate,
                        control,
                    )
                }?;
                self.apply_build_order_result(
                    &candidate,
                    tiling,
                    Some(solution_key),
                    Some(ordinal as u64),
                    build,
                    control,
                )?;
                self.coexisting_retained_bytes = original_coexisting.checked_add(scratch).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "extended_geometry_memory_projection_unavailable",
                    ),
                )?;
            }
            Ok(())
        })();
        self.coexisting_retained_bytes = original_coexisting;
        result
    }

    fn reset_distributed_evidence_aggregation(&mut self, collect_finesse: bool) {
        if let Some(observation) = self.pc_observation.as_mut() {
            observation.reset();
        }
        self.finesse_requested = collect_finesse;
        self.covered_patterns = if self.trivial_target {
            PatternBitSet::all(self.covered_patterns.pattern_count())
        } else {
            PatternBitSet::new(self.covered_patterns.pattern_count())
        };
        self.buildable_tilings.clear();
        if let Some(solution_coverage) = self.solution_coverage.as_mut() {
            solution_coverage.clear();
        }
        self.spin_execution_graphs.clear();
        self.distributed_solution_keys.clear();
        self.finesse_languages.clear();
        self.searched_build_nodes = 0;
        self.reachability_states = 0;
        self.coverage_product_states = 0;
        self.coverage_product_edge_checks = 0;
        self.coverage_product_words = 0;
        self.peak_build_order_nodes = 0;
        self.total_build_order_nodes = 0;
        self.peak_build_scratch_bytes = 0;
        self.witnessed_pattern_count = 0;
        self.representative_path.clear();
        self.representative_pattern_id = None;
        self.representative_rank = None;
        self.distributed_execution_constraint_materialized = false;
        self.distributed_count_complete = true;
        self.distributed_probability_complete = true;
    }

    pub(super) fn finalize_pc_observation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.external_geometry
            || self
                .pc_observation
                .as_ref()
                .is_none_or(|owner| owner.evaluated())
        {
            return Ok(());
        }
        let owned = self
            .pc_observation
            .as_ref()
            .expect("unevaluated observation owner exists")
            .checked_retained_bytes()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        let mut observation = self
            .pc_observation
            .take()
            .expect("unevaluated observation owner exists");
        let problem = Arc::clone(&self.problem);
        let mut admitted_peak = 0_u128;
        let result = observation.evaluate_with_memory_guard(&problem, control, |future| {
            let future = owned.saturating_add(future);
            self.ensure_memory_bound(future)?;
            let peak = self
                .checked_retained_bytes_with_coexisting_owners()
                .and_then(|bytes| bytes.checked_add(future))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "extended_pc_observation_memory_projection_overflow",
                ))?;
            admitted_peak = admitted_peak.max(peak);
            Ok(())
        });
        self.pc_observation = Some(observation);
        let coverage = result?;
        let observation = self
            .pc_observation
            .as_ref()
            .expect("observation owner restored on every result");
        self.ensure_memory_bound(observation.evaluation_peak_bytes())?;
        let peak = self
            .checked_retained_bytes_with_coexisting_owners()
            .and_then(|bytes| bytes.checked_add(observation.evaluation_peak_bytes()))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        self.native_parallel_peak_bytes =
            self.native_parallel_peak_bytes.max(peak).max(admitted_peak);
        if let Some(coverage) = coverage {
            self.covered_patterns = coverage.covered_patterns;
        }
        Ok(())
    }

    fn complete_with_control(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.finalize_pc_observation(control)?;
        self.complete()
    }

    fn complete(&mut self) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.ensure_result_materialization_bound()?;
        self.finished = true;
        let result = self.build_result()?;
        self.ensure_materialized_result_bound(&result)?;
        Ok(BuildProbabilityAdvance::Completed(result))
    }

    fn build_result(&mut self) -> Result<CoreExecutionResult, WasmExactSearchError> {
        self.finalize_standard_bag_coverage()?;
        if self.purpose == ExtendedFamilyPurpose::Pc {
            return self.build_pc_family_result();
        }
        let tiling_only = self.aggregation.is_tiling_only();
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("extended build probability requires a materialized universe");
        let count_complete = self.supply_projection_complete
            && self.distributed_count_complete
            && self.truncated_reason.is_none();
        let coverage_source_row_count = self
            .buildable_tilings
            .len()
            .checked_add(self.distributed_solution_keys.len())
            .and_then(|count| count.checked_add(usize::from(self.trivial_target)))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_coverage_source_count_overflow",
            ))?;
        let coverage_aggregation = if tiling_only {
            None
        } else {
            Some(build_pattern_coverage_aggregation(
                &self.problem,
                coverage_source_row_count,
                &self.covered_patterns,
                PatternCoverageCompleteness::new(
                    self.supply_projection_complete,
                    self.distributed_count_complete
                        && self.truncated_reason.is_none()
                        && self.distributed_probability_complete,
                    true,
                ),
            )?)
        };
        let probability = coverage_aggregation.as_ref().map_or_else(
            || "not-calculated".to_owned(),
            |summary| {
                super::build_probability::probability_text(summary.success_probability().get())
            },
        );
        let probability_complete = coverage_aggregation
            .as_ref()
            .is_some_and(|summary| summary.completeness().is_complete());
        let execution_constraints = self.problem.objective().execution_constraints();
        let constraints_materialized = self.build_constraints_materialized()
            || self.distributed_execution_constraint_materialized;
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || (execution_constraints.requested() && !constraints_materialized);
        let spin_batch = if execution_evidence_requested {
            // Worker completion order is not a source ordinal. Preserve the
            // complete graph family and publish it in canonical physical order.
            self.spin_execution_graphs
                .sort_unstable_by(compare_spin_graphs);
            let patterns = (0..universe.pattern_count())
                .map(|pattern| universe.sequence_at(pattern).into_owned())
                .collect();
            let (kick_table_id, rule_profile_id) = replay_profile_ids(&self.problem);
            Some(SpinCoverageExecutionBatch::new(
                patterns,
                self.problem.initial_hold().cursor(),
                self.problem.initial_hold().hold_piece(),
                self.problem.supply().hold_enabled(),
                self.problem.supply().projects_unplaced_lookahead(),
                self.problem.supply().projects_standard_bag_lookahead(),
                kick_table_id,
                rule_profile_id,
                core::mem::take(&mut self.spin_execution_graphs),
                count_complete && probability_complete,
            ))
        } else {
            None
        };
        let source_sequence_length = universe.sequence_at(0).len();
        let mut normalized_keys = self
            .buildable_tilings
            .iter()
            .map(|tiling| tiling.canonical_key(self.catalog.initial_board(), self.field.height()))
            .collect::<Vec<_>>();
        normalized_keys.extend(self.distributed_solution_keys.iter().cloned());
        if self.trivial_target {
            normalized_keys.push(
                ExtendedTilingKey::empty()
                    .canonical_key(self.catalog.initial_board(), self.field.height()),
            );
        }
        normalized_keys.sort_unstable();
        normalized_keys.dedup();
        let mut normalized_solution_coverages = self
            .solution_coverage
            .as_ref()
            .map(|coverage| {
                coverage
                    .iter()
                    .map(|(key, patterns)| {
                        NormalizedSolutionCoverage::new(key.clone(), patterns.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        normalized_solution_coverages
            .sort_unstable_by(|left, right| left.solution_key().cmp(right.solution_key()));
        if self.trivial_target && self.solution_coverage.is_some() {
            let empty_key = normalized_keys
                .first()
                .expect("the canonical empty extended tiling is materialized")
                .clone();
            if !normalized_solution_coverages
                .iter()
                .any(|coverage| coverage.solution_key() == empty_key)
            {
                normalized_solution_coverages.push(NormalizedSolutionCoverage::new(
                    empty_key,
                    PatternBitSet::all(universe.pattern_count()),
                ));
                normalized_solution_coverages
                    .sort_unstable_by(|left, right| left.solution_key().cmp(right.solution_key()));
            }
        }
        let logical_target =
            super::extended_board::ExtendedBoard::from_mask(self.field.target_board());
        let mut completed_rows = 0_u32;
        let full_row = (1_u16 << self.field.width()) - 1;
        for row in 0..self.field.height() {
            if logical_target.row_bits(self.field.width(), row) == full_row {
                completed_rows |= 1_u32 << row;
            }
        }
        let final_board = compact_logical_board(
            self.field.width(),
            self.field.height(),
            logical_target,
            completed_rows,
        );
        let contract_storage = if self.field.height() <= 12 {
            "board128"
        } else {
            "board256"
        };
        let backend_requested = self.problem.backend_policy().requested_backend().as_str();
        let gpu_capability_requested = matches!(backend_requested, "gpu" | "hybrid");
        let hybrid_requested = backend_requested == "hybrid";
        let fields = vec![
            field("backend_requested", backend_requested),
            field("backend_selected", "wasm-cpu-build-probability-extended"),
            field("actual_backend", "wasm-cpu-build-probability-extended"),
            field(
                "backend_fallback_allowed",
                self.problem.backend_policy().allow_backend_fallback(),
            ),
            field("backend_fallback_used", false),
            field("fallback_used", false),
            field("backend_fallback_reason", "none"),
            field("fallback_backend", "none"),
            field("gpu_available", false),
            field(
                "gpu_disabled_reason",
                if gpu_capability_requested {
                    "gpu_kernel_unavailable"
                } else {
                    "not_requested"
                },
            ),
            field("gpu_trust_state", "not-used"),
            field(
                "hybrid_status",
                if hybrid_requested {
                    "cpu-selected"
                } else {
                    "not-requested"
                },
            ),
            field(
                "hybrid_disabled_reason",
                if hybrid_requested {
                    "gpu_kernel_unavailable"
                } else {
                    "not_requested"
                },
            ),
            field("workers_requested", self.problem.backend_policy().workers()),
            field("workers_used", self.workers_used),
            field("cpu_parallel_execution", self.workers_used > 1),
            field(
                "cpu_parallel_decision_reason",
                if self.workers_used > 1 && self.native_parallel_peak_bytes != 0 {
                    "native-shared-build-family-queue"
                } else if self.workers_used > 1 {
                    "browser-worker-build-probability-extended-pipeline"
                } else {
                    "serial-extended-build-probability"
                },
            ),
            field("cpu_parallel_active_workers", self.parallel_active_workers),
            field(
                "cpu_parallel_minimum_worker_candidates",
                self.parallel_minimum_worker_candidates,
            ),
            field(
                "cpu_parallel_maximum_worker_candidates",
                self.parallel_maximum_worker_candidates,
            ),
            field(
                "cpu_warmup_requested",
                self.problem.backend_policy().cpu_warmup(),
            ),
            field("cpu_warmup_performed", false),
            field(
                "supply_window_resolution",
                self.problem.supply().supply_window_resolution(),
            ),
            field(
                "projects_unplaced_lookahead",
                self.problem.supply().projects_unplaced_lookahead(),
            ),
            field(
                "projects_standard_bag_lookahead",
                self.problem.supply().projects_standard_bag_lookahead(),
            ),
            field("source_sequence_length", source_sequence_length),
            field(
                "total_possible_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field("search_kind", "build-probability"),
            field(
                "build_probability_completion",
                "exact-board-with-inverse-lock-clear",
            ),
            field("board_storage", "board256-canonical"),
            field("board_contract_storage", contract_storage),
            field(
                "geometry_state_storage",
                if self.geometry.uses_dense_static_geometry() {
                    "dense-u64-static-universe"
                } else {
                    "board256-residual"
                },
            ),
            field(
                "geometry_dense_cell_count",
                self.catalog
                    .dense_geometry()
                    .map_or(0, |dense| dense.cell_count()),
            ),
            field(
                "geometry_component_join",
                if self.geometry.component_compositions() != 0 {
                    "placement-hypergraph-piece-signature-dp"
                } else {
                    "not-applied"
                },
            ),
            field("board_height", self.field.height()),
            field("build_base_mask", words_hex(self.field.base_words())),
            field(
                "build_target_cells_mask",
                words_hex(self.field.target_words()),
            ),
            field(
                "build_target_board_mask",
                words_hex(self.field.target_board().words()),
            ),
            field("build_final_board_mask", words_hex(final_board.words())),
            field("completed_target_rows", format!("0x{completed_rows:x}")),
            field("target_piece_count", self.field.target_piece_count()),
            field(
                "inverse_lock_clear_catalog_identity",
                format!("{:016x}", self.catalog.identity_digest()),
            ),
            field(
                "solution_found",
                self.trivial_target || !normalized_keys.is_empty(),
            ),
            field(
                "packing_candidate_count",
                if self.external_geometry {
                    self.processed_candidate_count
                } else {
                    self.geometry.candidate_count()
                },
            ),
            field(
                "geometry_candidate_family_count",
                self.geometry.candidate_family_count().map_or_else(
                    || "overflow-or-incomplete".to_owned(),
                    |count| count.to_string(),
                ),
            ),
            field(
                "packing_candidate_set_digest",
                format!("{:016x}", self.candidate_digest),
            ),
            field("unique_solution_count", normalized_keys.len()),
            field(
                "normalized_solution_set_hash",
                super::build_probability::normalized_string_solution_set_hash(&normalized_keys),
            ),
            field("witnessed_pattern_count", self.witnessed_pattern_count),
            field("build_variant_count_exact", false),
            field(
                "build_probability_evaluation_basis",
                if tiling_only {
                    "geometry-only"
                } else {
                    "candidate-pattern-existence"
                },
            ),
            field("build_path_multiplicity_counted", false),
            field("materialized_pattern_count", universe.pattern_count()),
            field("coverage_pattern_count", universe.pattern_count()),
            field("piece_source_id", self.problem.piece_source().id().get()),
            field("pattern_universe_id", universe.pattern_universe_id().get()),
            field(
                "pattern_weight_model_id",
                universe.pattern_weight_model_id().get(),
            ),
            field(
                "coverage_aggregation_contract",
                PatternCoverageAggregation::CONTRACT_ID,
            ),
            field(
                "coverage_aggregation_availability",
                coverage_aggregation
                    .as_ref()
                    .map_or("not-calculated", |summary| summary.availability().as_str()),
            ),
            field("coverage_aggregation_complete", probability_complete),
            field(
                "coverage_aggregation_source_row_count",
                coverage_source_row_count,
            ),
            field(
                "covered_pattern_count",
                coverage_aggregation
                    .as_ref()
                    .map_or(0, PatternCoverageAggregation::success_pattern_count),
            ),
            field(
                "failed_pattern_count",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| summary.failed_pattern_count().to_string(),
                ),
            ),
            field("coverage_probability", probability),
            field(
                "failed_coverage_probability",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| {
                        super::build_probability::probability_text(
                            summary.failed_probability().get(),
                        )
                    },
                ),
            ),
            field(
                "materialized_probability_mass",
                super::build_probability::probability_text(universe.weights().total_weight().get()),
            ),
            field(
                "coverage_probability_denominator",
                "full-materialized-pattern-universe",
            ),
            field(
                "success_conditional_probability_denominator",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| {
                        super::build_probability::probability_text(
                            summary.success_probability().get(),
                        )
                    },
                ),
            ),
            field("probability_complete", probability_complete),
            field("count_complete", count_complete),
            field(
                "solution_probabilities_requested",
                self.problem.solution_probability_policy().requested(),
            ),
            field(
                "searched_nodes",
                self.geometry
                    .expanded_nodes()
                    .saturating_add(self.searched_build_nodes),
            ),
            field("geometry_searched_nodes", self.geometry.expanded_nodes()),
            field("buildup_searched_nodes", self.searched_build_nodes),
            field(
                "geometry_domain_pruned_states",
                self.geometry.domain_pruned_states(),
            ),
            field(
                "geometry_hall_pruned_states",
                self.geometry.hall_pruned_states(),
            ),
            field(
                "geometry_column_pruned_states",
                self.geometry.column_pruned_states(),
            ),
            field(
                "geometry_component_pruned_states",
                self.geometry.component_pruned_states(),
            ),
            field(
                "geometry_component_compositions",
                self.geometry.component_compositions(),
            ),
            field("total_build_order_nodes", self.total_build_order_nodes),
            field("peak_build_order_nodes", self.peak_build_order_nodes),
            field("coverage_product_words", self.coverage_product_words),
            field("coverage_product_states", self.coverage_product_states),
            field(
                "coverage_product_edge_checks",
                self.coverage_product_edge_checks,
            ),
            field(
                "resource_peak_frontier_states",
                self.geometry.peak_frontier(),
            ),
            field(
                "resource_peak_cpu_bytes",
                self.retained_bytes()
                    .saturating_add(self.distributed_worker_memory_bytes),
            ),
            field("total_reachability_states", self.reachability_states),
            field("resource_truncated", self.truncated_reason.is_some()),
            field(
                "resource_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field("objective", "build-probability"),
            field("build_probability_aggregation", self.aggregation.as_str()),
            field("buildability_verified", !tiling_only),
            field("coverage_calculated", !tiling_only),
            field("probability_calculated", !tiling_only),
            field(
                "spin_profile_requested",
                self.aggregation
                    .spin_profile()
                    .map_or("none", |profile| profile.as_str()),
            ),
            field(
                "postprocess_build_spin_requested",
                self.aggregation.requests_spin_coverage(),
            ),
            field(
                "execution_constraint_preserve_b2b",
                execution_constraints.preserves_back_to_back(),
            ),
            field(
                "execution_constraint_spin_profile",
                execution_constraints.spin_profile().as_str(),
            ),
            field(
                "execution_constraint_materialized",
                self.distributed_execution_constraint_materialized,
            ),
            field("objective_search_complete", count_complete),
            field(
                "objective_complete",
                count_complete
                    && (!execution_constraints.requested()
                        || self.distributed_execution_constraint_materialized),
            ),
            field(
                "objective_incomplete_reason",
                if !count_complete {
                    self.truncated_reason
                        .unwrap_or("pattern_universe_incomplete")
                } else if execution_constraints.requested()
                    && !self.distributed_execution_constraint_materialized
                {
                    "b2b_preservation_not_materialized"
                } else {
                    "none"
                },
            ),
            field(
                "representative_pattern_id",
                self.representative_pattern_id
                    .map_or_else(|| "none".to_owned(), |id| id.to_string()),
            ),
            field(
                "representative_candidate_ordinal",
                self.representative_rank
                    .map_or_else(|| "none".to_owned(), |rank| rank.to_string()),
            ),
        ];
        let result = CoreExecutionResult::new(fields, self.representative_path.clone())
            .with_normalized_solution_keys(normalized_keys)
            .with_normalized_solution_coverages(normalized_solution_coverages)
            .with_coverage_pattern_words(self.covered_patterns.to_owned_words())
            .with_spin_coverage_execution_batch(spin_batch);
        let result = if self.build_constraints_materialized() {
            mark_build_constraints_filtered(result)?
        } else {
            result
        };
        Ok(
            if execution_constraints.requested()
                && (!constraints_materialized
                    || self.problem.solution_probability_policy().requested())
            {
                let pattern_weights = (0..universe.pattern_count())
                    .map(|pattern| universe.weight_at(pattern).get().to_string())
                    .collect();
                result.with_postprocess_execution_batch(
                    Vec::new(),
                    count_complete && probability_complete,
                    pattern_weights,
                )
            } else {
                result
            },
        )
    }

    fn count_pc_build_paths(&self) -> bool {
        self.purpose == ExtendedFamilyPurpose::Pc
            && self.problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountAll
    }

    fn validate_pc_representative_physical_chain(
        &self,
        has_solutions: bool,
    ) -> Result<(), WasmExactSearchError> {
        // This is only the already-selected witness. Full replay multiplicity
        // and query-bound terminal authority remain separate contracts.
        if self.representative_path.is_empty() {
            return if !has_solutions {
                Ok(())
            } else {
                Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_representative_replay_missing",
                ))
            };
        }
        let mut board = clearra_core_domain::board::standard_pc_board::Board256Mask::from_words(
            self.catalog.initial_board().words(),
        );
        for step in &self.representative_path {
            let rotation = clearra_core_domain::piece::rotation::RotationState::from_quarter_turns(
                step.rotation(),
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("extended_pc_replay_rotation_invalid")
            })?;
            let projected = FullHeightReplayProjector::project_lock(
                self.field.height(),
                board,
                step.piece(),
                rotation,
                step.x(),
                step.y(),
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("extended_pc_physical_replay_invalid")
            })?;
            if projected.cleared_lines() != step.cleared_lines() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_replay_cleared_lines_mismatch",
                ));
            }
            board = projected.after_line_clear();
        }
        if !board.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_replay_terminal_board_not_empty",
            ));
        }
        Ok(())
    }

    fn validate_pc_family_result_source(
        &self,
        has_solutions: bool,
    ) -> Result<(), WasmExactSearchError> {
        if !self.external_geometry
            && self
                .pc_observation
                .as_ref()
                .is_some_and(|owner| !owner.evaluated())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_not_finalized",
            ));
        }
        self.validate_pc_representative_physical_chain(has_solutions)
    }

    fn build_pc_family_result(&mut self) -> Result<CoreExecutionResult, WasmExactSearchError> {
        self.validate_pc_family_result_source(!self.buildable_tilings.is_empty())?;
        let mut keys = Vec::new();
        keys.try_reserve_exact(self.buildable_tilings.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_build_probability_solution_storage_unavailable",
                )
            })?;
        let mut coverages = Vec::new();
        if let Some(rows) = self.solution_coverage.as_ref() {
            coverages.try_reserve_exact(rows.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "extended_pc_result_solution_coverage_storage_unavailable",
                )
            })?;
        }
        // The terminal resource preflight and representative validation have
        // already run. End the private producer lifetimes before assembling
        // probabilities, replay graphs, or typed proof. Each extended coverage
        // key and its bitset become the public row's actual backing owners.
        let initial = self.catalog.initial_board();
        let height = self.field.height();
        keys.extend(
            core::mem::take(&mut self.buildable_tilings)
                .into_iter()
                .map(|tiling| tiling.canonical_key(initial, height)),
        );
        keys.sort_unstable();
        keys.dedup();
        if let Some(rows) = self.solution_coverage.take() {
            coverages.extend(
                rows.into_iter()
                    .map(|(key, bits)| NormalizedSolutionCoverage::new(key, bits)),
            );
            coverages.sort_unstable_by(|left, right| left.solution_key().cmp(right.solution_key()));
        }
        let result = self.assemble_pc_family_result(&keys, coverages, None)?;
        Ok(result.with_normalized_solution_keys(keys))
    }

    /// Terminal join for the sliced native coordinator. Its existing
    /// request ledger continues to charge the root, remaining workers, and
    /// this cursor slot; the slot already owns the received keys and hash.
    /// No public or distributed input can construct a sealed family.
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    fn complete_pc_family_with_sealed_keys(
        &mut self,
        family: SealedPcFamily,
        owner: &ExecutionMemoryOwner,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        use clearra_core_domain::objective::objective_kind::ObjectiveKind;
        use clearra_problem::SearchOutputPolicy;

        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let invalid =
            || WasmExactSearchError::InvalidProblem("extended_pc_sealed_result_contract_invalid");
        let projection_error = || {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_result_materialization_projection_overflow",
            )
        };
        let has_solutions = !family.keys.is_empty();
        let candidate_count = self.geometry.candidate_count();
        let depth = self.problem.exact_pieces().ok_or_else(invalid)?;
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or_else(invalid)?;
        if self.finished
            || self.purpose != ExtendedFamilyPurpose::Pc
            || self.shared_immutable_owners
            || self.external_geometry
            || self.trivial_target
            || self.geometry.is_compiling()
            || control.partition().count() != 1
            || self.problem.objective().kind() != ObjectiveKind::All
            || !matches!(
                self.problem.output_policy(),
                SearchOutputPolicy::Summary | SearchOutputPolicy::Trace
            )
            || self.problem.objective().score().requested()
            || self.problem.objective().execution_constraints().requested()
            || self.problem.solution_probability_policy().requested()
            || self.problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::Disabled
            || self
                .problem
                .queue_observation_policy()
                .requires_observation_policy()
            || self
                .problem
                .core_query()
                .remaining_queue()
                .as_fixed_sequence()
                .is_none()
            || !universe.complete()
            || universe.pattern_count() != 1
            || self.covered_patterns.pattern_count() != 1
            || self.solution_coverage.is_some()
            || self.pc_observation.is_some()
            || !self.buildable_tilings.is_empty()
            || !self.distributed_solution_keys.is_empty()
            || !self.spin_execution_graphs.is_empty()
            || !self.finesse_languages.is_empty()
            || self.truncated_reason.is_some()
            || !self.supply_projection_complete
            || !self.count_pc_build_paths()
            || !self.pc_build_variant_count_complete
            || self.processed_candidate_count != candidate_count
            || self.geometry.candidate_family_count() != Some(candidate_count as u128)
            || family.keys.len() > candidate_count
            || has_solutions == self.covered_patterns.is_empty()
            || self.representative_path.len() > 60
            || family.hash.strip_prefix("cts1:").is_none_or(|raw| {
                raw.len() != 16
                    || !raw
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err(invalid());
        }
        self.validate_pc_family_result_source(has_solutions)?;
        if has_solutions {
            if self.representative_pattern_id != Some(0)
                || self.representative_path.len() != depth
                || self
                    .representative_rank
                    .is_none_or(|rank| u128::from(rank) >= candidate_count as u128)
                || self.pc_build_variant_count < family.keys.len() as u128
            {
                return Err(invalid());
            }
        } else if !self.representative_path.is_empty()
            || self.representative_pattern_id.is_some()
            || self.representative_rank.is_some()
            || self.pc_build_variant_count != 0
        {
            return Err(invalid());
        }

        // Each identity has equal fixed-width text in this single exact
        // height/depth family. The bounded cursor checked length == capacity
        // before admitting each row, so this confirms its accumulated
        // credit without revisiting the family.
        let key_len = ExtendedTilingKey::empty()
            .canonical_key_len(self.field.height())
            .checked_add(depth.checked_mul(66).ok_or_else(projection_error)?)
            .and_then(|n| n.checked_add(depth.saturating_sub(1)))
            .ok_or_else(projection_error)?;
        if family.key_text_bytes != (family.keys.len() as u128) * (key_len as u128) {
            return Err(invalid());
        }
        let key_storage = (family.keys.capacity() as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)
            .and_then(|n| n.checked_add(family.key_text_bytes))
            .ok_or_else(projection_error)?;
        // Private variable-size solution stores above are empty. This
        // preflight can only inspect the one coverage word and bounded path
        // in addition to the common fixed result envelope.
        let result_future = self
            .checked_result_materialization_future_bytes()
            .ok_or_else(projection_error)?;
        let reserved = key_storage
            .checked_add(family.hash.capacity() as u128)
            .and_then(|n| n.checked_add(result_future))
            .ok_or_else(projection_error)?;
        owner
            .ensure(reserved)
            .map_err(WasmExactSearchError::resource_admission)?;
        let result = self.assemble_pc_family_result(&family.keys, Vec::new(), Some(family.hash))?;
        let retained = result
            .checked_resource_retained_bytes()
            .ok_or_else(projection_error)?;
        if retained > result_future {
            return Err(projection_error());
        }
        // The formatted hash scratch is gone; strings inside the result
        // are independently retained. Move the Vec into that result after
        // the final admission, without another key loop or reservation.
        owner
            .ensure(
                key_storage
                    .checked_add(retained)
                    .ok_or_else(projection_error)?,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        self.finished = true;
        Ok(result.with_normalized_solution_keys(family.keys))
    }

    // The advanced native reducer owns the keys outside this helper so that
    // the fixed result can be measured before attaching the live key owner.
    // The direct caller still derives its cts1 hash here from its sorted keys.
    fn assemble_pc_family_result(
        &mut self,
        keys: &[String],
        coverages: Vec<NormalizedSolutionCoverage>,
        sealed_hash: Option<String>,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        let minimum_source = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_minimum_cover_v2_evidence();
        let score_source = self.problem.objective().score().requested();
        let path_source = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_path_v2_evidence();
        let failed_source = self
            .problem
            .pc_chance_evidence_policy()
            .pc_failed_queue_example_limit()
            .is_some();
        let chance_source = self.problem.pc_chance_evidence_policy()
            == PcChanceEvidencePolicy::PcProbabilityV2
            || failed_source;
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let count_complete = self.supply_projection_complete
            && self.truncated_reason.is_none()
            && self.pc_build_variant_count_complete;
        let policy_requested = self.pc_observation.is_some();
        let policy_evaluated = self
            .pc_observation
            .as_ref()
            .is_some_and(|owner| owner.evaluated())
            && !self.external_geometry;
        let probability_complete = universe.complete()
            && self.truncated_reason.is_none()
            && (!policy_requested || policy_evaluated);
        let probabilities = if self.problem.solution_probability_policy().requested()
            && !policy_requested
        {
            crate::solution_probability::normalized_solution_probability_reports(
                keys,
                &coverages,
                universe.weights(),
                probability_complete && count_complete,
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("extended_pc_solution_probability_mismatch")
            })?
        } else {
            Vec::new()
        };
        // The public PC document contract uses the semantic `cts1` hash,
        // not Build's private string-owner `ctks1` envelope. Use the borrowed
        // four-word codec so exporting the same family neither truncates the
        // field nor acquires compact Board64 reducer authority.
        let hash = if let Some(hash) = sealed_hash {
            hash
        } else {
            let mut hasher =
                clearra_core_domain::solution::NormalizedTilingSolutionSetHasher::default();
            for key in keys {
                let identity = ExtendedTilingSolutionKey::parse_canonical(key).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("extended_pc_solution_identity_invalid")
                })?;
                hasher.update_extended_canonical_key(identity);
            }
            hasher.finish()
        };
        let probability = universe
            .weights()
            .covered_weight(&self.covered_patterns)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_coverage_universe_mismatch",
            ))?;
        let mut fields = vec![
            field("problem_preset", self.problem.preset().as_str()),
            field("compiled_goal", "clear-to-empty"),
            field("search_kind", "pc"),
            field(
                "objective",
                if minimum_source {
                    "minimum-cover"
                } else if self.problem.objective().kind()
                    == clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique
                {
                    "unique"
                } else {
                    "all"
                },
            ),
            field(
                "search_output_policy",
                self.problem.output_policy().as_str(),
            ),
            field("actual_solution_set_contract", "normalized-tiling-set"),
            field(
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            field("backend_selected", "wasm-cpu-pc-extended"),
            field("actual_backend", "wasm-cpu-pc-extended"),
            field("backend_fallback_used", false),
            field("board_height", self.field.height()),
            field("board_storage", "board256-canonical"),
            field("workers_requested", self.problem.backend_policy().workers()),
            field("workers_used", self.workers_used),
            field("cpu_parallel_execution", self.workers_used > 1),
            field(
                "cpu_parallel_decision_reason",
                if self.workers_used > 1 {
                    "extended-pc-shared-family-private-verifiers"
                } else {
                    "direct-extended-pc-family"
                },
            ),
            field("cpu_parallel_active_workers", self.parallel_active_workers),
            field(
                "cpu_parallel_minimum_worker_candidates",
                self.parallel_minimum_worker_candidates,
            ),
            field(
                "cpu_parallel_maximum_worker_candidates",
                self.parallel_maximum_worker_candidates,
            ),
            field("solution_found", !keys.is_empty()),
            field(
                "representative_physical_replay_validated",
                !self.representative_path.is_empty(),
            ),
            field("solution_set_materialized", true),
            field("solution_keys_materialized_count", keys.len()),
            field("solution_keys_complete", count_complete),
            // This producer returns the complete family inline; it does not
            // expose the separate partial-page store used by Tiling.
            field("solution_page_available", false),
            field("unique_solution_count", keys.len()),
            field("total_solution_count", keys.len()),
            field("normalized_unique_solution_count", keys.len()),
            field("actual_normalized_unique_solution_count", keys.len()),
            field(
                "normalized_solution_key_algorithm",
                clearra_core_domain::solution::NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            ),
            field(
                "normalized_solution_set_hash_algorithm",
                clearra_core_domain::solution::NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            ),
            field("normalized_solution_set_hash", &hash),
            field("actual_normalized_solution_set_hash", &hash),
            field("solution_count_calculated", true),
            field("count_complete", count_complete),
            field("objective_search_complete", count_complete),
            field("coverage_row_count", "not-calculated"),
            field("renormalized", false),
            field(
                "objective_complete",
                !minimum_source && !score_source && count_complete && probability_complete,
            ),
            field("build_variant_count", self.pc_build_variant_count),
            field(
                "build_variant_count_exact",
                self.count_pc_build_paths() && count_complete,
            ),
            field("buildup_executed", true),
            field("buildability_verified", true),
            field("coverage_calculated", true),
            field("probability_calculated", true),
            field("covered_pattern_count", self.covered_patterns.count_ones()),
            field("coverage_pattern_count", universe.pattern_count()),
            field("piece_source_id", self.problem.piece_source().id().get()),
            field("pattern_universe_id", universe.pattern_universe_id().get()),
            field(
                "pattern_weight_model_id",
                universe.pattern_weight_model_id().get(),
            ),
            field("materialized_pattern_count", universe.pattern_count()),
            field(
                "total_possible_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field(
                "materialized_probability_mass",
                universe.materialized_probability_mass().get(),
            ),
            field("coverage_probability", probability.get()),
            field("probability_complete", probability_complete),
            field("supply_probability_complete", universe.complete()),
            field(
                "resource_probability_complete",
                self.truncated_reason.is_none(),
            ),
            field(
                "solution_probabilities_requested",
                self.problem.solution_probability_policy().requested(),
            ),
            field("solution_probability_count", probabilities.len()),
            field(
                "solution_probability_complete",
                !self.problem.solution_probability_policy().requested()
                    || !policy_requested && probability_complete && count_complete,
            ),
            field("packing_candidate_count", self.geometry.candidate_count()),
            field(
                "packing_candidate_digest",
                format!("{:016x}", self.candidate_digest),
            ),
            field("searched_geometry_nodes", self.geometry.expanded_nodes()),
            field("searched_build_nodes", self.searched_build_nodes),
            field(
                "resource_node_budget_consumed",
                self.request_node_budget
                    .as_ref()
                    .map_or(0, |budget| budget.consumed()),
            ),
            field("total_reachability_states", self.reachability_states),
            field(
                "coverage_product_edge_checks",
                self.coverage_product_edge_checks,
            ),
            field(
                "resource_peak_cpu_bytes",
                (self.retained_bytes() as u128).max(self.native_parallel_peak_bytes),
            ),
            field(
                "resource_peak_cpu_bytes_kind",
                "checked-retained-projection",
            ),
            field("resource_truncated", self.truncated_reason.is_some()),
            field(
                "resource_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field(
                "count_truncated_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
        ];
        if self.problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::PcProbabilityV2 {
            // The validated Chance route owns probability rows only. Match
            // the compact producer's explicit request flags before App checks
            // the filtered authority; missing flags are not an implicit false.
            fields.extend([
                field("postprocess_scoring_requested", false),
                field("postprocess_pc_save_requested", false),
                field("postprocess_pc_path_requested", false),
            ]);
        }
        if policy_requested {
            let metrics = self
                .pc_observation
                .as_ref()
                .expect("policy owner exists")
                .metrics();
            fields.extend([
                field(
                    "queue_knowledge",
                    self.problem.queue_observation_policy().keyword(),
                ),
                field(
                    "visible_piece_count",
                    self.problem
                        .queue_observation_policy()
                        .visible_piece_count()
                        .unwrap_or(0),
                ),
                field("observation_policy_evaluated", policy_evaluated),
                field(
                    "coverage_semantics",
                    if policy_evaluated {
                        self.problem.queue_observation_policy().coverage_semantics()
                    } else {
                        "partial-family-oracle-upper-bound"
                    },
                ),
                field("observation_policy_states", metrics.policy_states),
                field("observation_policy_action_checks", metrics.action_checks),
                field("observation_trie_nodes", metrics.observation_nodes),
            ]);
            if self.problem.solution_probability_policy().requested() {
                fields.extend([
                    field(
                        "solution_probability_basis",
                        "unsupported-under-visible-seven-policy",
                    ),
                    field(
                        "solution_probability_incomplete_reason",
                        "per-solution-policy-language-not-materialized",
                    ),
                ]);
            }
        }
        let constraints = self.problem.objective().execution_constraints();
        if constraints.requested() {
            // The PC BuildUp language is already filtered. App must not apply
            // a compact batch filter after typed coverage/replay proof was
            // created, nor renormalize away unsuccessful supplied queues.
            fields.extend([
                field("execution_constraint_preserve_b2b", true),
                field(
                    "execution_constraint_spin_profile",
                    constraints.spin_profile().as_str(),
                ),
                field("execution_constraint_materialized", true),
                field("b2b_preservation_selection", "existential"),
                field(
                    "b2b_preservation_denominator_semantics",
                    "original-materialized-queue",
                ),
                field("b2b_preserving_solution_count", keys.len()),
                field(
                    "b2b_preserving_pattern_count",
                    self.covered_patterns.count_ones(),
                ),
                field(
                    "b2b_preservation_pattern_universe_count",
                    universe.pattern_count(),
                ),
                field("b2b_preservation_probability", probability.get()),
                field("b2b_preservation_count_complete", count_complete),
                field(
                    "b2b_preservation_probability_complete",
                    probability_complete,
                ),
                field(
                    "b2b_preserving_candidate_pattern_count",
                    coverages
                        .iter()
                        .map(|coverage| u128::from(coverage.covered_patterns().count_ones()))
                        .sum::<u128>(),
                ),
            ]);
        }
        let evidence = if minimum_source || chance_source {
            let mut union = PatternBitSet::new(universe.pattern_count());
            let mut rows = Vec::new();
            rows.try_reserve_exact(coverages.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("extended_pc_minimum_row_allocation_failed")
            })?;
            let mut aligned = keys.len() == coverages.len();
            for (index, coverage) in coverages.iter().enumerate() {
                aligned &= keys
                    .get(index)
                    .is_some_and(|key| key == coverage.solution_key());
                union.union_with(coverage.covered_patterns()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "extended_pc_minimum_row_universe_mismatch",
                    )
                })?;
                let candidate_id = u64::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_minimum_candidate_index_overflow",
                    ))?;
                rows.push(CoverageRow::new_with_piece_source(
                    candidate_id,
                    CoverageRowKind::Build,
                    self.problem.piece_source().id().get(),
                    universe.pattern_universe_id(),
                    universe.pattern_weight_model_id(),
                    coverage.covered_patterns().clone(),
                ));
            }
            if minimum_source {
                fields.extend([
                    field("minimum_cover_requested", true),
                    field("minimum_cover_complete", false),
                    field("minimum_cover_proven_minimum", false),
                    field("minimum_cover_incomplete_reason", "deferred-to-coordinator"),
                    field("objective_incomplete_reason", "deferred-to-coordinator"),
                    field("minimum_cover_required_pattern_count", union.count_ones()),
                    field("minimum_cover_source_solution_count", keys.len()),
                    field("minimum_cover_selected_solution_count", keys.len()),
                ]);
            }
            let evidence = PcChanceCoverageEvidence::from_problem_rows(
                &self.problem,
                rows,
                aligned && count_complete && probability_complete && union == self.covered_patterns,
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "extended_pc_minimum_problem_evidence_mismatch",
                )
            })?;
            Some(if minimum_source {
                evidence
                    .bind_extended_minimum_source_keys(keys)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "extended_pc_minimum_problem_evidence_mismatch",
                        )
                    })?
            } else {
                evidence
            })
        } else {
            None
        };
        if failed_source {
            fields.extend([
                field("status", "percent-executed"),
                field("pattern_count", universe.pattern_count()),
                field("c_buildup_coverage_row_count", coverages.len()),
                field("probability", probability.get()),
                field("weighted_probability", probability.get()),
                field("truncated", self.truncated_reason.is_some()),
            ]);
        }
        let score_evidence = if score_source {
            Some(
                if self
                    .problem
                    .pc_chance_evidence_policy()
                    .retains_pc_score_portfolio_v2_evidence()
                {
                    PcScoreProblemEvidence::from_executed_score_portfolio_problem(&self.problem)
                } else {
                    PcScoreProblemEvidence::from_executed_problem(&self.problem)
                }
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "extended_pc_score_problem_evidence_mismatch",
                    )
                })?,
            )
        } else {
            None
        };
        let mut physical_batch = if score_source || path_source {
            // A digest is not a canonical candidate index. Rebind every actual
            // physical graph to the sorted complete family; multiple physical
            // realizations of one colored tiling deliberately share its index.
            self.spin_execution_graphs
                .sort_unstable_by(compare_spin_graphs);
            let mut covered_key_count = 0_usize;
            let mut previous_key_index = None;
            for graph in &mut self.spin_execution_graphs {
                let index = keys
                    .binary_search_by(|key| key.as_str().cmp(graph.candidate_key()))
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "extended_pc_score_graph_family_mismatch",
                        )
                    })?;
                if previous_key_index != Some(index) {
                    covered_key_count += 1;
                    previous_key_index = Some(index);
                }
                let id = u64::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "extended_pc_score_candidate_index_overflow",
                    ))?;
                graph.set_candidate_id(id);
            }
            if covered_key_count != keys.len() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_score_graph_family_incomplete",
                ));
            }
            let patterns = (0..universe.pattern_count())
                .map(|index| universe.sequence_at(index).into_owned())
                .collect();
            let (kick_table_id, rule_profile_id) = replay_profile_ids(&self.problem);
            let batch = SpinCoverageExecutionBatch::new(
                patterns,
                self.problem.initial_hold().cursor(),
                self.problem.initial_hold().hold_piece(),
                self.problem.supply().hold_enabled(),
                self.problem.supply().projects_unplaced_lookahead(),
                self.problem.supply().projects_standard_bag_lookahead(),
                kick_table_id,
                rule_profile_id,
                core::mem::take(&mut self.spin_execution_graphs),
                count_complete && probability_complete,
            );
            if score_source {
                fields.extend([
                    field("postprocess_scoring_requested", true),
                    field(
                        "score_objective_mode",
                        self.problem.objective().score().mode().as_str(),
                    ),
                    field(
                        "score_profile_requested",
                        self.problem.objective().score().profile().as_str(),
                    ),
                    field(
                        "spin_profile_requested",
                        self.problem.objective().score().spin_profile().as_str(),
                    ),
                    field(
                        "score_initial_b2b",
                        self.problem.objective().score().initial_b2b(),
                    ),
                    field("score_summary_requested", true),
                    field("score_summary_complete", false),
                    field("score_summary_incomplete_reason", "deferred-to-coordinator"),
                    field("objective_incomplete_reason", "deferred-to-coordinator"),
                    field(
                        "full_height_score_execution_graph_count",
                        batch.graphs().len(),
                    ),
                ]);
            } else {
                fields.extend([
                    field(
                        "full_height_replay_execution_graph_count",
                        batch.graphs().len(),
                    ),
                    field("complete_replay_requested", true),
                    field("complete_replay_complete", false),
                    field("objective_incomplete_reason", "deferred-to-coordinator"),
                ]);
            }
            Some(
                FullHeightExecutionBatch::from_spin_coverage(
                    self.field.height(),
                    clearra_core_domain::board::standard_pc_board::Board256Mask::from_words(
                        self.catalog.initial_board().words(),
                    ),
                    batch,
                )
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("extended_pc_score_physical_batch_invalid")
                })?,
            )
        } else {
            None
        };
        let path_evidence = if path_source && count_complete && probability_complete {
            Some(
                PcFullHeightReplayEvidence::from_executed_problem(
                    &self.problem,
                    keys,
                    physical_batch
                        .take()
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "extended_pc_replay_physical_batch_missing",
                        ))?,
                )
                .map_err(WasmExactSearchError::InvalidProblem)?,
            )
        } else {
            None
        };
        // Minimum evidence comes from this PC producer's verified language,
        // not from Build result fields. The common exact reducer runs in App.
        let mut result = CoreExecutionResult::new(fields, self.representative_path.clone())
            .with_normalized_solution_coverages(coverages)
            .with_solution_probabilities(probabilities)
            .with_coverage_pattern_words(self.covered_patterns.to_owned_words());
        if let Some(evidence) = evidence {
            result = result.with_pc_chance_coverage_evidence(evidence);
        }
        if super::pc_chance_constraints::filters_pc_chance_constraints(&self.problem) {
            result = super::pc_chance_constraints::mark_filtered_pc_chance_constraints(
                result,
                &self.problem,
            )?;
        }
        result = result.with_pc_score_problem_evidence(score_evidence);
        if score_source {
            if let Some(batch) = physical_batch {
                result = result.with_full_height_scoring_execution_batch(batch);
            }
        }
        if let Some(evidence) = path_evidence {
            result = result.with_pc_full_height_replay_evidence(evidence);
        }
        Ok(
            if self.problem.solution_probability_policy().requested() || score_source {
                // Canonical weights are denominator evidence only. An empty,
                // incomplete replay batch grants no score/path execution authority.
                let weights = (0..universe.pattern_count())
                    .map(|index| universe.weight_at(index).get().to_string())
                    .collect();
                result.with_postprocess_execution_batch(
                    Vec::new(),
                    score_source && count_complete && probability_complete,
                    weights,
                )
            } else {
                result
            },
        )
    }

    pub(super) fn finesse_search_material(
        &self,
    ) -> Result<FinesseSearchMaterial, WasmExactSearchError> {
        if !self.finesse_requested {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_material_not_requested",
            ));
        }
        let mut languages = Vec::new();
        languages
            .try_reserve_exact(self.finesse_languages.len() + usize::from(self.trivial_target))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_evaluation_language_storage_unavailable",
                )
            })?;
        for (solution_key, prepared) in &self.finesse_languages {
            languages.push((solution_key.clone(), costed_finesse_language(prepared)?));
        }
        if self.trivial_target {
            let language = CostedGeometryLanguage::new(
                GeometryNodeId::new(0),
                vec![GeometryLanguageNode::new(
                    0,
                    true,
                    Vec::<CostedGeometryEdge>::new(),
                )],
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_trivial_language_invalid",
                )
            })?;
            languages.push((
                ExtendedTilingKey::empty()
                    .canonical_key(self.catalog.initial_board(), self.field.height()),
                language,
            ));
        }
        // Equal canonical keys can still expose different concrete minimal
        // movement alternatives. Preserve them for the shared exact union.
        languages.sort_by(|left, right| left.0.cmp(&right.0));

        FinesseSearchMaterial::new(
            &self.problem,
            languages,
            self.supply_projection_complete && self.truncated_reason.is_none(),
        )
    }

    pub(super) fn checked_finesse_search_material_future_bytes(&self) -> Option<u128> {
        let language_count = self
            .finesse_languages
            .len()
            .checked_add(usize::from(self.trivial_target))?;
        let mut bytes = (language_count as u128)
            .checked_mul(core::mem::size_of::<(String, CostedGeometryLanguage)>() as u128)?;
        for (solution_key, prepared) in &self.finesse_languages {
            bytes = bytes
                .checked_add(solution_key.len() as u128)?
                .checked_add(
                    (prepared.nodes.len() as u128)
                        .checked_mul(core::mem::size_of::<GeometryLanguageNode>() as u128)?,
                )?
                .checked_add(
                    (prepared.edges.len() as u128)
                        .checked_mul(core::mem::size_of::<CostedGeometryEdge>() as u128)?,
                )?;
        }
        if self.trivial_target {
            bytes = bytes
                .checked_add(
                    ExtendedTilingKey::empty().canonical_key_len(self.field.height()) as u128,
                )?
                .checked_add(core::mem::size_of::<GeometryLanguageNode>() as u128)?;
        }
        bytes.checked_add(FinesseSearchMaterial::checked_fixed_creation_future_bytes(
            &self.problem,
        )?)
    }

    fn node_budget_exhausted(&self) -> bool {
        self.problem.backend_request().max_nodes() != 0 && self.remaining_node_budget() == 0
    }

    fn advance_geometry_bounded(
        &mut self,
    ) -> Result<ExtendedGeometryAdvance, WasmExactSearchError> {
        let outside = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(self.coexisting_retained_bytes))
            .and_then(|bytes| bytes.checked_sub(self.geometry.retained_bytes() as u128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_geometry_memory_projection_unavailable",
            ))?;
        let geometry_span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometryAdvance,
            profile_sample_scale(self.geometry.expanded_nodes()),
        );
        let outcome = self.geometry.advance_bounded(
            &self.catalog,
            self.request_node_budget.as_deref(),
            self.memory_bound,
            outside,
        );
        geometry_span.finish(1);
        outcome
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    fn pc_resource_stop_error(&self, reason: &'static str) -> WasmExactSearchError {
        if reason == "node_budget_exceeded" {
            if let Some(budget) = self.request_node_budget.as_ref() {
                return WasmExactSearchError::resource_admission(budget.exhaustion_report());
            }
        }
        WasmExactSearchError::InvalidProblem(reason)
    }

    fn remaining_node_budget(&self) -> usize {
        if let Some(budget) = self.request_node_budget.as_ref() {
            return budget.remaining();
        }
        let limit = self.problem.backend_request().max_nodes();
        if limit == 0 {
            return 0;
        }
        limit.saturating_sub(
            self.geometry
                .expanded_nodes()
                .saturating_add(self.searched_build_nodes),
        )
    }

    fn retained_bytes(&self) -> usize {
        self.checked_retained_bytes()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(usize::MAX)
    }

    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        let problem_bytes = match self.purpose {
            ExtendedFamilyPurpose::Build => {
                super::build_probability::checked_build_probability_problem_nested_retained_bytes(
                    &self.problem,
                )
            }
            ExtendedFamilyPurpose::Pc => {
                super::extended_pc_search::checked_pc_family_problem_nested_retained_bytes(
                    &self.problem,
                )
            }
        }?;
        let owned_problem_bytes = if self.shared_immutable_owners {
            0
        } else {
            problem_bytes.checked_add(core::mem::size_of::<SearchProblem>() as u128 + 32)?
        };
        owned_problem_bytes.checked_add(self.checked_non_problem_retained_bytes()?)
    }

    pub(super) fn checked_retained_bytes_with_coexisting_owners(&self) -> Option<u128> {
        self.checked_retained_bytes()?
            .checked_add(self.coexisting_retained_bytes)
    }

    fn checked_non_problem_retained_bytes(&self) -> Option<u128> {
        let mut total = if !self.shared_immutable_owners && self.request_node_budget.is_some() {
            core::mem::size_of::<SharedExecutionNodeBudget>() as u128 + 32
        } else {
            0
        };
        if !self.shared_immutable_owners {
            total = total.checked_add(
                clearra_core_domain::execution_cancellation::ComputeBudget::SHARED_ALLOCATION_RETAINED_BYTES,
            )?;
        }
        if let Some(prepared) = self.shared_build_preparation.as_ref() {
            total = total.checked_add(prepared.checked_retained_bytes()?)?;
        }
        if let Some(observation) = self.pc_observation.as_ref() {
            total = total.checked_add(observation.checked_retained_bytes()?)?;
        }
        for bytes in [
            if self.shared_immutable_owners {
                0
            } else {
                self.catalog.retained_bytes() + core::mem::size_of::<ExtendedInverseCatalog>() + 32
            },
            self.geometry.retained_bytes(),
            self.build_order_workspace.retained_bytes(),
            self.coverage_evaluator.retained_bytes(),
            self.piece_order_languages.retained_bytes(),
            self.standard_bag_coverage
                .as_ref()
                .map_or(0, StandardBagCoverage::retained_bytes),
            self.covered_patterns.retained_bytes(),
            self.peak_build_scratch_bytes,
        ] {
            total = total.checked_add(bytes as u128)?;
        }
        total = total.checked_add(checked_hash_retained_bytes(
            self.buildable_tilings.capacity(),
            core::mem::size_of::<ExtendedTilingKey>(),
        )?)?;
        for key in &self.buildable_tilings {
            total = total.checked_add(
                (key.retained_bytes() as u128)
                    .checked_sub(core::mem::size_of::<ExtendedTilingKey>() as u128)?,
            )?;
        }
        if let Some(coverage) = self.solution_coverage.as_ref() {
            total = total.checked_add(checked_hash_retained_bytes(
                coverage.capacity(),
                core::mem::size_of::<(String, PatternBitSet)>(),
            )?)?;
            for (key, patterns) in coverage {
                total = total
                    .checked_add(key.capacity() as u128)?
                    .checked_add(patterns.retained_bytes() as u128)?;
            }
        }
        total = total.checked_add(checked_hash_retained_bytes(
            self.distributed_solution_keys.capacity(),
            core::mem::size_of::<String>(),
        )?)?;
        for key in &self.distributed_solution_keys {
            total = total.checked_add(key.capacity() as u128)?;
        }
        total = total.checked_add(
            (self.representative_path.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
        )?;
        for graph in &self.spin_execution_graphs {
            total = total.checked_add(graph.retained_bytes() as u128)?;
        }
        total = total.checked_add(
            (self.spin_execution_graphs.capacity() as u128)
                .checked_mul(core::mem::size_of::<SpinCoverageExecutionGraph>() as u128)?,
        )?;
        total = total.checked_add(
            (self.finesse_languages.capacity() as u128)
                .checked_mul(core::mem::size_of::<(String, PreparedFinesseLanguage)>() as u128)?,
        )?;
        for (key, language) in &self.finesse_languages {
            total = total.checked_add(key.capacity() as u128)?;
            total = total.checked_add((language.nodes.capacity() as u128).checked_mul(
                core::mem::size_of::<super::buildup::PreparedFinesseNode>() as u128,
            )?)?;
            total = total.checked_add((language.edges.capacity() as u128).checked_mul(
                core::mem::size_of::<super::buildup::PreparedFinesseEdge>() as u128,
            )?)?;
        }
        Some(total)
    }

    pub(super) fn set_coexisting_retained_bytes(&mut self, bytes: u128) {
        self.coexisting_retained_bytes = bytes;
    }

    #[cfg(test)]
    pub(super) fn set_memory_bound_for_test(&mut self, memory_bound: ExecutionMemoryBound) {
        self.memory_bound = memory_bound;
    }

    fn ensure_result_materialization_bound(&self) -> Result<(), WasmExactSearchError> {
        let future = self.checked_result_materialization_future_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_result_materialization_projection_overflow",
            ),
        )?;
        self.ensure_memory_bound(future)
    }

    fn ensure_materialized_result_bound(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let result_bytes = super::build_probability::checked_public_result_bytes(result).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_result_materialization_projection_overflow",
            ),
        )?;
        self.ensure_memory_bound(result_bytes)
    }

    fn checked_result_materialization_future_bytes(&self) -> Option<u128> {
        let mut key_count = self
            .buildable_tilings
            .len()
            .checked_add(self.distributed_solution_keys.len())?;
        key_count = key_count.checked_add(usize::from(self.trivial_target))?;
        let mut key_bytes = 0_u128;
        for tiling in &self.buildable_tilings {
            key_bytes =
                key_bytes.checked_add(tiling.canonical_key_len(self.field.height()) as u128)?;
        }
        for key in &self.distributed_solution_keys {
            key_bytes = key_bytes.checked_add(key.len() as u128)?;
        }
        let trivial_key_len = if self.trivial_target {
            ExtendedTilingKey::empty().canonical_key_len(self.field.height()) as u128
        } else {
            0
        };
        key_bytes = key_bytes.checked_add(trivial_key_len)?;

        let mut coverage_count = self.solution_coverage.as_ref().map_or(0, HashMap::len);
        if self.trivial_target
            && self
                .solution_coverage
                .as_ref()
                .is_some_and(HashMap::is_empty)
        {
            coverage_count = coverage_count.checked_add(1)?;
        }
        let mut coverage_key_bytes = self
            .solution_coverage
            .as_ref()
            .into_iter()
            .flat_map(|coverage| coverage.keys())
            .try_fold(0_u128, |bytes, key| bytes.checked_add(key.len() as u128))?;
        if self.trivial_target
            && self
                .solution_coverage
                .as_ref()
                .is_some_and(HashMap::is_empty)
        {
            coverage_key_bytes = coverage_key_bytes.checked_add(trivial_key_len)?;
        }

        let mut future = (key_count as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)?
            .checked_add(key_bytes)?
            .checked_add(
                (coverage_count as u128)
                    .checked_mul(core::mem::size_of::<NormalizedSolutionCoverage>() as u128)?,
            )?
            .checked_add(coverage_key_bytes)?
            .checked_add(
                (self.covered_patterns.word_count() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?
            .checked_add(
                (self.representative_path.len() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
            )?;
        future = future.checked_add(
            super::build_probability::checked_build_probability_fixed_result_surface_bytes()?,
        )?;
        if self.purpose == ExtendedFamilyPurpose::Pc
            && matches!(
                self.problem.pc_chance_evidence_policy(),
                PcChanceEvidencePolicy::PcMinimumCoverV2
                    | PcChanceEvidencePolicy::PcProbabilityV2
                    | PcChanceEvidencePolicy::PcFailedQueueV2 { .. }
                    | PcChanceEvidencePolicy::PcScorePortfolioV2
            )
        {
            future = future
                .checked_add(
                    PcChanceCoverageEvidence::checked_pc_family_creation_future_bytes(
                        &self.problem,
                        coverage_count,
                    )?,
                )?
                .checked_add(
                    PatternBitSet::checked_all_projection(self.covered_patterns.pattern_count())?
                        .constructor_peak_bytes,
                )?;
        }
        if self.purpose == ExtendedFamilyPurpose::Pc
            && (self.problem.solution_probability_policy().requested()
                || self.problem.objective().score().requested())
        {
            future = future
                .checked_add((key_count as u128).checked_mul(
                    core::mem::size_of::<crate::SolutionProbabilityReport>() as u128 + 128,
                )?)?
                .checked_add(key_bytes)?;
            future = future.checked_add(
                (self.covered_patterns.pattern_count() as u128).checked_mul(
                    core::mem::size_of::<String>() as u128
                        + super::build_probability::MAX_CANONICAL_PROBABILITY_TEXT_BYTES,
                )?,
            )?;
        }
        if self.trivial_target && self.solution_coverage.is_some() {
            future = future.checked_add(
                PatternBitSet::checked_all_projection(self.covered_patterns.pattern_count())?
                    .constructor_peak_bytes,
            )?;
        }
        if self.purpose == ExtendedFamilyPurpose::Build
            && self.problem.objective().execution_constraints().requested()
            && (!self.build_constraints_materialized()
                || self.problem.solution_probability_policy().requested())
        {
            future = future.checked_add(
                (self.covered_patterns.pattern_count() as u128).checked_mul(
                    (core::mem::size_of::<String>() as u128).checked_add(
                        super::build_probability::MAX_CANONICAL_PROBABILITY_TEXT_BYTES,
                    )?,
                )?,
            )?;
        }

        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().score().requested()
            || (self.problem.objective().execution_constraints().requested()
                && self.purpose == ExtendedFamilyPurpose::Build
                && !self.build_constraints_materialized())
            || (self.purpose == ExtendedFamilyPurpose::Pc
                && self
                    .problem
                    .pc_chance_evidence_policy()
                    .retains_pc_path_v2_evidence());
        if execution_evidence_requested
            && (self.aggregation.requests_spin_coverage()
                || !(self.problem.objective().execution_constraints().requested()
                    && self.distributed_execution_constraint_materialized))
        {
            let universe = self.problem.piece_source().materialized_universe()?;
            future = future.checked_add(
                (universe.pattern_count() as u128)
                    .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?,
            )?;
            for pattern in 0..universe.pattern_count() {
                future = future.checked_add(
                    (universe.sequence_at(pattern).len() as u128)
                        .checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
                )?;
            }
        }
        if self.purpose == ExtendedFamilyPurpose::Pc && self.problem.objective().score().requested()
        {
            future = future.checked_add(PcScoreProblemEvidence::checked_creation_future_bytes(
                &self.problem,
            )?)?;
        }
        if self.purpose == ExtendedFamilyPurpose::Pc
            && self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_path_v2_evidence()
        {
            future = future.checked_add(
                PcFullHeightReplayEvidence::checked_creation_future_bytes(&self.problem)?,
            )?;
        }
        Some(future)
    }

    #[cfg_attr(
        all(feature = "local-search-ab", not(target_family = "wasm")),
        track_caller
    )]
    fn ensure_memory_bound(&self, checked_future_bytes: u128) -> Result<(), WasmExactSearchError> {
        let observed = self.checked_retained_bytes().ok_or_else(|| {
            WasmExactSearchError::resource_admission(
                self.memory_bound
                    .ensure(u128::MAX, 1)
                    .expect_err("checked retained-byte overflow is unavailable"),
            )
        })?;
        let future = self
            .coexisting_retained_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| {
                WasmExactSearchError::resource_admission(
                    self.memory_bound
                        .ensure(u128::MAX, 1)
                        .expect_err("checked coexisting retained-byte overflow is unavailable"),
                )
            })?;
        if let Some(owner) = self.private_memory_owner.as_ref() {
            let private = observed.checked_add(future).ok_or_else(|| {
                WasmExactSearchError::resource_admission(
                    self.memory_bound
                        .ensure(u128::MAX, 1)
                        .expect_err("private owner overflow is unavailable"),
                )
            })?;
            owner
                .ensure(private)
                .map_err(WasmExactSearchError::resource_admission)
        } else {
            self.memory_bound
                .ensure(observed, future)
                .map_err(WasmExactSearchError::resource_admission)
        }
    }

    fn prepare_build_order_resources(&mut self) -> Result<(), WasmExactSearchError> {
        let external = self
            .checked_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_sub(self.build_order_workspace.retained_bytes() as u128)
            })
            .and_then(|bytes| bytes.checked_add(self.coexisting_retained_bytes))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_build_order_memory_projection_overflow",
            ))?;
        self.build_order_workspace.set_resource_boundary(
            self.memory_bound,
            external,
            self.private_memory_owner.clone(),
            self.request_node_budget.clone(),
        );
        Ok(())
    }
}

fn compare_spin_graphs(
    left: &SpinCoverageExecutionGraph,
    right: &SpinCoverageExecutionGraph,
) -> core::cmp::Ordering {
    fn edge_key(edge: &clearra_replay::ScoringExecutionEdge) -> impl Ord {
        let evidence = edge.lock_evidence();
        (
            edge.to(),
            edge.operation_index(),
            edge.piece(),
            edge.rotation(),
            edge.x(),
            edge.y(),
            edge.cleared_lines(),
            edge.blocked_t_corners(),
            edge.blocked_t_front_corners(),
            edge.perfect_clear(),
            (
                evidence.last_action_was_rotation(),
                evidence.used_kick(),
                evidence.used_180(),
                evidence.from_rotation(),
                evidence.rotation_request() as u8,
                evidence.kick_index(),
                evidence.kick_dx(),
                evidence.kick_dy(),
                evidence.predecessor(),
                evidence.first_success_confirmed(),
                evidence.immobile_before_clear(),
            ),
        )
    }
    fn nodes(graph: &SpinCoverageExecutionGraph) -> impl Iterator<Item = (u32, u32, bool)> + '_ {
        (0..graph.node_count()).map(|index| {
            let node = graph.node(index as u32).expect("stored graph node");
            (node.edge_start(), node.edge_count(), node.accepting())
        })
    }
    left.candidate_key()
        .cmp(right.candidate_key())
        .then_with(|| left.root().cmp(&right.root()))
        .then_with(|| nodes(left).cmp(nodes(right)))
        .then_with(|| {
            left.all_edges()
                .iter()
                .map(edge_key)
                .cmp(right.all_edges().iter().map(edge_key))
        })
        .then_with(|| left.candidate_id().cmp(&right.candidate_id()))
}

fn extended_row_ids_from_canonical_key(
    key: &str,
    expected_height: u8,
    catalog: &ExtendedInverseCatalog,
) -> Result<Vec<u32>, WasmExactSearchError> {
    let identity = ExtendedTilingSolutionKey::parse_canonical(key).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_extended_finesse_solution_key_encoding_invalid")
    })?;
    if identity.height() != expected_height {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_solution_key_header_invalid",
        ));
    }
    if identity.initial_board().words() != catalog.initial_board().words() {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_solution_key_initial_board_mismatch",
        ));
    }
    let mut row_ids = Vec::new();
    if identity.placement_count() != 0 {
        row_ids
            .try_reserve_exact(identity.placement_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_storage_unavailable",
                )
            })?;
        for placement in identity.placements() {
            let piece = placement.piece();
            let cells = super::extended_board::ExtendedBoard::from_words(placement.cells().words());
            let mut matches = catalog
                .skeletons()
                .iter()
                .enumerate()
                .filter(|(_, row)| row.piece == piece && row.cells == cells);
            let (row_id, _) = matches.next().ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_placement_not_in_catalog",
            ))?;
            if matches.next().is_some() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_catalog_collision",
                ));
            }
            row_ids.push(u32::try_from(row_id).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_row_overflow",
                )
            })?);
        }
    }
    Ok(row_ids)
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

fn checked_hash_growth_future_bytes(capacity: usize, len: usize, item: usize) -> Option<u128> {
    let needed = len.checked_add(1)?;
    if needed <= capacity {
        return Some(0);
    }
    (needed as u128)
        .checked_mul(4)?
        .checked_mul(item as u128 + 1)?
        .checked_add(16)
}

fn checked_hash_retained_bytes(capacity: usize, item: usize) -> Option<u128> {
    if capacity == 0 {
        return Some(0);
    }
    (capacity as u128)
        .checked_mul(8)?
        .div_ceil(7)
        .checked_mul(item as u128 + 1)?
        .checked_add(16)
}

#[cfg(test)]
#[path = "extended_standard_bag_tests.rs"]
mod symbolic_tests;

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{
        PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    #[test]
    fn extended_filtered_b2b_worker_rejects_conflicting_constraint_authority() {
        use clearra_objectives::policy::{
            objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
        };

        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(8, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(
            ObjectivePolicy::unique()
                .with_back_to_back_preservation(SpinProfileSelection::AllSpinPlus),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("one-I B2B source");
        let target_field =
            BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0xf, 0, 0, 0])
                .expect("one-I extended target");
        let mut producer = ExtendedBuildProbabilitySession::new(
            &problem,
            target_field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("one-I producer");
        let mut result = None;
        for _ in 0..64 {
            match producer.advance(16, &ExecutionControl::default()).unwrap() {
                BuildProbabilityAdvance::Pending => {}
                BuildProbabilityAdvance::Completed(completed) => {
                    result = Some(completed);
                    break;
                }
                BuildProbabilityAdvance::Cancelled => panic!("uncancelled one-I fixture"),
            }
        }
        let result = result.expect("one-I fixture completes within its bounded slices");
        assert_eq!(
            result.field("execution_constraint_materialized"),
            Some("true")
        );
        assert!(producer
            .validate_distributed_solution_surface(&result, 1)
            .is_ok());
        for (name, invalid) in [
            ("search_kind", "pc"),
            ("execution_constraint_preserve_b2b", "false"),
            ("execution_constraint_materialized", "false"),
            ("execution_constraint_spin_profile", "t-spins"),
            (
                "b2b_preservation_evaluation_basis",
                "candidate-pattern-existence",
            ),
            ("b2b_preservation_selection", "universal"),
            ("b2b_preservation_denominator_semantics", "covered-only"),
            ("b2b_preservation_path_multiplicity_counted", "true"),
        ] {
            for changed in [
                result
                    .clone()
                    .with_replaced_fields(vec![field(name, invalid)]),
                result
                    .clone()
                    .with_additional_fields(vec![field(name, invalid)]),
            ] {
                assert!(
                    matches!(
                        producer.validate_distributed_solution_surface(&changed, 1),
                        Err(WasmExactSearchError::InvalidProblem(
                            "wasm_extended_distributed_filtered_constraint_mismatch"
                        ))
                    ),
                    "accepted conflicting or duplicate filtered worker field {name}"
                );
            }
        }
    }

    #[test]
    fn extended_tiling_build_uses_build_memory_projection_under_finite_budget() {
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;

        for (height, base, target) in [
            (8, 0xf03c0f03c0f03c0f_u128, 0x3c00000000000000000_u128),
            (
                10,
                0xf03c0f03c0f03c0f03c0f_u128,
                0x3c0000000000000000000000_u128,
            ),
        ] {
            let words = |mask: u128| [mask as u64, (mask >> 64) as u64, 0, 0];
            let field = BuildProbabilityField::from_words(height, words(base), words(target))
                .expect("the occupied top row retains the extended height");
            assert_eq!(field.height(), height);
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(u16::from(height), 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
                PieceWindow::new(1),
            )
            .with_exact_pieces(Some(1))
            .with_allow_hold(false)
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::tiling())
            .with_retained_trace_limit(1);
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            // The Build shell must remain distinct from both typed PC owners.
            assert!(problem.checked_pc_family_pointee_retained_bytes().is_none());
            assert!(problem.checked_pc_tiling_pointee_retained_bytes().is_none());
            assert!(problem
                .checked_build_probability_pointee_retained_bytes()
                .is_some());
            let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
            let finite = unbounded.with_cap(4 * 1024 * 1024).unwrap();
            let mut session = ExtendedBuildProbabilitySession::new_with_memory_bound(
                &problem,
                field,
                BuildProbabilityAggregation::TilingOnly,
                finite,
            )
            .expect("a measured Build owner must not require PC capability authority");
            let control = ExecutionControl::default();
            let mut completed = None;
            for _ in 0..4096 {
                match session
                    .advance(128, &control)
                    .expect("finite tiling search")
                {
                    BuildProbabilityAdvance::Pending => {}
                    BuildProbabilityAdvance::Completed(result) => {
                        completed = Some(result);
                        break;
                    }
                    BuildProbabilityAdvance::Cancelled => panic!("uncancelled tiling fixture"),
                }
            }
            let result = completed.expect("one I must finish within the test-only slice bound");
            assert_eq!(
                result.normalized_solution_keys(),
                [format!(
                    "ctk2|height={height}|initial={base:064x}|placements=I:{target:064x}"
                )]
            );
            for (name, expected) in [
                ("unique_solution_count", "1"),
                ("materialized_pattern_count", "1"),
                ("count_complete", "true"),
                ("buildability_verified", "false"),
                ("coverage_calculated", "false"),
                ("probability_calculated", "false"),
                ("probability_complete", "false"),
                ("covered_pattern_count", "0"),
                ("coverage_probability", "not-calculated"),
            ] {
                assert_eq!(
                    result.field(name),
                    Some(expected),
                    "{name}, height={height}"
                );
            }

            assert!(matches!(
                ExtendedBuildProbabilitySession::new_with_memory_bound(
                    &problem,
                    field,
                    BuildProbabilityAggregation::TilingOnly,
                    unbounded.with_cap(1).unwrap(),
                ),
                Err(WasmExactSearchError::ResourceAdmission(_))
            ));
        }
    }

    #[test]
    fn shared_build_preparation_reuses_exact_owners_and_accounts_backing_once() {
        let (problem, field) =
            super::super::build_probability::inverse_dependency_build_fixture(8, false);
        let mut root = ExtendedBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .unwrap();
        let before = root.checked_retained_bytes().unwrap();
        let prepared = root.shared_build_preparation().unwrap().unwrap();
        let prepared_bytes = prepared.checked_retained_bytes().unwrap();
        assert!(prepared_bytes > core::mem::size_of::<ExtendedBuildPreparedInputs>() as u128);
        assert_eq!(root.checked_retained_bytes(), Some(before + prepared_bytes));
        let repeated = root.shared_build_preparation().unwrap().unwrap();
        assert!(Arc::ptr_eq(&prepared.0, &repeated.0));
        assert_eq!(root.checked_retained_bytes(), Some(before + prepared_bytes));

        let mut workers = Vec::new();
        for _ in 0..3 {
            let worker = ExtendedBuildProbabilitySession::new_shared_build_verifier(
                &prepared,
                field,
                BuildProbabilityAggregation::Buildability,
                root.memory_bound,
                0,
            )
            .unwrap();
            assert!(Arc::ptr_eq(&root.problem, &worker.problem));
            assert!(Arc::ptr_eq(&root.catalog, &worker.catalog));
            assert!(worker.shared_immutable_owners);
            assert!(!worker.geometry.is_compiling());
            assert_eq!(worker.geometry.retained_bytes(), 0);
            assert_eq!(
                worker.checked_retained_bytes(),
                worker.checked_non_problem_retained_bytes()
            );
            workers.push(worker);
        }
        // Root accounting never changes with Arc reference counts, including
        // after the producer's compiler has completed and releases its targets.
        assert_eq!(root.checked_retained_bytes(), Some(before + prepared_bytes));
        let control = ExecutionControl::default();
        let first_packet = loop {
            match root.advance_distributed_geometry(0, &control).unwrap() {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => break candidate,
                _ => panic!("the forced I fixture has a candidate"),
            }
        };
        workers[0]
            .process_external_candidate(&first_packet, &control)
            .unwrap();
        assert_eq!(workers[0].covered_patterns.count_ones(), 1);
        assert_eq!(workers[1].covered_patterns.count_ones(), 0);
        loop {
            match root.advance_distributed_geometry(0, &control).unwrap() {
                WasmCandidateProducerAdvance::Pending
                | WasmCandidateProducerAdvance::Candidate(_) => {}
                WasmCandidateProducerAdvance::Completed(_) => break,
                WasmCandidateProducerAdvance::Cancelled => panic!("not cancelled"),
            }
        }
        let retained_with_workers = root.checked_retained_bytes();
        drop(workers);
        assert_eq!(root.checked_retained_bytes(), retained_with_workers);
        assert!(root.shared_build_preparation.is_some());
    }

    #[test]
    fn shared_build_verifier_matches_reconstructed_source_ids_and_evidence() {
        use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

        let partial_query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(8, 0),
            PcQueueInput::pattern_expression(QueuePatternExpression::parse("[IO]!", 2).unwrap()),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let partial_problem = ProblemCompiler::compile_scenario_pc(&partial_query).unwrap();
        let partial_field =
            BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0xf, 0, 0, 0]).unwrap();
        let fixtures = [
            super::super::build_probability::inverse_dependency_build_fixture(8, false),
            super::super::build_probability::inverse_dependency_build_fixture(8, true),
            (partial_problem, partial_field),
        ];
        for (problem, field) in fixtures {
            let mut root = ExtendedBuildProbabilitySession::new(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
            )
            .unwrap();
            let prepared = root.shared_build_preparation().unwrap().unwrap();
            let mut shared = ExtendedBuildProbabilitySession::new_shared_build_verifier(
                &prepared,
                field,
                BuildProbabilityAggregation::Buildability,
                root.memory_bound,
                0,
            )
            .unwrap();
            let mut reconstructed =
                ExtendedBuildProbabilitySession::new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
                    &problem,
                    field,
                    BuildProbabilityAggregation::Buildability,
                    root.memory_bound,
                    0,
                )
                .unwrap();
            assert!(!Arc::ptr_eq(&shared.catalog, &reconstructed.catalog));
            let control = ExecutionControl::default();
            loop {
                match root.advance_distributed_geometry(0, &control).unwrap() {
                    WasmCandidateProducerAdvance::Pending => {}
                    WasmCandidateProducerAdvance::Candidate(candidate) => {
                        shared
                            .process_external_candidate(&candidate, &control)
                            .unwrap();
                        reconstructed
                            .process_external_candidate(&candidate, &control)
                            .unwrap();
                    }
                    WasmCandidateProducerAdvance::Completed(_) => break,
                    WasmCandidateProducerAdvance::Cancelled => panic!("not cancelled"),
                }
            }
            let shared = shared.complete_distributed_worker().unwrap();
            let reconstructed = reconstructed.complete_distributed_worker().unwrap();
            for key in [
                "count_complete",
                "probability_complete",
                "packing_candidate_count",
                "packing_candidate_set_digest",
                "piece_source_id",
                "pattern_universe_id",
                "pattern_weight_model_id",
                "unique_solution_count",
                "coverage_probability",
                "coverage_probability_denominator",
                "solution_probability_complete",
            ] {
                assert_eq!(shared.field(key), reconstructed.field(key), "{key}");
            }
            assert_eq!(
                shared.coverage_pattern_words(),
                reconstructed.coverage_pattern_words()
            );
            assert_eq!(
                shared.normalized_solution_keys(),
                reconstructed.normalized_solution_keys()
            );
            assert_eq!(
                shared.normalized_solution_coverages(),
                reconstructed.normalized_solution_coverages()
            );
            assert_eq!(
                shared.solution_probabilities(),
                reconstructed.solution_probabilities()
            );
            assert_eq!(shared.path_steps(), reconstructed.path_steps());
        }
    }

    #[test]
    fn shared_build_preparation_rejects_wrong_pass_and_preflights_new_backing() {
        let (problem, field) =
            super::super::build_probability::inverse_dependency_build_fixture(8, false);
        let mut root = ExtendedBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .unwrap();
        let observed = root.checked_retained_bytes().unwrap();
        let future = root
            .geometry
            .checked_external_verifier_future_bytes()
            .unwrap()
            + core::mem::size_of::<ExtendedBuildPreparedInputs>() as u128
            + 32;
        let original_bound = root.memory_bound;
        root.memory_bound = original_bound.with_cap(observed + future - 1).unwrap();
        assert!(matches!(
            root.shared_build_preparation(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert!(root.shared_build_preparation.is_none());
        assert_eq!(root.checked_retained_bytes(), Some(observed));
        root.memory_bound = original_bound;
        let prepared = root.shared_build_preparation().unwrap().unwrap();
        let foreign_field =
            BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0xf0, 0, 0, 0])
                .unwrap();
        assert!(matches!(
            ExtendedBuildProbabilitySession::new_shared_build_verifier(
                &prepared,
                foreign_field,
                BuildProbabilityAggregation::Buildability,
                original_bound,
                0,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "extended_build_shared_preparation_request_mismatch"
            ))
        ));
        let tiny = original_bound.with_cap(1).unwrap();
        assert!(matches!(
            ExtendedBuildProbabilitySession::new_shared_build_verifier(
                &prepared,
                field,
                BuildProbabilityAggregation::Buildability,
                tiny,
                0,
            ),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        let mut verifier = ExtendedBuildProbabilitySession::new_shared_build_verifier(
            &prepared,
            field,
            BuildProbabilityAggregation::Buildability,
            original_bound,
            0,
        )
        .unwrap();
        // A preparation authorizes no unchecked transport packet.
        let invalid = WasmCandidatePacket::for_extended_pass(0, 0, vec![u32::MAX]);
        assert!(matches!(
            verifier.process_external_candidate(&invalid, &ExecutionControl::default()),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_candidate_invalid"
            ))
        ));
    }

    #[test]
    fn shared_extended_identity_resolves_only_matching_full_height_catalog_rows() {
        use clearra_core_domain::board::standard_pc_board::Board256Mask;
        let height = 24;
        let base = Board256Mask::singleton(190)
            .unwrap()
            .union(Board256Mask::singleton(239).unwrap());
        let target = [200, 210, 220, 230]
            .into_iter()
            .fold(Board256Mask::EMPTY, |mask, bit| {
                mask.union(Board256Mask::singleton(bit).unwrap())
            });
        let field = BuildProbabilityField::from_words_preserving_height(
            height,
            base.words(),
            target.words(),
        )
        .unwrap();
        let catalog = ExtendedInverseCatalog::compile(field).unwrap();
        let mask_hex = |mask: Board256Mask| {
            let words = mask.words();
            format!(
                "{:016x}{:016x}{:016x}{:016x}",
                words[3], words[2], words[1], words[0]
            )
        };
        let key = format!(
            "ctk2|height=24|initial={}|placements=I:{}",
            mask_hex(base),
            mask_hex(target)
        );
        let rows = extended_row_ids_from_canonical_key(&key, height, &catalog).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(catalog.skeleton(rows[0]).cells.words(), target.words());
        assert!(extended_row_ids_from_canonical_key(&key, 23, &catalog).is_err());
        let wrong_initial = key.replace(&mask_hex(base), &mask_hex(Board256Mask::EMPTY));
        assert!(extended_row_ids_from_canonical_key(&wrong_initial, height, &catalog).is_err());
        assert!(extended_row_ids_from_canonical_key(
            &key.replace("placements=I:", "placements=O:"),
            height,
            &catalog
        )
        .is_err());
        assert!(extended_row_ids_from_canonical_key(
            &key.replace("height=24", "height=024"),
            height,
            &catalog
        )
        .is_err());
        assert!(extended_row_ids_from_canonical_key(
            &format!("{key},I:{}", mask_hex(target)),
            height,
            &catalog
        )
        .is_err());
    }

    #[test]
    fn extended_retained_bytes_count_owned_problem_nested_heap_once() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(24, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("extended problem");
        let field = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0; 4])
            .expect("extended field");
        let session = ExtendedBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("extended session");
        let source_pointee = problem
            .checked_build_probability_pointee_retained_bytes()
            .expect("typed BuildProbability problem");
        let source_nested = source_pointee
            .checked_sub(core::mem::size_of::<SearchProblem>() as u128)
            .expect("pointee includes its inline owner");
        let owned_pointee = session
            .problem
            .checked_build_probability_pointee_retained_bytes()
            .expect("owned typed BuildProbability problem");
        let owned_nested = owned_pointee
            .checked_sub(core::mem::size_of::<SearchProblem>() as u128)
            .expect("owned pointee includes its inline owner");

        assert!(source_nested > 0);
        assert!(owned_nested > 0);
        assert!(!session.shared_immutable_owners);
        assert_eq!(Arc::strong_count(&session.problem), 1);
        assert_eq!(
            session.checked_retained_bytes(),
            session
                .checked_non_problem_retained_bytes()
                // The Arc pointee is no longer inline in the session. Count
                // its whole allocation once, plus the conservative 32-byte
                // Arc header/alignment allowance, not only its nested heap.
                .and_then(|bytes| bytes.checked_add(owned_pointee))
                .and_then(|bytes| bytes.checked_add(32))
        );
        assert_eq!(
            crate::backend::wasm_cpu::build_probability::
                checked_build_probability_problem_nested_retained_bytes(&session.problem),
            Some(owned_nested)
        );
        assert_eq!(
            crate::backend::wasm_cpu::build_probability::
                checked_build_probability_problem_nested_retained_bytes(&problem),
            Some(source_nested)
        );
    }

    #[test]
    fn extended_pc_terminal_projection_moves_private_solution_owners() {
        use clearra_core_domain::board::standard_pc_board::Board256Mask;
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend};
        use clearra_rules::profile::builtin_rules::srs_plus;

        let control = ExecutionControl::default();
        for height in [7_u8, 24] {
            // The same forced vertical I holes used by the App full-height
            // fixtures. The upper rows must survive identity projection.
            let starts = if height == 7 {
                vec![0_u16, 3]
            } else {
                (0..u16::from(height)).step_by(4).collect()
            };
            let pieces = starts.len();
            let mut target = Board256Mask::EMPTY;
            for (column, start) in starts.into_iter().enumerate() {
                for row in start..start + 4 {
                    target = target.union(
                        Board256Mask::singleton(row * 10 + column as u16).expect("fixture cell"),
                    );
                }
            }
            let initial = Board256Mask::all_cells(u16::from(height) * 10)
                .expect("fixture size")
                .without(target);
            let field = BuildProbabilityField::from_words_preserving_height(
                height,
                initial.words(),
                target.words(),
            )
            .expect("full-height fixture");
            let mut baseline: Option<CoreExecutionResult> = None;
            for policy in [
                PcSolutionProbabilityPolicy::Omit,
                PcSolutionProbabilityPolicy::Include,
            ] {
                let query = PcScenarioQuery::new(
                    PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words())
                        .expect("fixture board"),
                    PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
                    PieceWindow::new(pieces),
                )
                .with_rule(srs_plus())
                .with_exact_pieces(Some(pieces))
                .with_allow_hold(false)
                .with_min_remaining_queue(0)
                .with_count_policy(PcCountPolicy::CountAll)
                .with_objective(ObjectivePolicy::all())
                .with_retained_trace_limit(1)
                .with_solution_probability_policy(policy)
                .with_execution_policy(
                    PcExecutionPolicy::default()
                        .with_requested_backend(RequestedSearchBackend::Cpu)
                        .with_workers(1)
                        .with_allow_backend_fallback(false),
                );
                let problem = ProblemCompiler::compile_scenario_pc(&query).expect("fixture PC");
                let memory_bound =
                    ExecutionMemoryBound::unbounded_for_problem(&problem).expect("memory owner");
                let mut session = ExtendedBuildProbabilitySession::new_pc_family(
                    &problem,
                    field,
                    memory_bound,
                    0,
                )
                .expect("ordinary full-height PC session");
                let mut saw_private_tiling = false;
                let mut private_row_backing = None;
                let mut completed = None;
                for _ in 0..10_000 {
                    match session
                        .advance(1, &control)
                        .expect("cooperative fixture search")
                    {
                        BuildProbabilityAdvance::Pending => {
                            saw_private_tiling |= !session.buildable_tilings.is_empty();
                            private_row_backing =
                                session.solution_coverage.as_ref().and_then(|rows| {
                                    rows.iter()
                                        .next()
                                        .map(|(key, row)| (key.as_ptr(), row.storage_component(0)))
                                });
                        }
                        BuildProbabilityAdvance::Completed(result) => {
                            completed = Some(result);
                            break;
                        }
                        BuildProbabilityAdvance::Cancelled => panic!("unexpected cancellation"),
                    }
                }
                let result = completed.expect("small fixture must finish within its work bound");
                assert!(saw_private_tiling);
                assert_eq!(session.buildable_tilings.capacity(), 0);
                assert!(session.solution_coverage.is_none());
                assert_eq!(result.normalized_solution_keys().len(), 1);
                let identity = ExtendedTilingSolutionKey::parse_canonical(
                    &result.normalized_solution_keys()[0],
                )
                .expect("exact published identity");
                assert_eq!(identity.initial_board(), initial);
                assert_eq!(identity.placement_count(), pieces);
                let placed = identity
                    .placements()
                    .fold(Board256Mask::EMPTY, |board, placement| {
                        board.union(placement.cells())
                    });
                assert_eq!(placed, target);
                assert_eq!(result.coverage_pattern_words(), &[1]);
                assert_eq!(result.bool_field("count_complete"), Some(true));
                assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
                assert_eq!(
                    result.bool_field("representative_physical_replay_validated"),
                    Some(true)
                );
                if let Some(baseline) = &baseline {
                    assert_eq!(
                        result.normalized_solution_keys(),
                        baseline.normalized_solution_keys()
                    );
                    assert_eq!(
                        result.field("normalized_solution_set_hash"),
                        baseline.field("normalized_solution_set_hash")
                    );
                    assert_eq!(result.path_steps(), baseline.path_steps());
                    let [coverage] = result.normalized_solution_coverages() else {
                        panic!("Include must return the exact coverage row");
                    };
                    assert_eq!(
                        coverage.solution_key(),
                        &result.normalized_solution_keys()[0]
                    );
                    let original = private_row_backing.expect("producer row must have existed");
                    assert!(original.1.is_some());
                    assert_eq!(
                        (
                            coverage.solution_key().as_ptr(),
                            coverage.covered_patterns().storage_component(0)
                        ),
                        original,
                    );
                    assert_eq!(coverage.covered_patterns().word_at(0), 1);
                    assert_eq!(result.solution_probabilities()[0].probability(), "1");
                } else {
                    assert!(private_row_backing.is_none());
                    assert!(result.normalized_solution_coverages().is_empty());
                    baseline = Some(result);
                }
            }
        }
    }

    #[test]
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    fn extended_pc_sealed_terminal_preserves_direct_result_and_admission() {
        use crate::resource::SharedExecutionMemoryBudget;
        use clearra_core_domain::{
            board::standard_pc_board::Board256Mask,
            execution_cancellation::ExecutionCancellationToken,
        };
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend};
        use clearra_rules::profile::builtin_rules::srs_plus;

        let height = 7;
        let mut target = Board256Mask::EMPTY;
        for (column, start_row) in [(0, 0), (1, 3)] {
            for row in start_row..start_row + 4 {
                target = target.union(Board256Mask::singleton(row * 10 + column).unwrap());
            }
        }
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(target);
        let field = BuildProbabilityField::from_words_preserving_height(
            height,
            initial.words(),
            target.words(),
        )
        .unwrap();
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 2])),
            PieceWindow::new(2),
        )
        .with_rule(srs_plus())
        .with_exact_pieces(Some(2))
        .with_allow_hold(false)
        .with_min_remaining_queue(0)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all())
        .with_retained_trace_limit(1)
        .with_execution_policy(
            PcExecutionPolicy::default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_allow_backend_fallback(false),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let bound = ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
        let control = ExecutionControl::default();
        let make_terminal_source = || {
            let mut session =
                ExtendedBuildProbabilitySession::new_pc_family(&problem, field, bound, 0).unwrap();
            for _ in 0..10_000 {
                match session.advance_geometry_bounded().unwrap() {
                    ExtendedGeometryAdvance::Pending => {}
                    ExtendedGeometryAdvance::Candidate(candidate) => {
                        let ordinal = session.geometry.candidate_count().saturating_sub(1) as u64;
                        session
                            .process_candidate(candidate, Some(ordinal), &control)
                            .unwrap();
                    }
                    ExtendedGeometryAdvance::Complete => return session,
                    ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                        panic!("small fixture did not complete Geometry: {reason}")
                    }
                }
            }
            panic!("small fixture exceeded its work limit")
        };
        let BuildProbabilityAdvance::Completed(direct) = make_terminal_source()
            .complete_with_control(&control)
            .unwrap()
        else {
            panic!("direct terminal did not complete");
        };
        assert_eq!(direct.normalized_solution_keys().len(), 1);
        assert_eq!(direct.coverage_pattern_words(), &[1]);
        assert!(!direct.path_steps().is_empty());
        let family = || {
            let keys = direct.normalized_solution_keys().to_vec();
            let key_text_bytes = keys.iter().map(|s| s.capacity() as u128).sum();
            SealedPcFamily {
                keys,
                hash: direct
                    .field("normalized_solution_set_hash")
                    .unwrap()
                    .to_owned(),
                key_text_bytes,
            }
        };

        let mut sealed = make_terminal_source();
        // The cooperative reducer moves these away before final assembly.
        drop(core::mem::take(&mut sealed.buildable_tilings));
        let shared_bytes = sealed.checked_retained_bytes().unwrap();
        let probe = SharedExecutionMemoryBudget::new(bound, shared_bytes, 1).unwrap();
        let candidate = family();
        let projected = (candidate.keys.capacity() as u128)
            * (core::mem::size_of::<String>() as u128)
            + candidate.key_text_bytes
            + candidate.hash.capacity() as u128
            + sealed
                .checked_result_materialization_future_bytes()
                .unwrap();
        probe.owner(0).ensure(projected).unwrap();
        let required = probe.admitted_peak_bytes();
        drop(probe);
        let exact =
            SharedExecutionMemoryBudget::new(bound.with_cap(required).unwrap(), shared_bytes, 1)
                .unwrap();
        let too_short = SharedExecutionMemoryBudget::new(
            bound.with_cap(required - 1).unwrap(),
            shared_bytes,
            1,
        )
        .unwrap();
        let representative = core::mem::take(&mut sealed.representative_path);
        assert!(matches!(
            sealed.complete_pc_family_with_sealed_keys(family(), &exact.owner(0), &control),
            Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_representative_replay_missing"
            ))
        ));
        sealed.representative_path = representative;
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        assert!(matches!(
            sealed.complete_pc_family_with_sealed_keys(
                family(),
                &exact.owner(0),
                &ExecutionControl::new(cancellation)
            ),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(matches!(
            sealed.complete_pc_family_with_sealed_keys(family(), &too_short.owner(0), &control),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        let received = family();
        let backing = received.keys.as_ptr();
        let result = sealed
            .complete_pc_family_with_sealed_keys(received, &exact.owner(0), &control)
            .unwrap();
        assert_eq!(result.normalized_solution_keys().as_ptr(), backing);
        assert_eq!(
            result.normalized_solution_keys(),
            direct.normalized_solution_keys()
        );
        assert_eq!(result.path_steps(), direct.path_steps());
        assert_eq!(
            result.coverage_pattern_words(),
            direct.coverage_pattern_words()
        );
        for field in [
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
            "build_variant_count",
            "build_variant_count_exact",
            "count_complete",
            "probability_complete",
            "coverage_probability",
            "packing_candidate_digest",
            "packing_candidate_count",
            "representative_physical_replay_validated",
        ] {
            assert_eq!(result.field(field), direct.field(field), "{field}");
        }
        assert!(matches!(
            sealed.complete_pc_family_with_sealed_keys(family(), &exact.owner(0), &control),
            Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_sealed_result_contract_invalid"
            ))
        ));
    }

    #[test]
    fn extended_result_materialization_projection_has_an_exact_one_byte_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(24, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty-target problem");
        let field = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0; 4])
            .expect("empty extended field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = ExtendedBuildProbabilitySession::new_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("extended session");
        let required = session
            .checked_retained_bytes()
            .and_then(|retained| {
                session
                    .checked_result_materialization_future_bytes()
                    .and_then(|future| retained.checked_add(future))
            })
            .expect("checked materialization projection");
        session.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short bound");
        assert!(matches!(
            session.ensure_result_materialization_bound(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        session.memory_bound = unbounded.with_cap(required).expect("exact bound");
        session
            .ensure_result_materialization_bound()
            .expect("exact projection fits");
    }

    #[test]
    fn finite_extended_initial_peak_counts_coexisting_owner_bytes() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(24, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty-target problem");
        let field = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0; 4])
            .expect("empty extended field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let coexisting_retained_bytes = 509_u128;
        let session =
            ExtendedBuildProbabilitySession::new_with_memory_bound_and_coexisting_retained_bytes(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                false,
                unbounded,
                coexisting_retained_bytes,
            )
            .expect("finite-shaped extended session");

        assert_eq!(session.coexisting_retained_bytes, coexisting_retained_bytes);
        session
            .ensure_memory_bound(0)
            .expect("coexisting owner fits the unbounded initial peak");
    }

    #[test]
    fn extended_distributed_candidate_rows_have_an_exact_allocator_capacity_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(24, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("extended test problem");
        let field = BuildProbabilityField::from_words_preserving_height(24, [0; 4], [0; 4])
            .expect("empty extended field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = ExtendedBuildProbabilitySession::new_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("extended session");
        let coexisting = 509_u128;
        session.set_coexisting_retained_bytes(coexisting);
        let source = [2_u32, 5, 13, 29, 31, 37];
        let rows = session
            .try_copy_distributed_candidate_row_ids(&source)
            .expect("unbounded candidate row copy");
        let actual_row_bytes = (rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .expect("checked candidate row capacity");
        let required = session
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(coexisting))
            .and_then(|bytes| bytes.checked_add(actual_row_bytes))
            .expect("checked candidate row peak");
        drop(rows);

        session.memory_bound = unbounded.with_cap(required).expect("exact bound");
        assert_eq!(
            session
                .try_copy_distributed_candidate_row_ids(&source)
                .expect("the exact candidate row peak must fit"),
            source
        );
        session.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short bound");
        assert!(matches!(
            session.try_copy_distributed_candidate_row_ids(&source),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
    }
}

// SRP rationale: this module has one behavior-level change reason: exact extended-board build-probability evaluation.
