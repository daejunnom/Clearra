use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use super::{
    buildup::{verify_candidate, BuildUpWorkspace, CandidateBuildResult, CandidateWitnessMode},
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometryAdvance, GeometryCandidate, GeometrySearch, TargetGroup},
    parallel_coverage::SharedCoverage,
    reachability::ReachabilityMetrics,
    result::retains_buildable_identity_evidence,
    WasmExactSearchError,
};

pub(super) struct ParallelBranchTask {
    pub canonical_index: usize,
    pub priority: usize,
    pub search: GeometrySearch,
}

pub(super) struct ParallelBranchQueue {
    tasks: Mutex<VecDeque<ParallelBranchTask>>,
    aborted: AtomicBool,
}

impl ParallelBranchQueue {
    pub fn new(mut tasks: Vec<ParallelBranchTask>) -> Self {
        tasks.sort_unstable_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.canonical_index.cmp(&right.canonical_index))
        });
        Self {
            tasks: Mutex::new(tasks.into()),
            aborted: AtomicBool::new(false),
        }
    }

    pub fn pop(&self) -> Option<ParallelBranchTask> {
        if self.is_aborted() {
            return None;
        }
        self.tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::Acquire)
    }
}

pub(super) struct BranchSearchOutcome {
    pub canonical_index: usize,
    pub geometry: GeometrySearch,
    pub candidate_count: usize,
    pub candidate_hashes: Vec<u64>,
    pub truncated_reason: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(super) struct RepresentativeCandidate {
    pub branch_index: usize,
    pub local_ordinal: usize,
    pub candidate: GeometryCandidate,
}

impl RepresentativeCandidate {
    fn rank(self) -> (usize, usize) {
        (self.branch_index, self.local_ordinal)
    }
}

#[derive(Default)]
pub(super) struct WorkerAggregate {
    pub buildable_identities: Vec<StandardBoard64TilingIdentity>,
    pub solution_coverage: BTreeMap<StandardBoard64TilingIdentity, PatternBitSet>,
    pub coverage_row_count: usize,
    pub pattern_verified_execution_count: usize,
    pub build_variant_count: u128,
    pub count_complete: bool,
    pub representative: Option<RepresentativeCandidate>,
    pub peak_build_nodes: usize,
    pub total_build_nodes: usize,
    pub coverage_product_words: usize,
    pub coverage_product_states: usize,
    pub coverage_product_edge_checks: usize,
    pub feasibility_states: usize,
    pub feasibility_rejected_candidates: usize,
    pub peak_reachability_states: usize,
    pub total_reachability_states: usize,
    pub worker_retained_bytes: usize,
    pub piece_language_cache_hits: usize,
    pub piece_language_cache_misses: usize,
    pub standard_bag_cache_hits: usize,
    pub standard_bag_cache_misses: usize,
    pub reachability_metrics: ReachabilityMetrics,
}

impl WorkerAggregate {
    fn observe_tiling(
        &mut self,
        branch_index: usize,
        local_ordinal: usize,
        candidate: GeometryCandidate,
    ) -> Result<(), WasmExactSearchError> {
        self.buildable_identities.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_parallel_solution_storage_unavailable")
        })?;
        self.buildable_identities.push(candidate.identity);
        let representative = RepresentativeCandidate {
            branch_index,
            local_ordinal,
            candidate,
        };
        if self
            .representative
            .is_none_or(|current| representative.rank() < current.rank())
        {
            self.representative = Some(representative);
        }
        Ok(())
    }

    // Worker telemetry mirrors the shared progress contract without allocation.
    #[allow(clippy::too_many_arguments)]
    fn observe(
        &mut self,
        branch_index: usize,
        local_ordinal: usize,
        candidate: GeometryCandidate,
        result: CandidateBuildResult,
        solution_coverage: Option<PatternBitSet>,
        retain_solution_set: bool,
        retain_representative: bool,
    ) -> Result<(), WasmExactSearchError> {
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
        self.feasibility_states = self
            .feasibility_states
            .saturating_add(result.feasibility_states);
        self.feasibility_rejected_candidates = self
            .feasibility_rejected_candidates
            .saturating_add(usize::from(result.feasibility_rejected));
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(result.reachability_states);
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(result.reachability_states);
        self.coverage_row_count = self.coverage_row_count.saturating_add(usize::from(
            result.covered_patterns.is_some() || result.symbolic_coverage_root.is_some(),
        ));
        self.pattern_verified_execution_count = self
            .pattern_verified_execution_count
            .saturating_add(
                result
                    .covered_patterns
                    .as_ref()
                    .map_or(0, |coverage| coverage.count_ones() as usize),
            )
            .saturating_add(result.symbolic_covered_pattern_count);
        if !result.buildable {
            return Ok(());
        }

        if let Some(coverage) = solution_coverage {
            let entry = self
                .solution_coverage
                .entry(candidate.identity)
                .or_insert_with(|| PatternBitSet::new(coverage.pattern_count()));
            entry.union_with(&coverage).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_solution_coverage_universe_mismatch",
                )
            })?;
        }

        if retain_solution_set {
            self.buildable_identities.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_parallel_solution_storage_unavailable")
            })?;
            self.buildable_identities.push(candidate.identity);
        }
        let next = self
            .build_variant_count
            .checked_add(result.build_variant_count);
        self.build_variant_count = next.unwrap_or(u128::MAX);
        self.count_complete &= next.is_some() && result.count_complete;
        if retain_representative {
            let representative = RepresentativeCandidate {
                branch_index,
                local_ordinal,
                candidate,
            };
            if self
                .representative
                .is_none_or(|current| representative.rank() < current.rank())
            {
                self.representative = Some(representative);
            }
        }
        Ok(())
    }

    pub fn merge(&mut self, mut other: Self) -> Result<(), WasmExactSearchError> {
        self.buildable_identities
            .append(&mut other.buildable_identities);
        for (identity, coverage) in other.solution_coverage {
            let entry = self
                .solution_coverage
                .entry(identity)
                .or_insert_with(|| PatternBitSet::new(coverage.pattern_count()));
            entry.union_with(&coverage).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_solution_coverage_universe_mismatch",
                )
            })?;
        }
        self.coverage_row_count = self
            .coverage_row_count
            .saturating_add(other.coverage_row_count);
        self.pattern_verified_execution_count = self
            .pattern_verified_execution_count
            .saturating_add(other.pattern_verified_execution_count);
        let next = self
            .build_variant_count
            .checked_add(other.build_variant_count);
        self.build_variant_count = next.unwrap_or(u128::MAX);
        self.count_complete &= other.count_complete && next.is_some();
        if let Some(candidate) = other.representative {
            if self
                .representative
                .is_none_or(|current| candidate.rank() < current.rank())
            {
                self.representative = Some(candidate);
            }
        }
        self.peak_build_nodes = self.peak_build_nodes.max(other.peak_build_nodes);
        self.total_build_nodes = self
            .total_build_nodes
            .saturating_add(other.total_build_nodes);
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(other.coverage_product_words);
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(other.coverage_product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(other.coverage_product_edge_checks);
        self.feasibility_states = self
            .feasibility_states
            .saturating_add(other.feasibility_states);
        self.feasibility_rejected_candidates = self
            .feasibility_rejected_candidates
            .saturating_add(other.feasibility_rejected_candidates);
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(other.peak_reachability_states);
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(other.total_reachability_states);
        self.worker_retained_bytes = self
            .worker_retained_bytes
            .saturating_add(other.worker_retained_bytes);
        self.piece_language_cache_hits = self
            .piece_language_cache_hits
            .saturating_add(other.piece_language_cache_hits);
        self.piece_language_cache_misses = self
            .piece_language_cache_misses
            .saturating_add(other.piece_language_cache_misses);
        self.standard_bag_cache_hits = self
            .standard_bag_cache_hits
            .saturating_add(other.standard_bag_cache_hits);
        self.standard_bag_cache_misses = self
            .standard_bag_cache_misses
            .saturating_add(other.standard_bag_cache_misses);
        add_reachability_metrics(&mut self.reachability_metrics, other.reachability_metrics);
        Ok(())
    }
}

