use std::collections::{HashMap, HashSet};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    reducer::pattern_coverage_aggregation::{
        PatternCoverageAggregation, PatternCoverageCompleteness,
    },
};
use clearra_finesse::{
    CostedGeometryEdge, CostedGeometryLanguage, GeometryLanguageNode, GeometryNodeId,
};
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};
use clearra_replay::{SpinCoverageExecutionBatch, SpinCoverageExecutionGraph};
use clearra_supply::pattern_universe::{
    PackingMultisetFamily, PackingPatternMembershipKind, PieceMultisetKey,
};

use crate::{
    resource::ExecutionMemoryBound, CoreExecutionResult, CorePathStep, NormalizedSolutionCoverage,
};

use super::{
    build_probability::{
        build_pattern_coverage_aggregation, costed_finesse_language, exact_bool_field,
        exact_solution_probabilities_requested, exact_usize_field,
        solution_coverage_union_matches_global, validate_distributed_coverage_aggregation_surface,
        validate_distributed_coverage_authority, validate_worker_partial_probability_surface,
        BuildProbabilityAdvance, FinesseSearchMaterial,
    },
    buildup::{representative_pattern_path, PreparedFinesseLanguage},
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedBackendExecution,
        WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    extended_board::{compact_logical_board, words_hex},
    extended_buildup::{
        build_extended_order_graph, build_extended_order_graph_with_finesse,
        ExtendedBuildOrderResult, ExtendedBuildOrderWorkspace, ExtendedTilingKey,
    },
    extended_geometry::{ExtendedGeometryAdvance, ExtendedGeometrySearch},
    extended_inverse_catalog::ExtendedInverseCatalog,
    kick_profiles::replay_profile_ids,
    mix_digest, WasmExactSearchError,
};

