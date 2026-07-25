use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_coverage::{
    cover::exact_minimum_cover::exact_minimum_cover, pattern::pattern_bitset::PatternBitSet,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_problem::SearchProblem;
use clearra_replay::ExactScoringExecutionBatch;

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    CoreExecutionResult, CorePathStep,
};

#[cfg(feature = "parallel")]
use super::parallel_search::{self, ParallelSearchDecision, ParallelSearchOutcome};
use super::{
    buildup::{exact_scoring_execution_graph, verify_candidate, BuildUpWorkspace},
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    exact_collections::{ExactHashMap, ExactHashSet},
    geometry::{compile_target_groups, GeometryAdvance, GeometryCandidate, GeometrySearch},
    kick_profiles::replay_profile_ids,
    mix_digest,
    reachability::ReachabilityMetrics,
    standard_bag_coverage::StandardBagCoverage,
    WasmExactSearchError, MAX_BOARD64_PIECES,
};
use crate::solution_probability::{
    covers_all_identities, probability_reports, NormalizedSolutionCoverage, SolutionCoverage,
    SolutionProbabilityReport,
};

// Keep browser-worker cancellation responsive without paying an ABI/event-loop
// round trip after every tiny candidate batch.
const MAX_BUILDUP_CANDIDATES_PER_ADVANCE: usize = 512;

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
#[inline]
const fn profile_sample_scale(ordinal: usize) -> u64 {
    if ordinal <= 4_096 {
        1
    } else if ordinal & 1_023 == 0 {
        1_024
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug)]
struct TilingIdentityEntry {
    bucket_hash: u64,
    identity: StandardBoard64TilingIdentity,
}

impl TilingIdentityEntry {
    fn new(identity: StandardBoard64TilingIdentity) -> Self {
        Self {
            bucket_hash: identity.bucket_hash(),
            identity,
        }
    }
}

impl PartialEq for TilingIdentityEntry {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for TilingIdentityEntry {}

impl Hash for TilingIdentityEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.bucket_hash);
    }
}

