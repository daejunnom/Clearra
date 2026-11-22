// SRP rationale: the single change reason is executing one compact native branch
// worker, including its private frontier, memory accounting and mergeable result evidence.
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::{pattern::pattern_bitset::PatternBitSet, row::coverage_row::CoverageRow};
use clearra_problem::SearchProblem;

use crate::performance::{profile_sample_scale, ExecutorSearchStage, SearchStageSpan};

use super::buildup::ObservationProducerShard;
#[cfg(any(test, feature = "local-search-ab"))]
use super::result::{capture_candidate_solution_coverage, SolutionCoverageCapture};
use super::{
    buildup::{
        verify_candidate, BuildUpMemoryComponents, BuildUpWorkspace, CandidateBuildResult,
        CandidateWitnessMode,
    },
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometryAdvance, GeometryCandidate, GeometrySearch, TargetGroup},
    parallel_coverage::SharedCoverage,
    reachability::{ReachabilityMetrics, SharedReachabilityTemplates},
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
    state: AtomicU8,
    #[cfg(test)]
    lock_attempts: AtomicUsize,
}

#[repr(u8)]
enum BranchQueueState {
    Available,
    Exhausted,
    Aborted,
}

impl ParallelBranchQueue {
    pub fn new(mut tasks: Vec<ParallelBranchTask>) -> Self {
        tasks.sort_unstable_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.canonical_index.cmp(&right.canonical_index))
        });
        let state = if tasks.is_empty() {
            BranchQueueState::Exhausted
        } else {
            BranchQueueState::Available
        };
        Self {
            tasks: Mutex::new(tasks.into()),
            state: AtomicU8::new(state as u8),
            #[cfg(test)]
            lock_attempts: AtomicUsize::new(0),
        }
    }

    pub fn pop(&self) -> Result<Option<ParallelBranchTask>, WasmExactSearchError> {
        // This queue only hands out pre-partitioned branches; workers do not
        // enqueue into it. Once exhausted, there is no work to steal later.
        if self.state.load(Ordering::Acquire) != BranchQueueState::Available as u8 {
            return Ok(None);
        }
        #[cfg(test)]
        self.lock_attempts.fetch_add(1, Ordering::Relaxed);
        let mut tasks = self.tasks.lock().map_err(|_| {
            self.abort();
            WasmExactSearchError::InvalidProblem("wasm_parallel_branch_queue_poisoned")
        })?;
        let Some(task) = tasks.pop_front() else {
            // This worker can have observed Available before another worker
            // claimed the last branch under the same lock.
            return Ok(None);
        };
        if tasks.is_empty() {
            self.mark_exhausted();
        }
        Ok(Some(task))
    }

    fn mark_exhausted(&self) {
        // The final claim and cancellation can race. Exhaustion must never
        // replace Aborted, since it does not cancel an in-flight branch.
        let _ = self.state.compare_exchange(
            BranchQueueState::Available as u8,
            BranchQueueState::Exhausted as u8,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    pub fn abort(&self) {
        self.state
            .store(BranchQueueState::Aborted as u8, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.state.load(Ordering::Acquire) == BranchQueueState::Aborted as u8
    }
}

pub(super) struct BranchSearchOutcome {
    pub canonical_index: usize,
    pub geometry: GeometrySearch,
    pub candidate_count: usize,
    pub candidate_digest: CandidateDigestEvidence,
    pub chance_rows: Vec<(usize, PatternBitSet)>,
    pub candidate_domain_exhausted: bool,
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
    /// Moved physical proofs and their matching kind arena. These have left
    /// the workspace before its scratch snapshot and stay live for the
    /// coordinator's single global observation-policy evaluation.
    pub observation_producer_bytes: usize,
    /// Included in buildup.standard_bag_bytes, not an additional allocation.
    pub standard_bag_memo_payload_bytes: usize,
    /// Split memo/directory detail, also nested in buildup.standard_bag_bytes.
    pub standard_bag_memo: StandardBagMemoAccounting,
    /// One request-owned cursor/suffix payload, max-folded rather than summed.
    /// Excluded from buildup.standard_bag_bytes and total_bytes().
    pub shared_standard_bag_request_bytes: usize,
    /// Request-owned immutable transitions and lazy sky entries, never summed.
    pub shared_reachability_template_bytes: usize,
    /// Request-owned mutable observation producer, moved once at the completed
    /// native barrier. These snapshots are diagnostic, not worker allocations.
    pub shared_observation_producer_bytes: usize,
    pub shared_observation_producer_peak_bytes: usize,
    /// Sum of private source-to-destination map growth overlaps. The maps'
    /// live buffers are already in buildup.other_bytes; this is peak-only.
    pub incremental_import_peak_extra_bytes: usize,
    #[cfg(any(test, feature = "local-search-ab"))]
    pub exit_snapshot_count: usize,
    /// Largest sampled per-worker scratch at return, not a time-series peak.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub max_exit_scratch_bytes: usize,
    /// Reserved inline branch outcome slots sent to the coordinator; excludes
    /// nested geometry and digest allocations.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub returned_branch_slots_bytes: usize,
}

impl WorkerMemoryComponents {
    pub fn total_bytes(self) -> usize {
        self.private_scratch_bytes()
            .saturating_add(self.returned_payload_bytes())
    }

    pub fn private_scratch_bytes(self) -> usize {
        self.buildup
            .piece_language_bytes
            .saturating_add(self.buildup.standard_bag_bytes)
            .saturating_add(self.buildup.reachability_bytes)
            .saturating_add(self.buildup.graph_projection_bytes)
            .saturating_add(self.buildup.other_bytes)
            .saturating_add(self.evaluator_bytes)
    }

    pub fn returned_payload_bytes(self) -> usize {
        self.solution_identity_bytes
            .saturating_add(self.solution_coverage_bytes)
            .saturating_add(self.candidate_digest_bytes)
            .saturating_add(self.observation_producer_bytes)
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
        self.observation_producer_bytes = self
            .observation_producer_bytes
            .saturating_add(other.observation_producer_bytes);
        self.standard_bag_memo_payload_bytes = self
            .standard_bag_memo_payload_bytes
            .saturating_add(other.standard_bag_memo_payload_bytes);
        self.standard_bag_memo.merge(other.standard_bag_memo);
        self.shared_standard_bag_request_bytes = self
            .shared_standard_bag_request_bytes
            .max(other.shared_standard_bag_request_bytes);
        self.shared_reachability_template_bytes = self
            .shared_reachability_template_bytes
            .max(other.shared_reachability_template_bytes);
        self.shared_observation_producer_bytes = self
            .shared_observation_producer_bytes
            .max(other.shared_observation_producer_bytes);
        self.shared_observation_producer_peak_bytes = self
            .shared_observation_producer_peak_bytes
            .max(other.shared_observation_producer_peak_bytes);
        self.incremental_import_peak_extra_bytes = self
            .incremental_import_peak_extra_bytes
            .saturating_add(other.incremental_import_peak_extra_bytes);
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.exit_snapshot_count = self
                .exit_snapshot_count
                .saturating_add(other.exit_snapshot_count);
            self.max_exit_scratch_bytes = self
                .max_exit_scratch_bytes
                .max(other.max_exit_scratch_bytes);
            self.returned_branch_slots_bytes = self
                .returned_branch_slots_bytes
                .saturating_add(other.returned_branch_slots_bytes);
        }
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
    pub observation_producer_shards: Vec<ObservationProducerShard>,
}

pub(super) struct SharedWorkerRequest {
    pub standard_bag: Option<Arc<SharedStandardBagRequest>>,
    #[cfg(feature = "placement-dependency-pc")]
    pub board_observation_plan: Option<Arc<super::queue_observation_policy::BoardObservationPlan>>,
    #[cfg(feature = "placement-dependency-pc")]
    pub shared_observation_producer:
        Option<Arc<super::queue_observation_policy::SharedObservationProducer>>,
    pub reachability: Arc<SharedReachabilityTemplates>,
    pub probability_row_budget: Option<ParallelProbabilityRowBudget>,
    #[cfg(any(test, feature = "local-search-ab"))]
    pub solution_coverage_capture: Option<SolutionCoverageCapture>,
    #[cfg(feature = "local-search-ab")]
    pub operation_masks: Option<super::buildup::OperationMaskExperiment>,
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    pub strict_prefix: Option<super::symbolic_prefix_parallel::StrictParallelPrefixRequest>,
}

/// Shared allowance for typed chance proof rows, not a limit on candidate
/// search or a new per-worker cap. This is only for the newly moved proof:
/// other products keep their existing row and execution authority.
pub(super) struct ParallelProbabilityRowBudget {
    limit: usize,
    dense_constructor_bytes: usize,
    charged: AtomicUsize,
}

impl ParallelProbabilityRowBudget {
    pub fn new(pattern_count: usize) -> Result<Self, WasmExactSearchError> {
        let dense_constructor_bytes = PatternBitSet::checked_all_projection(pattern_count)
            .and_then(|projection| usize::try_from(projection.constructor_peak_bytes).ok())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_probability_row_projection_overflow",
            ))?;
        Ok(Self {
            limit: usize::try_from(
                crate::resource::shared_execution_resource_capacity().memory_bytes,
            )
            .unwrap_or(usize::MAX),
            dense_constructor_bytes,
            charged: AtomicUsize::new(0),
        })
    }

    fn charge(&self, payload_bytes: usize) -> Result<(), WasmExactSearchError> {
        use clearra_core_domain::resource::{
            ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport,
        };

        // Reserve the branch vector's growth and the final owned row slots.
        // Charge the constructor peak for symbolic rows so simultaneous
        // materializations cannot exceed the same request-wide allowance.
        let bytes = payload_bytes.checked_add(
            2 * core::mem::size_of::<(usize, PatternBitSet)>()
                + core::mem::size_of::<CoverageRow>(),
        );
        self.charged
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
                previous
                    .checked_add(bytes?)
                    .filter(|&next| next <= self.limit)
            })
            .map(|_| ())
            .map_err(|previous| {
                let required = bytes
                    .and_then(|bytes| previous.checked_add(bytes))
                    .map_or(u128::MAX, |bytes| bytes as u128);
                let mut report = ResourceReport::execution_incomplete(
                    ExecutionAvailability::exhausted(
                        ExecutionAvailabilityReason::MemoryBudgetExceeded,
                    )
                    .with_required_memory_bytes(required),
                );
                report.mark_truncated_unknown();
                WasmExactSearchError::resource_admission(report)
            })
    }
}