pub(super) struct ParallelWorkerResult {
    pub branches: Vec<BranchSearchOutcome>,
    pub aggregate: WorkerAggregate,
    pub candidate_count: usize,
}

pub(super) fn run_branch_worker(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    targets: &[TargetGroup],
    control: &ExecutionControl,
    queue: &ParallelBranchQueue,
    shared_coverage: &SharedCoverage,
) -> Result<ParallelWorkerResult, WasmExactSearchError> {
    let mut workspace = BuildUpWorkspace::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let mut aggregate = WorkerAggregate {
        count_complete: true,
        ..WorkerAggregate::default()
    };
    let mut outcomes = Vec::new();
    let mut worker_candidate_count = 0usize;

    while let Some(task) = queue.pop() {
        if control.is_cancelled() {
            queue.abort();
            return Err(WasmExactSearchError::Cancelled);
        }
        let (outcome, candidate_count) = run_branch(
            problem,
            catalog,
            targets,
            control,
            queue,
            shared_coverage,
            task,
            &mut workspace,
            &mut evaluator,
            &mut aggregate,
        )?;
        worker_candidate_count = worker_candidate_count.saturating_add(candidate_count);
        outcomes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_parallel_branch_result_unavailable")
        })?;
        outcomes.push(outcome);
    }

    if problem.objective().kind() != ObjectiveKind::Tiling {
        if let Some(coverage) = workspace.materialize_standard_bag_coverage()? {
            shared_coverage.union(&coverage)?;
        }
    }
    aggregate.worker_retained_bytes = workspace
        .retained_bytes()
        .saturating_add(evaluator.retained_bytes())
        .saturating_add(
            aggregate.buildable_identities.capacity()
                * core::mem::size_of::<StandardBoard64TilingIdentity>(),
        )
        .saturating_add(
            aggregate
                .solution_coverage
                .values()
                .map(PatternBitSet::retained_bytes)
                .sum::<usize>(),
        );
    aggregate.piece_language_cache_hits = workspace.piece_language_coverage_hits();
    aggregate.piece_language_cache_misses = workspace.piece_language_coverage_misses();
    aggregate.standard_bag_cache_hits = workspace.standard_bag_coverage_hits();
    aggregate.standard_bag_cache_misses = workspace.standard_bag_coverage_misses();
    aggregate.reachability_metrics = workspace.reachability_metrics();
    Ok(ParallelWorkerResult {
        branches: outcomes,
        aggregate,
        candidate_count: worker_candidate_count,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_branch(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    targets: &[TargetGroup],
    control: &ExecutionControl,
    queue: &ParallelBranchQueue,
    shared_coverage: &SharedCoverage,
    task: ParallelBranchTask,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    aggregate: &mut WorkerAggregate,
) -> Result<(BranchSearchOutcome, usize), WasmExactSearchError> {
    let mut search = task.search;
    let mut candidate_hashes = Vec::new();
    let mut local_ordinal = 0usize;
    let mut truncated_reason = None;
    loop {
        if control.is_cancelled() {
            queue.abort();
            return Err(WasmExactSearchError::Cancelled);
        }
        if queue.is_aborted() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_branch_queue_aborted",
            ));
        }
        match search.advance(catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Complete => break,
            GeometryAdvance::ResourceIncomplete(reason) => {
                truncated_reason = Some(reason);
                break;
            }
            GeometryAdvance::Candidate(candidate) => {
                if !problem.allows_solution_identity(&candidate.identity) {
                    continue;
                }
                if problem.output_policy().retains_candidate_digest() {
                    candidate_hashes.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_parallel_candidate_digest_storage_unavailable",
                        )
                    })?;
                    candidate_hashes.push(candidate.identity.bucket_hash());
                }
                if problem.objective().kind() == ObjectiveKind::Tiling {
                    aggregate.observe_tiling(task.canonical_index, local_ordinal, candidate)?;
                    local_ordinal = local_ordinal.saturating_add(1);
                    continue;
                }
                let target = targets.get(candidate.target_index as usize).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_geometry_candidate_target_out_of_range",
                    ),
                )?;
                let solution_coverage_required = retains_solution_coverage_evidence(problem);
                let coverage_already_known = workspace.standard_bag_coverage_complete()
                    || shared_coverage.is_superset(target.possible_patterns.as_ref());
                let witness_mode = CandidateWitnessMode::for_candidate(
                    problem,
                    target,
                    coverage_already_known,
                    solution_coverage_required,
                );
                let result = verify_candidate(
                    problem,
                    catalog,
                    &candidate,
                    target,
                    workspace,
                    evaluator,
                    witness_mode,
                    false,
                    0,
                    control,
                )?;
                let mut solution_coverage = None;
                if let Some(coverage) = result.covered_patterns.as_ref() {
                    shared_coverage.union(coverage)?;
                    if solution_coverage_required {
                        solution_coverage = Some(coverage.clone());
                    }
                }
                if let Some(root) = result.symbolic_coverage_root {
                    if solution_coverage_required {
                        let materialized = workspace.materialize_standard_bag_root(root)?;
                        if let Some(solution_coverage) = solution_coverage.as_mut() {
                            solution_coverage.union_with(&materialized).map_err(|_| {
                                WasmExactSearchError::InvalidProblem(
                                    "wasm_parallel_solution_coverage_universe_mismatch",
                                )
                            })?;
                        } else {
                            solution_coverage = Some(materialized);
                        }
                    }
                    workspace.merge_standard_bag_coverage(root)?;
                }
                aggregate.observe(
                    task.canonical_index,
                    local_ordinal,
                    candidate,
                    result,
                    solution_coverage,
                    retains_buildable_identity_evidence(problem),
                    problem.output_policy().retains_representative_trace(),
                )?;
                local_ordinal = local_ordinal.saturating_add(1);
            }
        }
    }
    Ok((
        BranchSearchOutcome {
            canonical_index: task.canonical_index,
            geometry: search,
            candidate_count: local_ordinal,
            candidate_hashes,
            truncated_reason,
        },
        local_ordinal,
    ))
}

