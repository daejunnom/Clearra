use std::{
    cmp::Ordering,
    fmt::Write as _,
    hash::{Hash, Hasher},
    sync::Arc,
};

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_coverage::{
    cover::{
        ExactMinimumCoverPortfolio, ExactMinimumCoverPortfolioEnumerator,
        ExactMinimumCoverPortfolioError,
    },
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_problem::{PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem};
use clearra_replay::ExactScoringExecutionBatch;
use clearra_rules::profile::rule_capability::RuleCapability;
use clearra_supply::pattern_universe::PackingPatternMembershipKind;

use crate::{
    pc_chance_coverage_evidence::{
        strict_coverage_pattern_bitset_from_words, PcChanceCoverageEvidence, PcScoreProblemEvidence,
    },
    performance::{ExecutorSearchStage, SearchStageSpan},
    resource::{
        admit_budget_bound_search_execution,
        admit_budget_bound_search_execution_under_terminal_authority, admit_search_execution,
        shared_execution_resource_capacity, DensePatternPreflight, ExecutionAdmission,
        ExecutionAdmissionPlan, ExecutionMemoryBound, WasmCpuTerminalResourceAuthority,
    },
    tiling_solution_store::{
        canonicalize_catalog_rows, pack_canonical_tiling_row_ids, read_packed_tiling_row,
        PackedTilingRows, TilingSolutionPageStore, PACKED_TILING_MAX_ROW_ID,
    },
    CoreExecutionResult, CorePathStep, PcTilingMemoryAdmissionEvidence,
};

#[cfg(feature = "parallel")]
use super::parallel_search::{self, ParallelSearchDecision, ParallelSearchOutcome};
use super::{
    buildup::{
        checked_candidate_verification_peak_upper_bound, exact_scoring_execution_graph,
        exact_scoring_execution_graph_memory_projection, verify_candidate, BuildUpWorkspace,
        CandidateWitnessMode,
    },
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    exact_collections::{ExactHashMap, ExactHashSet},
    geometry::{
        checked_target_group_build_peak_additional_bytes, GeometryAdvance, GeometryCandidate,
        GeometrySearch,
    },
    kick_profiles::replay_profile_ids,
    mix_digest,
    pc4_tablebase::{loaded_pc4_compact_tablebase, pc4_tablebase_profile_identity},
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
const TILING_SOLUTION_INITIAL_PAGE_SIZE: usize = 100;

fn canonical_minimum_cover_portfolio(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Result<Option<ExactMinimumCoverPortfolio>, ExactMinimumCoverPortfolioError> {
    let mut enumerator = ExactMinimumCoverPortfolioEnumerator::new(required, rows)?;
    enumerator.next_portfolio()
}

fn try_empty_pattern_bitset(
    pattern_count: usize,
    unavailable_reason: &'static str,
) -> Result<PatternBitSet, WasmExactSearchError> {
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let mut words = Vec::new();
    words
        .try_reserve_exact(word_count)
        .map_err(|_| WasmExactSearchError::InvalidProblem(unavailable_reason))?;
    words.resize(word_count, 0_u64);
    PatternBitSet::from_words(pattern_count, words)
        .map_err(|_| WasmExactSearchError::InvalidProblem(unavailable_reason))
}

/// Reads one worker-owned scalar only when its summary representation is both
/// unique and canonical. Distributed typed evidence is authority-bearing, so a
/// first-match lookup must never make a duplicate or alternate spelling valid.
fn exact_canonical_usize_field(result: &CoreExecutionResult, key: &str) -> Option<usize> {
    let mut matches = result
        .summary_field_entries()
        .filter_map(|(candidate, value)| (candidate == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let parsed = value.parse::<usize>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn exact_canonical_bool_field(result: &CoreExecutionResult, key: &str) -> Option<bool> {
    let mut matches = result
        .summary_field_entries()
        .filter_map(|(candidate, value)| (candidate == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn exact_canonical_u128_field(result: &CoreExecutionResult, key: &str) -> Option<u128> {
    let mut matches = result
        .summary_field_entries()
        .filter_map(|(candidate, value)| (candidate == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let parsed = value.parse::<u128>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn exact_canonical_optional_u64_field(
    result: &CoreExecutionResult,
    key: &str,
) -> Option<Option<u64>> {
    let mut matches = result
        .summary_field_entries()
        .filter_map(|(candidate, value)| (candidate == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    if value.is_empty() {
        return Some(None);
    }
    let parsed = value.parse::<u64>().ok()?;
    (parsed.to_string() == value).then_some(Some(parsed))
}

fn exact_canonical_optional_u32_field(
    result: &CoreExecutionResult,
    key: &str,
) -> Option<Option<u32>> {
    let mut matches = result
        .summary_field_entries()
        .filter_map(|(candidate, value)| (candidate == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    if value.is_empty() {
        return Some(None);
    }
    let parsed = value.parse::<u32>().ok()?;
    (parsed.to_string() == value).then_some(Some(parsed))
}

#[derive(Clone, Copy)]
struct DistributedWorkerScalarEvidence {
    packing_candidate_count: usize,
    coverage_row_count: usize,
    pattern_verified_execution_count: usize,
    build_variant_count: u128,
    count_complete: bool,
    execution_constraint_materialized: bool,
    peak_build_order_nodes: usize,
    total_build_order_nodes: usize,
    coverage_product_words: usize,
    coverage_product_states: usize,
    coverage_product_edge_checks: usize,
    realization_feasibility_states: usize,
    realization_feasibility_rejected_candidates: usize,
    peak_reachability_states: usize,
    total_reachability_states: usize,
    resource_peak_cpu_bytes: usize,
    piece_language_coverage_cache_hits: usize,
    piece_language_coverage_cache_misses: usize,
    standard_bag_symbolic_cache_hits: usize,
    standard_bag_symbolic_cache_misses: usize,
    reachability_lock_queries: usize,
    reachability_harddrop_queries: usize,
    reachability_harddrop_hits: usize,
    reachability_cache_reachable_hits: usize,
    reachability_cache_unreachable_hits: usize,
    reachability_cache_key_misses: usize,
    reachability_partial_searches: usize,
    reachability_exhaustive_searches: usize,
    representative_candidate_ordinal: Option<u64>,
    representative_candidate_id: Option<u64>,
    representative_pattern_id: Option<u32>,
    resource_truncated: bool,
}

fn exact_distributed_worker_scalar_evidence(
    result: &CoreExecutionResult,
) -> Option<DistributedWorkerScalarEvidence> {
    Some(DistributedWorkerScalarEvidence {
        packing_candidate_count: exact_canonical_usize_field(result, "packing_candidate_count")?,
        coverage_row_count: exact_canonical_usize_field(result, "coverage_row_count")?,
        pattern_verified_execution_count: exact_canonical_usize_field(
            result,
            "pattern_verified_execution_count",
        )?,
        build_variant_count: exact_canonical_u128_field(result, "build_variant_count")?,
        count_complete: exact_canonical_bool_field(result, "count_complete")?,
        execution_constraint_materialized: exact_canonical_bool_field(
            result,
            "execution_constraint_materialized",
        )?,
        peak_build_order_nodes: exact_canonical_usize_field(result, "peak_build_order_nodes")?,
        total_build_order_nodes: exact_canonical_usize_field(result, "total_build_order_nodes")?,
        coverage_product_words: exact_canonical_usize_field(result, "coverage_product_words")?,
        coverage_product_states: exact_canonical_usize_field(result, "coverage_product_states")?,
        coverage_product_edge_checks: exact_canonical_usize_field(
            result,
            "coverage_product_edge_checks",
        )?,
        realization_feasibility_states: exact_canonical_usize_field(
            result,
            "realization_feasibility_states",
        )?,
        realization_feasibility_rejected_candidates: exact_canonical_usize_field(
            result,
            "realization_feasibility_rejected_candidates",
        )?,
        peak_reachability_states: exact_canonical_usize_field(result, "peak_reachability_states")?,
        total_reachability_states: exact_canonical_usize_field(
            result,
            "total_reachability_states",
        )?,
        resource_peak_cpu_bytes: exact_canonical_usize_field(result, "resource_peak_cpu_bytes")?,
        piece_language_coverage_cache_hits: exact_canonical_usize_field(
            result,
            "piece_language_coverage_cache_hits",
        )?,
        piece_language_coverage_cache_misses: exact_canonical_usize_field(
            result,
            "piece_language_coverage_cache_misses",
        )?,
        standard_bag_symbolic_cache_hits: exact_canonical_usize_field(
            result,
            "standard_bag_symbolic_cache_hits",
        )?,
        standard_bag_symbolic_cache_misses: exact_canonical_usize_field(
            result,
            "standard_bag_symbolic_cache_misses",
        )?,
        reachability_lock_queries: exact_canonical_usize_field(
            result,
            "reachability_lock_queries",
        )?,
        reachability_harddrop_queries: exact_canonical_usize_field(
            result,
            "reachability_harddrop_queries",
        )?,
        reachability_harddrop_hits: exact_canonical_usize_field(
            result,
            "reachability_harddrop_hits",
        )?,
        reachability_cache_reachable_hits: exact_canonical_usize_field(
            result,
            "reachability_cache_reachable_hits",
        )?,
        reachability_cache_unreachable_hits: exact_canonical_usize_field(
            result,
            "reachability_cache_unreachable_hits",
        )?,
        reachability_cache_key_misses: exact_canonical_usize_field(
            result,
            "reachability_cache_key_misses",
        )?,
        reachability_partial_searches: exact_canonical_usize_field(
            result,
            "reachability_partial_searches",
        )?,
        reachability_exhaustive_searches: exact_canonical_usize_field(
            result,
            "reachability_exhaustive_searches",
        )?,
        representative_candidate_ordinal: exact_canonical_optional_u64_field(
            result,
            "representative_candidate_ordinal",
        )?,
        representative_candidate_id: exact_canonical_optional_u64_field(
            result,
            "representative_candidate_id",
        )?,
        representative_pattern_id: exact_canonical_optional_u32_field(
            result,
            "representative_pattern_id",
        )?,
        resource_truncated: exact_canonical_bool_field(result, "resource_truncated")?,
    })
}

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

#[derive(Debug, Default)]
struct DistributedTilingRootRun {
    identities: Vec<PackedTilingRows>,
    committed_chunks: Vec<DistributedTilingChunkCommit>,
    next_chunk_sequence: u32,
    complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DistributedTilingChunkCommit {
    identity_end: usize,
    root_complete: bool,
    completed_roots: usize,
    candidate_family_count: Option<u128>,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_compositions: usize,
}

impl DistributedTilingChunkCommit {
    fn from_chunk(
        chunk: &super::tiling_parallel::WasmTilingRootChunk,
        identity_end: usize,
    ) -> Self {
        Self {
            identity_end,
            root_complete: chunk.root_complete(),
            completed_roots: chunk.completed_roots(),
            candidate_family_count: chunk.candidate_family_count(),
            expanded_nodes: chunk.expanded_nodes(),
            peak_frontier: chunk.peak_frontier(),
            domain_pruned_states: chunk.domain_pruned_states(),
            hall_pruned_states: chunk.hall_pruned_states(),
            column_pruned_states: chunk.column_pruned_states(),
            component_compositions: chunk.component_compositions(),
        }
    }
}

impl DistributedTilingRootRun {
    fn absorb_chunk(
        &mut self,
        chunk: &super::tiling_parallel::WasmTilingRootChunk,
        exact_reservation: bool,
    ) -> Result<bool, WasmExactSearchError> {
        let sequence = usize::try_from(chunk.chunk_sequence()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_tiling_root_chunk_sequence_invalid")
        })?;
        if sequence < self.committed_chunks.len() {
            let begin = sequence
                .checked_sub(1)
                .map_or(0, |previous| self.committed_chunks[previous].identity_end);
            let committed = self.committed_chunks[sequence];
            let replay_matches = committed
                == DistributedTilingChunkCommit::from_chunk(chunk, committed.identity_end)
                && committed.identity_end.saturating_sub(begin) == chunk.identities().len()
                && self.identities[begin..committed.identity_end]
                    .iter()
                    .copied()
                    .eq(chunk
                        .identities()
                        .iter()
                        .map(|identity| identity.packed_rows()));
            return replay_matches
                .then_some(false)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_replay_mismatch",
                ));
        }
        if self.complete
            || chunk.chunk_sequence() != self.next_chunk_sequence
            || sequence != self.committed_chunks.len()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_tiling_root_chunk_sequence_invalid",
            ));
        }

        let begin = self.identities.len();
        let identity_end = begin.checked_add(chunk.identities().len()).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_compact_tiling_identity_storage_unavailable",
            ),
        )?;
        let mut previous = self.identities.last().copied();
        for packed in chunk.identities().iter().copied() {
            let packed_rows = packed.packed_rows();
            if !packed_rows_are_valid(&packed_rows) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_packed_tiling_identity_invalid",
                ));
            }
            if previous.is_some_and(|previous| previous >= packed_rows) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_identity_order_invalid",
                ));
            }
            previous = Some(packed_rows);
        }

        let identity_reservation = if exact_reservation {
            self.identities.try_reserve_exact(chunk.identities().len())
        } else {
            self.identities.try_reserve(chunk.identities().len())
        };
        identity_reservation.map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_compact_tiling_identity_storage_unavailable")
        })?;
        let commit_reservation = if exact_reservation {
            self.committed_chunks.try_reserve_exact(1)
        } else {
            self.committed_chunks.try_reserve(1)
        };
        commit_reservation.map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_tiling_root_commit_storage_unavailable")
        })?;
        self.identities.extend(
            chunk
                .identities()
                .iter()
                .map(|identity| identity.packed_rows()),
        );
        self.committed_chunks
            .push(DistributedTilingChunkCommit::from_chunk(
                chunk,
                identity_end,
            ));
        self.next_chunk_sequence =
            self.next_chunk_sequence
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_sequence_overflow",
                ))?;
        if chunk.root_complete() {
            self.complete = true;
        }
        Ok(true)
    }
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

fn packed_rows_from_identity(
    catalog: &GeometryCatalog,
    canonical_rank_by_source: &[u32],
    identity: StandardBoard64TilingIdentity,
) -> Option<PackedTilingRows> {
    let mut rows = [0_u32; MAX_BOARD64_PIECES];
    let count = identity.placement_count();
    if count == 0 || count > rows.len() {
        return None;
    }
    for (index, output) in rows.iter_mut().enumerate().take(count) {
        let placement = identity.placement(index)?;
        *output = catalog.skeleton_id(placement.piece(), placement.cells_mask())?;
    }
    pack_canonical_tiling_row_ids(&rows[..count], canonical_rank_by_source)
}

pub(super) fn canonical_tiling_rank_by_source(
    catalog: &GeometryCatalog,
) -> Result<Vec<u32>, WasmExactSearchError> {
    let mut catalog_rows = Vec::new();
    catalog_rows
        .try_reserve_exact(catalog.skeleton_count())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_tiling_page_catalog_unavailable")
        })?;
    for row_id in 0..catalog.skeleton_count() {
        let row = catalog.skeleton(row_id as u32);
        catalog_rows.push(PiecePlacementMask::new(row.piece, row.cells));
    }
    canonicalize_catalog_rows(catalog_rows)
        .map(|(_, ranks)| ranks)
        .map_err(WasmExactSearchError::InvalidProblem)
}

fn packed_rows_are_valid(packed_rows: &PackedTilingRows) -> bool {
    let mut count = 0;
    let mut ended = false;
    for index in 0..MAX_BOARD64_PIECES {
        let encoded = read_packed_tiling_row(packed_rows, index);
        if encoded == 0 {
            ended = true;
            continue;
        }
        if ended {
            return false;
        }
        let Ok(row_id) = u32::try_from(encoded - 1) else {
            return false;
        };
        if row_id > PACKED_TILING_MAX_ROW_ID {
            return false;
        }
        count += 1;
    }
    count != 0
}

