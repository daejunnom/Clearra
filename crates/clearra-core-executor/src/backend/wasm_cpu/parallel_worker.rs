use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use super::{
    buildup::{
        verify_candidate, BuildUpMemoryComponents, BuildUpWorkspace, CandidateBuildResult,
        CandidateWitnessMode,
    },
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometryAdvance, GeometryCandidate, GeometrySearch, TargetGroup},
    parallel_coverage::SharedCoverage,
    reachability::ReachabilityMetrics,
    result::retains_buildable_identity_evidence,
    standard_bag_coverage::{SharedStandardBagRequest, StandardBagMemoAccounting},
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
    pub candidate_digest: CandidateDigestEvidence,
    pub truncated_reason: Option<&'static str>,
}

/// Ordered products retain their exact stream. The established PC multiset
/// digest is a sum of independently mixed members, so retaining one hash per
/// candidate adds no evidence and can be replaced by the identical scalar fold.
pub(super) enum CandidateDigestEvidence {
    Disabled,
    Ordered(Vec<u64>),
    Multiset(u64),
}

impl CandidateDigestEvidence {
    fn new(enabled: bool, order_independent: bool) -> Self {
        if !enabled {
            Self::Disabled
        } else if order_independent {
            Self::Multiset(0)
        } else {
            Self::Ordered(Vec::new())
        }
    }

    fn observe(&mut self, hash: u64) -> Result<(), WasmExactSearchError> {
        match self {
            Self::Disabled => {}
            Self::Ordered(hashes) => {
                hashes.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_parallel_candidate_digest_storage_unavailable",
                    )
                })?;
                hashes.push(hash);
            }
            Self::Multiset(digest) => {
                *digest = super::mix_order_independent_candidate_digest(*digest, hash);
            }
        }
        Ok(())
    }

    pub fn fold_into(
        &self,
        digest: u64,
        order_independent: bool,
    ) -> Result<u64, WasmExactSearchError> {
        match self {
            Self::Disabled => Ok(digest),
            Self::Ordered(hashes) if !order_independent => Ok(hashes
                .iter()
                .fold(digest, |digest, &hash| super::mix_digest(digest, hash))),
            Self::Multiset(branch_digest) if order_independent => {
                // The stored value already includes member mixing. Mixing it
                // again would change the accepted multiset digest contract.
                Ok(digest.wrapping_add(*branch_digest))
            }
            _ => Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_candidate_digest_mode_mismatch",
            )),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::Ordered(hashes) => hashes.capacity() * core::mem::size_of::<u64>(),
            Self::Disabled | Self::Multiset(_) => 0,
        }
    }
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

/// Sum of exit-time private retained logical payloads, not a concurrent
/// allocator/OS peak. Immutable supply tables are counted separately once;
/// accelerator Arc owners are not replicated or charged per worker here.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WorkerMemoryComponents {
    pub buildup: BuildUpMemoryComponents,
    pub evaluator_bytes: usize,
    pub solution_identity_bytes: usize,
    pub solution_coverage_bytes: usize,
    pub candidate_digest_bytes: usize,
    /// Included in buildup.standard_bag_bytes, not an additional allocation.
    pub standard_bag_memo_payload_bytes: usize,
    /// Split memo/directory detail, also nested in buildup.standard_bag_bytes.
    pub standard_bag_memo: StandardBagMemoAccounting,
    /// One request-owned cursor/suffix payload, max-folded rather than summed.
    /// Excluded from buildup.standard_bag_bytes and total_bytes().
    pub shared_standard_bag_request_bytes: usize,
}

impl WorkerMemoryComponents {
    pub fn total_bytes(self) -> usize {
        self.buildup
            .piece_language_bytes
            .saturating_add(self.buildup.standard_bag_bytes)
            .saturating_add(self.buildup.reachability_bytes)
            .saturating_add(self.buildup.graph_projection_bytes)
            .saturating_add(self.buildup.other_bytes)
            .saturating_add(self.evaluator_bytes)
            .saturating_add(self.solution_identity_bytes)
            .saturating_add(self.solution_coverage_bytes)
            .saturating_add(self.candidate_digest_bytes)
    }