pub(super) fn retains_ordinal_chance_rows(problem: &SearchProblem) -> bool {
    let policy = problem.pc_chance_evidence_policy();
    policy.retains_pc_probability_v2_evidence() || policy.pc_failed_queue_example_limit().is_some()
}

/// One owner for immutable inputs and the explicitly shared work/coverage
/// queues. Pooled jobs pin this owner once; scratch remains worker-local.
pub(super) struct BranchWorkerInputs {
    pub problem: Arc<SearchProblem>,
    pub catalog: Arc<GeometryCatalog>,
    pub targets: Arc<[TargetGroup]>,
    pub control: ExecutionControl,
    pub queue: ParallelBranchQueue,
    pub coverage: SharedCoverage,
    pub request: SharedWorkerRequest,
}

pub(super) fn run_branch_worker(
    inputs: &BranchWorkerInputs,
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))] memory_probe: Option<(
        &super::parallel_memory_peak::MemoryProbe,
        usize,
    )>,
) -> Result<ParallelWorkerResult, WasmExactSearchError> {
    let run = || {
        run_branch_worker_inner(
            inputs,
            #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
            memory_probe,
        )
    };
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    if let Some(request) = &inputs.request.strict_prefix {
        use super::symbolic_prefix_orders::{
            with_catalog_static_guards, with_legacy_upper, with_packed_upper_target,
            with_strict_prefix_order,
        };

        let catalog_table = request.catalog_static_guards(&inputs.catalog, &inputs.control);
        let catalog_table = match catalog_table {
            Ok(table) => table,
            Err(error) => {
                inputs.queue.abort();
                return Err(error);
            }
        };
        let strict = || {
            with_legacy_upper(request.legacy_upper(), || {
                with_packed_upper_target(|| with_strict_prefix_order(run))
            })
        };
        let (outcome, work) = match catalog_table {
            Some(table) => with_catalog_static_guards(table, strict),
            None => strict(),
        };
        let outcome = outcome?;
        request.publish_worker(outcome.candidate_count, work)?;
        return Ok(outcome);
    }
    let result = run();
    if let Err(error) = &result {
        inputs.queue.abort();
        #[cfg(feature = "placement-dependency-pc")]
        if let Some(shared) = &inputs.request.shared_observation_producer {
            shared.abort_with_error("native_worker", error);
        }
        #[cfg(not(feature = "placement-dependency-pc"))]
        let _ = error;
    }
    result
}