fn canonical_multiset_order(left: [u8; 7], right: [u8; 7]) -> Ordering {
    // StandardBoard64TilingIdentity orders placements by ASCII piece name.
    // With a fixed placement count, larger counts of the first differing
    // piece therefore form the lexicographically earlier identity group.
    for index in [0_usize, 5, 6, 1, 3, 2, 4] {
        let order = right[index].cmp(&left[index]);
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

fn preflight_and_acquire_execution_resources(
    problem: &SearchProblem,
) -> Result<ExecutionAdmission, WasmExactSearchError> {
    let admission = if problem.objective().score().requested() {
        // Exact score preparation and App-side score reduction are one live
        // execution. Reserve the request's complete configured cap (or the
        // complete host cap for an unbounded request) so the terminal authority
        // can retain this lease through post-processing.
        admit_budget_bound_search_execution(problem, 1)
    } else {
        // Generic exact search keeps its established small projection.
        admit_search_execution(problem, ExecutionAdmissionPlan::exact_search())
    };
    admission.map_err(WasmExactSearchError::resource_admission)
}

fn checked_typed_problem_evidence_upper_bound(
    problem: &SearchProblem,
    external_retained_upper_bound_bytes: u128,
) -> Option<u128> {
    let evidence_owner_count = if problem
        .pc_chance_evidence_policy()
        .retains_pc_score_portfolio_v2_evidence()
    {
        2
    } else {
        1
    };
    external_retained_upper_bound_bytes.checked_mul(evidence_owner_count)
}

/// Conservative heap-layout bound for the hashbrown table behind std
/// HashMap/HashSet. Reported capacity is the usable load-limited capacity, so
/// two raw buckets per usable slot (plus one) cover the bucket array, control
/// bytes, group tail, and alignment padding without describing that
/// implementation-dependent allocation as exact.
fn checked_hash_table_retained_upper_bound(capacity: usize, entry_size: usize) -> Option<u128> {
    if capacity == 0 {
        return Some(0);
    }
    let raw_bucket_upper_bound = (capacity as u128).checked_add(1)?.checked_mul(2)?;
    raw_bucket_upper_bound
        .checked_mul((entry_size as u128).checked_add(1)?)?
        .checked_add(64)
}

// Completed results move directly out of the exact-search session.
#[allow(clippy::large_enum_variant)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchProblemRetention {
    /// The compatibility constructors cloned the caller's problem into a new
    /// Arc allocation. SearchProblem does not expose exact nested retained
    /// storage, so this ownership form cannot back terminal public-memory
    /// authority.
    SessionOwnedCloneUnaccounted,
    /// The caller shared an Arc without the request-level parent authority and
    /// conservative external retained bound required for public-memory proof.
    CallerOwnedSharedInputWithoutParentAuthority,
    /// A request-level parent owns the full physical surface from before query
    /// compilation through post-processing. This bound includes the shared
    /// SearchProblem plus all other caller-retained request/proof storage.
    ParentAuthorizedSharedInput {
        checked_external_retained_upper_bound_bytes: u128,
    },
}

pub(crate) struct WasmExactSearchSession {
    problem: Arc<SearchProblem>,
    problem_retention: SearchProblemRetention,
    catalog: Arc<GeometryCatalog>,
    geometry: GeometrySearch,
    buildup_workspace: BuildUpWorkspace,
    coverage_evaluator: CoverageProductEvaluator,
    covered_patterns: PatternBitSet,
    coverage_rows: Vec<CoverageRow>,
    coverage_rows_complete: bool,
    pc_chance_coverage_evidence_available: bool,
    distributed_minimum_cover_source_complete: bool,
    buildable_identities: ExactHashSet<TilingIdentityEntry>,
    compact_tiling_identities: Option<Vec<PackedTilingRows>>,
    distributed_tiling_root_runs: Option<Vec<DistributedTilingRootRun>>,
    tiling_canonical_rank_by_source: Option<Arc<[u32]>>,
    tiling_supply_projection_complete: bool,
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
    observation_policy_states: usize,
    observation_policy_action_checks: usize,
    observation_trie_nodes: usize,
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
    tablebase_requested: bool,
    tablebase_status: &'static str,
    tablebase_artifact_bytes: usize,
    tablebase_retained_bytes: usize,
    tablebase_payload_sha256: Option<String>,
    truncated_reason: Option<&'static str>,
    dense_pattern_preflight: DensePatternPreflight,
    _execution_admission: ExecutionAdmission,
    execution_control: ExecutionControl,
    finished: bool,
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    profile_geometry_advance_calls: usize,
}

impl WasmExactSearchSession {
    fn initialization_memory_projection_unavailable(
        execution_admission: &ExecutionAdmission,
    ) -> WasmExactSearchError {
        WasmExactSearchError::resource_admission(
            execution_admission
                .ensure_memory_bound(u128::MAX, 1)
                .expect_err("checked initialization memory projection is unavailable"),
        )
    }

    fn checked_initialization_retained_bytes(
        problem_retention: SearchProblemRetention,
        catalog: &GeometryCatalog,
        tablebase_retained_bytes: usize,
    ) -> Option<u128> {
        let external = match problem_retention {
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes,
            } => checked_external_retained_upper_bound_bytes,
            SearchProblemRetention::SessionOwnedCloneUnaccounted
            | SearchProblemRetention::CallerOwnedSharedInputWithoutParentAuthority => 0,
        };
        external
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add(core::mem::size_of::<GeometryCatalog>() as u128)?
            .checked_add(catalog.retained_bytes() as u128)?
            .checked_add(tablebase_retained_bytes as u128)
    }

    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, false)
    }

    pub(crate) fn new_shared(problem: Arc<SearchProblem>) -> Result<Self, WasmExactSearchError> {
        Self::new_with_problem(
            problem,
            false,
            SearchProblemRetention::CallerOwnedSharedInputWithoutParentAuthority,
        )
    }

    pub(crate) fn new_shared_under_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        Self::validate_tiling_session_problem(problem.as_ref())?;
        if !(problem.objective().score().requested()
            || problem.output_policy() == SearchOutputPolicy::TilingOnly
                && problem.objective().kind() == ObjectiveKind::Tiling)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_terminal_authority_requires_typed_score_or_tiling",
            ));
        }
        let execution_admission = admit_budget_bound_search_execution_under_terminal_authority(
            problem.as_ref(),
            checked_external_retained_upper_bound_bytes,
            authority,
            if cfg!(target_family = "wasm") || !problem.objective().score().requested() {
                1
            } else {
                problem.backend_policy().workers()
            },
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_preacquired_execution_admission(
            problem,
            false,
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes,
            },
            execution_admission,
        )
    }

    /// Builds the coordinator-side geometry owner for a typed tiling request
    /// under the same request-scoped terminal authority that will validate the
    /// final pageable family. This is the distributed counterpart of
    /// `new_shared_under_authority`: geometry is exported as root tasks, while
    /// the finalizer retains the parent-authorized memory evidence.
    pub fn new_shared_external_geometry_under_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        Self::validate_tiling_session_problem(problem.as_ref())?;
        if problem.output_policy() != SearchOutputPolicy::TilingOnly
            || problem.objective().kind() != ObjectiveKind::Tiling
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_terminal_authority_requires_typed_tiling_geometry",
            ));
        }
        let execution_admission = admit_budget_bound_search_execution_under_terminal_authority(
            problem.as_ref(),
            checked_external_retained_upper_bound_bytes,
            authority,
            1,
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_preacquired_execution_admission(
            problem,
            true,
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes,
            },
            execution_admission,
        )
    }

    /// Builds a worker-side score verifier under the request-scoped terminal
    /// authority. The geometry stream is supplied by the coordinator, so this
    /// session owns only verification state and a compute child of the parent
    /// lease; it never acquires a second global memory surface.
    pub(crate) fn new_shared_external_verifier_under_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        Self::validate_tiling_session_problem(problem.as_ref())?;
        if !problem.objective().score().requested() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_terminal_authority_requires_typed_score_verifier",
            ));
        }
        let execution_admission = admit_budget_bound_search_execution_under_terminal_authority(
            problem.as_ref(),
            checked_external_retained_upper_bound_bytes,
            authority,
            1,
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_preacquired_execution_admission(
            problem,
            true,
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes,
            },
            execution_admission,
        )
    }

    pub fn new_external_geometry(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, true)
    }

    pub(super) fn new_external_geometry_for_required_cells_on_board(
        problem: &SearchProblem,
        initial_board: u64,
        required_cells: u64,
    ) -> Result<Self, WasmExactSearchError> {
        Self::validate_tiling_session_problem(problem)?;
        let execution_admission = preflight_and_acquire_execution_resources(problem)?;
        let dense_pattern_preflight = execution_admission.dense_preflight;
        let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionCatalogCompile);
        let catalog = Arc::new(GeometryCatalog::compile_for_required_cells_on_board(
            problem,
            initial_board,
            required_cells,
        )?);
        catalog_span.finish(catalog.skeleton_count() as u64);
        Self::new_with_external_geometry_catalog(
            Arc::new(problem.clone()),
            SearchProblemRetention::SessionOwnedCloneUnaccounted,
            true,
            catalog,
            dense_pattern_preflight,
            execution_admission,
        )
    }

    // Exposed to the distributed observer path on targets that report geometry telemetry.
    #[allow(dead_code)]
    pub(super) fn geometry_expanded_nodes(&self) -> usize {
        self.geometry.expanded_nodes()
    }

    pub(super) fn geometry_target_preparation_pending(&self) -> bool {
        self.geometry.target_preparation_pending()
    }

    /// Advances only the deferred target/index preparation owned by an
    /// external-geometry verifier or WebGPU adapter. A caller must yield after
    /// every `false` result; candidates are not admissible until this returns
    /// `true`.
    pub(super) fn advance_external_geometry_preparation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.execution_control = control.clone();
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !self.geometry.target_preparation_pending() {
            return Ok(true);
        }
        let advance = if matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            self.ensure_session_memory_bound(0)?;
            let limit = self
                .checked_geometry_retained_limit(0)?
                .expect("parent-authorized geometry has a retained limit");
            self.geometry
                .advance_with_retained_limit(&self.catalog, limit)
        } else {
            self.geometry.advance(&self.catalog)
        };
        match advance {
            GeometryAdvance::Pending => Ok(!self.geometry.target_preparation_pending()),
            GeometryAdvance::ResourceIncomplete(reason) => {
                Err(WasmExactSearchError::InvalidProblem(reason))
            }
            GeometryAdvance::Candidate(_) | GeometryAdvance::Complete => Err(
                WasmExactSearchError::InvalidProblem("external_geometry_preparation_state_invalid"),
            ),
        }
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
        Self::new_with_problem(
            Arc::new(problem.clone()),
            external_geometry,
            SearchProblemRetention::SessionOwnedCloneUnaccounted,
        )
    }

    fn new_with_problem(
        problem: Arc<SearchProblem>,
        external_geometry: bool,
        problem_retention: SearchProblemRetention,
    ) -> Result<Self, WasmExactSearchError> {
        Self::validate_tiling_session_problem(problem.as_ref())?;
        let execution_admission = preflight_and_acquire_execution_resources(problem.as_ref())?;
        Self::new_with_preacquired_execution_admission(
            problem,
            external_geometry,
            problem_retention,
            execution_admission,
        )
    }

    fn new_with_preacquired_execution_admission(
        problem: Arc<SearchProblem>,
        external_geometry: bool,
        problem_retention: SearchProblemRetention,
        execution_admission: ExecutionAdmission,
    ) -> Result<Self, WasmExactSearchError> {
        let dense_pattern_preflight = execution_admission.dense_preflight;
        if let SearchProblemRetention::ParentAuthorizedSharedInput {
            checked_external_retained_upper_bound_bytes,
        } = problem_retention
        {
            let catalog_peak = GeometryCatalog::checked_compile_peak_upper_bound(problem.as_ref())
                .ok_or_else(|| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?;
            let retained_before_catalog = checked_external_retained_upper_bound_bytes
                .checked_add(core::mem::size_of::<Self>() as u128)
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of::<GeometryCatalog>() as u128)
                })
                .ok_or_else(|| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?;
            execution_admission
                .ensure_memory_bound(retained_before_catalog, catalog_peak)
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionCatalogCompile);
        let catalog = Arc::new(GeometryCatalog::compile(problem.as_ref())?);
        catalog_span.finish(catalog.skeleton_count() as u64);
        Self::new_with_external_geometry_catalog(
            problem,
            problem_retention,
            external_geometry,
            catalog,
            dense_pattern_preflight,
            execution_admission,
        )
    }

    fn validate_tiling_session_problem(
        problem: &SearchProblem,
    ) -> Result<(), WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
        if problem.objective().kind() == ObjectiveKind::Tiling
            && (problem.objective().score().requested()
                || problem.objective().execution_constraints().requested()
                || problem.solution_probability_policy().requested()
                || problem
                    .queue_observation_policy()
                    .requires_observation_policy()
                || problem.backend_policy().tablebase_requested()
                || problem.backend_policy().precompute_build_dependencies())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_tiling_only_option_conflict",
            ));
        }
        Ok(())
    }

    fn new_with_external_geometry_catalog(
        problem: Arc<SearchProblem>,
        problem_retention: SearchProblemRetention,
        external_geometry: bool,
        catalog: Arc<GeometryCatalog>,
        dense_pattern_preflight: DensePatternPreflight,
        execution_admission: ExecutionAdmission,
    ) -> Result<Self, WasmExactSearchError> {
        let tablebase_requested = problem.backend_policy().tablebase_requested();
        let loaded_tablebase = tablebase_requested
            .then(loaded_pc4_compact_tablebase)
            .flatten();
        let tablebase_artifact_bytes = loaded_tablebase
            .as_ref()
            .map_or(0, |loaded| loaded.artifact_bytes());
        let tablebase_retained_bytes = loaded_tablebase
            .as_ref()
            .map_or(0, |loaded| loaded.retained_bytes());
        let tablebase_payload_sha256 = loaded_tablebase
            .as_ref()
            .map(|loaded| loaded.payload_sha256_hex());
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
        let retained_before_family = Self::checked_initialization_retained_bytes(
            problem_retention,
            catalog.as_ref(),
            tablebase_retained_bytes,
        )
        .ok_or_else(|| Self::initialization_memory_projection_unavailable(&execution_admission))?;
        let multiset_family = if matches!(
            problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            universe
                .packing_multiset_family_for_execution_with_workers_and_memory_limit(
                    target_piece_count,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    super::packing_hold_projection(problem.as_ref()),
                    1,
                    retained_before_family,
                    execution_admission.memory_cap_bytes(),
                )
                .map_err(|_| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?
        } else {
            universe.packing_multiset_family_for_execution(
                target_piece_count,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                super::packing_hold_projection(problem.as_ref()),
            )
        };
        if multiset_family.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_supply_has_no_reachable_piece_multiset",
            ));
        }
        let expected_tablebase_profile =
            pc4_tablebase_profile_identity(problem.as_ref(), catalog.identity_digest());
        let (tablebase, tablebase_status) = match loaded_tablebase.as_ref() {
            None if tablebase_requested => (None, "unavailable"),
            None => (None, "disabled"),
            Some(_) if external_geometry => (None, "unsupported-backend"),
            Some(_)
                if catalog.width() != 10
                    || catalog.height() != 4
                    || catalog.initial_board() != 0
                    || catalog.required_cells() != (1_u64 << 40) - 1 =>
            {
                (None, "unsupported-request")
            }
            Some(loaded)
                if loaded.catalog_identity() != catalog.identity_digest()
                    || loaded.compiler_identity() != expected_tablebase_profile =>
            {
                (None, "profile-mismatch")
            }
            Some(loaded) => (Some(Arc::clone(loaded)), "connected-exact-dead-index"),
        };
        supply_span.finish(universe.pattern_count() as u64);
        let family_retained_bytes = multiset_family.checked_retained_bytes().ok_or_else(|| {
            Self::initialization_memory_projection_unavailable(&execution_admission)
        })?;
        let retained_before_covered_patterns = retained_before_family
            .checked_add(family_retained_bytes)
            .ok_or_else(|| {
                Self::initialization_memory_projection_unavailable(&execution_admission)
            })?;
        if matches!(
            problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            let bitset_peak = PatternBitSet::checked_all_projection(universe.pattern_count())
                .map(|projection| projection.constructor_peak_bytes)
                .ok_or_else(|| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?;
            execution_admission
                .ensure_memory_bound(retained_before_covered_patterns, bitset_peak)
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let covered_patterns = if matches!(
            problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            try_empty_pattern_bitset(
                universe.pattern_count(),
                "wasm_covered_pattern_storage_unavailable",
            )?
        } else {
            PatternBitSet::new(universe.pattern_count())
        };
        let retained_before_geometry = retained_before_covered_patterns
            .checked_add(
                covered_patterns
                    .checked_storage_retained_bytes()
                    .ok_or_else(|| {
                        Self::initialization_memory_projection_unavailable(&execution_admission)
                    })?,
            )
            .ok_or_else(|| {
                Self::initialization_memory_projection_unavailable(&execution_admission)
            })?;
        let omits_geometry_pattern_indices = problem.objective().kind() == ObjectiveKind::Tiling
            || (problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountUnique
                && StandardBagCoverage::supports(universe, problem.initial_hold()));
        let geometry_span = SearchStageSpan::begin(ExecutorSearchStage::WasmSessionGeometryPrepare);
        let parent_authorized = matches!(
            problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        );
        if parent_authorized {
            let target_peak = checked_target_group_build_peak_additional_bytes(
                universe,
                &multiset_family,
                !omits_geometry_pattern_indices,
            )
            .ok_or_else(|| {
                Self::initialization_memory_projection_unavailable(&execution_admission)
            })?;
            execution_admission
                .ensure_memory_bound(retained_before_geometry, target_peak)
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let geometry = if external_geometry {
            GeometrySearch::external_deferred(
                universe,
                &multiset_family,
                !omits_geometry_pattern_indices,
                parent_authorized.then_some((
                    retained_before_geometry,
                    execution_admission.memory_cap_bytes(),
                )),
            )?
        } else if let Some(tablebase) = tablebase {
            if parent_authorized {
                GeometrySearch::new_with_tablebase_and_memory_limit(
                    universe,
                    &multiset_family,
                    catalog.required_cells(),
                    !omits_geometry_pattern_indices,
                    tablebase,
                    retained_before_geometry,
                    execution_admission.memory_cap_bytes(),
                )?
            } else {
                GeometrySearch::new_with_tablebase(
                    universe,
                    &multiset_family,
                    catalog.required_cells(),
                    !omits_geometry_pattern_indices,
                    tablebase,
                )?
            }
        } else {
            if parent_authorized {
                GeometrySearch::new_with_memory_limit(
                    universe,
                    &multiset_family,
                    catalog.required_cells(),
                    !omits_geometry_pattern_indices,
                    retained_before_geometry,
                    execution_admission.memory_cap_bytes(),
                )?
            } else {
                GeometrySearch::new(
                    universe,
                    &multiset_family,
                    catalog.required_cells(),
                    !omits_geometry_pattern_indices,
                )?
            }
        };
        geometry_span.finish(geometry.targets().map_or(0, |targets| targets.len()) as u64);
        let peak_cpu_bytes = catalog
            .retained_bytes()
            .saturating_add(geometry.retained_bytes())
            .saturating_add(tablebase_retained_bytes);
        let compact_tiling_identity_supported = problem.objective().kind() == ObjectiveKind::Tiling
            && catalog.skeleton_count() <= PACKED_TILING_MAX_ROW_ID as usize + 1;
        if problem.output_policy() == SearchOutputPolicy::TilingOnly
            && !compact_tiling_identity_supported
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_pc_tiling_compact_identity_unavailable",
            ));
        }
        if parent_authorized && problem.output_policy() == SearchOutputPolicy::TilingOnly {
            let retained_before_rank = retained_before_geometry
                .checked_add(geometry.retained_bytes() as u128)
                .ok_or_else(|| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?;
            let rank_peak =
                TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
                    catalog.skeleton_count(),
                    0,
                    0,
                )
                .ok_or_else(|| {
                    Self::initialization_memory_projection_unavailable(&execution_admission)
                })?;
            execution_admission
                .ensure_memory_bound(retained_before_rank, rank_peak)
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let tiling_canonical_rank_by_source = compact_tiling_identity_supported
            .then(|| canonical_tiling_rank_by_source(&catalog))
            .transpose()?
            .map(Arc::from);
        let tiling_supply_projection_complete = universe.complete()
            || multiset_family.membership_kind()
                == PackingPatternMembershipKind::ExactSymbolicStandardBag;
        let retain_pc_chance_coverage_evidence = problem
            .pc_chance_evidence_policy()
            .retains_pc_coverage_evidence();
        let session = Self {
            problem: Arc::clone(&problem),
            problem_retention,
            catalog,
            geometry,
            buildup_workspace: BuildUpWorkspace::default(),
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns,
            coverage_rows: Vec::new(),
            coverage_rows_complete: retain_pc_chance_coverage_evidence
                && !problem
                    .queue_observation_policy()
                    .requires_observation_policy(),
            pc_chance_coverage_evidence_available: retain_pc_chance_coverage_evidence,
            distributed_minimum_cover_source_complete: false,
            buildable_identities: ExactHashSet::default(),
            compact_tiling_identities: compact_tiling_identity_supported.then(Vec::new),
            distributed_tiling_root_runs: None,
            tiling_canonical_rank_by_source,
            tiling_supply_projection_complete,
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
            observation_policy_states: 0,
            observation_policy_action_checks: 0,
            observation_trie_nodes: 0,
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
            tablebase_requested,
            tablebase_status,
            tablebase_artifact_bytes,
            tablebase_retained_bytes,
            tablebase_payload_sha256,
            truncated_reason: None,
            dense_pattern_preflight,
            _execution_admission: execution_admission,
            execution_control: ExecutionControl::default(),
            finished: false,
            #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
            profile_geometry_advance_calls: 0,
        };
        if matches!(
            session.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            // The external bound was checked before catalog construction; now
            // include the actual retained catalog/geometry/session stores
            // before exposing the successfully constructed child session.
            session.ensure_session_memory_bound(0)?;
        }
        Ok(session)
    }

    #[cfg(feature = "webgpu-search")]
    pub(super) fn catalog(&self) -> Arc<GeometryCatalog> {
        Arc::clone(&self.catalog)
    }

    #[cfg(feature = "webgpu-search")]
    pub(super) fn geometry_targets(&self) -> Option<&[super::geometry::TargetGroup]> {
        self.geometry.targets()
    }

    // Fields mirror the public backend report and are written atomically as one transition.
    #[allow(clippy::too_many_arguments)]
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
        self.execution_control = control.clone();
        if worker_count <= 1 {
            return Ok(None);
        }
        if self
            .problem
            .queue_observation_policy()
            .requires_observation_policy()
        {
            self.parallel_decision_reason =
                "visible-seven-policy-requires-global-language-finalizer";
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
        mut outcome: ParallelSearchOutcome,
    ) -> Result<(), WasmExactSearchError> {
        let canonical_minimum_source_complete =
            self.validate_parallel_minimum_cover_source(&mut outcome)?;
        self.geometry = outcome.geometry;
        self.covered_patterns
            .union_with(&outcome.covered_patterns)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_parallel_coverage_universe_mismatch")
            })?;
        if self.problem.objective().kind() != ObjectiveKind::Tiling {
            self.coverage_rows_complete &= canonical_minimum_source_complete;
        }
        // Native family workers retain complete per-solution rows, not the
        // serial verifier's raw candidate row log. Reuse the same validated
        // canonical row reconstruction as the distributed minimum source.
        self.distributed_minimum_cover_source_complete |= canonical_minimum_source_complete;
        for identity in outcome.buildable_identities {
            if self.problem.objective().kind() == ObjectiveKind::Tiling {
                self.insert_tiling_result_identity(identity)?;
                continue;
            }
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
                .saturating_add(self.tablebase_retained_bytes)
                .saturating_add(self.parallel_worker_retained_bytes)
                .saturating_add(self.solution_identity_retained_bytes()),
        );
        if let Some(reason) = outcome.truncated_reason {
            self.mark_truncated(reason);
        }
        if self.memory_budget_exceeded() {
            self.mark_truncated("memory_budget_exceeded");
        }
        Ok(())
    }

    #[cfg(feature = "parallel")]
    fn validate_parallel_minimum_cover_source(
        &self,
        outcome: &mut ParallelSearchOutcome,
    ) -> Result<bool, WasmExactSearchError> {
        if self.problem.objective().kind() != ObjectiveKind::MinimumCover
            || !self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_minimum_cover_v2_evidence()
            || self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_score_portfolio_v2_evidence()
            || !outcome.count_complete
            || outcome.truncated_reason.is_some()
        {
            return Ok(false);
        }
        let pattern_count = self
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_source_not_materialized",
            ))?
            .pattern_count();
        outcome.buildable_identities.sort_unstable();
        outcome.buildable_identities.dedup();
        if outcome.solution_coverage.len() != outcome.buildable_identities.len()
            || outcome.covered_patterns.pattern_count() != pattern_count
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_pc_minimum_source_incomplete",
            ));
        }
        for (entry, identity) in outcome
            .solution_coverage
            .iter()
            .zip(&outcome.buildable_identities)
        {
            if entry.identity() != *identity
                || entry.covered_patterns().pattern_count() != pattern_count
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_pc_minimum_source_identity_mismatch",
                ));
            }
        }
        // Validate the union without allocating a second universe or filling
        // sparse rows' lazy dense caches while the complete source is live.
        for word_index in 0..outcome.covered_patterns.word_count() {
            let source_word = outcome.solution_coverage.iter().fold(0_u64, |word, entry| {
                word | entry.covered_patterns().word_at(word_index)
            });
            if source_word != outcome.covered_patterns.word_at(word_index) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_pc_minimum_source_coverage_mismatch",
                ));
            }
        }
        Ok(true)
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        self.execution_control = control.clone();
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
            let geometry_advance = if matches!(
                self.problem_retention,
                SearchProblemRetention::ParentAuthorizedSharedInput { .. }
            ) {
                // Geometry may return one owned row-id vector. Keep that live
                // candidate inside the same request surface while the compiler
                // or traversal frontier grows.
                let checked_candidate_bytes = (core::mem::size_of::<GeometryCandidate>() as u128)
                    .checked_add(
                        (MAX_BOARD64_PIECES as u128)
                            .checked_mul(core::mem::size_of::<u32>() as u128)
                            .ok_or_else(|| {
                                self.memory_projection_unavailable(
                                    "geometry candidate projection overflow",
                                )
                            })?,
                    )
                    .ok_or_else(|| {
                        self.memory_projection_unavailable("geometry candidate projection overflow")
                    })?;
                self.ensure_session_memory_bound(checked_candidate_bytes)?;
                let limit = self
                    .checked_geometry_retained_limit(checked_candidate_bytes)?
                    .expect("parent-authorized geometry has a retained limit");
                self.geometry
                    .advance_with_retained_limit(&self.catalog, limit)
            } else {
                self.geometry.advance(&self.catalog)
            };
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

    pub(super) fn distributed_tiling_root_order(&self) -> Result<Vec<u32>, WasmExactSearchError> {
        let targets = self
            .geometry
            .targets()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_tiling_root_targets_unavailable",
            ))?;
        self.preadmit_canonical_tiling_vec_growth::<u32>(0, 0, targets.len())?;
        let mut roots = Vec::new();
        roots.try_reserve_exact(targets.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_tiling_root_order_storage_unavailable")
        })?;
        for index in 0..targets.len() {
            roots.push(u32::try_from(index).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_tiling_root_index_overflow")
            })?);
        }
        roots.sort_unstable_by(|left, right| {
            canonical_multiset_order(
                targets[*left as usize].key.counts(),
                targets[*right as usize].key.counts(),
            )
        });
        self.validate_canonical_tiling_allocation()?;
        Ok(roots)
    }

    pub(super) fn prepare_distributed_tiling_root_runs(
        &mut self,
        root_count: usize,
    ) -> Result<(), WasmExactSearchError> {
        let compact =
            self.compact_tiling_identities
                .as_ref()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_unavailable",
                ))?;
        if !compact.is_empty() || self.distributed_tiling_root_runs.is_some() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_tiling_root_merge_already_started",
            ));
        }
        self.preadmit_canonical_tiling_vec_growth::<DistributedTilingRootRun>(0, 0, root_count)?;
        let mut runs = Vec::new();
        runs.try_reserve_exact(root_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_tiling_root_runs_storage_unavailable")
        })?;
        runs.resize_with(root_count, DistributedTilingRootRun::default);
        self.distributed_tiling_root_runs = Some(runs);
        self.validate_canonical_tiling_allocation()?;
        Ok(())
    }

    fn solution_identity_count(&self) -> usize {
        self.distributed_tiling_root_runs.as_ref().map_or_else(
            || {
                self.compact_tiling_identities
                    .as_ref()
                    .map_or_else(|| self.buildable_identities.len(), Vec::len)
            },
            |runs| runs.iter().map(|run| run.identities.len()).sum(),
        )
    }

    fn solution_identity_retained_bytes(&self) -> usize {
        self.checked_solution_identity_retained_bytes()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(usize::MAX)
    }

    fn checked_solution_identity_retained_bytes(&self) -> Option<u128> {
        let mut bytes = checked_hash_table_retained_upper_bound(
            self.buildable_identities.capacity(),
            core::mem::size_of::<TilingIdentityEntry>(),
        )?;
        if let Some(identities) = &self.compact_tiling_identities {
            bytes = bytes.checked_add(
                (identities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PackedTilingRows>() as u128)?,
            )?;
        }
        if let Some(runs) = &self.distributed_tiling_root_runs {
            bytes = bytes.checked_add(
                (runs.capacity() as u128)
                    .checked_mul(core::mem::size_of::<DistributedTilingRootRun>() as u128)?,
            )?;
            for run in runs {
                bytes = bytes.checked_add(
                    (run.identities.capacity() as u128)
                        .checked_mul(core::mem::size_of::<PackedTilingRows>() as u128)?,
                )?;
                bytes = bytes
                    .checked_add((run.committed_chunks.capacity() as u128).checked_mul(
                        core::mem::size_of::<DistributedTilingChunkCommit>() as u128,
                    )?)?;
            }
        }
        Some(bytes)
    }

    fn coverage_rows_retained_bytes(&self) -> usize {
        self.checked_coverage_rows_retained_bytes()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(usize::MAX)
    }

    fn checked_coverage_rows_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (self.coverage_rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<CoverageRow>() as u128)?;
        for row in &self.coverage_rows {
            bytes = bytes.checked_add(row.coverage_bits().retained_bytes() as u128)?;
        }
        Some(bytes)
    }

    /// Checked executor-owned retained storage. Inline fields are counted once
    /// by `size_of::<Self>()`; every executor-owned heap store is then added
    /// from its owning capacity. Caller-retained request storage, including a
    /// shared SearchProblem pointee, is added separately only when a parent
    /// resource authority supplied its conservative checked upper bound.
    /// Peak-only worker metrics are deliberately not counted because their
    /// workers have already been joined and dropped.
    fn checked_executor_retained_bytes(&self) -> Option<u128> {
        let mut bytes = core::mem::size_of::<Self>() as u128;
        // GeometryCatalog is an executor-owned Arc pointee and therefore is
        // not part of the session's inline size.
        bytes = bytes.checked_add(core::mem::size_of::<GeometryCatalog>() as u128)?;
        for retained in [
            self.catalog.retained_bytes(),
            self.geometry.retained_bytes(),
            self.buildup_workspace.retained_bytes(),
            self.coverage_evaluator.retained_bytes(),
            self.covered_patterns.retained_bytes(),
            self.tablebase_retained_bytes,
        ] {
            bytes = bytes.checked_add(retained as u128)?;
        }
        bytes = bytes.checked_add(self.checked_solution_identity_retained_bytes()?)?;
        bytes = bytes.checked_add(self.checked_coverage_rows_retained_bytes()?)?;
        if let Some(rank) = &self.tiling_canonical_rank_by_source {
            bytes = bytes.checked_add(
                (rank.len() as u128).checked_mul(core::mem::size_of::<u32>() as u128)?,
            )?;
        }
        if let Some(coverage) = &self.solution_coverage {
            bytes = bytes.checked_add(checked_hash_table_retained_upper_bound(
                coverage.capacity(),
                core::mem::size_of::<StandardBoard64TilingIdentity>()
                    + core::mem::size_of::<PatternBitSet>(),
            )?)?;
            bytes = bytes.checked_add(self.solution_coverage_bytes as u128)?;
        }
        bytes = bytes.checked_add(
            (self.representative_path.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
        )?;
        for value in [
            self.gpu_adapter_name.as_ref(),
            self.gpu_adapter_backend.as_ref(),
            self.gpu_shader_hash.as_ref(),
            self.tablebase_payload_sha256.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }

    fn checked_external_retained_upper_bound_bytes(&self) -> Option<u128> {
        match self.problem_retention {
            SearchProblemRetention::ParentAuthorizedSharedInput {
                checked_external_retained_upper_bound_bytes,
            } => Some(checked_external_retained_upper_bound_bytes),
            SearchProblemRetention::SessionOwnedCloneUnaccounted
            | SearchProblemRetention::CallerOwnedSharedInputWithoutParentAuthority => None,
        }
    }

    /// Terminal public-memory authority requires the request-level parent
    /// lease and the caller's checked conservative retained upper bound.
    /// Clone-owned and unparented shared compatibility paths both return None
    /// and therefore fail closed when asked to validate a public result.
    fn checked_session_retained_bytes(&self) -> Option<u128> {
        self.checked_executor_retained_bytes()?
            .checked_add(self.checked_external_retained_upper_bound_bytes()?)
    }

    fn checked_allocation_guard_retained_bytes(&self) -> Option<u128> {
        let executor = self.checked_executor_retained_bytes()?;
        match self.checked_external_retained_upper_bound_bytes() {
            Some(external) => executor.checked_add(external),
            None => Some(executor),
        }
    }

    fn memory_projection_unavailable(&self, message: &'static str) -> WasmExactSearchError {
        WasmExactSearchError::resource_admission(
            self._execution_admission
                .ensure_memory_bound(u128::MAX, 1)
                .expect_err(message),
        )
    }

    fn ensure_session_memory_bound(
        &self,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let retained = self
            .checked_allocation_guard_retained_bytes()
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked executor retained-byte overflow is unavailable",
                )
            })?;
        self._execution_admission
            .ensure_memory_bound(retained, checked_future_bytes)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn ensure_score_session_memory_bound(
        &self,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        if !self.problem.objective().score().requested() {
            // Exact execution-constraint evidence shares the scoring graph
            // builder, but generic exact admission deliberately retains its
            // historical one-bitset projection. Only a score request owns the
            // complete configured/host-cap lease against which these checked
            // allocation projections are authoritative.
            return Ok(());
        }
        self.ensure_session_memory_bound(checked_future_bytes)
    }

    fn canonical_tiling_terminal_authorized(&self) -> bool {
        self.problem.output_policy() == SearchOutputPolicy::TilingOnly
            && self.problem.objective().kind() == ObjectiveKind::Tiling
            && matches!(
                self.problem_retention,
                SearchProblemRetention::ParentAuthorizedSharedInput { .. }
            )
    }

    fn preadmit_canonical_tiling_vec_growth<T>(
        &self,
        len: usize,
        capacity: usize,
        additional: usize,
    ) -> Result<(), WasmExactSearchError> {
        if !self.canonical_tiling_terminal_authorized() {
            return Ok(());
        }
        let required = len.checked_add(additional).ok_or_else(|| {
            self.memory_projection_unavailable("pc tiling vector growth projection overflow")
        })?;
        if required <= capacity {
            return Ok(());
        }
        let future = (required as u128)
            .checked_mul(core::mem::size_of::<T>() as u128)
            .ok_or_else(|| {
                self.memory_projection_unavailable("pc tiling vector growth projection overflow")
            })?;
        self.ensure_session_memory_bound(future)
    }

    fn validate_canonical_tiling_allocation(&self) -> Result<(), WasmExactSearchError> {
        if self.canonical_tiling_terminal_authorized() {
            self.ensure_session_memory_bound(0)?;
        }
        Ok(())
    }

    fn checked_geometry_retained_limit(
        &self,
        checked_candidate_bytes: u128,
    ) -> Result<Option<u128>, WasmExactSearchError> {
        if !matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            return Ok(None);
        }
        let retained = self
            .checked_allocation_guard_retained_bytes()
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked geometry retained-byte projection is unavailable",
                )
            })?;
        let geometry = self.geometry.retained_bytes() as u128;
        let base = retained.checked_sub(geometry).ok_or_else(|| {
            self.memory_projection_unavailable(
                "checked geometry retained-byte projection is inconsistent",
            )
        })?;
        let limit = self
            ._execution_admission
            .memory_cap_bytes()
            .checked_sub(base)
            .and_then(|bytes| bytes.checked_sub(checked_candidate_bytes))
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "geometry candidate exceeds the admitted memory surface",
                )
            })?;
        Ok(Some(limit))
    }

    // Local domain admission results are mapped into the boxed public search error below.
    #[allow(clippy::result_large_err)]
    pub(crate) fn validate_external_result_memory_with_future(
        &self,
        external_retained_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let checked_external_and_future = external_retained_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked distributed ingress memory projection overflow is unavailable",
                )
            })?;
        if matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            return self.ensure_session_memory_bound(checked_external_and_future);
        }

        // Ordinary exact-search admission reserves the algorithm's projected
        // dense state, which is intentionally much smaller than the bounded
        // caller-owned wire + decoded-result ingress surface. Validate that
        // transient whole-live surface against the request cap (or the finite
        // host cap when no explicit cap was requested), while retaining the
        // original execution lease as the algorithm allocation authority.
        let cap_bytes = match self.problem.backend_request().max_memory_mib() {
            Some(mib) => u128::from(mib).checked_mul(1024 * 1024).ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked distributed ingress memory cap overflow is unavailable",
                )
            })?,
            None => u128::from(shared_execution_resource_capacity().memory_bytes),
        };
        let retained = self
            .checked_allocation_guard_retained_bytes()
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked distributed ingress retained-byte overflow is unavailable",
                )
            })?;
        ExecutionMemoryBound::unbounded_for_problem(self.problem.as_ref())
            .and_then(|bound| bound.with_cap(cap_bytes))
            .and_then(|bound| bound.ensure(retained, checked_external_and_future))
            .map_err(WasmExactSearchError::resource_admission)
    }

    pub(crate) fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let retained = self.checked_session_retained_bytes().ok_or_else(|| {
            self.memory_projection_unavailable(
                "terminal authority requires a parent lease and checked external retained bound",
            )
        })?;
        let result_bytes = result.checked_resource_retained_bytes().ok_or_else(|| {
            self.memory_projection_unavailable(
                "checked public-result retained-byte overflow is unavailable",
            )
        })?;
        let future = result_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "checked terminal memory projection overflow is unavailable",
                )
            })?;
        self._execution_admission
            .ensure_memory_bound(retained, future)
            .map_err(WasmExactSearchError::resource_admission)
    }

    #[cfg(test)]
    pub(crate) const fn admitted_memory_cap_bytes(&self) -> u128 {
        self._execution_admission.memory_cap_bytes()
    }

    #[cfg(test)]
    pub(crate) fn shares_problem_arc(&self, problem: &Arc<SearchProblem>) -> bool {
        Arc::ptr_eq(&self.problem, problem)
    }

    #[cfg(test)]
    pub(crate) fn checked_terminal_retained_bytes(
        &self,
        result: &CoreExecutionResult,
    ) -> Option<u128> {
        self.checked_session_retained_bytes()?
            .checked_add(result.checked_resource_retained_bytes()?)
    }

    fn insert_tiling_candidate_identity(
        &mut self,
        candidate: &GeometryCandidate,
    ) -> Result<(), WasmExactSearchError> {
        if self.compact_tiling_identities.is_some() {
            let canonical_rank_by_source = self.tiling_canonical_rank_by_source.as_deref().ok_or(
                WasmExactSearchError::InvalidProblem("wasm_tiling_canonical_rank_unavailable"),
            )?;
            let packed_rows =
                pack_canonical_tiling_row_ids(candidate.row_ids(), canonical_rank_by_source)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_compact_tiling_identity_invalid",
                    ))?;
            let (len, capacity) = self
                .compact_tiling_identities
                .as_ref()
                .map(|identities| (identities.len(), identities.capacity()))
                .expect("compact tiling identity storage was checked");
            self.preadmit_canonical_tiling_vec_growth::<PackedTilingRows>(len, capacity, 1)?;
            let terminal_authorized = self.canonical_tiling_terminal_authorized();
            let identities = self.compact_tiling_identities.as_mut().ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_storage_unavailable",
                ),
            )?;
            let reservation = if terminal_authorized {
                identities.try_reserve_exact(1)
            } else {
                identities.try_reserve(1)
            };
            reservation.map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_storage_unavailable",
                )
            })?;
            identities.push(packed_rows);
            self.validate_canonical_tiling_allocation()?;
            return Ok(());
        }
        self.insert_standard_solution_identity(candidate.identity)
    }

    fn insert_tiling_result_identity(
        &mut self,
        identity: StandardBoard64TilingIdentity,
    ) -> Result<(), WasmExactSearchError> {
        if self.compact_tiling_identities.is_some() {
            let canonical_rank_by_source = self.tiling_canonical_rank_by_source.as_deref().ok_or(
                WasmExactSearchError::InvalidProblem("wasm_tiling_canonical_rank_unavailable"),
            )?;
            let packed_rows =
                packed_rows_from_identity(&self.catalog, canonical_rank_by_source, identity)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_compact_tiling_result_identity_invalid",
                    ))?;
            let (len, capacity) = self
                .compact_tiling_identities
                .as_ref()
                .map(|identities| (identities.len(), identities.capacity()))
                .expect("compact tiling identity storage was checked");
            self.preadmit_canonical_tiling_vec_growth::<PackedTilingRows>(len, capacity, 1)?;
            let terminal_authorized = self.canonical_tiling_terminal_authorized();
            let identities = self.compact_tiling_identities.as_mut().ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_storage_unavailable",
                ),
            )?;
            let reservation = if terminal_authorized {
                identities.try_reserve_exact(1)
            } else {
                identities.try_reserve(1)
            };
            reservation.map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_storage_unavailable",
                )
            })?;
            identities.push(packed_rows);
            self.validate_canonical_tiling_allocation()?;
            return Ok(());
        }
        self.insert_standard_solution_identity(identity)
    }

    pub(super) fn absorb_packed_tiling_chunk(
        &mut self,
        chunk: &super::tiling_parallel::WasmTilingRootChunk,
    ) -> Result<bool, WasmExactSearchError> {
        if self.problem.objective().kind() != ObjectiveKind::Tiling {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_packed_tiling_chunk_requires_tiling_objective",
            ));
        }
        if self.distributed_tiling_root_runs.is_some() {
            let root_ordinal = chunk
                .root_ordinal()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_ordinal_missing",
                ))?;
            let root_index = root_ordinal as usize;
            let run = self
                .distributed_tiling_root_runs
                .as_ref()
                .and_then(|runs| runs.get(root_index))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_ordinal_invalid",
                ))?;
            let sequence = usize::try_from(chunk.chunk_sequence()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_tiling_root_chunk_sequence_invalid")
            })?;
            if sequence >= run.committed_chunks.len() {
                self.preadmit_canonical_tiling_vec_growth::<PackedTilingRows>(
                    run.identities.len(),
                    run.identities.capacity(),
                    chunk.identities().len(),
                )?;
                self.preadmit_canonical_tiling_vec_growth::<DistributedTilingChunkCommit>(
                    run.committed_chunks.len(),
                    run.committed_chunks.capacity(),
                    1,
                )?;
            }
            let terminal_authorized = self.canonical_tiling_terminal_authorized();
            let run = self
                .distributed_tiling_root_runs
                .as_mut()
                .and_then(|runs| runs.get_mut(root_index))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_ordinal_invalid",
                ))?;
            if !run.absorb_chunk(chunk, terminal_authorized)? {
                return Ok(false);
            }
        } else {
            if chunk.root_ordinal().is_some() || chunk.root_complete() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_chunk_unexpected",
                ));
            }
            let (len, capacity) = self
                .compact_tiling_identities
                .as_ref()
                .map(|identities| (identities.len(), identities.capacity()))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_unavailable",
                ))?;
            self.preadmit_canonical_tiling_vec_growth::<PackedTilingRows>(
                len,
                capacity,
                chunk.identities().len(),
            )?;
            let terminal_authorized = self.canonical_tiling_terminal_authorized();
            let identities = self.compact_tiling_identities.as_mut().ok_or(
                WasmExactSearchError::InvalidProblem("wasm_compact_tiling_identity_unavailable"),
            )?;
            let reservation = if terminal_authorized {
                identities.try_reserve_exact(chunk.identities().len())
            } else {
                identities.try_reserve(chunk.identities().len())
            };
            reservation.map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_compact_tiling_identity_storage_unavailable",
                )
            })?;
            for packed in chunk.identities().iter().copied() {
                let packed_rows = packed.packed_rows();
                if !packed_rows_are_valid(&packed_rows) {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_packed_tiling_identity_invalid",
                    ));
                }
                identities.push(packed_rows);
            }
        }
        self.validate_canonical_tiling_allocation()?;
        for packed in chunk.identities().iter().copied() {
            self.packing_candidate_count = self.packing_candidate_count.saturating_add(1);
            self.packing_candidate_digest =
                mix_digest(self.packing_candidate_digest, packed.bucket_hash());
        }
        self.peak_cpu_bytes = self
            .peak_cpu_bytes
            .max(self.solution_identity_retained_bytes());
        Ok(true)
    }

    fn insert_standard_solution_identity(
        &mut self,
        identity: StandardBoard64TilingIdentity,
    ) -> Result<(), WasmExactSearchError> {
        let identity = TilingIdentityEntry::new(identity);
        if !self.buildable_identities.contains(&identity) {
            self.buildable_identities.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_solution_identity_storage_unavailable")
            })?;
            self.buildable_identities.insert(identity);
        }
        Ok(())
    }

    fn take_sorted_solution_identities(
        &mut self,
    ) -> Result<Vec<StandardBoard64TilingIdentity>, WasmExactSearchError> {
        let full_set = core::mem::take(&mut self.buildable_identities);
        let mut identities = Vec::new();
        identities.try_reserve_exact(full_set.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_solution_sort_storage_unavailable")
        })?;
        identities.extend(full_set.into_iter().map(|entry| entry.identity));
        identities.sort_unstable();
        Ok(identities)
    }

    fn take_tiling_solution_page_store(
        &mut self,
    ) -> Result<Option<Arc<TilingSolutionPageStore>>, WasmExactSearchError> {
        let Some(packed) = self.compact_tiling_identities.take() else {
            return Ok(None);
        };

        let mut catalog_rows = Vec::new();
        catalog_rows
            .try_reserve_exact(self.catalog.skeleton_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_tiling_page_catalog_unavailable")
            })?;
        for row_id in 0..self.catalog.skeleton_count() {
            let row = self.catalog.skeleton(row_id as u32);
            catalog_rows.push(PiecePlacementMask::new(row.piece, row.cells));
        }
        let store = if let Some(runs) = self.distributed_tiling_root_runs.take() {
            if runs.iter().any(|run| !run.complete) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_runs_incomplete",
                ));
            }
            if !packed.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_tiling_root_and_sequential_storage_mixed",
                ));
            }
            TilingSolutionPageStore::new_canonical_runs(
                self.catalog.initial_board(),
                catalog_rows,
                runs.into_iter().map(|run| run.identities).collect(),
            )
        } else {
            TilingSolutionPageStore::new_canonical(
                self.catalog.initial_board(),
                catalog_rows,
                packed,
            )
        }
        .map_err(WasmExactSearchError::InvalidProblem)?;
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(store.retained_bytes());
        Ok(Some(Arc::new(store)))
    }

    // External GPU candidate ingestion is retained for the optional WebGPU backend.
    #[cfg_attr(not(feature = "webgpu-search"), allow(dead_code))]
    pub fn process_external_candidate(
        &mut self,
        target_index: u32,
        row_ids: &[u32],
        control: &ExecutionControl,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        self.execution_control = control.clone();
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
        self.execution_control = control.clone();
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

    // External candidate identities bind distributed WebGPU result packets.
    #[cfg_attr(not(feature = "webgpu-search"), allow(dead_code))]
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
        if !self.problem.allows_solution_identity(&candidate.identity) {
            return Ok(None);
        }
        let candidate_ordinal = external_ordinal.unwrap_or(self.packing_candidate_count as u64);
        self.packing_candidate_count += 1;
        self.packing_candidate_digest = mix_digest(
            self.packing_candidate_digest,
            candidate.identity.bucket_hash(),
        );
        if self.problem.objective().kind() == ObjectiveKind::Tiling {
            return self.observe_tiling_candidate(candidate, candidate_ordinal);
        }
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
        let coverage_already_known = self.buildup_workspace.standard_bag_coverage_complete()
            || self
                .covered_patterns
                .is_superset(target.possible_patterns.as_ref())
                .expect("candidate pattern group belongs to the session universe");
        let witness_mode = CandidateWitnessMode::for_candidate(
            &self.problem,
            target,
            coverage_already_known,
            solution_coverage_required,
        );
        if matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            let future = checked_candidate_verification_peak_upper_bound(
                &self.problem,
                &self.catalog,
                &candidate,
                self.problem.output_policy().retains_representative_trace()
                    && self.representative_path.is_empty(),
            )
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "candidate verification memory projection is unavailable",
                )
            })?;
            self.ensure_session_memory_bound(future)?;
        }
        let result = match verify_candidate(
            &self.problem,
            &self.catalog,
            &candidate,
            target,
            &mut self.buildup_workspace,
            &mut self.coverage_evaluator,
            witness_mode,
            self.problem.output_policy().retains_representative_trace()
                && self.representative_path.is_empty(),
            profile_scale,
            control,
        ) {
            Ok(result) => result,
            Err(WasmExactSearchError::Cancelled) => {
                return Ok(Some(ExactSearchAdvance::Cancelled));
            }
            Err(error) => return Err(error),
        };
        if matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            let universe = self.problem.piece_source().materialized_universe().ok_or(
                WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
            )?;
            let bitset_peak = PatternBitSet::checked_all_projection(universe.pattern_count())
                .map(|projection| projection.constructor_peak_bytes)
                .ok_or_else(|| {
                    self.memory_projection_unavailable(
                        "candidate reduction bitset projection is unavailable",
                    )
                })?;
            let row_capacity = self
                .coverage_rows
                .capacity()
                .saturating_mul(2)
                .max(self.coverage_rows.len().saturating_add(1));
            let identity_capacity = self
                .buildable_identities
                .capacity()
                .saturating_mul(2)
                .max(self.buildable_identities.len().saturating_add(1));
            let coverage_capacity = self.solution_coverage.as_ref().map_or(0, |coverage| {
                coverage
                    .capacity()
                    .saturating_mul(2)
                    .max(coverage.len().saturating_add(1))
            });
            let reduction_future = bitset_peak
                .checked_mul(4)
                .and_then(|bytes| {
                    bytes.checked_add(
                        (row_capacity as u128)
                            .checked_mul(core::mem::size_of::<CoverageRow>() as u128)?,
                    )
                })
                .and_then(|bytes| {
                    bytes.checked_add(checked_hash_table_retained_upper_bound(
                        identity_capacity,
                        core::mem::size_of::<TilingIdentityEntry>(),
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(checked_hash_table_retained_upper_bound(
                        coverage_capacity,
                        core::mem::size_of::<StandardBoard64TilingIdentity>()
                            + core::mem::size_of::<PatternBitSet>(),
                    )?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        (result.representative_path.len() as u128)
                            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
                    )
                })
                .and_then(|bytes| bytes.checked_add(result.retained_bytes as u128))
                .ok_or_else(|| {
                    self.memory_projection_unavailable(
                        "candidate reduction memory projection is unavailable",
                    )
                })?;
            self.ensure_session_memory_bound(reduction_future)?;
        }
        if let Some(root) = result.observation_language_root {
            self.buildup_workspace.merge_observation_language(root)?;
        }
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
                + self.tablebase_retained_bytes
                + result.retained_bytes
                + self.coverage_evaluator.retained_bytes()
                + self.buildup_workspace.retained_bytes()
                + self.solution_identity_retained_bytes()
                + self.coverage_rows_retained_bytes()
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
            if solution_coverage_required {
                solution_coverage = Some(candidate_coverage.clone());
            }
            self.record_build_coverage_row(
                candidate.identity.bucket_hash(),
                candidate_coverage.clone(),
            )?;
        }
        if let Some(root) = result.symbolic_coverage_root {
            let materialized = (solution_coverage_required
                || self.pc_chance_coverage_evidence_available)
                .then(|| self.buildup_workspace.materialize_standard_bag_root(root))
                .transpose()?;
            if solution_coverage_required {
                let materialized = materialized
                    .as_ref()
                    .expect("solution coverage requested symbolic materialization");
                if let Some(solution_coverage) = solution_coverage.as_mut() {
                    solution_coverage.union_with(materialized).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_solution_coverage_universe_mismatch",
                        )
                    })?;
                } else {
                    solution_coverage = Some(materialized.clone());
                }
            }
            if self.pc_chance_coverage_evidence_available {
                self.record_build_coverage_row(
                    candidate.identity.bucket_hash(),
                    materialized.expect("chance evidence requested symbolic materialization"),
                )?;
            }
            self.buildup_workspace.merge_standard_bag_coverage(root)?;
            self.coverage_row_count = self.coverage_row_count.saturating_add(1);
            self.pattern_verified_execution_count = self
                .pattern_verified_execution_count
                .saturating_add(result.symbolic_covered_pattern_count);
        }
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
            self.catalog
                .retained_bytes()
                .saturating_add(self.geometry.retained_bytes())
                .saturating_add(self.tablebase_retained_bytes)
                .saturating_add(self.coverage_evaluator.retained_bytes())
                .saturating_add(self.buildup_workspace.retained_bytes())
                .saturating_add(self.solution_identity_retained_bytes())
                .saturating_add(self.coverage_rows_retained_bytes())
                .saturating_add(result.retained_bytes)
                .saturating_add(self.solution_coverage.as_ref().map_or(0, |coverage| {
                    coverage.capacity()
                        * (core::mem::size_of::<StandardBoard64TilingIdentity>()
                            + core::mem::size_of::<PatternBitSet>())
                }))
                .saturating_add(self.solution_coverage_bytes),
        );
        if self.memory_budget_exceeded() {
            self.mark_truncated("memory_budget_exceeded");
            return self.complete().map(Some);
        }
        if result.buildable {
            if let Some(solution_coverage) = solution_coverage {
                self.merge_solution_coverage(candidate.identity, &solution_coverage)?;
            }
            if retains_buildable_identity_evidence(&self.problem) {
                let identity = TilingIdentityEntry::new(candidate.identity);
                if !self.buildable_identities.contains(&identity)
                    && self.buildable_identities.try_reserve(1).is_err()
                {
                    self.mark_truncated("solution_identity_storage_unavailable");
                    return self.complete().map(Some);
                }
                self.buildable_identities.insert(identity);
            }
            let next = self
                .build_variant_count
                .checked_add(result.build_variant_count);
            self.build_variant_count = next.unwrap_or(u128::MAX);
            self.count_complete &= next.is_some() && result.count_complete;
            if self.problem.output_policy().retains_representative_trace()
                && self
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

    fn observe_tiling_candidate(
        &mut self,
        candidate: GeometryCandidate,
        candidate_ordinal: u64,
    ) -> Result<Option<ExactSearchAdvance>, WasmExactSearchError> {
        if let Err(error) = self.insert_tiling_candidate_identity(&candidate) {
            if self.canonical_tiling_terminal_authorized() {
                return Err(error);
            }
            self.mark_truncated("solution_identity_storage_unavailable");
            return self.complete().map(Some);
        }
        if self
            .representative_rank
            .is_none_or(|rank| candidate_ordinal < rank)
        {
            self.representative_rank = Some(candidate_ordinal);
            self.representative_identity = Some(candidate.identity);
            self.representative_candidate_id = Some(candidate.identity.bucket_hash());
        }
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
            self.catalog
                .retained_bytes()
                .saturating_add(self.geometry.retained_bytes())
                .saturating_add(self.tablebase_retained_bytes)
                .saturating_add(self.solution_identity_retained_bytes()),
        );
        if self.memory_budget_exceeded() {
            self.mark_truncated("memory_budget_exceeded");
            return self.complete().map(Some);
        }
        Ok(None)
    }

    fn merge_solution_coverage(
        &mut self,
        identity: StandardBoard64TilingIdentity,
        coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let parent_authorized = matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        );
        let needs_entry = !self
            .solution_coverage
            .as_ref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_solution_coverage_not_requested",
            ))?
            .contains_key(&identity);
        if needs_entry && parent_authorized {
            let map = self
                .solution_coverage
                .as_ref()
                .expect("coverage map exists");
            let projected_capacity = map
                .capacity()
                .saturating_mul(2)
                .max(map.len().saturating_add(1));
            let future = checked_hash_table_retained_upper_bound(
                projected_capacity,
                core::mem::size_of::<StandardBoard64TilingIdentity>()
                    + core::mem::size_of::<PatternBitSet>(),
            )
            .and_then(|bytes| {
                bytes.checked_add(
                    PatternBitSet::checked_all_projection(coverage.pattern_count())?
                        .constructor_peak_bytes,
                )
            })
            .ok_or_else(|| {
                self.memory_projection_unavailable(
                    "solution coverage growth projection is unavailable",
                )
            })?;
            self.ensure_session_memory_bound(future)?;
        }
        let map = self
            .solution_coverage
            .as_mut()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_solution_coverage_not_requested",
            ))?;
        if needs_entry {
            map.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_solution_coverage_storage_unavailable")
            })?;
            let empty = if parent_authorized {
                try_empty_pattern_bitset(
                    coverage.pattern_count(),
                    "wasm_solution_coverage_storage_unavailable",
                )?
            } else {
                PatternBitSet::new(coverage.pattern_count())
            };
            map.insert(identity, empty);
        }
        let entry = map
            .get_mut(&identity)
            .expect("solution coverage entry exists");
        let before = entry.retained_bytes();
        entry.union_with(coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_solution_coverage_universe_mismatch")
        })?;
        self.solution_coverage_bytes = self
            .solution_coverage_bytes
            .saturating_add(entry.retained_bytes().saturating_sub(before));
        Ok(())
    }

    fn record_build_coverage_row(
        &mut self,
        candidate_id: u64,
        coverage: PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_pc_chance_coverage_universe_mismatch")
        })?;
        if !self.pc_chance_coverage_evidence_available {
            return Ok(());
        }
        self.coverage_rows.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_pc_chance_coverage_row_storage_unavailable")
        })?;
        let source = self.problem.piece_source();
        let universe =
            source
                .materialized_universe()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_source_not_materialized",
                ))?;
        self.coverage_rows.push(CoverageRow::new_with_piece_source(
            candidate_id,
            CoverageRowKind::Build,
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            coverage,
        ));
        Ok(())
    }

    pub(super) fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let scalar = exact_distributed_worker_scalar_evidence(result).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_scalar_authority_invalid",
            ),
        )?;
        let representative_identity = result.representative_solution_identity();
        if scalar.representative_candidate_ordinal.is_some()
            != scalar.representative_candidate_id.is_some()
            || scalar.representative_candidate_ordinal.is_some()
                != representative_identity.is_some()
            || (scalar.representative_candidate_ordinal.is_none()
                && (!result.path_steps().is_empty() || scalar.representative_pattern_id.is_some()))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_representative_authority_invalid",
            ));
        }
        let pattern_count = exact_canonical_usize_field(result, "coverage_pattern_count").ok_or(
            WasmExactSearchError::InvalidProblem("wasm_distributed_result_pattern_count_missing"),
        )?;
        let coverage = strict_coverage_pattern_bitset_from_words(
            pattern_count,
            result.coverage_pattern_words(),
        )
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_distributed_result_coverage_invalid")
        })?;
        let evidence_policy = self.problem.pc_chance_evidence_policy();
        let distributed_minimum_cover = evidence_policy == PcChanceEvidencePolicy::PcMinimumCoverV2;
        if distributed_minimum_cover {
            self.absorb_distributed_minimum_cover_source(result, pattern_count, &coverage)?;
        } else if evidence_policy == PcChanceEvidencePolicy::PcProbabilityV2 {
            self.absorb_distributed_pc_chance_rows(result, pattern_count, &coverage)?;
        } else {
            self.pc_chance_coverage_evidence_available = false;
            self.coverage_rows_complete = false;
            self.coverage_rows = Vec::new();
        }

        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_distributed_coverage_universe_mismatch")
        })?;

        for identity in result
            .normalized_solution_identities()
            .iter()
            .filter(|_| !distributed_minimum_cover)
        {
            if !self.problem.output_policy().retains_solution_set() {
                break;
            }
            if self.problem.objective().kind() == ObjectiveKind::Tiling {
                self.insert_tiling_result_identity(*identity)
                    .map_err(|error| match error {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_compact_tiling_identity_storage_unavailable",
                        )
                        | WasmExactSearchError::InvalidProblem(
                            "wasm_solution_identity_storage_unavailable",
                        ) => WasmExactSearchError::InvalidProblem(
                            "wasm_distributed_solution_storage_unavailable",
                        ),
                        other => other,
                    })?;
                continue;
            }
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
        for solution_coverage in result
            .solution_coverages()
            .iter()
            .filter(|_| !distributed_minimum_cover)
        {
            self.merge_solution_coverage(
                solution_coverage.identity(),
                solution_coverage.covered_patterns(),
            )?;
        }

        let worker_candidates = scalar.packing_candidate_count;
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
            .saturating_add(scalar.coverage_row_count);
        self.pattern_verified_execution_count = self
            .pattern_verified_execution_count
            .saturating_add(scalar.pattern_verified_execution_count);
        let next_variants = self
            .build_variant_count
            .checked_add(scalar.build_variant_count);
        self.build_variant_count = next_variants.unwrap_or(u128::MAX);
        self.count_complete &= next_variants.is_some() && scalar.count_complete;
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &=
                scalar.execution_constraint_materialized;
        }

        self.peak_build_nodes = self.peak_build_nodes.max(scalar.peak_build_order_nodes);
        self.total_build_nodes = self
            .total_build_nodes
            .saturating_add(scalar.total_build_order_nodes);
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(scalar.coverage_product_words);
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(scalar.coverage_product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(scalar.coverage_product_edge_checks);
        self.realization_feasibility_states = self
            .realization_feasibility_states
            .saturating_add(scalar.realization_feasibility_states);
        self.realization_feasibility_rejected_candidates = self
            .realization_feasibility_rejected_candidates
            .saturating_add(scalar.realization_feasibility_rejected_candidates);
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(scalar.peak_reachability_states);
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(scalar.total_reachability_states);
        self.parallel_worker_retained_bytes = self
            .parallel_worker_retained_bytes
            .saturating_add(scalar.resource_peak_cpu_bytes);
        self.parallel_piece_language_cache_hits = self
            .parallel_piece_language_cache_hits
            .saturating_add(scalar.piece_language_coverage_cache_hits);
        self.parallel_piece_language_cache_misses = self
            .parallel_piece_language_cache_misses
            .saturating_add(scalar.piece_language_coverage_cache_misses);
        self.parallel_standard_bag_cache_hits = self
            .parallel_standard_bag_cache_hits
            .saturating_add(scalar.standard_bag_symbolic_cache_hits);
        self.parallel_standard_bag_cache_misses = self
            .parallel_standard_bag_cache_misses
            .saturating_add(scalar.standard_bag_symbolic_cache_misses);
        self.parallel_reachability_metrics.lock_queries = self
            .parallel_reachability_metrics
            .lock_queries
            .saturating_add(scalar.reachability_lock_queries);
        self.parallel_reachability_metrics.harddrop_queries = self
            .parallel_reachability_metrics
            .harddrop_queries
            .saturating_add(scalar.reachability_harddrop_queries);
        self.parallel_reachability_metrics.harddrop_hits = self
            .parallel_reachability_metrics
            .harddrop_hits
            .saturating_add(scalar.reachability_harddrop_hits);
        self.parallel_reachability_metrics.cache_reachable_hits = self
            .parallel_reachability_metrics
            .cache_reachable_hits
            .saturating_add(scalar.reachability_cache_reachable_hits);
        self.parallel_reachability_metrics.cache_unreachable_hits = self
            .parallel_reachability_metrics
            .cache_unreachable_hits
            .saturating_add(scalar.reachability_cache_unreachable_hits);
        self.parallel_reachability_metrics.cache_key_misses = self
            .parallel_reachability_metrics
            .cache_key_misses
            .saturating_add(scalar.reachability_cache_key_misses);
        self.parallel_reachability_metrics.partial_searches = self
            .parallel_reachability_metrics
            .partial_searches
            .saturating_add(scalar.reachability_partial_searches);
        self.parallel_reachability_metrics.exhaustive_searches = self
            .parallel_reachability_metrics
            .exhaustive_searches
            .saturating_add(scalar.reachability_exhaustive_searches);

        if self.problem.output_policy().retains_representative_trace() {
            if let Some(rank) = scalar.representative_candidate_ordinal {
                if self
                    .representative_rank
                    .is_none_or(|current| rank < current)
                {
                    self.representative_rank = Some(rank);
                    self.representative_identity = representative_identity;
                    self.representative_candidate_id = scalar.representative_candidate_id;
                    self.representative_pattern_id = scalar.representative_pattern_id;
                    self.representative_path = result.path_steps().to_vec();
                }
            }
        }
        if scalar.resource_truncated {
            self.mark_truncated("distributed_worker_incomplete");
        }
        Ok(())
    }

    /// Validates an untrusted worker-row batch against the coordinator's
    /// retained typed chance problem before committing any row. Missing
    /// transport leaves the ordinary public aggregate usable but deliberately
    /// removes typed chance authority, preserving fail-closed compatibility.
    fn absorb_distributed_pc_chance_rows(
        &mut self,
        result: &CoreExecutionResult,
        pattern_count: usize,
        aggregate_coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let Some(transport) = result.distributed_pc_chance_coverage_rows() else {
            self.pc_chance_coverage_evidence_available = false;
            self.coverage_rows_complete = false;
            self.coverage_rows = Vec::new();
            return Ok(());
        };
        if !self.pc_chance_coverage_evidence_available {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_inconsistent",
            ));
        }
        if !transport.complete()
            || exact_canonical_bool_field(result, "count_complete") != Some(true)
            || exact_canonical_bool_field(result, "probability_complete") != Some(true)
            || exact_canonical_bool_field(result, "resource_probability_complete") != Some(true)
            || exact_canonical_bool_field(result, "resource_truncated") != Some(false)
            || exact_canonical_usize_field(result, "coverage_row_count")
                != Some(transport.rows().len())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_incomplete",
            ));
        }
        let source = self.problem.piece_source();
        let universe =
            source
                .materialized_universe()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_source_not_materialized",
                ))?;
        if pattern_count != universe.pattern_count()
            || transport.pattern_count() != pattern_count
            || transport.piece_source_id() != source.id().get()
            || transport.pattern_universe_id() != universe.pattern_universe_id()
            || transport.pattern_weight_model_id() != universe.pattern_weight_model_id()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_identity_mismatch",
            ));
        }

        let mut row_union = try_empty_pattern_bitset(
            pattern_count,
            "wasm_distributed_pc_chance_union_storage_unavailable",
        )?;
        for row in transport.rows() {
            row_union.union_with(row.coverage_bits()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_distributed_pc_chance_evidence_universe_mismatch",
                )
            })?;
            if self
                .coverage_rows
                .binary_search_by_key(&row.candidate_id(), CoverageRow::candidate_id)
                .is_ok()
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_distributed_pc_chance_candidate_duplicate",
                ));
            }
        }
        if &row_union != aggregate_coverage {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_coverage_mismatch",
            ));
        }

        self.coverage_rows
            .try_reserve(transport.rows().len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_distributed_pc_chance_row_storage_unavailable",
                )
            })?;
        self.coverage_rows.extend(transport.rows().iter().cloned());
        self.coverage_rows
            .sort_unstable_by_key(CoverageRow::candidate_id);
        Ok(())
    }

    /// Rebuilds the minimum-cover producer evidence from the same canonical
    /// per-solution coverage dictionary that App validates at the terminal.
    /// Workers never transport a second private row matrix: the coordinator
    /// binds these rows to its retained prepared problem after validating the
    /// complete flags and exact aggregate union in one linear pass.
    fn absorb_distributed_minimum_cover_source(
        &mut self,
        result: &CoreExecutionResult,
        pattern_count: usize,
        aggregate_coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let source = result.solution_coverages();
        if exact_canonical_bool_field(result, "count_complete") != Some(true)
            || exact_canonical_bool_field(result, "probability_complete") != Some(true)
            || exact_canonical_bool_field(result, "resource_probability_complete") != Some(true)
            || exact_canonical_bool_field(result, "resource_truncated") != Some(false)
            || exact_canonical_usize_field(result, "minimum_cover_source_solution_count")
                != Some(source.len())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_minimum_source_incomplete",
            ));
        }
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        if pattern_count != universe.pattern_count() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_minimum_source_universe_mismatch",
            ));
        }

        let mut source_union = PatternBitSet::new(pattern_count);
        let mut previous_identity: Option<StandardBoard64TilingIdentity> = None;
        for entry in source {
            let identity = entry.identity();
            if previous_identity.is_some_and(|previous| previous >= identity)
                || entry.covered_patterns().pattern_count() != pattern_count
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_distributed_pc_minimum_source_not_canonical",
                ));
            }
            source_union
                .union_with(entry.covered_patterns())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_distributed_pc_minimum_source_universe_mismatch",
                    )
                })?;
            previous_identity = Some(identity);
        }
        if &source_union != aggregate_coverage {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_minimum_source_coverage_mismatch",
            ));
        }

        // Commit only after the entire borrowed partial has passed its closed
        // flag, identity-order, universe-shape, and aggregate-union checks.
        // A forged trailing row therefore cannot leave accepted coordinator
        // authority behind before the partial is rejected.
        for entry in source {
            let identity = entry.identity();
            let identity_entry = TilingIdentityEntry::new(identity);
            if !self.buildable_identities.contains(&identity_entry) {
                self.buildable_identities.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_distributed_solution_storage_unavailable",
                    )
                })?;
                self.buildable_identities.insert(identity_entry);
            }
            self.merge_solution_coverage(identity, entry.covered_patterns())?;
        }

        self.distributed_minimum_cover_source_complete = true;
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
            WasmDistributedBackendExecution::Cpu => {
                self.backend_selected = "wasm-cpu";
            }
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
                .saturating_add(self.tablebase_retained_bytes)
                .saturating_add(self.parallel_worker_retained_bytes),
        );
        if let Some(reason) = summary.truncated_reason {
            self.mark_truncated(reason);
        }
        self.complete()
    }

    // External geometry completion is the optional WebGPU backend's finalization seam.
    #[cfg_attr(not(any(feature = "webgpu-search", test)), allow(dead_code))]
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
        if self.problem.objective().kind() != ObjectiveKind::Tiling {
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
            if self
                .problem
                .queue_observation_policy()
                .requires_observation_policy()
            {
                self.coverage_rows_complete = false;
                let control = self.execution_control.clone();
                match self
                    .buildup_workspace
                    .evaluate_observation_language(&self.problem, &control)
                {
                    Ok(Some(coverage)) => {
                        let metrics = coverage.metrics;
                        self.covered_patterns = coverage.covered_patterns;
                        self.observation_policy_states = metrics.policy_states;
                        self.observation_policy_action_checks = metrics.action_checks;
                        self.observation_trie_nodes = metrics.observation_nodes;
                        self.peak_cpu_bytes = self.peak_cpu_bytes.max(
                            self.catalog
                                .retained_bytes()
                                .saturating_add(self.geometry.retained_bytes())
                                .saturating_add(self.tablebase_retained_bytes)
                                .saturating_add(self.coverage_evaluator.retained_bytes())
                                .saturating_add(self.buildup_workspace.retained_bytes())
                                .saturating_add(self.solution_identity_retained_bytes())
                                .saturating_add(self.coverage_rows_retained_bytes())
                                .saturating_add(self.solution_coverage.as_ref().map_or(
                                    0,
                                    |solution_coverage| {
                                        solution_coverage.capacity()
                                            * (core::mem::size_of::<StandardBoard64TilingIdentity>(
                                            ) + core::mem::size_of::<PatternBitSet>())
                                    },
                                ))
                                .saturating_add(self.solution_coverage_bytes)
                                .saturating_add(metrics.retained_bytes),
                        );
                    }
                    Ok(None) => {
                        let pattern_count = self
                            .problem
                            .piece_source()
                            .materialized_universe()
                            .map_or(0, |universe| universe.pattern_count());
                        self.covered_patterns = if matches!(
                            self.problem_retention,
                            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
                        ) {
                            try_empty_pattern_bitset(
                                pattern_count,
                                "wasm_observation_coverage_storage_unavailable",
                            )?
                        } else {
                            PatternBitSet::new(pattern_count)
                        };
                    }
                    Err(WasmExactSearchError::Cancelled) => {
                        return Ok(ExactSearchAdvance::Cancelled);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        coverage_span.finish(u64::from(self.covered_patterns.count_ones()));
        self.finished = true;
        let result_span = SearchStageSpan::begin(ExecutorSearchStage::WasmResultCanonicalize);
        let scoring_requested = self.problem.objective().score().requested();
        let execution_constraints = self.problem.objective().execution_constraints();
        let save_execution_requested = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_save_groups_v2_evidence();
        let path_execution_requested = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_path_v2_evidence();
        let execution_evidence_requested = scoring_requested
            || execution_constraints.requested()
            || save_execution_requested
            || path_execution_requested;
        let scoring_batch = if execution_evidence_requested {
            Some(self.prepare_exact_scoring_execution_batch()?)
        } else {
            None
        };
        let solution_identity_count = self.solution_identity_count();
        let result = self.build_result(include_normalized_keys, scoring_batch)?;
        if (scoring_requested || self.canonical_tiling_terminal_authorized())
            && matches!(
                self.problem_retention,
                SearchProblemRetention::ParentAuthorizedSharedInput { .. }
            )
        {
            self.validate_public_result_memory_with_future(&result, 0)?;
        }
        result_span.finish(solution_identity_count as u64);
        Ok(ExactSearchAdvance::Completed(result))
    }

    fn prepare_exact_scoring_execution_batch(
        &mut self,
    ) -> Result<ExactScoringExecutionBatch, WasmExactSearchError> {
        let identity_count = self.buildable_identities.len();
        let identity_bytes = (identity_count as u128)
            .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        let graph_slot_bytes = (identity_count as u128)
            .checked_mul(core::mem::size_of::<clearra_replay::ExactScoringExecutionGraph>() as u128)
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        self.ensure_score_session_memory_bound(
            identity_bytes
                .checked_add(graph_slot_bytes)
                .ok_or_else(|| self.scoring_memory_projection_overflow())?,
        )?;

        let mut identities = Vec::new();
        identities.try_reserve_exact(identity_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_identity_storage_unavailable")
        })?;
        identities.extend(self.buildable_identities.iter().map(|entry| entry.identity));
        identities.sort_unstable();
        let mut graphs = Vec::new();
        graphs.try_reserve_exact(identities.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_scoring_graph_storage_unavailable")
        })?;
        let mut complete = true;
        let identity_live_bytes = (identities.capacity() as u128)
            .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        let graph_outer_bytes = (graphs.capacity() as u128)
            .checked_mul(core::mem::size_of::<clearra_replay::ExactScoringExecutionGraph>() as u128)
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        let mut retained_graph_nested_bytes = 0_u128;
        for (index, identity) in identities.into_iter().enumerate() {
            let candidate_id = u64::try_from(index + 1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_scoring_candidate_id_overflow")
            })?;
            let projection = exact_scoring_execution_graph_memory_projection(
                &self.problem,
                &self.catalog,
                identity,
            )?;
            let candidate_future_bytes = identity_live_bytes
                .checked_add(graph_outer_bytes)
                .and_then(|bytes| bytes.checked_add(retained_graph_nested_bytes))
                .and_then(|bytes| bytes.checked_add(projection.peak_additional_bytes))
                .ok_or_else(|| self.scoring_memory_projection_overflow())?;
            self.ensure_score_session_memory_bound(candidate_future_bytes)?;
            match exact_scoring_execution_graph(
                &self.problem,
                &self.catalog,
                identity,
                candidate_id,
                &mut self.buildup_workspace,
            )? {
                Some(graph) => {
                    let retained = graph
                        .checked_nested_retained_bytes()
                        .ok_or_else(|| self.scoring_memory_projection_overflow())?;
                    debug_assert!(retained <= projection.retained_graph_nested_bytes);
                    retained_graph_nested_bytes = retained_graph_nested_bytes
                        .checked_add(retained)
                        .ok_or_else(|| self.scoring_memory_projection_overflow())?;
                    self.ensure_score_session_memory_bound(
                        identity_live_bytes
                            .checked_add(graph_outer_bytes)
                            .and_then(|bytes| bytes.checked_add(retained_graph_nested_bytes))
                            .ok_or_else(|| self.scoring_memory_projection_overflow())?,
                    )?;
                    graphs.push(graph);
                }
                None => {
                    complete = false;
                    self.ensure_score_session_memory_bound(
                        identity_live_bytes
                            .checked_add(graph_outer_bytes)
                            .and_then(|bytes| bytes.checked_add(retained_graph_nested_bytes))
                            .ok_or_else(|| self.scoring_memory_projection_overflow())?,
                    )?;
                }
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
        let pattern_outer_bytes = (universe.pattern_count() as u128)
            .checked_mul(
                core::mem::size_of::<Vec<clearra_core_domain::piece::piece_kind::PieceKind>>()
                    as u128,
            )
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        let mut pattern_nested_bytes = 0_u128;
        for pattern_index in 0..universe.pattern_count() {
            pattern_nested_bytes = pattern_nested_bytes
                .checked_add(
                    (universe.sequence_len_at(pattern_index) as u128)
                        .checked_mul(core::mem::size_of::<
                            clearra_core_domain::piece::piece_kind::PieceKind,
                        >() as u128)
                        .ok_or_else(|| self.scoring_memory_projection_overflow())?,
                )
                .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        }
        let graph_bytes = graph_outer_bytes
            .checked_add(retained_graph_nested_bytes)
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        self.ensure_score_session_memory_bound(
            graph_bytes
                .checked_add(pattern_outer_bytes)
                .and_then(|bytes| bytes.checked_add(pattern_nested_bytes))
                .ok_or_else(|| self.scoring_memory_projection_overflow())?,
        )?;
        let mut patterns = Vec::new();
        patterns
            .try_reserve_exact(universe.pattern_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_scoring_pattern_storage_unavailable")
            })?;
        let pattern_outer_live_bytes = (patterns.capacity() as u128)
            .checked_mul(
                core::mem::size_of::<Vec<clearra_core_domain::piece::piece_kind::PieceKind>>()
                    as u128,
            )
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        self.ensure_score_session_memory_bound(
            graph_bytes
                .checked_add(pattern_outer_live_bytes)
                .and_then(|bytes| bytes.checked_add(pattern_nested_bytes))
                .ok_or_else(|| self.scoring_memory_projection_overflow())?,
        )?;
        let mut pattern_nested_live_bytes = 0_u128;
        for pattern_index in 0..universe.pattern_count() {
            let sequence_len = universe.sequence_len_at(pattern_index);
            let requested_pattern_bytes = (sequence_len as u128)
                .checked_mul(
                    core::mem::size_of::<clearra_core_domain::piece::piece_kind::PieceKind>()
                        as u128,
                )
                .ok_or_else(|| self.scoring_memory_projection_overflow())?;
            self.ensure_score_session_memory_bound(
                graph_bytes
                    .checked_add(pattern_outer_live_bytes)
                    .and_then(|bytes| bytes.checked_add(pattern_nested_live_bytes))
                    .and_then(|bytes| bytes.checked_add(requested_pattern_bytes))
                    .ok_or_else(|| self.scoring_memory_projection_overflow())?,
            )?;
            let mut pattern = Vec::new();
            pattern.try_reserve_exact(sequence_len).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_scoring_pattern_storage_unavailable")
            })?;
            let actual_pattern_bytes = (pattern.capacity() as u128)
                .checked_mul(
                    core::mem::size_of::<clearra_core_domain::piece::piece_kind::PieceKind>()
                        as u128,
                )
                .ok_or_else(|| self.scoring_memory_projection_overflow())?;
            pattern_nested_live_bytes = pattern_nested_live_bytes
                .checked_add(actual_pattern_bytes)
                .ok_or_else(|| self.scoring_memory_projection_overflow())?;
            self.ensure_score_session_memory_bound(
                graph_bytes
                    .checked_add(pattern_outer_live_bytes)
                    .and_then(|bytes| bytes.checked_add(pattern_nested_live_bytes))
                    .ok_or_else(|| self.scoring_memory_projection_overflow())?,
            )?;
            universe.write_sequence_at(pattern_index, &mut pattern);
            if pattern.len() != sequence_len {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_scoring_pattern_length_mismatch",
                ));
            }
            patterns.push(pattern);
        }
        let (kick_table_id, rule_profile_id) = replay_profile_ids(&self.problem);
        let batch = ExactScoringExecutionBatch::new(
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
        );
        let batch_bytes = (core::mem::size_of::<ExactScoringExecutionBatch>() as u128)
            .checked_add(
                batch
                    .checked_nested_retained_bytes()
                    .ok_or_else(|| self.scoring_memory_projection_overflow())?,
            )
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        self.ensure_score_session_memory_bound(batch_bytes)?;
        Ok(batch)
    }

    fn scoring_memory_projection_overflow(&self) -> WasmExactSearchError {
        WasmExactSearchError::resource_admission(
            self._execution_admission
                .ensure_memory_bound(u128::MAX, 1)
                .expect_err("checked scoring memory projection overflow is unavailable"),
        )
    }

    fn checked_terminal_result_build_peak_upper_bound(
        &self,
        scoring_batch: Option<&ExactScoringExecutionBatch>,
    ) -> Option<u128> {
        const FIELD_COUNT_UPPER_BOUND: u128 = 256;
        const FIELD_BACKING_BYTES_UPPER_BOUND: u128 = 192;
        const NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND: u128 = 512;
        const DECIMAL_U128_BACKING_BYTES: u128 = 39;

        let universe = self.problem.piece_source().materialized_universe()?;
        let pattern_count = universe.pattern_count() as u128;
        let identity_count_usize = self.solution_identity_count();
        let identity_count = identity_count_usize as u128;
        let coverage_count = self
            .solution_coverage
            .as_ref()
            .map_or(0_u128, |coverage| coverage.len() as u128);
        let bitset_peak =
            PatternBitSet::checked_all_projection(universe.pattern_count())?.constructor_peak_bytes;
        let pattern_word_bytes = pattern_count
            .checked_add(63)?
            .checked_div(64)?
            .checked_mul(core::mem::size_of::<u64>() as u128)?;

        let field_copy_bytes = FIELD_COUNT_UPPER_BOUND
            .checked_mul(
                (core::mem::size_of::<(String, String)>() as u128)
                    .checked_add(FIELD_BACKING_BYTES_UPPER_BOUND.checked_mul(2)?)?,
            )?
            // fields, SearchExecutionReport, and fail-closed summary rebuild
            .checked_mul(3)?;
        let identity_bytes = identity_count
            .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?;
        let normalized_key_bytes = identity_count.checked_mul(
            (core::mem::size_of::<String>() as u128)
                .checked_add(NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND)?,
        )?;
        let coverage_outer_bytes = coverage_count.checked_mul(
            (core::mem::size_of::<SolutionCoverage>()
                + core::mem::size_of::<NormalizedSolutionCoverage>()) as u128,
        )?;
        let coverage_clone_count = coverage_count
            .checked_mul(3)?
            .checked_add(identity_count)?
            .checked_add(2)?;
        let coverage_bytes = coverage_clone_count.checked_mul(bitset_peak)?;
        let probability_bytes =
            identity_count.checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?;
        let pattern_weight_bytes = pattern_count.checked_mul(
            (core::mem::size_of::<String>() as u128).checked_add(DECIMAL_U128_BACKING_BYTES)?,
        )?;
        let trace_bytes = (self.representative_path.len() as u128)
            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?;
        let scoring_batch_bytes = scoring_batch.map_or(Some(0_u128), |batch| {
            (core::mem::size_of::<ExactScoringExecutionBatch>() as u128)
                .checked_add(batch.checked_nested_retained_bytes()?)
        })?;
        // Each typed problem-evidence owner is a normalized clone of the
        // executed problem. Reusing one request external upper bound per clone
        // is conservative and avoids constructing evidence merely to learn its
        // size. Score portfolios retain two independent snapshots: one for the
        // coverage proof and one for the score replay proof.
        let evidence_bytes = checked_typed_problem_evidence_upper_bound(
            &self.problem,
            self.checked_external_retained_upper_bound_bytes()?,
        )?;
        let (tiling_store_build_bytes, tiling_initial_page_bytes) =
            if self.problem.output_policy() == SearchOutputPolicy::TilingOnly {
                let run_count = self
                    .distributed_tiling_root_runs
                    .as_ref()
                    .map_or(usize::from(identity_count_usize != 0), Vec::len);
                (
                    TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
                        self.catalog.skeleton_count(),
                        identity_count_usize,
                        run_count,
                    )?,
                    TilingSolutionPageStore::checked_initial_page_build_peak_upper_bound(
                        identity_count_usize.min(TILING_SOLUTION_INITIAL_PAGE_SIZE),
                    )?,
                )
            } else {
                (0, 0)
            };

        (core::mem::size_of::<CoreExecutionResult>() as u128)
            .checked_add(field_copy_bytes)?
            .checked_add(identity_bytes.checked_mul(2)?)?
            .checked_add(normalized_key_bytes)?
            .checked_add(coverage_outer_bytes)?
            .checked_add(coverage_bytes)?
            .checked_add(probability_bytes)?
            .checked_add(pattern_weight_bytes)?
            .checked_add(pattern_word_bytes.checked_mul(2)?)?
            .checked_add(trace_bytes)?
            .checked_add(scoring_batch_bytes)?
            .checked_add(evidence_bytes)?
            .checked_add(tiling_store_build_bytes)?
            .checked_add(tiling_initial_page_bytes)
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
        &mut self,
        include_normalized_keys: bool,
        scoring_batch: Option<ExactScoringExecutionBatch>,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        if matches!(
            self.problem_retention,
            SearchProblemRetention::ParentAuthorizedSharedInput { .. }
        ) {
            let future = self
                .checked_terminal_result_build_peak_upper_bound(scoring_batch.as_ref())
                .ok_or_else(|| {
                    self.memory_projection_unavailable(
                        "terminal result build memory projection is unavailable",
                    )
                })?;
            self.ensure_session_memory_bound(future)?;
        }
        let tiling_only = self.problem.objective().kind() == ObjectiveKind::Tiling;
        let canonical_tiling = self.problem.output_policy() == SearchOutputPolicy::TilingOnly;
        let solution_set_materialized = self.problem.output_policy().retains_solution_set();
        let tiling_solution_store = if tiling_only {
            self.take_tiling_solution_page_store()?
        } else {
            None
        };
        let mut identities = if !solution_set_materialized {
            Vec::new()
        } else if let Some(store) = &tiling_solution_store {
            store
                .page_identities(0, TILING_SOLUTION_INITIAL_PAGE_SIZE)
                .map_err(WasmExactSearchError::InvalidProblem)?
        } else {
            self.take_sorted_solution_identities()?
        };
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
        let coverage_probability = if tiling_only {
            "not-calculated".to_owned()
        } else {
            universe
                .weights()
                .covered_weight(&self.covered_patterns)
                .expect("coverage and supply use one pattern universe")
                .get()
                .to_string()
        };
        let probability_complete =
            !tiling_only && universe.complete() && self.truncated_reason.is_none();
        let count_complete = self.truncated_reason.is_none()
            && self.count_complete
            && (!tiling_only || self.tiling_supply_projection_complete);
        let source_solution_count = tiling_solution_store
            .as_ref()
            .map_or(identities.len(), |store| store.len());
        let mut solution_coverages = Vec::new();
        if let Some(coverage) = self.solution_coverage.as_ref() {
            solution_coverages
                .try_reserve_exact(coverage.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_result_solution_coverage_storage_unavailable",
                    )
                })?;
            solution_coverages.extend(
                coverage
                    .iter()
                    .map(|(identity, bits)| SolutionCoverage::new(*identity, bits.clone())),
            );
            solution_coverages.sort_unstable_by_key(SolutionCoverage::identity);
        }
        let mut normalized_solution_coverages = Vec::new();
        normalized_solution_coverages
            .try_reserve_exact(solution_coverages.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_result_normalized_coverage_storage_unavailable",
                )
            })?;
        normalized_solution_coverages.extend(solution_coverages.iter().map(|coverage| {
            NormalizedSolutionCoverage::new(
                NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity())
                    .as_str(),
                coverage.covered_patterns().clone(),
            )
        }));
        if include_normalized_keys && self.distributed_minimum_cover_source_complete {
            self.coverage_rows.clear();
            self.coverage_rows
                .try_reserve_exact(solution_coverages.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_distributed_pc_minimum_source_storage_unavailable",
                    )
                })?;
            let piece_source_id = self.problem.piece_source().id().get();
            let pattern_universe_id = universe.pattern_universe_id();
            let pattern_weight_model_id = universe.pattern_weight_model_id();
            self.coverage_rows
                .extend(solution_coverages.iter().map(|coverage| {
                    CoverageRow::new_with_piece_source(
                        coverage.identity().bucket_hash(),
                        CoverageRowKind::Build,
                        piece_source_id,
                        pattern_universe_id,
                        pattern_weight_model_id,
                        coverage.covered_patterns().clone(),
                    )
                }));
        }
        let observation_policy = self.problem.queue_observation_policy();
        let visible_seven_policy = observation_policy.requires_observation_policy();
        let minimum_cover_requested =
            self.problem.objective().kind() == ObjectiveKind::MinimumCover;
        let pc_minimum_cover_deferred_to_coordinator = minimum_cover_requested
            && !visible_seven_policy
            && self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_minimum_cover_v2_evidence()
            && !self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_score_portfolio_v2_evidence();
        let score_portfolio_deferred_to_coordinator = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_score_portfolio_v2_evidence();
        // Score portfolios need the complete buildable candidate dictionary.
        // Their score-only eligibility rows and exact minimum cover are derived
        // by the typed App postprocessor; reducing here would instead choose a
        // coverage-only cover and orphan the exact scoring batch identities.
        let minimum_cover_product_reduction = minimum_cover_requested
            && !pc_minimum_cover_deferred_to_coordinator
            && !score_portfolio_deferred_to_coordinator
            && include_normalized_keys
            && !visible_seven_policy;
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
                let canonical_selection = {
                    let proof_span =
                        SearchStageSpan::begin(ExecutorSearchStage::WasmMinimumCoverProof);
                    // The branch-and-bound proof is cardinality authority only:
                    // dominance reduction may intentionally remove an original
                    // row that belongs to the lexicographically first optimum.
                    // Presentation identity therefore comes from the exact
                    // original-row portfolio enumerator after it proves k*.
                    let selection =
                        canonical_minimum_cover_portfolio(&self.covered_patterns, &rows);
                    proof_span.finish(u64::try_from(rows.len()).unwrap_or(u64::MAX));
                    selection
                };
                match canonical_selection {
                    Ok(Some(selection)) => {
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
                    Ok(None) => minimum_cover_reason = "required-pattern-cover-incomplete",
                    Err(_) => minimum_cover_reason = "pattern-universe-mismatch",
                }
            }
        } else if minimum_cover_requested {
            minimum_cover_reason = if visible_seven_policy {
                "visible-seven-policy-minimum-cover-not-materialized"
            } else if pc_minimum_cover_deferred_to_coordinator
                || score_portfolio_deferred_to_coordinator
            {
                "deferred-to-coordinator"
            } else {
                "minimum-cover-not-materialized"
            };
        }
        let normalized_hash = if !solution_set_materialized {
            "not-calculated".to_owned()
        } else {
            tiling_solution_store.as_ref().map_or_else(
                || {
                    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                        &identities,
                    )
                },
                |store| store.normalized_hash().to_owned(),
            )
        };
        let normalized_keys = if solution_set_materialized && include_normalized_keys {
            let mut keys = Vec::new();
            keys.try_reserve_exact(identities.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_result_normalized_key_storage_unavailable",
                )
            })?;
            keys.extend(
                identities
                    .iter()
                    .copied()
                    .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
                    .map(|key| key.as_str().to_owned()),
            );
            keys
        } else {
            Vec::new()
        };
        let solution_probabilities_requested =
            !tiling_only && self.problem.solution_probability_policy().requested();
        let solution_probability_complete = !solution_probabilities_requested
            || (!visible_seven_policy
                && probability_complete
                && count_complete
                && covers_all_identities(&identities, &solution_coverages));
        let solution_probabilities: Vec<SolutionProbabilityReport> =
            if solution_probabilities_requested && !visible_seven_policy {
                probability_reports(
                    &identities,
                    &solution_coverages,
                    universe.weights(),
                    solution_probability_complete,
                )
            } else {
                Vec::new()
            };
        let build_variant_count_exact = !tiling_only
            && self.problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountAll
            && count_complete;
        let objective = match self.problem.objective().kind() {
            ObjectiveKind::All => "all",
            ObjectiveKind::Unique => "unique",
            ObjectiveKind::MinimumCover => "minimum-cover",
            ObjectiveKind::Tiling => "tiling",
        };
        let score_policy = self.problem.objective().score();
        let execution_constraints = self.problem.objective().execution_constraints();
        let execution_constraint_requested = execution_constraints.requested();
        let save_execution_requested = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_save_groups_v2_evidence();
        let path_execution_requested = self
            .problem
            .pc_chance_evidence_policy()
            .retains_pc_path_v2_evidence();
        let scoring_execution_complete = scoring_batch
            .as_ref()
            .is_some_and(ExactScoringExecutionBatch::complete);
        let score_objective_requested = score_policy.requested();
        // Coverage evidence certifies the complete producer rows, not whether
        // the score-aware coordinator has already selected its portfolio. For
        // score-minimals the latter is deliberately deferred while the former
        // must remain complete so App can derive the exact score-only cover.
        let non_score_objective_complete = count_complete
            && (score_portfolio_deferred_to_coordinator
                || !minimum_cover_requested
                || (minimum_cover_complete && minimum_cover_proven));
        let solution_count = if !solution_set_materialized {
            0
        } else if minimum_cover_requested {
            identities.len()
        } else {
            source_solution_count
        };
        let solution_found = if solution_set_materialized {
            solution_count != 0
        } else {
            !self.covered_patterns.is_empty()
        };
        let mut reachability_metrics = self.buildup_workspace.reachability_metrics();
        add_reachability_metrics(
            &mut reachability_metrics,
            self.parallel_reachability_metrics,
        );
        let requested_backend = self.problem.backend_policy().requested_backend();
        let runtime_webgpu_available = self.problem.backend_policy().runtime_webgpu_available();
        let explicit_gpu_backend = matches!(
            requested_backend,
            RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
        );
        let gpu_available = self.backend_selected == "webgpu"
            || (explicit_gpu_backend
                && runtime_webgpu_available
                && cfg!(feature = "webgpu-search"));
        let gpu_disabled_reason = if self.backend_selected == "webgpu" {
            "none"
        } else if self.backend_fallback_used {
            self.backend_fallback_reason
        } else if explicit_gpu_backend && !runtime_webgpu_available {
            "gpu_device_not_found"
        } else if explicit_gpu_backend && !cfg!(feature = "webgpu-search") {
            "gpu_kernel_unavailable"
        } else if explicit_gpu_backend {
            "gpu_backend_not_connected"
        } else {
            "not_requested"
        };
        let gpu_trust_state = if self.backend_selected == "webgpu" {
            "gpu-computed-cpu-confirmed"
        } else if self.backend_fallback_used {
            "fallback-used"
        } else {
            "not-used"
        };
        let hybrid_status = if requested_backend != RequestedSearchBackend::Hybrid {
            "not-requested"
        } else if self.backend_selected == "webgpu" {
            "gpu-ready"
        } else if self.backend_selected == "wasm-cpu" {
            "cpu-selected"
        } else {
            "unavailable"
        };
        let hybrid_disabled_reason = if requested_backend != RequestedSearchBackend::Hybrid {
            "not_requested"
        } else if self.backend_selected == "webgpu" {
            "none"
        } else {
            gpu_disabled_reason
        };
        let rule = self.problem.rule_profile_value();
        let kick_profile = self.problem.kick_profile();
        let rule_capability = RuleCapability::from_rule(rule);
        let field_values = [
            field(
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            field("backend_selected", self.backend_selected),
            field("actual_backend", self.backend_selected),
            field("rule_profile", rule.id().as_str()),
            field("kick_profile", kick_profile.profile_id().as_str()),
            field(
                "effective_kick_model",
                rule_capability.kick_model().as_str(),
            ),
            field("verified_kick_profile", kick_profile.verified()),
            field(
                "kick_profile_transition_count",
                kick_profile.transition_count(),
            ),
            field(
                "backend_fallback_allowed",
                self.problem.backend_policy().allow_backend_fallback(),
            ),
            field("backend_fallback_used", self.backend_fallback_used),
            field("fallback_used", self.backend_fallback_used),
            field("backend_fallback_reason", self.backend_fallback_reason),
            field("fallback_backend", self.fallback_backend.unwrap_or("none")),
            field("gpu_available", gpu_available),
            field("gpu_disabled_reason", gpu_disabled_reason),
            field("gpu_trust_state", gpu_trust_state),
            field("hybrid_status", hybrid_status),
            field("hybrid_disabled_reason", hybrid_disabled_reason),
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
            field(
                "search_output_policy",
                self.problem.output_policy().as_str(),
            ),
            field("search_traversal", "canonical-skeleton-exact-cover"),
            field("tablebase_requested", self.tablebase_requested),
            field("tablebase_status", self.tablebase_status),
            field(
                "tablebase_tier",
                if self.tablebase_status == "connected-exact-dead-index" {
                    "pc4-compact-exact"
                } else {
                    "none"
                },
            ),
            field(
                "tablebase_semantics",
                if self.tablebase_status == "connected-exact-dead-index" {
                    "exact-dead-hit-or-unknown-with-generic-exact-fallback"
                } else {
                    "generic-exact"
                },
            ),
            field("tablebase_artifact_bytes", self.tablebase_artifact_bytes),
            field(
                "tablebase_payload_sha256",
                self.tablebase_payload_sha256.as_deref().unwrap_or("none"),
            ),
            field(
                "tablebase_pruned_states",
                self.geometry.tablebase_pruned_states(),
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
            field("queue_knowledge", observation_policy.keyword()),
            field(
                "coverage_semantics",
                if tiling_only {
                    "not-evaluated-tiling-only"
                } else {
                    observation_policy.coverage_semantics()
                },
            ),
            field(
                "visible_piece_count",
                observation_policy
                    .visible_piece_count()
                    .map_or_else(|| "all".to_owned(), |count| count.to_string()),
            ),
            field("source_sequence_length", source_sequence_length),
            field(
                "total_possible_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field(
                "execution_availability_state",
                self.dense_pattern_preflight.availability.state().as_str(),
            ),
            field("execution_availability_reason", "none"),
            field(
                "execution_descriptor_pattern_count",
                self.dense_pattern_preflight.descriptor_pattern_count,
            ),
            field(
                "execution_dense_pattern_count",
                self.dense_pattern_preflight.dense_pattern_count,
            ),
            field(
                "execution_required_dense_bytes",
                self.dense_pattern_preflight.required_dense_bytes,
            ),
            field(
                "execution_required_memory_bytes",
                self.dense_pattern_preflight
                    .availability
                    .required_memory_bytes()
                    .unwrap_or(self.dense_pattern_preflight.required_dense_bytes),
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
            field("packing_candidate_is_solution", tiling_only),
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
                if self.problem.output_policy().retains_candidate_digest() {
                    format!("{:016x}", self.packing_candidate_digest)
                } else {
                    "not-calculated".to_owned()
                },
            ),
            field(
                "packing_candidate_set_digest_calculated",
                self.problem.output_policy().retains_candidate_digest(),
            ),
            field("packing_count_complete", count_complete),
            field(
                "packing_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field("solution_found", solution_found),
            field(
                "unique_solution_count",
                if solution_set_materialized {
                    solution_count.to_string()
                } else {
                    "not-calculated".to_owned()
                },
            ),
            field(
                "normalized_unique_solution_count",
                if solution_set_materialized {
                    solution_count.to_string()
                } else {
                    "not-calculated".to_owned()
                },
            ),
            field("solution_count_calculated", solution_set_materialized),
            field("solution_set_materialized", solution_set_materialized),
            field("solution_keys_materialized_count", normalized_keys.len()),
            field(
                "solution_keys_complete",
                solution_set_materialized
                    && tiling_solution_store
                        .as_ref()
                        .is_none_or(|store| store.len() == normalized_keys.len()),
            ),
            field(
                "solution_page_available",
                tiling_solution_store
                    .as_ref()
                    .is_some_and(|store| store.len() > normalized_keys.len()),
            ),
            field("minimum_cover_requested", minimum_cover_requested),
            field("minimum_cover_source_solution_count", source_solution_count),
            field("minimum_cover_selected_solution_count", solution_count),
            field(
                "minimum_cover_required_pattern_count",
                if tiling_only {
                    0
                } else {
                    self.covered_patterns.count_ones()
                },
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
            field("buildability_verified", !tiling_only),
            field("coverage_calculated", !tiling_only),
            field("probability_calculated", !tiling_only),
            field(
                "pattern_verified_execution_count",
                self.pattern_verified_execution_count,
            ),
            field("coverage_row_count", self.coverage_row_count),
            field("coverage_pattern_count", universe.pattern_count()),
            field("materialized_pattern_count", universe.pattern_count()),
            field(
                "covered_pattern_count",
                if tiling_only {
                    0
                } else {
                    self.covered_patterns.count_ones()
                },
            ),
            field("coverage_probability", coverage_probability),
            field("observation_policy_states", self.observation_policy_states),
            field(
                "observation_policy_action_checks",
                self.observation_policy_action_checks,
            ),
            field("observation_trie_nodes", self.observation_trie_nodes),
            field(
                "materialized_probability_mass",
                universe.materialized_probability_mass().get(),
            ),
            field("renormalized", false),
            field("probability_complete", probability_complete),
            field("supply_probability_complete", probability_complete),
            field("resource_probability_complete", probability_complete),
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
                if solution_probabilities_requested && visible_seven_policy {
                    "unsupported-under-visible-seven-policy"
                } else if solution_probabilities_requested {
                    "normalized-solution-pattern-bitset-or-union"
                } else {
                    "not-requested"
                },
            ),
            field(
                "solution_probability_incomplete_reason",
                if solution_probabilities_requested && visible_seven_policy {
                    "per-solution-policy-language-not-materialized"
                } else if solution_probabilities_requested && !solution_probability_complete {
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
            field("postprocess_pc_save_requested", save_execution_requested),
            field("postprocess_pc_path_requested", path_execution_requested),
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
        let mut fields = Vec::new();
        fields.try_reserve_exact(field_values.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_result_field_storage_unavailable")
        })?;
        fields.extend(field_values);
        if save_execution_requested {
            fields.try_reserve_exact(4).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_pc_save_identity_field_storage_unavailable",
                )
            })?;
            fields.extend([
                field("problem_preset", self.problem.preset().as_str()),
                field("piece_source_id", self.problem.piece_source().id().get()),
                field("pattern_universe_id", universe.pattern_universe_id().get()),
                field(
                    "pattern_weight_model_id",
                    universe.pattern_weight_model_id().get(),
                ),
            ]);
        }
        if canonical_tiling {
            let incomplete_reason = self
                .truncated_reason
                .or_else(|| {
                    (!self.tiling_supply_projection_complete)
                        .then_some("supply_universe_incomplete")
                })
                .unwrap_or("none");
            let replacements =
                crate::service::pc_summary_builder::canonical_tiling_family_result_fields(
                    &self.problem,
                    solution_count,
                    &normalized_hash,
                    normalized_keys.len(),
                    count_complete,
                    incomplete_reason,
                    self.canonical_tiling_terminal_authorized(),
                );
            fields.retain(|(key, _)| {
                !replacements
                    .iter()
                    .any(|(replacement, _)| replacement == key)
            });
            fields.try_reserve_exact(replacements.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_result_field_storage_unavailable")
            })?;
            fields.extend(replacements);
        }
        let pattern_weights = if score_policy.requested()
            || execution_constraint_requested
            || save_execution_requested
        {
            let mut weights = Vec::new();
            weights
                .try_reserve_exact(universe.pattern_count())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_pattern_weight_storage_unavailable")
                })?;
            for pattern in 0..universe.pattern_count() {
                let mut weight = String::new();
                weight.try_reserve_exact(39).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_pattern_weight_storage_unavailable")
                })?;
                write!(&mut weight, "{}", universe.weight_at(pattern).get()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_pattern_weight_format_unavailable")
                })?;
                weights.push(weight);
            }
            weights
        } else {
            Vec::new()
        };
        let pc_score_problem_evidence = if score_objective_requested && scoring_batch.is_some() {
            let evidence = if self
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_score_portfolio_v2_evidence()
            {
                PcScoreProblemEvidence::from_executed_score_portfolio_problem(&self.problem)
            } else {
                PcScoreProblemEvidence::from_executed_problem(&self.problem)
            };
            Some(evidence.map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_pc_score_problem_evidence_identity_mismatch",
                )
            })?)
        } else {
            None
        };
        let pc_chance_coverage_evidence = if tiling_only
            || !self.pc_chance_coverage_evidence_available
        {
            None
        } else {
            self.coverage_rows
                .sort_unstable_by_key(CoverageRow::candidate_id);
            let mut row_union = if matches!(
                self.problem_retention,
                SearchProblemRetention::ParentAuthorizedSharedInput { .. }
            ) {
                try_empty_pattern_bitset(
                    universe.pattern_count(),
                    "wasm_pc_chance_coverage_row_union_storage_unavailable",
                )?
            } else {
                PatternBitSet::new(universe.pattern_count())
            };
            for row in &self.coverage_rows {
                row_union.union_with(row.coverage_bits()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_pc_chance_coverage_row_union_mismatch",
                    )
                })?;
            }
            let deferred_minimum_cover_source_complete =
                pc_minimum_cover_deferred_to_coordinator && count_complete && probability_complete;
            let complete = self.coverage_rows_complete
                && probability_complete
                && count_complete
                && (non_score_objective_complete || deferred_minimum_cover_source_complete)
                && (self.distributed_minimum_cover_source_complete
                    || self.coverage_rows.len() == self.coverage_row_count)
                && row_union == self.covered_patterns;
            Some(
                PcChanceCoverageEvidence::from_problem_rows(
                    &self.problem,
                    core::mem::take(&mut self.coverage_rows),
                    complete,
                )
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_pc_chance_coverage_evidence_identity_mismatch",
                    )
                })?,
            )
        };
        let mut coverage_pattern_words = Vec::new();
        coverage_pattern_words
            .try_reserve_exact(self.covered_patterns.words().len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_result_coverage_word_storage_unavailable",
                )
            })?;
        coverage_pattern_words.extend_from_slice(self.covered_patterns.words());
        let mut representative_path = Vec::new();
        representative_path
            .try_reserve_exact(self.representative_path.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_result_representative_path_storage_unavailable",
                )
            })?;
        representative_path.extend_from_slice(&self.representative_path);
        let mut result = CoreExecutionResult::new(fields, representative_path)
            .with_normalized_solution_keys(normalized_keys)
            .with_normalized_solution_identities(identities)
            .with_representative_solution_identity(self.representative_identity)
            .with_coverage_pattern_words(coverage_pattern_words)
            .with_solution_coverages(solution_coverages)
            .with_normalized_solution_coverages(normalized_solution_coverages)
            .with_solution_probabilities(solution_probabilities)
            .with_postprocess_execution_batch(
                Vec::new(),
                scoring_execution_complete,
                pattern_weights,
            )
            .with_exact_scoring_execution_batch(scoring_batch)
            .with_pc_score_problem_evidence(pc_score_problem_evidence);
        if let Some(evidence) = pc_chance_coverage_evidence {
            result = result.with_pc_chance_coverage_evidence(evidence);
        }
        if let Some(store) = tiling_solution_store {
            result = result.with_tiling_solution_page_store(store);
        }
        if self.canonical_tiling_terminal_authorized() {
            result = result.with_pc_tiling_memory_admission_evidence(
                PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority,
            );
        }
        Ok(result)
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

