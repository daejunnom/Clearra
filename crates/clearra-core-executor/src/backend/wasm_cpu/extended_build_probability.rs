use std::collections::{HashMap, HashSet};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_finesse::{
    CostedGeometryEdge, CostedGeometryLanguage, GeometryLanguageNode, GeometryNodeId,
};
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};
use clearra_replay::{SpinCoverageExecutionBatch, SpinCoverageExecutionGraph};
use clearra_supply::pattern_universe::PackingPatternMembershipKind;

use crate::{CoreExecutionResult, CorePathStep, NormalizedSolutionCoverage};

use super::{
    build_probability::{costed_finesse_language, BuildProbabilityAdvance, FinesseSearchMaterial},
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
    finished: bool,
}

impl ExtendedBuildProbabilitySession {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_mode(problem, field, aggregation, false)
    }

    pub fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_mode(problem, field, aggregation, true)
    }

    fn new_mode(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
    ) -> Result<Self, WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
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
        Ok(Self {
            problem: problem.clone(),
            aggregation,
            field,
            catalog,
            geometry,
            build_order_workspace,
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns,
            buildable_tilings: HashSet::new(),
            solution_coverage: problem
                .objective()
                .execution_constraints()
                .requested()
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
            finished: false,
        })
    }

    pub fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let mut session = Self::new(problem, field, aggregation)?;
        if !session.geometry.prepare_external() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_external_geometry_prepare_failed",
            ));
        }
        session.external_geometry = true;
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
            if self.memory_budget_exhausted() {
                self.truncated_reason = Some("memory_budget_exceeded");
                return self.complete();
            }
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
        let candidate_key = (execution_evidence_requested || self.finesse_requested)
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
                return Ok(());
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
                return Ok(());
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

    pub fn advance_distributed_geometry(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_geometry_state_invalid",
            ));
        }
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
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
                let ordinal = self.geometry.candidate_count().saturating_sub(1) as u64;
                let tiling = ExtendedTilingKey::from_candidate(&self.catalog, &candidate);
                self.candidate_digest = mix_digest(self.candidate_digest, tiling.digest());
                Ok(WasmCandidateProducerAdvance::Candidate(
                    WasmCandidatePacket::for_extended_pass(
                        ordinal,
                        pass_index,
                        candidate.row_ids().to_vec(),
                    ),
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
        self.process_candidate(geometry, Some(candidate.ordinal()), control)
    }

    pub fn complete_distributed_worker(
        &mut self,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        if !self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_verifier_state_invalid",
            ));
        }
        self.finished = true;
        Ok(self.build_result())
    }

    pub fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        if self.external_geometry || self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_merger_state_invalid",
            ));
        }
        let pattern_count = result.usize_field("coverage_pattern_count").ok_or(
            WasmExactSearchError::InvalidProblem("wasm_extended_distributed_pattern_count_missing"),
        )?;
        let coverage =
            PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_distributed_coverage_invalid",
                    )
                })?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_extended_distributed_coverage_mismatch")
        })?;
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &= result
                .bool_field("execution_constraint_materialized")
                .unwrap_or(false);
        }

        let worker_solution_count = result.usize_field("unique_solution_count").ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_count_missing",
            ),
        )?;
        if worker_solution_count != result.normalized_solution_keys().len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_distributed_solution_keys_incomplete",
            ));
        }
        self.distributed_solution_keys
            .try_reserve(result.normalized_solution_keys().len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_distributed_solution_storage_unavailable",
                )
            })?;
        self.distributed_solution_keys
            .extend(result.normalized_solution_keys().iter().cloned());
        if self.solution_coverage.is_some() {
            for coverage in result.normalized_solution_coverages() {
                self.merge_solution_coverage(coverage.solution_key(), coverage.covered_patterns())?;
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
                self.representative_path = result.path_steps().to_vec();
            }
        }
        if result.bool_field("resource_truncated").unwrap_or(true) {
            self.truncated_reason = Some("distributed_worker_incomplete");
        }
        Ok(())
    }

    pub fn complete_distributed_geometry(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
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
    }

    fn complete(&mut self) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.finished = true;
        Ok(BuildProbabilityAdvance::Completed(self.build_result()))
    }

    fn build_result(&mut self) -> CoreExecutionResult {
        let tiling_only = self.aggregation.is_tiling_only();
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("extended build probability requires a materialized universe");
        let probability = if tiling_only {
            "not-calculated".to_owned()
        } else {
            universe
                .weights()
                .covered_weight(&self.covered_patterns)
                .expect("coverage belongs to the materialized universe")
                .get()
                .to_string()
        };
        let count_complete = self.supply_projection_complete && self.truncated_reason.is_none();
        let probability_complete = !tiling_only && count_complete;
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
            field(
                "unique_solution_count",
                normalized_keys.len() + usize::from(self.trivial_target),
            ),
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
            field(
                "covered_pattern_count",
                if tiling_only {
                    0
                } else {
                    self.covered_patterns.count_ones()
                },
            ),
            field("coverage_probability", probability),
            field("probability_complete", probability_complete),
            field("count_complete", count_complete),
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
            .with_coverage_pattern_words(self.covered_patterns.words().to_vec())
            .with_spin_coverage_execution_batch(spin_batch);
        if execution_constraints.requested() {
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
        }
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

    fn memory_budget_exhausted(&self) -> bool {
        let Some(max_memory_mib) = self.problem.backend_request().max_memory_mib() else {
            return false;
        };
        let limit = max_memory_mib.saturating_mul(1024 * 1024);
        u64::try_from(self.retained_bytes()).unwrap_or(u64::MAX) > limit
    }

    fn retained_bytes(&self) -> usize {
        self.catalog.retained_bytes()
            + self.geometry.retained_bytes()
            + self.build_order_workspace.retained_bytes()
            + self.coverage_evaluator.retained_bytes()
            + self.covered_patterns.retained_bytes()
            + self
                .buildable_tilings
                .iter()
                .map(ExtendedTilingKey::retained_bytes)
                .sum::<usize>()
            + self.solution_coverage.as_ref().map_or(0, |coverage| {
                coverage
                    .iter()
                    .map(|(key, patterns)| key.capacity() + patterns.retained_bytes())
                    .sum::<usize>()
            })
            + self
                .distributed_solution_keys
                .iter()
                .map(|key| key.capacity())
                .sum::<usize>()
            + self
                .spin_execution_graphs
                .iter()
                .map(SpinCoverageExecutionGraph::retained_bytes)
                .sum::<usize>()
            + self.finesse_languages.capacity()
                * core::mem::size_of::<(String, PreparedFinesseLanguage)>()
            + self
                .finesse_languages
                .iter()
                .map(|(key, language)| {
                    key.capacity()
                        + language.nodes.capacity()
                            * core::mem::size_of::<super::buildup::PreparedFinesseNode>()
                        + language.edges.capacity()
                            * core::mem::size_of::<super::buildup::PreparedFinesseEdge>()
                })
                .sum::<usize>()
            + self.peak_build_scratch_bytes
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
    if parse_extended_board_hex(initial)? != catalog.initial_board() {
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
            let piece = PieceKind::from_ascii(characters.next().ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_missing",
                ),
            )?)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_solution_key_piece_invalid",
                )
            })?;
            if characters.next().is_some() {
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

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

// SRP rationale: this module has one behavior-level change reason: exact extended-board build-probability evaluation.