fn run_branch_worker_inner(
    inputs: &BranchWorkerInputs,
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))] memory_probe: Option<(
        &super::parallel_memory_peak::MemoryProbe,
        usize,
    )>,
) -> Result<ParallelWorkerResult, WasmExactSearchError> {
    let BranchWorkerInputs {
        problem,
        catalog,
        targets,
        control,
        queue,
        coverage: shared_coverage,
        request: shared_request,
    } = inputs;
    let mut workspace = BuildUpWorkspace::default();
    #[cfg(feature = "placement-dependency-pc")]
    if let Some(plan) = &shared_request.board_observation_plan {
        workspace.set_board_observation_plan(Arc::clone(plan))?;
    }
    #[cfg(feature = "placement-dependency-pc")]
    if let Some(shared) = &shared_request.shared_observation_producer {
        workspace.set_shared_observation_producer(Arc::clone(shared))?;
    }
    #[cfg(feature = "local-search-ab")]
    if let Some(experiment) = &shared_request.operation_masks {
        workspace.begin_operation_mask_worker(experiment);
    }
    workspace.set_shared_reachability_templates(Arc::clone(&shared_request.reachability))?;
    if let Some(shared_standard_bag) = &shared_request.standard_bag {
        workspace.set_shared_standard_bag_request(Arc::clone(shared_standard_bag))?;
    }
    let mut evaluator = CoverageProductEvaluator::default();
    let mut aggregate = WorkerAggregate {
        count_complete: true,
        ..WorkerAggregate::default()
    };
    let mut outcomes = Vec::new();
    let mut worker_candidate_count = 0usize;

    #[cfg(all(feature = "local-search-ab", feature = "placement-dependency-pc"))]
    if shared_request.board_observation_plan.is_some() {
        eprintln!("clearra_observation_board_worker_start");
    }

    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    if let Some((probe, worker)) = memory_probe {
        probe.worker(worker, Default::default(), false);
    }
    while let Some(task) = queue.pop()? {
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
            shared_request.probability_row_budget.as_ref(),
            #[cfg(any(test, feature = "local-search-ab"))]
            shared_request.solution_coverage_capture.as_ref(),
            #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
            memory_probe.map(|(probe, worker)| (probe, worker, branch_storage_bytes(&outcomes))),
        )?;
        worker_candidate_count = worker_candidate_count.saturating_add(candidate_count);
        outcomes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_parallel_branch_result_unavailable")
        })?;
        outcomes.push(outcome);
        #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
        if let Some((probe, worker)) = memory_probe {
            probe.worker(
                worker,
                worker_storage(
                    &workspace,
                    &evaluator,
                    &aggregate,
                    branch_storage_bytes(&outcomes),
                ),
                false,
            );
        }
    }

    if problem.objective().kind() != ObjectiveKind::Tiling {
        workspace.publish_worker_standard_bag_coverage(shared_coverage)?;
    }
    #[cfg(feature = "local-search-ab")]
    if let Some(experiment) = &shared_request.operation_masks {
        workspace.finish_operation_mask_worker(experiment)?;
    }
    // Capture diagnostics before transferring the matching kind owner.
    // Physical proofs are not worker policies: the coordinator must union
    // their concrete actions and make one global choice before each reveal.
    aggregate.piece_language_cache_hits = workspace.piece_language_coverage_hits();
    aggregate.piece_language_cache_misses = workspace.piece_language_coverage_misses();
    #[cfg(all(feature = "local-search-ab", feature = "placement-dependency-pc"))]
    if shared_request.board_observation_plan.is_some() {
        eprintln!(
            "clearra_observation_board_worker_done candidates={} kind_storage={:?} source_existence={:?} board_and_prefix_cones={:?} prefix_cone_exact={:?}",
            worker_candidate_count,
            workspace.observation_kind_storage_metrics(),
            workspace.observation_source_existence_metrics(),
            workspace.observation_board_producer_metrics(),
            workspace.observation_prefix_cone_exact_producer_metrics(),
        );
        if let Some(shared) = &shared_request.shared_observation_producer {
            // Common request snapshots overlap. They must not be summed as
            // if each worker retained an independent producer graph.
            eprintln!(
                "clearra_observation_shared_producer_worker_snapshot metrics={:?}",
                shared.metrics(),
            );
        }
    }
    let mut observation_producer_shards = Vec::new();
    if let Some(shard) = workspace.take_observation_producer_shard() {
        observation_producer_shards
            .try_reserve_exact(1)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_observation_producer_storage_unavailable",
                )
            })?;
        observation_producer_shards.push(shard);
    }
    let observation_producer_bytes = observation_producer_shards.iter().fold(
        observation_producer_shards.capacity() * core::mem::size_of::<ObservationProducerShard>(),
        |bytes, shard| bytes.saturating_add(shard.retained_bytes()),
    );
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
            .sum::<usize>()
            .saturating_add(outcomes.iter().fold(0usize, |bytes, branch| {
                let rows = &branch.chance_rows;
                let backing = rows
                    .capacity()
                    .saturating_mul(core::mem::size_of::<(usize, PatternBitSet)>());
                rows.iter()
                    .fold(bytes.saturating_add(backing), |total, (_, bits)| {
                        total.saturating_add(bits.retained_bytes())
                    })
            })),
        candidate_digest_bytes: aggregate.candidate_digest_retained_bytes,
        observation_producer_bytes,
        standard_bag_memo_payload_bytes: workspace.standard_bag_memo_retained_payload_bytes(),
        standard_bag_memo: workspace.standard_bag_memo_accounting(),
        shared_standard_bag_request_bytes,
        shared_reachability_template_bytes: shared_request.reachability.retained_bytes(),
        // The coordinator reads the one live request owner after the barrier.
        // Worker snapshots must not multiply its graph or destination arena.
        shared_observation_producer_bytes: 0,
        shared_observation_producer_peak_bytes: 0,
        incremental_import_peak_extra_bytes: workspace
            .shared_observation_private_peak_extra_bytes(),
        #[cfg(any(test, feature = "local-search-ab"))]
        exit_snapshot_count: 1,
        #[cfg(any(test, feature = "local-search-ab"))]
        max_exit_scratch_bytes: 0,
        #[cfg(any(test, feature = "local-search-ab"))]
        returned_branch_slots_bytes: outcomes
            .capacity()
            .saturating_mul(core::mem::size_of::<BranchSearchOutcome>()),
    };
    #[cfg(any(test, feature = "local-search-ab"))]
    {
        aggregate.worker_memory_components.max_exit_scratch_bytes =
            aggregate.worker_memory_components.private_scratch_bytes();
    }
    aggregate.worker_retained_bytes = aggregate.worker_memory_components.total_bytes();
    aggregate.standard_bag_memo_storage = workspace.standard_bag_memo_storage_label();
    aggregate.standard_bag_cache_hits = workspace.standard_bag_coverage_hits();
    aggregate.standard_bag_cache_misses = workspace.standard_bag_coverage_misses();
    aggregate.reachability_metrics = workspace.reachability_metrics();
    aggregate.legal_board_verified_negative_prunes =
        workspace.legal_board_verified_negative_prunes();
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    if let Some((probe, worker)) = memory_probe {
        drop(workspace);
        drop(evaluator);
        probe.worker(
            worker,
            super::parallel_memory_peak::WorkerStorage {
                solution_identity_bytes: aggregate.worker_memory_components.solution_identity_bytes,
                branch_and_digest_bytes: branch_storage_bytes(&outcomes),
                observation_producer_bytes,
                ..Default::default()
            },
            true,
        );
    }
    Ok(ParallelWorkerResult {
        branches: outcomes,
        aggregate,
        candidate_count: worker_candidate_count,
        observation_producer_shards,
    })
}