    fn merge(&mut self, other: Self) {
        self.buildup.piece_language_bytes = self
            .buildup
            .piece_language_bytes
            .saturating_add(other.buildup.piece_language_bytes);
        self.buildup.standard_bag_bytes = self
            .buildup
            .standard_bag_bytes
            .saturating_add(other.buildup.standard_bag_bytes);
        self.buildup.reachability_bytes = self
            .buildup
            .reachability_bytes
            .saturating_add(other.buildup.reachability_bytes);
        self.buildup.graph_projection_bytes = self
            .buildup
            .graph_projection_bytes
            .saturating_add(other.buildup.graph_projection_bytes);
        self.buildup.other_bytes = self
            .buildup
            .other_bytes
            .saturating_add(other.buildup.other_bytes);
        self.evaluator_bytes = self.evaluator_bytes.saturating_add(other.evaluator_bytes);
        self.solution_identity_bytes = self
            .solution_identity_bytes
            .saturating_add(other.solution_identity_bytes);
        self.solution_coverage_bytes = self
            .solution_coverage_bytes
            .saturating_add(other.solution_coverage_bytes);
        self.candidate_digest_bytes = self
            .candidate_digest_bytes
            .saturating_add(other.candidate_digest_bytes);
        self.standard_bag_memo_payload_bytes = self
            .standard_bag_memo_payload_bytes
            .saturating_add(other.standard_bag_memo_payload_bytes);
        self.standard_bag_memo.merge(other.standard_bag_memo);
        self.shared_standard_bag_request_bytes = self
            .shared_standard_bag_request_bytes
            .max(other.shared_standard_bag_request_bytes);
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
    pub candidate_digest_retained_bytes: usize,
    pub worker_memory_components: WorkerMemoryComponents,
    pub standard_bag_memo_storage: &'static str,
    pub piece_language_cache_hits: usize,
    pub piece_language_cache_misses: usize,
    pub standard_bag_cache_hits: usize,
    pub standard_bag_cache_misses: usize,
    pub reachability_metrics: ReachabilityMetrics,
    pub legal_board_verified_negative_prunes: usize,
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
            merge_owned_coverage(&mut self.solution_coverage, candidate.identity, coverage)?;
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
        if self.buildable_identities.is_empty() {
            // Adopt the first payload. Appending to an empty vector allocated
            // another complete identity array while its producer stayed live.
            self.buildable_identities = std::mem::take(&mut other.buildable_identities);
        } else {
            self.buildable_identities
                .try_reserve(other.buildable_identities.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_parallel_solution_storage_unavailable",
                    )
                })?;
            self.buildable_identities
                .append(&mut other.buildable_identities);
        }
        if self.solution_coverage.is_empty() {
            // A BTreeMap ownership transfer also preserves canonical key order.
            self.solution_coverage = other.solution_coverage;
        } else {
            for (identity, coverage) in other.solution_coverage {
                merge_owned_coverage(&mut self.solution_coverage, identity, coverage)?;
            }
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
        self.candidate_digest_retained_bytes = self
            .candidate_digest_retained_bytes
            .saturating_add(other.candidate_digest_retained_bytes);
        self.worker_memory_components
            .merge(other.worker_memory_components);
        self.standard_bag_memo_storage = merge_memo_storage_labels(
            self.standard_bag_memo_storage,
            other.standard_bag_memo_storage,
        );
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
        self.legal_board_verified_negative_prunes = self
            .legal_board_verified_negative_prunes
            .saturating_add(other.legal_board_verified_negative_prunes);
        Ok(())
    }
}

fn merge_memo_storage_labels(left: &'static str, right: &'static str) -> &'static str {
    match (left, right) {
        ("" | "not-used", label) => label,
        (label, "" | "not-used") => label,
        (left, right) if left == right => left,
        _ => "mixed",
    }
}