pub(crate) enum ExactSearchAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub(super) enum DistributedGeometryAdvance {
    Pending,
    Candidate {
        target_index: u32,
        row_ids: Vec<u32>,
        identity_hash: u64,
    },
    Complete,
    ResourceIncomplete(&'static str),
}

pub(crate) struct WasmExactSearchSession {
    problem: Arc<SearchProblem>,
    catalog: Arc<GeometryCatalog>,
    geometry: GeometrySearch,
    buildup_workspace: BuildUpWorkspace,
    coverage_evaluator: CoverageProductEvaluator,
    covered_patterns: PatternBitSet,
    buildable_identities: ExactHashSet<TilingIdentityEntry>,
    solution_coverage: Option<ExactHashMap<StandardBoard64TilingIdentity, PatternBitSet>>,
    solution_coverage_bytes: usize,
    packing_candidate_count: usize,
    packing_candidate_digest: u64,
    coverage_row_count: usize,
    pattern_verified_execution_count: usize,
    build_variant_count: u128,
    count_complete: bool,
    representative_path: Vec<CorePathStep>,
    representative_rank: Option<u64>,
    representative_identity: Option<StandardBoard64TilingIdentity>,
    representative_candidate_id: Option<u64>,
    representative_pattern_id: Option<u32>,
    peak_build_nodes: usize,
    total_build_nodes: usize,
    coverage_product_words: usize,
    coverage_product_states: usize,
    coverage_product_edge_checks: usize,
    realization_feasibility_states: usize,
    realization_feasibility_rejected_candidates: usize,
    peak_reachability_states: usize,
    total_reachability_states: usize,
    peak_cpu_bytes: usize,
    parallel_worker_retained_bytes: usize,
    parallel_piece_language_cache_hits: usize,
    parallel_piece_language_cache_misses: usize,
    parallel_standard_bag_cache_hits: usize,
    parallel_standard_bag_cache_misses: usize,
    parallel_reachability_metrics: ReachabilityMetrics,
    workers_used: usize,
    parallel_active_workers: usize,
    parallel_minimum_worker_candidates: usize,
    parallel_maximum_worker_candidates: usize,
    parallel_decision_reason: &'static str,
    distributed_execution_constraint_materialized: bool,
    cpu_warmup_requested: bool,
    cpu_warmup_performed: bool,
    gpu_warmup_requested: bool,
    gpu_warmup_performed: bool,
    gpu_session_reused: bool,
    backend_selected: &'static str,
    backend_fallback_used: bool,
    backend_fallback_reason: &'static str,
    fallback_backend: Option<&'static str>,
    gpu_failure_class: Option<&'static str>,
    gpu_failure_stage: Option<&'static str>,
    discarded_partial_gpu_result: bool,
    gpu_original_result_incomplete: bool,
    gpu_adapter_index: Option<u8>,
    gpu_adapter_name: Option<String>,
    gpu_adapter_type: Option<&'static str>,
    gpu_adapter_backend: Option<String>,
    gpu_peak_bytes: u64,
    gpu_shader_hash: Option<String>,
    gpu_shader_version: Option<&'static str>,
    truncated_reason: Option<&'static str>,
    finished: bool,
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    profile_geometry_advance_calls: usize,
}

impl WasmExactSearchSession {
    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, false)
    }

    pub fn new_external_geometry(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, true)
    }

    pub(super) fn geometry_expanded_nodes(&self) -> usize {
        self.geometry.expanded_nodes()
    }

    pub(super) fn distributed_progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.geometry.expanded_nodes(),
            candidates: self.packing_candidate_count,
            candidate_family_count: self.geometry.candidate_family_count(),
            build_nodes: self.total_build_nodes,
            coverage_checks: self.coverage_product_edge_checks,
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    fn new_with_external_geometry(
        problem: &SearchProblem,
        external_geometry: bool,
    ) -> Result<Self, WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
        let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionCatalogCompile);
        let catalog = Arc::new(GeometryCatalog::compile(problem)?);
        catalog_span.finish(catalog.skeleton_count() as u64);
        let target_piece_count = catalog.required_cells().count_ones() as usize / 4;
        if target_piece_count > MAX_BOARD64_PIECES {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_board64_piece_count_exceeds_exact_limit",
            ));
        }
        if problem
            .exact_pieces()
            .is_some_and(|exact| exact != target_piece_count)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_exact_piece_count_does_not_match_required_area",
            ));
        }
        let supply_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionSupplyCompile);
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let multiset_family = universe.packing_multiset_family(
            target_piece_count,
            problem.initial_hold(),
            super::packing_projection_hold_enabled(problem),
        );
        if multiset_family.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_supply_has_no_reachable_piece_multiset",
            ));
        }
        supply_span.finish(universe.pattern_count() as u64);
        let covered_patterns = PatternBitSet::new(universe.pattern_count());
        let uses_symbolic_standard_bag = problem.count_policy()
            == clearra_pc_graph::request::PcCountPolicy::CountUnique
            && StandardBagCoverage::supports(universe, problem.initial_hold());
        let geometry_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionGeometryPrepare);
        let geometry = if external_geometry {
            let (targets, pattern_index_bytes) =
                compile_target_groups(universe, &multiset_family, !uses_symbolic_standard_bag)?;
            GeometrySearch::external(targets, pattern_index_bytes)
        } else {
            GeometrySearch::new(
                universe,
                &multiset_family,
                catalog.required_cells(),
                !uses_symbolic_standard_bag,
            )?
        };
        geometry_span.finish(geometry.targets().map_or(0, |targets| targets.len()) as u64);
        let peak_cpu_bytes = catalog.retained_bytes() + geometry.retained_bytes();
        Ok(Self {
            problem: Arc::new(problem.clone()),
            catalog,
            geometry,
            buildup_workspace: BuildUpWorkspace::default(),
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns,
            buildable_identities: ExactHashSet::default(),
            solution_coverage: (problem.solution_probability_policy().requested()
                || problem.objective().kind() == ObjectiveKind::MinimumCover
                || problem.objective().execution_constraints().requested())
            .then(ExactHashMap::default),
            solution_coverage_bytes: 0,
            packing_candidate_count: 0,
            packing_candidate_digest: 0,
            coverage_row_count: 0,
            pattern_verified_execution_count: 0,
            build_variant_count: 0,
            count_complete: true,
            representative_path: Vec::new(),
            representative_rank: None,
            representative_identity: None,
            representative_candidate_id: None,
            representative_pattern_id: None,
            peak_build_nodes: 0,
            total_build_nodes: 0,
            coverage_product_words: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            realization_feasibility_states: 0,
            realization_feasibility_rejected_candidates: 0,
            peak_reachability_states: 0,
            total_reachability_states: 0,
            peak_cpu_bytes,
            parallel_worker_retained_bytes: 0,
            parallel_piece_language_cache_hits: 0,
            parallel_piece_language_cache_misses: 0,
            parallel_standard_bag_cache_hits: 0,
            parallel_standard_bag_cache_misses: 0,
            parallel_reachability_metrics: ReachabilityMetrics::default(),
            workers_used: 1,
            parallel_active_workers: 1,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            parallel_decision_reason: if cfg!(feature = "parallel") {
                "not-evaluated"
            } else {
                "parallel-feature-disabled"
            },
            distributed_execution_constraint_materialized: false,
            cpu_warmup_requested: problem.backend_policy().cpu_warmup(),
            cpu_warmup_performed: false,
            gpu_warmup_requested: problem.backend_policy().gpu_warmup(),
            gpu_warmup_performed: false,
            gpu_session_reused: false,
            backend_selected: if external_geometry {
                "webgpu"
            } else {
                "wasm-cpu"
            },
            backend_fallback_used: false,
            backend_fallback_reason: "none",
            fallback_backend: None,
            gpu_failure_class: None,
            gpu_failure_stage: None,
            discarded_partial_gpu_result: false,
            gpu_original_result_incomplete: false,
            gpu_adapter_index: None,
            gpu_adapter_name: None,
            gpu_adapter_type: None,
            gpu_adapter_backend: None,
            gpu_peak_bytes: 0,
            gpu_shader_hash: None,
            gpu_shader_version: None,
            truncated_reason: None,
            finished: false,
            #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
            profile_geometry_advance_calls: 0,
        })
    }

    #[cfg(feature = "webgpu-search")]
    pub(super) fn catalog(&self) -> Arc<GeometryCatalog> {
        Arc::clone(&self.catalog)
    }

    #[cfg(feature = "webgpu-search")]
    pub(super) fn geometry_targets(&self) -> Option<&[super::geometry::TargetGroup]> {
        self.geometry.targets()
    }

    pub fn mark_webgpu_execution(
        &mut self,
        adapter_index: u8,
        adapter_name: String,
        adapter_type: &'static str,
        adapter_backend: String,
        peak_gpu_bytes: u64,
        shader_hash: String,
        shader_version: &'static str,
        warmup_performed: bool,
        session_reused: bool,
    ) {
        self.backend_selected = "webgpu";
        self.gpu_adapter_index = Some(adapter_index);
        self.gpu_adapter_name = Some(adapter_name);
        self.gpu_adapter_type = Some(adapter_type);
        self.gpu_adapter_backend = Some(adapter_backend);
        self.gpu_peak_bytes = peak_gpu_bytes;
        self.gpu_shader_hash = Some(shader_hash);
        self.gpu_shader_version = Some(shader_version);
        self.gpu_warmup_performed = warmup_performed;
        self.gpu_session_reused = session_reused;
    }

    pub fn mark_cpu_fallback(
        &mut self,
        reason: &'static str,
        failure_class: &'static str,
        failure_stage: &'static str,
        discarded_partial_result: bool,
        original_result_incomplete: bool,
    ) {
        self.backend_selected = "wasm-cpu";
        self.backend_fallback_used = true;
        self.backend_fallback_reason = reason;
        self.fallback_backend = Some("wasm-cpu");
        self.gpu_failure_class = Some(failure_class);
        self.gpu_failure_stage = Some(failure_stage);
        self.discarded_partial_gpu_result = discarded_partial_result;
        self.gpu_original_result_incomplete = original_result_incomplete;
    }

    #[cfg(feature = "parallel")]
    pub fn execute_parallel_if_worthwhile(
        &mut self,
        worker_count: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        if worker_count <= 1 {
            return Ok(None);
        }
        let geometry = std::mem::replace(&mut self.geometry, GeometrySearch::placeholder());
        match parallel_search::execute_if_worthwhile(
            Arc::clone(&self.problem),
            Arc::clone(&self.catalog),
            geometry,
            control.clone(),
            worker_count,
            self.cpu_warmup_requested,
        )? {
            ParallelSearchDecision::Serial { geometry, reason } => {
                self.geometry = geometry;
                self.parallel_decision_reason = reason;
                Ok(None)
            }
            ParallelSearchDecision::Completed(outcome) => {
                self.parallel_decision_reason = "parallel-immutable-family-queue";
                self.cpu_warmup_performed = self.cpu_warmup_requested;
                self.absorb_parallel_outcome(outcome)?;
                match self.complete()? {
                    ExactSearchAdvance::Completed(result) => Ok(Some(result)),
                    ExactSearchAdvance::Pending | ExactSearchAdvance::Cancelled => {
                        Err(WasmExactSearchError::InvalidProblem(
                            "wasm_parallel_search_completion_invalid",
                        ))
                    }
                }
            }
        }
    }

    #[cfg(feature = "parallel")]
    fn absorb_parallel_outcome(
        &mut self,
        outcome: ParallelSearchOutcome,
    ) -> Result<(), WasmExactSearchError> {
        self.geometry = outcome.geometry;
        self.covered_patterns
            .union_with(&outcome.covered_patterns)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_parallel_coverage_universe_mismatch")
            })?;
        for identity in outcome.buildable_identities {
            let identity = TilingIdentityEntry::new(identity);
            if !self.buildable_identities.contains(&identity)
                && self.buildable_identities.try_reserve(1).is_err()
            {
                self.mark_truncated("solution_identity_storage_unavailable");
                break;
            }
            self.buildable_identities.insert(identity);
        }
        for coverage in outcome.solution_coverage {
            self.merge_solution_coverage(coverage.identity(), coverage.covered_patterns())?;
        }
        self.packing_candidate_count = outcome.packing_candidate_count;
        self.packing_candidate_digest = outcome.packing_candidate_digest;
        self.coverage_row_count = outcome.coverage_row_count;
        self.pattern_verified_execution_count = outcome.pattern_verified_execution_count;
        self.build_variant_count = outcome.build_variant_count;
        self.count_complete &= outcome.count_complete;
        self.representative_path = outcome.representative_path;
        self.representative_candidate_id = outcome.representative_candidate_id;
        self.representative_pattern_id = outcome.representative_pattern_id;
        self.peak_build_nodes = outcome.peak_build_nodes;
        self.total_build_nodes = outcome.total_build_nodes;
        self.coverage_product_words = outcome.coverage_product_words;
        self.coverage_product_states = outcome.coverage_product_states;
        self.coverage_product_edge_checks = outcome.coverage_product_edge_checks;
        self.realization_feasibility_states = outcome.feasibility_states;
        self.realization_feasibility_rejected_candidates = outcome.feasibility_rejected_candidates;
        self.peak_reachability_states = outcome.peak_reachability_states;
        self.total_reachability_states = outcome.total_reachability_states;
        self.parallel_worker_retained_bytes = outcome.worker_retained_bytes;
        self.parallel_piece_language_cache_hits = outcome.piece_language_cache_hits;
        self.parallel_piece_language_cache_misses = outcome.piece_language_cache_misses;
        self.parallel_standard_bag_cache_hits = outcome.standard_bag_cache_hits;
        self.parallel_standard_bag_cache_misses = outcome.standard_bag_cache_misses;
        self.parallel_reachability_metrics = outcome.reachability_metrics;
        self.workers_used = outcome.workers_used;
        self.parallel_active_workers = outcome.active_workers;
        self.parallel_minimum_worker_candidates = outcome.minimum_worker_candidates;
        self.parallel_maximum_worker_candidates = outcome.maximum_worker_candidates;
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
            self.catalog
                .retained_bytes()
                .saturating_add(self.geometry.retained_bytes())
                .saturating_add(self.parallel_worker_retained_bytes)
                .saturating_add(
                    self.buildable_identities.capacity()
                        * core::mem::size_of::<TilingIdentityEntry>(),
                ),
        );
        if let Some(reason) = outcome.truncated_reason {
            self.mark_truncated(reason);
        }
        if self.memory_budget_exceeded() {
            self.mark_truncated("memory_budget_exceeded");
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(ExactSearchAdvance::Cancelled);
        }
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_search_session_already_finished",
            ));
        }
        let work_budget = work_budget.max(1);
        let mut processed_candidates = 0usize;
        for _ in 0..work_budget {
            if control.is_cancelled() {
                return Ok(ExactSearchAdvance::Cancelled);
            }
            let node_budget = self.problem.budget().max_nodes();
            if node_budget != 0 && self.geometry.expanded_nodes() >= node_budget {
                self.truncated_reason = Some("frontier_budget_exceeded");
                self.count_complete = false;
                return self.complete();
            }
            let candidate_budget = self.problem.backend_request().max_candidates();
            if candidate_budget != 0 && self.packing_candidate_count >= candidate_budget {
                self.mark_truncated("candidate_budget_exceeded");
                return self.complete();
            }
            #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
            let geometry_profile_scale = {
                self.profile_geometry_advance_calls =
                    self.profile_geometry_advance_calls.saturating_add(1);
                profile_sample_scale(self.profile_geometry_advance_calls)
            };
            #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
            let geometry_profile_scale = 0;
            let geometry_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::WasmGeometryAdvance,
                geometry_profile_scale,
            );
            let geometry_advance = self.geometry.advance(&self.catalog);
            geometry_span.finish(1);
            match geometry_advance {
                GeometryAdvance::Pending => {}
                GeometryAdvance::Candidate(candidate) => {
                    processed_candidates += 1;
                    if let Some(outcome) = self.process_candidate(candidate, control)? {
                        return Ok(outcome);
                    }
                    if processed_candidates >= MAX_BUILDUP_CANDIDATES_PER_ADVANCE {
                        control.report_progress(
                            "wasm-exact-cover",
                            self.geometry.expanded_nodes() as u64,
                            None,
                        );
                        return Ok(ExactSearchAdvance::Pending);
                    }
                }
                GeometryAdvance::ResourceIncomplete(reason) => {
                    self.mark_truncated(reason);
                    return self.complete();
                }
                GeometryAdvance::Complete => return self.complete(),
            }
        }
        control.report_progress(
            "wasm-exact-cover",
            self.geometry.expanded_nodes() as u64,
            None,
        );
        Ok(ExactSearchAdvance::Pending)
    }

    pub(super) fn advance_distributed_geometry(
        &mut self,
        produced_candidate_count: usize,
    ) -> Result<DistributedGeometryAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_search_session_already_finished",
            ));
        }
        let node_budget = self.problem.budget().max_nodes();
        if node_budget != 0 && self.geometry.expanded_nodes() >= node_budget {
            return Ok(DistributedGeometryAdvance::ResourceIncomplete(
                "frontier_budget_exceeded",
            ));
        }
        let candidate_budget = self.problem.backend_request().max_candidates();
        if candidate_budget != 0 && produced_candidate_count >= candidate_budget {
            return Ok(DistributedGeometryAdvance::ResourceIncomplete(
                "candidate_budget_exceeded",
            ));
        }
        Ok(match self.geometry.advance(&self.catalog) {
            GeometryAdvance::Pending => DistributedGeometryAdvance::Pending,
            GeometryAdvance::Candidate(candidate) => DistributedGeometryAdvance::Candidate {
                target_index: candidate.target_index,
                row_ids: candidate.row_ids().to_vec(),
                identity_hash: candidate.identity.bucket_hash(),
            },
            GeometryAdvance::Complete => DistributedGeometryAdvance::Complete,
            GeometryAdvance::ResourceIncomplete(reason) => {
                DistributedGeometryAdvance::ResourceIncomplete(reason)
            }
        })
    }

    pub(super) fn distributed_geometry_summary(
        &self,
        candidate_count: usize,
        candidate_digest: u64,
        truncated_reason: Option<&'static str>,
    ) -> WasmDistributedGeometrySummary {
        WasmDistributedGeometrySummary {
            candidate_count,
            candidate_digest,
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

    pub(super) fn into_distributed_finalizer(mut self) -> Result<Self, WasmExactSearchError> {
        self.parallel_active_workers = 0;
        self.parallel_minimum_worker_candidates = usize::MAX;
        self.parallel_maximum_worker_candidates = 0;
        self.parallel_decision_reason = "browser-worker-candidate-pipeline";
        if self.problem.objective().execution_constraints().requested()
            && self.solution_coverage.is_none()
        {
            self.solution_coverage = Some(ExactHashMap::default());
        }
        self.distributed_execution_constraint_materialized =
            self.problem.objective().execution_constraints().requested();
        Ok(self)
    }

    pub fn process_external_candidate(
        &mut self,
        target_index: u32,
        row_ids: &[u32],
        control: &ExecutionControl,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_search_session_already_finished",
            ));
        }
        let candidate_budget = self.problem.backend_request().max_candidates();
        if candidate_budget != 0 && self.packing_candidate_count >= candidate_budget {
            self.mark_truncated("candidate_budget_exceeded");
            return self.complete().map(Some);
        }
        let candidate = GeometryCandidate::from_rows(&self.catalog, target_index, row_ids).ok_or(
            WasmExactSearchError::InvalidProblem("webgpu_geometry_candidate_invalid"),
        )?;
        self.process_candidate_ranked(candidate, None, control)
    }

    pub(super) fn process_external_candidate_with_ordinal(
        &mut self,
        target_index: u32,
        row_ids: &[u32],
        ordinal: u64,
        control: &ExecutionControl,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_search_session_already_finished",
            ));
        }
        let candidate = GeometryCandidate::from_rows(&self.catalog, target_index, row_ids).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_distributed_geometry_candidate_invalid"),
        )?;
        self.process_candidate_ranked(candidate, Some(ordinal), control)
    }

    pub(super) fn external_candidate_identity_hash(
        &self,
        target_index: u32,
        row_ids: &[u32],
    ) -> Result<u64, WasmExactSearchError> {
        GeometryCandidate::from_rows(&self.catalog, target_index, row_ids)
            .map(|candidate| candidate.identity.bucket_hash())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_geometry_candidate_invalid",
            ))
    }

    fn process_candidate(
        &mut self,
        candidate: GeometryCandidate,
        control: &ExecutionControl,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        self.process_candidate_ranked(candidate, None, control)
    }

    fn process_candidate_ranked(
        &mut self,
        candidate: GeometryCandidate,
        external_ordinal: Option<u64>,
        control: &ExecutionControl,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        let candidate_ordinal = external_ordinal.unwrap_or(self.packing_candidate_count as u64);
        self.packing_candidate_count += 1;
        self.packing_candidate_digest = mix_digest(
            self.packing_candidate_digest,
            candidate.identity.bucket_hash(),
        );
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        let profile_scale = profile_sample_scale(self.packing_candidate_count);
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        let profile_scale = 0;
        let target = self.geometry.target(candidate.target_index).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_geometry_candidate_target_out_of_range"),
        )?;
        let solution_probabilities_requested =
            self.problem.solution_probability_policy().requested();
        let solution_coverage_required = solution_probabilities_requested
            || self.problem.objective().kind() == ObjectiveKind::MinimumCover
            || self.problem.objective().execution_constraints().requested();
        let coverage_only_needs_witness = !solution_coverage_required
            && self.problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountUnique
            && target.single_pattern_witness_is_exact()
            && (self.buildup_workspace.standard_bag_coverage_complete()
                || self
                    .covered_patterns
                    .is_superset(target.possible_patterns.as_ref())
                    .expect("candidate pattern group belongs to the session universe"));
        let result = match verify_candidate(
            &self.problem,
            &self.catalog,
            &candidate,
            target,
            &mut self.buildup_workspace,
            &mut self.coverage_evaluator,
            coverage_only_needs_witness,
            self.representative_path.is_empty(),
            profile_scale,
            control,
        ) {
            Ok(result) => result,
            Err(WasmExactSearchError::Cancelled) => {
                return Ok(Some(ExactSearchAdvance::Cancelled));
            }
            Err(error) => return Err(error),
        };
        let reduction_span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmCandidateResultReduce,
            profile_scale,
        );
        self.peak_build_nodes = self.peak_build_nodes.max(result.graph_nodes);
        self.total_build_nodes = self.total_build_nodes.saturating_add(result.graph_nodes);
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(result.coverage_product_words);
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(result.coverage_product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(result.coverage_product_edge_checks);
        self.realization_feasibility_states = self
            .realization_feasibility_states
            .saturating_add(result.feasibility_states);
        self.realization_feasibility_rejected_candidates +=
            usize::from(result.feasibility_rejected);
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(result.reachability_states);
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(result.reachability_states);
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
            self.catalog.retained_bytes()
                + self.geometry.retained_bytes()
                + result.retained_bytes
                + self.coverage_evaluator.retained_bytes()
                + self.buildup_workspace.retained_bytes()
                + self.buildable_identities.capacity()
                    * core::mem::size_of::<TilingIdentityEntry>()
                + self.solution_coverage.as_ref().map_or(0, |coverage| {
                    coverage.capacity()
                        * (core::mem::size_of::<StandardBoard64TilingIdentity>()
                            + core::mem::size_of::<PatternBitSet>())
                })
                + self.solution_coverage_bytes,
        );
        if self.memory_budget_exceeded() {
            self.mark_truncated("memory_budget_exceeded");
            return self.complete().map(Some);
        }
        let mut solution_coverage = None;
        if let Some(candidate_coverage) = result.covered_patterns.as_ref() {
            self.coverage_row_count += 1;
            self.pattern_verified_execution_count += candidate_coverage.count_ones() as usize;
            self.covered_patterns
                .union_with(candidate_coverage)
                .expect("all rows use the materialized source universe");
            if solution_coverage_required {
                solution_coverage = Some(candidate_coverage.clone());
            }
        }
        if let Some(root) = result.symbolic_coverage_root {
            if solution_coverage_required {
                let materialized = self.buildup_workspace.materialize_standard_bag_root(root)?;
                if let Some(solution_coverage) = solution_coverage.as_mut() {
                    solution_coverage.union_with(&materialized).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_solution_coverage_universe_mismatch",
                        )
                    })?;
                } else {
                    solution_coverage = Some(materialized);
                }
            }
            self.buildup_workspace.merge_standard_bag_coverage(root)?;
            self.coverage_row_count = self.coverage_row_count.saturating_add(1);
            self.pattern_verified_execution_count = self
                .pattern_verified_execution_count
                .saturating_add(result.symbolic_covered_pattern_count);
        }
        if result.buildable {
            if let Some(solution_coverage) = solution_coverage {
                self.merge_solution_coverage(candidate.identity, &solution_coverage)?;
            }
            let identity = TilingIdentityEntry::new(candidate.identity);
            if !self.buildable_identities.contains(&identity)
                && self.buildable_identities.try_reserve(1).is_err()
            {
                self.mark_truncated("solution_identity_storage_unavailable");
                return self.complete().map(Some);
            }
            self.buildable_identities.insert(identity);
            let next = self
                .build_variant_count
                .checked_add(result.build_variant_count);
            self.build_variant_count = next.unwrap_or(u128::MAX);
            self.count_complete &= next.is_some() && result.count_complete;
            if self
                .representative_rank
                .is_none_or(|rank| candidate_ordinal < rank)
            {
                self.representative_path = result.representative_path;
                self.representative_rank = Some(candidate_ordinal);
                self.representative_identity = Some(candidate.identity);
                self.representative_candidate_id = Some(candidate.identity.bucket_hash());
                self.representative_pattern_id = result.witness_pattern_id.or_else(|| {
                    result
                        .covered_patterns
                        .as_ref()
                        .and_then(PatternBitSet::first_pattern)
                        .map(|id| id.index() as u32)
                });
            }
        }
        reduction_span.finish(1);
        Ok(None)
    }

    fn merge_solution_coverage(
        &mut self,
        identity: StandardBoard64TilingIdentity,
        coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let map = self
            .solution_coverage
            .as_mut()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_solution_coverage_not_requested",
            ))?;
        if !map.contains_key(&identity) {
            map.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_solution_coverage_storage_unavailable")
            })?;
        }
        let entry = map
            .entry(identity)
            .or_insert_with(|| PatternBitSet::new(coverage.pattern_count()));
        let before = entry.retained_bytes();
        entry.union_with(coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_solution_coverage_universe_mismatch")
        })?;
        self.solution_coverage_bytes = self
            .solution_coverage_bytes
            .saturating_add(entry.retained_bytes().saturating_sub(before));
        Ok(())
    }

    pub(super) fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let pattern_count = result.usize_field("coverage_pattern_count").ok_or(
            WasmExactSearchError::InvalidProblem("wasm_distributed_result_pattern_count_missing"),
        )?;
        let coverage =
            PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_distributed_result_coverage_invalid")
                })?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_distributed_coverage_universe_mismatch")
        })?;

        for identity in result.normalized_solution_identities() {
            let identity = TilingIdentityEntry::new(*identity);
            if !self.buildable_identities.contains(&identity) {
                self.buildable_identities.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_distributed_solution_storage_unavailable",
                    )
                })?;
                self.buildable_identities.insert(identity);
            }
        }
        for solution_coverage in result.solution_coverages() {
            self.merge_solution_coverage(
                solution_coverage.identity(),
                solution_coverage.covered_patterns(),
            )?;
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
        self.coverage_row_count = self
            .coverage_row_count
            .saturating_add(result.usize_field("coverage_row_count").unwrap_or(0));
        self.pattern_verified_execution_count =
            self.pattern_verified_execution_count.saturating_add(
                result
                    .usize_field("pattern_verified_execution_count")
                    .unwrap_or(0),
            );
        let next_variants = result
            .field("build_variant_count")
            .and_then(|value| value.parse::<u128>().ok())
            .and_then(|value| self.build_variant_count.checked_add(value));
        self.build_variant_count = next_variants.unwrap_or(u128::MAX);
        self.count_complete &=
            next_variants.is_some() && result.bool_field("count_complete").unwrap_or(false);
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &= result
                .bool_field("execution_constraint_materialized")
                .unwrap_or(false);
        }

        self.peak_build_nodes = self
            .peak_build_nodes
            .max(result.usize_field("peak_build_order_nodes").unwrap_or(0));
        self.total_build_nodes = self
            .total_build_nodes
            .saturating_add(result.usize_field("total_build_order_nodes").unwrap_or(0));
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(result.usize_field("coverage_product_words").unwrap_or(0));
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(result.usize_field("coverage_product_states").unwrap_or(0));
        self.coverage_product_edge_checks = self.coverage_product_edge_checks.saturating_add(
            result
                .usize_field("coverage_product_edge_checks")
                .unwrap_or(0),
        );
        self.realization_feasibility_states = self.realization_feasibility_states.saturating_add(
            result
                .usize_field("realization_feasibility_states")
                .unwrap_or(0),
        );
        self.realization_feasibility_rejected_candidates = self
            .realization_feasibility_rejected_candidates
            .saturating_add(
                result
                    .usize_field("realization_feasibility_rejected_candidates")
                    .unwrap_or(0),
            );
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(result.usize_field("peak_reachability_states").unwrap_or(0));
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(result.usize_field("total_reachability_states").unwrap_or(0));
        self.parallel_worker_retained_bytes = self
            .parallel_worker_retained_bytes
            .saturating_add(result.usize_field("resource_peak_cpu_bytes").unwrap_or(0));
        self.parallel_piece_language_cache_hits =
            self.parallel_piece_language_cache_hits.saturating_add(
                result
                    .usize_field("piece_language_coverage_cache_hits")
                    .unwrap_or(0),
            );
        self.parallel_piece_language_cache_misses =
            self.parallel_piece_language_cache_misses.saturating_add(
                result
                    .usize_field("piece_language_coverage_cache_misses")
                    .unwrap_or(0),
            );
        self.parallel_standard_bag_cache_hits =
            self.parallel_standard_bag_cache_hits.saturating_add(
                result
                    .usize_field("standard_bag_symbolic_cache_hits")
                    .unwrap_or(0),
            );
        self.parallel_standard_bag_cache_misses =
            self.parallel_standard_bag_cache_misses.saturating_add(
                result
                    .usize_field("standard_bag_symbolic_cache_misses")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.lock_queries = self
            .parallel_reachability_metrics
            .lock_queries
            .saturating_add(result.usize_field("reachability_lock_queries").unwrap_or(0));
        self.parallel_reachability_metrics.harddrop_queries = self
            .parallel_reachability_metrics
            .harddrop_queries
            .saturating_add(
                result
                    .usize_field("reachability_harddrop_queries")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.harddrop_hits = self
            .parallel_reachability_metrics
            .harddrop_hits
            .saturating_add(
                result
                    .usize_field("reachability_harddrop_hits")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.cache_reachable_hits = self
            .parallel_reachability_metrics
            .cache_reachable_hits
            .saturating_add(
                result
                    .usize_field("reachability_cache_reachable_hits")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.cache_unreachable_hits = self
            .parallel_reachability_metrics
            .cache_unreachable_hits
            .saturating_add(
                result
                    .usize_field("reachability_cache_unreachable_hits")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.cache_key_misses = self
            .parallel_reachability_metrics
            .cache_key_misses
            .saturating_add(
                result
                    .usize_field("reachability_cache_key_misses")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.partial_searches = self
            .parallel_reachability_metrics
            .partial_searches
            .saturating_add(
                result
                    .usize_field("reachability_partial_searches")
                    .unwrap_or(0),
            );
        self.parallel_reachability_metrics.exhaustive_searches = self
            .parallel_reachability_metrics
            .exhaustive_searches
            .saturating_add(
                result
                    .usize_field("reachability_exhaustive_searches")
                    .unwrap_or(0),
            );

        if let Some(rank) = result
            .field("representative_candidate_ordinal")
            .and_then(|value| value.parse::<u64>().ok())
        {
            if self
                .representative_rank
                .is_none_or(|current| rank < current)
            {
                self.representative_rank = Some(rank);
                self.representative_identity = result.representative_solution_identity();
                self.representative_candidate_id = result
                    .field("representative_candidate_id")
                    .and_then(|value| value.parse::<u64>().ok());
                self.representative_pattern_id = result
                    .field("representative_pattern_id")
                    .and_then(|value| value.parse::<u32>().ok());
                self.representative_path = result.path_steps().to_vec();
            }
        }
        if result.bool_field("resource_truncated").unwrap_or(true) {
            self.mark_truncated("distributed_worker_incomplete");
        }
        Ok(())
    }

    pub(super) fn complete_distributed_geometry(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        self.packing_candidate_count = summary.candidate_count;
        self.packing_candidate_digest = summary.candidate_digest;
        self.workers_used = workers_used.max(1);
        if self.parallel_minimum_worker_candidates == usize::MAX {
            self.parallel_minimum_worker_candidates = 0;
        }
        self.cpu_warmup_performed = self.cpu_warmup_requested;
        match &summary.backend_execution {
            WasmDistributedBackendExecution::Cpu => {}
            WasmDistributedBackendExecution::WebGpu {
                adapter_index,
                adapter_name,
                adapter_type,
                adapter_backend,
                peak_gpu_bytes,
                shader_hash,
                shader_version,
                warmup_performed,
                session_reused,
            } => self.mark_webgpu_execution(
                *adapter_index,
                adapter_name.clone(),
                adapter_type,
                adapter_backend.clone(),
                *peak_gpu_bytes,
                shader_hash.clone(),
                shader_version,
                *warmup_performed,
                *session_reused,
            ),
            WasmDistributedBackendExecution::CpuFallback {
                reason,
                failure_class,
                failure_stage,
                discarded_partial_gpu_result,
                original_gpu_result_incomplete,
            } => self.mark_cpu_fallback(
                reason,
                failure_class,
                failure_stage,
                *discarded_partial_gpu_result,
                *original_gpu_result_incomplete,
            ),
        }
        self.geometry.finish_external_summary(summary);
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
            self.catalog
                .retained_bytes()
                .saturating_add(self.geometry.retained_bytes())
                .saturating_add(self.parallel_worker_retained_bytes),
        );
        if let Some(reason) = summary.truncated_reason {
            self.mark_truncated(reason);
        }
        self.complete()
    }

    pub fn complete_external_geometry(
        &mut self,
        expanded_nodes: usize,
        peak_frontier: usize,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        self.geometry
            .finish_external(self.packing_candidate_count, expanded_nodes, peak_frontier);
        self.complete()
    }

    pub(super) fn complete_distributed_worker(
        &mut self,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        self.geometry
            .finish_external(self.packing_candidate_count, 0, 0);
        self.complete_internal(false)
    }

    fn complete(&mut self) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        self.complete_internal(true)
    }

    fn complete_internal(
        &mut self,
        include_normalized_keys: bool,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        let coverage_span = SearchStageSpan::begin(ExecutorSearchStage::WasmFinalCoverage);
        if let Some(symbolic_coverage) =
            self.buildup_workspace.materialize_standard_bag_coverage()?
        {
            self.coverage_product_words = self
                .coverage_product_words
                .saturating_add(symbolic_coverage.word_count());
            self.covered_patterns
                .union_with(&symbolic_coverage)
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_coverage_universe_mismatch",
                    )
                })?;
        }
        coverage_span.finish(u64::from(self.covered_patterns.count_ones()));
        self.finished = true;
        let result_span = SearchStageSpan::begin(ExecutorSearchStage::WasmResultCanonicalize);
        let scoring_requested = self.problem.objective().score().requested();
        let execution_constraints = self.problem.objective().execution_constraints();
        let execution_evidence_requested = scoring_requested || execution_constraints.requested();
        let scoring_batch = if execution_evidence_requested {
            Some(self.prepare_exact_scoring_execution_batch()?)
        } else {
            None
        };
        let result = self.build_result(include_normalized_keys, scoring_batch);
        result_span.finish(self.buildable_identities.len() as u64);
        Ok(ExactSearchAdvance::Completed(result))
    }

    fn prepare_exact_scoring_execution_batch(
        &mut self,
    ) -> Result<ExactScoringExecutionBatch, WasmExactSearchError> {
        let mut identities = self
            .buildable_identities
            .iter()
            .map(|entry| entry.identity)
            .collect::<Vec<_>>();
        identities.sort_unstable();
        let mut graphs = Vec::new();
        graphs.try_reserve_exact(identities.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_graph_storage_unavailable")
        })?;
        let mut complete = true;
        for (index, identity) in identities.into_iter().enumerate() {
            let candidate_id = u64::try_from(index + 1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_scoring_candidate_id_overflow")
            })?;
            match exact_scoring_execution_graph(
                &self.problem,
                &self.catalog,
                identity,
                candidate_id,
                &mut self.buildup_workspace,
            )? {
                Some(graph) => graphs.push(graph),
                None => complete = false,
            }
        }
        let board_size = BoardSize::new(
            u16::from(self.catalog.width()),
            u16::from(self.catalog.height()),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_scoring_layout_invalid"))?;
        let layout = Board64Layout::new(board_size)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_scoring_layout_not_board64"))?;
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let patterns = (0..universe.pattern_count())
            .map(|pattern| universe.sequence_at(pattern).into_owned())
            .collect();
        let (kick_table_id, rule_profile_id) = replay_profile_ids(&self.problem);
        Ok(ExactScoringExecutionBatch::new(
            layout,
            self.catalog.initial_board(),
            patterns,
            self.problem.initial_hold().cursor(),
            self.problem.initial_hold().hold_piece(),
            self.problem.supply().hold_enabled(),
            self.problem.supply().projects_unplaced_lookahead(),
            self.problem.supply().projects_standard_bag_lookahead(),
            kick_table_id,
            rule_profile_id,
            graphs,
            complete,
        ))
    }

    fn mark_truncated(&mut self, reason: &'static str) {
        self.truncated_reason.get_or_insert(reason);
        self.count_complete = false;
    }

    fn memory_budget_exceeded(&self) -> bool {
        let Some(max_memory_mib) = self.problem.backend_request().max_memory_mib() else {
            return false;
        };
        let limit = max_memory_mib.saturating_mul(1024 * 1024);
        self.peak_cpu_bytes as u64 > limit
    }

    fn build_result(
        &self,
        include_normalized_keys: bool,
        scoring_batch: Option<ExactScoringExecutionBatch>,
    ) -> CoreExecutionResult {
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("session construction requires materialized supply");
        let source_sequence_length = if universe.pattern_count() == 0 {
            0
        } else {
            universe.sequence_at(0).len()
        };
        let coverage_probability = universe
            .weights()
            .covered_weight(&self.covered_patterns)
            .expect("coverage and supply use one pattern universe")
            .get();
        let probability_complete = universe.complete() && self.truncated_reason.is_none();
        let count_complete = self.truncated_reason.is_none() && self.count_complete;
        let mut identities = self
            .buildable_identities
            .iter()
            .map(|entry| entry.identity)
            .collect::<Vec<_>>();
        identities.sort_unstable();
        let source_solution_count = identities.len();
        let mut solution_coverages = self
            .solution_coverage
            .as_ref()
            .map(|coverage| {
                let mut entries = coverage
                    .iter()
                    .map(|(identity, bits)| SolutionCoverage::new(*identity, bits.clone()))
                    .collect::<Vec<_>>();
                entries.sort_unstable_by_key(SolutionCoverage::identity);
                entries
            })
            .unwrap_or_default();
        let normalized_solution_coverages = solution_coverages
            .iter()
            .map(|coverage| {
                NormalizedSolutionCoverage::new(
                    NormalizedTilingSolutionKey::from_standard_board64_identity(
                        coverage.identity(),
                    )
                    .as_str(),
                    coverage.covered_patterns().clone(),
                )
            })
            .collect();
        let minimum_cover_requested =
            self.problem.objective().kind() == ObjectiveKind::MinimumCover;
        let minimum_cover_product_reduction = minimum_cover_requested && include_normalized_keys;
        let mut minimum_cover_complete = false;
        let mut minimum_cover_proven = false;
        let mut minimum_cover_reason = if minimum_cover_requested {
            "minimum-cover-not-evaluated"
        } else {
            "not-requested"
        };
        if minimum_cover_product_reduction {
            if !count_complete {
                minimum_cover_reason = self.truncated_reason.unwrap_or("search-incomplete");
            } else if !probability_complete {
                minimum_cover_reason = "pattern-universe-incomplete";
            } else if !covers_all_identities(&identities, &solution_coverages) {
                minimum_cover_reason = "solution-coverage-incomplete";
            } else {
                let rows = identities
                    .iter()
                    .map(|identity| {
                        let index = solution_coverages
                            .binary_search_by_key(identity, SolutionCoverage::identity)
                            .expect("minimum-cover identity coverage was checked above");
                        solution_coverages[index].covered_patterns().clone()
                    })
                    .collect::<Vec<_>>();
                match exact_minimum_cover(&self.covered_patterns, &rows) {
                    Ok(selection) if selection.complete() => {
                        identities = selection
                            .row_indices()
                            .iter()
                            .map(|index| identities[*index])
                            .collect();
                        solution_coverages.retain(|coverage| {
                            identities.binary_search(&coverage.identity()).is_ok()
                        });
                        minimum_cover_complete = true;
                        minimum_cover_proven = true;
                        minimum_cover_reason = "none";
                    }
                    Ok(_) => minimum_cover_reason = "required-pattern-cover-incomplete",
                    Err(_) => minimum_cover_reason = "pattern-universe-mismatch",
                }
            }
        } else if minimum_cover_requested {
            minimum_cover_reason = "deferred-to-coordinator";
        }
        let normalized_hash =
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities,
            );
        let normalized_keys = if include_normalized_keys {
            identities
                .iter()
                .copied()
                .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
                .map(|key| key.as_str().to_owned())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let solution_probabilities_requested =
            self.problem.solution_probability_policy().requested();
        let solution_probability_complete = !solution_probabilities_requested
            || (probability_complete
                && count_complete
                && covers_all_identities(&identities, &solution_coverages));
        let solution_probabilities: Vec<SolutionProbabilityReport> =
            if solution_probabilities_requested {
                probability_reports(
                    &identities,
                    &solution_coverages,
                    universe.weights(),
                    solution_probability_complete,
                )
            } else {
                Vec::new()
            };
        let build_variant_count_exact = self.problem.count_policy()
            == clearra_pc_graph::request::PcCountPolicy::CountAll
            && count_complete;
        let objective = match self.problem.objective().kind() {
            ObjectiveKind::All => "all",
            ObjectiveKind::Unique => "unique",
            ObjectiveKind::MinimumCover => "minimum-cover",
        };
        let score_policy = self.problem.objective().score();
        let execution_constraints = self.problem.objective().execution_constraints();
        let execution_constraint_requested = execution_constraints.requested();
        let scoring_execution_complete = scoring_batch
            .as_ref()
            .is_some_and(ExactScoringExecutionBatch::complete);
        let score_objective_requested = score_policy.requested();
        let non_score_objective_complete = count_complete
            && (!minimum_cover_requested || (minimum_cover_complete && minimum_cover_proven));
        let solution_found = !identities.is_empty();
        let mut reachability_metrics = self.buildup_workspace.reachability_metrics();
        add_reachability_metrics(
            &mut reachability_metrics,
            self.parallel_reachability_metrics,
        );
        let fields = vec![
            field(
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            field("backend_selected", self.backend_selected),
            field("actual_backend", self.backend_selected),
            field("backend_fallback_used", self.backend_fallback_used),
            field("backend_fallback_reason", self.backend_fallback_reason),
            field("fallback_backend", self.fallback_backend.unwrap_or("none")),
            field(
                "gpu_failure_class",
                self.gpu_failure_class.unwrap_or("none"),
            ),
            field(
                "gpu_failure_stage",
                self.gpu_failure_stage.unwrap_or("none"),
            ),
            field(
                "discarded_partial_gpu_result",
                self.discarded_partial_gpu_result,
            ),
            field(
                "gpu_original_result_incomplete",
                self.gpu_original_result_incomplete,
            ),
            field(
                "gpu_device",
                self.problem
                    .backend_policy()
                    .gpu_device()
                    .as_display_string(),
            ),
            field(
                "workers_requested",
                self.problem
                    .backend_policy()
                    .workers_requested()
                    .map_or_else(|| "auto".to_owned(), |workers| workers.to_string()),
            ),
            field("workers_used", self.workers_used),
            field(
                "logical_processor_count",
                self.problem.backend_policy().worker_hardware_limit(),
            ),
            field(
                "all_cpu_threads_requested",
                self.problem.backend_policy().use_all_logical_processors(),
            ),
            field("cpu_parallel_execution", self.workers_used > 1),
            field(
                "cpu_parallel_decision_reason",
                self.parallel_decision_reason,
            ),
            field(
                "cpu_parallel_task_granularity",
                "immutable-family-traversal",
            ),
            field("parallel_active_workers", self.parallel_active_workers),
            field(
                "parallel_minimum_worker_candidates",
                self.parallel_minimum_worker_candidates,
            ),
            field(
                "parallel_maximum_worker_candidates",
                self.parallel_maximum_worker_candidates,
            ),
            field("cpu_warmup_requested", self.cpu_warmup_requested),
            field("cpu_warmup_performed", self.cpu_warmup_performed),
            field("gpu_warmup_requested", self.gpu_warmup_requested),
            field("gpu_warmup_performed", self.gpu_warmup_performed),
            field("gpu_session_reused", self.gpu_session_reused),
            field(
                "gpu_adapter",
                self.gpu_adapter_name.as_deref().unwrap_or("none"),
            ),
            field(
                "gpu_device_selected_index",
                self.gpu_adapter_index
                    .map_or_else(|| "none".to_owned(), |index| index.to_string()),
            ),
            field(
                "gpu_device_selected_name",
                self.gpu_adapter_name.as_deref().unwrap_or("none"),
            ),
            field(
                "gpu_device_selected_type",
                self.gpu_adapter_type.unwrap_or("none"),
            ),
            field(
                "gpu_device_selected_backend",
                self.gpu_adapter_backend.as_deref().unwrap_or("none"),
            ),
            field("gpu_peak_bytes", self.gpu_peak_bytes),
            field(
                "gpu_shader_hash",
                self.gpu_shader_hash.as_deref().unwrap_or("none"),
            ),
            field(
                "gpu_shader_version",
                self.gpu_shader_version.unwrap_or("none"),
            ),
            field("gpu_cpu_duplicate_search", false),
            field("search_traversal", "canonical-skeleton-exact-cover"),
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
            field(
                "geometry_catalog_digest",
                format!("{:016x}", self.catalog.identity_digest()),
            ),
            field("geometry_skeleton_count", self.catalog.skeleton_count()),
            field(
                "concrete_realization_count",
                self.catalog.realization_count(),
            ),
            field(
                "instantiated_realization_count",
                self.catalog.instantiated_realization_count(),
            ),
            field(
                "instantiation_table_connected",
                self.catalog.has_instantiation_table(),
            ),
            field("packing_candidate_is_solution", false),
            field("packing_candidate_count", self.packing_candidate_count),
            field(
                "geometry_candidate_family_count",
                self.geometry
                    .candidate_family_count()
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "overflow-or-incomplete".to_owned()),
            ),
            field(
                "packing_candidate_set_digest",
                format!("{:016x}", self.packing_candidate_digest),
            ),
            field("packing_count_complete", self.truncated_reason.is_none()),
            field(
                "packing_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field("solution_found", solution_found),
            field("unique_solution_count", identities.len()),
            field("normalized_unique_solution_count", identities.len()),
            field("minimum_cover_requested", minimum_cover_requested),
            field("minimum_cover_source_solution_count", source_solution_count),
            field("minimum_cover_selected_solution_count", identities.len()),
            field(
                "minimum_cover_required_pattern_count",
                self.covered_patterns.count_ones(),
            ),
            field("minimum_cover_complete", minimum_cover_complete),
            field("minimum_cover_proven_minimum", minimum_cover_proven),
            field("minimum_cover_incomplete_reason", minimum_cover_reason),
            field(
                "normalized_solution_key_algorithm",
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            ),
            field(
                "normalized_solution_set_hash_algorithm",
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            ),
            field("normalized_solution_set_hash", &normalized_hash),
            field("actual_normalized_solution_set_hash", &normalized_hash),
            field("build_variant_count", self.build_variant_count),
            field("build_variant_count_exact", build_variant_count_exact),
            field(
                "pattern_verified_execution_count",
                self.pattern_verified_execution_count,
            ),
            field("coverage_row_count", self.coverage_row_count),
            field("coverage_pattern_count", universe.pattern_count()),
            field("materialized_pattern_count", universe.pattern_count()),
            field("covered_pattern_count", self.covered_patterns.count_ones()),
            field("coverage_probability", coverage_probability),
            field(
                "materialized_probability_mass",
                universe.materialized_probability_mass().get(),
            ),
            field("renormalized", false),
            field("probability_complete", probability_complete),
            field("count_complete", count_complete),
            field(
                "solution_probabilities_requested",
                solution_probabilities_requested,
            ),
            field("solution_probability_count", solution_probabilities.len()),
            field(
                "solution_probability_complete",
                solution_probability_complete,
            ),
            field(
                "solution_probability_basis",
                if solution_probabilities_requested {
                    "normalized-solution-pattern-bitset-or-union"
                } else {
                    "not-requested"
                },
            ),
            field(
                "solution_probability_incomplete_reason",
                if solution_probabilities_requested && !solution_probability_complete {
                    "pattern-specific-coverage-incomplete"
                } else {
                    "none"
                },
            ),
            field(
                "count_truncated_reason",
                self.truncated_reason
                    .or_else(|| (!universe.complete()).then_some("supply_universe_incomplete"))
                    .unwrap_or("none"),
            ),
            field("searched_nodes", self.geometry.expanded_nodes()),
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
                "geometry_component_compositions",
                self.geometry.component_compositions(),
            ),
            field("peak_frontier_states", self.geometry.peak_frontier()),
            field("peak_cpu_bytes", self.peak_cpu_bytes),
            field(
                "resource_peak_frontier_states",
                self.geometry.peak_frontier(),
            ),
            field("resource_peak_cpu_bytes", self.peak_cpu_bytes),
            field("resource_peak_gpu_bytes", self.gpu_peak_bytes),
            field("peak_build_order_nodes", self.peak_build_nodes),
            field("total_build_order_nodes", self.total_build_nodes),
            field("coverage_product_words", self.coverage_product_words),
            field("coverage_product_states", self.coverage_product_states),
            field(
                "coverage_product_edge_checks",
                self.coverage_product_edge_checks,
            ),
            field(
                "piece_language_coverage_cache_hits",
                self.buildup_workspace
                    .piece_language_coverage_hits()
                    .saturating_add(self.parallel_piece_language_cache_hits),
            ),
            field(
                "piece_language_coverage_cache_misses",
                self.buildup_workspace
                    .piece_language_coverage_misses()
                    .saturating_add(self.parallel_piece_language_cache_misses),
            ),
            field(
                "standard_bag_symbolic_cache_hits",
                self.buildup_workspace
                    .standard_bag_coverage_hits()
                    .saturating_add(self.parallel_standard_bag_cache_hits),
            ),
            field(
                "standard_bag_symbolic_cache_misses",
                self.buildup_workspace
                    .standard_bag_coverage_misses()
                    .saturating_add(self.parallel_standard_bag_cache_misses),
            ),
            field(
                "realization_feasibility_states",
                self.realization_feasibility_states,
            ),
            field(
                "realization_feasibility_rejected_candidates",
                self.realization_feasibility_rejected_candidates,
            ),
            field("peak_reachability_states", self.peak_reachability_states),
            field("total_reachability_states", self.total_reachability_states),
            field(
                "reachability_lock_queries",
                reachability_metrics.lock_queries,
            ),
            field(
                "reachability_harddrop_queries",
                reachability_metrics.harddrop_queries,
            ),
            field(
                "reachability_harddrop_hits",
                reachability_metrics.harddrop_hits,
            ),
            field(
                "reachability_cache_reachable_hits",
                reachability_metrics.cache_reachable_hits,
            ),
            field(
                "reachability_cache_unreachable_hits",
                reachability_metrics.cache_unreachable_hits,
            ),
            field(
                "reachability_cache_key_misses",
                reachability_metrics.cache_key_misses,
            ),
            field(
                "reachability_partial_searches",
                reachability_metrics.partial_searches,
            ),
            field(
                "reachability_exhaustive_searches",
                reachability_metrics.exhaustive_searches,
            ),
            field("resource_truncated", self.truncated_reason.is_some()),
            field(
                "resource_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field("objective", objective),
            field("objective_search_complete", count_complete),
            field(
                "objective_complete",
                non_score_objective_complete
                    && !score_objective_requested
                    && (!execution_constraint_requested
                        || self.distributed_execution_constraint_materialized),
            ),
            field(
                "objective_incomplete_reason",
                if score_objective_requested {
                    "score_matrix_not_materialized"
                } else if execution_constraint_requested
                    && !self.distributed_execution_constraint_materialized
                {
                    "b2b_preservation_not_materialized"
                } else if minimum_cover_requested {
                    minimum_cover_reason
                } else {
                    self.truncated_reason.unwrap_or("none")
                },
            ),
            field("postprocess_scoring_requested", score_policy.requested()),
            field("score_objective_mode", score_policy.mode().as_str()),
            field("score_profile_requested", score_policy.profile().as_str()),
            field(
                "spin_profile_requested",
                score_policy.spin_profile().as_str(),
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
            field("score_initial_b2b", score_policy.initial_b2b()),
            field("postprocess_execution_complete", scoring_execution_complete),
            field(
                "sample_trace_available",
                !self.representative_path.is_empty(),
            ),
            field(
                "retained_trace_count",
                usize::from(!self.representative_path.is_empty()),
            ),
            field("trace_retention_truncated", false),
            field("trace_retention_reason", "none"),
            field("trace_steps", self.representative_path.len()),
            field(
                "representative_candidate_id",
                self.representative_candidate_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
            field(
                "representative_candidate_ordinal",
                self.representative_rank
                    .map(|rank| rank.to_string())
                    .unwrap_or_default(),
            ),
            field(
                "representative_pattern_id",
                self.representative_pattern_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
        ];
        let pattern_weights = if score_policy.requested() || execution_constraint_requested {
            (0..universe.pattern_count())
                .map(|pattern| universe.weight_at(pattern).get().to_string())
                .collect()
        } else {
            Vec::new()
        };
        CoreExecutionResult::new(fields, self.representative_path.clone())
            .with_normalized_solution_keys(normalized_keys)
            .with_normalized_solution_identities(identities)
            .with_representative_solution_identity(self.representative_identity)
            .with_coverage_pattern_words(self.covered_patterns.words().to_vec())
            .with_solution_coverages(solution_coverages)
            .with_normalized_solution_coverages(normalized_solution_coverages)
            .with_solution_probabilities(solution_probabilities)
            .with_postprocess_execution_batch(
                Vec::new(),
                scoring_execution_complete,
                pattern_weights,
            )
            .with_exact_scoring_execution_batch(scoring_batch)
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

fn add_reachability_metrics(total: &mut ReachabilityMetrics, next: ReachabilityMetrics) {
    total.lock_queries = total.lock_queries.saturating_add(next.lock_queries);
    total.harddrop_queries = total.harddrop_queries.saturating_add(next.harddrop_queries);
    total.harddrop_hits = total.harddrop_hits.saturating_add(next.harddrop_hits);
    total.cache_reachable_hits = total
        .cache_reachable_hits
        .saturating_add(next.cache_reachable_hits);
    total.cache_unreachable_hits = total
        .cache_unreachable_hits
        .saturating_add(next.cache_unreachable_hits);
    total.cache_key_misses = total.cache_key_misses.saturating_add(next.cache_key_misses);
    total.partial_searches = total.partial_searches.saturating_add(next.partial_searches);
    total.exhaustive_searches = total
        .exhaustive_searches
        .saturating_add(next.exhaustive_searches);
}
// SRP rationale: this module has one behavior-level change reason: canonical exact-search result accumulation and finalization.