#[cfg(all(feature = "local-search-ab", target_os = "linux"))]
fn branch_storage_bytes(branches: &Vec<BranchSearchOutcome>) -> usize {
    branches.capacity() * core::mem::size_of::<BranchSearchOutcome>()
        + branches
            .iter()
            .map(|branch| branch.candidate_digest.retained_bytes())
            .sum::<usize>()
}

#[cfg(all(feature = "local-search-ab", target_os = "linux"))]
fn worker_storage(
    workspace: &BuildUpWorkspace,
    evaluator: &CoverageProductEvaluator,
    aggregate: &WorkerAggregate,
    branch_and_digest_bytes: usize,
) -> super::parallel_memory_peak::WorkerStorage {
    let buildup = workspace.memory_components();
    let memo = workspace.standard_bag_memo_accounting();
    // Read after memory_components(): shared bag initialization is one-way,
    // so an initialization race can undercount the private estimate by at
    // most one shared-payload amount and cannot charge the common table per worker.
    let shared_bag = workspace.shared_standard_bag_request_retained_bytes();
    super::parallel_memory_peak::WorkerStorage {
        standard_bag_bytes: buildup.standard_bag_bytes.saturating_sub(shared_bag),
        bag_product_memo_bytes: memo.product_payload_bytes,
        bag_union_memo_bytes: memo.union_payload_bytes,
        piece_language_bytes: buildup.piece_language_bytes,
        reachability_bytes: buildup.reachability_bytes,
        graph_projection_bytes: buildup.graph_projection_bytes,
        other_buildup_bytes: buildup.other_bytes,
        evaluator_bytes: evaluator.retained_bytes(),
        solution_identity_bytes: aggregate.buildable_identities.capacity()
            * core::mem::size_of::<StandardBoard64TilingIdentity>(),
        branch_and_digest_bytes,
        observation_producer_bytes: 0,
    }
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
    probability_row_budget: Option<&ParallelProbabilityRowBudget>,
    #[cfg(any(test, feature = "local-search-ab"))] solution_coverage_capture: Option<
        &SolutionCoverageCapture,
    >,
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))] memory_probe: Option<(
        &super::parallel_memory_peak::MemoryProbe,
        usize,
        usize,
    )>,
) -> Result<(BranchSearchOutcome, usize), WasmExactSearchError> {
    let mut search = task.search;
    let retain_candidate_digest = problem.output_policy().retains_candidate_digest()
        && !search.physical_prefix_pruning_enabled();
    let mut candidate_digest = CandidateDigestEvidence::new(
        retain_candidate_digest,
        super::uses_order_independent_pc_candidate_digest(problem),
    );
    let mut local_ordinal = 0usize;
    let mut truncated_reason = None;
    let mut candidate_domain_exhausted = false;
    let retain_chance_rows = retains_ordinal_chance_rows(problem);
    let mut chance_rows = Vec::new();
    let solution_coverage_required = retains_solution_coverage_evidence(problem);
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    let mut geometry_advance_calls = 0_usize;
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
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        let geometry_profile_scale = {
            geometry_advance_calls = geometry_advance_calls.saturating_add(1);
            profile_sample_scale(geometry_advance_calls)
        };
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        let geometry_profile_scale = 0;
        let geometry_span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometryAdvance,
            geometry_profile_scale,
        );
        let advancement = search.advance(catalog);
        geometry_span.finish(1);
        match advancement {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Complete => {
                candidate_domain_exhausted = true;
                break;
            }
            GeometryAdvance::ResourceIncomplete(reason) => {
                truncated_reason = Some(reason);
                break;
            }
            GeometryAdvance::Candidate(candidate) => {
                if !problem.allows_solution_identity(&candidate.identity) {
                    continue;
                }
                if retain_candidate_digest {
                    candidate_digest.observe(candidate.identity.bucket_hash())?;
                }
                if problem.objective().kind() == ObjectiveKind::Tiling {
                    aggregate.observe_tiling(task.canonical_index, local_ordinal, candidate)?;
                    local_ordinal = local_ordinal.saturating_add(1);
                    continue;
                }
                #[cfg(feature = "placement-dependency-pc")]
                let observation_produced = {
                    #[cfg(any(test, feature = "local-search-ab"))]
                    let source_only_allowed = solution_coverage_capture.is_none();
                    #[cfg(not(any(test, feature = "local-search-ab")))]
                    let source_only_allowed = true;
                    source_only_allowed
                        && workspace.try_produce_board_observation(
                            problem,
                            catalog,
                            &candidate,
                            control,
                            profile_sample_scale(local_ordinal.saturating_add(1)),
                        )? != super::buildup::ObservationProducerDecision::NotSelected
                };
                #[cfg(not(feature = "placement-dependency-pc"))]
                let observation_produced = false;
                // This typed producer retains physical evidence. It does not
                // fabricate an oracle row, witness, identity, or execution
                // count. Common ordinal and memory accounting still run.
                if !observation_produced {
                    let target = targets.get(candidate.target_index as usize).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_geometry_candidate_target_out_of_range",
                        ),
                    )?;
                    let exact_candidate_coverage = solution_coverage_required || retain_chance_rows;
                    #[cfg(any(test, feature = "local-search-ab"))]
                    let exact_candidate_coverage =
                        exact_candidate_coverage || solution_coverage_capture.is_some();
                    let witness_mode = CandidateWitnessMode::for_candidate(
                        problem,
                        target,
                        || {
                            workspace.standard_bag_coverage_complete()
                                || shared_coverage.is_superset(target.possible_patterns.as_ref())
                        },
                        exact_candidate_coverage,
                    );
                    let mut result = verify_candidate(
                        problem,
                        catalog,
                        &candidate,
                        target,
                        workspace,
                        evaluator,
                        witness_mode,
                        false,
                        profile_sample_scale(local_ordinal.saturating_add(1)),
                        control,
                    )?;
                    let mut solution_coverage = None;
                    #[cfg(any(test, feature = "local-search-ab"))]
                    if let Some(capture) = solution_coverage_capture {
                        capture_candidate_solution_coverage(
                            capture,
                            candidate.identity,
                            &result,
                            workspace,
                        )?;
                    }
                    if let Some(coverage) = result.covered_patterns.as_ref() {
                        shared_coverage.union(coverage)?;
                        if solution_coverage_required {
                            solution_coverage = Some(coverage.clone());
                        }
                    }
                    if let Some(root) = result.symbolic_coverage_root {
                        if solution_coverage_required || retain_chance_rows {
                            if let Some(budget) = probability_row_budget {
                                budget.charge(budget.dense_constructor_bytes)?;
                            }
                            let materialized = workspace.materialize_standard_bag_root(root)?;
                            if retain_chance_rows {
                                // Concrete and symbolic results are disjoint; a
                                // second row for one candidate is not valid proof.
                                if result.covered_patterns.is_some() {
                                    return Err(WasmExactSearchError::InvalidProblem(
                                        "wasm_parallel_chance_candidate_row_ambiguous",
                                    ));
                                }
                                if solution_coverage_required {
                                    solution_coverage = Some(materialized.clone());
                                }
                                record_chance_branch_row(
                                    &mut chance_rows,
                                    local_ordinal,
                                    materialized,
                                )?;
                            } else if let Some(solution_coverage) = solution_coverage.as_mut() {
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
                    if retain_chance_rows {
                        if let Some(coverage) = result.covered_patterns.take() {
                            if let Some(budget) = probability_row_budget {
                                budget.charge(coverage.retained_bytes())?;
                            }
                            // Account the row and covered ordinals exactly as
                            // WorkerAggregate::observe would, then move the bits.
                            aggregate.coverage_row_count =
                                aggregate.coverage_row_count.saturating_add(1);
                            aggregate.pattern_verified_execution_count = aggregate
                                .pattern_verified_execution_count
                                .saturating_add(coverage.count_ones() as usize);
                            record_chance_branch_row(&mut chance_rows, local_ordinal, coverage)?;
                        }
                    }
                    let retain_identity = retains_buildable_identity_evidence(problem);
                    #[cfg(any(test, feature = "local-search-ab"))]
                    let retain_identity = retain_identity || solution_coverage_capture.is_some();
                    aggregate.observe(
                        task.canonical_index,
                        local_ordinal,
                        candidate,
                        result,
                        solution_coverage,
                        retain_identity,
                        problem.output_policy().retains_representative_trace(),
                    )?;
                }
                local_ordinal = local_ordinal.saturating_add(1);
                #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
                if let Some((probe, worker, branch_bytes)) = memory_probe {
                    if local_ordinal & super::parallel_memory_peak::WORKER_SAMPLE_MASK == 0 {
                        probe.worker(
                            worker,
                            worker_storage(
                                workspace,
                                evaluator,
                                aggregate,
                                branch_bytes.saturating_add(candidate_digest.retained_bytes()),
                            ),
                            false,
                        );
                    }
                }
            }
        }
    }
    Ok((
        BranchSearchOutcome {
            canonical_index: task.canonical_index,
            geometry: search,
            candidate_count: local_ordinal,
            candidate_digest,
            chance_rows,
            candidate_domain_exhausted,
            truncated_reason,
        },
        local_ordinal,
    ))
}