fn retains_solution_coverage_evidence(problem: &SearchProblem) -> bool {
    problem.solution_probability_policy().requested()
        || problem.objective().kind() == ObjectiveKind::MinimumCover
        || problem.objective().execution_constraints().requested()
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

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        retains_buildable_identity_evidence, retains_solution_coverage_evidence,
        CandidateBuildResult, GeometryCandidate, GeometryCatalog, WorkerAggregate,
    };

    #[test]
    fn coverage_summary_b2b_worker_retains_identity_and_solution_coverage_evidence() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(0)
        .with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        );
        let problem = ProblemCompiler::compile_scenario_percent(&query).expect("problem");
        assert!(retains_buildable_identity_evidence(&problem));
        assert!(retains_solution_coverage_evidence(&problem));

        let catalog = GeometryCatalog::compile(&problem).expect("catalog");
        let candidate = (0..catalog.skeleton_count())
            .find_map(|row_id| {
                GeometryCandidate::from_rows(&catalog, 0, &[u32::try_from(row_id).expect("row id")])
            })
            .expect("one-row geometry candidate");
        let coverage = PatternBitSet::from_words(1, vec![1]).expect("coverage");
        let result = CandidateBuildResult {
            buildable: true,
            covered_patterns: Some(coverage.clone()),
            symbolic_coverage_root: None,
            observation_language_root: None,
            symbolic_covered_pattern_count: 0,
            witness_pattern_id: Some(0),
            build_variant_count: 1,
            count_complete: true,
            representative_path: Vec::new(),
            graph_nodes: 1,
            coverage_product_words: 1,
            coverage_product_states: 1,
            coverage_product_edge_checks: 1,
            feasibility_states: 1,
            feasibility_rejected: false,
            reachability_states: 1,
            retained_bytes: 0,
            finesse_language: None,
        };
        let mut aggregate = WorkerAggregate {
            count_complete: true,
            ..WorkerAggregate::default()
        };

        aggregate
            .observe(
                0,
                0,
                candidate,
                result,
                Some(coverage.clone()),
                retains_buildable_identity_evidence(&problem),
                false,
            )
            .expect("worker evidence");

        assert_eq!(aggregate.buildable_identities, vec![candidate.identity]);
        assert_eq!(
            aggregate.solution_coverage.get(&candidate.identity),
            Some(&coverage)
        );
        assert!(aggregate.representative.is_none());
    }
}