fn merge_owned_coverage(
    rows: &mut BTreeMap<StandardBoard64TilingIdentity, PatternBitSet>,
    identity: StandardBoard64TilingIdentity,
    coverage: PatternBitSet,
) -> Result<(), WasmExactSearchError> {
    match rows.entry(identity) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            // The row already owns its exact words. Move that owner instead of
            // allocating and zeroing a second dense bitset before copying it.
            entry.insert(coverage);
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            entry.get_mut().union_with(&coverage).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_solution_coverage_universe_mismatch",
                )
            })?;
        }
    }
    Ok(())
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
    shared_standard_bag: Option<Arc<SharedStandardBagRequest>>,
) -> Result<ParallelWorkerResult, WasmExactSearchError> {
    let mut workspace = BuildUpWorkspace::default();
    if let Some(shared_request) = shared_standard_bag {
        workspace.set_shared_standard_bag_request(shared_request)?;
    }
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
    aggregate.candidate_digest_retained_bytes = outcomes
        .iter()
        .map(|branch: &BranchSearchOutcome| branch.candidate_digest.retained_bytes())
        .sum();
    let shared_standard_bag_request_bytes = workspace.shared_standard_bag_request_retained_bytes();
    let mut buildup_memory = workspace.memory_components();
    buildup_memory.standard_bag_bytes = buildup_memory
        .standard_bag_bytes
        .saturating_sub(shared_standard_bag_request_bytes);
    aggregate.worker_memory_components = WorkerMemoryComponents {
        buildup: buildup_memory,
        evaluator_bytes: evaluator.retained_bytes(),
        solution_identity_bytes: aggregate.buildable_identities.capacity()
            * core::mem::size_of::<StandardBoard64TilingIdentity>(),
        solution_coverage_bytes: aggregate
            .solution_coverage
            .values()
            .map(PatternBitSet::retained_bytes)
            .sum(),
        candidate_digest_bytes: aggregate.candidate_digest_retained_bytes,
        standard_bag_memo_payload_bytes: workspace.standard_bag_memo_retained_payload_bytes(),
        standard_bag_memo: workspace.standard_bag_memo_accounting(),
        shared_standard_bag_request_bytes,
    };
    aggregate.worker_retained_bytes = aggregate.worker_memory_components.total_bytes();
    aggregate.standard_bag_memo_storage = workspace.standard_bag_memo_storage_label();
    aggregate.piece_language_cache_hits = workspace.piece_language_coverage_hits();
    aggregate.piece_language_cache_misses = workspace.piece_language_coverage_misses();
    aggregate.standard_bag_cache_hits = workspace.standard_bag_coverage_hits();
    aggregate.standard_bag_cache_misses = workspace.standard_bag_coverage_misses();
    aggregate.reachability_metrics = workspace.reachability_metrics();
    aggregate.legal_board_verified_negative_prunes =
        workspace.legal_board_verified_negative_prunes();
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
    let mut candidate_digest = CandidateDigestEvidence::new(
        problem.output_policy().retains_candidate_digest(),
        super::uses_order_independent_pc_candidate_digest(problem),
    );
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
                    candidate_digest.observe(candidate.identity.bucket_hash())?;
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
            candidate_digest,
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
    total.conditioned_complete_hits = total
        .conditioned_complete_hits
        .saturating_add(next.conditioned_complete_hits);
    total.conditioned_misses = total
        .conditioned_misses
        .saturating_add(next.conditioned_misses);
    total.conditioned_lookup_attempts = total
        .conditioned_lookup_attempts
        .saturating_add(next.conditioned_lookup_attempts);
    total.conditioned_empty_entry_sets = total
        .conditioned_empty_entry_sets
        .saturating_add(next.conditioned_empty_entry_sets);
    total.conditioned_no_query_context = total
        .conditioned_no_query_context
        .saturating_add(next.conditioned_no_query_context);
    total.conditioned_cache_short_circuits = total
        .conditioned_cache_short_circuits
        .saturating_add(next.conditioned_cache_short_circuits);
    total.conditioned_out_of_scope = total
        .conditioned_out_of_scope
        .saturating_add(next.conditioned_out_of_scope);
    total.conditioned_unknown = total
        .conditioned_unknown
        .saturating_add(next.conditioned_unknown);
    total.conditioned_snapshot_mismatch = total
        .conditioned_snapshot_mismatch
        .saturating_add(next.conditioned_snapshot_mismatch);
    total.conditioned_invalid_asset = total
        .conditioned_invalid_asset
        .saturating_add(next.conditioned_invalid_asset);
    total.conditioned_requested |= next.conditioned_requested;
    total.conditioned_policy_enabled |= next.conditioned_policy_enabled;
    total.conditioned_snapshot_active |= next.conditioned_snapshot_active;
    total.partial_searches = total.partial_searches.saturating_add(next.partial_searches);
    total.exhaustive_searches = total
        .exhaustive_searches
        .saturating_add(next.exhaustive_searches);
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
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
        CandidateBuildResult, CandidateDigestEvidence, GeometryCandidate, GeometryCatalog,
        StandardBagMemoAccounting, WorkerAggregate,
    };

    #[test]
    fn scalar_multiset_digest_preserves_duplicates_overflow_and_worker_partitioning() {
        let hashes = [0, u64::MAX, 3, 3, 1_u64 << 63, u64::MAX, 42, 42];
        let reference = hashes.iter().fold(0, |digest, &hash| {
            super::super::mix_order_independent_candidate_digest(digest, hash)
        });
        for workers in [1, 2, 11] {
            let mut branches = (0..workers)
                .map(|_| CandidateDigestEvidence::new(true, true))
                .collect::<Vec<_>>();
            for (index, hash) in hashes.iter().copied().enumerate() {
                branches[index % workers]
                    .observe(hash)
                    .expect("scalar fold");
            }
            for reverse in [false, true] {
                let order = if reverse {
                    (0..workers).rev().collect::<Vec<_>>()
                } else {
                    (0..workers).collect::<Vec<_>>()
                };
                let mut digest = 0;
                for index in order {
                    digest = branches[index].fold_into(digest, true).expect("same mode");
                    assert_eq!(branches[index].retained_bytes(), 0);
                }
                assert_eq!(digest, reference);
            }
        }
    }

    #[test]
    fn ordered_digest_keeps_the_complete_stream_and_rejects_mode_mismatch() {
        let hashes = [u64::MAX, 0, 19, 19, 42];
        let mut evidence = CandidateDigestEvidence::new(true, false);
        for hash in hashes {
            evidence.observe(hash).expect("ordered hash");
        }
        let reference = hashes
            .iter()
            .fold(0, |digest, &hash| super::super::mix_digest(digest, hash));
        assert_eq!(
            evidence.fold_into(0, false).expect("ordered fold"),
            reference
        );
        assert!(evidence.retained_bytes() >= hashes.len() * core::mem::size_of::<u64>());
        assert!(evidence.fold_into(0, true).is_err());
        assert!(CandidateDigestEvidence::new(true, true)
            .fold_into(0, false)
            .is_err());
        let mut disabled = CandidateDigestEvidence::new(false, false);
        disabled.observe(1).expect("not retained");
        assert_eq!(disabled.fold_into(17, false).expect("disabled fold"), 17);
        assert_eq!(disabled.retained_bytes(), 0);
    }

    #[test]
    fn coverage_owner_moves_for_new_identity_and_only_unions_duplicates() {
        let identity = clearra_core_domain::solution::normalized_tiling_solution::
            StandardBoard64TilingIdentity::from_placements(
                0,
                std::iter::empty(),
            )
            .expect("empty identity");
        let coverage = PatternBitSet::from_words(128, vec![u64::MAX, 0xff]).expect("coverage");
        let retained_owner = coverage.clone();
        let mut rows = std::collections::BTreeMap::new();
        super::merge_owned_coverage(&mut rows, identity, coverage).expect("owned first row");
        assert!(rows[&identity].shares_storage_with(&retained_owner));
        let duplicate = PatternBitSet::from_words(128, vec![0, 0xff00]).expect("duplicate");
        let reference = retained_owner.union(&duplicate).expect("reference union");
        super::merge_owned_coverage(&mut rows, identity, duplicate).expect("duplicate union");
        assert_eq!(rows[&identity], reference);
        assert_eq!(rows.len(), 1);
        assert!(super::merge_owned_coverage(&mut rows, identity, PatternBitSet::new(1)).is_err());
        assert_eq!(rows[&identity], reference);
    }

    #[test]
    fn v081_worker_component_accounting_preserves_sum_and_nested_memo_bytes() {
        let component = super::WorkerMemoryComponents {
            buildup: super::BuildUpMemoryComponents {
                piece_language_bytes: 100,
                standard_bag_bytes: 200,
                reachability_bytes: 300,
                graph_projection_bytes: 400,
                other_bytes: 500,
            },
            evaluator_bytes: 600,
            solution_identity_bytes: 700,
            solution_coverage_bytes: 800,
            candidate_digest_bytes: 900,
            // Nested memo payload must not be added to the total again.
            standard_bag_memo_payload_bytes: 144,
            standard_bag_memo: StandardBagMemoAccounting {
                product_layout: "state-major",
                product_storage: "state-major",
                union_storage: "reference",
                product_payload_bytes: 80,
                union_payload_bytes: 64,
                product_directory_bytes: 16,
                product_entries: 5,
                union_entries: 3,
                product_capacity: 10,
                union_capacity: 4,
                product_active_rows: 2,
                product_allocated_rows: 3,
                product_row_slots: 1024,
            },
            // Eleven workers share this exact request owner once.
            shared_standard_bag_request_bytes: 64,
        };
        assert_eq!(component.total_bytes(), 4500);
        let mut total = WorkerAggregate::default();
        for _ in 0..11 {
            total
                .merge(WorkerAggregate {
                    worker_retained_bytes: component.total_bytes(),
                    worker_memory_components: component,
                    standard_bag_memo_storage: "state-major",
                    ..WorkerAggregate::default()
                })
                .expect("worker components");
        }
        assert_eq!(total.worker_retained_bytes, 49_500);
        assert_eq!(total.worker_memory_components.total_bytes(), 49_500);
        assert_eq!(
            total
                .worker_memory_components
                .standard_bag_memo_payload_bytes,
            1584
        );
        assert_eq!(total.standard_bag_memo_storage, "state-major");
        let memo = total.worker_memory_components.standard_bag_memo;
        assert_eq!(memo.product_payload_bytes, 880);
        assert_eq!(memo.union_payload_bytes, 704);
        assert_eq!(memo.product_directory_bytes, 176);
        assert_eq!(
            memo.product_payload_bytes + memo.union_payload_bytes,
            total
                .worker_memory_components
                .standard_bag_memo_payload_bytes
        );
        assert_eq!(memo.product_entries, 55);
        assert_eq!(memo.product_active_rows, 22);
        assert_eq!(memo.product_allocated_rows, 33);
        assert_eq!(memo.product_row_slots, 11 * 1024);
        assert_eq!(memo.product_layout, "state-major");
        assert_eq!(memo.product_storage, "state-major");
        assert_eq!(memo.union_storage, "reference");
        assert_eq!(
            total
                .worker_memory_components
                .shared_standard_bag_request_bytes,
            64
        );
        assert_eq!(
            super::merge_memo_storage_labels("compact", "reference"),
            "mixed"
        );
        assert_eq!(
            super::merge_memo_storage_labels("compact", "not-used"),
            "compact"
        );
        let mut saturation = component;
        saturation.merge(super::WorkerMemoryComponents {
            evaluator_bytes: usize::MAX,
            ..Default::default()
        });
        assert_eq!(saturation.total_bytes(), usize::MAX);
    }

    #[test]
    fn v081_first_worker_merge_adopts_identity_allocation_and_coverage_backing() {
        let identity = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty())
            .expect("empty identity");
        let mut identities = Vec::with_capacity(64);
        identities.push(identity);
        let allocation = identities.as_ptr();
        let coverage =
            PatternBitSet::from_pattern_indices(100_000, vec![3, 999]).expect("sparse exact row");
        let evidence_owner = coverage.clone();
        let mut source_rows = std::collections::BTreeMap::new();
        source_rows.insert(identity, coverage);
        let mut destination = WorkerAggregate::default();
        destination
            .merge(WorkerAggregate {
                buildable_identities: identities,
                solution_coverage: source_rows,
                ..Default::default()
            })
            .expect("ownership transfer");
        assert_eq!(destination.buildable_identities.as_ptr(), allocation);
        assert_eq!(destination.buildable_identities.capacity(), 64);
        assert!(destination.solution_coverage[&identity].shares_storage_with(&evidence_owner));
        assert_eq!(destination.solution_coverage[&identity], evidence_owner);
    }

    #[test]
    fn conditioned_reachability_counters_survive_worker_merge() {
        let mut total = super::ReachabilityMetrics {
            conditioned_complete_hits: 1,
            conditioned_misses: 2,
            conditioned_lookup_attempts: 3,
            conditioned_requested: true,
            conditioned_policy_enabled: true,
            ..Default::default()
        };
        super::add_reachability_metrics(
            &mut total,
            super::ReachabilityMetrics {
                conditioned_complete_hits: 3,
                conditioned_misses: 4,
                conditioned_lookup_attempts: 5,
                conditioned_snapshot_active: true,
                ..Default::default()
            },
        );
        assert_eq!(total.conditioned_complete_hits, 4);
        assert_eq!(total.conditioned_misses, 6);
        assert_eq!(total.conditioned_lookup_attempts, 8);
        assert!(total.conditioned_requested);
        assert!(total.conditioned_policy_enabled);
        assert!(total.conditioned_snapshot_active);
    }

    #[test]
    fn legal_board_verified_negative_prunes_survive_worker_merge() {
        let mut total = WorkerAggregate {
            legal_board_verified_negative_prunes: 2,
            ..Default::default()
        };
        total
            .merge(WorkerAggregate {
                legal_board_verified_negative_prunes: 5,
                ..Default::default()
            })
            .expect("compatible worker counters");
        assert_eq!(total.legal_board_verified_negative_prunes, 7);
    }

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