pub(super) struct ExtendedBuildProbabilitySession {
    problem: SearchProblem,
    aggregation: BuildProbabilityAggregation,
    field: BuildProbabilityField,
    catalog: ExtendedInverseCatalog,
    geometry: ExtendedGeometrySearch,
    build_order_workspace: ExtendedBuildOrderWorkspace,
    coverage_evaluator: CoverageProductEvaluator,
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
    distributed_execution_constraint_materialized: bool,
    finesse_requested: bool,
    finesse_languages: Vec<(String, PreparedFinesseLanguage)>,
    memory_bound: ExecutionMemoryBound,
    coexisting_retained_bytes: u128,
    finished: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExtendedDistributedPartial {
    count_complete: bool,
    probability_complete: bool,
    resource_truncated: bool,
    coverage_source_row_count: usize,
}

// Browser workers call the distributed entrypoints; native builds retain the same
// session surface so compact and extended execution stay contract-compatible.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl ExtendedBuildProbabilitySession {
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
        Self::new_mode(problem, field, aggregation, false, memory_bound, 0)
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
        Self::new_mode(problem, field, aggregation, true, memory_bound, 0)
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
        )
    }

    fn new_mode(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
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
        let family = universe.packing_multiset_family_for_execution(
            target_piece_count,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            super::packing_hold_projection(problem),
        );
        if target_piece_count != 0 && family.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_supply_has_no_reachable_piece_multiset",
            ));
        }
        let catalog = ExtendedInverseCatalog::compile(field)?;
        let supply_projection_complete = universe.complete()
            || family.membership_kind() == PackingPatternMembershipKind::ExactSymbolicStandardBag;
        let geometry = ExtendedGeometrySearch::new(universe, &family, &catalog)?;
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
            problem: problem.clone(),
            aggregation,
            field,
            catalog,
            geometry,
            build_order_workspace,
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns,
            buildable_tilings: HashSet::new(),
            solution_coverage: (problem.solution_probability_policy().requested()
                || problem.objective().execution_constraints().requested())
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
            distributed_execution_constraint_materialized: false,
            finesse_requested,
            finesse_languages: Vec::new(),
            memory_bound,
            coexisting_retained_bytes,
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
            return self.complete();
        }

        let mut work = 0usize;
        while work < work_budget.max(1) {
            if control.is_cancelled() {
                return Ok(BuildProbabilityAdvance::Cancelled);
            }
            if self.node_budget_exhausted() {
                self.truncated_reason = Some("node_budget_exceeded");
                return self.complete();
            }
            self.ensure_memory_bound(0)?;
            let max_candidates = self.problem.backend_request().max_candidates();
            if max_candidates != 0 && self.geometry.candidate_count() >= max_candidates {
                self.truncated_reason = Some("candidate_budget_exceeded");
                return self.complete();
            }
            match self.geometry.advance(&self.catalog) {
                ExtendedGeometryAdvance::Pending => work += 1,
                ExtendedGeometryAdvance::Candidate(candidate) => {
                    let ordinal = self.geometry.candidate_count().saturating_sub(1) as u64;
                    self.process_candidate(candidate, Some(ordinal), control)?;
                    work += 1;
                    if self.truncated_reason.is_some() {
                        return self.complete();
                    }
                }
                ExtendedGeometryAdvance::Complete => return self.complete(),
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    self.truncated_reason = Some(reason);
                    return self.complete();
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
        self.processed_candidate_count = self.processed_candidate_count.saturating_add(1);
        let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
        let tiling_digest = tiling.digest();
        self.candidate_digest = mix_digest(self.candidate_digest, tiling_digest);
        if self.aggregation.is_tiling_only() {
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
            || self.problem.objective().execution_constraints().requested();
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
                let product = if let Some(language) = finesse_language.as_ref() {
                    self.coverage_evaluator.evaluate_with_finesse(
                        &graph,
                        candidate.pattern_index.as_ref(),
                        self.problem.initial_hold(),
                        self.problem.supply().hold_enabled(),
                        self.problem.supply().projects_unplaced_lookahead(),
                        self.problem.supply().projects_standard_bag_lookahead(),
                        false,
                        false,
                        &language.nodes,
                        self.problem.spawn_profile(),
                        control,
                    )?
                } else {
                    self.coverage_evaluator.evaluate(
                        &graph,
                        candidate.pattern_index.as_ref(),
                        self.problem.initial_hold(),
                        self.problem.supply().hold_enabled(),
                        self.problem.supply().projects_unplaced_lookahead(),
                        self.problem.supply().projects_standard_bag_lookahead(),
                        false,
                        false,
                        control,
                    )?
                };
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
                if let Some(spin_graph) = spin_graph {
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

    fn retain_buildable_tiling(
        &mut self,
        tiling: ExtendedTilingKey,
    ) -> Result<(), WasmExactSearchError> {
        if self.buildable_tilings.contains(&tiling) {
            return Ok(());
        }
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
    #[cfg(any(test, not(target_arch = "wasm32")))]
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
        match self.geometry.advance(&self.catalog) {
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
        let rest = key
            .strip_prefix("ctk2|height=")
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_header_invalid",
            ))?;
        let (height, rest) =
            rest.split_once("|initial=")
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_header_invalid",
                ))?;
        if !canonical_u8_text_matches(height, self.field.height()) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_header_invalid",
            ));
        }
        let (initial, placements) =
            rest.split_once("|placements=")
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_sections_invalid",
                ))?;
        if !is_canonical_extended_board_hex(initial)
            || parse_extended_board_hex(initial)? != self.catalog.initial_board()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_initial_board_mismatch",
            ));
        }

        let mut covered = super::extended_board::ExtendedBoard::EMPTY;
        let mut previous = None::<(PieceKind, super::extended_board::ExtendedBoard)>;
        let mut piece_counts = [0_u8; 7];
        let mut piece_count = 0_usize;
        for placement in placements.split(',').filter(|_| !placements.is_empty()) {
            if placement.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_placement_invalid",
                ));
            }
            let (piece, cells) =
                placement
                    .split_once(':')
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_finesse_solution_key_placement_invalid",
                    ))?;
            let mut characters = piece.chars();
            let piece_character = characters
                .next()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_missing",
                ))?;
            let piece = PieceKind::from_ascii(piece_character).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_invalid",
                )
            })?;
            if piece_character != piece.as_ascii()
                || characters.next().is_some()
                || !is_canonical_extended_board_hex(cells)
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_invalid",
                ));
            }
            let cells = parse_extended_board_hex(cells)?;
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
            if previous.is_some_and(|previous| previous >= (piece, cells)) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_key_reconstruction_mismatch",
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
            previous = Some((piece, cells));
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
        if placements.is_empty() != (piece_count == 0)
            || piece_count != self.field.target_piece_count()
        {
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
        self.ensure_memory_bound(0)?;
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_distributed_annotation_state_invalid",
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
        let mut reconstruction = ExtendedGeometrySearch::new(universe, &family, &self.catalog)?;
        if !reconstruction.prepare_external() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_reconstruction_targets_unavailable",
            ));
        }
        let mut solution_keys = self
            .distributed_solution_keys
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        solution_keys.sort_unstable();
        self.reset_distributed_finesse_aggregation();
        for (ordinal, solution_key) in solution_keys.into_iter().enumerate() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
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
                || self.problem.objective().execution_constraints().requested())
            .then(|| (tiling.digest(), solution_key.clone()));
            let build = build_extended_order_graph_with_finesse(
                &self.problem,
                &self.catalog,
                &candidate,
                &mut self.build_order_workspace,
                usize::MAX,
                spin_candidate,
                self.aggregation.requests_spin_coverage(),
                control,
            )?;
            self.apply_build_order_result(
                &candidate,
                tiling,
                Some(solution_key),
                Some(ordinal as u64),
                build,
                control,
            )?;
        }
        Ok(())
    }

    fn reset_distributed_finesse_aggregation(&mut self) {
        self.finesse_requested = true;
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

    fn complete(&mut self) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.ensure_result_materialization_bound()?;
        self.finished = true;
        let result = self.build_result()?;
        self.ensure_materialized_result_bound(&result)?;
        Ok(BuildProbabilityAdvance::Completed(result))
    }

    fn build_result(&mut self) -> Result<CoreExecutionResult, WasmExactSearchError> {
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
        let execution_evidence_requested =
            self.aggregation.requests_spin_coverage() || execution_constraints.requested();
        let spin_batch = if execution_evidence_requested
            && !(execution_constraints.requested()
                && self.distributed_execution_constraint_materialized)
        {
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
                if self.workers_used > 1 {
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
            field(
                "cpu_warmup_performed",
                self.problem.backend_policy().cpu_warmup(),
            ),
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
        Ok(if execution_constraints.requested() {
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
        })
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

    fn remaining_node_budget(&self) -> usize {
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
        super::build_probability::checked_build_probability_problem_nested_retained_bytes(
            &self.problem,
        )?
        .checked_add(self.checked_non_problem_retained_bytes()?)
    }

    fn checked_non_problem_retained_bytes(&self) -> Option<u128> {
        let mut total = 0_u128;
        for bytes in [
            self.catalog.retained_bytes(),
            self.geometry.retained_bytes(),
            self.build_order_workspace.retained_bytes(),
            self.coverage_evaluator.retained_bytes(),
            self.covered_patterns.retained_bytes(),
            self.peak_build_scratch_bytes,
        ] {
            total = total.checked_add(bytes as u128)?;
        }
        total = total.checked_add(
            (self.buildable_tilings.capacity() as u128)
                .checked_mul(core::mem::size_of::<ExtendedTilingKey>() as u128)?,
        )?;
        for key in &self.buildable_tilings {
            total = total.checked_add(
                (key.retained_bytes() as u128)
                    .checked_sub(core::mem::size_of::<ExtendedTilingKey>() as u128)?,
            )?;
        }
        if let Some(coverage) = self.solution_coverage.as_ref() {
            total = total.checked_add((coverage.capacity() as u128).checked_mul(
                (core::mem::size_of::<String>() + core::mem::size_of::<PatternBitSet>()) as u128,
            )?)?;
            for (key, patterns) in coverage {
                total = total
                    .checked_add(key.capacity() as u128)?
                    .checked_add(patterns.retained_bytes() as u128)?;
            }
        }
        total = total.checked_add(
            (self.distributed_solution_keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
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
        if self.trivial_target && self.solution_coverage.is_some() {
            future = future.checked_add(
                PatternBitSet::checked_all_projection(self.covered_patterns.pattern_count())?
                    .constructor_peak_bytes,
            )?;
        }
        if self.problem.objective().execution_constraints().requested() {
            future = future.checked_add(
                (self.covered_patterns.pattern_count() as u128).checked_mul(
                    (core::mem::size_of::<String>() as u128).checked_add(
                        super::build_probability::MAX_CANONICAL_PROBABILITY_TEXT_BYTES,
                    )?,
                )?,
            )?;
        }

        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested();
        if execution_evidence_requested
            && !(self.problem.objective().execution_constraints().requested()
                && self.distributed_execution_constraint_materialized)
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
        Some(future)
    }

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
        self.memory_bound
            .ensure(observed, future)
            .map_err(WasmExactSearchError::resource_admission)
    }
}

fn extended_row_ids_from_canonical_key(
    key: &str,
    expected_height: u8,
    catalog: &ExtendedInverseCatalog,
) -> Result<Vec<u32>, WasmExactSearchError> {
    let prefix = format!("ctk2|height={expected_height}|initial=");
    let rest = key
        .strip_prefix(&prefix)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_solution_key_header_invalid",
        ))?;
    let (initial, placements) =
        rest.split_once("|placements=")
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_solution_key_sections_invalid",
            ))?;
    if !is_canonical_extended_board_hex(initial)
        || parse_extended_board_hex(initial)? != catalog.initial_board()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_solution_key_initial_board_mismatch",
        ));
    }
    let mut row_ids = Vec::new();
    if !placements.is_empty() {
        row_ids
            .try_reserve_exact(placements.split(',').count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_storage_unavailable",
                )
            })?;
        for placement in placements.split(',') {
            let (piece, cells) =
                placement
                    .split_once(':')
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_finesse_solution_key_placement_invalid",
                    ))?;
            let mut characters = piece.chars();
            let piece_character = characters
                .next()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_missing",
                ))?;
            let piece = PieceKind::from_ascii(piece_character).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_invalid",
                )
            })?;
            if piece_character != piece.as_ascii()
                || characters.next().is_some()
                || !is_canonical_extended_board_hex(cells)
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_invalid",
                ));
            }
            let cells = parse_extended_board_hex(cells)?;
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

fn parse_extended_board_hex(
    value: &str,
) -> Result<super::extended_board::ExtendedBoard, WasmExactSearchError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_solution_key_mask_invalid",
        ));
    }
    let mut words = [0_u64; 4];
    for (chunk, word_index) in [3_usize, 2, 1, 0].into_iter().enumerate() {
        let start = chunk * 16;
        words[word_index] = u64::from_str_radix(&value[start..start + 16], 16).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_finesse_solution_key_mask_invalid")
        })?;
    }
    Ok(super::extended_board::ExtendedBoard::from_words(words))
}

fn canonical_u8_text_matches(value: &str, expected: u8) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u8>() == Ok(expected)
}

fn is_canonical_extended_board_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

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
        assert_eq!(
            session.checked_retained_bytes(),
            session
                .checked_non_problem_retained_bytes()
                .and_then(|bytes| bytes.checked_add(owned_nested))
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