fn record_chance_branch_row(
    rows: &mut Vec<(usize, PatternBitSet)>,
    local_ordinal: usize,
    coverage: PatternBitSet,
) -> Result<(), WasmExactSearchError> {
    rows.try_reserve(1).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_parallel_chance_row_storage_unavailable")
    })?;
    rows.push((local_ordinal, coverage));
    Ok(())
}

fn retains_solution_coverage_evidence(problem: &SearchProblem) -> bool {
    problem.solution_probability_policy().requested()
        || problem.objective().kind() == ObjectiveKind::MinimumCover
        || problem.objective().execution_constraints().requested()
}

fn add_reachability_metrics(total: &mut ReachabilityMetrics, next: ReachabilityMetrics) {
    total.static_influence.accumulate(next.static_influence);
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
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::thread;

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
        GeometrySearch, ParallelBranchQueue, ParallelBranchTask, StandardBagMemoAccounting,
        WasmExactSearchError, WorkerAggregate,
    };

    fn branch_task(canonical_index: usize, priority: usize) -> ParallelBranchTask {
        ParallelBranchTask {
            canonical_index,
            priority,
            search: GeometrySearch::placeholder(),
        }
    }

    #[test]
    fn parallel_branch_queue_skips_the_lock_after_the_last_claim() {
        let queue = ParallelBranchQueue::new(vec![
            branch_task(0, 1),
            branch_task(2, 10),
            branch_task(1, 10),
        ]);
        for expected in [1, 2, 0] {
            assert_eq!(
                queue
                    .pop()
                    .expect("healthy queue")
                    .expect("queued branch")
                    .canonical_index,
                expected,
                "available branches retain priority and canonical tie order"
            );
        }
        assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 3);
        for _ in 0..11 {
            assert!(queue.pop().expect("healthy queue").is_none());
        }
        assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 3);
        assert!(!queue.is_aborted(), "in-flight final branch is still valid");

        queue.abort();
        assert!(queue.is_aborted(), "a later cancellation remains visible");
        assert!(queue.pop().expect("aborted queue").is_none());
        assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn parallel_branch_queue_final_claim_cannot_clear_an_abort() {
        let queue = ParallelBranchQueue::new(vec![branch_task(0, 1)]);
        queue.abort();
        // The abort can be published after a worker's available check, but
        // before it changes the state for the final claim.
        queue.mark_exhausted();
        assert!(queue.is_aborted());
        assert!(queue.pop().expect("aborted queue").is_none());
        assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 0);

        let empty = ParallelBranchQueue::new(Vec::new());
        assert!(!empty.is_aborted());
        assert!(empty.pop().expect("initially empty queue").is_none());
        assert_eq!(empty.lock_attempts.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn parallel_branch_queue_ready_worker_can_claim_while_early_branch_expands() {
        let queue = ParallelBranchQueue::new(vec![
            branch_task(0, 10),
            branch_task(1, 1),
            branch_task(2, 1),
        ]);
        thread::scope(|scope| {
            let (claimed_sender, claimed_receiver) = mpsc::channel();
            let (release_sender, release_receiver) = mpsc::channel();
            let owner_queue = &queue;
            let initial_worker = scope.spawn(move || {
                let branch = owner_queue
                    .pop()
                    .expect("healthy queue")
                    .expect("first branch");
                claimed_sender
                    .send(branch.canonical_index)
                    .expect("notify ready worker");
                // The branch remains owned while another worker drains the
                // queue; its in-flight work is not cancelled by exhaustion.
                release_receiver.recv().expect("release first branch");
                (
                    owner_queue.pop().expect("healthy queue").is_none(),
                    owner_queue.is_aborted(),
                )
            });
            let first = claimed_receiver
                .recv()
                .expect("initial claim was published");
            let next = queue.pop();
            let last = queue.pop();
            let tail = queue.pop();
            release_sender.send(()).expect("release initial worker");
            let initial_worker_exit = initial_worker.join().expect("initial worker joined");

            assert_eq!(first, 0);
            assert_eq!(
                next.expect("healthy queue")
                    .expect("second branch")
                    .canonical_index,
                1
            );
            assert_eq!(
                last.expect("healthy queue")
                    .expect("third branch")
                    .canonical_index,
                2
            );
            assert!(tail.expect("healthy queue").is_none());
            assert_eq!(initial_worker_exit, (true, false));
            assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 3);
        });
    }

    #[test]
    fn parallel_branch_queue_poison_aborts_without_claiming_work() {
        let queue = ParallelBranchQueue::new(vec![branch_task(0, 1)]);
        thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let _tasks = queue.tasks.lock().expect("initially healthy queue");
                panic!("inject branch queue poison");
            });
            assert!(worker.join().is_err());
        });
        assert!(matches!(
            queue.pop(),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_branch_queue_poisoned"
            ))
        ));
        assert!(queue.is_aborted());
        assert!(queue.pop().expect("skip poisoned aborted queue").is_none());
        assert_eq!(queue.lock_attempts.load(Ordering::Relaxed), 1);
    }

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
            observation_producer_bytes: 1_000,
            // Nested memo payload must not be added to the total again.
            standard_bag_memo_payload_bytes: 144,
            standard_bag_memo: StandardBagMemoAccounting {
                product_policy: "adaptive",
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
                product_promotion_attempts: 2,
                product_promotions: 1,
            },
            // Eleven workers share this exact request owner once.
            shared_standard_bag_request_bytes: 64,
            shared_reachability_template_bytes: 128,
            shared_observation_producer_bytes: 256,
            shared_observation_producer_peak_bytes: 512,
            incremental_import_peak_extra_bytes: 32,
            exit_snapshot_count: 1,
            max_exit_scratch_bytes: 2_100,
            returned_branch_slots_bytes: 512,
        };
        assert_eq!(component.total_bytes(), 5500);
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
        assert_eq!(total.worker_retained_bytes, 60_500);
        assert_eq!(
            total
                .worker_memory_components
                .shared_observation_producer_bytes,
            256
        );
        assert_eq!(
            total
                .worker_memory_components
                .shared_observation_producer_peak_bytes,
            512
        );
        assert_eq!(
            total
                .worker_memory_components
                .incremental_import_peak_extra_bytes,
            11 * 32
        );
        let memory = total.worker_memory_components;
        assert_eq!(memory.total_bytes(), 60_500);
        assert_eq!(memory.private_scratch_bytes(), 23_100);
        assert_eq!(memory.returned_payload_bytes(), 37_400);
        assert_eq!(memory.observation_producer_bytes, 11_000);
        assert_eq!(memory.exit_snapshot_count, 11);
        assert_eq!(memory.max_exit_scratch_bytes, 2_100);
        assert_eq!(memory.returned_branch_slots_bytes, 11 * 512);
        assert_eq!(
            total
                .worker_memory_components
                .shared_reachability_template_bytes,
            128,
        );
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
        assert_eq!(memo.product_policy, "adaptive");
        assert_eq!(memo.product_promotion_attempts, 22);
        assert_eq!(memo.product_promotions, 11);
        assert_eq!(memo.product_storage, "state-major");
        assert_eq!(memo.union_storage, "reference");
        let mut mixed = memo;
        mixed.merge(StandardBagMemoAccounting {
            product_policy: "adaptive",
            product_layout: "flat",
            product_storage: "reference",
            union_storage: "reference",
            ..Default::default()
        });
        assert_eq!(mixed.product_policy, "adaptive");
        assert_eq!(mixed.product_layout, "mixed");
        assert_eq!(mixed.product_storage, "mixed");
        assert_eq!(mixed.product_promotions, 11);
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
            static_influence:
                super::super::reachability::static_influence::StaticInfluenceMetrics {
                    queries: 3,
                    reachable: 1,
                    unreachable: 2,
                    row_steps: 4,
                    cache_hits: 1,
                    masked_queries: 5,
                    ignored_cells: 6,
                    mask_compilations: 1,
                },
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
                static_influence:
                    super::super::reachability::static_influence::StaticInfluenceMetrics {
                        queries: 5,
                        reachable: 3,
                        unreachable: 2,
                        row_steps: 7,
                        cache_hits: 2,
                        masked_queries: 9,
                        ignored_cells: 10,
                        mask_compilations: 2,
                    },
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
        assert_eq!(total.static_influence.queries, 8);
        assert_eq!(total.static_influence.reachable, 4);
        assert_eq!(total.static_influence.unreachable, 4);
        assert_eq!(total.static_influence.row_steps, 11);
        assert_eq!(total.static_influence.cache_hits, 3);
        assert_eq!(total.static_influence.masked_queries, 14);
        assert_eq!(total.static_influence.ignored_cells, 16);
        assert_eq!(total.static_influence.mask_compilations, 3);
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
