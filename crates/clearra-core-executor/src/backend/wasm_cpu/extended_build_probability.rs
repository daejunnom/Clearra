use std::collections::HashSet;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};

use crate::{CoreExecutionResult, CorePathStep};

use super::{
    build_probability::BuildProbabilityAdvance,
    buildup::representative_pattern_path,
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedBackendExecution,
        WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    extended_board::{compact_logical_board, words_hex},
    extended_buildup::{
        build_extended_order_graph, ExtendedBuildOrderResult, ExtendedBuildOrderWorkspace,
        ExtendedTilingKey,
    },
    extended_geometry::{ExtendedGeometryAdvance, ExtendedGeometrySearch},
    extended_inverse_catalog::ExtendedInverseCatalog,
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
    witnessed_pattern_executions: u128,
    representative_path: Vec<CorePathStep>,
    representative_pattern_id: Option<u32>,
    representative_rank: Option<u64>,
    truncated_reason: Option<&'static str>,
    trivial_target: bool,
    external_geometry: bool,
    workers_used: usize,
    parallel_active_workers: usize,
    parallel_minimum_worker_candidates: usize,
    parallel_maximum_worker_candidates: usize,
    distributed_worker_memory_bytes: usize,
    finished: bool,
}

impl ExtendedBuildProbabilitySession {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        if field.is_compact() || !(7..=24).contains(&field.height()) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_height_invalid",
            ));
        }
        if aggregation.requests_spin_coverage() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_probability_spin_not_supported",
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
        let family = universe.packing_multiset_family(
            target_piece_count,
            problem.initial_hold(),
            problem.supply().hold_enabled() && !problem.supply().projects_unplaced_lookahead(),
        );
        if target_piece_count != 0 && family.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_supply_has_no_reachable_piece_multiset",
            ));
        }
        let catalog = ExtendedInverseCatalog::compile(field)?;
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
            witnessed_pattern_executions: 0,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            truncated_reason: None,
            trivial_target: target_piece_count == 0,
            external_geometry: false,
            workers_used: 1,
            parallel_active_workers: 0,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            distributed_worker_memory_bytes: 0,
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
        let node_limit = self.remaining_node_budget();
        if self.problem.backend_request().max_nodes() != 0 && node_limit == 0 {
            self.truncated_reason = Some("node_budget_exceeded");
            return Ok(());
        }
        match build_extended_order_graph(
            &self.catalog,
            &candidate,
            &mut self.build_order_workspace,
            node_limit,
            control,
        )? {
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
                let product = self.coverage_evaluator.evaluate(
                    &graph,
                    candidate.pattern_index.as_ref(),
                    self.problem.initial_hold(),
                    self.problem.supply().hold_enabled(),
                    self.problem.supply().projects_unplaced_lookahead(),
                    false,
                    false,
                    control,
                )?;
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
                self.witnessed_pattern_executions = self
                    .witnessed_pattern_executions
                    .saturating_add(u128::from(product.coverage_bits.count_ones()));
                let rank = external_ordinal.unwrap_or(self.processed_candidate_count as u64 - 1);
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
                    let path =
                        representative_pattern_path(&self.problem, &graph, sequence.as_ref());
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
                self.retain_buildable_tiling(tiling)?;
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
        self.witnessed_pattern_executions = self.witnessed_pattern_executions.saturating_add(
            result
                .field("witnessed_pattern_execution_count")
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

    fn complete(&mut self) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.finished = true;
        Ok(BuildProbabilityAdvance::Completed(self.build_result()))
    }

    fn build_result(&self) -> CoreExecutionResult {
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("extended build probability requires a materialized universe");
        let probability = universe
            .weights()
            .covered_weight(&self.covered_patterns)
            .expect("coverage belongs to the materialized universe")
            .get();
        let complete = universe.complete() && self.truncated_reason.is_none();
        let source_sequence_length = universe.sequence_at(0).len();
        let mut normalized_keys = self
            .buildable_tilings
            .iter()
            .map(|tiling| tiling.canonical_key(self.catalog.initial_board(), self.field.height()))
            .collect::<Vec<_>>();
        normalized_keys.extend(self.distributed_solution_keys.iter().cloned());
        normalized_keys.sort_unstable();
        normalized_keys.dedup();
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
        let fields = vec![
            field(
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            field("backend_selected", "wasm-cpu-build-probability-extended"),
            field("actual_backend", "wasm-cpu-build-probability-extended"),
            field("backend_fallback_used", false),
            field("backend_fallback_reason", "none"),
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
            field(
                "witnessed_pattern_execution_count",
                self.witnessed_pattern_executions,
            ),
            field("build_variant_count_exact", false),
            field("materialized_pattern_count", universe.pattern_count()),
            field("coverage_pattern_count", universe.pattern_count()),
            field("covered_pattern_count", self.covered_patterns.count_ones()),
            field("coverage_probability", probability),
            field("probability_complete", complete),
            field("count_complete", complete),
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
            field("objective_search_complete", complete),
            field("objective_complete", complete),
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
        CoreExecutionResult::new(fields, self.representative_path.clone())
            .with_normalized_solution_keys(normalized_keys)
            .with_coverage_pattern_words(self.covered_patterns.words().to_vec())
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
            + self
                .distributed_solution_keys
                .iter()
                .map(|key| key.capacity())
                .sum::<usize>()
            + self.peak_build_scratch_bytes
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}