pub(super) fn retains_buildable_identity_evidence(problem: &SearchProblem) -> bool {
    // CoverageSummary still hides identities in `build_result`; requested execution constraints
    // retain them only long enough to build the authoritative post-processing graph.
    problem.output_policy().retains_solution_set()
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
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::{
        canonical_minimum_cover_portfolio, checked_typed_problem_evidence_upper_bound,
        packed_rows_are_valid, DistributedTilingRootRun, ExactSearchAdvance,
        WasmExactSearchSession, MAX_BOARD64_PIECES,
    };
    use crate::backend::wasm_cpu::tiling_parallel::{
        WasmPackedTilingIdentity, WasmTilingRootChunk,
    };
    use crate::terminal_supply_conformance::{
        terminal_supply_p0_expected_identities, terminal_supply_p0_fixed_problem,
        terminal_supply_p0_generic_problem, TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
    };
    use crate::tiling_solution_store::{
        pack_tiling_row_ids, read_packed_tiling_row, PackedTilingRows, PACKED_TILING_MAX_ROW_ID,
    };
    use crate::{
        CoreExecutionResult, DistributedPcChanceCoverageRows, WasmCandidateProducerAdvance,
        WasmCpuCandidateProducer, WasmCpuSearchBackend, WasmDistributedVerifier,
    };
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
            PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };
    use clearra_coverage::{
        pattern::pattern_bitset::PatternBitSet,
        row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    };
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard,
        PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::{
        queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
        QueueObservationPolicy,
    };

    #[test]
    fn generic_minimum_cover_uses_original_row_lex_first_identity_after_proof_reduction() {
        fn row(patterns: &[u32]) -> PatternBitSet {
            PatternBitSet::from_pattern_indices(3, patterns.to_vec()).expect("fixture row")
        }

        let rows = vec![row(&[1, 2]), row(&[0]), row(&[0, 1])];
        let selection = canonical_minimum_cover_portfolio(&PatternBitSet::all(3), &rows)
            .expect("exact portfolio authority")
            .expect("complete cover");

        // Row 1 is properly dominated by row 2, but [0, 1] is still the
        // original-row lexicographic first optimum and therefore the product
        // identity that Core must send to the App validation boundary.
        assert_eq!(selection.row_indices(), &[0, 1]);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn exact_six_line_session_fails_dense_preflight_before_catalog_allocation() {
        let problem =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::six_lines()))
                .expect("lazy six-line descriptor");
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("materialized descriptor");
        assert_eq!(universe.pattern_count() as u128, 35_384_428_800);

        let error = match WasmExactSearchSession::new(&problem) {
            Ok(_) => panic!("dense preflight must reject exact six-line allocation"),
            Err(error) => error,
        };
        let super::WasmExactSearchError::ResourceAdmission(report) = error else {
            panic!("expected typed resource admission evidence, got {error:?}");
        };
        assert!(!report.execution_started());
        assert!(!report.result_complete());
        assert_eq!(
            report.execution_availability().descriptor_pattern_count(),
            Some(35_384_428_800)
        );
        assert_eq!(
            report.execution_availability().dense_pattern_count(),
            Some(35_384_428_800)
        );
        assert_eq!(
            report.execution_availability().required_dense_bytes(),
            Some(4_423_053_600)
        );
        assert_eq!(
            report.execution_availability().required_memory_bytes(),
            Some(4_423_053_600)
        );
    }

    #[test]
    fn distributed_target_preparation_yields_with_monotonic_progress_and_cancel() {
        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_count_policy(PcCountPolicy::CountAll)
            .with_execution_policy(PcExecutionPolicy::default().with_max_patterns(16_385));
        let problem = ProblemCompiler::compile_opening_pc(&query)
            .expect("bounded high-cardinality opening problem");
        let control = ExecutionControl::default();
        let mut producer =
            WasmCpuCandidateProducer::new(&problem).expect("deferred target-index producer");

        assert_eq!(producer.progress().geometry_nodes, 0);
        assert!(matches!(
            producer.advance(&control).expect("first preparation step"),
            WasmCandidateProducerAdvance::Pending
        ));
        let first = producer.progress().geometry_nodes;
        assert!(matches!(
            producer.advance(&control).expect("second preparation step"),
            WasmCandidateProducerAdvance::Pending
        ));
        let second = producer.progress().geometry_nodes;
        assert!(first > 0);
        assert!(second > first);

        control.cancellation.handle().cancel();
        assert!(matches!(
            producer
                .advance(&control)
                .expect("cancelled preparation step"),
            WasmCandidateProducerAdvance::Cancelled
        ));
    }

    #[test]
    fn compact_tiling_rows_round_trip_across_word_boundaries() {
        let mut rows = (0..MAX_BOARD64_PIECES as u32).collect::<Vec<_>>();
        *rows.last_mut().expect("last row") = PACKED_TILING_MAX_ROW_ID;
        let packed_rows = pack_tiling_row_ids(&rows).expect("compact identity");

        for (index, row_id) in rows.iter().copied().enumerate() {
            assert_eq!(
                read_packed_tiling_row(&packed_rows, index),
                u64::from(row_id) + 1
            );
        }
        assert!(packed_rows_are_valid(&packed_rows));
        assert_eq!(core::mem::size_of::<PackedTilingRows>(), 24);
    }

    #[test]
    fn compact_tiling_rows_require_a_canonical_strict_order() {
        assert!(pack_tiling_row_ids(&[3, 3]).is_none());
        assert!(pack_tiling_row_ids(&[4, 3]).is_none());
        assert!(pack_tiling_row_ids(&[PACKED_TILING_MAX_ROW_ID + 1]).is_none());
    }

    #[test]
    fn coverage_summary_b2b_retains_internal_graph_and_coverage_evidence() {
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
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("problem")
            .with_pc_chance_probability_v2_evidence();

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("coverage-summary B2B producer");

        assert_eq!(
            result.field("search_output_policy"),
            Some("coverage-summary")
        );
        assert_eq!(result.coverage_pattern_words(), &[1]);
        let chance_evidence = result
            .pc_chance_coverage_evidence()
            .expect("local cooperative typed Build coverage evidence");
        assert!(chance_evidence.complete());
        assert!(chance_evidence.problem().matches_search_problem(&problem));
        assert_eq!(chance_evidence.row_count(), 1);
        assert_eq!(
            chance_evidence.coverage_union().words(),
            result.coverage_pattern_words()
        );
        assert!(chance_evidence.rows().iter().all(|row| row.row_kind()
            == &clearra_coverage::row::coverage_row_kind::CoverageRowKind::Build));
        assert_eq!(result.solution_coverages().len(), 1);
        assert_eq!(result.normalized_solution_coverages().len(), 1);
        let batch = result
            .exact_scoring_execution_batch()
            .expect("B2B execution evidence batch");
        assert!(batch.complete());
        assert_eq!(batch.graphs().len(), 1);
        assert_eq!(
            batch.graphs()[0].identity(),
            result.solution_coverages()[0].identity()
        );
        assert_eq!(
            result.solution_coverages()[0]
                .covered_patterns()
                .count_ones(),
            1
        );
        assert!(result.normalized_solution_identities().is_empty());
        assert!(result.path_steps().is_empty());
    }

    #[test]
    fn score_portfolio_policy_produces_simultaneous_coverage_and_score_evidence() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("score-portfolio problem")
            .with_pc_score_portfolio_v2_evidence();

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("score-portfolio producer");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
        let coverage = result
            .pc_chance_coverage_evidence()
            .expect("score-portfolio coverage evidence");
        assert!(coverage.complete());
        assert!(coverage.problem().matches_search_problem(&problem));
        assert_eq!(coverage.row_count(), 1);
        let score = result
            .pc_score_problem_evidence()
            .expect("score-portfolio problem evidence");
        assert!(score.matches_search_problem(&problem));
        assert!(result
            .exact_scoring_execution_batch()
            .is_some_and(|batch| batch.complete()));
    }

    #[test]
    fn score_portfolio_retains_every_exact_scoring_candidate_before_app_reduction() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
            PieceWindow::new(5),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(5))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
        let generic_problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("generic score plus minimum-cover problem");
        let generic_result = WasmCpuSearchBackend::execute_with_control(
            &generic_problem,
            &ExecutionControl::default(),
        )
        .expect("generic score plus minimum-cover producer");
        assert_eq!(
            generic_result.bool_field("minimum_cover_complete"),
            Some(true)
        );
        assert_eq!(
            generic_result.bool_field("minimum_cover_proven_minimum"),
            Some(true)
        );
        assert_eq!(
            generic_result.field("minimum_cover_incomplete_reason"),
            Some("none")
        );
        assert!(
            generic_result
                .usize_field("minimum_cover_source_solution_count")
                .is_some_and(
                    |source| source > generic_result.normalized_solution_identities().len()
                ),
            "generic min-cover plus score must retain the historical Core reduction"
        );

        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("multi-candidate score-portfolio problem")
            .with_pc_score_portfolio_v2_evidence();

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("multi-candidate score-portfolio producer");
        let identities = result.normalized_solution_identities();
        let batch = result
            .exact_scoring_execution_batch()
            .expect("complete exact scoring candidate dictionary");

        assert!(
            identities.len() > 1,
            "fixture must exercise the reduction boundary"
        );
        assert_eq!(
            result.usize_field("minimum_cover_source_solution_count"),
            Some(identities.len())
        );
        assert_eq!(
            result.usize_field("minimum_cover_selected_solution_count"),
            Some(identities.len())
        );
        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
        assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(batch.graphs().len(), identities.len());
        assert!(result
            .pc_chance_coverage_evidence()
            .is_some_and(|evidence| evidence.complete()));
        assert!(batch.graphs().iter().enumerate().all(|(index, graph)| {
            graph.candidate_id() == (index + 1) as u64 && graph.identity() == identities[index]
        }));
        assert_eq!(result.solution_coverages().len(), identities.len());
        assert_eq!(
            result.normalized_solution_coverages().len(),
            identities.len()
        );
    }

    #[test]
    fn pc_minimum_cover_product_defers_reduction_and_retains_the_full_source_dictionary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
            PieceWindow::new(5),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(5))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_solution_probability_policy(
            clearra_pc_graph::request::PcSolutionProbabilityPolicy::Include,
        )
        .with_objective(ObjectivePolicy::minimum_cover());
        let generic_problem =
            ProblemCompiler::compile_scenario_pc(&query).expect("generic minimum-cover problem");
        let generic = WasmCpuSearchBackend::execute_with_control(
            &generic_problem,
            &ExecutionControl::default(),
        )
        .expect("generic minimum-cover producer");
        assert_eq!(generic.bool_field("minimum_cover_complete"), Some(true));
        assert_eq!(generic.normalized_solution_keys().len(), 1);

        let product_problem = generic_problem.with_pc_minimum_cover_v2_evidence();
        let product = WasmCpuSearchBackend::execute_with_control(
            &product_problem,
            &ExecutionControl::default(),
        )
        .expect("deferred product minimum-cover producer");
        assert_eq!(product.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            product.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            product.field("minimum_cover_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
        assert_eq!(
            product.field("objective_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
        assert_eq!(product.bool_field("objective_complete"), Some(false));
        assert_eq!(product.normalized_solution_keys().len(), 4);
        assert_eq!(product.normalized_solution_identities().len(), 4);
        assert_eq!(product.solution_coverages().len(), 4);
        assert_eq!(product.normalized_solution_coverages().len(), 4);
        assert_eq!(product.solution_probabilities().len(), 4);
        assert_eq!(
            product.usize_field("minimum_cover_source_solution_count"),
            Some(4)
        );
        assert_eq!(
            product.usize_field("minimum_cover_selected_solution_count"),
            Some(4)
        );
        assert!(product
            .pc_chance_coverage_evidence()
            .is_some_and(|evidence| evidence.complete()));
        assert!(product
            .normalized_solution_keys()
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert!(product
            .solution_probabilities()
            .iter()
            .zip(product.normalized_solution_keys())
            .all(|(probability, key)| probability.solution_key() == key));
    }

    #[test]
    fn score_portfolio_terminal_projection_counts_both_problem_evidence_owners() {
        let base_query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll);
        let score_problem = ProblemCompiler::compile_scenario_pc(
            &base_query
                .clone()
                .with_objective(ObjectivePolicy::all().with_score_summary()),
        )
        .expect("score-only problem");
        let portfolio_problem = ProblemCompiler::compile_scenario_pc(
            &base_query.with_objective(ObjectivePolicy::minimum_cover().with_score_summary()),
        )
        .expect("score-portfolio problem")
        .with_pc_score_portfolio_v2_evidence();

        assert_eq!(
            checked_typed_problem_evidence_upper_bound(&score_problem, 4096),
            Some(4096)
        );
        assert_eq!(
            checked_typed_problem_evidence_upper_bound(&portfolio_problem, 4096),
            Some(8192)
        );
        assert_eq!(
            checked_typed_problem_evidence_upper_bound(&portfolio_problem, u128::MAX),
            None
        );
    }

    #[test]
    fn generic_coverage_summary_never_retains_private_chance_rows() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique);
        let problem =
            ProblemCompiler::compile_scenario_percent(&query).expect("generic percent problem");
        let session = WasmExactSearchSession::new(&problem).expect("generic percent session");
        assert!(!session.pc_chance_coverage_evidence_available);
        assert!(!session.coverage_rows_complete);
        assert_eq!(session.coverage_rows.capacity(), 0);
        drop(session);

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("generic percent result");
        assert!(result.pc_chance_coverage_evidence().is_none());
    }

    fn distributed_worker_scalar_fields(
        coverage_row_count: usize,
        packing_candidate_count: usize,
        build_variant_count: u128,
    ) -> Vec<(String, String)> {
        [
            ("coverage_pattern_count", "1".to_owned()),
            ("coverage_row_count", coverage_row_count.to_string()),
            (
                "packing_candidate_count",
                packing_candidate_count.to_string(),
            ),
            ("pattern_verified_execution_count", "0".to_owned()),
            ("build_variant_count", build_variant_count.to_string()),
            ("count_complete", "true".to_owned()),
            ("execution_constraint_materialized", "true".to_owned()),
            ("peak_build_order_nodes", "0".to_owned()),
            ("total_build_order_nodes", "0".to_owned()),
            ("coverage_product_words", "0".to_owned()),
            ("coverage_product_states", "0".to_owned()),
            ("coverage_product_edge_checks", "0".to_owned()),
            ("realization_feasibility_states", "0".to_owned()),
            (
                "realization_feasibility_rejected_candidates",
                "0".to_owned(),
            ),
            ("peak_reachability_states", "0".to_owned()),
            ("total_reachability_states", "0".to_owned()),
            ("resource_peak_cpu_bytes", "0".to_owned()),
            ("piece_language_coverage_cache_hits", "0".to_owned()),
            ("piece_language_coverage_cache_misses", "0".to_owned()),
            ("standard_bag_symbolic_cache_hits", "0".to_owned()),
            ("standard_bag_symbolic_cache_misses", "0".to_owned()),
            ("reachability_lock_queries", "0".to_owned()),
            ("reachability_harddrop_queries", "0".to_owned()),
            ("reachability_harddrop_hits", "0".to_owned()),
            ("reachability_cache_reachable_hits", "0".to_owned()),
            ("reachability_cache_unreachable_hits", "0".to_owned()),
            ("reachability_cache_key_misses", "0".to_owned()),
            ("reachability_partial_searches", "0".to_owned()),
            ("reachability_exhaustive_searches", "0".to_owned()),
            ("representative_candidate_ordinal", String::new()),
            ("representative_candidate_id", String::new()),
            ("representative_pattern_id", String::new()),
            ("resource_truncated", "false".to_owned()),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
    }

    fn typed_chance_worker_fixture() -> (clearra_problem::SearchProblem, CoreExecutionResult) {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("problem")
            .with_pc_chance_probability_v2_evidence();
        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .expect("materialized chance universe");
        let rows = vec![CoverageRow::new_with_piece_source(
            7,
            CoverageRowKind::Build,
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            clearra_coverage::pattern::pattern_bitset::PatternBitSet::from_words(
                universe.pattern_count(),
                vec![1],
            )
            .expect("one-pattern chance row"),
        )];
        let transport = DistributedPcChanceCoverageRows::try_from_untrusted_rows(
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            universe.pattern_count(),
            rows,
            true,
        )
        .expect("valid worker transport");
        let mut fields = distributed_worker_scalar_fields(1, 1, 1);
        fields.extend([
            ("probability_complete".to_owned(), "true".to_owned()),
            (
                "resource_probability_complete".to_owned(),
                "true".to_owned(),
            ),
        ]);
        (
            problem,
            CoreExecutionResult::new(fields, Vec::new())
                .with_coverage_pattern_words(vec![1])
                .with_distributed_pc_chance_coverage_rows(transport),
        )
    }

    fn typed_chance_distributed_test_guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn distributed_typed_chance_rows_rebind_to_the_coordinator_problem() {
        let _guard = typed_chance_distributed_test_guard();
        let (problem, worker) = typed_chance_worker_fixture();
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");
        coordinator
            .absorb_distributed_result(&worker)
            .expect("identity-bound worker rows");
        let result = match coordinator
            .complete_external_geometry(0, 0)
            .expect("complete coordinator")
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Pending | ExactSearchAdvance::Cancelled => {
                panic!("coordinator must complete")
            }
        };
        let evidence = result
            .pc_chance_coverage_evidence()
            .expect("coordinator-bound chance evidence");
        assert!(evidence.complete());
        assert!(evidence.problem().matches_search_problem(&problem));
        assert_eq!(
            evidence.coverage_union().words(),
            result.coverage_pattern_words()
        );
    }

    #[test]
    fn distributed_typed_chance_rows_reject_foreign_incomplete_and_duplicate_batches() {
        let _guard = typed_chance_distributed_test_guard();
        let (problem, worker) = typed_chance_worker_fixture();
        let transport = worker
            .distributed_pc_chance_coverage_rows()
            .expect("worker transport");

        let foreign_universe =
            clearra_coverage::universe::pattern_universe_id::PatternUniverseId::new(
                transport.pattern_universe_id().get().saturating_add(1),
            );
        let foreign_rows = transport
            .rows()
            .iter()
            .map(|row| {
                CoverageRow::new_with_piece_source(
                    row.candidate_id(),
                    CoverageRowKind::Build,
                    row.piece_source_id(),
                    foreign_universe,
                    row.pattern_weight_model_id(),
                    row.coverage_bits().clone(),
                )
            })
            .collect();
        let foreign = DistributedPcChanceCoverageRows::try_from_untrusted_rows(
            transport.piece_source_id(),
            foreign_universe,
            transport.pattern_weight_model_id(),
            transport.pattern_count(),
            foreign_rows,
            true,
        )
        .expect("well-formed foreign transport");
        let foreign_worker = worker
            .clone()
            .without_pc_chance_transient_evidence()
            .with_distributed_pc_chance_coverage_rows(foreign);
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("foreign coordinator");
        assert!(matches!(
            coordinator.absorb_distributed_result(&foreign_worker),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_identity_mismatch"
            ))
        ));
        drop(coordinator);

        let incomplete = DistributedPcChanceCoverageRows::try_from_untrusted_rows(
            transport.piece_source_id(),
            transport.pattern_universe_id(),
            transport.pattern_weight_model_id(),
            transport.pattern_count(),
            transport.rows().to_vec(),
            false,
        )
        .expect("well-formed incomplete transport");
        let incomplete_worker = worker
            .clone()
            .without_pc_chance_transient_evidence()
            .with_distributed_pc_chance_coverage_rows(incomplete);
        let mut coordinator = WasmExactSearchSession::new_external_geometry(&problem)
            .expect("incomplete coordinator");
        assert!(matches!(
            coordinator.absorb_distributed_result(&incomplete_worker),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_incomplete"
            ))
        ));
        drop(coordinator);

        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("duplicate coordinator");
        coordinator
            .absorb_distributed_result(&worker)
            .expect("first batch");
        assert!(matches!(
            coordinator.absorb_distributed_result(&worker),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_candidate_duplicate"
            ))
        ));
        drop(coordinator);

        let duplicate_authority = worker
            .clone()
            .with_additional_fields(vec![("count_complete".to_owned(), "true".to_owned())]);
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("duplicate authority");
        assert!(matches!(
            coordinator.absorb_distributed_result(&duplicate_authority),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_scalar_authority_invalid"
            ))
        ));
        drop(coordinator);

        let missing_scalar = worker
            .clone()
            .without_field_for_test("resource_peak_cpu_bytes");
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("missing scalar");
        assert!(matches!(
            coordinator.absorb_distributed_result(&missing_scalar),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_scalar_authority_invalid"
            ))
        ));
        assert_eq!(coordinator.covered_patterns.count_ones(), 0);
        drop(coordinator);

        let missing_authority = worker
            .clone()
            .without_field_for_test("resource_probability_complete");
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("missing authority");
        assert!(matches!(
            coordinator.absorb_distributed_result(&missing_authority),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_chance_evidence_incomplete"
            ))
        ));
        drop(coordinator);

        let noncanonical_count = replace_worker_field(worker.clone(), "coverage_row_count", "01");
        let mut coordinator = WasmExactSearchSession::new_external_geometry(&problem)
            .expect("noncanonical authority");
        assert!(matches!(
            coordinator.absorb_distributed_result(&noncanonical_count),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_scalar_authority_invalid"
            ))
        ));
        drop(coordinator);

        let dirty_aggregate = worker
            .clone()
            .with_coverage_pattern_words(vec![1 | (1_u64 << 63)]);
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("dirty aggregate");
        assert!(matches!(
            coordinator.absorb_distributed_result(&dirty_aggregate),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_coverage_invalid"
            ))
        ));
        drop(coordinator);

        let duplicate_pattern_count = worker
            .clone()
            .with_additional_fields(vec![("coverage_pattern_count".to_owned(), "1".to_owned())]);
        let mut coordinator = WasmExactSearchSession::new_external_geometry(&problem)
            .expect("duplicate pattern count");
        assert!(matches!(
            coordinator.absorb_distributed_result(&duplicate_pattern_count),
            Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_result_pattern_count_missing"
            ))
        ));
    }

    #[test]
    fn distributed_aggregate_never_synthesizes_missing_typed_chance_rows() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("problem")
            .with_pc_chance_probability_v2_evidence();
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");
        let worker =
            CoreExecutionResult::new(distributed_worker_scalar_fields(1, 1, 1), Vec::new())
                .with_coverage_pattern_words(vec![1]);

        coordinator
            .absorb_distributed_result(&worker)
            .expect("aggregate public worker result");
        let result = match coordinator
            .complete_external_geometry(0, 0)
            .expect("complete coordinator")
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Pending | ExactSearchAdvance::Cancelled => {
                panic!("coordinator must complete")
            }
        };

        assert_eq!(result.coverage_pattern_words(), &[1]);
        assert!(result.pc_chance_coverage_evidence().is_none());
    }

    fn minimum_cover_worker_fixture() -> (clearra_problem::SearchProblem, CoreExecutionResult) {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::minimum_cover());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("minimum-cover problem")
            .with_pc_minimum_cover_v2_evidence();
        let worker =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("minimum-cover worker result");
        (problem, worker)
    }

    #[cfg(feature = "parallel")]
    fn native_parallel_minimum_problem() -> clearra_problem::SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
            PieceWindow::new(5),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(5))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::minimum_cover());
        ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_pc_minimum_cover_v2_evidence()
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn native_parallel_minimum_cover_retains_complete_problem_bound_source_rows() {
        let problem = native_parallel_minimum_problem();
        let mut session = WasmExactSearchSession::new(&problem).expect("minimum session");
        let result = session
            .execute_parallel_if_worthwhile(2, &ExecutionControl::default())
            .expect("parallel minimum source")
            .expect("fixture takes native family workers");
        let evidence = result
            .pc_chance_coverage_evidence()
            .expect("typed minimum source evidence");
        assert!(evidence.complete());
        assert!(evidence.problem().matches_search_problem(&problem));
        assert_eq!(result.normalized_solution_coverages().len(), 4);
        assert_eq!(
            evidence.coverage_union().words(),
            result.coverage_pattern_words()
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn native_parallel_minimum_cover_rejects_missing_rows_and_false_unions() {
        let problem = native_parallel_minimum_problem();
        let mut session = WasmExactSearchSession::new(&problem).unwrap();
        let geometry =
            core::mem::replace(&mut session.geometry, super::GeometrySearch::placeholder());
        let decision = super::parallel_search::execute_if_worthwhile(
            std::sync::Arc::clone(&session.problem),
            std::sync::Arc::clone(&session.catalog),
            geometry,
            ExecutionControl::default(),
            2,
            false,
        )
        .expect("native source");
        let super::ParallelSearchDecision::Completed(mut outcome) = decision else {
            panic!("fixture must exercise the native family workers");
        };
        assert!(session
            .validate_parallel_minimum_cover_source(&mut outcome)
            .unwrap());
        let row = outcome.solution_coverage.pop().expect("four-row source");
        assert!(session
            .validate_parallel_minimum_cover_source(&mut outcome)
            .is_err());
        outcome.solution_coverage.push(row);
        let union = outcome.covered_patterns.clone();
        outcome.covered_patterns = PatternBitSet::new(union.pattern_count());
        assert!(session
            .validate_parallel_minimum_cover_source(&mut outcome)
            .is_err());
        outcome.covered_patterns = union;
        outcome.count_complete = false;
        assert!(!session
            .validate_parallel_minimum_cover_source(&mut outcome)
            .unwrap());
        outcome.count_complete = true;
        outcome.truncated_reason = Some("fixture-incomplete");
        assert!(!session
            .validate_parallel_minimum_cover_source(&mut outcome)
            .unwrap());
    }

    fn replace_worker_field(
        worker: CoreExecutionResult,
        key: &str,
        value: &str,
    ) -> CoreExecutionResult {
        let mut fields = worker.summary_fields();
        fields
            .iter_mut()
            .find(|(name, _)| name == key)
            .expect("worker field")
            .1 = value.to_owned();
        worker.with_replaced_fields(fields)
    }

    #[test]
    fn distributed_minimum_cover_rebuilds_problem_bound_rows_from_canonical_source() {
        let (problem, worker) = minimum_cover_worker_fixture();
        let source_count = worker.normalized_solution_coverages().len();
        assert_ne!(source_count, 0, "fixture owns a canonical source row");
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");

        coordinator
            .absorb_distributed_result(&worker)
            .expect("canonical source coverage is coordinator-replayable");
        let result = match coordinator
            .complete_external_geometry(0, 0)
            .expect("complete coordinator")
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Pending | ExactSearchAdvance::Cancelled => {
                panic!("coordinator must complete")
            }
        };
        let merged = result
            .pc_chance_coverage_evidence()
            .expect("coordinator-owned typed coverage evidence");
        assert!(merged.complete());
        assert_eq!(merged.row_count(), source_count);
        assert_eq!(
            merged.coverage_union().words(),
            worker.coverage_pattern_words()
        );
    }

    #[test]
    fn distributed_minimum_cover_rejects_incomplete_worker_source() {
        let (problem, worker) = minimum_cover_worker_fixture();
        let worker = replace_worker_field(worker, "count_complete", "false");
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");

        let error = coordinator
            .absorb_distributed_result(&worker)
            .expect_err("incomplete worker source must fail closed");

        assert!(matches!(
            error,
            super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_minimum_source_incomplete"
            )
        ));
    }

    #[test]
    fn distributed_minimum_cover_rejects_source_union_tamper() {
        let (problem, worker) = minimum_cover_worker_fixture();
        let (source_identity, source_pattern_count) = worker
            .solution_coverages()
            .first()
            .map(|source| (source.identity(), source.covered_patterns().pattern_count()))
            .expect("source coverage");
        let worker = worker.with_solution_coverages(vec![crate::SolutionCoverage::new(
            source_identity,
            clearra_coverage::pattern::pattern_bitset::PatternBitSet::new(source_pattern_count),
        )]);
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");

        let error = coordinator
            .absorb_distributed_result(&worker)
            .expect_err("source union tamper must fail closed");

        assert!(matches!(
            error,
            super::WasmExactSearchError::InvalidProblem(
                "wasm_distributed_pc_minimum_source_coverage_mismatch"
            )
        ));
        assert!(
            coordinator.buildable_identities.is_empty(),
            "a rejected partial must not commit candidate identity authority"
        );
        assert!(
            coordinator
                .solution_coverage
                .as_ref()
                .is_some_and(|coverage| coverage.is_empty()),
            "a rejected partial must not commit candidate coverage authority"
        );
        assert_eq!(
            coordinator.covered_patterns.count_ones(),
            0,
            "a rejected partial must not commit aggregate coverage"
        );
    }

    #[test]
    fn distributed_minimum_cover_accepts_complete_empty_filtered_standard_bag_source() {
        // The empty supplied-solution allow-list makes the producer source
        // deterministically empty. This fixture exercises the coordinator's
        // complete zero-row boundary; it must not depend on whether an
        // unrestricted two-line opening happens to have a legal PC.
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::minimum_cover())
        .with_allowed_colored_solution_identities(std::iter::empty());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("filtered one-piece Standard7Bag scenario")
            .with_pc_minimum_cover_v2_evidence();
        let worker =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("complete empty filtered worker result");
        assert!(worker.solution_coverages().is_empty());
        assert!(worker.normalized_solution_coverages().is_empty());
        assert_eq!(
            worker.usize_field("minimum_cover_source_solution_count"),
            Some(0)
        );
        assert_eq!(
            worker
                .coverage_pattern_words()
                .iter()
                .copied()
                .fold(0, |union, word| union | word),
            0
        );
        let mut coordinator =
            WasmExactSearchSession::new_external_geometry(&problem).expect("coordinator");

        coordinator
            .absorb_distributed_result(&worker)
            .expect("complete empty source remains authoritative");
        let result = match coordinator
            .complete_external_geometry(0, 0)
            .expect("complete empty coordinator")
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Pending | ExactSearchAdvance::Cancelled => {
                panic!("empty coordinator must complete")
            }
        };
        let evidence = result
            .pc_chance_coverage_evidence()
            .expect("complete empty evidence");
        assert!(evidence.complete());
        assert!(evidence.rows().is_empty());
        assert_eq!(evidence.coverage_union().count_ones(), 0);
    }

    #[test]
    fn observation_policy_marks_candidate_rows_incomplete_before_execution() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("problem")
            .with_pc_chance_probability_v2_evidence();

        let session = WasmExactSearchSession::new(&problem).expect("observation session");

        assert!(!session.coverage_rows_complete);
        assert!(session.pc_chance_coverage_evidence_available);
        drop(session);

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("observation result");
        let evidence = result
            .pc_chance_coverage_evidence()
            .expect("observation retains only incomplete candidate rows");
        assert!(!evidence.complete());
        assert!(evidence.problem().matches_search_problem(&problem));
    }

    #[test]
    fn tiling_result_exposes_a_complete_public_not_calculated_probability_invariant() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_objective(ObjectivePolicy::tiling());
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("tiling problem");

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("tiling result");

        assert_eq!(result.field("coverage_probability"), Some("not-calculated"));
        assert_eq!(result.bool_field("probability_calculated"), Some(false));
        assert_eq!(result.bool_field("probability_complete"), Some(false));
        assert_eq!(
            result.bool_field("supply_probability_complete"),
            Some(false)
        );
        assert_eq!(
            result.bool_field("resource_probability_complete"),
            Some(false)
        );
        assert!(result.field("actual_solution_set_contract").is_none());
        assert!(result.field("packing_source_raw_geometry").is_none());
        assert!(result
            .field("tiling_materialization_memory_admission_accounted")
            .is_none());
        assert!(result.pc_tiling_memory_admission_evidence().is_none());
        assert!(result.pc_chance_coverage_evidence().is_none());
    }

    #[test]
    fn wasm_result_preserves_builtin_rule_and_kick_identity() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_rule(clearra_rules::profile::builtin_rules::srs_x());
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("SRS-X problem");

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("SRS-X result");

        assert_eq!(result.field("rule_profile"), Some("srs-x"));
        assert_eq!(result.field("kick_profile"), Some("srs-x"));
        assert_eq!(result.field("effective_kick_model"), Some("srs-x"));
        assert_eq!(result.bool_field("verified_kick_profile"), Some(true));
        assert_eq!(
            result.usize_field("kick_profile_transition_count"),
            Some(80)
        );
    }

    #[test]
    fn finite_terminal_hold_fixed_and_generic_witnesses_match_the_micro_oracle_set_and_hash() {
        fn execute(queue: PcQueueInput) -> crate::CoreExecutionResult {
            let first_o = 0x0c03u64;
            let second_o = 0x300cu64;
            let initial_mask = 0x0f_ffffu64 & !(first_o | second_o);
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, initial_mask),
                queue,
                PieceWindow::new(2),
            )
            .with_hold_piece(Some(PieceKind::O))
            .with_exact_pieces(Some(2));
            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
            assert!(problem.supply().projects_unplaced_lookahead());
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("terminal projection search")
        }

        let first_o = 0x0c03u64;
        let second_o = 0x300cu64;
        let initial_mask = 0x0f_ffffu64 & !(first_o | second_o);
        let oracle = StandardBoard64TilingIdentity::from_placements(
            initial_mask,
            [
                PiecePlacementMask::new(PieceKind::O, first_o),
                PiecePlacementMask::new(PieceKind::O, second_o),
            ],
        )
        .expect("independent two-O micro oracle");
        let oracle_hash =
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&[oracle]);

        let fixed = execute(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
        ])));
        let generic = execute(PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("[O]!", 1).expect("single O pattern"),
        ));

        for result in [&fixed, &generic] {
            assert_eq!(result.normalized_solution_identities(), &[oracle]);
            assert_eq!(
                result.field("normalized_solution_set_hash"),
                Some(oracle_hash.as_str())
            );
            assert_eq!(result.path_steps().len(), 2);
            assert_eq!(
                result
                    .path_steps()
                    .iter()
                    .filter(|step| step.hold() == "release-held-at-terminal")
                    .count(),
                1
            );
            assert_eq!(
                result.path_steps().last().map(crate::CorePathStep::hold),
                Some("release-held-at-terminal")
            );
        }
        assert_eq!(
            fixed.normalized_solution_identities(),
            generic.normalized_solution_identities()
        );
        assert_eq!(
            fixed.field("normalized_solution_set_hash"),
            generic.field("normalized_solution_set_hash")
        );
    }

    fn execute_terminal_supply_p0_distributed(
        problem: &clearra_problem::SearchProblem,
    ) -> CoreExecutionResult {
        let control = ExecutionControl::default();
        let mut producer = WasmCpuCandidateProducer::new(problem).expect("P0 producer");
        assert!(producer.verification_required());
        let mut verifiers = [
            WasmDistributedVerifier::new(problem).expect("P0 verifier zero"),
            WasmDistributedVerifier::new(problem).expect("P0 verifier one"),
        ];
        let mut packet_count = 0usize;
        let summary = loop {
            match producer.advance(&control).expect("P0 producer advance") {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => {
                    let verifier_index = packet_count % verifiers.len();
                    verifiers[verifier_index]
                        .consume(&packet, &control)
                        .expect("P0 distributed verify");
                    packet_count = packet_count.saturating_add(1);
                }
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => panic!("P0 producer cancelled"),
            }
        };
        assert_eq!(packet_count, summary.candidate_count);

        let mut merger = producer.into_merger().expect("P0 merger");
        for verifier in &mut verifiers {
            let result = verifier.finish().expect("P0 verifier finish");
            merger.absorb(&result).expect("P0 merge worker result");
        }
        merger
            .finish(&summary, verifiers.len())
            .expect("P0 distributed finish")
    }

    #[test]
    fn terminal_supply_p0_fixed_generic_serial_and_distributed_share_exact_18_set_and_hash() {
        let fixed_problem = terminal_supply_p0_fixed_problem();
        let generic_problem = terminal_supply_p0_generic_problem();
        let fixed = WasmCpuSearchBackend::execute_with_control(
            &fixed_problem,
            &ExecutionControl::default(),
        )
        .expect("P0 fixed serial");
        let generic = WasmCpuSearchBackend::execute_with_control(
            &generic_problem,
            &ExecutionControl::default(),
        )
        .expect("P0 generic serial");
        let distributed = execute_terminal_supply_p0_distributed(&generic_problem);
        let expected_identities = terminal_supply_p0_expected_identities();

        for result in [&fixed, &generic, &distributed] {
            let identities = result.normalized_solution_identities();
            assert_eq!(identities, expected_identities);
            assert_eq!(identities.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
            assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
            let calculated_hash =
                normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                    identities,
                );
            assert_eq!(
                result.field("normalized_solution_set_hash"),
                Some(calculated_hash.as_str())
            );
            assert_eq!(
                calculated_hash.as_str(),
                TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
            );
            assert_eq!(
                result.usize_field("unique_solution_count"),
                Some(TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT)
            );
        }
        assert_eq!(
            fixed.normalized_solution_identities(),
            generic.normalized_solution_identities()
        );
        assert_eq!(
            fixed.normalized_solution_identities(),
            distributed.normalized_solution_identities()
        );
    }

    #[test]
    fn distributed_tiling_chunk_replay_is_exact_and_idempotent() {
        fn chunk(row_id: u32, sequence: u32, root_complete: bool) -> WasmTilingRootChunk {
            let packed = pack_tiling_row_ids(&[row_id]).expect("packed row");
            WasmTilingRootChunk::from_wire_parts(
                0,
                0,
                sequence,
                root_complete,
                vec![WasmPackedTilingIdentity::new(u64::from(row_id), packed)],
                usize::from(root_complete),
                Some(u128::from(root_complete)),
                usize::from(root_complete) * 11,
                usize::from(root_complete) * 7,
                usize::from(root_complete) * 5,
                usize::from(root_complete) * 3,
                usize::from(root_complete) * 2,
                usize::from(root_complete),
            )
        }

        let mut run = DistributedTilingRootRun::default();
        let first = chunk(3, 0, false);
        let last = chunk(4, 1, true);
        let exact_reservation = false;
        assert!(run
            .absorb_chunk(&first, exact_reservation)
            .expect("first chunk"));
        assert!(!run
            .absorb_chunk(&first, exact_reservation)
            .expect("first replay"));
        assert!(run
            .absorb_chunk(&last, exact_reservation)
            .expect("last chunk"));
        assert!(!run
            .absorb_chunk(&first, exact_reservation)
            .expect("replay after completion"));
        assert!(!run
            .absorb_chunk(&last, exact_reservation)
            .expect("last replay"));
        assert_eq!(run.identities.len(), 2);

        let mismatched_replay = chunk(5, 0, false);
        assert!(run
            .absorb_chunk(&mismatched_replay, exact_reservation)
            .is_err());
        assert_eq!(run.identities.len(), 2);
    }
}
// SRP rationale: this module has one behavior-level change reason: canonical exact-search result accumulation and finalization.
