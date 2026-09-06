// SRP rationale: this module has one change reason: maintain the exact
// minimum-cardinality set-cover proof, including its lossless reductions,
// admissible bounds, memoization, and governed-memory accounting as one
// correctness unit.
use crate::pattern::pattern_bitset::PatternBitSet;

use super::exact_dual_lower_bound::{
    CertifiedResidualDual, DualProposalWorkspace, MAX_DUAL_INCIDENCE_COUNT,
    checked_maximum_persistent_dual_certificate_bytes,
    checked_maximum_residual_dual_workspace_bytes, checked_residual_dual_memory_projection,
    should_attempt_residual_dual, should_prepare_root_dual,
};

#[cfg(feature = "diagnostic-probes")]
use std::time::Instant;

const MAX_EXACT_NODES_PER_ADVANCE: u64 = 16;

#[cfg(feature = "diagnostic-probes")]
static DIAGNOSTIC_REPAIR_WORD_MASKS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

#[cfg(feature = "diagnostic-probes")]
static DIAGNOSTIC_LEGACY_WASM32_RANDOM_CHOICE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "diagnostic-probes")]
static DIAGNOSTIC_CONDITIONAL_ROW_PRUNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Same-binary A/B only. Set before creating proof workers; each exact search
/// snapshots the switch. It changes neither certificate construction nor
/// the existing diagnostics/wire schema.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
pub fn set_diagnostic_conditional_row_pruning(enabled: bool) {
    DIAGNOSTIC_CONDITIONAL_ROW_PRUNING.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Proposal-only same-binary A/B; defaults to off. Set before starting proof
/// workers. A workspace snapshots it once and retains it when cloned. This
/// neither enables an ordinary product path nor changes exact proof rules.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
pub fn set_diagnostic_residual_warm_seed(enabled: bool) {
    super::exact_dual_lower_bound::set_diagnostic_residual_warm_seed(enabled);
}

/// Selects the legacy scorer only for controlled, same-binary diagnostics.
/// Set this before starting solver threads; each repair session snapshots it.
/// Ordinary product builds contain only the word-mask scorer.
#[cfg(feature = "diagnostic-probes")]
#[doc(hidden)]
pub fn set_diagnostic_repair_word_masks(enabled: bool) {
    DIAGNOSTIC_REPAIR_WORD_MASKS.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Reproduces the former wasm32 narrowing-before-modulo heuristic only in
/// diagnostic builds. Set before starting workers; sessions snapshot the mode.
/// This option changes no exact proof rule and supplies no witness authority.
#[cfg(feature = "diagnostic-probes")]
#[doc(hidden)]
pub fn set_diagnostic_legacy_wasm32_random_choice(enabled: bool) {
    DIAGNOSTIC_LEGACY_WASM32_RANDOM_CHOICE.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
struct CoverRandomChoice {
    #[cfg(any(test, feature = "diagnostic-probes"))]
    legacy_wasm32: bool,
}

impl CoverRandomChoice {
    fn for_new_session() -> Self {
        Self {
            #[cfg(feature = "diagnostic-probes")]
            legacy_wasm32: DIAGNOSTIC_LEGACY_WASM32_RANDOM_CHOICE
                .load(std::sync::atomic::Ordering::Relaxed),
            #[cfg(all(test, not(feature = "diagnostic-probes")))]
            legacy_wasm32: false,
        }
    }

    #[inline]
    fn index(self, random: u64, candidate_count: usize) -> usize {
        // Callers have already established a nonempty candidate family.
        // Take the remainder in the PRNG's fixed width before converting the
        // bounded result; truncating first changed native/WASM search order.
        debug_assert_ne!(candidate_count, 0);
        #[cfg(any(test, feature = "diagnostic-probes"))]
        let random = if self.legacy_wasm32 {
            u64::from(random as u32)
        } else {
            random
        };
        let divisor = candidate_count as u64;
        (random % divisor) as usize
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverResult {
    row_indices: Vec<usize>,
    covered_patterns: PatternBitSet,
    complete: bool,
}

/// One replay-validated cover whose cardinality is at most the requested
/// limit. Unlike [`ExactMinimumCoverResult`], this result does not claim that
/// the returned cover is globally minimum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCoverAtMostResult {
    row_indices: Vec<usize>,
    covered_patterns: PatternBitSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactCoverAtMostDecision {
    Found(ExactCoverAtMostResult),
    ProvedNone,
    Cancelled,
}

impl ExactCoverAtMostResult {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub fn into_parts(self) -> (Vec<usize>, PatternBitSet) {
        (self.row_indices, self.covered_patterns)
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.row_indices.capacity() as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)?
            .checked_add(self.covered_patterns.checked_storage_retained_bytes()?)
    }
}

impl ExactMinimumCoverResult {
    /// Private bridge from an exhaustive, source-bound negative AtMost(k-1)
    /// query and an independently replayed k-cover. No public caller can
    /// inject a cardinality or construct minimum proof authority.
    pub(super) fn from_parallel_negative(
        coordinator: &super::exact_at_most_parallel::ExactAtMostCoordinator,
        row_indices: Vec<usize>,
    ) -> Option<Self> {
        use super::exact_at_most_parallel::ExactAtMostParallelDecision;
        let query = coordinator.query();
        if coordinator.decision() != &ExactAtMostParallelDecision::ProvedNone
            || query.limit().checked_add(1)? != row_indices.len()
            || row_indices.windows(2).any(|pair| pair[0] >= pair[1])
            || row_indices.iter().any(|&row| row >= query.rows().len())
            || (0..query.required().word_count()).any(|word| {
                row_indices
                    .iter()
                    .fold(0, |union, &row| union | query.rows()[row].word_at(word))
                    & query.required().word_at(word)
                    != query.required().word_at(word)
            })
        {
            return None;
        }
        Some(Self {
            row_indices,
            covered_patterns: query.required().clone(),
            complete: true,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactCoverSearchGoal {
    Minimum,
    AtMost(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactCoverIncumbentPolicy {
    Standard,
    WitnessAssisted,
    WitnessAssistedAfterRawSearch,
}

#[derive(Clone, Debug)]
enum ExactCoverInternalDecision {
    Found(ExactMinimumCoverResult),
    ProvedNone,
    Cancelled,
}

/// One bounded advance of an exact minimum-cover proof.
///
/// `visited_nodes` is retained for API compatibility, but counts deterministic
/// work units across every session phase: one preparation transition, one
/// randomized-incumbent trial, or one branch-and-bound node. A pending result
/// therefore makes consumed cooperative work observable without exposing the
/// private preparation or DFS state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverSessionAdvance {
    Pending {
        visited_nodes: u64,
    },
    Found {
        result: ExactMinimumCoverResult,
        visited_nodes: u64,
    },
    ProvedNone {
        visited_nodes: u64,
    },
    Cancelled {
        visited_nodes: u64,
    },
    Finished,
}

/// Probe-only counters for separating exact DFS work from optional residual
/// dual acceleration. These counters are observability, never proof input.
#[doc(hidden)]
#[cfg(any(test, feature = "diagnostic-probes"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactMinimumCoverResidualDiagnostics {
    pub search_nodes: u64,
    pub proposal_attempts: u64,
    pub proposal_iterations: u64,
    pub certified_prunes: u64,
    pub remaining_proposal_iterations: usize,
    /// Buckets for certified-dual gaps 1, 2, 3, and 4 respectively.
    pub proposal_attempts_by_dual_gap: [u64; 4],
    pub proposal_iterations_by_dual_gap: [u64; 4],
    pub certified_prunes_by_dual_gap: [u64; 4],
    /// Buckets for search depths 6--9, 10--13, and 14+ respectively.
    pub proposal_attempts_by_depth: [u64; 3],
    pub proposal_iterations_by_depth: [u64; 3],
    pub certified_prunes_by_depth: [u64; 3],
    /// Certified prunes returned at checkpoints 25, 50, ..., 200.
    pub certified_prunes_by_checkpoint: [u64; 8],
}

/// Separate opt-in probe counters: do not extend the existing HotCost/wire
/// schema merely to evaluate this root-certificate branch filter.
#[doc(hidden)]
#[cfg(any(test, feature = "diagnostic-probes"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactMinimumCoverConditionalRowDiagnostics {
    pub assessed_nodes: u64,
    pub candidate_rows: u64,
    pub examined_weights: u64,
    pub pruned_rows: u64,
}

/// Separate experiment counters; not part of the stable HotCost/wire schema.
#[doc(hidden)]
#[cfg(any(test, feature = "diagnostic-probes"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactMinimumCoverWarmSeedDiagnostics {
    pub attempts: u64,
    pub applied: u64,
    pub matched_patterns: u64,
    pub seeded_constraints: u64,
}

/// Probe-only snapshot of the cooperative AtMost cursor. It exposes no proof
/// data and is used only to verify that an in-flight witness shortcut advances
/// monotonically instead of restarting at a page boundary.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverSessionDiagnostics {
    pub phase: &'static str,
    pub witness_phase: Option<&'static str>,
    pub supporter_position: Option<usize>,
    pub randomized_trial: Option<usize>,
    pub randomized_trial_end: Option<usize>,
    pub breakout_attempted_swaps: Option<usize>,
    pub search_nodes: u64,
}

/// Probe-only result of the legacy positive witness shortcut without its
/// exact-search fallback. A miss has no negative authority.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverWitnessShortcutDiagnostics {
    Found(Vec<usize>),
    Miss,
    Cancelled,
}

/// Probe-only wall-cost attribution for one exact-search session. Every field
/// is excluded from production builds and carries no proof authority.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactMinimumCoverHotCostDiagnostics {
    pub memo_calls: u64,
    pub memo_nanoseconds: u128,
    pub rarest_support_calls: u64,
    pub rarest_support_nanoseconds: u128,
    pub top_gain_calls: u64,
    pub top_gain_nanoseconds: u128,
    pub root_certificate_calls: u64,
    pub root_certificate_nanoseconds: u128,
    pub packing_calls: u64,
    pub packing_nanoseconds: u128,
    pub branch_calls: u64,
    pub branch_nanoseconds: u128,
    pub residual_prepare_calls: u64,
    pub residual_prepare_nanoseconds: u128,
    pub mirror_prox_iterations: u64,
    pub mirror_prox_nanoseconds: u128,
    pub softmax_p_nanoseconds: u128,
    pub softmax_q_nanoseconds: u128,
    pub softmax_middle_p_nanoseconds: u128,
    pub softmax_middle_q_nanoseconds: u128,
    pub softmax_p_entries: u64,
    pub softmax_p_cutoff_entries: u64,
    pub softmax_q_entries: u64,
    pub softmax_q_cutoff_entries: u64,
    pub softmax_q_row_incidences: u64,
    pub softmax_q_cutoff_row_incidences: u64,
    pub softmax_middle_p_entries: u64,
    pub softmax_middle_p_cutoff_entries: u64,
    pub softmax_middle_q_entries: u64,
    pub softmax_middle_q_cutoff_entries: u64,
    pub softmax_middle_q_row_incidences: u64,
    pub softmax_middle_q_cutoff_row_incidences: u64,
    pub first_gradient_nanoseconds: u128,
    pub middle_gradient_nanoseconds: u128,
    pub log_update_nanoseconds: u128,
    pub averaging_nanoseconds: u128,
    pub exact_recertification_calls: u64,
    pub exact_recertification_nanoseconds: u128,
}

/// Probe-only admission window for attributing residual-dual value without
/// changing the production proof policy. The window is checked in addition
/// to every production dimension and gap guard; it can only suppress an
/// optional proposal, never authorize a new proof shortcut.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverResidualAdmissionPolicy {
    pub minimum_dual_gap: usize,
    pub maximum_dual_gap: usize,
    pub minimum_search_depth: usize,
    pub maximum_search_depth: usize,
    pub maximum_iterations_per_attempt: usize,
    /// Probe-only floating proposal optimization. Exact pruning authority
    /// remains the checked-u128 recertification in the dual workspace.
    pub use_sparse_proposal_softmax: bool,
}

#[cfg(feature = "diagnostic-probes")]
impl Default for ExactMinimumCoverResidualAdmissionPolicy {
    fn default() -> Self {
        Self {
            minimum_dual_gap: 1,
            maximum_dual_gap: 4,
            minimum_search_depth: 0,
            maximum_search_depth: usize::MAX,
            maximum_iterations_per_attempt: usize::MAX,
            use_sparse_proposal_softmax: true,
        }
    }
}

#[cfg(feature = "diagnostic-probes")]
impl ExactMinimumCoverResidualAdmissionPolicy {
    fn admits(self, dual_gap: usize, search_depth: usize) -> bool {
        (self.minimum_dual_gap..=self.maximum_dual_gap).contains(&dual_gap)
            && (self.minimum_search_depth..=self.maximum_search_depth).contains(&search_depth)
    }
}

/// Native probe-only result for comparing the former recursive DFS shape with
/// the production resumable state machine from one identical prepared search
/// snapshot. It is observability only and never enters a product decision.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverRecursiveReference {
    pub result: ExactMinimumCoverResult,
    pub visited_nodes: u64,
    pub residual: ExactMinimumCoverResidualDiagnostics,
}

#[cfg(any(test, feature = "diagnostic-probes"))]
fn checked_add_diagnostic_counter(
    counter: &mut u64,
    delta: u64,
) -> Result<(), ExactMinimumCoverError> {
    *counter = counter
        .checked_add(delta)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    Ok(())
}

/// Resumable exact minimum-cover proof authority.
///
/// Construction owns every reduced row and proof workspace needed by later
/// advances. No caller-owned row borrow is retained, which lets App/WASM keep
/// this session across cooperative ABI calls. The ordinary blocking entry
/// points below drive the same session with an unbounded node slice.
#[derive(Clone, Debug)]
pub struct ExactMinimumCoverSession {
    inner: ExactCoverSearchSession,
}

#[derive(Clone, Debug)]
pub(super) struct ExactCoverSearchSession {
    state: ExactCoverSearchSessionState,
}

#[derive(Clone, Debug)]
enum ExactCoverSearchSessionState {
    Unprepared {
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        goal: ExactCoverSearchGoal,
        incumbent_policy: ExactCoverIncumbentPolicy,
        witness_hint: Option<Vec<usize>>,
    },
    WitnessShortcut(WitnessShortcutSession),
    InitialQuotient {
        context: LazyExactCoverReductionContext,
        target_words: Vec<u64>,
        target_weights: Vec<usize>,
    },
    FixedPointDominance {
        context: LazyExactCoverReductionContext,
        compact_target: ExactCompactTarget,
    },
    FixedPointQuotient {
        context: LazyExactCoverReductionContext,
        compact_target: ExactCompactTarget,
        previous_row_count: usize,
        previous_constraint_count: usize,
    },
    BuildSearch {
        context: LazyExactCoverReductionContext,
        compact_target: ExactCompactTarget,
    },
    Ready(Option<ExactCoverInternalDecision>),
    PreparingRootDual {
        pattern_count: usize,
        complete: bool,
        dense_rows: Vec<DenseRow>,
        materialization_words: Vec<Vec<u64>>,
        search: MinimumCoverSearch,
    },
    ImprovingBreakout {
        pattern_count: usize,
        complete: bool,
        dense_rows: Vec<DenseRow>,
        materialization_words: Vec<Vec<u64>>,
        search: MinimumCoverSearch,
        incumbent_search: FixedCardinalityCoverSearchSession,
    },
    ImprovingIncumbent {
        pattern_count: usize,
        complete: bool,
        dense_rows: Vec<DenseRow>,
        materialization_words: Vec<Vec<u64>>,
        search: MinimumCoverSearch,
        incumbent_search: RandomizedCompactCoverSearchSession,
    },
    Searching {
        pattern_count: usize,
        complete: bool,
        dense_rows: Vec<DenseRow>,
        materialization_words: Vec<Vec<u64>>,
        search: MinimumCoverSearch,
    },
    Finished,
}

#[derive(Clone, Debug)]
struct LazyExactCoverReductionContext {
    required: PatternBitSet,
    source_rows: Vec<PatternBitSet>,
    goal: ExactCoverSearchGoal,
    incumbent_policy: ExactCoverIncumbentPolicy,
    dense_rows: Vec<DenseRow>,
    complete: bool,
}

/// Positive-only, resumable warm-witness acceleration for canonical AtMost
/// queries. None of its miss paths contribute proof authority; a miss always
/// hands the untouched original matrix to the ordinary exact session.
#[derive(Clone, Debug)]
struct WitnessShortcutSession {
    required: PatternBitSet,
    source_rows: Vec<PatternBitSet>,
    goal: ExactCoverSearchGoal,
    witness_hint: Vec<usize>,
    dense_rows: Vec<DenseRow>,
    target_words: Vec<u64>,
    constraint_weights: Vec<usize>,
    best: Vec<usize>,
    rarest_constraint: Option<MinimumSupportConstraintRows>,
    uses_preferred_missing_constraint: bool,
    support_by_pattern: Option<Vec<Vec<usize>>>,
    phase: WitnessShortcutPhase,
}

#[derive(Clone, Debug)]
enum WitnessShortcutPhase {
    ReplayHint,
    MaterializeDense,
    PrepareTarget,
    Greedy,
    HintBreakout {
        search: FixedCardinalityCoverSearchSession,
    },
    PrepareSupporters,
    WarmSeed {
        supporter_position: usize,
    },
    Breakout {
        supporter_position: usize,
        search: FixedCardinalityCoverSearchSession,
    },
    PrepareForcedSupporters,
    ForcedGreedy {
        supporter_position: usize,
    },
    Randomized {
        supporter_position: usize,
        search: RandomizedCompactCoverSearchSession,
    },
}

#[derive(Clone, Debug)]
struct RandomizedCompactCoverSearchSession {
    next_trial: usize,
    trial_end: usize,
    random_state: u64,
    random_choice: CoverRandomChoice,
    stop_at_cardinality: Option<usize>,
    forced_row: Option<usize>,
    covered: Vec<u64>,
    replay: Vec<u64>,
    selected: Vec<bool>,
    proposal: Vec<usize>,
    candidates: Vec<(usize, usize)>,
}

#[derive(Clone, Debug)]
struct FixedCardinalityCoverSearchSession {
    seed: Vec<usize>,
    protected_row: Option<usize>,
    target_cardinality: usize,
    restart_count: usize,
    swaps_per_restart: usize,
    restart: usize,
    iteration: usize,
    attempted_swaps: usize,
    total_swap_budget: usize,
    random_state: u64,
    random_choice: CoverRandomChoice,
    restart_initialized: bool,
    replay: Vec<u64>,
    singly_covered_words: Vec<u64>,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    word_mask_scoring: bool,
    coverage_counts: Vec<usize>,
    selected: Vec<bool>,
    proposal: Vec<usize>,
    pivot_candidates: Vec<usize>,
    remove_candidates: Vec<usize>,
    swap_candidates: Vec<BreakoutSwapCandidate>,
}

#[derive(Clone, Debug)]
enum WitnessShortcutStep {
    Pending,
    FoundDense(Vec<usize>),
    Miss,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OptionalHeuristicStep {
    Pending,
    Found(Vec<usize>),
    Finished,
    Cancelled,
}

impl WitnessShortcutSession {
    fn new(
        required: PatternBitSet,
        source_rows: Vec<PatternBitSet>,
        goal: ExactCoverSearchGoal,
        witness_hint: Vec<usize>,
    ) -> Self {
        Self {
            required,
            source_rows,
            goal,
            witness_hint,
            dense_rows: Vec::new(),
            target_words: Vec::new(),
            constraint_weights: Vec::new(),
            best: Vec::new(),
            rarest_constraint: None,
            uses_preferred_missing_constraint: false,
            support_by_pattern: None,
            phase: WitnessShortcutPhase::ReplayHint,
        }
    }

    fn checked_retained_capacity_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        let mut bytes = self
            .required
            .checked_storage_retained_bytes()
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?
            .checked_add(checked_pattern_bitset_rows_retained_bytes(
                &self.source_rows,
            )?)
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.witness_hint).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_dense_rows_retained_bytes(&self.dense_rows).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.target_words).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.constraint_weights).ok()?)
            })
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.best).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        if let Some(rarest) = &self.rarest_constraint {
            bytes = bytes
                .checked_add(checked_vec_retained_bytes(&rarest.row_indices)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        }
        if let Some(support) = &self.support_by_pattern {
            bytes = bytes
                .checked_add(checked_support_retained_bytes(support)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        }
        bytes = match &self.phase {
            WitnessShortcutPhase::Breakout { search, .. }
            | WitnessShortcutPhase::HintBreakout { search } => bytes
                .checked_add(search.checked_retained_capacity_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            WitnessShortcutPhase::Randomized { search, .. } => bytes
                .checked_add(search.checked_retained_capacity_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            _ => bytes,
        };
        Ok(bytes)
    }

    fn advance_one(
        &mut self,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<WitnessShortcutStep, ExactMinimumCoverError> {
        // One randomized trial is one documented cooperative work unit.
        // Keeping the primitive and reported units identical also prevents a
        // portfolio page budget from silently consuming twice its allowance.
        const RANDOMIZED_TRIALS_PER_SLICE: usize = 1;
        if cancelled() {
            return Ok(WitnessShortcutStep::Cancelled);
        }
        let ExactCoverSearchGoal::AtMost(limit) = self.goal else {
            return Err(ExactMinimumCoverError::ProjectionOverflow);
        };
        let phase = core::mem::replace(&mut self.phase, WitnessShortcutPhase::ReplayHint);
        match phase {
            WitnessShortcutPhase::ReplayHint => {
                // The hint seeds the same greedy/forced-supporter sequence as
                // the blocking helper; it is not itself a preferred answer.
                // Returning a replay-valid hint here changed deterministic row
                // identity (and masked non-required covered patterns) relative
                // to that authority.
                self.phase = WitnessShortcutPhase::MaterializeDense;
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::MaterializeDense => {
                let base_live = self.checked_retained_capacity_bytes()?;
                let mut dense_rows = try_vec_with_capacity(
                    self.source_rows.len(),
                    base_live,
                    memory_guard,
                    "exact_cover_at_most_lazy_witness_dense_rows",
                )?;
                for (source_index, row) in self.source_rows.iter().enumerate() {
                    if cancelled() {
                        return Ok(WitnessShortcutStep::Cancelled);
                    }
                    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
                    let mut words = try_vec_with_capacity(
                        self.required.word_count(),
                        base_live
                            .checked_add(dense_live)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        memory_guard,
                        "exact_cover_at_most_lazy_witness_dense_words",
                    )?;
                    let mut nonempty = false;
                    for word_index in 0..self.required.word_count() {
                        let word = row.word_at(word_index) & self.required.word_at(word_index);
                        nonempty |= word != 0;
                        words.push(word);
                    }
                    if nonempty {
                        dense_rows.push(DenseRow {
                            source_index,
                            words,
                        });
                    }
                }
                self.dense_rows = dense_rows;
                self.phase = WitnessShortcutPhase::PrepareTarget;
                memory_guard(self.checked_retained_capacity_bytes()?)?;
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::PrepareTarget => {
                let base_live = self.checked_retained_capacity_bytes()?;
                let mut target_words = try_vec_with_capacity(
                    self.required.word_count(),
                    base_live,
                    memory_guard,
                    "exact_cover_at_most_cooperative_witness_target",
                )?;
                target_words.extend(
                    (0..self.required.word_count()).map(|word| self.required.word_at(word)),
                );
                let target_live = checked_vec_retained_bytes(&target_words)?;
                let pattern_slots = target_words
                    .len()
                    .checked_mul(u64::BITS as usize)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                let mut constraint_weights = try_vec_with_capacity(
                    pattern_slots,
                    base_live
                        .checked_add(target_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    memory_guard,
                    "exact_cover_at_most_cooperative_witness_weights",
                )?;
                constraint_weights.resize(pattern_slots, 1);
                self.target_words = target_words;
                self.constraint_weights = constraint_weights;
                self.phase = WitnessShortcutPhase::Greedy;
                memory_guard(self.checked_retained_capacity_bytes()?)?;
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::Greedy => {
                let base_live = self.checked_retained_capacity_bytes()?;
                let Some(best) = greedy_cover_with_memory_guard(
                    &self.dense_rows,
                    &self.target_words,
                    &self.constraint_weights,
                    base_live,
                    memory_guard,
                )?
                else {
                    return Ok(WitnessShortcutStep::Miss);
                };
                self.best = best;
                if self.best.len() <= limit {
                    return Ok(WitnessShortcutStep::FoundDense(core::mem::take(
                        &mut self.best,
                    )));
                }
                if self.prepare_replayed_hint_breakout(limit, memory_guard)? {
                    return Ok(WitnessShortcutStep::Pending);
                }
                if limit.checked_add(1) == Some(self.witness_hint.len()) {
                    // An exclusion can invalidate this former incumbent.
                    // A missing non-selector constraint cannot seed the
                    // single-selector repair; proceed directly to exact DFS.
                    return Ok(WitnessShortcutStep::Miss);
                }
                self.phase = WitnessShortcutPhase::PrepareSupporters;
                memory_guard(self.checked_retained_capacity_bytes()?)?;
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::HintBreakout { mut search } => {
                let step = search.advance_one(
                    &self.dense_rows,
                    &self.target_words,
                    self.support_by_pattern
                        .as_ref()
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    cancelled,
                )?;
                match step {
                    OptionalHeuristicStep::Found(selected) if selected.len() <= limit => {
                        Ok(WitnessShortcutStep::FoundDense(selected))
                    }
                    OptionalHeuristicStep::Found(_) | OptionalHeuristicStep::Finished => {
                        // A failed repair cannot prove this cube empty.
                        Ok(WitnessShortcutStep::Miss)
                    }
                    OptionalHeuristicStep::Pending => {
                        self.phase = WitnessShortcutPhase::HintBreakout { search };
                        Ok(WitnessShortcutStep::Pending)
                    }
                    OptionalHeuristicStep::Cancelled => Ok(WitnessShortcutStep::Cancelled),
                }
            }
            WitnessShortcutPhase::PrepareSupporters => {
                let target_constraint_count = self
                    .target_words
                    .iter()
                    .map(|word| word.count_ones() as usize)
                    .sum::<usize>();
                if self.dense_rows.len() < 64 || target_constraint_count < 128 {
                    return Ok(WitnessShortcutStep::Miss);
                }
                let base_live = self.checked_retained_capacity_bytes()?;
                let preferred = witness_unique_missing_constraint_rows_with_memory_guard(
                    &self.dense_rows,
                    &self.target_words,
                    limit,
                    &self.witness_hint,
                    base_live,
                    memory_guard,
                )?;
                self.uses_preferred_missing_constraint = preferred.is_some();
                let rarest = match preferred {
                    Some(preferred) => Some(preferred),
                    None => minimum_support_constraint_rows_with_memory_guard(
                        &self.dense_rows,
                        &self.target_words,
                        base_live,
                        memory_guard,
                    )?,
                };
                let Some(rarest) = rarest else {
                    return Ok(WitnessShortcutStep::Miss);
                };
                self.rarest_constraint = Some(rarest);
                self.phase = WitnessShortcutPhase::WarmSeed {
                    supporter_position: 0,
                };
                memory_guard(self.checked_retained_capacity_bytes()?)?;
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::WarmSeed { supporter_position } => {
                let rarest = self
                    .rarest_constraint
                    .as_ref()
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                if supporter_position >= rarest.row_indices.len() {
                    self.support_by_pattern = None;
                    self.phase = WitnessShortcutPhase::PrepareForcedSupporters;
                    return Ok(WitnessShortcutStep::Pending);
                }
                let forced_row = rarest.row_indices[supporter_position];
                let base_live = self.checked_retained_capacity_bytes()?;
                let Some(warm_seed) = warm_seed_with_forced_supporter_memory_guard(
                    &self.dense_rows,
                    &self.target_words,
                    limit,
                    &self.witness_hint,
                    rarest.word_index,
                    rarest.bit,
                    forced_row,
                    base_live,
                    memory_guard,
                )?
                else {
                    self.phase = WitnessShortcutPhase::WarmSeed {
                        supporter_position: supporter_position + 1,
                    };
                    return Ok(WitnessShortcutStep::Pending);
                };
                if warm_seed.len() <= limit {
                    return Ok(WitnessShortcutStep::FoundDense(warm_seed));
                }
                if self.support_by_pattern.is_none() {
                    let support_base = self
                        .checked_retained_capacity_bytes()?
                        .checked_add(checked_vec_retained_bytes(&warm_seed)?)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    self.support_by_pattern = Some(build_support_by_pattern_with_memory_guard(
                        &self.dense_rows,
                        &self.target_words,
                        support_base,
                        memory_guard,
                    )?);
                }
                let breakout_base = self.checked_retained_capacity_bytes()?;
                let breakout_swap_budget = if self.uses_preferred_missing_constraint {
                    preferred_witness_breakout_budget(rarest.row_indices.len(), supporter_position)
                } else {
                    WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET
                };
                if breakout_swap_budget == 0 {
                    self.phase = WitnessShortcutPhase::WarmSeed {
                        supporter_position: supporter_position + 1,
                    };
                    return Ok(WitnessShortcutStep::Pending);
                }
                let Some(search) = FixedCardinalityCoverSearchSession::try_new(
                    &self.dense_rows,
                    &self.target_words,
                    self.support_by_pattern
                        .as_ref()
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    warm_seed,
                    breakout_swap_budget,
                    Some(forced_row),
                    breakout_base,
                    memory_guard,
                )?
                else {
                    self.phase = WitnessShortcutPhase::WarmSeed {
                        supporter_position: supporter_position + 1,
                    };
                    return Ok(WitnessShortcutStep::Pending);
                };
                self.phase = WitnessShortcutPhase::Breakout {
                    supporter_position,
                    search,
                };
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::Breakout {
                supporter_position,
                mut search,
            } => {
                let step = search.advance_one(
                    &self.dense_rows,
                    &self.target_words,
                    self.support_by_pattern
                        .as_ref()
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    cancelled,
                )?;
                match step {
                    OptionalHeuristicStep::Found(selected) if selected.len() <= limit => {
                        Ok(WitnessShortcutStep::FoundDense(selected))
                    }
                    OptionalHeuristicStep::Found(_) | OptionalHeuristicStep::Finished => {
                        self.phase = WitnessShortcutPhase::WarmSeed {
                            supporter_position: supporter_position + 1,
                        };
                        Ok(WitnessShortcutStep::Pending)
                    }
                    OptionalHeuristicStep::Pending => {
                        self.phase = WitnessShortcutPhase::Breakout {
                            supporter_position,
                            search,
                        };
                        Ok(WitnessShortcutStep::Pending)
                    }
                    OptionalHeuristicStep::Cancelled => Ok(WitnessShortcutStep::Cancelled),
                }
            }
            WitnessShortcutPhase::PrepareForcedSupporters => {
                if self.uses_preferred_missing_constraint {
                    let base_live = self.checked_retained_capacity_bytes()?;
                    let Some(rarest) = minimum_support_constraint_rows_with_memory_guard(
                        &self.dense_rows,
                        &self.target_words,
                        base_live,
                        memory_guard,
                    )?
                    else {
                        return Ok(WitnessShortcutStep::Miss);
                    };
                    self.rarest_constraint = Some(rarest);
                    self.uses_preferred_missing_constraint = false;
                    memory_guard(self.checked_retained_capacity_bytes()?)?;
                }
                self.phase = WitnessShortcutPhase::ForcedGreedy {
                    supporter_position: 0,
                };
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::ForcedGreedy { supporter_position } => {
                let rarest = self
                    .rarest_constraint
                    .as_ref()
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                if supporter_position >= rarest.row_indices.len() {
                    return Ok(WitnessShortcutStep::Miss);
                }
                let forced_row = rarest.row_indices[supporter_position];
                let base_live = self.checked_retained_capacity_bytes()?;
                if let Some(forced_best) = greedy_cover_with_forced_row_memory_guard(
                    &self.dense_rows,
                    &self.target_words,
                    &self.constraint_weights,
                    Some(forced_row),
                    base_live,
                    memory_guard,
                )? {
                    if forced_best.len() <= limit {
                        return Ok(WitnessShortcutStep::FoundDense(forced_best));
                    }
                    if forced_best.len() < self.best.len() {
                        self.best = forced_best;
                    }
                }
                let supporter_count = rarest.row_indices.len();
                let trial_budget = WITNESS_ASSISTED_COMPACT_COVER_TRIALS / supporter_count
                    + usize::from(
                        supporter_position
                            < WITNESS_ASSISTED_COMPACT_COVER_TRIALS % supporter_count,
                    );
                if trial_budget == 0 {
                    self.phase = WitnessShortcutPhase::ForcedGreedy {
                        supporter_position: supporter_position + 1,
                    };
                    return Ok(WitnessShortcutStep::Pending);
                }
                let random_seed = WITNESS_ASSISTED_RANDOM_SEED
                    ^ (self.dense_rows[forced_row].source_index as u64)
                        .wrapping_mul(0xd6e8_feb8_6659_fd93);
                let search = RandomizedCompactCoverSearchSession::try_new(
                    &self.dense_rows,
                    &self.target_words,
                    0,
                    trial_budget,
                    random_seed,
                    Some(limit),
                    Some(forced_row),
                    self.checked_retained_capacity_bytes()?,
                    memory_guard,
                )?;
                self.phase = WitnessShortcutPhase::Randomized {
                    supporter_position,
                    search,
                };
                Ok(WitnessShortcutStep::Pending)
            }
            WitnessShortcutPhase::Randomized {
                supporter_position,
                mut search,
            } => match search.advance(
                &self.dense_rows,
                &self.target_words,
                &mut self.best,
                RANDOMIZED_TRIALS_PER_SLICE,
                cancelled,
            )? {
                OptionalHeuristicStep::Found(selected) => {
                    Ok(WitnessShortcutStep::FoundDense(selected))
                }
                OptionalHeuristicStep::Finished => {
                    self.phase = WitnessShortcutPhase::ForcedGreedy {
                        supporter_position: supporter_position + 1,
                    };
                    Ok(WitnessShortcutStep::Pending)
                }
                OptionalHeuristicStep::Pending => {
                    self.phase = WitnessShortcutPhase::Randomized {
                        supporter_position,
                        search,
                    };
                    Ok(WitnessShortcutStep::Pending)
                }
                OptionalHeuristicStep::Cancelled => Ok(WitnessShortcutStep::Cancelled),
            },
        }
    }

    /// Parallel cubes can turn a k-cover hint into a (k+1)-cover of their
    /// residual matrix. Repair that replayed cover before restarting an exact
    /// positive search from greedy. A repair miss has no negative authority.
    fn prepare_replayed_hint_breakout(
        &mut self,
        limit: usize,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, ExactMinimumCoverError> {
        if limit.checked_add(1) != Some(self.witness_hint.len())
            || !self.witness_hint.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Ok(false);
        }
        let base = self.checked_retained_capacity_bytes()?;
        let mut seed = try_vec_with_capacity(
            self.witness_hint.len(),
            base,
            memory_guard,
            "exact_cover_parallel_warm_seed",
        )?;
        for source in &self.witness_hint {
            let Ok(row) = self
                .dense_rows
                .binary_search_by_key(source, |row| row.source_index)
            else {
                return Ok(false);
            };
            seed.push(row);
        }
        // No temporary replay bitmap is needed: original source row identities
        // were mapped above and each required word is independently replayed.
        if self.target_words.iter().enumerate().any(|(word, target)| {
            seed.iter().fold(0, |covered, &row| {
                covered | self.dense_rows[row].words[word]
            }) & target
                != *target
        }) {
            return Ok(false);
        }
        let support_base = base
            .checked_add(checked_vec_retained_bytes(&seed)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        self.support_by_pattern = Some(build_support_by_pattern_with_memory_guard(
            &self.dense_rows,
            &self.target_words,
            support_base,
            memory_guard,
        )?);
        let Some(search) = FixedCardinalityCoverSearchSession::try_new(
            &self.dense_rows,
            &self.target_words,
            self.support_by_pattern
                .as_ref()
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            seed,
            WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
            None,
            self.checked_retained_capacity_bytes()?,
            memory_guard,
        )?
        else {
            return Ok(false);
        };
        self.phase = WitnessShortcutPhase::HintBreakout { search };
        memory_guard(self.checked_retained_capacity_bytes()?)?;
        Ok(true)
    }

    fn materialize_dense_result(
        self,
        selected_rows: Vec<usize>,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<ExactCoverInternalDecision, ExactMinimumCoverError> {
        // Build the original-row replay while every cursor-owned allocation is
        // still live and charge that whole-live peak.  Once it exists, discard
        // the non-authoritative shortcut scratch before result construction.
        let whole_live = self.checked_retained_capacity_bytes()?;
        let materialization_words = build_materialization_words_with_memory_guard(
            &self.required,
            &self.source_rows,
            &self.dense_rows,
            whole_live,
            memory_guard,
        )?;
        let pattern_count = self.required.pattern_count();
        let dense_rows = self.dense_rows;
        drop(self.support_by_pattern);
        drop(self.rarest_constraint);
        drop(self.best);
        drop(self.constraint_weights);
        drop(self.target_words);
        drop(self.witness_hint);
        drop(self.source_rows);
        drop(self.required);
        materialize_internal_result_from_owned_words(
            pattern_count,
            dense_rows,
            materialization_words,
            selected_rows,
            true,
            memory_guard,
        )
    }

    fn into_fallback_parts(self) -> (PatternBitSet, Vec<PatternBitSet>, ExactCoverSearchGoal) {
        (self.required, self.source_rows, self.goal)
    }
}

impl RandomizedCompactCoverSearchSession {
    #[allow(clippy::too_many_arguments)]
    fn try_new(
        rows: &[DenseRow],
        target: &[u64],
        first_trial: usize,
        trial_budget: usize,
        random_seed: u64,
        stop_at_cardinality: Option<usize>,
        forced_row: Option<usize>,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        let trial_end = first_trial
            .checked_add(trial_budget)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut covered = try_vec_with_capacity(
            target.len(),
            base_live_bytes,
            memory_guard,
            "exact_minimum_cover_randomized_session_covered",
        )?;
        covered.resize(target.len(), 0);
        let covered_live = checked_vec_retained_bytes(&covered)?;
        let mut replay = try_vec_with_capacity(
            target.len(),
            base_live_bytes
                .checked_add(covered_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_randomized_session_replay",
        )?;
        replay.resize(target.len(), 0);
        let replay_live = checked_vec_retained_bytes(&replay)?;
        let mut selected = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(covered_live)
                .and_then(|bytes| bytes.checked_add(replay_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_randomized_session_selected",
        )?;
        selected.resize(rows.len(), false);
        let selected_live = checked_vec_retained_bytes(&selected)?;
        let proposal = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(covered_live)
                .and_then(|bytes| bytes.checked_add(replay_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_randomized_session_proposal",
        )?;
        let proposal_live = checked_vec_retained_bytes(&proposal)?;
        let candidates = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(covered_live)
                .and_then(|bytes| bytes.checked_add(replay_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .and_then(|bytes| bytes.checked_add(proposal_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_randomized_session_candidates",
        )?;
        let result = Self {
            next_trial: first_trial,
            trial_end,
            random_state: random_seed,
            random_choice: CoverRandomChoice::for_new_session(),
            stop_at_cardinality,
            forced_row,
            covered,
            replay,
            selected,
            proposal,
            candidates,
        };
        memory_guard(
            base_live_bytes
                .checked_add(result.checked_retained_capacity_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(result)
    }

    fn checked_retained_capacity_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.covered)?
            .checked_add(checked_vec_retained_bytes(&self.replay)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.selected).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.proposal).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.candidates).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }

    fn advance(
        &mut self,
        rows: &[DenseRow],
        target: &[u64],
        best: &mut Vec<usize>,
        max_trials: usize,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<OptionalHeuristicStep, ExactMinimumCoverError> {
        if self.next_trial >= self.trial_end {
            return Ok(OptionalHeuristicStep::Finished);
        }
        let slice_end = self
            .next_trial
            .checked_add(max_trials)
            .map_or(self.trial_end, |end| end.min(self.trial_end));
        while self.next_trial < slice_end {
            if cancelled() {
                return Ok(OptionalHeuristicStep::Cancelled);
            }
            let trial = self.next_trial;
            self.next_trial += 1;
            self.covered.fill(0);
            self.replay.fill(0);
            self.selected.fill(false);
            self.proposal.clear();

            if let Some(forced_row) = self.forced_row {
                let row = rows
                    .get(forced_row)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                self.selected[forced_row] = true;
                self.proposal.push(forced_row);
                union_words(&mut self.covered, &row.words);
            }

            while !is_superset(&self.covered, target) {
                if cancelled() {
                    return Ok(OptionalHeuristicStep::Cancelled);
                }
                self.candidates.clear();
                for (row_index, row) in rows.iter().enumerate() {
                    if self.selected[row_index] {
                        continue;
                    }
                    let gain = row.words.iter().zip(&self.covered).zip(target).fold(
                        0_usize,
                        |gain, ((row, covered), target)| {
                            gain.saturating_add((row & target & !covered).count_ones() as usize)
                        },
                    );
                    if gain > 0 {
                        self.candidates.push((gain, row_index));
                    }
                }
                if self.candidates.is_empty() {
                    break;
                }
                self.candidates
                    .sort_unstable_by(|(left_gain, left), (right_gain, right)| {
                        right_gain
                            .cmp(left_gain)
                            .then_with(|| rows[*left].source_index.cmp(&rows[*right].source_index))
                    });
                self.random_state ^= self.random_state << 13;
                self.random_state ^= self.random_state >> 7;
                self.random_state ^= self.random_state << 17;
                let restricted_candidate_count = (2 + trial % 7).min(self.candidates.len());
                let choice = self
                    .random_choice
                    .index(self.random_state, restricted_candidate_count);
                let row_index = self.candidates[choice].1;
                self.selected[row_index] = true;
                self.proposal.push(row_index);
                union_words(&mut self.covered, &rows[row_index].words);
            }
            if !is_superset(&self.covered, target) {
                continue;
            }

            for position in (0..self.proposal.len()).rev() {
                if self
                    .forced_row
                    .is_some_and(|forced_row| self.proposal[position] == forced_row)
                {
                    continue;
                }
                self.replay.fill(0);
                for (other_position, row_index) in self.proposal.iter().copied().enumerate() {
                    if other_position != position {
                        union_words(&mut self.replay, &rows[row_index].words);
                    }
                }
                if is_superset(&self.replay, target) {
                    let removed = self.proposal.remove(position);
                    self.selected[removed] = false;
                    self.covered.copy_from_slice(&self.replay);
                }
            }

            if self.proposal.len() >= best.len() {
                continue;
            }
            self.replay.fill(0);
            for row_index in self.proposal.iter().copied() {
                union_words(&mut self.replay, &rows[row_index].words);
            }
            if is_superset(&self.replay, target) {
                best.clear();
                best.extend_from_slice(&self.proposal);
                if self
                    .stop_at_cardinality
                    .is_some_and(|limit| best.len() <= limit)
                {
                    return Ok(OptionalHeuristicStep::Found(core::mem::take(best)));
                }
            }
        }
        if self.next_trial >= self.trial_end {
            Ok(OptionalHeuristicStep::Finished)
        } else {
            Ok(OptionalHeuristicStep::Pending)
        }
    }
}

impl FixedCardinalityCoverSearchSession {
    #[allow(clippy::too_many_arguments)]
    fn try_new(
        rows: &[DenseRow],
        target: &[u64],
        support_by_pattern: &[Vec<usize>],
        seed: Vec<usize>,
        total_swap_budget: usize,
        protected_row: Option<usize>,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Option<Self>, ExactMinimumCoverError> {
        if seed.len() <= 1 || total_swap_budget == 0 {
            return Ok(None);
        }
        if protected_row.is_some_and(|protected| !seed.contains(&protected)) {
            return Err(ExactMinimumCoverError::ProjectionOverflow);
        }
        let seed_live = checked_vec_retained_bytes(&seed)?;
        let live = base_live_bytes
            .checked_add(seed_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut replay = try_vec_with_capacity(
            target.len(),
            live,
            memory_guard,
            "exact_minimum_cover_breakout_session_replay",
        )?;
        replay.resize(target.len(), 0);
        let replay_live = checked_vec_retained_bytes(&replay)?;
        let mut singly_covered_words = try_vec_with_capacity(
            target.len(),
            live.checked_add(replay_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_single_coverage",
        )?;
        singly_covered_words.resize(target.len(), 0);
        let word_scratch_live = replay_live
            .checked_add(checked_vec_retained_bytes(&singly_covered_words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut coverage_counts = try_vec_with_capacity(
            support_by_pattern.len(),
            live.checked_add(word_scratch_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_counts",
        )?;
        coverage_counts.resize(support_by_pattern.len(), 0);
        let counts_live = checked_vec_retained_bytes(&coverage_counts)?;
        let mut selected = try_vec_with_capacity(
            rows.len(),
            live.checked_add(word_scratch_live)
                .and_then(|bytes| bytes.checked_add(counts_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_selected",
        )?;
        selected.resize(rows.len(), false);
        let selected_live = checked_vec_retained_bytes(&selected)?;
        let proposal = try_vec_with_capacity(
            rows.len(),
            live.checked_add(word_scratch_live)
                .and_then(|bytes| bytes.checked_add(counts_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_proposal",
        )?;
        let proposal_live = checked_vec_retained_bytes(&proposal)?;
        let pivot_candidates = try_vec_with_capacity(
            support_by_pattern.len(),
            live.checked_add(word_scratch_live)
                .and_then(|bytes| bytes.checked_add(counts_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .and_then(|bytes| bytes.checked_add(proposal_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_pivots",
        )?;
        let pivots_live = checked_vec_retained_bytes(&pivot_candidates)?;
        let remove_candidates = try_vec_with_capacity(
            rows.len(),
            live.checked_add(word_scratch_live)
                .and_then(|bytes| bytes.checked_add(counts_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .and_then(|bytes| bytes.checked_add(proposal_live))
                .and_then(|bytes| bytes.checked_add(pivots_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_removes",
        )?;
        let removes_live = checked_vec_retained_bytes(&remove_candidates)?;
        let swap_candidates = try_vec_with_capacity(
            rows.len(),
            live.checked_add(word_scratch_live)
                .and_then(|bytes| bytes.checked_add(counts_live))
                .and_then(|bytes| bytes.checked_add(selected_live))
                .and_then(|bytes| bytes.checked_add(proposal_live))
                .and_then(|bytes| bytes.checked_add(pivots_live))
                .and_then(|bytes| bytes.checked_add(removes_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_breakout_session_swaps",
        )?;
        let restart_count = seed.len();
        let result = Self {
            target_cardinality: restart_count - 1,
            restart_count,
            swaps_per_restart: total_swap_budget.div_ceil(restart_count),
            seed,
            protected_row,
            restart: 0,
            iteration: 0,
            attempted_swaps: 0,
            total_swap_budget,
            random_state: 0x9e37_79b9_7f4a_7c15_u64,
            random_choice: CoverRandomChoice::for_new_session(),
            restart_initialized: false,
            replay,
            singly_covered_words,
            #[cfg(feature = "diagnostic-probes")]
            word_mask_scoring: DIAGNOSTIC_REPAIR_WORD_MASKS
                .load(std::sync::atomic::Ordering::Relaxed),
            #[cfg(all(test, not(feature = "diagnostic-probes")))]
            word_mask_scoring: true,
            coverage_counts,
            selected,
            proposal,
            pivot_candidates,
            remove_candidates,
            swap_candidates,
        };
        memory_guard(
            base_live_bytes
                .checked_add(result.checked_retained_capacity_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(Some(result))
    }

    fn checked_retained_capacity_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.seed)?
            .checked_add(checked_vec_retained_bytes(&self.replay)?)
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.singly_covered_words).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.coverage_counts).ok()?)
            })
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.selected).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.proposal).ok()?))
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.pivot_candidates).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.remove_candidates).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.swap_candidates).ok()?)
            })
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }

    #[inline]
    fn uses_word_mask_scoring(&self) -> bool {
        #[cfg(any(test, feature = "diagnostic-probes"))]
        {
            self.word_mask_scoring
        }
        #[cfg(not(any(test, feature = "diagnostic-probes")))]
        {
            true
        }
    }

    fn advance_one(
        &mut self,
        rows: &[DenseRow],
        target: &[u64],
        support_by_pattern: &[Vec<usize>],
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<OptionalHeuristicStep, ExactMinimumCoverError> {
        if cancelled() {
            return Ok(OptionalHeuristicStep::Cancelled);
        }
        if self.restart >= self.restart_count || self.attempted_swaps >= self.total_swap_budget {
            return Ok(OptionalHeuristicStep::Finished);
        }
        if !self.restart_initialized {
            let dropped_position = self.restart % self.restart_count;
            if self
                .protected_row
                .is_some_and(|protected| self.seed[dropped_position] == protected)
            {
                self.restart += 1;
                return Ok(OptionalHeuristicStep::Pending);
            }
            self.coverage_counts.fill(0);
            self.selected.fill(false);
            self.proposal.clear();
            for (position, row_index) in self.seed.iter().copied().enumerate() {
                if position == dropped_position {
                    continue;
                }
                self.selected[row_index] = true;
                self.proposal.push(row_index);
                for (word_index, (row_word, target_word)) in
                    rows[row_index].words.iter().zip(target).enumerate()
                {
                    let mut bits = row_word & target_word;
                    while bits != 0 {
                        let bit = bits.trailing_zeros() as usize;
                        let pattern = word_index * u64::BITS as usize + bit;
                        self.coverage_counts[pattern] += 1;
                        bits &= bits - 1;
                    }
                }
            }
            debug_assert_eq!(self.proposal.len(), self.target_cardinality);
            self.iteration = 0;
            self.restart_initialized = true;
            return Ok(OptionalHeuristicStep::Pending);
        }
        if self.iteration >= self.swaps_per_restart {
            self.restart += 1;
            self.restart_initialized = false;
            return Ok(OptionalHeuristicStep::Pending);
        }

        self.iteration += 1;
        self.attempted_swaps += 1;
        self.pivot_candidates.clear();
        let arbitrary_pivot = (self.iteration - 1) % 20 == 0;
        let mut minimum_support = usize::MAX;
        let word_mask_scoring = self.uses_word_mask_scoring();
        if word_mask_scoring {
            // Reuse witness replay scratch until a positive proposal needs it.
            // Both masks are restricted to target bits, including the last word.
            self.replay.fill(0);
            self.singly_covered_words.fill(0);
        }
        for (word_index, target_word) in target.iter().copied().enumerate() {
            let mut bits = target_word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                bits &= bits - 1;
                let coverage_count = self.coverage_counts[pattern];
                if coverage_count != 0 {
                    if word_mask_scoring && coverage_count == 1 {
                        self.singly_covered_words[word_index] |= 1_u64 << bit;
                    }
                    continue;
                }
                if word_mask_scoring {
                    self.replay[word_index] |= 1_u64 << bit;
                }
                if arbitrary_pivot {
                    self.pivot_candidates.push(pattern);
                    continue;
                }
                let support_count = support_by_pattern[pattern].len();
                match support_count.cmp(&minimum_support) {
                    core::cmp::Ordering::Less => {
                        minimum_support = support_count;
                        self.pivot_candidates.clear();
                        self.pivot_candidates.push(pattern);
                    }
                    core::cmp::Ordering::Equal => self.pivot_candidates.push(pattern),
                    core::cmp::Ordering::Greater => {}
                }
            }
        }
        if self.pivot_candidates.is_empty() {
            self.replay.fill(0);
            for row_index in self.proposal.iter().copied() {
                union_words(&mut self.replay, &rows[row_index].words);
            }
            if self.proposal.len() == self.target_cardinality && is_superset(&self.replay, target) {
                self.seed.clear();
                self.seed.extend_from_slice(&self.proposal);
                return Ok(OptionalHeuristicStep::Found(core::mem::take(
                    &mut self.seed,
                )));
            }
            self.restart += 1;
            self.restart_initialized = false;
            return Ok(OptionalHeuristicStep::Pending);
        }

        let pivot = self.pivot_candidates[self.random_choice.index(
            next_breakout_random(&mut self.random_state),
            self.pivot_candidates.len(),
        )];
        self.swap_candidates.clear();
        for add_row in support_by_pattern[pivot]
            .iter()
            .copied()
            .filter(|row_index| !self.selected[*row_index])
        {
            #[cfg(not(any(test, feature = "diagnostic-probes")))]
            let make_gain = breakout_word_make_gain(&rows[add_row].words, &self.replay);
            #[cfg(any(test, feature = "diagnostic-probes"))]
            let make_gain = if word_mask_scoring {
                breakout_word_make_gain(&rows[add_row].words, &self.replay)
            } else {
                breakout_reference_make_gain(&rows[add_row].words, target, &self.coverage_counts)
            };
            let mut minimum_break = usize::MAX;
            self.remove_candidates.clear();
            for remove_row in self.proposal.iter().copied() {
                if self
                    .protected_row
                    .is_some_and(|protected| remove_row == protected)
                {
                    continue;
                }
                #[cfg(not(any(test, feature = "diagnostic-probes")))]
                let break_loss = breakout_word_break_loss(
                    &rows[remove_row].words,
                    &rows[add_row].words,
                    &self.singly_covered_words,
                );
                #[cfg(any(test, feature = "diagnostic-probes"))]
                let break_loss = if word_mask_scoring {
                    breakout_word_break_loss(
                        &rows[remove_row].words,
                        &rows[add_row].words,
                        &self.singly_covered_words,
                    )
                } else {
                    breakout_reference_break_loss(
                        &rows[remove_row].words,
                        &rows[add_row].words,
                        target,
                        &self.coverage_counts,
                    )
                };
                match break_loss.cmp(&minimum_break) {
                    core::cmp::Ordering::Less => {
                        minimum_break = break_loss;
                        self.remove_candidates.clear();
                        self.remove_candidates.push(remove_row);
                    }
                    core::cmp::Ordering::Equal => self.remove_candidates.push(remove_row),
                    core::cmp::Ordering::Greater => {}
                }
            }
            if self.remove_candidates.is_empty() {
                continue;
            }
            let remove_row = self.remove_candidates[self.random_choice.index(
                next_breakout_random(&mut self.random_state),
                self.remove_candidates.len(),
            )];
            self.swap_candidates.push(BreakoutSwapCandidate {
                net_gain: make_gain as i128 - minimum_break as i128,
                make_gain,
                break_loss: minimum_break,
                add_row,
                remove_row,
            });
        }
        if self.swap_candidates.is_empty() {
            self.restart += 1;
            self.restart_initialized = false;
            return Ok(OptionalHeuristicStep::Pending);
        }
        self.swap_candidates.sort_unstable_by(|left, right| {
            right
                .net_gain
                .cmp(&left.net_gain)
                .then_with(|| right.make_gain.cmp(&left.make_gain))
                .then_with(|| left.break_loss.cmp(&right.break_loss))
                .then_with(|| right.add_row.cmp(&left.add_row))
                .then_with(|| left.remove_row.cmp(&right.remove_row))
        });
        let perturb = next_breakout_random(&mut self.random_state) % 100 < 8;
        let choice_count = if perturb {
            self.swap_candidates.len().min(4)
        } else {
            self.swap_candidates
                .iter()
                .take_while(|candidate| candidate.net_gain == self.swap_candidates[0].net_gain)
                .count()
        };
        let choice = self.random_choice.index(
            next_breakout_random(&mut self.random_state),
            choice_count.max(1),
        );
        let selected_swap = self.swap_candidates[choice];
        self.selected[selected_swap.remove_row] = false;
        for (word_index, (row_word, target_word)) in rows[selected_swap.remove_row]
            .words
            .iter()
            .zip(target)
            .enumerate()
        {
            let mut bits = row_word & target_word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                self.coverage_counts[pattern] -= 1;
                bits &= bits - 1;
            }
        }
        self.selected[selected_swap.add_row] = true;
        for (word_index, (row_word, target_word)) in rows[selected_swap.add_row]
            .words
            .iter()
            .zip(target)
            .enumerate()
        {
            let mut bits = row_word & target_word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                self.coverage_counts[pattern] += 1;
                bits &= bits - 1;
            }
        }
        let remove_position = self
            .proposal
            .iter()
            .position(|row_index| *row_index == selected_swap.remove_row)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        self.proposal[remove_position] = selected_swap.add_row;
        Ok(OptionalHeuristicStep::Pending)
    }
}

fn optional_dual_acceleration<T>(
    result: Result<T, ExactMinimumCoverError>,
) -> Result<Option<T>, ExactMinimumCoverError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(
            ExactMinimumCoverError::AllocationFailed { .. }
            | ExactMinimumCoverError::ProjectionOverflow,
        ) => Ok(None),
        Err(error) => Err(error),
    }
}

fn optional_dual_preflight(
    result: Result<(), ExactMinimumCoverError>,
) -> Result<bool, ExactMinimumCoverError> {
    match result {
        Ok(()) => Ok(true),
        Err(ExactMinimumCoverError::MemoryCapacityExceeded { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

impl ExactMinimumCoverResult {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn into_parts(self) -> (Vec<usize>, PatternBitSet, bool) {
        (self.row_indices, self.covered_patterns, self.complete)
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.row_indices.capacity() as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)?
            .checked_add(self.covered_patterns.checked_storage_retained_bytes()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverError {
    RowPatternCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    MemoryGuardRejected,
    AllocationFailed {
        component: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverMemoryProjection {
    pub memo_state_upper_bound: u128,
    pub memo_state_bytes_upper_bound: u128,
    pub fixed_workspace_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Debug)]
struct DenseRow {
    source_index: usize,
    words: Vec<u64>,
}

pub fn exact_minimum_cover(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    exact_minimum_cover_with_memory_guard(required, rows, &mut |_| Ok(()))
}

/// Returns any exact cover of `required` using at most `limit` original rows,
/// or `None` only after proving that no such cover exists.
///
/// Row identities always refer to the caller's original matrix, including
/// when lossless row/constraint reductions were used internally.
pub fn exact_cover_at_most(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
) -> Result<Option<ExactCoverAtMostResult>, ExactMinimumCoverError> {
    exact_cover_at_most_with_memory_guard(required, rows, limit, &mut |_| Ok(()))
}

pub fn exact_cover_at_most_with_control(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverAtMostDecision, ExactMinimumCoverError> {
    exact_cover_at_most_with_memory_guard_and_control(
        required,
        rows,
        limit,
        &mut |_| Ok(()),
        cancelled,
    )
}

/// Returns the maximum number of distinct covered states representable by
/// `row_count` unions over `required_bit_count` required bits.
///
/// `None` is deliberately fail-closed: it means that even the state count
/// cannot be represented in the projection's `u128` accounting domain.
pub fn checked_exact_minimum_cover_state_upper_bound(
    required_bit_count: usize,
    row_count: usize,
) -> Option<u128> {
    u32::try_from(required_bit_count.min(row_count))
        .ok()
        .and_then(|exponent| 1_u128.checked_shl(exponent))
}

/// Computes an input-size-only upper bound for this module's requested heap
/// capacities, including the fully owned deterministic dual-proposal
/// workspace. The guarded solver checks allocator-returned capacities after
/// every owned reserve and grows the exponential memo incrementally, so callers
/// do not need to reserve that worst case merely to execute a small realized
/// search.
pub fn checked_exact_minimum_cover_memory_projection(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Option<ExactMinimumCoverMemoryProjection> {
    if rows
        .iter()
        .any(|row| row.pattern_count() != required.pattern_count())
    {
        return None;
    }
    let row_count = rows.len() as u128;
    let word_count = required.word_count() as u128;
    let word_bytes = word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
    let required_bit_count = required.count_ones() as usize;
    let memo_state_upper_bound =
        checked_exact_minimum_cover_state_upper_bound(required_bit_count, rows.len())?;
    let memo_entry_bytes =
        (core::mem::size_of::<ExactCoveredStateMemoEntry>() as u128).checked_add(word_bytes)?;
    // The exact memo keeps its deterministic open-addressed index at or below
    // a 1/2 load factor. The eight-slot floor is also used by the incremental
    // allocator below.
    // The final power-of-two table is strictly below 4*N slots. During a
    // rehash, its previous (half-sized) table remains live until the new table
    // is populated, so 6*N is a sound requested-capacity peak bound.
    let memo_bucket_upper_bound = memo_state_upper_bound.checked_mul(6)?.max(8);
    let memo_state_bytes_upper_bound = memo_state_upper_bound
        .checked_mul(memo_entry_bytes)?
        .checked_add(memo_bucket_upper_bound.checked_mul(core::mem::size_of::<usize>() as u128)?)?;

    let dense_rows = row_count
        .checked_mul(core::mem::size_of::<DenseRow>() as u128)?
        .checked_add(row_count.checked_mul(word_bytes)?)?;
    // Resumable sessions retain an original-universe word snapshot beside the
    // quotient-mutated rows so terminal result materialization remains exact.
    let materialization_rows = row_count
        .checked_mul(core::mem::size_of::<Vec<u64>>() as u128)?
        .checked_add(row_count.checked_mul(word_bytes)?)?;
    let pattern_slots = word_count.checked_mul(u64::BITS as u128)?;
    let support_slots = pattern_slots
        .checked_mul(core::mem::size_of::<Vec<usize>>() as u128)?
        .checked_add(
            row_count
                .checked_mul(required_bit_count as u128)?
                .checked_mul(core::mem::size_of::<usize>() as u128)?,
        )?;
    let dominance_index_scratch = row_count
        .checked_mul(core::mem::size_of::<usize>() as u128)?
        .checked_add(
            pattern_slots
                .checked_mul(3)?
                .checked_add(1)?
                .checked_mul(core::mem::size_of::<usize>() as u128)?,
        )?
        .checked_add(
            row_count
                .checked_mul(required_bit_count as u128)?
                .checked_mul(core::mem::size_of::<usize>() as u128)?,
        )?
        .checked_add(row_count.checked_mul(core::mem::size_of::<bool>() as u128)?)?;
    let support_word_count = row_count
        .checked_add(u64::BITS as u128 - 1)?
        .checked_div(u64::BITS as u128)?;
    let row_word_bytes = support_word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
    let target_constraint_scratch = (required_bit_count as u128)
        .checked_mul(
            (core::mem::size_of::<usize>() as u128)
                .checked_mul(5)?
                .checked_add(core::mem::size_of::<bool>() as u128)?,
        )?
        .checked_add(row_count.checked_mul(core::mem::size_of::<usize>() as u128)?)?
        .checked_add(
            (required_bit_count as u128)
                .checked_mul(support_word_count)?
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?
        .checked_add(word_bytes.checked_mul(2)?)?;
    let packing_bound_scratch = pattern_slots
        .checked_mul(core::mem::size_of::<usize>() as u128)?
        .checked_mul(3)?
        .checked_add(row_count.checked_mul(core::mem::size_of::<u32>() as u128)?)?
        .checked_add(row_count.checked_mul(core::mem::size_of::<usize>() as u128)?)?
        .checked_add(row_word_bytes)?;
    let row_index_scratch = row_count
        .checked_mul(core::mem::size_of::<usize>() as u128)?
        .checked_mul(7)?;
    let recursive_branch_scratch = row_count
        .checked_mul(row_count)?
        .checked_mul(core::mem::size_of::<usize>() as u128)?;
    let selected_scratch = row_count
        .checked_mul(core::mem::size_of::<bool>() as u128)?
        .checked_mul(3)?;
    let recursive_changed_scratch = row_count
        .checked_mul(word_count)?
        .checked_mul(core::mem::size_of::<(usize, u64)>() as u128)?;
    let recursive_unit_scratch = row_count
        .checked_mul(row_count)?
        .checked_mul(core::mem::size_of::<usize>() as u128)?
        .checked_add(row_count.checked_mul(word_bytes)?)?;
    let randomized_upper_bound_scratch =
        word_bytes
            .checked_mul(2)?
            .checked_add(row_count.checked_mul(
                (core::mem::size_of::<bool>()
                    + core::mem::size_of::<usize>()
                    + core::mem::size_of::<(usize, usize)>()) as u128,
            )?)?;
    let breakout_upper_bound_scratch = word_bytes
        .checked_mul(2)?
        .checked_add(
            pattern_slots
                .checked_mul(core::mem::size_of::<usize>() as u128)?
                .checked_mul(2)?,
        )?
        .checked_add(row_count.checked_mul(
            (core::mem::size_of::<bool>()
                + core::mem::size_of::<usize>() * 2
                + core::mem::size_of::<BreakoutSwapCandidate>()) as u128,
        )?)?;
    let residual_dual_workspace =
        checked_maximum_residual_dual_workspace_bytes(rows.len(), required_bit_count)?;
    let persistent_dual_certificate =
        checked_maximum_persistent_dual_certificate_bytes(required_bit_count)?;
    let fixed_workspace_bytes = dense_rows
        .checked_add(materialization_rows)?
        .checked_add(word_bytes.checked_mul(7)?)?
        .checked_add(support_slots)?
        .checked_add(dominance_index_scratch)?
        .checked_add(target_constraint_scratch)?
        .checked_add(packing_bound_scratch)?
        .checked_add(row_index_scratch)?
        .checked_add(recursive_branch_scratch)?
        .checked_add(selected_scratch)?
        .checked_add(recursive_changed_scratch)?
        .checked_add(recursive_unit_scratch)?
        .checked_add(randomized_upper_bound_scratch)?
        .checked_add(breakout_upper_bound_scratch)?
        .checked_add(residual_dual_workspace)?
        .checked_add(persistent_dual_certificate)?
        .checked_add(PatternBitSet::checked_shared_construction_upper_bound(
            required.pattern_count(),
            1,
            required_bit_count as u128,
        )?)?;
    let required_peak_bytes = fixed_workspace_bytes.checked_add(memo_state_bytes_upper_bound)?;
    Some(ExactMinimumCoverMemoryProjection {
        memo_state_upper_bound,
        memo_state_bytes_upper_bound,
        fixed_workspace_bytes,
        required_peak_bytes,
    })
}

/// Runs the exact solver while reporting its complete currently-owned heap plus
/// the next requested allocation before each allocation and its actual
/// capacity immediately afterwards. The caller owns all external-live memory
/// accounting and may reject any reported peak.
pub fn exact_minimum_cover_with_memory_guard(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    let mut session =
        ExactMinimumCoverSession::new_with_memory_guard(required, rows, memory_guard)?;
    loop {
        match session
            .advance_with_memory_guard_and_control(u64::MAX, memory_guard, &mut || false)?
        {
            ExactMinimumCoverSessionAdvance::Pending { .. } => {}
            ExactMinimumCoverSessionAdvance::Found { result, .. } => return Ok(result),
            ExactMinimumCoverSessionAdvance::ProvedNone { .. }
            | ExactMinimumCoverSessionAdvance::Cancelled { .. } => {
                unreachable!("unbounded minimum-cover search cannot be cancelled or infeasible")
            }
            ExactMinimumCoverSessionAdvance::Finished => {
                unreachable!("blocking minimum-cover driver consumes one terminal result")
            }
        }
    }
}

pub fn exact_cover_at_most_with_memory_guard(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<ExactCoverAtMostResult>, ExactMinimumCoverError> {
    match exact_cover_at_most_with_memory_guard_and_control(
        required,
        rows,
        limit,
        memory_guard,
        &mut || false,
    )? {
        ExactCoverAtMostDecision::Found(result) => Ok(Some(result)),
        ExactCoverAtMostDecision::ProvedNone => Ok(None),
        ExactCoverAtMostDecision::Cancelled => {
            unreachable!("non-cancellable exact-cover wrapper cannot be cancelled")
        }
    }
}

pub fn exact_cover_at_most_with_memory_guard_and_control(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverAtMostDecision, ExactMinimumCoverError> {
    Ok(
        match exact_cover_with_goal_memory_guard(
            required,
            rows,
            ExactCoverSearchGoal::AtMost(limit),
            ExactCoverIncumbentPolicy::Standard,
            None,
            memory_guard,
            cancelled,
        )? {
            ExactCoverInternalDecision::Found(result) => {
                ExactCoverAtMostDecision::Found(ExactCoverAtMostResult {
                    row_indices: result.row_indices,
                    covered_patterns: result.covered_patterns,
                })
            }
            ExactCoverInternalDecision::ProvedNone => ExactCoverAtMostDecision::ProvedNone,
            ExactCoverInternalDecision::Cancelled => ExactCoverAtMostDecision::Cancelled,
        },
    )
}

/// Runs the same exact at-most decision as the plain entry point, but permits
/// one bounded deterministic incumbent search over the caller's original row
/// identities before dominance reduction. This is kept inside the `cover`
/// module for the absolute-first portfolio refinement; all negative and
/// successor decisions retain the lean plain path. A heuristic miss has no
/// proof authority and falls through to the unchanged exact decision.
#[cfg(test)]
pub(super) fn exact_cover_at_most_with_witness_search_memory_guard_and_control(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
    witness_hint: &[usize],
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverAtMostDecision, ExactMinimumCoverError> {
    Ok(
        match exact_cover_with_goal_memory_guard(
            required,
            rows,
            ExactCoverSearchGoal::AtMost(limit),
            ExactCoverIncumbentPolicy::WitnessAssisted,
            Some(witness_hint),
            memory_guard,
            cancelled,
        )? {
            ExactCoverInternalDecision::Found(result) => {
                ExactCoverAtMostDecision::Found(ExactCoverAtMostResult {
                    row_indices: result.row_indices,
                    covered_patterns: result.covered_patterns,
                })
            }
            ExactCoverInternalDecision::ProvedNone => ExactCoverAtMostDecision::ProvedNone,
            ExactCoverInternalDecision::Cancelled => ExactCoverAtMostDecision::Cancelled,
        },
    )
}

fn exact_cover_with_goal_memory_guard(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    goal: ExactCoverSearchGoal,
    incumbent_policy: ExactCoverIncumbentPolicy,
    witness_hint: Option<&[usize]>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverInternalDecision, ExactMinimumCoverError> {
    let mut session = if matches!(goal, ExactCoverSearchGoal::AtMost(_)) {
        // Preserve the legacy blocking AtMost wrapper's eager accelerations and
        // guard-observation contract. Product portfolio callers bypass this
        // wrapper and use the lazy internal AtMost constructor below.
        prepare_exact_cover_search_session(
            required,
            rows,
            goal,
            incumbent_policy,
            witness_hint,
            memory_guard,
            cancelled,
            true,
        )?
    } else {
        prepare_lazy_exact_cover_search_session(
            required,
            rows,
            goal,
            incumbent_policy,
            witness_hint,
            memory_guard,
            cancelled,
        )?
    };
    loop {
        match session.advance(u64::MAX, memory_guard, cancelled)? {
            ExactMinimumCoverSessionAdvance::Pending { .. } => {}
            ExactMinimumCoverSessionAdvance::Found { result, .. } => {
                return Ok(ExactCoverInternalDecision::Found(result));
            }
            ExactMinimumCoverSessionAdvance::ProvedNone { .. } => {
                return Ok(ExactCoverInternalDecision::ProvedNone);
            }
            ExactMinimumCoverSessionAdvance::Cancelled { .. } => {
                return Ok(ExactCoverInternalDecision::Cancelled);
            }
            ExactMinimumCoverSessionAdvance::Finished => {
                unreachable!("blocking exact-cover driver consumes one terminal result")
            }
        }
    }
}

fn prepare_lazy_exact_cover_search_session(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    goal: ExactCoverSearchGoal,
    incumbent_policy: ExactCoverIncumbentPolicy,
    witness_hint: Option<&[usize]>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverSearchSession, ExactMinimumCoverError> {
    if cancelled() {
        return Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::Cancelled,
        ));
    }
    memory_guard(0)?;
    for (row_index, row) in rows.iter().enumerate() {
        if row.pattern_count() != required.pattern_count() {
            return Err(ExactMinimumCoverError::RowPatternCountMismatch {
                row_index,
                expected: required.pattern_count(),
                actual: row.pattern_count(),
            });
        }
    }
    if required.is_empty() {
        return Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::Found(ExactMinimumCoverResult {
                row_indices: Vec::new(),
                covered_patterns: PatternBitSet::new(required.pattern_count()),
                complete: true,
            }),
        ));
    }

    let required = required.clone();
    let required_live = required
        .checked_storage_retained_bytes()
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut owned_rows = try_vec_with_capacity(
        rows.len(),
        required_live,
        memory_guard,
        "exact_minimum_cover_session_input_rows",
    )?;
    owned_rows.extend(rows.iter().cloned());
    let rows_live = checked_pattern_bitset_rows_retained_bytes(&owned_rows)?;
    let witness_hint = match witness_hint {
        Some(hint) => {
            let mut owned = try_vec_with_capacity(
                hint.len(),
                required_live
                    .checked_add(rows_live)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                "exact_minimum_cover_session_witness_hint",
            )?;
            owned.extend_from_slice(hint);
            Some(owned)
        }
        None => None,
    };
    let witness_live = witness_hint
        .as_ref()
        .map_or(Ok(0), checked_vec_retained_bytes)?;
    memory_guard(
        required_live
            .checked_add(rows_live)
            .and_then(|bytes| bytes.checked_add(witness_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(ExactCoverSearchSession {
        state: ExactCoverSearchSessionState::Unprepared {
            required,
            rows: owned_rows,
            goal,
            incumbent_policy,
            witness_hint,
        },
    })
}

fn prepare_lazy_initial_reduction(
    required: PatternBitSet,
    source_rows: Vec<PatternBitSet>,
    goal: ExactCoverSearchGoal,
    incumbent_policy: ExactCoverIncumbentPolicy,
    witness_hint: Option<Vec<usize>>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverSearchSessionState, ExactMinimumCoverError> {
    let input_live = required
        .checked_storage_retained_bytes()
        .and_then(|bytes| {
            bytes.checked_add(checked_pattern_bitset_rows_retained_bytes(&source_rows).ok()?)
        })
        .and_then(|bytes| {
            bytes.checked_add(checked_optional_vec_retained_bytes(&witness_hint).ok()?)
        })
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    drop(witness_hint);
    let mut dense_rows = try_vec_with_capacity(
        source_rows.len(),
        input_live,
        memory_guard,
        "exact_minimum_cover_dense_rows",
    )?;
    for (source_index, row) in source_rows.iter().enumerate() {
        if cancelled() {
            memory_guard(0)?;
            return Ok(ExactCoverSearchSessionState::Ready(Some(
                ExactCoverInternalDecision::Cancelled,
            )));
        }
        let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
        let mut words = try_vec_with_capacity(
            required.word_count(),
            input_live
                .checked_add(dense_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_dense_row_words",
        )?;
        let mut nonempty = false;
        for word_index in 0..required.word_count() {
            let word = row.word_at(word_index) & required.word_at(word_index);
            nonempty |= word != 0;
            words.push(word);
        }
        if nonempty {
            dense_rows.push(DenseRow {
                source_index,
                words,
            });
        }
        memory_guard(
            input_live
                .checked_add(checked_dense_rows_retained_bytes(&dense_rows)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
    }
    remove_dominated_rows_with_memory_guard(&mut dense_rows, &mut |solver_live| {
        memory_guard(
            input_live
                .checked_add(solver_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )
    })?;

    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let base_live = input_live
        .checked_add(dense_live)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut coverable_words = try_vec_with_capacity(
        required.word_count(),
        base_live,
        memory_guard,
        "exact_minimum_cover_coverable_words",
    )?;
    coverable_words.resize(required.word_count(), 0);
    for row in &dense_rows {
        union_words(&mut coverable_words, &row.words);
    }
    let complete = (0..required.word_count())
        .all(|index| coverable_words[index] & required.word_at(index) == required.word_at(index));
    if matches!(goal, ExactCoverSearchGoal::AtMost(_)) && !complete {
        memory_guard(0)?;
        return Ok(ExactCoverSearchSessionState::Ready(Some(
            ExactCoverInternalDecision::ProvedNone,
        )));
    }
    let coverable_live = checked_vec_retained_bytes(&coverable_words)?;
    let mut target_words = try_vec_with_capacity(
        required.word_count(),
        base_live
            .checked_add(coverable_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_target_words",
    )?;
    for (word_index, coverable) in coverable_words.iter().copied().enumerate() {
        target_words.push(required.word_at(word_index) & coverable);
    }
    drop(coverable_words);
    let target_live = checked_vec_retained_bytes(&target_words)?;
    let target_count = target_words
        .iter()
        .map(|word| word.count_ones() as usize)
        .try_fold(0_usize, |sum, count| sum.checked_add(count))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut target_weights = try_vec_with_capacity(
        target_count,
        base_live
            .checked_add(target_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_target_weights",
    )?;
    target_weights.resize(target_count, 1);
    let context = LazyExactCoverReductionContext {
        required,
        source_rows,
        goal,
        incumbent_policy,
        dense_rows,
        complete,
    };
    memory_guard(
        checked_lazy_reduction_context_retained_bytes(&context)?
            .checked_add(checked_vec_retained_bytes(&target_words)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&target_weights).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(ExactCoverSearchSessionState::InitialQuotient {
        context,
        target_words,
        target_weights,
    })
}

fn prepare_exact_cover_search_session(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    goal: ExactCoverSearchGoal,
    incumbent_policy: ExactCoverIncumbentPolicy,
    witness_hint: Option<&[usize]>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
    eager_acceleration: bool,
) -> Result<ExactCoverSearchSession, ExactMinimumCoverError> {
    if cancelled() {
        return Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::Cancelled,
        ));
    }
    memory_guard(0)?;
    for (row_index, row) in rows.iter().enumerate() {
        if row.pattern_count() != required.pattern_count() {
            return Err(ExactMinimumCoverError::RowPatternCountMismatch {
                row_index,
                expected: required.pattern_count(),
                actual: row.pattern_count(),
            });
        }
    }
    if required.is_empty() {
        return Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::Found(ExactMinimumCoverResult {
                row_indices: Vec::new(),
                covered_patterns: PatternBitSet::new(required.pattern_count()),
                complete: true,
            }),
        ));
    }

    let mut dense_rows = try_vec_with_capacity(
        rows.len(),
        0,
        memory_guard,
        "exact_minimum_cover_dense_rows",
    )?;
    for (source_index, row) in rows.iter().enumerate() {
        let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
        let mut words = try_vec_with_capacity(
            required.word_count(),
            dense_live,
            memory_guard,
            "exact_minimum_cover_dense_row_words",
        )?;
        let mut nonempty = false;
        for word_index in 0..required.word_count() {
            let word = row.word_at(word_index) & required.word_at(word_index);
            nonempty |= word != 0;
            words.push(word);
        }
        if nonempty {
            dense_rows.push(DenseRow {
                source_index,
                words,
            });
        }
        memory_guard(checked_dense_rows_retained_bytes(&dense_rows)?)?;
    }
    let incumbent_policy =
        if incumbent_policy == ExactCoverIncumbentPolicy::WitnessAssisted && eager_acceleration {
            let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
            let ExactCoverSearchGoal::AtMost(limit) = goal else {
                return Err(ExactMinimumCoverError::ProjectionOverflow);
            };
            match witness_assisted_cover_before_dominance(
                required,
                &dense_rows,
                limit,
                witness_hint.unwrap_or_default(),
                dense_live,
                memory_guard,
                cancelled,
            )? {
                WitnessAssistedCoverDecision::Found(selected_rows) => {
                    let decision = materialize_internal_result(
                        required,
                        rows,
                        dense_rows,
                        selected_rows,
                        true,
                        memory_guard,
                    )?;
                    return Ok(ExactCoverSearchSession::ready(decision));
                }
                WitnessAssistedCoverDecision::Miss => {
                    ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch
                }
                WitnessAssistedCoverDecision::Cancelled => {
                    return Ok(ExactCoverSearchSession::ready(
                        ExactCoverInternalDecision::Cancelled,
                    ));
                }
            }
        } else {
            if incumbent_policy == ExactCoverIncumbentPolicy::WitnessAssisted {
                // Portfolio sessions retain the hint for future acceleration, but
                // never run its multi-thousand-trial heuristic inside one ABI
                // advance. Exact AtMost search remains the authority.
                ExactCoverIncumbentPolicy::Standard
            } else {
                incumbent_policy
            }
        };
    remove_dominated_rows_with_memory_guard(&mut dense_rows, memory_guard)?;

    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let mut coverable_words = try_vec_with_capacity(
        required.word_count(),
        dense_live,
        memory_guard,
        "exact_minimum_cover_coverable_words",
    )?;
    coverable_words.resize(required.word_count(), 0);
    for row in &dense_rows {
        union_words(&mut coverable_words, &row.words);
    }
    let complete = (0..required.word_count())
        .all(|index| coverable_words[index] & required.word_at(index) == required.word_at(index));
    if matches!(goal, ExactCoverSearchGoal::AtMost(_)) && !complete {
        return Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::ProvedNone,
        ));
    }
    let coverable_live = checked_vec_retained_bytes(&coverable_words)?;
    let mut target_words = try_vec_with_capacity(
        required.word_count(),
        dense_live
            .checked_add(coverable_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_target_words",
    )?;
    for (word_index, coverable) in coverable_words.iter().copied().enumerate() {
        target_words.push(required.word_at(word_index) & coverable);
    }
    drop(coverable_words);
    memory_guard(
        dense_live
            .checked_add(checked_vec_retained_bytes(&target_words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let target_live = checked_vec_retained_bytes(&target_words)?;
    let target_count = target_words
        .iter()
        .map(|word| word.count_ones() as usize)
        .try_fold(0_usize, |sum, count| sum.checked_add(count))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut target_weights = try_vec_with_capacity(
        target_count,
        dense_live
            .checked_add(target_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_target_weights",
    )?;
    target_weights.resize(target_count, 1);
    let mut compact_target = quotient_redundant_target_constraints_with_memory_guard(
        &mut dense_rows,
        target_words,
        target_weights,
        true,
        memory_guard,
    )?;
    loop {
        let previous_row_count = dense_rows.len();
        let previous_constraint_count = compact_target.weights.len();
        let compact_target_live = checked_vec_retained_bytes(&compact_target.words)?
            .checked_add(checked_vec_retained_bytes(&compact_target.weights)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        remove_dominated_rows_with_memory_guard(&mut dense_rows, &mut |row_solver_bytes| {
            memory_guard(
                row_solver_bytes
                    .checked_add(compact_target_live)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
        })?;
        compact_target = quotient_redundant_target_constraints_with_memory_guard(
            &mut dense_rows,
            compact_target.words,
            compact_target.weights,
            true,
            memory_guard,
        )?;
        if dense_rows.len() == previous_row_count
            && compact_target.weights.len() == previous_constraint_count
        {
            break;
        }
    }
    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let materialization_words = build_materialization_words_with_memory_guard(
        required,
        rows,
        &dense_rows,
        dense_live,
        memory_guard,
    )?;
    let materialization_live = checked_nested_words_retained_bytes(&materialization_words)?;
    let search_base_live = dense_live
        .checked_add(materialization_live)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    if compact_target.words.iter().all(|word| *word == 0) {
        let decision = materialize_internal_result_from_owned_words(
            required.pattern_count(),
            dense_rows,
            materialization_words,
            Vec::new(),
            complete,
            memory_guard,
        )?;
        return Ok(ExactCoverSearchSession::ready(decision));
    }
    let search_preparation = MinimumCoverSearch::try_new(
        &dense_rows,
        compact_target.words,
        compact_target.weights,
        goal,
        incumbent_policy,
        search_base_live,
        memory_guard,
        cancelled,
        eager_acceleration,
    )?;
    match search_preparation {
        MinimumCoverSearchPreparation::Search(search) => Ok(ExactCoverSearchSession {
            state: ExactCoverSearchSessionState::Searching {
                pattern_count: required.pattern_count(),
                complete,
                dense_rows,
                materialization_words,
                search,
            },
        }),
        MinimumCoverSearchPreparation::Found(selected_rows) => {
            let decision = materialize_internal_result_from_owned_words(
                required.pattern_count(),
                dense_rows,
                materialization_words,
                selected_rows,
                complete,
                memory_guard,
            )?;
            Ok(ExactCoverSearchSession::ready(decision))
        }
        MinimumCoverSearchPreparation::Cancelled => Ok(ExactCoverSearchSession::ready(
            ExactCoverInternalDecision::Cancelled,
        )),
    }
}

fn prepare_lazy_search_workspace(
    context: LazyExactCoverReductionContext,
    compact_target: ExactCompactTarget,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ExactCoverSearchSessionState, ExactMinimumCoverError> {
    let LazyExactCoverReductionContext {
        required,
        source_rows,
        goal,
        incumbent_policy,
        dense_rows,
        complete,
    } = context;
    let source_live = required
        .checked_storage_retained_bytes()
        .and_then(|bytes| {
            bytes.checked_add(checked_pattern_bitset_rows_retained_bytes(&source_rows).ok()?)
        })
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let pattern_count = required.pattern_count();
    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let materialization_words = build_materialization_words_with_memory_guard(
        &required,
        &source_rows,
        &dense_rows,
        source_live
            .checked_add(dense_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
    )?;
    let materialization_live = checked_nested_words_retained_bytes(&materialization_words)?;
    let search_base_live = source_live
        .checked_add(dense_live)
        .and_then(|bytes| bytes.checked_add(materialization_live))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    if compact_target.words.iter().all(|word| *word == 0) {
        let decision = materialize_internal_result_from_owned_words(
            required.pattern_count(),
            dense_rows,
            materialization_words,
            Vec::new(),
            complete,
            memory_guard,
        )?;
        return Ok(ExactCoverSearchSessionState::Ready(Some(decision)));
    }
    // The blocking witness path runs its post-dominance breakout against the
    // actual greedy incumbent, before `AtMost` replaces a too-large incumbent
    // with its `limit + 1` sentinel.  Preserve that seed outside the search so
    // the cooperative phase can replay the same ordering without storing
    // sentinel row IDs in the breakout cursor.
    let deferred_breakout_seed = if matches!(
        (goal, incumbent_policy),
        (
            ExactCoverSearchGoal::AtMost(_),
            ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch,
        )
    ) {
        greedy_cover_with_memory_guard(
            &dense_rows,
            &compact_target.words,
            &compact_target.weights,
            search_base_live,
            memory_guard,
        )?
    } else {
        None
    };
    let deferred_seed_live = deferred_breakout_seed
        .as_ref()
        .map_or(Ok(0), checked_vec_retained_bytes)?;
    let search_construction_live = search_base_live
        .checked_add(deferred_seed_live)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let search_preparation = MinimumCoverSearch::try_new(
        &dense_rows,
        compact_target.words,
        compact_target.weights,
        goal,
        ExactCoverIncumbentPolicy::Standard,
        search_construction_live,
        memory_guard,
        cancelled,
        false,
    )?;
    drop(source_rows);
    drop(required);
    match search_preparation {
        MinimumCoverSearchPreparation::Search(search) => {
            let should_resume_breakout = matches!(
                (goal, incumbent_policy),
                (
                    ExactCoverSearchGoal::AtMost(limit),
                    ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch,
                ) if deferred_breakout_seed
                    .as_ref()
                    .is_some_and(|seed| seed.len() > limit)
            ) && dense_rows.len() >= 64
                && search.constraint_weights.len() >= 128
                && deferred_breakout_seed
                    .as_ref()
                    .is_some_and(|seed| seed.len() > 1);
            if should_resume_breakout {
                let base_live = checked_dense_rows_retained_bytes(&dense_rows)?
                    .checked_add(checked_nested_words_retained_bytes(&materialization_words)?)
                    .and_then(|bytes| bytes.checked_add(search.checked_heap_retained_bytes().ok()?))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                let seed = deferred_breakout_seed
                    .expect("post-witness breakout condition checked its seed");
                if let Some(incumbent_search) = FixedCardinalityCoverSearchSession::try_new(
                    &dense_rows,
                    &search.target_words,
                    &search.support_by_pattern,
                    seed,
                    WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
                    None,
                    base_live,
                    memory_guard,
                )? {
                    return Ok(ExactCoverSearchSessionState::ImprovingBreakout {
                        pattern_count,
                        complete,
                        dense_rows,
                        materialization_words,
                        search,
                        incumbent_search,
                    });
                }
            }
            Ok(ExactCoverSearchSessionState::PreparingRootDual {
                pattern_count,
                complete,
                dense_rows,
                materialization_words,
                search,
            })
        }
        MinimumCoverSearchPreparation::Found(selected_rows) => {
            drop(deferred_breakout_seed);
            let decision = materialize_internal_result_from_owned_words(
                pattern_count,
                dense_rows,
                materialization_words,
                selected_rows,
                complete,
                memory_guard,
            )?;
            Ok(ExactCoverSearchSessionState::Ready(Some(decision)))
        }
        MinimumCoverSearchPreparation::Cancelled => {
            drop(deferred_breakout_seed);
            Ok(ExactCoverSearchSessionState::Ready(Some(
                ExactCoverInternalDecision::Cancelled,
            )))
        }
    }
}

impl ExactCoverSearchSession {
    #[cfg(any(test, feature = "diagnostic-probes"))]
    pub(super) fn diagnostic_residual_warm_seed(
        &self,
    ) -> Option<ExactMinimumCoverWarmSeedDiagnostics> {
        let search = match &self.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingBreakout { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return None,
        };
        search
            .dual_workspace
            .as_ref()
            .map(DualProposalWorkspace::diagnostic_warm_seed)
    }

    #[cfg(any(test, feature = "diagnostic-probes"))]
    pub(super) fn diagnostic_conditional_rows(
        &self,
    ) -> Option<ExactMinimumCoverConditionalRowDiagnostics> {
        match &self.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingBreakout { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => {
                Some(search.diagnostic_conditional_rows)
            }
            _ => None,
        }
    }
    #[cfg(feature = "diagnostic-probes")]
    pub(super) fn diagnostic_hot_cost(&self) -> Option<ExactMinimumCoverHotCostDiagnostics> {
        let search = match &self.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return None,
        };
        Some(search.diagnostic_hot_cost())
    }

    #[cfg(feature = "diagnostic-probes")]
    pub(super) fn diagnostic_residual_progress(
        &self,
    ) -> Option<ExactMinimumCoverResidualDiagnostics> {
        let search = match &self.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return None,
        };
        Some(search.diagnostic_residual_progress())
    }

    /// The parallel coordinator may spend a bounded positive-only warm pass
    /// before releasing cubes. A warm miss must stop before entering exact
    /// reductions/DFS; it is not a negative answer for any partition.
    pub(super) fn witness_shortcut_exhausted(&self) -> bool {
        match &self.state {
            ExactCoverSearchSessionState::Unprepared {
                incumbent_policy, ..
            } => *incumbent_policy != ExactCoverIncumbentPolicy::WitnessAssisted,
            // This predicate is called only by the global owner. Worker and
            // serial cursors keep the ordinary forced/randomized fallback.
            ExactCoverSearchSessionState::WitnessShortcut(session) => matches!(
                session.phase,
                WitnessShortcutPhase::PrepareForcedSupporters
                    | WitnessShortcutPhase::ForcedGreedy { .. }
                    | WitnessShortcutPhase::Randomized { .. }
            ),
            _ => false,
        }
    }

    fn ready(decision: ExactCoverInternalDecision) -> Self {
        Self {
            state: ExactCoverSearchSessionState::Ready(Some(decision)),
        }
    }

    /// Internal seam for the canonical portfolio self-reduction. It shares the
    /// same owned, resumable DFS authority as minimum proof construction; the
    /// portfolio layer can add its transactional staged-clone policy without
    /// introducing a second exact solver.
    pub(super) fn prepare_at_most_with_memory_guard_and_control(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        limit: usize,
        witness_hint: Option<&[usize]>,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Self, ExactMinimumCoverError> {
        let incumbent_policy = if witness_hint.is_some() {
            ExactCoverIncumbentPolicy::WitnessAssisted
        } else {
            ExactCoverIncumbentPolicy::Standard
        };
        prepare_lazy_exact_cover_search_session(
            required,
            rows,
            ExactCoverSearchGoal::AtMost(limit),
            incumbent_policy,
            witness_hint,
            memory_guard,
            cancelled,
        )
    }

    /// Heap retained by this owned proof session. Inline enum/struct storage is
    /// excluded, matching the surrounding portfolio transaction accounting.
    pub(super) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match &self.state {
            ExactCoverSearchSessionState::Unprepared {
                required,
                rows,
                witness_hint,
                ..
            } => required
                .checked_storage_retained_bytes()?
                .checked_add(checked_pattern_bitset_rows_retained_bytes(rows).ok()?)?
                .checked_add(checked_optional_vec_retained_bytes(witness_hint).ok()?),
            ExactCoverSearchSessionState::WitnessShortcut(session) => {
                session.checked_retained_capacity_bytes().ok()
            }
            ExactCoverSearchSessionState::InitialQuotient {
                context,
                target_words,
                target_weights,
            } => checked_lazy_reduction_context_retained_bytes(context)
                .ok()?
                .checked_add(checked_vec_retained_bytes(target_words).ok()?)?
                .checked_add(checked_vec_retained_bytes(target_weights).ok()?),
            ExactCoverSearchSessionState::FixedPointDominance {
                context,
                compact_target,
            }
            | ExactCoverSearchSessionState::FixedPointQuotient {
                context,
                compact_target,
                ..
            }
            | ExactCoverSearchSessionState::BuildSearch {
                context,
                compact_target,
            } => checked_lazy_reduction_context_retained_bytes(context)
                .ok()?
                .checked_add(checked_exact_compact_target_retained_bytes(compact_target).ok()?),
            ExactCoverSearchSessionState::Ready(Some(ExactCoverInternalDecision::Found(
                result,
            ))) => result.checked_retained_bytes(),
            ExactCoverSearchSessionState::Ready(_) | ExactCoverSearchSessionState::Finished => {
                Some(0)
            }
            ExactCoverSearchSessionState::ImprovingIncumbent {
                dense_rows,
                materialization_words,
                search,
                incumbent_search,
                ..
            } => checked_dense_rows_retained_bytes(dense_rows)
                .ok()?
                .checked_add(checked_nested_words_retained_bytes(materialization_words).ok()?)?
                .checked_add(search.checked_heap_retained_bytes().ok()?)?
                .checked_add(incumbent_search.checked_retained_capacity_bytes().ok()?),
            ExactCoverSearchSessionState::ImprovingBreakout {
                dense_rows,
                materialization_words,
                search,
                incumbent_search,
                ..
            } => checked_dense_rows_retained_bytes(dense_rows)
                .ok()?
                .checked_add(checked_nested_words_retained_bytes(materialization_words).ok()?)?
                .checked_add(search.checked_heap_retained_bytes().ok()?)?
                .checked_add(incumbent_search.checked_retained_capacity_bytes().ok()?),
            ExactCoverSearchSessionState::PreparingRootDual {
                dense_rows,
                materialization_words,
                search,
                ..
            }
            | ExactCoverSearchSessionState::Searching {
                dense_rows,
                materialization_words,
                search,
                ..
            } => checked_dense_rows_retained_bytes(dense_rows)
                .ok()?
                .checked_add(checked_nested_words_retained_bytes(materialization_words).ok()?)?
                .checked_add(search.checked_heap_retained_bytes().ok()?),
        }
    }

    /// Stages an independent resumable oracle under the caller's whole-live
    /// guard. The ordinary `Clone` implementation remains available for the
    /// crate's pre-existing value contracts; production page transactions use
    /// this seam so a configured capacity rejection occurs before cloning.
    pub(super) fn try_clone_with_memory_guard(
        &self,
        external_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        let retained = self
            .checked_retained_capacity_bytes()
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        memory_guard(
            external_live_bytes
                .checked_add(retained)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        let cloned = self.clone();
        let cloned_retained = cloned
            .checked_retained_capacity_bytes()
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        memory_guard(
            external_live_bytes
                .checked_add(cloned_retained)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(cloned)
    }

    pub(super) fn advance(
        &mut self,
        max_nodes: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverSessionAdvance, ExactMinimumCoverError> {
        const MAX_RANDOMIZED_TRIALS_PER_ADVANCE: u64 = 2;

        let mut visited_nodes = 0_u64;
        loop {
            let state = core::mem::replace(&mut self.state, ExactCoverSearchSessionState::Finished);
            match state {
                ExactCoverSearchSessionState::Unprepared {
                    required,
                    rows,
                    goal,
                    incumbent_policy,
                    witness_hint,
                } => {
                    if cancelled() {
                        drop(witness_hint);
                        drop(rows);
                        drop(required);
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::Unprepared {
                            required,
                            rows,
                            goal,
                            incumbent_policy,
                            witness_hint,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    if incumbent_policy == ExactCoverIncumbentPolicy::WitnessAssisted
                        && matches!(goal, ExactCoverSearchGoal::AtMost(_))
                        && witness_hint.is_some()
                    {
                        self.state = ExactCoverSearchSessionState::WitnessShortcut(
                            WitnessShortcutSession::new(
                                required,
                                rows,
                                goal,
                                witness_hint.expect("witness existence was checked"),
                            ),
                        );
                        visited_nodes += 1;
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    self.state = prepare_lazy_initial_reduction(
                        required,
                        rows,
                        goal,
                        incumbent_policy,
                        witness_hint,
                        memory_guard,
                        cancelled,
                    )?;
                    visited_nodes = visited_nodes
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    memory_guard(
                        self.checked_retained_capacity_bytes()
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )?;
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::WitnessShortcut(mut session) => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::WitnessShortcut(session);
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let step = session.advance_one(memory_guard, cancelled)?;
                    visited_nodes = visited_nodes
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    match step {
                        WitnessShortcutStep::Pending => {
                            self.state = ExactCoverSearchSessionState::WitnessShortcut(session);
                            return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                        }
                        WitnessShortcutStep::FoundDense(selected_rows) => {
                            let decision =
                                session.materialize_dense_result(selected_rows, memory_guard)?;
                            self.state = ExactCoverSearchSessionState::Ready(Some(decision));
                            return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                        }
                        WitnessShortcutStep::Miss => {
                            let repaired_incumbent = matches!(
                                session.goal,
                                ExactCoverSearchGoal::AtMost(limit)
                                    if limit.checked_add(1) == Some(session.witness_hint.len())
                            );
                            let (required, rows, goal) = session.into_fallback_parts();
                            self.state = ExactCoverSearchSessionState::Unprepared {
                                required,
                                rows,
                                goal,
                                // The blocking authority runs one additional
                                // unprotected breakout over the reduced matrix
                                // after the raw witness shortcut misses.  Carry
                                // that performance-only policy through lazy
                                // reductions; `ImprovingBreakout` executes the
                                // same search cooperatively before exact DFS.
                                incumbent_policy: if repaired_incumbent {
                                    // This cube already spent its bounded
                                    // k+1 repair allowance; do not repeat it
                                    // over the reduced matrix before DFS.
                                    ExactCoverIncumbentPolicy::Standard
                                } else {
                                    ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch
                                },
                                witness_hint: None,
                            };
                            return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                        }
                        WitnessShortcutStep::Cancelled => {
                            memory_guard(0)?;
                            return Ok(ExactMinimumCoverSessionAdvance::Cancelled {
                                visited_nodes,
                            });
                        }
                    }
                }
                ExactCoverSearchSessionState::InitialQuotient {
                    mut context,
                    target_words,
                    target_weights,
                } => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::InitialQuotient {
                            context,
                            target_words,
                            target_weights,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let context_live = checked_lazy_reduction_context_retained_bytes(&context)?;
                    let dense_live = checked_dense_rows_retained_bytes(&context.dense_rows)?;
                    let non_dense_context_live = context_live
                        .checked_sub(dense_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    let compact_target = quotient_redundant_target_constraints_with_memory_guard(
                        &mut context.dense_rows,
                        target_words,
                        target_weights,
                        true,
                        &mut |phase_live| {
                            memory_guard(
                                non_dense_context_live
                                    .checked_add(phase_live)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                    )?;
                    visited_nodes += 1;
                    self.state = ExactCoverSearchSessionState::FixedPointDominance {
                        context,
                        compact_target,
                    };
                    memory_guard(
                        self.checked_retained_capacity_bytes()
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )?;
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::FixedPointDominance {
                    mut context,
                    compact_target,
                } => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::FixedPointDominance {
                            context,
                            compact_target,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let previous_row_count = context.dense_rows.len();
                    let previous_constraint_count = compact_target.weights.len();
                    let context_live = checked_lazy_reduction_context_retained_bytes(&context)?;
                    let dense_live = checked_dense_rows_retained_bytes(&context.dense_rows)?;
                    let compact_live =
                        checked_exact_compact_target_retained_bytes(&compact_target)?;
                    let non_dense_live = context_live
                        .checked_sub(dense_live)
                        .and_then(|bytes| bytes.checked_add(compact_live))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    remove_dominated_rows_with_memory_guard(
                        &mut context.dense_rows,
                        &mut |phase_live| {
                            memory_guard(
                                non_dense_live
                                    .checked_add(phase_live)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                    )?;
                    visited_nodes += 1;
                    self.state = ExactCoverSearchSessionState::FixedPointQuotient {
                        context,
                        compact_target,
                        previous_row_count,
                        previous_constraint_count,
                    };
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::FixedPointQuotient {
                    mut context,
                    compact_target,
                    previous_row_count,
                    previous_constraint_count,
                } => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::FixedPointQuotient {
                            context,
                            compact_target,
                            previous_row_count,
                            previous_constraint_count,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let context_live = checked_lazy_reduction_context_retained_bytes(&context)?;
                    let dense_live = checked_dense_rows_retained_bytes(&context.dense_rows)?;
                    let non_dense_context_live = context_live
                        .checked_sub(dense_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    let compact_target = quotient_redundant_target_constraints_with_memory_guard(
                        &mut context.dense_rows,
                        compact_target.words,
                        compact_target.weights,
                        true,
                        &mut |phase_live| {
                            memory_guard(
                                non_dense_context_live
                                    .checked_add(phase_live)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                    )?;
                    visited_nodes += 1;
                    self.state = if context.dense_rows.len() == previous_row_count
                        && compact_target.weights.len() == previous_constraint_count
                    {
                        ExactCoverSearchSessionState::BuildSearch {
                            context,
                            compact_target,
                        }
                    } else {
                        ExactCoverSearchSessionState::FixedPointDominance {
                            context,
                            compact_target,
                        }
                    };
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::BuildSearch {
                    context,
                    compact_target,
                } => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::BuildSearch {
                            context,
                            compact_target,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    self.state = prepare_lazy_search_workspace(
                        context,
                        compact_target,
                        memory_guard,
                        cancelled,
                    )?;
                    visited_nodes += 1;
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::ImprovingBreakout {
                    pattern_count,
                    complete,
                    dense_rows,
                    materialization_words,
                    mut search,
                    mut incumbent_search,
                } => {
                    if cancelled() {
                        drop(incumbent_search);
                        drop(search);
                        drop(dense_rows);
                        drop(materialization_words);
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::ImprovingBreakout {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                            incumbent_search,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let step = incumbent_search.advance_one(
                        &dense_rows,
                        &search.target_words,
                        &search.support_by_pattern,
                        cancelled,
                    )?;
                    visited_nodes = visited_nodes
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    match step {
                        OptionalHeuristicStep::Found(selected_rows) => {
                            search.best.clear();
                            search.best.extend_from_slice(&selected_rows);
                            if search.feasibility_goal_is_satisfied() {
                                drop(selected_rows);
                                drop(incumbent_search);
                                let selected_rows = core::mem::take(&mut search.best);
                                drop(search);
                                let decision = materialize_internal_result_from_owned_words(
                                    pattern_count,
                                    dense_rows,
                                    materialization_words,
                                    selected_rows,
                                    complete,
                                    memory_guard,
                                )?;
                                self.state = ExactCoverSearchSessionState::Ready(Some(decision));
                            } else {
                                self.state = ExactCoverSearchSessionState::PreparingRootDual {
                                    pattern_count,
                                    complete,
                                    dense_rows,
                                    materialization_words,
                                    search,
                                };
                            }
                        }
                        OptionalHeuristicStep::Finished => {
                            self.state = ExactCoverSearchSessionState::PreparingRootDual {
                                pattern_count,
                                complete,
                                dense_rows,
                                materialization_words,
                                search,
                            };
                        }
                        OptionalHeuristicStep::Pending => {
                            self.state = ExactCoverSearchSessionState::ImprovingBreakout {
                                pattern_count,
                                complete,
                                dense_rows,
                                materialization_words,
                                search,
                                incumbent_search,
                            };
                        }
                        OptionalHeuristicStep::Cancelled => {
                            drop(incumbent_search);
                            drop(search);
                            drop(dense_rows);
                            drop(materialization_words);
                            memory_guard(0)?;
                            return Ok(ExactMinimumCoverSessionAdvance::Cancelled {
                                visited_nodes,
                            });
                        }
                    }
                    // One restart setup, swap, or terminal transition is the
                    // hard interactive slice for the reduced-matrix fallback.
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::PreparingRootDual {
                    pattern_count,
                    complete,
                    dense_rows,
                    materialization_words,
                    mut search,
                } => {
                    if cancelled() {
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if visited_nodes >= max_nodes {
                        self.state = ExactCoverSearchSessionState::PreparingRootDual {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
                    let materialization_live =
                        checked_nested_words_retained_bytes(&materialization_words)?;
                    let search_base_live = dense_live
                        .checked_add(materialization_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    search.prepare_root_dual_with_memory_guard(
                        200,
                        search_base_live,
                        memory_guard,
                    )?;
                    visited_nodes += 1;
                    let trial_limit = search.cooperative_randomized_trial_budget(&dense_rows);
                    self.state = if trial_limit != 0 {
                        let incumbent_base = search_base_live
                            .checked_add(search.checked_heap_retained_bytes()?)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                        let incumbent_search = RandomizedCompactCoverSearchSession::try_new(
                            &dense_rows,
                            &search.target_words,
                            0,
                            trial_limit,
                            RANDOMIZED_COMPACT_COVER_SEED,
                            None,
                            None,
                            incumbent_base,
                            memory_guard,
                        )?;
                        ExactCoverSearchSessionState::ImprovingIncumbent {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                            incumbent_search,
                        }
                    } else {
                        ExactCoverSearchSessionState::Searching {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                        }
                    };
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::Ready(mut decision) => {
                    let Some(decision) = decision.take() else {
                        return Ok(ExactMinimumCoverSessionAdvance::Finished);
                    };
                    return Ok(map_internal_session_decision(decision, visited_nodes));
                }
                ExactCoverSearchSessionState::ImprovingIncumbent {
                    pattern_count,
                    complete,
                    dense_rows,
                    materialization_words,
                    mut search,
                    mut incumbent_search,
                } => {
                    if cancelled() {
                        drop(search);
                        drop(dense_rows);
                        drop(materialization_words);
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    let remaining_budget = max_nodes.saturating_sub(visited_nodes);
                    if remaining_budget == 0 {
                        self.state = ExactCoverSearchSessionState::ImprovingIncumbent {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                            incumbent_search,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let batch = usize::try_from(
                        remaining_budget.min(MAX_RANDOMIZED_TRIALS_PER_ADVANCE).min(
                            incumbent_search
                                .trial_end
                                .saturating_sub(incumbent_search.next_trial)
                                as u64,
                        ),
                    )
                    .map_err(|_| ExactMinimumCoverError::ProjectionOverflow)?;
                    let before_trial = incumbent_search.next_trial;
                    let step = incumbent_search.advance(
                        &dense_rows,
                        &search.target_words,
                        &mut search.best,
                        batch,
                        cancelled,
                    )?;
                    let completed = incumbent_search.next_trial.saturating_sub(before_trial);
                    if completed != 0 {
                        visited_nodes = visited_nodes
                            .checked_add(completed as u64)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    }
                    if step == OptionalHeuristicStep::Cancelled {
                        drop(search);
                        drop(dense_rows);
                        drop(materialization_words);
                        memory_guard(0)?;
                        return Ok(ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes });
                    }
                    if step == OptionalHeuristicStep::Finished {
                        self.state = ExactCoverSearchSessionState::Searching {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                        };
                    } else {
                        self.state = ExactCoverSearchSessionState::ImprovingIncumbent {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                            incumbent_search,
                        };
                    }
                    // A tiny deterministic trial batch is the maximum safe
                    // heuristic slice; always hand control back before DFS.
                    return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                }
                ExactCoverSearchSessionState::Searching {
                    pattern_count,
                    complete,
                    dense_rows,
                    materialization_words,
                    mut search,
                } => {
                    let remaining_budget = max_nodes.saturating_sub(visited_nodes);
                    if remaining_budget == 0 {
                        self.state = ExactCoverSearchSessionState::Searching {
                            pattern_count,
                            complete,
                            dense_rows,
                            materialization_words,
                            search,
                        };
                        return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                    }
                    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
                    let materialization_live =
                        checked_nested_words_retained_bytes(&materialization_words)?;
                    let search_base_live = dense_live
                        .checked_add(materialization_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    // The search itself batches cheap DFS nodes and returns
                    // immediately after the first node that actually consumes
                    // an atomic residual-dual proposal. Keeping that policy
                    // inside the DFS loop avoids millions of one-node function
                    // re-entries while retaining at most one proposal per ABI
                    // call.
                    match search.advance(
                        &dense_rows,
                        remaining_budget.min(MAX_EXACT_NODES_PER_ADVANCE),
                        search_base_live,
                        memory_guard,
                        cancelled,
                    )? {
                        MinimumCoverSearchAdvance::Pending {
                            visited_nodes: advanced,
                            ..
                        } => {
                            visited_nodes = visited_nodes
                                .checked_add(advanced)
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                            self.state = ExactCoverSearchSessionState::Searching {
                                pattern_count,
                                complete,
                                dense_rows,
                                materialization_words,
                                search,
                            };
                            return Ok(ExactMinimumCoverSessionAdvance::Pending { visited_nodes });
                        }
                        MinimumCoverSearchAdvance::Cancelled {
                            visited_nodes: advanced,
                            ..
                        } => {
                            visited_nodes = visited_nodes
                                .checked_add(advanced)
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                            drop(search);
                            drop(dense_rows);
                            drop(materialization_words);
                            memory_guard(0)?;
                            return Ok(ExactMinimumCoverSessionAdvance::Cancelled {
                                visited_nodes,
                            });
                        }
                        MinimumCoverSearchAdvance::Finished {
                            visited_nodes: advanced,
                            ..
                        } => {
                            visited_nodes = visited_nodes
                                .checked_add(advanced)
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                            let decision = search.into_solve_decision(
                                &dense_rows,
                                search_base_live,
                                memory_guard,
                            )?;
                            let decision = match decision {
                                MinimumCoverSolveDecision::Found(selected_rows) => {
                                    materialize_internal_result_from_owned_words(
                                        pattern_count,
                                        dense_rows,
                                        materialization_words,
                                        selected_rows,
                                        complete,
                                        memory_guard,
                                    )?
                                }
                                MinimumCoverSolveDecision::ProvedNone => {
                                    drop(dense_rows);
                                    drop(materialization_words);
                                    memory_guard(0)?;
                                    ExactCoverInternalDecision::ProvedNone
                                }
                                MinimumCoverSolveDecision::Cancelled => {
                                    drop(dense_rows);
                                    drop(materialization_words);
                                    memory_guard(0)?;
                                    ExactCoverInternalDecision::Cancelled
                                }
                            };
                            return Ok(map_internal_session_decision(decision, visited_nodes));
                        }
                    }
                }
                ExactCoverSearchSessionState::Finished => {
                    return Ok(ExactMinimumCoverSessionAdvance::Finished);
                }
            }
        }
    }
}

fn map_internal_session_decision(
    decision: ExactCoverInternalDecision,
    visited_nodes: u64,
) -> ExactMinimumCoverSessionAdvance {
    match decision {
        ExactCoverInternalDecision::Found(result) => ExactMinimumCoverSessionAdvance::Found {
            result,
            visited_nodes,
        },
        ExactCoverInternalDecision::ProvedNone => {
            ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes }
        }
        ExactCoverInternalDecision::Cancelled => {
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes }
        }
    }
}

impl ExactMinimumCoverSession {
    pub fn new(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
    ) -> Result<Self, ExactMinimumCoverError> {
        Self::new_with_memory_guard(required, rows, &mut |_| Ok(()))
    }

    pub fn new_with_memory_guard(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        Ok(Self {
            inner: prepare_lazy_exact_cover_search_session(
                required,
                rows,
                ExactCoverSearchGoal::Minimum,
                ExactCoverIncumbentPolicy::Standard,
                None,
                memory_guard,
                &mut || false,
            )?,
        })
    }

    pub fn advance(
        &mut self,
        max_nodes: u64,
    ) -> Result<ExactMinimumCoverSessionAdvance, ExactMinimumCoverError> {
        self.advance_with_memory_guard_and_control(max_nodes, &mut |_| Ok(()), &mut || false)
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.inner.checked_retained_capacity_bytes()
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_at_most(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        limit: usize,
    ) -> Result<Self, ExactMinimumCoverError> {
        Ok(Self {
            inner: ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
                required,
                rows,
                limit,
                None,
                &mut |_| Ok(()),
                &mut || false,
            )?,
        })
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_at_most_with_witness(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        limit: usize,
        witness_hint: &[usize],
    ) -> Result<Self, ExactMinimumCoverError> {
        Ok(Self {
            inner: ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
                required,
                rows,
                limit,
                Some(witness_hint),
                &mut |_| Ok(()),
                &mut || false,
            )?,
        })
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_execution_state(&self) -> ExactMinimumCoverSessionDiagnostics {
        let mut diagnostics = ExactMinimumCoverSessionDiagnostics {
            phase: "unknown",
            witness_phase: None,
            supporter_position: None,
            randomized_trial: None,
            randomized_trial_end: None,
            breakout_attempted_swaps: None,
            search_nodes: 0,
        };
        match &self.inner.state {
            ExactCoverSearchSessionState::Unprepared { .. } => diagnostics.phase = "unprepared",
            ExactCoverSearchSessionState::WitnessShortcut(shortcut) => {
                diagnostics.phase = "witness-shortcut";
                match &shortcut.phase {
                    WitnessShortcutPhase::ReplayHint => {
                        diagnostics.witness_phase = Some("replay-hint")
                    }
                    WitnessShortcutPhase::MaterializeDense => {
                        diagnostics.witness_phase = Some("materialize-dense")
                    }
                    WitnessShortcutPhase::PrepareTarget => {
                        diagnostics.witness_phase = Some("prepare-target")
                    }
                    WitnessShortcutPhase::Greedy => diagnostics.witness_phase = Some("greedy"),
                    WitnessShortcutPhase::HintBreakout { search } => {
                        diagnostics.witness_phase = Some("hint-breakout");
                        diagnostics.breakout_attempted_swaps = Some(search.attempted_swaps);
                    }
                    WitnessShortcutPhase::PrepareSupporters => {
                        diagnostics.witness_phase = Some("prepare-supporters")
                    }
                    WitnessShortcutPhase::WarmSeed { supporter_position } => {
                        diagnostics.witness_phase = Some("warm-seed");
                        diagnostics.supporter_position = Some(*supporter_position);
                    }
                    WitnessShortcutPhase::Breakout {
                        supporter_position,
                        search,
                    } => {
                        diagnostics.witness_phase = Some("breakout");
                        diagnostics.supporter_position = Some(*supporter_position);
                        diagnostics.breakout_attempted_swaps = Some(search.attempted_swaps);
                    }
                    WitnessShortcutPhase::PrepareForcedSupporters => {
                        diagnostics.witness_phase = Some("prepare-forced-supporters")
                    }
                    WitnessShortcutPhase::ForcedGreedy { supporter_position } => {
                        diagnostics.witness_phase = Some("forced-greedy");
                        diagnostics.supporter_position = Some(*supporter_position);
                    }
                    WitnessShortcutPhase::Randomized {
                        supporter_position,
                        search,
                    } => {
                        diagnostics.witness_phase = Some("randomized");
                        diagnostics.supporter_position = Some(*supporter_position);
                        diagnostics.randomized_trial = Some(search.next_trial);
                        diagnostics.randomized_trial_end = Some(search.trial_end);
                    }
                }
            }
            ExactCoverSearchSessionState::InitialQuotient { .. } => {
                diagnostics.phase = "initial-quotient"
            }
            ExactCoverSearchSessionState::FixedPointDominance { .. } => {
                diagnostics.phase = "fixed-point-dominance"
            }
            ExactCoverSearchSessionState::FixedPointQuotient { .. } => {
                diagnostics.phase = "fixed-point-quotient"
            }
            ExactCoverSearchSessionState::BuildSearch { .. } => diagnostics.phase = "build-search",
            ExactCoverSearchSessionState::Ready(_) => diagnostics.phase = "ready",
            ExactCoverSearchSessionState::PreparingRootDual { search, .. } => {
                diagnostics.phase = "preparing-root-dual";
                diagnostics.search_nodes = search.diagnostic_search_nodes;
            }
            ExactCoverSearchSessionState::ImprovingBreakout { search, .. } => {
                diagnostics.phase = "improving-breakout";
                diagnostics.search_nodes = search.diagnostic_search_nodes;
            }
            ExactCoverSearchSessionState::ImprovingIncumbent { search, .. } => {
                diagnostics.phase = "improving-incumbent";
                diagnostics.search_nodes = search.diagnostic_search_nodes;
            }
            ExactCoverSearchSessionState::Searching { search, .. } => {
                diagnostics.phase = "searching";
                diagnostics.search_nodes = search.diagnostic_search_nodes;
            }
            ExactCoverSearchSessionState::Finished => diagnostics.phase = "finished",
        }
        diagnostics
    }

    /// Feature-gated observability for ignored native performance probes. It
    /// is absent from the production dependency graph and conveys no proof
    /// authority.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_incumbent_progress(&self) -> Option<(usize, usize)> {
        match &self.inner.state {
            ExactCoverSearchSessionState::ImprovingIncumbent {
                search,
                incumbent_search,
                ..
            } => Some((incumbent_search.next_trial, search.best.len())),
            ExactCoverSearchSessionState::Searching { .. } => None,
            _ => None,
        }
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_root_dual_lower_bound(&self) -> Option<usize> {
        let search = match &self.inner.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return None,
        };
        search.root_dual.as_ref().and_then(|certificate| {
            certificate.certified_lower_bound_for_uncovered(&search.target_words, &search.covered)
        })
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_residual_progress(&self) -> Option<ExactMinimumCoverResidualDiagnostics> {
        self.inner.diagnostic_residual_progress()
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "diagnostic-probes"))]
    pub fn diagnostic_conditional_rows(
        &self,
    ) -> Option<ExactMinimumCoverConditionalRowDiagnostics> {
        self.inner.diagnostic_conditional_rows()
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "diagnostic-probes"))]
    pub fn diagnostic_residual_warm_seed(&self) -> Option<ExactMinimumCoverWarmSeedDiagnostics> {
        self.inner.diagnostic_residual_warm_seed()
    }

    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_hot_cost(&self) -> Option<ExactMinimumCoverHotCostDiagnostics> {
        self.inner.diagnostic_hot_cost()
    }

    /// Clears preparation/incumbent timing so a probe can attribute only the
    /// subsequent exact proof. Production callers cannot enable this seam.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_reset_hot_cost(&mut self) -> bool {
        let search = match &mut self.inner.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return false,
        };
        search.reset_diagnostic_hot_cost();
        true
    }

    /// Restricts optional residual-dual attempts for native diagnostic A/B
    /// probes. Production builds do not contain this seam, and all exact
    /// certificate checks remain unchanged for admitted attempts.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_set_residual_admission(
        &mut self,
        policy: ExactMinimumCoverResidualAdmissionPolicy,
    ) -> bool {
        if policy.minimum_dual_gap > policy.maximum_dual_gap
            || policy.minimum_search_depth > policy.maximum_search_depth
            || policy.maximum_iterations_per_attempt == 0
        {
            return false;
        }
        let search = match &mut self.inner.state {
            ExactCoverSearchSessionState::PreparingRootDual { search, .. }
            | ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return false,
        };
        search.diagnostic_residual_admission = policy;
        if let Some(workspace) = search.dual_workspace.as_mut() {
            workspace.set_diagnostic_sparse_proposal_softmax(policy.use_sparse_proposal_softmax);
        }
        true
    }

    /// Drives a clone of a pristine `Searching` snapshot with a recursive
    /// reference implementation. The production session is not mutated, so a
    /// probe can immediately drive the explicit state machine from the exact
    /// same incumbent, memo, exclusion, root certificate, and residual budget.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_recursive_reference(
        &self,
    ) -> Result<Option<ExactMinimumCoverRecursiveReference>, ExactMinimumCoverError> {
        let ExactCoverSearchSessionState::Searching {
            pattern_count,
            complete,
            dense_rows,
            materialization_words,
            search,
        } = &self.inner.state
        else {
            return Ok(None);
        };
        if search.finished
            || search.cancelled
            || !search.enter_child
            || !search.frames.is_empty()
            || !search.current.is_empty()
            || search.excluded_rows.iter().any(|word| *word != 0)
        {
            return Ok(None);
        }

        let dense_rows = dense_rows.clone();
        let materialization_words = materialization_words.clone();
        let mut search = search.clone();
        let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
        let materialization_live = checked_nested_words_retained_bytes(&materialization_words)?;
        let search_base_live = dense_live
            .checked_add(materialization_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let visited_nodes = search.diagnostic_solve_recursive_reference(
            &dense_rows,
            search_base_live,
            &mut |_| Ok(()),
        )?;
        let residual = search.diagnostic_residual_progress();
        let decision =
            search.into_solve_decision(&dense_rows, search_base_live, &mut |_| Ok(()))?;
        let MinimumCoverSolveDecision::Found(selected_rows) = decision else {
            return Ok(None);
        };
        let decision = materialize_internal_result_from_owned_words(
            *pattern_count,
            dense_rows,
            materialization_words,
            selected_rows,
            *complete,
            &mut |_| Ok(()),
        )?;
        let ExactCoverInternalDecision::Found(result) = decision else {
            return Ok(None);
        };
        Ok(Some(ExactMinimumCoverRecursiveReference {
            result,
            visited_nodes,
            residual,
        }))
    }

    /// Probe-only A/B control. The requested value can only reduce the
    /// remaining optional residual proposal budget and cannot weaken the
    /// already-materialized root certificate.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_limit_residual_iterations(&mut self, remaining: usize) -> bool {
        let search = match &mut self.inner.state {
            ExactCoverSearchSessionState::ImprovingIncumbent { search, .. }
            | ExactCoverSearchSessionState::Searching { search, .. } => search,
            _ => return false,
        };
        let Some(workspace) = search.dual_workspace.as_mut() else {
            return false;
        };
        let current = workspace.remaining_proposal_iterations();
        workspace.set_remaining_iterations_for_test(remaining.min(current));
        true
    }

    /// Probe-only control used to compare exact proof cost with and without
    /// the optional randomized incumbent phase.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_skip_incumbent_trials(&mut self) -> bool {
        let state = core::mem::replace(
            &mut self.inner.state,
            ExactCoverSearchSessionState::Finished,
        );
        match state {
            ExactCoverSearchSessionState::ImprovingIncumbent {
                pattern_count,
                complete,
                dense_rows,
                materialization_words,
                search,
                ..
            } => {
                self.inner.state = ExactCoverSearchSessionState::Searching {
                    pattern_count,
                    complete,
                    dense_rows,
                    materialization_words,
                    search,
                };
                true
            }
            state => {
                self.inner.state = state;
                false
            }
        }
    }

    pub fn advance_with_memory_guard_and_control(
        &mut self,
        max_nodes: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverSessionAdvance, ExactMinimumCoverError> {
        self.inner.advance(max_nodes, memory_guard, cancelled)
    }
}

fn materialize_internal_result(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    dense_rows: Vec<DenseRow>,
    selected_rows: Vec<usize>,
    complete: bool,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactCoverInternalDecision, ExactMinimumCoverError> {
    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let materialization_words = build_materialization_words_with_memory_guard(
        required,
        rows,
        &dense_rows,
        dense_live,
        memory_guard,
    )?;
    materialize_internal_result_from_owned_words(
        required.pattern_count(),
        dense_rows,
        materialization_words,
        selected_rows,
        complete,
        memory_guard,
    )
}

fn materialize_internal_result_from_owned_words(
    pattern_count: usize,
    dense_rows: Vec<DenseRow>,
    materialization_words: Vec<Vec<u64>>,
    selected_rows: Vec<usize>,
    complete: bool,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactCoverInternalDecision, ExactMinimumCoverError> {
    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    if materialization_words.len() != dense_rows.len() {
        return Err(ExactMinimumCoverError::ProjectionOverflow);
    }
    let materialization_live = checked_nested_words_retained_bytes(&materialization_words)?;
    let selected_live = checked_vec_retained_bytes(&selected_rows)?;
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let mut covered_words = try_vec_with_capacity(
        word_count,
        dense_live
            .checked_add(materialization_live)
            .and_then(|bytes| bytes.checked_add(selected_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_result_words",
    )?;
    covered_words.resize(word_count, 0);
    let mut row_indices = try_vec_with_capacity(
        selected_rows.len(),
        dense_live
            .checked_add(materialization_live)
            .and_then(|bytes| bytes.checked_add(selected_live))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&covered_words).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_result_indices",
    )?;
    for row_index in selected_rows {
        let source_index = dense_rows[row_index].source_index;
        for (word_index, covered_word) in covered_words.iter_mut().enumerate() {
            *covered_word |= materialization_words[row_index][word_index];
        }
        row_indices.push(source_index);
    }
    row_indices.sort_unstable();

    drop(dense_rows);
    drop(materialization_words);
    let result_live = checked_vec_retained_bytes(&row_indices)?
        .checked_add(checked_vec_retained_bytes(&covered_words)?)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let construction_future =
        PatternBitSet::checked_external_words_materialize_union_future_bytes(pattern_count)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        result_live
            .checked_add(construction_future)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let covered_patterns = PatternBitSet::from_words(pattern_count, covered_words)
        .expect("minimum-cover words preserve the required pattern universe");
    memory_guard(
        checked_vec_retained_bytes(&row_indices)?
            .checked_add(
                covered_patterns
                    .checked_storage_retained_bytes()
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;

    Ok(ExactCoverInternalDecision::Found(ExactMinimumCoverResult {
        row_indices,
        covered_patterns,
        complete,
    }))
}

enum WitnessAssistedCoverDecision {
    Found(Vec<usize>),
    Miss,
    Cancelled,
}

/// Runs the bounded positive-only shortcut on the caller's original dense row
/// order. In particular, no dominance reduction can replace a smaller
/// original identity before the portfolio layer receives its acceleration
/// witness. A miss is deliberately non-authoritative: every allocation is
/// dropped and the caller continues through the ordinary exact proof.
fn witness_assisted_cover_before_dominance(
    required: &PatternBitSet,
    rows: &[DenseRow],
    limit: usize,
    witness_hint: &[usize],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<WitnessAssistedCoverDecision, ExactMinimumCoverError> {
    if cancelled() {
        return Ok(WitnessAssistedCoverDecision::Cancelled);
    }
    let mut target_words = try_vec_with_capacity(
        required.word_count(),
        base_live_bytes,
        memory_guard,
        "exact_cover_at_most_original_target",
    )?;
    target_words.extend((0..required.word_count()).map(|word| required.word_at(word)));
    let target_live = checked_vec_retained_bytes(&target_words)?;
    let pattern_slots = target_words
        .len()
        .checked_mul(u64::BITS as usize)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut constraint_weights = try_vec_with_capacity(
        pattern_slots,
        base_live_bytes
            .checked_add(target_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_cover_at_most_original_weights",
    )?;
    constraint_weights.resize(pattern_slots, 1);
    let weights_live = checked_vec_retained_bytes(&constraint_weights)?;
    let heuristic_base = base_live_bytes
        .checked_add(target_live)
        .and_then(|bytes| bytes.checked_add(weights_live))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let Some(mut best) = greedy_cover_with_memory_guard(
        rows,
        &target_words,
        &constraint_weights,
        heuristic_base,
        memory_guard,
    )?
    else {
        drop(constraint_weights);
        drop(target_words);
        memory_guard(base_live_bytes)?;
        return Ok(WitnessAssistedCoverDecision::Miss);
    };
    if best.len() <= limit {
        drop(constraint_weights);
        drop(target_words);
        memory_guard(
            base_live_bytes
                .checked_add(checked_vec_retained_bytes(&best)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        return Ok(WitnessAssistedCoverDecision::Found(best));
    }

    let target_constraint_count = target_words
        .iter()
        .map(|word| word.count_ones() as usize)
        .sum::<usize>();
    let outcome = if rows.len() >= 64 && target_constraint_count >= 128 {
        let best_live = checked_vec_retained_bytes(&best)?;
        let supporter_base = heuristic_base
            .checked_add(best_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let preferred_constraint = witness_unique_missing_constraint_rows_with_memory_guard(
            rows,
            &target_words,
            limit,
            witness_hint,
            supporter_base,
            memory_guard,
        )?;
        let uses_preferred_missing_constraint = preferred_constraint.is_some();
        let rarest_constraint = match preferred_constraint {
            Some(preferred) => Some(preferred),
            None => minimum_support_constraint_rows_with_memory_guard(
                rows,
                &target_words,
                supporter_base,
                memory_guard,
            )?,
        };
        let Some(rarest_constraint) = rarest_constraint else {
            return Ok(WitnessAssistedCoverDecision::Miss);
        };
        let warm_supporters_live = checked_vec_retained_bytes(&rarest_constraint.row_indices)?;
        let warm_randomized_base = heuristic_base
            .checked_add(warm_supporters_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut outcome = IncumbentSearchOutcome::Completed;
        let mut support_by_pattern = None;
        for (supporter_position, forced_row) in
            rarest_constraint.row_indices.iter().copied().enumerate()
        {
            if cancelled() {
                outcome = IncumbentSearchOutcome::Cancelled;
                break;
            }
            let support_live = support_by_pattern
                .as_ref()
                .map_or(Ok(0), checked_support_retained_bytes)?;
            let warm_base = warm_randomized_base
                .checked_add(checked_vec_retained_bytes(&best)?)
                .and_then(|bytes| bytes.checked_add(support_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            let Some(mut warm_seed) = warm_seed_with_forced_supporter_memory_guard(
                rows,
                &target_words,
                limit,
                witness_hint,
                rarest_constraint.word_index,
                rarest_constraint.bit,
                forced_row,
                warm_base,
                memory_guard,
            )?
            else {
                continue;
            };
            if warm_seed.len() <= limit {
                best = warm_seed;
                outcome = IncumbentSearchOutcome::FoundAtMost;
                break;
            }
            if support_by_pattern.is_none() {
                let support_base = warm_randomized_base
                    .checked_add(checked_vec_retained_bytes(&best)?)
                    .and_then(|bytes| {
                        bytes.checked_add(checked_vec_retained_bytes(&warm_seed).ok()?)
                    })
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                support_by_pattern = Some(build_support_by_pattern_with_memory_guard(
                    rows,
                    &target_words,
                    support_base,
                    memory_guard,
                )?);
            }
            let support = support_by_pattern
                .as_ref()
                .expect("warm breakout support is initialized above");
            let breakout_base = warm_randomized_base
                .checked_add(checked_vec_retained_bytes(&best)?)
                .and_then(|bytes| bytes.checked_add(checked_support_retained_bytes(support).ok()?))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            let breakout_swap_budget = if uses_preferred_missing_constraint {
                preferred_witness_breakout_budget(
                    rarest_constraint.row_indices.len(),
                    supporter_position,
                )
            } else {
                WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET
            };
            if breakout_swap_budget == 0 {
                continue;
            }
            if improve_fixed_cardinality_cover_with_memory_guard(
                rows,
                &target_words,
                support,
                &mut warm_seed,
                breakout_swap_budget,
                Some(forced_row),
                breakout_base,
                memory_guard,
                cancelled,
            )? {
                outcome = IncumbentSearchOutcome::Cancelled;
                break;
            }
            if warm_seed.len() <= limit {
                best = warm_seed;
                outcome = IncumbentSearchOutcome::FoundAtMost;
                break;
            }
        }
        drop(support_by_pattern);

        let rarest_constraint =
            if uses_preferred_missing_constraint && outcome == IncumbentSearchOutcome::Completed {
                drop(rarest_constraint);
                let forced_supporter_base = heuristic_base
                    .checked_add(checked_vec_retained_bytes(&best)?)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                let Some(global_rarest) = minimum_support_constraint_rows_with_memory_guard(
                    rows,
                    &target_words,
                    forced_supporter_base,
                    memory_guard,
                )?
                else {
                    return Ok(WitnessAssistedCoverDecision::Miss);
                };
                global_rarest
            } else {
                rarest_constraint
            };
        let supporter_count = rarest_constraint.row_indices.len();
        let forced_supporters_live = checked_vec_retained_bytes(&rarest_constraint.row_indices)?;
        let randomized_base = heuristic_base
            .checked_add(forced_supporters_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;

        for (supporter_position, forced_row) in
            rarest_constraint.row_indices.iter().copied().enumerate()
        {
            if outcome != IncumbentSearchOutcome::Completed {
                break;
            }
            if cancelled() {
                outcome = IncumbentSearchOutcome::Cancelled;
                break;
            }

            let greedy_base = randomized_base
                .checked_add(checked_vec_retained_bytes(&best)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            if let Some(forced_best) = greedy_cover_with_forced_row_memory_guard(
                rows,
                &target_words,
                &constraint_weights,
                Some(forced_row),
                greedy_base,
                memory_guard,
            )? {
                if forced_best.len() <= limit {
                    best = forced_best;
                    outcome = IncumbentSearchOutcome::FoundAtMost;
                    break;
                }
                if forced_best.len() < best.len() {
                    best = forced_best;
                }
            }

            let trial_budget = WITNESS_ASSISTED_COMPACT_COVER_TRIALS / supporter_count
                + usize::from(
                    supporter_position < WITNESS_ASSISTED_COMPACT_COVER_TRIALS % supporter_count,
                );
            if trial_budget == 0 {
                continue;
            }
            let random_seed = WITNESS_ASSISTED_RANDOM_SEED
                ^ (rows[forced_row].source_index as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
            outcome = improve_randomized_compact_cover_with_memory_guard(
                rows,
                &target_words,
                &mut best,
                trial_budget,
                random_seed,
                Some(limit),
                Some(forced_row),
                randomized_base,
                memory_guard,
                cancelled,
            )?;
            if outcome != IncumbentSearchOutcome::Completed {
                break;
            }
        }
        drop(rarest_constraint);
        outcome
    } else {
        IncumbentSearchOutcome::Completed
    };
    drop(constraint_weights);
    drop(target_words);
    match outcome {
        IncumbentSearchOutcome::Cancelled => {
            drop(best);
            memory_guard(base_live_bytes)?;
            Ok(WitnessAssistedCoverDecision::Cancelled)
        }
        IncumbentSearchOutcome::FoundAtMost => {
            if best.len() > limit {
                return Err(ExactMinimumCoverError::ProjectionOverflow);
            }
            memory_guard(
                base_live_bytes
                    .checked_add(checked_vec_retained_bytes(&best)?)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )?;
            Ok(WitnessAssistedCoverDecision::Found(best))
        }
        IncumbentSearchOutcome::Completed => {
            drop(best);
            memory_guard(base_live_bytes)?;
            Ok(WitnessAssistedCoverDecision::Miss)
        }
    }
}

/// Runs only the former blocking positive shortcut for a native diagnostic
/// comparison. It deliberately does not fall through to exact search after a
/// miss, so a probe can distinguish heuristic divergence from DFS cost.
#[doc(hidden)]
#[cfg(feature = "diagnostic-probes")]
pub fn diagnostic_blocking_witness_shortcut(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    limit: usize,
    witness_hint: &[usize],
) -> Result<ExactMinimumCoverWitnessShortcutDiagnostics, ExactMinimumCoverError> {
    for (row_index, row) in rows.iter().enumerate() {
        if row.pattern_count() != required.pattern_count() {
            return Err(ExactMinimumCoverError::RowPatternCountMismatch {
                row_index,
                expected: required.pattern_count(),
                actual: row.pattern_count(),
            });
        }
    }
    let mut memory_guard = |_| Ok(());
    let mut dense_rows = try_vec_with_capacity(
        rows.len(),
        0,
        &mut memory_guard,
        "diagnostic_witness_shortcut_dense_rows",
    )?;
    for (source_index, row) in rows.iter().enumerate() {
        let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
        let mut words = try_vec_with_capacity(
            required.word_count(),
            dense_live,
            &mut memory_guard,
            "diagnostic_witness_shortcut_dense_words",
        )?;
        let mut nonempty = false;
        for word_index in 0..required.word_count() {
            let word = row.word_at(word_index) & required.word_at(word_index);
            nonempty |= word != 0;
            words.push(word);
        }
        if nonempty {
            dense_rows.push(DenseRow {
                source_index,
                words,
            });
        }
    }
    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    match witness_assisted_cover_before_dominance(
        required,
        &dense_rows,
        limit,
        witness_hint,
        dense_live,
        &mut memory_guard,
        &mut || false,
    )? {
        WitnessAssistedCoverDecision::Found(selected) => {
            let mut row_indices = selected
                .iter()
                .map(|row| dense_rows[*row].source_index)
                .collect::<Vec<_>>();
            row_indices.sort_unstable();
            row_indices.dedup();
            if row_indices.len() > limit {
                return Err(ExactMinimumCoverError::ProjectionOverflow);
            }
            let mut replay = vec![0_u64; required.word_count()];
            for row in row_indices.iter().copied() {
                for (word_index, replay_word) in replay.iter_mut().enumerate() {
                    *replay_word |= rows[row].word_at(word_index) & required.word_at(word_index);
                }
            }
            let target = (0..required.word_count())
                .map(|word| required.word_at(word))
                .collect::<Vec<_>>();
            if !is_superset(&replay, &target) {
                return Err(ExactMinimumCoverError::ProjectionOverflow);
            }
            Ok(ExactMinimumCoverWitnessShortcutDiagnostics::Found(
                row_indices,
            ))
        }
        WitnessAssistedCoverDecision::Miss => Ok(ExactMinimumCoverWitnessShortcutDiagnostics::Miss),
        WitnessAssistedCoverDecision::Cancelled => {
            Ok(ExactMinimumCoverWitnessShortcutDiagnostics::Cancelled)
        }
    }
}

/// Returns every original row supporting the deterministic rarest required
/// constraint. The list follows original source identity order; it is an
/// acceleration partition only and contributes no infeasibility authority.
#[derive(Clone, Debug)]
struct MinimumSupportConstraintRows {
    word_index: usize,
    bit: u64,
    row_indices: Vec<usize>,
}

/// Prefer the one constraint that a validated-size warm witness does not
/// cover. Canonical self-reduction adds exactly such a selector constraint:
/// choosing an unrelated globally rare constraint would make the warm seed
/// fail its non-pivot replay check before the required selector supporter can
/// be tried. This remains a positive-only heuristic; malformed hints or any
/// other uncovered shape fall back to the ordinary rarest-constraint choice.
fn witness_unique_missing_constraint_rows_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    limit: usize,
    witness_hint: &[usize],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<MinimumSupportConstraintRows>, ExactMinimumCoverError> {
    if witness_hint.len() != limit || !witness_hint.windows(2).all(|pair| pair[0] < pair[1]) {
        return Ok(None);
    }

    let mut covered = try_vec_with_capacity(
        target.len(),
        base_live_bytes,
        memory_guard,
        "exact_cover_at_most_witness_missing_replay",
    )?;
    covered.resize(target.len(), 0);
    for source_index in witness_hint {
        let mut matches = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.source_index == *source_index);
        let Some((row_index, _)) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Ok(None);
        }
        union_words(&mut covered, &rows[row_index].words);
    }

    let mut missing_constraint = None;
    for (word_index, target_word) in target.iter().copied().enumerate() {
        let mut missing = target_word & !covered[word_index];
        while missing != 0 {
            if missing_constraint.is_some() {
                return Ok(None);
            }
            let bit = 1_u64 << missing.trailing_zeros();
            missing_constraint = Some((word_index, bit));
            missing &= missing - 1;
        }
    }
    let Some((word_index, bit)) = missing_constraint else {
        return Ok(None);
    };
    let support_count = rows
        .iter()
        .filter(|row| row.words[word_index] & bit != 0)
        .count();
    if support_count == 0 {
        return Ok(None);
    }

    drop(covered);
    memory_guard(base_live_bytes)?;
    let mut supporters = try_vec_with_capacity(
        support_count,
        base_live_bytes,
        memory_guard,
        "exact_cover_at_most_witness_missing_supporters",
    )?;
    supporters.extend(
        rows.iter()
            .enumerate()
            .filter(|(_, row)| row.words[word_index] & bit != 0)
            .map(|(row_index, _)| row_index),
    );
    supporters.sort_unstable_by_key(|row_index| rows[*row_index].source_index);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&supporters)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(Some(MinimumSupportConstraintRows {
        word_index,
        bit,
        row_indices: supporters,
    }))
}

fn minimum_support_constraint_rows_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<MinimumSupportConstraintRows>, ExactMinimumCoverError> {
    let mut minimum_support = usize::MAX;
    let mut pivot = None;
    for (word_index, target_word) in target.iter().copied().enumerate() {
        let mut remaining = target_word;
        while remaining != 0 {
            let bit_index = remaining.trailing_zeros() as usize;
            let bit = 1_u64 << bit_index;
            let support = rows
                .iter()
                .filter(|row| row.words[word_index] & bit != 0)
                .count();
            if support < minimum_support {
                minimum_support = support;
                pivot = Some((word_index, bit));
                if support == 0 {
                    break;
                }
            }
            remaining &= remaining - 1;
        }
        if minimum_support == 0 {
            break;
        }
    }

    let Some((word_index, bit)) = pivot else {
        return Ok(None);
    };
    let capacity = minimum_support;
    let mut supporters = try_vec_with_capacity(
        capacity,
        base_live_bytes,
        memory_guard,
        "exact_cover_at_most_rarest_constraint_supporters",
    )?;
    supporters.extend(
        rows.iter()
            .enumerate()
            .filter(|(_, row)| row.words[word_index] & bit != 0)
            .map(|(row_index, _)| row_index),
    );
    supporters.sort_unstable_by_key(|row_index| rows[*row_index].source_index);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&supporters)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(Some(MinimumSupportConstraintRows {
        word_index,
        bit,
        row_indices: supporters,
    }))
}

#[allow(clippy::too_many_arguments)]
fn warm_seed_with_forced_supporter_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    limit: usize,
    witness_hint: &[usize],
    pivot_word_index: usize,
    pivot_bit: u64,
    forced_row: usize,
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<Vec<usize>>, ExactMinimumCoverError> {
    if witness_hint.len() != limit
        || !witness_hint.windows(2).all(|pair| pair[0] < pair[1])
        || forced_row >= rows.len()
    {
        return Ok(None);
    }
    let capacity = witness_hint
        .len()
        .checked_add(1)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut seed = try_vec_with_capacity(
        capacity,
        base_live_bytes,
        memory_guard,
        "exact_cover_at_most_warm_seed",
    )?;
    for source_index in witness_hint {
        let Ok(row_index) = rows.binary_search_by_key(source_index, |row| row.source_index) else {
            return Ok(None);
        };
        seed.push(row_index);
    }
    let seed_live = checked_vec_retained_bytes(&seed)?;
    let mut covered = try_vec_with_capacity(
        target.len(),
        base_live_bytes
            .checked_add(seed_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_cover_at_most_warm_seed_replay",
    )?;
    covered.resize(target.len(), 0);
    for row_index in seed.iter().copied() {
        union_words(&mut covered, &rows[row_index].words);
    }
    let covers_nonpivot_target = target.iter().copied().enumerate().all(|(word, target)| {
        let required = if word == pivot_word_index {
            target & !pivot_bit
        } else {
            target
        };
        covered[word] & required == required
    });
    if !covers_nonpivot_target {
        return Ok(None);
    }
    if !seed.contains(&forced_row) {
        seed.push(forced_row);
        union_words(&mut covered, &rows[forced_row].words);
    }
    seed.sort_unstable_by_key(|row_index| rows[*row_index].source_index);
    seed.dedup();
    if !is_superset(&covered, target) {
        return Ok(None);
    }
    drop(covered);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&seed)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(Some(seed))
}

fn build_support_by_pattern_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Vec<Vec<usize>>, ExactMinimumCoverError> {
    let pattern_count = target
        .len()
        .checked_mul(u64::BITS as usize)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut support_by_pattern = try_vec_with_capacity(
        pattern_count,
        base_live_bytes,
        memory_guard,
        "exact_minimum_cover_support_slots",
    )?;
    support_by_pattern.resize_with(pattern_count, Vec::new);
    // Keep exact retained accounting incrementally. Rewalking every preceding
    // nested Vec for every pattern made construction quadratic in the number
    // of target slots even though the support matrix itself is linear in its
    // incidences.
    let mut support_live = checked_vec_retained_bytes(&support_by_pattern)?;
    for pattern in 0..pattern_count {
        let word_index = pattern / u64::BITS as usize;
        let bit = pattern % u64::BITS as usize;
        let support_count = rows
            .iter()
            .filter(|row| row.words[word_index] & (1_u64 << bit) != 0)
            .count();
        if support_count == 0 {
            continue;
        }
        let mut support = try_vec_with_capacity(
            support_count,
            base_live_bytes
                .checked_add(support_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_support_rows",
        )?;
        for (row_index, row) in rows.iter().enumerate() {
            if row.words[word_index] & (1_u64 << bit) != 0 {
                support.push(row_index);
            }
        }
        support_live = support_live
            .checked_add(checked_vec_retained_bytes(&support)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        support_by_pattern[pattern] = support;
        memory_guard(
            base_live_bytes
                .checked_add(support_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
    }
    Ok(support_by_pattern)
}

pub fn exact_minimum_cover_with_memory_limit(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    exact_minimum_cover_with_memory_guard(required, rows, &mut |solver_owned_bytes| {
        let required_memory_bytes = already_retained_bytes
            .checked_add(solver_owned_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        Ok(())
    })
}

fn remove_dominated_rows_with_memory_guard(
    rows: &mut Vec<DenseRow>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<(), ExactMinimumCoverError> {
    rows.retain(|row| row.words.iter().any(|word| *word != 0));
    if rows.is_empty() {
        memory_guard(checked_dense_rows_retained_bytes(rows)?)?;
        return Ok(());
    }
    let rows_live = checked_dense_rows_retained_bytes(rows)?;
    let dominance_index = ExactDominanceIndex::try_new(rows, rows_live, memory_guard)?;
    let index_live = dominance_index.checked_heap_retained_bytes()?;
    let mut dominated = try_vec_with_capacity(
        rows.len(),
        rows_live
            .checked_add(index_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_dominated_rows",
    )?;
    dominated.resize(rows.len(), false);
    memory_guard(
        rows_live
            .checked_add(index_live)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&dominated).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    for left in 0..rows.len() {
        if dominated[left] {
            continue;
        }
        for right in dominance_index.rarest_bit_support(&rows[left]) {
            let right = *right;
            if dominance_index.row_pattern_counts[right] < dominance_index.row_pattern_counts[left]
            {
                break;
            }
            if left == right {
                continue;
            }
            if is_superset(&rows[right].words, &rows[left].words) {
                let equal = rows[right].words == rows[left].words;
                if !equal || rows[right].source_index < rows[left].source_index {
                    dominated[left] = true;
                    break;
                }
            }
        }
    }
    let mut index = 0;
    rows.retain(|_| {
        let keep = !dominated[index];
        index += 1;
        keep
    });
    drop(dominated);
    drop(dominance_index);
    memory_guard(checked_dense_rows_retained_bytes(rows)?)?;
    Ok(())
}

/// Exact inverted index for dominance reduction. A superset of a non-empty
/// row must occur in the support list of every bit in that row, so probing the
/// rarest such list removes the quadratic all-row candidate scan without
/// making dominance probabilistic.
struct ExactDominanceIndex {
    row_pattern_counts: Vec<usize>,
    support_offsets: Vec<usize>,
    support_rows: Vec<usize>,
}

impl ExactDominanceIndex {
    fn try_new(
        rows: &[DenseRow],
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        let word_count = rows.first().map_or(0, |row| row.words.len());
        let pattern_count = word_count
            .checked_mul(u64::BITS as usize)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut row_pattern_counts = try_vec_with_capacity(
            rows.len(),
            base_live_bytes,
            memory_guard,
            "exact_minimum_cover_dominance_row_pattern_counts",
        )?;
        let row_counts_live = checked_vec_retained_bytes(&row_pattern_counts)?;
        let mut support_counts = try_vec_with_capacity(
            pattern_count,
            base_live_bytes
                .checked_add(row_counts_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_dominance_support_counts",
        )?;
        support_counts.resize(pattern_count, 0_usize);
        for row in rows {
            let mut row_pattern_count = 0_usize;
            for (word_index, word) in row.words.iter().copied().enumerate() {
                row_pattern_count = row_pattern_count
                    .checked_add(word.count_ones() as usize)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                let mut remaining = word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    support_counts[pattern] = support_counts[pattern]
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    remaining &= remaining - 1;
                }
            }
            row_pattern_counts.push(row_pattern_count);
        }

        let row_counts_live = checked_vec_retained_bytes(&row_pattern_counts)?;
        let support_counts_live = checked_vec_retained_bytes(&support_counts)?;
        let mut support_offsets = try_vec_with_capacity(
            pattern_count
                .checked_add(1)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            base_live_bytes
                .checked_add(row_counts_live)
                .and_then(|bytes| bytes.checked_add(support_counts_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_dominance_support_offsets",
        )?;
        support_offsets.push(0_usize);
        for count in support_counts.iter().copied() {
            support_offsets.push(
                support_offsets
                    .last()
                    .copied()
                    .expect("dominance support offsets contain the origin")
                    .checked_add(count)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            );
        }

        let offsets_live = checked_vec_retained_bytes(&support_offsets)?;
        let support_row_count = support_offsets.last().copied().unwrap_or(0);
        let mut support_rows = try_vec_with_capacity(
            support_row_count,
            base_live_bytes
                .checked_add(row_counts_live)
                .and_then(|bytes| bytes.checked_add(support_counts_live))
                .and_then(|bytes| bytes.checked_add(offsets_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_dominance_support_rows",
        )?;
        support_rows.resize(support_row_count, 0_usize);
        let support_rows_live = checked_vec_retained_bytes(&support_rows)?;
        let mut support_cursors = try_vec_with_capacity(
            pattern_count,
            base_live_bytes
                .checked_add(row_counts_live)
                .and_then(|bytes| bytes.checked_add(support_counts_live))
                .and_then(|bytes| bytes.checked_add(offsets_live))
                .and_then(|bytes| bytes.checked_add(support_rows_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_dominance_support_cursors",
        )?;
        support_cursors.extend_from_slice(&support_offsets[..pattern_count]);
        for (row_index, row) in rows.iter().enumerate() {
            for (word_index, word) in row.words.iter().copied().enumerate() {
                let mut remaining = word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    let cursor = &mut support_cursors[pattern];
                    support_rows[*cursor] = row_index;
                    *cursor += 1;
                    remaining &= remaining - 1;
                }
            }
        }
        drop(support_counts);
        drop(support_cursors);

        for pattern in 0..pattern_count {
            let start = support_offsets[pattern];
            let end = support_offsets[pattern + 1];
            support_rows[start..end].sort_unstable_by(|left, right| {
                row_pattern_counts[*right]
                    .cmp(&row_pattern_counts[*left])
                    .then_with(|| rows[*left].source_index.cmp(&rows[*right].source_index))
            });
        }
        let index = Self {
            row_pattern_counts,
            support_offsets,
            support_rows,
        };
        memory_guard(
            base_live_bytes
                .checked_add(index.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(index)
    }

    fn rarest_bit_support(&self, row: &DenseRow) -> &[usize] {
        let mut rarest_pattern = None;
        let mut rarest_count = usize::MAX;
        for (word_index, word) in row.words.iter().copied().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                let count = self.support_offsets[pattern + 1] - self.support_offsets[pattern];
                if count < rarest_count {
                    rarest_pattern = Some(pattern);
                    rarest_count = count;
                }
                remaining &= remaining - 1;
            }
        }
        let pattern = rarest_pattern.expect("dense dominance rows are non-empty");
        &self.support_rows[self.support_offsets[pattern]..self.support_offsets[pattern + 1]]
    }

    fn checked_heap_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.row_pattern_counts)?
            .checked_add(checked_vec_retained_bytes(&self.support_offsets)?)
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.support_rows).ok()?)
            })
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }
}

/// Transposed exact support matrix for required target constraints. A target
/// pattern is a set-cover constraint whose support is the set of rows that
/// cover it. Equal supports are the same constraint, so they share one compact
/// bit while retaining their multiplicity as a weight. If support A is a
/// subset of support B, satisfying A also satisfies B, so B can then be
/// removed without changing the feasible row-set family or its optimum.
struct ExactTargetConstraintIndex {
    source_patterns: Vec<usize>,
    support_words: Vec<u64>,
    support_word_count: usize,
    order: Vec<usize>,
    redundant: Vec<bool>,
    multiplicities: Vec<usize>,
    anchor_rows: Vec<(usize, usize)>,
}

impl ExactTargetConstraintIndex {
    fn try_new(
        rows: &[DenseRow],
        target_words: &[u64],
        target_weights: &[usize],
        base_live_bytes: u128,
        remove_support_supersets: bool,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        let target_count = target_words
            .iter()
            .map(|word| word.count_ones() as usize)
            .try_fold(0_usize, |sum, count| sum.checked_add(count))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let support_word_count = rows.len().div_ceil(u64::BITS as usize);
        let mut source_patterns = try_vec_with_capacity(
            target_count,
            base_live_bytes,
            memory_guard,
            "exact_minimum_cover_target_constraint_patterns",
        )?;
        for (word_index, word) in target_words.iter().copied().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                source_patterns.push(word_index * u64::BITS as usize + bit);
                remaining &= remaining - 1;
            }
        }
        let patterns_live = checked_vec_retained_bytes(&source_patterns)?;
        let support_slot_count = target_count
            .checked_mul(support_word_count)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut support_words = try_vec_with_capacity(
            support_slot_count,
            base_live_bytes
                .checked_add(patterns_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_support_words",
        )?;
        support_words.resize(support_slot_count, 0);
        for (constraint_index, source_pattern) in source_patterns.iter().copied().enumerate() {
            let source_word = source_pattern / u64::BITS as usize;
            let source_bit = source_pattern % u64::BITS as usize;
            let support_offset = constraint_index * support_word_count;
            for (row_index, row) in rows.iter().enumerate() {
                if row.words[source_word] & (1_u64 << source_bit) == 0 {
                    continue;
                }
                support_words[support_offset + row_index / u64::BITS as usize] |=
                    1_u64 << (row_index % u64::BITS as usize);
            }
        }

        let support_live = checked_vec_retained_bytes(&support_words)?;
        let mut order = try_vec_with_capacity(
            target_count,
            base_live_bytes
                .checked_add(patterns_live)
                .and_then(|bytes| bytes.checked_add(support_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_order",
        )?;
        order.extend(0..target_count);
        let order_live = checked_vec_retained_bytes(&order)?;
        let mut redundant = try_vec_with_capacity(
            target_count,
            base_live_bytes
                .checked_add(patterns_live)
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_redundant",
        )?;
        redundant.resize(target_count, false);
        let redundant_live = checked_vec_retained_bytes(&redundant)?;
        debug_assert_eq!(target_weights.len(), target_count);
        let mut multiplicities = try_vec_with_capacity(
            target_count,
            base_live_bytes
                .checked_add(patterns_live)
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .and_then(|bytes| bytes.checked_add(redundant_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_multiplicities",
        )?;
        multiplicities.extend_from_slice(target_weights);
        let multiplicities_live = checked_vec_retained_bytes(&multiplicities)?;
        let existing_live = patterns_live
            .checked_add(support_live)
            .and_then(|bytes| bytes.checked_add(order_live))
            .and_then(|bytes| bytes.checked_add(redundant_live))
            .and_then(|bytes| bytes.checked_add(multiplicities_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let row_slot_count = support_word_count
            .checked_mul(u64::BITS as usize)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut row_constraint_counts = try_vec_with_capacity(
            row_slot_count,
            base_live_bytes
                .checked_add(existing_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_row_counts",
        )?;
        row_constraint_counts.resize(row_slot_count, 0_usize);
        for constraint_index in 0..target_count {
            let support_offset = constraint_index * support_word_count;
            for (word_index, support_word) in support_words
                [support_offset..support_offset + support_word_count]
                .iter()
                .copied()
                .enumerate()
            {
                let mut remaining = support_word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    let row = word_index * u64::BITS as usize + bit;
                    row_constraint_counts[row] = row_constraint_counts[row]
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    remaining &= remaining - 1;
                }
            }
        }
        let row_counts_live = checked_vec_retained_bytes(&row_constraint_counts)?;
        let mut anchor_rows = try_vec_with_capacity(
            target_count,
            base_live_bytes
                .checked_add(existing_live)
                .and_then(|bytes| bytes.checked_add(row_counts_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_target_constraint_anchor_rows",
        )?;
        for constraint_index in 0..target_count {
            let support_offset = constraint_index * support_word_count;
            let mut anchor = usize::MAX;
            let mut anchor_count = usize::MAX;
            let mut secondary_anchor = usize::MAX;
            let mut secondary_anchor_count = usize::MAX;
            for (word_index, support_word) in support_words
                [support_offset..support_offset + support_word_count]
                .iter()
                .copied()
                .enumerate()
            {
                let mut remaining = support_word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    let row = word_index * u64::BITS as usize + bit;
                    let count = row_constraint_counts[row];
                    if count < anchor_count {
                        secondary_anchor = anchor;
                        secondary_anchor_count = anchor_count;
                        anchor = row;
                        anchor_count = count;
                    } else if count < secondary_anchor_count {
                        secondary_anchor = row;
                        secondary_anchor_count = count;
                    }
                    remaining &= remaining - 1;
                }
            }
            anchor_rows.push((anchor, secondary_anchor));
        }
        drop(row_constraint_counts);

        let mut index = Self {
            source_patterns,
            support_words,
            support_word_count,
            order,
            redundant,
            multiplicities,
            anchor_rows,
        };
        index.quotient_equal_constraints(remove_support_supersets)?;
        memory_guard(
            base_live_bytes
                .checked_add(index.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(index)
    }

    fn quotient_equal_constraints(
        &mut self,
        remove_support_supersets: bool,
    ) -> Result<(), ExactMinimumCoverError> {
        let support_words = &self.support_words;
        let support_word_count = self.support_word_count;
        let source_patterns = &self.source_patterns;
        self.order.sort_unstable_by(|left, right| {
            let left_start = *left * support_word_count;
            let right_start = *right * support_word_count;
            support_words[left_start..left_start + support_word_count]
                .cmp(&support_words[right_start..right_start + support_word_count])
                .then_with(|| source_patterns[*left].cmp(&source_patterns[*right]))
        });
        let mut group_start = 0;
        while group_start < self.order.len() {
            let representative = self.order[group_start];
            let mut group_end = group_start + 1;
            while group_end < self.order.len()
                && self.support(self.order[group_end]) == self.support(representative)
            {
                self.redundant[self.order[group_end]] = true;
                self.multiplicities[representative] = self.multiplicities[representative]
                    .checked_add(self.multiplicities[self.order[group_end]])
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                group_end += 1;
            }
            group_start = group_end;
        }
        self.order.retain(|index| !self.redundant[*index]);
        if remove_support_supersets {
            self.remove_support_superset_constraints();
        }
        self.order
            .sort_unstable_by_key(|index| self.source_patterns[*index]);
        Ok(())
    }

    fn remove_support_superset_constraints(&mut self) {
        let support_words = &self.support_words;
        let support_word_count = self.support_word_count;
        let source_patterns = &self.source_patterns;
        self.order.sort_unstable_by(|left, right| {
            let left_start = *left * support_word_count;
            let right_start = *right * support_word_count;
            let left_count = support_words[left_start..left_start + support_word_count]
                .iter()
                .map(|word| word.count_ones())
                .sum::<u32>();
            let right_count = support_words[right_start..right_start + support_word_count]
                .iter()
                .map(|word| word.count_ones())
                .sum::<u32>();
            left_count
                .cmp(&right_count)
                .then_with(|| source_patterns[*left].cmp(&source_patterns[*right]))
        });
        for right_position in 0..self.order.len() {
            let right = self.order[right_position];
            for left_position in 0..right_position {
                let left = self.order[left_position];
                if self.redundant[left] {
                    continue;
                }
                // Any subset support must contain the first row in `left`.
                // Rejecting on that single anchor bit avoids scanning every
                // support word for the overwhelmingly common non-candidates
                // while preserving the exact subset test below.
                let (anchor_row, secondary_anchor_row) = self.anchor_rows[left];
                if anchor_row == usize::MAX {
                    continue;
                }
                let anchor_word = anchor_row / u64::BITS as usize;
                let anchor_bit = 1_u64 << (anchor_row % u64::BITS as usize);
                if self.support(right)[anchor_word] & anchor_bit == 0 {
                    continue;
                }
                if secondary_anchor_row != usize::MAX {
                    let secondary_word = secondary_anchor_row / u64::BITS as usize;
                    let secondary_bit = 1_u64 << (secondary_anchor_row % u64::BITS as usize);
                    if self.support(right)[secondary_word] & secondary_bit == 0 {
                        continue;
                    }
                }
                if is_superset(self.support(right), self.support(left)) {
                    self.redundant[right] = true;
                    break;
                }
            }
        }
        self.order.retain(|index| !self.redundant[*index]);
    }

    fn support(&self, constraint_index: usize) -> &[u64] {
        let start = constraint_index * self.support_word_count;
        &self.support_words[start..start + self.support_word_count]
    }

    fn support_contains_row(&self, constraint_index: usize, row_index: usize) -> bool {
        self.support(constraint_index)[row_index / u64::BITS as usize]
            & (1_u64 << (row_index % u64::BITS as usize))
            != 0
    }

    fn retained_constraint_count(&self) -> usize {
        self.order.len()
    }

    fn checked_heap_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.source_patterns)?
            .checked_add(checked_vec_retained_bytes(&self.support_words)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.order).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.redundant).ok()?))
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.multiplicities).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&self.anchor_rows).ok()?)
            })
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }
}

#[derive(Clone, Debug)]
struct ExactCompactTarget {
    words: Vec<u64>,
    weights: Vec<usize>,
}

fn quotient_redundant_target_constraints_with_memory_guard(
    rows: &mut Vec<DenseRow>,
    target_words: Vec<u64>,
    target_weights: Vec<usize>,
    remove_support_supersets: bool,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactCompactTarget, ExactMinimumCoverError> {
    if target_words.iter().all(|word| *word == 0) {
        return Ok(ExactCompactTarget {
            words: target_words,
            weights: target_weights,
        });
    }
    let rows_live = checked_dense_rows_retained_bytes(rows)?;
    let target_live = checked_vec_retained_bytes(&target_words)?;
    let target_weights_live = checked_vec_retained_bytes(&target_weights)?;
    let index = ExactTargetConstraintIndex::try_new(
        rows,
        &target_words,
        &target_weights,
        rows_live
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(target_weights_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        remove_support_supersets,
        memory_guard,
    )?;
    let index_live = index.checked_heap_retained_bytes()?;
    let retained_count = index.retained_constraint_count();
    let compact_word_count = retained_count.div_ceil(u64::BITS as usize);
    let mut compact_target = try_vec_with_capacity(
        compact_word_count,
        rows_live
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(target_weights_live))
            .and_then(|bytes| bytes.checked_add(index_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_compact_target_words",
    )?;
    compact_target.resize(compact_word_count, u64::MAX);
    if let Some(last) = compact_target.last_mut() {
        let tail_bits = retained_count % u64::BITS as usize;
        if tail_bits != 0 {
            *last = (1_u64 << tail_bits) - 1;
        }
    }
    let compact_target_live = checked_vec_retained_bytes(&compact_target)?;
    let mut compact_weights = try_vec_with_capacity(
        retained_count,
        rows_live
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(target_weights_live))
            .and_then(|bytes| bytes.checked_add(index_live))
            .and_then(|bytes| bytes.checked_add(compact_target_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_compact_target_weights",
    )?;
    compact_weights.extend(
        index
            .order
            .iter()
            .map(|constraint_index| index.multiplicities[*constraint_index]),
    );
    let compact_weights_live = checked_vec_retained_bytes(&compact_weights)?;

    // Replacing a row changes only its word allocation. Recounting the whole
    // matrix around every replacement turns lossless quotient preparation
    // into quadratic bookkeeping, repeated by canonical suffix proofs.
    let mut current_rows_live = rows_live;
    for row_index in 0..rows.len() {
        let mut compact_row = try_vec_with_capacity(
            compact_word_count,
            current_rows_live
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(target_weights_live))
                .and_then(|bytes| bytes.checked_add(index_live))
                .and_then(|bytes| bytes.checked_add(compact_target_live))
                .and_then(|bytes| bytes.checked_add(compact_weights_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_compact_row_words",
        )?;
        compact_row.resize(compact_word_count, 0);
        for (compact_pattern, constraint_index) in index.order.iter().copied().enumerate() {
            if index.support_contains_row(constraint_index, row_index) {
                compact_row[compact_pattern / u64::BITS as usize] |=
                    1_u64 << (compact_pattern % u64::BITS as usize);
            }
        }
        current_rows_live = current_rows_live
            .checked_sub(checked_vec_retained_bytes(&rows[row_index].words)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&compact_row).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        rows[row_index].words = compact_row;
        memory_guard(
            current_rows_live
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(target_weights_live))
                .and_then(|bytes| bytes.checked_add(index_live))
                .and_then(|bytes| bytes.checked_add(compact_target_live))
                .and_then(|bytes| bytes.checked_add(compact_weights_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
    }
    drop(target_words);
    drop(target_weights);
    drop(index);
    memory_guard(
        current_rows_live
            .checked_add(compact_target_live)
            .and_then(|bytes| bytes.checked_add(compact_weights_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(ExactCompactTarget {
        words: compact_target,
        weights: compact_weights,
    })
}

const EMPTY_MEMO_BUCKET: usize = usize::MAX;
const MIN_MEMO_BUCKET_COUNT: usize = 8;

#[derive(Clone, Debug)]
struct ExactCoveredStateMemoEntry {
    words: Vec<u64>,
    depth: usize,
    hash: u64,
}

/// Exact covered-state memoization with deterministic open addressing.
///
/// The former sorted `Vec` inserted every new state in lexicographic order,
/// shifting a growing suffix of large (often 79-word) states. Hashes are only
/// an index hint: a hit still compares every word, so collisions cannot change
/// solver correctness. Retained bytes are cached and updated only at allocation
/// boundaries, keeping memory-guard accounting O(1) per recursive node.
#[derive(Clone, Debug)]
struct ExactCoveredStateMemo {
    entries: Vec<ExactCoveredStateMemoEntry>,
    buckets: Vec<usize>,
    state_word_retained_bytes: u128,
}

impl ExactCoveredStateMemo {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            buckets: Vec::new(),
            state_word_retained_bytes: 0,
        }
    }

    fn find(&self, covered: &[u64], hash: u64) -> Option<usize> {
        if self.buckets.is_empty() {
            return None;
        }
        let mask = self.buckets.len() - 1;
        let mut bucket = hash as usize & mask;
        loop {
            let entry_index = self.buckets[bucket];
            if entry_index == EMPTY_MEMO_BUCKET {
                return None;
            }
            let entry = &self.entries[entry_index];
            if entry.hash == hash && entry.words == covered {
                return Some(entry_index);
            }
            bucket = (bucket + 1) & mask;
        }
    }

    fn should_prune_or_record(
        &mut self,
        covered: &[u64],
        depth: usize,
        external_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, ExactMinimumCoverError> {
        let hash = exact_memo_hash(covered);
        if let Some(entry_index) = self.find(covered, hash) {
            let entry = &mut self.entries[entry_index];
            if entry.depth <= depth {
                return Ok(true);
            }
            entry.depth = depth;
            return Ok(false);
        }

        self.ensure_insert_capacity(external_live_bytes, memory_guard)?;
        let memo_live = self.checked_heap_retained_bytes()?;
        let mut state = try_vec_with_capacity(
            covered.len(),
            external_live_bytes
                .checked_add(memo_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_memo_state",
        )?;
        state.extend_from_slice(covered);
        let state_live = checked_vec_retained_bytes(&state)?;

        let entries_before = checked_vec_retained_bytes(&self.entries)?;
        // Match the hash table's geometric capacity. Reserving exactly one
        // entry per new state can copy the whole growing memo on every DFS
        // node, especially in WASM's allocator, despite O(1) hash lookup.
        let mut additional_entries = if self.entries.len() == self.entries.capacity() {
            (self.buckets.len() / 2)
                .checked_sub(self.entries.len())
                .filter(|additional| *additional > 0)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?
        } else {
            0
        };
        let insertion_live = external_live_bytes
            .checked_add(memo_live)
            .and_then(|bytes| bytes.checked_add(state_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let requested_peak = insertion_live
            .checked_add(checked_requested_growth_bytes(
                &self.entries,
                additional_entries,
            )?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        match memory_guard(requested_peak) {
            Err(ExactMinimumCoverError::MemoryCapacityExceeded { .. })
                if additional_entries > 1 =>
            {
                // Spare capacity is optional. A tightly capped caller must
                // still admit the exact next state when only that fits.
                additional_entries = 1;
                memory_guard(
                    insertion_live
                        .checked_add(checked_requested_growth_bytes(&self.entries, 1)?)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )?;
            }
            result => result?,
        }
        self.entries
            .try_reserve_exact(additional_entries)
            .map_err(|_| ExactMinimumCoverError::AllocationFailed {
                component: "exact_minimum_cover_memo_entries",
            })?;
        let entries_after = checked_vec_retained_bytes(&self.entries)?;
        let non_entry_memo_live = memo_live
            .checked_sub(entries_before)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        memory_guard(
            external_live_bytes
                .checked_add(non_entry_memo_live)
                .and_then(|bytes| bytes.checked_add(entries_after))
                .and_then(|bytes| bytes.checked_add(state_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;

        let entry_index = self.entries.len();
        self.entries.push(ExactCoveredStateMemoEntry {
            words: state,
            depth,
            hash,
        });
        self.state_word_retained_bytes = self
            .state_word_retained_bytes
            .checked_add(state_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        self.insert_bucket(entry_index);
        memory_guard(
            external_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(false)
    }

    fn ensure_insert_capacity(
        &mut self,
        external_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<(), ExactMinimumCoverError> {
        let needs_growth = self.buckets.is_empty()
            || self
                .entries
                .len()
                .checked_add(1)
                .and_then(|next| next.checked_mul(2))
                .is_none_or(|required| required > self.buckets.len());
        if !needs_growth {
            return Ok(());
        }
        let next_bucket_count = if self.buckets.is_empty() {
            MIN_MEMO_BUCKET_COUNT
        } else {
            self.buckets
                .len()
                .checked_mul(2)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?
        };
        let old_memo_live = self.checked_heap_retained_bytes()?;
        let mut next_buckets = try_vec_with_capacity(
            next_bucket_count,
            external_live_bytes
                .checked_add(old_memo_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_memo_buckets",
        )?;
        next_buckets.resize(next_bucket_count, EMPTY_MEMO_BUCKET);
        for (entry_index, entry) in self.entries.iter().enumerate() {
            insert_memo_bucket(&mut next_buckets, entry.hash, entry_index);
        }
        self.buckets = next_buckets;
        memory_guard(
            external_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(())
    }

    fn insert_bucket(&mut self, entry_index: usize) {
        insert_memo_bucket(
            &mut self.buckets,
            self.entries[entry_index].hash,
            entry_index,
        );
    }

    fn checked_heap_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.entries)?
            .checked_add(checked_vec_retained_bytes(&self.buckets)?)
            .and_then(|bytes| bytes.checked_add(self.state_word_retained_bytes))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }
}

fn insert_memo_bucket(buckets: &mut [usize], hash: u64, entry_index: usize) {
    debug_assert!(buckets.len().is_power_of_two());
    let mask = buckets.len() - 1;
    let mut bucket = hash as usize & mask;
    while buckets[bucket] != EMPTY_MEMO_BUCKET {
        bucket = (bucket + 1) & mask;
    }
    buckets[bucket] = entry_index;
}

fn exact_memo_hash(covered: &[u64]) -> u64 {
    // Fixed FNV-1a plus a final avalanche: deterministic across processes and
    // targets, with equality always verified by `find`.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for word in covered {
        hash ^= *word;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= covered.len() as u64;
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

enum MinimumCoverSolveDecision {
    Found(Vec<usize>),
    ProvedNone,
    Cancelled,
}

// The search variant remains inline so preparation does not introduce an
// unguarded heap allocation solely to box the solver state.
#[allow(clippy::large_enum_variant)]
enum MinimumCoverSearchPreparation {
    Search(MinimumCoverSearch),
    Found(Vec<usize>),
    Cancelled,
}

#[derive(Clone, Debug)]
struct MinimumCoverSearch {
    target_words: Vec<u64>,
    constraint_weights: Vec<usize>,
    support_by_pattern: Vec<Vec<usize>>,
    support_pattern_order: Vec<usize>,
    packing_support_marks: Vec<u32>,
    packing_patterns: Vec<usize>,
    packing_adjusted_degrees: Vec<usize>,
    packing_generation: u32,
    excluded_rows: Vec<u64>,
    selected: Vec<bool>,
    current: Vec<usize>,
    best: Vec<usize>,
    goal: ExactCoverSearchGoal,
    dual_workspace: Option<DualProposalWorkspace>,
    root_dual: Option<CertifiedResidualDual>,
    cancelled: bool,
    fixed_retained_bytes: u128,
    memo_depth: ExactCoveredStateMemo,
    covered: Vec<u64>,
    frames: Vec<MinimumCoverSearchFrame>,
    enter_child: bool,
    finished: bool,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_conditional_rows: ExactMinimumCoverConditionalRowDiagnostics,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_conditional_rows_enabled: bool,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_search_nodes: u64,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_attempts: u64,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_iterations: u64,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_prunes: u64,
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_attempts_by_dual_gap: [u64; 4],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_iterations_by_dual_gap: [u64; 4],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_prunes_by_dual_gap: [u64; 4],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_attempts_by_depth: [u64; 3],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_iterations_by_depth: [u64; 3],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_prunes_by_depth: [u64; 3],
    #[cfg(any(test, feature = "diagnostic-probes"))]
    diagnostic_residual_prunes_by_checkpoint: [u64; 8],
    #[cfg(feature = "diagnostic-probes")]
    diagnostic_hot_cost: ExactMinimumCoverHotCostDiagnostics,
    #[cfg(feature = "diagnostic-probes")]
    diagnostic_residual_admission: ExactMinimumCoverResidualAdmissionPolicy,
}

#[derive(Clone, Debug)]
struct MinimumCoverSearchFrame {
    saved_covered: Vec<u64>,
    forced_rows: Vec<usize>,
    branches: Vec<usize>,
    next_branch: usize,
    active_branch: Option<MinimumCoverActiveBranch>,
}

#[derive(Clone, Debug)]
struct MinimumCoverActiveBranch {
    row_index: usize,
    changed_words: Vec<(usize, u64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MinimumCoverSearchAdvance {
    Pending {
        visited_nodes: u64,
        consumed_residual_dual: bool,
    },
    Finished {
        visited_nodes: u64,
        consumed_residual_dual: bool,
    },
    Cancelled {
        visited_nodes: u64,
        consumed_residual_dual: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MinimumCoverNodeEntry {
    Complete,
    Descend,
}

impl MinimumCoverSearch {
    #[cfg(any(test, feature = "diagnostic-probes"))]
    fn diagnostic_residual_progress(&self) -> ExactMinimumCoverResidualDiagnostics {
        ExactMinimumCoverResidualDiagnostics {
            search_nodes: self.diagnostic_search_nodes,
            proposal_attempts: self.diagnostic_residual_attempts,
            proposal_iterations: self.diagnostic_residual_iterations,
            certified_prunes: self.diagnostic_residual_prunes,
            remaining_proposal_iterations: self
                .dual_workspace
                .as_ref()
                .map_or(0, DualProposalWorkspace::remaining_proposal_iterations),
            proposal_attempts_by_dual_gap: self.diagnostic_residual_attempts_by_dual_gap,
            proposal_iterations_by_dual_gap: self.diagnostic_residual_iterations_by_dual_gap,
            certified_prunes_by_dual_gap: self.diagnostic_residual_prunes_by_dual_gap,
            proposal_attempts_by_depth: self.diagnostic_residual_attempts_by_depth,
            proposal_iterations_by_depth: self.diagnostic_residual_iterations_by_depth,
            certified_prunes_by_depth: self.diagnostic_residual_prunes_by_depth,
            certified_prunes_by_checkpoint: self.diagnostic_residual_prunes_by_checkpoint,
        }
    }

    #[cfg(feature = "diagnostic-probes")]
    fn diagnostic_hot_cost(&self) -> ExactMinimumCoverHotCostDiagnostics {
        let mut diagnostics = self.diagnostic_hot_cost;
        if let Some(workspace) = self.dual_workspace.as_ref() {
            let residual = workspace.diagnostic_hot_cost();
            diagnostics.residual_prepare_calls = residual.prepare_calls;
            diagnostics.residual_prepare_nanoseconds = residual.prepare_nanoseconds;
            diagnostics.mirror_prox_iterations = residual.mirror_prox_iterations;
            diagnostics.mirror_prox_nanoseconds = residual.mirror_prox_nanoseconds;
            diagnostics.softmax_p_nanoseconds = residual.softmax_p_nanoseconds;
            diagnostics.softmax_q_nanoseconds = residual.softmax_q_nanoseconds;
            diagnostics.softmax_middle_p_nanoseconds = residual.softmax_middle_p_nanoseconds;
            diagnostics.softmax_middle_q_nanoseconds = residual.softmax_middle_q_nanoseconds;
            diagnostics.softmax_p_entries = residual.softmax_p_entries;
            diagnostics.softmax_p_cutoff_entries = residual.softmax_p_cutoff_entries;
            diagnostics.softmax_q_entries = residual.softmax_q_entries;
            diagnostics.softmax_q_cutoff_entries = residual.softmax_q_cutoff_entries;
            diagnostics.softmax_q_row_incidences = residual.softmax_q_row_incidences;
            diagnostics.softmax_q_cutoff_row_incidences = residual.softmax_q_cutoff_row_incidences;
            diagnostics.softmax_middle_p_entries = residual.softmax_middle_p_entries;
            diagnostics.softmax_middle_p_cutoff_entries = residual.softmax_middle_p_cutoff_entries;
            diagnostics.softmax_middle_q_entries = residual.softmax_middle_q_entries;
            diagnostics.softmax_middle_q_cutoff_entries = residual.softmax_middle_q_cutoff_entries;
            diagnostics.softmax_middle_q_row_incidences = residual.softmax_middle_q_row_incidences;
            diagnostics.softmax_middle_q_cutoff_row_incidences =
                residual.softmax_middle_q_cutoff_row_incidences;
            diagnostics.first_gradient_nanoseconds = residual.first_gradient_nanoseconds;
            diagnostics.middle_gradient_nanoseconds = residual.middle_gradient_nanoseconds;
            diagnostics.log_update_nanoseconds = residual.log_update_nanoseconds;
            diagnostics.averaging_nanoseconds = residual.averaging_nanoseconds;
            diagnostics.exact_recertification_calls = residual.certificate_calls;
            diagnostics.exact_recertification_nanoseconds = residual.certificate_nanoseconds;
        }
        diagnostics
    }

    #[cfg(feature = "diagnostic-probes")]
    fn reset_diagnostic_hot_cost(&mut self) {
        self.diagnostic_hot_cost = ExactMinimumCoverHotCostDiagnostics::default();
        if let Some(workspace) = self.dual_workspace.as_mut() {
            workspace.reset_diagnostic_hot_cost();
        }
    }

    /// Probe-only recursive control for checking that the owned explicit stack
    /// visits the same proof tree as the pre-continuation implementation. All
    /// node preparation and bounds are the production implementations; only
    /// call-stack ownership differs.
    #[cfg(feature = "diagnostic-probes")]
    fn diagnostic_solve_recursive_reference(
        &mut self,
        rows: &[DenseRow],
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<u64, ExactMinimumCoverError> {
        if self.finished
            || self.cancelled
            || !self.enter_child
            || !self.frames.is_empty()
            || !self.current.is_empty()
            || self.excluded_rows.iter().any(|word| *word != 0)
        {
            return Err(ExactMinimumCoverError::ProjectionOverflow);
        }
        let mut visited_nodes = 0_u64;
        self.diagnostic_search_recursive_reference_node(
            rows,
            base_live_bytes,
            memory_guard,
            &mut visited_nodes,
        )?;
        self.finished = true;
        Ok(visited_nodes)
    }

    #[cfg(feature = "diagnostic-probes")]
    fn diagnostic_search_recursive_reference_node(
        &mut self,
        rows: &[DenseRow],
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        visited_nodes: &mut u64,
    ) -> Result<bool, ExactMinimumCoverError> {
        if self.feasibility_goal_is_satisfied() {
            return Ok(true);
        }
        *visited_nodes = visited_nodes
            .checked_add(1)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        checked_add_diagnostic_counter(&mut self.diagnostic_search_nodes, 1)?;

        let mut saved_covered = Vec::new();
        let mut forced_rows = Vec::new();
        if let Some((first_pivot, first_support)) = self.rarest_uncovered_pattern(&self.covered) {
            if first_support == 0 {
                return Ok(false);
            }
            if first_support == 1 {
                let search_live = base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                saved_covered = try_vec_with_capacity(
                    self.covered.len(),
                    search_live,
                    memory_guard,
                    "exact_minimum_cover_recursive_reference_saved_covered",
                )?;
                saved_covered.extend_from_slice(&self.covered);
                let saved_live = checked_vec_retained_bytes(&saved_covered)?;
                forced_rows = try_vec_with_capacity(
                    rows.len(),
                    search_live
                        .checked_add(saved_live)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    memory_guard,
                    "exact_minimum_cover_recursive_reference_forced_rows",
                )?;

                let mut next = Some((first_pivot, first_support));
                while let Some((pivot, support)) = next {
                    if support == 0 {
                        self.restore_unit_rows(&saved_covered, &forced_rows);
                        return Ok(false);
                    }
                    if support > 1 {
                        break;
                    }
                    let row_index = self.support_by_pattern[pivot]
                        .iter()
                        .copied()
                        .find(|row_index| {
                            !self.selected[*row_index] && !self.row_is_excluded(*row_index)
                        })
                        .expect("unit support identifies one recursive-reference row");
                    self.selected[row_index] = true;
                    self.current.push(row_index);
                    forced_rows.push(row_index);
                    union_words(&mut self.covered, &rows[row_index].words);
                    next = if is_superset(&self.covered, &self.target_words) {
                        None
                    } else {
                        self.rarest_uncovered_pattern(&self.covered)
                    };
                }
            }
        }

        let recursive_scratch_bytes = checked_vec_retained_bytes(&saved_covered)?
            .checked_add(checked_vec_retained_bytes(&forced_rows)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let covered = core::mem::take(&mut self.covered);
        let prepared = self.prepare_reduced_node(
            rows,
            &covered,
            base_live_bytes,
            recursive_scratch_bytes,
            memory_guard,
        );
        self.covered = covered;
        let branches = match prepared {
            Ok(Some(branches)) => branches,
            Ok(None) => {
                self.restore_unit_rows(&saved_covered, &forced_rows);
                return Ok(self.feasibility_goal_is_satisfied());
            }
            Err(error) => {
                self.restore_unit_rows(&saved_covered, &forced_rows);
                return Err(error);
            }
        };

        for row_index in branches.iter().copied() {
            let changed_count = rows[row_index]
                .words
                .iter()
                .copied()
                .zip(self.covered.iter().copied())
                .filter(|(row_word, covered_word)| *covered_word | *row_word != *covered_word)
                .count();
            let mut changed_words = try_vec_with_capacity(
                changed_count,
                base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                "exact_minimum_cover_recursive_reference_changed_words",
            )?;
            self.selected[row_index] = true;
            self.current.push(row_index);
            for (word_index, row_word) in rows[row_index].words.iter().copied().enumerate() {
                let next = self.covered[word_index] | row_word;
                if next != self.covered[word_index] {
                    changed_words.push((word_index, self.covered[word_index]));
                    self.covered[word_index] = next;
                }
            }

            let child = self.diagnostic_search_recursive_reference_node(
                rows,
                base_live_bytes,
                memory_guard,
                visited_nodes,
            );
            for (word_index, previous) in changed_words {
                self.covered[word_index] = previous;
            }
            let popped = self.current.pop();
            debug_assert_eq!(popped, Some(row_index));
            self.selected[row_index] = false;

            match child {
                Ok(true) => {
                    for branch in branches.iter().copied() {
                        self.set_row_excluded(branch, false);
                    }
                    self.restore_unit_rows(&saved_covered, &forced_rows);
                    return Ok(true);
                }
                Ok(false) => self.set_row_excluded(row_index, true),
                Err(error) => {
                    for branch in branches.iter().copied() {
                        self.set_row_excluded(branch, false);
                    }
                    self.restore_unit_rows(&saved_covered, &forced_rows);
                    return Err(error);
                }
            }
        }

        for row_index in branches {
            self.set_row_excluded(row_index, false);
        }
        self.restore_unit_rows(&saved_covered, &forced_rows);
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn try_new(
        rows: &[DenseRow],
        target_words: Vec<u64>,
        constraint_weights: Vec<usize>,
        goal: ExactCoverSearchGoal,
        incumbent_policy: ExactCoverIncumbentPolicy,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
        eager_acceleration: bool,
    ) -> Result<MinimumCoverSearchPreparation, ExactMinimumCoverError> {
        let target_live = checked_vec_retained_bytes(&target_words)?;
        let weights_live = checked_vec_retained_bytes(&constraint_weights)?;
        let mut best = greedy_cover_with_memory_guard(
            rows,
            &target_words,
            &constraint_weights,
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
        )?
        .unwrap_or_else(Vec::new);
        let target_constraint_count = target_words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        let incumbent_trial_budget = match (goal, incumbent_policy) {
            (ExactCoverSearchGoal::Minimum, _) if eager_acceleration => {
                RANDOMIZED_COMPACT_COVER_TRIALS
            }
            _ => 0,
        };
        if incumbent_trial_budget != 0
            && rows.len() >= 64
            && target_constraint_count >= 128
            && !best.is_empty()
        {
            if improve_randomized_compact_cover_with_memory_guard(
                rows,
                &target_words,
                &mut best,
                incumbent_trial_budget,
                RANDOMIZED_COMPACT_COVER_SEED,
                None,
                None,
                base_live_bytes
                    .checked_add(target_live)
                    .and_then(|bytes| bytes.checked_add(weights_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                cancelled,
            )? == IncumbentSearchOutcome::Cancelled
            {
                return Ok(MinimumCoverSearchPreparation::Cancelled);
            }
            if matches!(goal, ExactCoverSearchGoal::AtMost(limit) if best.len() <= limit) {
                return Ok(MinimumCoverSearchPreparation::Found(best));
            }
        }
        let best_live = checked_vec_retained_bytes(&best)?;
        let persistent_live = base_live_bytes
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(weights_live))
            .and_then(|bytes| bytes.checked_add(best_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let support_by_pattern = build_support_by_pattern_with_memory_guard(
            rows,
            &target_words,
            persistent_live,
            memory_guard,
        )?;
        let support_live = checked_support_retained_bytes(&support_by_pattern)?;
        let breakout_swap_budget = match (goal, incumbent_policy) {
            (ExactCoverSearchGoal::Minimum, _) if eager_acceleration => BREAKOUT_TOTAL_SWAP_BUDGET,
            (
                ExactCoverSearchGoal::AtMost(limit),
                ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch,
            ) if best.len() > limit => WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
            _ => 0,
        };
        if breakout_swap_budget != 0
            && rows.len() >= 64
            && target_constraint_count >= 128
            && best.len() > 1
        {
            if improve_fixed_cardinality_cover_with_memory_guard(
                rows,
                &target_words,
                &support_by_pattern,
                &mut best,
                breakout_swap_budget,
                None,
                base_live_bytes
                    .checked_add(target_live)
                    .and_then(|bytes| bytes.checked_add(weights_live))
                    .and_then(|bytes| bytes.checked_add(support_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                cancelled,
            )? {
                return Ok(MinimumCoverSearchPreparation::Cancelled);
            }
            if matches!(goal, ExactCoverSearchGoal::AtMost(limit) if best.len() <= limit) {
                return Ok(MinimumCoverSearchPreparation::Found(best));
            }
        }
        if let ExactCoverSearchGoal::AtMost(limit) = goal {
            if best.is_empty() || best.len() > limit {
                let sentinel_len = limit
                    .checked_add(1)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                if sentinel_len > rows.len() {
                    return Err(ExactMinimumCoverError::ProjectionOverflow);
                }
                if best.capacity() < sentinel_len {
                    let sentinel_live = base_live_bytes
                        .checked_add(target_live)
                        .and_then(|bytes| bytes.checked_add(weights_live))
                        .and_then(|bytes| bytes.checked_add(support_live))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                    best = try_vec_with_capacity(
                        sentinel_len,
                        sentinel_live,
                        memory_guard,
                        "exact_cover_at_most_sentinel",
                    )?;
                } else {
                    best.clear();
                }
                best.resize(sentinel_len, usize::MAX);
            }
        }
        let best_live = checked_vec_retained_bytes(&best)?;
        let mut row_pattern_counts = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_packing_row_pattern_counts",
        )?;
        row_pattern_counts.extend(rows.iter().map(|row| {
            row.words
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>()
        }));
        let row_pattern_counts_live = checked_vec_retained_bytes(&row_pattern_counts)?;
        let mut packing_order_keys = try_vec_with_capacity(
            constraint_weights.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(row_pattern_counts_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_packing_order_keys",
        )?;
        packing_order_keys.extend(
            support_by_pattern
                .iter()
                .take(constraint_weights.len())
                .map(|support| {
                    support
                        .iter()
                        .copied()
                        .map(|row_index| row_pattern_counts[row_index])
                        .fold((0_usize, 0_usize), |(sum, maximum), degree| {
                            (sum.saturating_add(degree), maximum.max(degree))
                        })
                }),
        );
        let packing_order_keys_live = checked_vec_retained_bytes(&packing_order_keys)?;
        let mut support_pattern_order = try_vec_with_capacity(
            constraint_weights.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(row_pattern_counts_live))
                .and_then(|bytes| bytes.checked_add(packing_order_keys_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_support_pattern_order",
        )?;
        support_pattern_order.extend(0..constraint_weights.len());
        support_pattern_order.sort_unstable_by(|left, right| {
            packing_order_keys[*left]
                .cmp(&packing_order_keys[*right])
                .then_with(|| left.cmp(right))
        });
        drop(packing_order_keys);
        drop(row_pattern_counts);
        let order_live = checked_vec_retained_bytes(&support_pattern_order)?;
        let mut packing_support_marks = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_packing_support_marks",
        )?;
        packing_support_marks.resize(rows.len(), 0);
        let packing_live = checked_vec_retained_bytes(&packing_support_marks)?;
        let packing_patterns = try_vec_with_capacity(
            constraint_weights.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .and_then(|bytes| bytes.checked_add(packing_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_packing_patterns",
        )?;
        let packing_patterns_live = checked_vec_retained_bytes(&packing_patterns)?;
        let mut packing_adjusted_degrees = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .and_then(|bytes| bytes.checked_add(packing_live))
                .and_then(|bytes| bytes.checked_add(packing_patterns_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_packing_adjusted_degrees",
        )?;
        packing_adjusted_degrees.resize(rows.len(), 0);
        let adjusted_degrees_live = checked_vec_retained_bytes(&packing_adjusted_degrees)?;
        let excluded_word_count = rows.len().div_ceil(u64::BITS as usize);
        let mut excluded_rows = try_vec_with_capacity(
            excluded_word_count,
            base_live_bytes
                .checked_add(target_live)
                .and_then(|bytes| bytes.checked_add(weights_live))
                .and_then(|bytes| bytes.checked_add(best_live))
                .and_then(|bytes| bytes.checked_add(support_live))
                .and_then(|bytes| bytes.checked_add(order_live))
                .and_then(|bytes| bytes.checked_add(packing_live))
                .and_then(|bytes| bytes.checked_add(packing_patterns_live))
                .and_then(|bytes| bytes.checked_add(adjusted_degrees_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_excluded_rows",
        )?;
        excluded_rows.resize(excluded_word_count, 0);
        let excluded_live = checked_vec_retained_bytes(&excluded_rows)?;
        let construction_live = base_live_bytes
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(weights_live))
            .and_then(|bytes| bytes.checked_add(best_live))
            .and_then(|bytes| bytes.checked_add(support_live))
            .and_then(|bytes| bytes.checked_add(order_live))
            .and_then(|bytes| bytes.checked_add(packing_live))
            .and_then(|bytes| bytes.checked_add(packing_patterns_live))
            .and_then(|bytes| bytes.checked_add(adjusted_degrees_live))
            .and_then(|bytes| bytes.checked_add(excluded_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut selected = try_vec_with_capacity(
            rows.len(),
            construction_live,
            memory_guard,
            "exact_minimum_cover_selected_flags",
        )?;
        selected.resize(rows.len(), false);
        let selected_live = checked_vec_retained_bytes(&selected)?;
        let current = try_vec_with_capacity(
            rows.len(),
            construction_live
                .checked_add(selected_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_current_rows",
        )?;
        let current_live = checked_vec_retained_bytes(&current)?;
        let static_live = construction_live
            .checked_add(selected_live)
            .and_then(|bytes| bytes.checked_add(current_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let root_incidence_count =
            target_words
                .iter()
                .enumerate()
                .try_fold(0_usize, |count, (word_index, target)| {
                    let mut required = *target;
                    let mut next = Some(count);
                    while required != 0 {
                        let bit = required.trailing_zeros() as usize;
                        let pattern = word_index * u64::BITS as usize + bit;
                        next = next?.checked_add(support_by_pattern.get(pattern)?.len());
                        required &= required - 1;
                    }
                    next
                });
        let dual_workspace_dimensions = root_incidence_count.filter(|count| {
            should_prepare_root_dual(rows.len(), target_constraint_count)
                && *count <= MAX_DUAL_INCIDENCE_COUNT
        });
        let dual_workspace_preflight = dual_workspace_dimensions
            .and_then(|incidence_count| {
                checked_residual_dual_memory_projection(
                    rows.len(),
                    target_constraint_count,
                    incidence_count,
                )
            })
            .and_then(|projection| static_live.checked_add(projection.required_peak_bytes));
        let mut dual_workspace = if let (Some(incidence_count), Some(requested_peak)) =
            (dual_workspace_dimensions, dual_workspace_preflight)
        {
            if optional_dual_preflight(memory_guard(requested_peak))? {
                optional_dual_acceleration(DualProposalWorkspace::try_new(
                    rows.len(),
                    target_constraint_count,
                    incidence_count,
                    static_live,
                    memory_guard,
                ))?
            } else {
                None
            }
        } else {
            None
        };
        let dual_workspace_live = dual_workspace
            .as_ref()
            .map_or(0, DualProposalWorkspace::retained_bytes);
        let root_dual = if eager_acceleration {
            'root_dual: {
                let Some(workspace) = dual_workspace.as_mut() else {
                    break 'root_dual None;
                };
                let root_phase_requested_peak = (target_words.len() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)
                    .and_then(|bytes| bytes.checked_mul(2))
                    .and_then(|root_covered_bytes| {
                        static_live
                            .checked_add(dual_workspace_live)?
                            .checked_add(root_covered_bytes)?
                            .checked_add(checked_maximum_persistent_dual_certificate_bytes(
                                target_constraint_count,
                            )?)
                    });
                let Some(root_phase_requested_peak) = root_phase_requested_peak else {
                    break 'root_dual None;
                };
                if !optional_dual_preflight(memory_guard(root_phase_requested_peak))? {
                    break 'root_dual None;
                }
                let optional_phase = (|| {
                    let mut root_covered = try_vec_with_capacity(
                        target_words.len(),
                        static_live
                            .checked_add(dual_workspace_live)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        memory_guard,
                        "exact_minimum_cover_root_dual_covered",
                    )?;
                    root_covered.resize(target_words.len(), 0);
                    let root_covered_live = checked_vec_retained_bytes(&root_covered)?;
                    workspace.prepare_root_certificate_with_memory_guard(
                        &support_by_pattern,
                        &target_words,
                        &root_covered,
                        &selected,
                        &excluded_rows,
                        static_live
                            .checked_add(root_covered_live)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        memory_guard,
                    )
                })();
                break 'root_dual optional_dual_acceleration(optional_phase)?.flatten();
            }
        } else {
            None
        };
        let root_dual_live = match root_dual.as_ref() {
            Some(certificate) => certificate
                .checked_retained_bytes()
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            None => 0,
        };
        let pre_traversal_retained_bytes = checked_vec_retained_bytes(&target_words)?
            .checked_add(checked_vec_retained_bytes(&constraint_weights)?)
            .and_then(|bytes| {
                bytes.checked_add(checked_support_retained_bytes(&support_by_pattern).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&support_pattern_order).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&packing_support_marks).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&packing_patterns).ok()?)
            })
            .and_then(|bytes| {
                bytes.checked_add(checked_vec_retained_bytes(&packing_adjusted_degrees).ok()?)
            })
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&excluded_rows).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&selected).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&current).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&best).ok()?))
            .and_then(|bytes| bytes.checked_add(dual_workspace_live))
            .and_then(|bytes| bytes.checked_add(root_dual_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut covered = try_vec_with_capacity(
            target_words.len(),
            base_live_bytes
                .checked_add(pre_traversal_retained_bytes)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_resumable_covered",
        )?;
        covered.resize(target_words.len(), 0);
        let covered_live = checked_vec_retained_bytes(&covered)?;
        let frames = try_vec_with_capacity(
            rows.len(),
            base_live_bytes
                .checked_add(pre_traversal_retained_bytes)
                .and_then(|bytes| bytes.checked_add(covered_live))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_resumable_frames",
        )?;
        let fixed_retained_bytes = pre_traversal_retained_bytes
            .checked_add(covered_live)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&frames).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let search = Self {
            target_words,
            constraint_weights,
            support_by_pattern,
            support_pattern_order,
            packing_support_marks,
            packing_patterns,
            packing_adjusted_degrees,
            packing_generation: 0,
            excluded_rows,
            selected,
            current,
            best,
            goal,
            dual_workspace,
            root_dual,
            cancelled: false,
            fixed_retained_bytes,
            memo_depth: ExactCoveredStateMemo::new(),
            covered,
            frames,
            enter_child: true,
            finished: false,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_conditional_rows: ExactMinimumCoverConditionalRowDiagnostics::default(),
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_conditional_rows_enabled: DIAGNOSTIC_CONDITIONAL_ROW_PRUNING
                .load(std::sync::atomic::Ordering::Relaxed),
            #[cfg(all(test, not(feature = "diagnostic-probes")))]
            diagnostic_conditional_rows_enabled: true,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_search_nodes: 0,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_attempts: 0,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_iterations: 0,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_prunes: 0,
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_attempts_by_dual_gap: [0; 4],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_iterations_by_dual_gap: [0; 4],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_prunes_by_dual_gap: [0; 4],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_attempts_by_depth: [0; 3],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_iterations_by_depth: [0; 3],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_prunes_by_depth: [0; 3],
            #[cfg(any(test, feature = "diagnostic-probes"))]
            diagnostic_residual_prunes_by_checkpoint: [0; 8],
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_hot_cost: ExactMinimumCoverHotCostDiagnostics::default(),
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_residual_admission: ExactMinimumCoverResidualAdmissionPolicy::default(),
        };
        memory_guard(
            base_live_bytes
                .checked_add(search.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(MinimumCoverSearchPreparation::Search(search))
    }

    fn into_solve_decision(
        mut self,
        rows: &[DenseRow],
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<MinimumCoverSolveDecision, ExactMinimumCoverError> {
        if self.cancelled {
            drop(self);
            memory_guard(base_live_bytes)?;
            return Ok(MinimumCoverSolveDecision::Cancelled);
        }
        if matches!(self.goal, ExactCoverSearchGoal::AtMost(_))
            && !self.feasibility_goal_is_satisfied()
        {
            drop(self);
            memory_guard(base_live_bytes)?;
            return Ok(MinimumCoverSolveDecision::ProvedNone);
        }
        self.best
            .sort_unstable_by_key(|index| rows[*index].source_index);
        let best = core::mem::take(&mut self.best);
        drop(self);
        memory_guard(
            base_live_bytes
                .checked_add(checked_vec_retained_bytes(&best)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(MinimumCoverSolveDecision::Found(best))
    }

    fn feasibility_goal_is_satisfied(&self) -> bool {
        matches!(self.goal, ExactCoverSearchGoal::AtMost(limit) if self.best.len() <= limit)
    }

    fn randomized_incumbent_is_applicable(&self, rows: &[DenseRow]) -> bool {
        rows.len() >= 64
            && self
                .target_words
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>()
                >= 128
            && !self.best.is_empty()
    }

    fn cooperative_randomized_trial_budget(&self, rows: &[DenseRow]) -> usize {
        if !matches!(self.goal, ExactCoverSearchGoal::Minimum)
            || !self.randomized_incumbent_is_applicable(rows)
        {
            return 0;
        }
        rows.len()
            .saturating_mul(COOPERATIVE_RANDOMIZED_TRIALS_PER_ROW)
            .clamp(
                MIN_COOPERATIVE_RANDOMIZED_TRIALS,
                MAX_COOPERATIVE_RANDOMIZED_TRIALS,
            )
    }

    fn prepare_root_dual_with_memory_guard(
        &mut self,
        iteration_limit: usize,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<(), ExactMinimumCoverError> {
        if self.root_dual.is_some() {
            return Ok(());
        }
        let Some(workspace_live) = self
            .dual_workspace
            .as_ref()
            .map(DualProposalWorkspace::retained_bytes)
        else {
            return Ok(());
        };
        let search_live = self.checked_heap_retained_bytes()?;
        let external_without_workspace = base_live_bytes
            .checked_add(
                search_live
                    .checked_sub(workspace_live)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let requested_peak = external_without_workspace
            .checked_add(workspace_live)
            .and_then(|bytes| {
                bytes.checked_add(checked_maximum_persistent_dual_certificate_bytes(
                    self.constraint_weights.len(),
                )?)
            });
        let Some(requested_peak) = requested_peak else {
            self.release_dual_workspace()?;
            return Ok(());
        };
        if !optional_dual_preflight(memory_guard(requested_peak))? {
            self.release_dual_workspace()?;
            memory_guard(
                base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )?;
            return Ok(());
        }
        let optional_phase = self
            .dual_workspace
            .as_mut()
            .expect("dual workspace existence was checked")
            .prepare_root_certificate_with_memory_guard_and_iteration_limit(
                &self.support_by_pattern,
                &self.target_words,
                &self.covered,
                &self.selected,
                &self.excluded_rows,
                external_without_workspace,
                iteration_limit,
                memory_guard,
            );
        if let Some(certificate) = optional_dual_acceleration(optional_phase)?.flatten() {
            let certificate_live = certificate
                .checked_retained_bytes()
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            self.fixed_retained_bytes = self
                .fixed_retained_bytes
                .checked_add(certificate_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            self.root_dual = Some(certificate);
        }
        // Keep the reusable proposal workspace after producing the persistent
        // root certificate.  The eager solver does the same: later gap-near
        // nodes use it to derive independently certified residual lower bounds.
        // Dropping it here leaves the proof exact, but expands the no-k search
        // tree by orders of magnitude on the product minimum-cover workload.
        // The cooperative driver limits a workspace-backed call to one node so
        // this atomic optional proposal cannot be multiplied inside one ABI
        // slice.
        memory_guard(
            base_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )
    }

    fn release_dual_workspace(&mut self) -> Result<(), ExactMinimumCoverError> {
        let released_workspace_live = self
            .dual_workspace
            .take()
            .map_or(0, |workspace| workspace.retained_bytes());
        self.fixed_retained_bytes = self
            .fixed_retained_bytes
            .checked_sub(released_workspace_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        Ok(())
    }

    /// Advances the exact DFS by at most `max_nodes` prepared branch nodes.
    ///
    /// The explicit frames preserve the same depth-first branch order,
    /// exclusions, unit propagation, memo state, and incumbent as the former
    /// recursive implementation. A budget stop never unwinds or re-enters a
    /// memoized node, so resuming cannot turn an in-progress subtree into an
    /// invalid memo prune.
    fn advance(
        &mut self,
        rows: &[DenseRow],
        max_nodes: u64,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<MinimumCoverSearchAdvance, ExactMinimumCoverError> {
        if self.finished {
            return Ok(MinimumCoverSearchAdvance::Finished {
                visited_nodes: 0,
                consumed_residual_dual: false,
            });
        }
        if self.cancelled || cancelled() {
            self.cancelled = true;
            return Ok(MinimumCoverSearchAdvance::Cancelled {
                visited_nodes: 0,
                consumed_residual_dual: false,
            });
        }
        if max_nodes == 0 {
            return Ok(MinimumCoverSearchAdvance::Pending {
                visited_nodes: 0,
                consumed_residual_dual: false,
            });
        }
        let mut visited_nodes = 0_u64;
        let mut consumed_residual_dual = false;
        loop {
            if self.cancelled || cancelled() {
                self.cancelled = true;
                return Ok(MinimumCoverSearchAdvance::Cancelled {
                    visited_nodes,
                    consumed_residual_dual,
                });
            }
            if self.feasibility_goal_is_satisfied() {
                self.finished = true;
                return Ok(MinimumCoverSearchAdvance::Finished {
                    visited_nodes,
                    consumed_residual_dual,
                });
            }

            if self.enter_child {
                if visited_nodes >= max_nodes {
                    return Ok(MinimumCoverSearchAdvance::Pending {
                        visited_nodes,
                        consumed_residual_dual,
                    });
                }
                visited_nodes = visited_nodes
                    .checked_add(1)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                #[cfg(any(test, feature = "diagnostic-probes"))]
                {
                    self.diagnostic_search_nodes = self
                        .diagnostic_search_nodes
                        .checked_add(1)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                }
                self.enter_child = false;
                let dual_iterations_before = self
                    .dual_workspace
                    .as_ref()
                    .map_or(0, DualProposalWorkspace::remaining_proposal_iterations);
                let node_entry = self.enter_node(rows, base_live_bytes, memory_guard)?;
                let dual_iterations_after = self
                    .dual_workspace
                    .as_ref()
                    .map_or(0, DualProposalWorkspace::remaining_proposal_iterations);
                consumed_residual_dual |= dual_iterations_after < dual_iterations_before;
                match node_entry {
                    MinimumCoverNodeEntry::Complete => {
                        if self.frames.is_empty() {
                            self.finished = true;
                            return Ok(MinimumCoverSearchAdvance::Finished {
                                visited_nodes,
                                consumed_residual_dual,
                            });
                        }
                    }
                    MinimumCoverNodeEntry::Descend => {}
                }
                if consumed_residual_dual {
                    return Ok(MinimumCoverSearchAdvance::Pending {
                        visited_nodes,
                        consumed_residual_dual,
                    });
                }
                continue;
            }

            let Some(mut frame) = self.frames.pop() else {
                self.finished = true;
                return Ok(MinimumCoverSearchAdvance::Finished {
                    visited_nodes,
                    consumed_residual_dual,
                });
            };

            if let Some(active) = frame.active_branch.take() {
                for (word_index, previous) in active.changed_words {
                    self.covered[word_index] = previous;
                }
                let popped = self.current.pop();
                debug_assert_eq!(popped, Some(active.row_index));
                self.selected[active.row_index] = false;
                if self.feasibility_goal_is_satisfied() {
                    self.complete_frame(frame);
                    self.finished = true;
                    memory_guard(
                        base_live_bytes
                            .checked_add(self.checked_heap_retained_bytes()?)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )?;
                    return Ok(MinimumCoverSearchAdvance::Finished {
                        visited_nodes,
                        consumed_residual_dual,
                    });
                }
                self.set_row_excluded(active.row_index, true);
            }

            if frame.next_branch >= frame.branches.len() {
                self.complete_frame(frame);
                memory_guard(
                    base_live_bytes
                        .checked_add(self.checked_heap_retained_bytes()?)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )?;
                if self.frames.is_empty() {
                    self.finished = true;
                    return Ok(MinimumCoverSearchAdvance::Finished {
                        visited_nodes,
                        consumed_residual_dual,
                    });
                }
                continue;
            }

            let row_index = frame.branches[frame.next_branch];
            frame.next_branch = frame
                .next_branch
                .checked_add(1)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
            let changed_count = rows[row_index]
                .words
                .iter()
                .copied()
                .zip(self.covered.iter().copied())
                .filter(|(row_word, covered_word)| *covered_word | *row_word != *covered_word)
                .count();
            let frame_live = frame.checked_nested_retained_bytes()?;
            let mut changed_words = try_vec_with_capacity(
                changed_count,
                base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .and_then(|bytes| bytes.checked_add(frame_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                "exact_minimum_cover_changed_words",
            )?;
            self.selected[row_index] = true;
            self.current.push(row_index);
            for (word_index, row_word) in rows[row_index].words.iter().copied().enumerate() {
                let next = self.covered[word_index] | row_word;
                if next != self.covered[word_index] {
                    changed_words.push((word_index, self.covered[word_index]));
                    self.covered[word_index] = next;
                }
            }
            frame.active_branch = Some(MinimumCoverActiveBranch {
                row_index,
                changed_words,
            });
            self.frames.push(frame);
            memory_guard(
                base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )?;
            self.enter_child = true;
        }
    }

    fn enter_node(
        &mut self,
        rows: &[DenseRow],
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<MinimumCoverNodeEntry, ExactMinimumCoverError> {
        let Some((first_pivot, first_support)) = self.rarest_uncovered_pattern(&self.covered)
        else {
            return self.prepare_and_push_frame(
                rows,
                Vec::new(),
                Vec::new(),
                base_live_bytes,
                memory_guard,
            );
        };
        if first_support == 0 {
            return Ok(MinimumCoverNodeEntry::Complete);
        }
        if first_support > 1 {
            return self.prepare_and_push_frame(
                rows,
                Vec::new(),
                Vec::new(),
                base_live_bytes,
                memory_guard,
            );
        }

        let search_live = base_live_bytes
            .checked_add(self.checked_heap_retained_bytes()?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut saved_covered = try_vec_with_capacity(
            self.covered.len(),
            search_live,
            memory_guard,
            "exact_minimum_cover_unit_saved_covered",
        )?;
        saved_covered.extend_from_slice(&self.covered);
        let saved_live = checked_vec_retained_bytes(&saved_covered)?;
        let mut forced_rows = try_vec_with_capacity(
            rows.len(),
            search_live
                .checked_add(saved_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_unit_forced_rows",
        )?;

        let mut next = Some((first_pivot, first_support));
        let mut dead = false;
        while let Some((pivot, support)) = next {
            if support == 0 {
                dead = true;
                break;
            }
            if support > 1 {
                break;
            }
            let row_index = self.support_by_pattern[pivot]
                .iter()
                .copied()
                .find(|row_index| !self.selected[*row_index] && !self.row_is_excluded(*row_index))
                .expect("unit support count identifies one available row");
            self.selected[row_index] = true;
            self.current.push(row_index);
            forced_rows.push(row_index);
            union_words(&mut self.covered, &rows[row_index].words);
            if is_superset(&self.covered, &self.target_words) {
                next = None;
            } else {
                next = self.rarest_uncovered_pattern(&self.covered);
            }
        }

        if dead {
            self.restore_unit_rows(&saved_covered, &forced_rows);
            return Ok(MinimumCoverNodeEntry::Complete);
        }
        self.prepare_and_push_frame(
            rows,
            saved_covered,
            forced_rows,
            base_live_bytes,
            memory_guard,
        )
    }

    fn prepare_and_push_frame(
        &mut self,
        rows: &[DenseRow],
        saved_covered: Vec<u64>,
        forced_rows: Vec<usize>,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<MinimumCoverNodeEntry, ExactMinimumCoverError> {
        let node_scratch_bytes = checked_vec_retained_bytes(&saved_covered)?
            .checked_add(checked_vec_retained_bytes(&forced_rows)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        // `prepare_reduced_node` mutates reusable bound workspaces but treats
        // coverage as immutable. Move the owned coverage out briefly to keep
        // that aliasing explicit without cloning the full pattern vector at
        // every DFS node.
        let covered = core::mem::take(&mut self.covered);
        let prepared = self.prepare_reduced_node(
            rows,
            &covered,
            base_live_bytes,
            node_scratch_bytes,
            memory_guard,
        );
        self.covered = covered;
        let Some(branches) = prepared? else {
            self.restore_unit_rows(&saved_covered, &forced_rows);
            return Ok(MinimumCoverNodeEntry::Complete);
        };
        self.frames.push(MinimumCoverSearchFrame {
            saved_covered,
            forced_rows,
            branches,
            next_branch: 0,
            active_branch: None,
        });
        memory_guard(
            base_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(MinimumCoverNodeEntry::Descend)
    }

    fn restore_unit_rows(&mut self, saved_covered: &[u64], forced_rows: &[usize]) {
        if !saved_covered.is_empty() {
            self.covered.copy_from_slice(saved_covered);
        }
        for row_index in forced_rows.iter().copied().rev() {
            let popped = self.current.pop();
            debug_assert_eq!(popped, Some(row_index));
            self.selected[row_index] = false;
        }
    }

    fn complete_frame(&mut self, frame: MinimumCoverSearchFrame) {
        for row_index in frame.branches.iter().copied() {
            self.set_row_excluded(row_index, false);
        }
        self.restore_unit_rows(&frame.saved_covered, &frame.forced_rows);
    }

    /// Performs the allocation-heavy bound and pivot preparation in a frame
    /// that is gone before recursive descent. Keeping only the returned branch
    /// vector in `search_reduced` materially bounds native and WASM stack use
    /// without changing the deterministic search tree or heap authority.
    fn prepare_reduced_node(
        &mut self,
        rows: &[DenseRow],
        covered: &[u64],
        base_live_bytes: u128,
        recursive_scratch_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Option<Vec<usize>>, ExactMinimumCoverError> {
        if self.feasibility_goal_is_satisfied() {
            return Ok(None);
        }
        if is_superset(covered, &self.target_words) {
            if self.best.is_empty() || self.current.len() < self.best.len() {
                self.best.clone_from(&self.current);
            }
            return Ok(None);
        }
        if !self.best.is_empty() && self.current.len() >= self.best.len() {
            return Ok(None);
        }
        let memo_external_live = base_live_bytes
            .checked_add(self.fixed_retained_bytes)
            .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        if self.excluded_rows.iter().all(|word| *word == 0) {
            #[cfg(feature = "diagnostic-probes")]
            let memo_started = Instant::now();
            let memo_pruned = self.memo_depth.should_prune_or_record(
                covered,
                self.current.len(),
                memo_external_live,
                memory_guard,
            )?;
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.memo_calls =
                    self.diagnostic_hot_cost.memo_calls.saturating_add(1);
                self.diagnostic_hot_cost.memo_nanoseconds = self
                    .diagnostic_hot_cost
                    .memo_nanoseconds
                    .saturating_add(memo_started.elapsed().as_nanos());
            }
            if memo_pruned {
                return Ok(None);
            }
        }

        #[cfg(feature = "diagnostic-probes")]
        let rarest_started = Instant::now();
        let rarest = self.rarest_uncovered_pattern(covered);
        #[cfg(feature = "diagnostic-probes")]
        {
            self.diagnostic_hot_cost.rarest_support_calls = self
                .diagnostic_hot_cost
                .rarest_support_calls
                .saturating_add(1);
            self.diagnostic_hot_cost.rarest_support_nanoseconds = self
                .diagnostic_hot_cost
                .rarest_support_nanoseconds
                .saturating_add(rarest_started.elapsed().as_nanos());
        }
        let Some((pivot, support)) = rarest else {
            return Ok(None);
        };
        if support == 0 {
            return Ok(None);
        }
        #[cfg(feature = "diagnostic-probes")]
        let top_gain_started = Instant::now();
        let uncovered_weight = uncovered_unit_count(covered, &self.target_words);
        let maximum_rows_to_improve = self
            .best
            .len()
            .checked_sub(self.current.len())
            .and_then(|remaining| remaining.checked_sub(1));
        if maximum_rows_to_improve == Some(0) {
            #[cfg(feature = "diagnostic-probes")]
            self.record_top_gain_hot_cost(top_gain_started);
            return Ok(None);
        }
        let top_gain_slots = maximum_rows_to_improve
            .unwrap_or(MAX_TOP_GAIN_BOUND_SLOTS + 1)
            .min(MAX_TOP_GAIN_BOUND_SLOTS);
        let mut largest_gains = [0_usize; MAX_TOP_GAIN_BOUND_SLOTS];
        let mut max_gain = 0_usize;
        for (row_index, row) in rows.iter().enumerate() {
            if self.selected[row_index] || self.row_is_excluded(row_index) {
                continue;
            }
            let gain = uncovered_unit_gain(&row.words, covered, &self.target_words);
            max_gain = max_gain.max(gain);
            if top_gain_slots == 0 || gain <= largest_gains[top_gain_slots - 1] {
                continue;
            }
            insert_descending_top_gain(&mut largest_gains[..top_gain_slots], gain);
        }
        if max_gain == 0 {
            #[cfg(feature = "diagnostic-probes")]
            self.record_top_gain_hot_cost(top_gain_started);
            return Ok(None);
        }
        let lower_bound = uncovered_weight.div_ceil(max_gain);
        if !self.best.is_empty() && self.current.len() + lower_bound >= self.best.len() {
            #[cfg(feature = "diagnostic-probes")]
            self.record_top_gain_hot_cost(top_gain_started);
            return Ok(None);
        }
        let mut top_gain_lower_bound = lower_bound;
        if let Some(row_limit) = maximum_rows_to_improve {
            if row_limit <= MAX_TOP_GAIN_BOUND_SLOTS {
                let mut optimistic_coverage = 0_usize;
                let Some(required_rows) = largest_gains[..row_limit]
                    .iter()
                    .position(|gain| {
                        optimistic_coverage = optimistic_coverage.saturating_add(*gain);
                        optimistic_coverage >= uncovered_weight
                    })
                    .map(|index| index + 1)
                else {
                    #[cfg(feature = "diagnostic-probes")]
                    self.record_top_gain_hot_cost(top_gain_started);
                    return Ok(None);
                };
                top_gain_lower_bound = top_gain_lower_bound.max(required_rows);
            }
        }
        #[cfg(feature = "diagnostic-probes")]
        self.record_top_gain_hot_cost(top_gain_started);
        // The covered/target words and root certificate do not change during
        // this node. Carry only the necessary row weight through the existing
        // optional bounds; never apply it after a DFS state transition.
        let mut minimum_root_row_weight = None;
        if let Some(row_limit) = maximum_rows_to_improve {
            #[cfg(feature = "diagnostic-probes")]
            let root_certificate_started = Instant::now();
            let root_assessment = self.root_dual.as_ref().and_then(|certificate| {
                certificate.certified_bound_and_row_requirement(
                    &self.target_words,
                    covered,
                    row_limit,
                )
            });
            let root_dual_lower_bound = root_assessment.map_or(0, |(bound, _)| bound);
            minimum_root_row_weight = root_assessment.and_then(|(_, requirement)| requirement);
            #[cfg(any(test, feature = "diagnostic-probes"))]
            if !self.diagnostic_conditional_rows_enabled {
                minimum_root_row_weight = None;
            }
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.root_certificate_calls = self
                    .diagnostic_hot_cost
                    .root_certificate_calls
                    .saturating_add(1);
                self.diagnostic_hot_cost.root_certificate_nanoseconds = self
                    .diagnostic_hot_cost
                    .root_certificate_nanoseconds
                    .saturating_add(root_certificate_started.elapsed().as_nanos());
            }
            if root_dual_lower_bound > row_limit {
                return Ok(None);
            }
            #[cfg(feature = "diagnostic-probes")]
            let packing_started = Instant::now();
            let packing_exceeds =
                self.sum_over_disjoint_support_packing_exceeds(covered, row_limit);
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.packing_calls =
                    self.diagnostic_hot_cost.packing_calls.saturating_add(1);
                self.diagnostic_hot_cost.packing_nanoseconds = self
                    .diagnostic_hot_cost
                    .packing_nanoseconds
                    .saturating_add(packing_started.elapsed().as_nanos());
            }
            if packing_exceeds {
                return Ok(None);
            }
            let packing_lower_bound = self.packing_patterns.len();
            let cheap_lower_bound = top_gain_lower_bound
                .max(packing_lower_bound)
                .max(root_dual_lower_bound);
            let dual_gap = row_limit
                .saturating_add(1)
                .saturating_sub(cheap_lower_bound);
            #[cfg(feature = "diagnostic-probes")]
            let diagnostic_admits_residual = self
                .diagnostic_residual_admission
                .admits(dual_gap, self.current.len());
            #[cfg(not(feature = "diagnostic-probes"))]
            let diagnostic_admits_residual = true;
            if dual_gap <= 4
                && diagnostic_admits_residual
                && should_attempt_residual_dual(
                    rows.len(),
                    self.constraint_weights.len(),
                    self.current.len(),
                    row_limit,
                )
            {
                let residual_outcome = self.dual_workspace.as_mut().and_then(|workspace| {
                    let iterations_before = workspace.remaining_proposal_iterations();
                    #[cfg(feature = "diagnostic-probes")]
                    let bound = workspace
                        .diagnostic_certified_residual_lower_bound_with_iteration_limit(
                            &self.support_by_pattern,
                            &self.target_words,
                            covered,
                            &self.selected,
                            &self.excluded_rows,
                            row_limit,
                            self.diagnostic_residual_admission
                                .maximum_iterations_per_attempt,
                        );
                    #[cfg(not(feature = "diagnostic-probes"))]
                    let bound = workspace.certified_residual_lower_bound(
                        &self.support_by_pattern,
                        &self.target_words,
                        covered,
                        &self.selected,
                        &self.excluded_rows,
                        row_limit,
                    );
                    let iterations_after = workspace.remaining_proposal_iterations();
                    Some((bound, iterations_before.saturating_sub(iterations_after)))
                });
                if let Some((bound, _consumed_iterations)) = residual_outcome {
                    let pruned = bound.is_some_and(|dual_lower_bound| dual_lower_bound > row_limit);
                    #[cfg(any(test, feature = "diagnostic-probes"))]
                    {
                        let gap_bucket = dual_gap
                            .checked_sub(1)
                            .filter(|bucket| *bucket < 4)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                        let depth_bucket = match self.current.len() {
                            0..=9 => 0,
                            10..=13 => 1,
                            _ => 2,
                        };
                        let consumed_iterations = u64::try_from(_consumed_iterations)
                            .map_err(|_| ExactMinimumCoverError::ProjectionOverflow)?;
                        let prune_count = u64::from(pruned);
                        checked_add_diagnostic_counter(&mut self.diagnostic_residual_attempts, 1)?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_iterations,
                            consumed_iterations,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_prunes,
                            prune_count,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_attempts_by_dual_gap[gap_bucket],
                            1,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_iterations_by_dual_gap[gap_bucket],
                            consumed_iterations,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_prunes_by_dual_gap[gap_bucket],
                            prune_count,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_attempts_by_depth[depth_bucket],
                            1,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_iterations_by_depth[depth_bucket],
                            consumed_iterations,
                        )?;
                        checked_add_diagnostic_counter(
                            &mut self.diagnostic_residual_prunes_by_depth[depth_bucket],
                            prune_count,
                        )?;
                        if pruned && consumed_iterations != 0 {
                            let checkpoint_bucket =
                                usize::try_from(consumed_iterations.saturating_sub(1) / 25)
                                    .map_err(|_| ExactMinimumCoverError::ProjectionOverflow)?
                                    .min(self.diagnostic_residual_prunes_by_checkpoint.len() - 1);
                            checked_add_diagnostic_counter(
                                &mut self.diagnostic_residual_prunes_by_checkpoint
                                    [checkpoint_bucket],
                                1,
                            )?;
                        }
                    }
                    if pruned {
                        return Ok(None);
                    }
                }
            }
        }

        #[cfg(feature = "diagnostic-probes")]
        let branch_started = Instant::now();
        let branch_count = self.support_by_pattern[pivot]
            .iter()
            .filter(|index| !self.selected[**index] && !self.row_is_excluded(**index))
            .count();
        let mut branches = try_vec_with_capacity(
            branch_count,
            base_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_branches",
        )?;
        #[cfg(any(test, feature = "diagnostic-probes"))]
        if minimum_root_row_weight.is_some() {
            self.diagnostic_conditional_rows.assessed_nodes = self
                .diagnostic_conditional_rows
                .assessed_nodes
                .saturating_add(1);
        }
        // Reserve the unchanged eligible-branch upper bound, then examine each
        // pivot row once. Conditional removal is local to this branch list:
        // do not mutate excluded_rows, whose undo trail belongs to the frame.
        for &row_index in &self.support_by_pattern[pivot] {
            if self.selected[row_index] || self.row_is_excluded(row_index) {
                continue;
            }
            let conditional = match (self.root_dual.as_ref(), minimum_root_row_weight) {
                (Some(certificate), Some(minimum)) => certificate.conditional_row_prune(
                    &self.target_words,
                    covered,
                    &rows[row_index].words,
                    minimum,
                ),
                _ => None,
            };
            #[cfg(any(test, feature = "diagnostic-probes"))]
            if minimum_root_row_weight.is_some() {
                self.diagnostic_conditional_rows.candidate_rows = self
                    .diagnostic_conditional_rows
                    .candidate_rows
                    .saturating_add(1);
                if let Some((pruned, examined)) = conditional {
                    self.diagnostic_conditional_rows.examined_weights = self
                        .diagnostic_conditional_rows
                        .examined_weights
                        .saturating_add(examined as u64);
                    self.diagnostic_conditional_rows.pruned_rows = self
                        .diagnostic_conditional_rows
                        .pruned_rows
                        .saturating_add(u64::from(pruned));
                }
            }
            if !conditional.is_some_and(|(pruned, _)| pruned) {
                branches.push(row_index);
            }
        }
        remove_residual_dominated_pivot_rows(&mut branches, rows, covered, &self.target_words);
        for row_index in branches.iter().copied() {
            self.packing_adjusted_degrees[row_index] = uncovered_gain(
                &rows[row_index].words,
                covered,
                &self.target_words,
                &self.constraint_weights,
            );
        }
        branches.sort_unstable_by(|left, right| {
            self.packing_adjusted_degrees[*right]
                .cmp(&self.packing_adjusted_degrees[*left])
                .then_with(|| rows[*left].source_index.cmp(&rows[*right].source_index))
        });
        #[cfg(feature = "diagnostic-probes")]
        {
            self.diagnostic_hot_cost.branch_calls =
                self.diagnostic_hot_cost.branch_calls.saturating_add(1);
            self.diagnostic_hot_cost.branch_nanoseconds = self
                .diagnostic_hot_cost
                .branch_nanoseconds
                .saturating_add(branch_started.elapsed().as_nanos());
        }
        Ok(Some(branches))
    }

    #[cfg(feature = "diagnostic-probes")]
    fn record_top_gain_hot_cost(&mut self, started: Instant) {
        self.diagnostic_hot_cost.top_gain_calls =
            self.diagnostic_hot_cost.top_gain_calls.saturating_add(1);
        self.diagnostic_hot_cost.top_gain_nanoseconds = self
            .diagnostic_hot_cost
            .top_gain_nanoseconds
            .saturating_add(started.elapsed().as_nanos());
    }

    fn checked_heap_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        self.fixed_retained_bytes
            .checked_add(self.memo_depth.checked_heap_retained_bytes()?)
            .and_then(|bytes| {
                self.frames.iter().try_fold(bytes, |bytes, frame| {
                    bytes.checked_add(frame.checked_nested_retained_bytes().ok()?)
                })
            })
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }

    fn rarest_uncovered_pattern(&self, covered: &[u64]) -> Option<(usize, usize)> {
        let mut rarest = None;
        for (word_index, (target, covered)) in self.target_words.iter().zip(covered).enumerate() {
            let mut remaining = target & !covered;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                let support = self.support_by_pattern[pattern]
                    .iter()
                    .filter(|row_index| {
                        !self.selected[**row_index] && !self.row_is_excluded(**row_index)
                    })
                    // Only a strictly smaller support changes the pivot. Once
                    // the incumbent count is reached, scanning more eligible
                    // rows cannot affect either the pivot or its exact count.
                    .take(rarest.map_or(usize::MAX, |(_, current)| current))
                    .count();
                if rarest.is_none_or(|(_, current)| support < current) {
                    rarest = Some((pattern, support));
                    if support <= 1 {
                        return rarest;
                    }
                }
                remaining &= remaining - 1;
            }
        }
        rarest
    }

    fn sum_over_disjoint_support_packing_exceeds(
        &mut self,
        covered: &[u64],
        row_limit: usize,
    ) -> bool {
        self.packing_generation = self.packing_generation.wrapping_add(1);
        if self.packing_generation == 0 {
            self.packing_support_marks.fill(0);
            self.packing_generation = 1;
        }
        let generation = self.packing_generation;
        self.packing_patterns.clear();
        for order_index in 0..self.support_pattern_order.len() {
            let pattern = self.support_pattern_order[order_index];
            let word_index = pattern / u64::BITS as usize;
            let bit = pattern % u64::BITS as usize;
            if self.target_words[word_index] & !covered[word_index] & (1_u64 << bit) == 0 {
                continue;
            }
            let support = &self.support_by_pattern[pattern];
            if support
                .iter()
                .filter(|row_index| {
                    !self.selected[**row_index] && !self.row_is_excluded(**row_index)
                })
                .any(|row_index| self.packing_support_marks[*row_index] == generation)
            {
                continue;
            }
            self.packing_patterns.push(pattern);
            if self.packing_patterns.len() > row_limit {
                return true;
            }
            for row_index in support {
                if self.selected[*row_index] || self.row_is_excluded(*row_index) {
                    continue;
                }
                self.packing_support_marks[*row_index] = generation;
            }
        }

        self.packing_adjusted_degrees.fill(0);
        let mut uncovered_constraint_count = 0_usize;
        for pattern in 0..self.constraint_weights.len() {
            let word_index = pattern / u64::BITS as usize;
            let bit = pattern % u64::BITS as usize;
            if self.target_words[word_index] & !covered[word_index] & (1_u64 << bit) == 0 {
                continue;
            }
            uncovered_constraint_count += 1;
            for row_index in &self.support_by_pattern[pattern] {
                if !self.selected[*row_index] && !self.row_is_excluded(*row_index) {
                    self.packing_adjusted_degrees[*row_index] += 1;
                }
            }
        }

        let mut optimistically_covered_constraints = 0_usize;
        for pattern in self.packing_patterns.iter().copied() {
            let support = &self.support_by_pattern[pattern];
            let Some(max_degree_row) = support
                .iter()
                .copied()
                .filter(|row_index| !self.selected[*row_index] && !self.row_is_excluded(*row_index))
                .max_by_key(|row_index| self.packing_adjusted_degrees[*row_index])
            else {
                return true;
            };
            optimistically_covered_constraints += self.packing_adjusted_degrees[max_degree_row];
            for row_index in support.iter().copied() {
                if self.selected[row_index] || self.row_is_excluded(row_index) {
                    continue;
                }
                self.packing_adjusted_degrees[row_index] -= 1;
            }
            self.packing_adjusted_degrees[max_degree_row] = 0;
        }

        self.packing_adjusted_degrees.sort_unstable();
        let mut additional_rows = 0_usize;
        for degree in self.packing_adjusted_degrees.iter().rev().copied() {
            if optimistically_covered_constraints >= uncovered_constraint_count {
                break;
            }
            if degree == 0 {
                break;
            }
            optimistically_covered_constraints += degree;
            additional_rows += 1;
        }
        self.packing_patterns.len() + additional_rows > row_limit
    }

    fn row_is_excluded(&self, row_index: usize) -> bool {
        self.excluded_rows[row_index / u64::BITS as usize]
            & (1_u64 << (row_index % u64::BITS as usize))
            != 0
    }

    fn set_row_excluded(&mut self, row_index: usize, excluded: bool) {
        let word = &mut self.excluded_rows[row_index / u64::BITS as usize];
        let bit = 1_u64 << (row_index % u64::BITS as usize);
        if excluded {
            *word |= bit;
        } else {
            *word &= !bit;
        }
    }
}

impl MinimumCoverSearchFrame {
    /// Heap retained behind this frame. The outer `frames` Vec storage is
    /// charged once in `MinimumCoverSearch::fixed_retained_bytes`.
    fn checked_nested_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        checked_vec_retained_bytes(&self.saved_covered)?
            .checked_add(checked_vec_retained_bytes(&self.forced_rows)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.branches).ok()?))
            .and_then(|bytes| {
                self.active_branch.as_ref().map_or(Some(bytes), |active| {
                    bytes.checked_add(checked_vec_retained_bytes(&active.changed_words).ok()?)
                })
            })
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)
    }
}

const MAX_TOP_GAIN_BOUND_SLOTS: usize = 64;

/// Preserve the same descending top-k multiset as replace-last-and-sort,
/// without sorting the unchanged prefix after every eligible proof row.
fn insert_descending_top_gain(gains: &mut [usize], gain: usize) {
    let Some(mut insertion) = gains.len().checked_sub(1) else {
        return;
    };
    if gain <= gains[insertion] {
        return;
    }
    while insertion > 0 && gain > gains[insertion - 1] {
        gains[insertion] = gains[insertion - 1];
        insertion -= 1;
    }
    gains[insertion] = gain;
}

/// A pivot branch only needs one representative among rows whose currently
/// uncovered contribution is a subset of another available pivot row. Any
/// completion using the smaller row can replace it with the larger row at the
/// same cardinality. Equal residual rows keep the smaller original source
/// identity solely for deterministic proof output; the separate portfolio
/// enumerator remains the authority for every original-row optimum.
fn remove_residual_dominated_pivot_rows(
    branches: &mut Vec<usize>,
    rows: &[DenseRow],
    covered: &[u64],
    target: &[u64],
) {
    let mut left_position = branches.len();
    while left_position > 0 {
        left_position -= 1;
        let left = branches[left_position];
        let dominated = branches
            .iter()
            .copied()
            .enumerate()
            .any(|(right_position, right)| {
                if left_position == right_position
                    || !residual_is_subset(&rows[left].words, &rows[right].words, covered, target)
                {
                    return false;
                }
                !residual_is_subset(&rows[right].words, &rows[left].words, covered, target)
                    || rows[right].source_index < rows[left].source_index
            });
        if dominated {
            branches.remove(left_position);
        }
    }
}

fn residual_is_subset(left: &[u64], right: &[u64], covered: &[u64], target: &[u64]) -> bool {
    left.iter()
        .zip(right)
        .zip(covered)
        .zip(target)
        .all(|(((left, right), covered), target)| {
            let left = left & target & !covered;
            let right = right & target & !covered;
            left & !right == 0
        })
}

fn greedy_cover_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    constraint_weights: &[usize],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<Vec<usize>>, ExactMinimumCoverError> {
    greedy_cover_with_forced_row_memory_guard(
        rows,
        target,
        constraint_weights,
        None,
        base_live_bytes,
        memory_guard,
    )
}

fn greedy_cover_with_forced_row_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    constraint_weights: &[usize],
    forced_row: Option<usize>,
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<Vec<usize>>, ExactMinimumCoverError> {
    let mut covered = try_vec_with_capacity(
        target.len(),
        base_live_bytes,
        memory_guard,
        "exact_minimum_cover_greedy_covered",
    )?;
    covered.resize(target.len(), 0);
    let mut selected = try_vec_with_capacity(
        rows.len(),
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&covered)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_greedy_selected",
    )?;
    selected.resize(rows.len(), false);
    let mut result = try_vec_with_capacity(
        rows.len(),
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&covered)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&selected).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_greedy_result",
    )?;
    if let Some(forced_row) = forced_row {
        let Some(row) = rows.get(forced_row) else {
            return Err(ExactMinimumCoverError::ProjectionOverflow);
        };
        selected[forced_row] = true;
        result.push(forced_row);
        union_words(&mut covered, &row.words);
    }
    while !is_superset(&covered, target) {
        let next = rows
            .iter()
            .enumerate()
            .filter(|(index, _)| !selected[*index])
            .map(|(index, row)| {
                (
                    uncovered_gain(&row.words, &covered, target, constraint_weights),
                    index,
                )
            })
            .filter(|(gain, _)| *gain > 0)
            .max_by(|(left_gain, left), (right_gain, right)| {
                left_gain
                    .cmp(right_gain)
                    .then_with(|| rows[*right].source_index.cmp(&rows[*left].source_index))
            })
            .map(|(_, index)| index);
        let Some(next) = next else {
            return Ok(None);
        };
        selected[next] = true;
        result.push(next);
        union_words(&mut covered, &rows[next].words);
    }
    drop(covered);
    drop(selected);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&result)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(Some(result))
}

const RANDOMIZED_COMPACT_COVER_TRIALS: usize = 20_000;
// Cooperative sessions scale optional incumbent work with the reduced row
// matrix rather than imposing the blocking wrapper's fixed 20k trials. Exact
// DFS remains the sole proof authority; these bounds change only runtime.
const COOPERATIVE_RANDOMIZED_TRIALS_PER_ROW: usize = 20;
const MIN_COOPERATIVE_RANDOMIZED_TRIALS: usize = 256;
const MAX_COOPERATIVE_RANDOMIZED_TRIALS: usize = 4_096;
const BREAKOUT_TOTAL_SWAP_BUDGET: usize = 20_000;
const WITNESS_ASSISTED_COMPACT_COVER_TRIALS: usize = 1_000;
pub(super) const WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET: usize = 1_000;
pub(super) const WITNESS_ASSISTED_PREFERRED_TOTAL_BREAKOUT_SWAP_BUDGET: usize = 8_000;
const RANDOMIZED_COMPACT_COVER_SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const WITNESS_ASSISTED_RANDOM_SEED: u64 = 1;

fn preferred_witness_breakout_budget(supporter_count: usize, supporter_position: usize) -> usize {
    if supporter_count == 0 || supporter_position >= supporter_count {
        return 0;
    }
    let even_share = WITNESS_ASSISTED_PREFERRED_TOTAL_BREAKOUT_SWAP_BUDGET / supporter_count;
    let remainder = WITNESS_ASSISTED_PREFERRED_TOTAL_BREAKOUT_SWAP_BUDGET % supporter_count;
    (even_share + usize::from(supporter_position < remainder))
        .min(WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IncumbentSearchOutcome {
    Completed,
    FoundAtMost,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
struct BreakoutSwapCandidate {
    net_gain: i128,
    make_gain: usize,
    break_loss: usize,
    add_row: usize,
    remove_row: usize,
}

#[inline]
fn breakout_word_make_gain(add: &[u64], uncovered: &[u64]) -> usize {
    add.iter()
        .zip(uncovered)
        .fold(0_usize, |gain, (add, uncovered)| {
            gain.saturating_add((add & uncovered).count_ones() as usize)
        })
}

#[inline]
fn breakout_word_break_loss(remove: &[u64], add: &[u64], singly_covered: &[u64]) -> usize {
    remove.iter().zip(add).zip(singly_covered).fold(
        0_usize,
        |loss, ((remove, add), singly_covered)| {
            loss.saturating_add((remove & !add & singly_covered).count_ones() as usize)
        },
    )
}

// Keep the former bit-at-a-time scorer only as a diagnostic/parity oracle.
#[cfg(any(test, feature = "diagnostic-probes"))]
fn breakout_reference_make_gain(add: &[u64], target: &[u64], counts: &[usize]) -> usize {
    add.iter()
        .zip(target)
        .enumerate()
        .fold(0_usize, |gain, (word_index, (add, target))| {
            let mut bits = add & target;
            let mut added = 0_usize;
            while bits != 0 {
                let pattern = word_index * u64::BITS as usize + bits.trailing_zeros() as usize;
                added += usize::from(counts[pattern] == 0);
                bits &= bits - 1;
            }
            gain.saturating_add(added)
        })
}

#[cfg(any(test, feature = "diagnostic-probes"))]
fn breakout_reference_break_loss(
    remove: &[u64],
    add: &[u64],
    target: &[u64],
    counts: &[usize],
) -> usize {
    remove.iter().zip(add).zip(target).enumerate().fold(
        0_usize,
        |loss, (word_index, ((remove, add), target))| {
            let mut bits = remove & target & !add;
            let mut removed = 0_usize;
            while bits != 0 {
                let pattern = word_index * u64::BITS as usize + bits.trailing_zeros() as usize;
                removed += usize::from(counts[pattern] == 1);
                bits &= bits - 1;
            }
            loss.saturating_add(removed)
        },
    )
}

/// Searches for a smaller incumbent without contributing any proof authority.
///
/// Every proposal is replayed against the exact compact target before it can
/// replace `best`; randomness therefore affects only search order and runtime,
/// never the minimum-cardinality proof or its result identity.
// The search controls stay explicit so callers cannot accidentally conflate a
// heuristic budget/seed, protected branch, memory authority, or cancellation.
#[allow(clippy::too_many_arguments)]
fn improve_randomized_compact_cover_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    best: &mut Vec<usize>,
    trial_budget: usize,
    random_seed: u64,
    stop_at_cardinality: Option<usize>,
    forced_row: Option<usize>,
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<IncumbentSearchOutcome, ExactMinimumCoverError> {
    let mut random_state = random_seed;
    improve_randomized_compact_cover_from_state_with_memory_guard(
        rows,
        target,
        best,
        0,
        trial_budget,
        &mut random_state,
        stop_at_cardinality,
        forced_row,
        base_live_bytes,
        memory_guard,
        cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
fn improve_randomized_compact_cover_from_state_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    best: &mut Vec<usize>,
    first_trial: usize,
    trial_budget: usize,
    random_state: &mut u64,
    stop_at_cardinality: Option<usize>,
    forced_row: Option<usize>,
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<IncumbentSearchOutcome, ExactMinimumCoverError> {
    let random_choice = CoverRandomChoice::for_new_session();
    let best_live = checked_vec_retained_bytes(best)?;
    let incumbent_live = base_live_bytes
        .checked_add(best_live)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut covered = try_vec_with_capacity(
        target.len(),
        incumbent_live,
        memory_guard,
        "exact_minimum_cover_randomized_covered",
    )?;
    covered.resize(target.len(), 0);
    let covered_live = checked_vec_retained_bytes(&covered)?;
    let mut replay = try_vec_with_capacity(
        target.len(),
        incumbent_live
            .checked_add(covered_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_randomized_replay",
    )?;
    replay.resize(target.len(), 0);
    let replay_live = checked_vec_retained_bytes(&replay)?;
    let mut selected = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(covered_live)
            .and_then(|bytes| bytes.checked_add(replay_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_randomized_selected",
    )?;
    selected.resize(rows.len(), false);
    let selected_live = checked_vec_retained_bytes(&selected)?;
    let mut proposal = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(covered_live)
            .and_then(|bytes| bytes.checked_add(replay_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_randomized_proposal",
    )?;
    let proposal_live = checked_vec_retained_bytes(&proposal)?;
    let mut candidates = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(covered_live)
            .and_then(|bytes| bytes.checked_add(replay_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .and_then(|bytes| bytes.checked_add(proposal_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_randomized_candidates",
    )?;
    let scratch_live = incumbent_live
        .checked_add(covered_live)
        .and_then(|bytes| bytes.checked_add(replay_live))
        .and_then(|bytes| bytes.checked_add(selected_live))
        .and_then(|bytes| bytes.checked_add(proposal_live))
        .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&candidates).ok()?))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(scratch_live)?;

    let mut outcome = IncumbentSearchOutcome::Completed;
    'trials: for trial_offset in 0..trial_budget {
        let trial = first_trial
            .checked_add(trial_offset)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        if cancelled() {
            outcome = IncumbentSearchOutcome::Cancelled;
            break;
        }
        covered.fill(0);
        replay.fill(0);
        selected.fill(false);
        proposal.clear();

        if let Some(forced_row) = forced_row {
            let Some(row) = rows.get(forced_row) else {
                return Err(ExactMinimumCoverError::ProjectionOverflow);
            };
            selected[forced_row] = true;
            proposal.push(forced_row);
            union_words(&mut covered, &row.words);
        }

        while !is_superset(&covered, target) {
            if cancelled() {
                outcome = IncumbentSearchOutcome::Cancelled;
                break 'trials;
            }
            candidates.clear();
            for (row_index, row) in rows.iter().enumerate() {
                if selected[row_index] {
                    continue;
                }
                let gain = row.words.iter().zip(&covered).zip(target).fold(
                    0_usize,
                    |gain, ((row, covered), target)| {
                        gain.saturating_add((row & target & !covered).count_ones() as usize)
                    },
                );
                if gain > 0 {
                    candidates.push((gain, row_index));
                }
            }
            if candidates.is_empty() {
                break;
            }
            candidates.sort_unstable_by(|(left_gain, left), (right_gain, right)| {
                right_gain
                    .cmp(left_gain)
                    .then_with(|| rows[*left].source_index.cmp(&rows[*right].source_index))
            });
            *random_state ^= *random_state << 13;
            *random_state ^= *random_state >> 7;
            *random_state ^= *random_state << 17;
            let restricted_candidate_count = (2 + trial % 7).min(candidates.len());
            let choice = random_choice.index(*random_state, restricted_candidate_count);
            let row_index = candidates[choice].1;
            selected[row_index] = true;
            proposal.push(row_index);
            union_words(&mut covered, &rows[row_index].words);
        }
        if !is_superset(&covered, target) {
            continue;
        }

        for position in (0..proposal.len()).rev() {
            if forced_row.is_some_and(|forced_row| proposal[position] == forced_row) {
                continue;
            }
            replay.fill(0);
            for (other_position, row_index) in proposal.iter().copied().enumerate() {
                if other_position != position {
                    union_words(&mut replay, &rows[row_index].words);
                }
            }
            if is_superset(&replay, target) {
                let removed = proposal.remove(position);
                selected[removed] = false;
                covered.copy_from_slice(&replay);
            }
        }

        if proposal.len() >= best.len() {
            continue;
        }
        replay.fill(0);
        for row_index in proposal.iter().copied() {
            union_words(&mut replay, &rows[row_index].words);
        }
        if is_superset(&replay, target) {
            best.clear();
            best.extend_from_slice(&proposal);
            memory_guard(scratch_live)?;
            if stop_at_cardinality.is_some_and(|limit| best.len() <= limit) {
                outcome = IncumbentSearchOutcome::FoundAtMost;
                break;
            }
        }
    }

    drop(candidates);
    drop(proposal);
    drop(selected);
    drop(replay);
    drop(covered);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(best)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(outcome)
}

/// Attempts to break a validated incumbent from cardinality `L` to `L - 1`.
///
/// This is a deterministic, globally bounded local search. It never proves a
/// lower bound: a proposal becomes an incumbent only after an exact replay of
/// its row union. The exact branch-and-bound search remains solely responsible
/// for proving optimality.
#[allow(clippy::too_many_arguments)]
fn improve_fixed_cardinality_cover_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    support_by_pattern: &[Vec<usize>],
    best: &mut Vec<usize>,
    total_swap_budget: usize,
    protected_row: Option<usize>,
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, ExactMinimumCoverError> {
    let random_choice = CoverRandomChoice::for_new_session();
    if best.len() <= 1 || total_swap_budget == 0 {
        return Ok(false);
    }
    if protected_row.is_some_and(|protected| !best.contains(&protected)) {
        return Err(ExactMinimumCoverError::ProjectionOverflow);
    }
    let best_live = checked_vec_retained_bytes(best)?;
    let incumbent_live = base_live_bytes
        .checked_add(best_live)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let mut replay = try_vec_with_capacity(
        target.len(),
        incumbent_live,
        memory_guard,
        "exact_minimum_cover_breakout_replay",
    )?;
    replay.resize(target.len(), 0);
    let replay_live = checked_vec_retained_bytes(&replay)?;
    let mut coverage_counts = try_vec_with_capacity(
        support_by_pattern.len(),
        incumbent_live
            .checked_add(replay_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_coverage_counts",
    )?;
    coverage_counts.resize(support_by_pattern.len(), 0_usize);
    let counts_live = checked_vec_retained_bytes(&coverage_counts)?;
    let mut selected = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(replay_live)
            .and_then(|bytes| bytes.checked_add(counts_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_selected",
    )?;
    selected.resize(rows.len(), false);
    let selected_live = checked_vec_retained_bytes(&selected)?;
    let mut proposal = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(replay_live)
            .and_then(|bytes| bytes.checked_add(counts_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_proposal",
    )?;
    let proposal_live = checked_vec_retained_bytes(&proposal)?;
    let mut pivot_candidates = try_vec_with_capacity(
        support_by_pattern.len(),
        incumbent_live
            .checked_add(replay_live)
            .and_then(|bytes| bytes.checked_add(counts_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .and_then(|bytes| bytes.checked_add(proposal_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_pivots",
    )?;
    let pivots_live = checked_vec_retained_bytes(&pivot_candidates)?;
    let mut remove_candidates = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(replay_live)
            .and_then(|bytes| bytes.checked_add(counts_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .and_then(|bytes| bytes.checked_add(proposal_live))
            .and_then(|bytes| bytes.checked_add(pivots_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_remove_candidates",
    )?;
    let removes_live = checked_vec_retained_bytes(&remove_candidates)?;
    let mut swap_candidates = try_vec_with_capacity(
        rows.len(),
        incumbent_live
            .checked_add(replay_live)
            .and_then(|bytes| bytes.checked_add(counts_live))
            .and_then(|bytes| bytes.checked_add(selected_live))
            .and_then(|bytes| bytes.checked_add(proposal_live))
            .and_then(|bytes| bytes.checked_add(pivots_live))
            .and_then(|bytes| bytes.checked_add(removes_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_breakout_swaps",
    )?;
    let scratch_live = incumbent_live
        .checked_add(replay_live)
        .and_then(|bytes| bytes.checked_add(counts_live))
        .and_then(|bytes| bytes.checked_add(selected_live))
        .and_then(|bytes| bytes.checked_add(proposal_live))
        .and_then(|bytes| bytes.checked_add(pivots_live))
        .and_then(|bytes| bytes.checked_add(removes_live))
        .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&swap_candidates).ok()?))
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(scratch_live)?;

    let seed_cardinality = best.len();
    let target_cardinality = seed_cardinality - 1;
    let restart_count = seed_cardinality;
    let swaps_per_restart = total_swap_budget.div_ceil(restart_count);
    let mut random_state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut was_cancelled = false;
    let mut attempted_swaps = 0_usize;

    'restarts: for restart in 0..restart_count {
        if cancelled() {
            was_cancelled = true;
            break;
        }
        coverage_counts.fill(0);
        selected.fill(false);
        proposal.clear();
        let dropped_position = restart % seed_cardinality;
        if protected_row.is_some_and(|protected| best[dropped_position] == protected) {
            continue;
        }
        for (position, row_index) in best.iter().copied().enumerate() {
            if position == dropped_position {
                continue;
            }
            selected[row_index] = true;
            proposal.push(row_index);
            for (word_index, (row_word, target_word)) in
                rows[row_index].words.iter().zip(target).enumerate()
            {
                let mut bits = row_word & target_word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    coverage_counts[pattern] += 1;
                    bits &= bits - 1;
                }
            }
        }
        debug_assert_eq!(proposal.len(), target_cardinality);

        for iteration in 0..swaps_per_restart {
            if attempted_swaps == total_swap_budget {
                break 'restarts;
            }
            if cancelled() {
                was_cancelled = true;
                break 'restarts;
            }
            attempted_swaps += 1;
            pivot_candidates.clear();
            let arbitrary_pivot = iteration % 20 == 0;
            let mut minimum_support = usize::MAX;
            for (word_index, target_word) in target.iter().copied().enumerate() {
                let mut bits = target_word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    bits &= bits - 1;
                    if coverage_counts[pattern] != 0 {
                        continue;
                    }
                    if arbitrary_pivot {
                        pivot_candidates.push(pattern);
                        continue;
                    }
                    let support_count = support_by_pattern[pattern].len();
                    match support_count.cmp(&minimum_support) {
                        core::cmp::Ordering::Less => {
                            minimum_support = support_count;
                            pivot_candidates.clear();
                            pivot_candidates.push(pattern);
                        }
                        core::cmp::Ordering::Equal => pivot_candidates.push(pattern),
                        core::cmp::Ordering::Greater => {}
                    }
                }
            }
            if pivot_candidates.is_empty() {
                replay.fill(0);
                for row_index in proposal.iter().copied() {
                    union_words(&mut replay, &rows[row_index].words);
                }
                if proposal.len() == target_cardinality && is_superset(&replay, target) {
                    if cancelled() {
                        was_cancelled = true;
                        break 'restarts;
                    }
                    best.clear();
                    best.extend_from_slice(&proposal);
                    memory_guard(scratch_live)?;
                    drop(swap_candidates);
                    drop(remove_candidates);
                    drop(pivot_candidates);
                    drop(proposal);
                    drop(selected);
                    drop(coverage_counts);
                    drop(replay);
                    memory_guard(
                        base_live_bytes
                            .checked_add(checked_vec_retained_bytes(best)?)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )?;
                    return Ok(false);
                }
                break;
            }

            let pivot = pivot_candidates[random_choice.index(
                next_breakout_random(&mut random_state),
                pivot_candidates.len(),
            )];
            swap_candidates.clear();
            for add_row in support_by_pattern[pivot]
                .iter()
                .copied()
                .filter(|row_index| !selected[*row_index])
            {
                let make_gain = rows[add_row].words.iter().zip(target).enumerate().fold(
                    0_usize,
                    |gain, (word_index, (row_word, target_word))| {
                        let mut bits = row_word & target_word;
                        let mut added = 0_usize;
                        while bits != 0 {
                            let bit = bits.trailing_zeros() as usize;
                            let pattern = word_index * u64::BITS as usize + bit;
                            added += usize::from(coverage_counts[pattern] == 0);
                            bits &= bits - 1;
                        }
                        gain.saturating_add(added)
                    },
                );
                let mut minimum_break = usize::MAX;
                remove_candidates.clear();
                for remove_row in proposal.iter().copied() {
                    if protected_row.is_some_and(|protected| remove_row == protected) {
                        continue;
                    }
                    let break_loss = rows[remove_row]
                        .words
                        .iter()
                        .zip(&rows[add_row].words)
                        .zip(target)
                        .enumerate()
                        .fold(
                            0_usize,
                            |loss, (word_index, ((remove_word, add_word), target_word))| {
                                let mut bits = remove_word & target_word & !add_word;
                                let mut removed = 0_usize;
                                while bits != 0 {
                                    let bit = bits.trailing_zeros() as usize;
                                    let pattern = word_index * u64::BITS as usize + bit;
                                    removed += usize::from(coverage_counts[pattern] == 1);
                                    bits &= bits - 1;
                                }
                                loss.saturating_add(removed)
                            },
                        );
                    match break_loss.cmp(&minimum_break) {
                        core::cmp::Ordering::Less => {
                            minimum_break = break_loss;
                            remove_candidates.clear();
                            remove_candidates.push(remove_row);
                        }
                        core::cmp::Ordering::Equal => remove_candidates.push(remove_row),
                        core::cmp::Ordering::Greater => {}
                    }
                }
                if remove_candidates.is_empty() {
                    continue;
                }
                let remove_row = remove_candidates[random_choice.index(
                    next_breakout_random(&mut random_state),
                    remove_candidates.len(),
                )];
                swap_candidates.push(BreakoutSwapCandidate {
                    net_gain: make_gain as i128 - minimum_break as i128,
                    make_gain,
                    break_loss: minimum_break,
                    add_row,
                    remove_row,
                });
            }
            if swap_candidates.is_empty() {
                break;
            }
            swap_candidates.sort_unstable_by(|left, right| {
                right
                    .net_gain
                    .cmp(&left.net_gain)
                    .then_with(|| right.make_gain.cmp(&left.make_gain))
                    .then_with(|| left.break_loss.cmp(&right.break_loss))
                    .then_with(|| right.add_row.cmp(&left.add_row))
                    .then_with(|| left.remove_row.cmp(&right.remove_row))
            });
            let perturb = next_breakout_random(&mut random_state) % 100 < 8;
            let choice_count = if perturb {
                swap_candidates.len().min(4)
            } else {
                swap_candidates
                    .iter()
                    .take_while(|candidate| candidate.net_gain == swap_candidates[0].net_gain)
                    .count()
            };
            let choice =
                random_choice.index(next_breakout_random(&mut random_state), choice_count.max(1));
            let selected_swap = swap_candidates[choice];

            selected[selected_swap.remove_row] = false;
            for (word_index, (row_word, target_word)) in rows[selected_swap.remove_row]
                .words
                .iter()
                .zip(target)
                .enumerate()
            {
                let mut bits = row_word & target_word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    coverage_counts[pattern] -= 1;
                    bits &= bits - 1;
                }
            }
            selected[selected_swap.add_row] = true;
            for (word_index, (row_word, target_word)) in rows[selected_swap.add_row]
                .words
                .iter()
                .zip(target)
                .enumerate()
            {
                let mut bits = row_word & target_word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let pattern = word_index * u64::BITS as usize + bit;
                    coverage_counts[pattern] += 1;
                    bits &= bits - 1;
                }
            }
            let remove_position = proposal
                .iter()
                .position(|row_index| *row_index == selected_swap.remove_row)
                .expect("selected breakout row must be present");
            proposal[remove_position] = selected_swap.add_row;
        }
    }

    drop(swap_candidates);
    drop(remove_candidates);
    drop(pivot_candidates);
    drop(proposal);
    drop(selected);
    drop(coverage_counts);
    drop(replay);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(best)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(was_cancelled)
}

fn next_breakout_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn try_vec_with_capacity<T>(
    capacity: usize,
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    component: &'static str,
) -> Result<Vec<T>, ExactMinimumCoverError> {
    let requested_bytes = (capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        live_bytes
            .checked_add(requested_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ExactMinimumCoverError::AllocationFailed { component })?;
    memory_guard(
        live_bytes
            .checked_add(checked_vec_retained_bytes(&values)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(values)
}

fn checked_requested_growth_bytes<T>(
    values: &Vec<T>,
    additional: usize,
) -> Result<u128, ExactMinimumCoverError> {
    let requested_capacity = values
        .len()
        .checked_add(additional)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let additional_capacity = requested_capacity.saturating_sub(values.capacity());
    (additional_capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_vec_retained_bytes<T>(values: &Vec<T>) -> Result<u128, ExactMinimumCoverError> {
    (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_optional_vec_retained_bytes<T>(
    values: &Option<Vec<T>>,
) -> Result<u128, ExactMinimumCoverError> {
    values.as_ref().map_or(Ok(0), checked_vec_retained_bytes)
}

fn checked_exact_compact_target_retained_bytes(
    target: &ExactCompactTarget,
) -> Result<u128, ExactMinimumCoverError> {
    checked_vec_retained_bytes(&target.words)?
        .checked_add(checked_vec_retained_bytes(&target.weights)?)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_lazy_reduction_context_retained_bytes(
    context: &LazyExactCoverReductionContext,
) -> Result<u128, ExactMinimumCoverError> {
    context
        .required
        .checked_storage_retained_bytes()
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?
        .checked_add(checked_pattern_bitset_rows_retained_bytes(
            &context.source_rows,
        )?)
        .and_then(|bytes| {
            bytes.checked_add(checked_dense_rows_retained_bytes(&context.dense_rows).ok()?)
        })
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_pattern_bitset_rows_retained_bytes(
    rows: &Vec<PatternBitSet>,
) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(rows)?;
    for row in rows {
        bytes = bytes
            .checked_add(
                row.checked_storage_retained_bytes()
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn checked_dense_rows_retained_bytes(rows: &Vec<DenseRow>) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(rows)?;
    for row in rows {
        bytes = bytes
            .checked_add(checked_vec_retained_bytes(&row.words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn checked_nested_words_retained_bytes(
    rows: &Vec<Vec<u64>>,
) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(rows)?;
    for words in rows {
        bytes = bytes
            .checked_add(checked_vec_retained_bytes(words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn build_materialization_words_with_memory_guard(
    required: &PatternBitSet,
    source_rows: &[PatternBitSet],
    dense_rows: &[DenseRow],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Vec<Vec<u64>>, ExactMinimumCoverError> {
    let mut result = try_vec_with_capacity(
        dense_rows.len(),
        base_live_bytes,
        memory_guard,
        "exact_minimum_cover_materialization_rows",
    )?;
    for dense_row in dense_rows {
        let source_row = source_rows
            .get(dense_row.source_index)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let result_live = checked_nested_words_retained_bytes(&result)?;
        let mut words = try_vec_with_capacity(
            required.word_count(),
            base_live_bytes
                .checked_add(result_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_materialization_row_words",
        )?;
        for word_index in 0..required.word_count() {
            words.push(source_row.word_at(word_index) & required.word_at(word_index));
        }
        result.push(words);
        memory_guard(
            base_live_bytes
                .checked_add(checked_nested_words_retained_bytes(&result)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
    }
    Ok(result)
}

fn checked_support_retained_bytes(
    support_by_pattern: &Vec<Vec<usize>>,
) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(support_by_pattern)?;
    for support in support_by_pattern {
        bytes = bytes
            .checked_add(checked_vec_retained_bytes(support)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn uncovered_gain(
    row: &[u64],
    covered: &[u64],
    target: &[u64],
    constraint_weights: &[usize],
) -> usize {
    row.iter()
        .zip(covered)
        .zip(target)
        .enumerate()
        .map(|(word_index, ((row, covered), target))| {
            weighted_word_count(
                row & target & !covered,
                word_index * u64::BITS as usize,
                constraint_weights,
            )
        })
        .sum()
}

fn uncovered_unit_gain(row: &[u64], covered: &[u64], target: &[u64]) -> usize {
    row.iter()
        .zip(covered)
        .zip(target)
        .map(|((row, covered), target)| (row & target & !covered).count_ones() as usize)
        .sum()
}

fn uncovered_unit_count(covered: &[u64], target: &[u64]) -> usize {
    covered
        .iter()
        .zip(target)
        .map(|(covered, target)| (target & !covered).count_ones() as usize)
        .sum()
}

fn weighted_word_count(
    mut bits: u64,
    constraint_offset: usize,
    constraint_weights: &[usize],
) -> usize {
    let mut count = 0_usize;
    while bits != 0 {
        let bit = bits.trailing_zeros() as usize;
        count += constraint_weights[constraint_offset + bit];
        bits &= bits - 1;
    }
    count
}

fn union_words(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
}

fn is_superset(covered: &[u64], required: &[u64]) -> bool {
    covered
        .iter()
        .zip(required)
        .all(|(covered, required)| covered & required == *required)
}

#[cfg(test)]
mod tests {
    use crate::pattern::pattern_id::PatternId;

    use super::*;

    #[test]
    fn cover_random_choice_is_fixed_width_and_preserves_native64_remainders() {
        let current = CoverRandomChoice {
            legacy_wasm32: false,
        };
        let legacy = CoverRandomChoice {
            legacy_wasm32: true,
        };
        let mut state = 0x9e37_79b9_7f4a_7c15;
        let first = next_breakout_random(&mut state);
        assert_eq!(first, 0xdc1b_77ae_0bf3_4dad);
        assert_eq!(current.index(first, 25), 14);
        assert_eq!(legacy.index(first, 25), 9);
        let mut saw_width_difference = false;
        for random in [0, 1, u32::MAX as u64, 1_u64 << 32, u64::MAX]
            .into_iter()
            .chain((0..2_000).map(|_| next_breakout_random(&mut state)))
        {
            for count in [1_u32, 2, 3, 5, 7, 25, 63, 64, 65, 1000, 65535, u32::MAX] {
                let expected = random % u64::from(count);
                let actual = current.index(random, count as usize);
                assert_eq!(actual as u64, expected);
                // Simulate the final narrowing explicitly on either test host.
                assert_eq!(u64::from(actual as u32), expected);
                let old = legacy.index(random, count as usize);
                assert_eq!(old as u64, u64::from(random as u32) % u64::from(count));
                assert!(actual < count as usize && old < count as usize);
                saw_width_difference |= actual != old;
            }
            #[cfg(target_pointer_width = "64")]
            for count in [u32::MAX as usize + 1, usize::MAX] {
                assert_eq!(current.index(random, count), random as usize % count);
            }
        }
        assert!(saw_width_difference);
    }

    #[test]
    fn cover_random_choice_modes_keep_positive_replay_and_exhaustion_without_proof() {
        for legacy_wasm32 in [false, true] {
            for protected in [None, Some(0)] {
                for can_improve in [false, true] {
                    let masks: &[u64] = if can_improve {
                        &[1, 2, 4, 3, 6]
                    } else {
                        &[1, 2, 4]
                    };
                    let rows = masks
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(source_index, mask)| DenseRow {
                            source_index,
                            words: vec![mask],
                        })
                        .collect::<Vec<_>>();
                    let target = [7];
                    let support = (0..3)
                        .map(|pattern| {
                            rows.iter()
                                .enumerate()
                                .filter_map(|(row, value)| {
                                    (value.words[0] & (1 << pattern) != 0).then_some(row)
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let mut repair = FixedCardinalityCoverSearchSession::try_new(
                        &rows,
                        &target,
                        &support,
                        vec![0, 1, 2],
                        80,
                        protected,
                        0,
                        &mut |_| Ok(()),
                    )
                    .unwrap()
                    .unwrap();
                    repair.random_choice = CoverRandomChoice { legacy_wasm32 };
                    let initial_seed = repair.seed.clone();
                    let initial_random = repair.random_state;
                    assert_eq!(
                        repair
                            .advance_one(&rows, &target, &support, &mut || true)
                            .unwrap(),
                        OptionalHeuristicStep::Cancelled
                    );
                    assert_eq!(repair.seed, initial_seed);
                    assert_eq!(repair.random_state, initial_random);
                    let mut terminal = None;
                    for _ in 0..100 {
                        match repair
                            .advance_one(&rows, &target, &support, &mut || false)
                            .unwrap()
                        {
                            OptionalHeuristicStep::Pending => {}
                            outcome => {
                                terminal = Some(outcome);
                                break;
                            }
                        }
                    }
                    if can_improve {
                        let Some(OptionalHeuristicStep::Found(selected)) = terminal else {
                            panic!("both fixed-width policies should find the tiny positive cover")
                        };
                        assert_eq!(selected.len(), 2);
                        assert_eq!(selected.iter().fold(0, |union, row| union | masks[*row]), 7);
                        assert!(protected.is_none_or(|row| selected.contains(&row)));
                    } else {
                        // Optional heuristic exhaustion is not an AtMost UNSAT receipt.
                        assert_eq!(terminal, Some(OptionalHeuristicStep::Finished));
                        assert_eq!(repair.seed, initial_seed);
                    }

                    let mut randomized = RandomizedCompactCoverSearchSession::try_new(
                        &rows,
                        &target,
                        0,
                        32,
                        0x9e37_79b9_7f4a_7c15,
                        Some(2),
                        protected,
                        0,
                        &mut |_| Ok(()),
                    )
                    .unwrap();
                    randomized.random_choice = CoverRandomChoice { legacy_wasm32 };
                    let mut best = vec![0, 1, 2];
                    let mut random_terminal = None;
                    for _ in 0..40 {
                        match randomized
                            .advance(&rows, &target, &mut best, 1, &mut || false)
                            .unwrap()
                        {
                            OptionalHeuristicStep::Pending => {}
                            outcome => {
                                random_terminal = Some(outcome);
                                break;
                            }
                        }
                    }
                    if can_improve {
                        let Some(OptionalHeuristicStep::Found(selected)) = random_terminal else {
                            panic!("both fixed-width policies should find the tiny greedy cover")
                        };
                        assert_eq!(selected.len(), 2);
                        assert_eq!(selected.iter().fold(0, |union, row| union | masks[*row]), 7);
                        assert!(protected.is_none_or(|row| selected.contains(&row)));
                    } else {
                        assert_eq!(random_terminal, Some(OptionalHeuristicStep::Finished));
                        assert_eq!(best, vec![0, 1, 2]);
                    }
                }
            }
        }
    }

    #[test]
    fn breakout_word_masks_match_reference_scores_across_words_and_padding() {
        let mut random = 0x6a09_e667_f3bc_c909_u64;
        for _ in 0..2_000 {
            let target = [
                next_breakout_random(&mut random),
                next_breakout_random(&mut random),
                next_breakout_random(&mut random) & 0x1ff,
            ];
            let mut counts = vec![0_usize; 137];
            for count in &mut counts {
                *count = [0, 1, 2, usize::MAX][next_breakout_random(&mut random) as usize % 4];
            }
            let mut uncovered = [0_u64; 3];
            let mut singly_covered = [0_u64; 3];
            for (pattern, count) in counts.iter().copied().enumerate() {
                let word = pattern / 64;
                let bit = 1_u64 << (pattern % 64);
                if target[word] & bit != 0 {
                    uncovered[word] |= if count == 0 { bit } else { 0 };
                    singly_covered[word] |= if count == 1 { bit } else { 0 };
                }
            }
            let add = core::array::from_fn::<_, 3, _>(|_| next_breakout_random(&mut random));
            let remove = core::array::from_fn::<_, 3, _>(|_| next_breakout_random(&mut random));
            assert_eq!(
                breakout_word_make_gain(&add, &uncovered),
                breakout_reference_make_gain(&add, &target, &counts)
            );
            assert_eq!(
                breakout_word_break_loss(&remove, &add, &singly_covered),
                breakout_reference_break_loss(&remove, &add, &target, &counts)
            );
        }
    }

    #[test]
    fn breakout_word_masks_preserve_every_swap_random_state_and_terminal_outcome() {
        let mut random = 0xbb67_ae85_84ca_a73b_u64;
        let mut saw_found = false;
        let mut saw_finished = false;
        for pattern_count in [7_usize, 65, 71, 129] {
            let mut target = vec![u64::MAX; pattern_count.div_ceil(64)];
            if pattern_count % 64 != 0 {
                *target.last_mut().unwrap() = (1_u64 << (pattern_count % 64)) - 1;
            }
            for case in 0..12 {
                let row_count = if case == 0 { 4 } else { 12 };
                let mut rows = (0..row_count)
                    .map(|source_index| DenseRow {
                        source_index,
                        words: vec![0; target.len()],
                    })
                    .collect::<Vec<_>>();
                for pattern in 0..pattern_count {
                    rows[pattern % 4].words[pattern / 64] |= 1_u64 << (pattern % 64);
                }
                for row in &mut rows[4..] {
                    for word in &mut row.words {
                        // Padding deliberately remains set outside the target.
                        *word = next_breakout_random(&mut random);
                    }
                }
                if case == 1 {
                    rows[4].words.clone_from(&target);
                }
                let support = (0..pattern_count)
                    .map(|pattern| {
                        rows.iter()
                            .enumerate()
                            .filter_map(|(row, value)| {
                                (value.words[pattern / 64] & (1_u64 << (pattern % 64)) != 0)
                                    .then_some(row)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                for protected in [None, Some(case % 4)] {
                    let mut reference = FixedCardinalityCoverSearchSession::try_new(
                        &rows,
                        &target,
                        &support,
                        vec![0, 1, 2, 3],
                        80,
                        protected,
                        0,
                        &mut |_| Ok(()),
                    )
                    .unwrap()
                    .unwrap();
                    let mut word_masks = reference.clone();
                    reference.word_mask_scoring = false;
                    word_masks.word_mask_scoring = true;
                    let before_cancel = word_masks.random_state;
                    assert_eq!(
                        word_masks
                            .advance_one(&rows, &target, &support, &mut || true)
                            .unwrap(),
                        OptionalHeuristicStep::Cancelled
                    );
                    assert_eq!(word_masks.random_state, before_cancel);
                    let mut terminal = false;
                    for step in 0..100 {
                        let old = reference
                            .advance_one(&rows, &target, &support, &mut || false)
                            .unwrap();
                        let new = word_masks
                            .advance_one(&rows, &target, &support, &mut || false)
                            .unwrap();
                        assert_eq!(
                            new, old,
                            "patterns={pattern_count}, case={case}, step={step}"
                        );
                        assert_eq!(word_masks.random_state, reference.random_state);
                        assert_eq!(word_masks.restart, reference.restart);
                        assert_eq!(word_masks.iteration, reference.iteration);
                        assert_eq!(word_masks.attempted_swaps, reference.attempted_swaps);
                        assert_eq!(
                            word_masks.restart_initialized,
                            reference.restart_initialized
                        );
                        assert_eq!(word_masks.proposal, reference.proposal);
                        assert_eq!(word_masks.seed, reference.seed);
                        assert_eq!(word_masks.selected, reference.selected);
                        assert_eq!(word_masks.coverage_counts, reference.coverage_counts);
                        assert_eq!(word_masks.pivot_candidates, reference.pivot_candidates);
                        assert_eq!(word_masks.remove_candidates, reference.remove_candidates);
                        let candidate_tuple = |candidate: &BreakoutSwapCandidate| {
                            (
                                candidate.net_gain,
                                candidate.make_gain,
                                candidate.break_loss,
                                candidate.add_row,
                                candidate.remove_row,
                            )
                        };
                        assert!(
                            word_masks
                                .swap_candidates
                                .iter()
                                .map(candidate_tuple)
                                .eq(reference.swap_candidates.iter().map(candidate_tuple))
                        );
                        match new {
                            OptionalHeuristicStep::Found(_) => {
                                saw_found = true;
                                terminal = true;
                                break;
                            }
                            OptionalHeuristicStep::Finished => {
                                saw_finished = true;
                                terminal = true;
                                break;
                            }
                            OptionalHeuristicStep::Pending => {}
                            OptionalHeuristicStep::Cancelled => panic!("unexpected cancellation"),
                        }
                    }
                    assert!(terminal, "bounded repair must finish");
                }
            }
        }
        assert!(
            saw_found && saw_finished,
            "cover both positive and exhausted searches"
        );
    }

    #[test]
    fn breakout_word_masks_include_the_new_buffer_in_peak_admission() {
        let target = [0b111, 0b111];
        let rows = (0..4)
            .map(|source_index| DenseRow {
                source_index,
                words: vec![1_u64 << (source_index % 3); 2],
            })
            .collect::<Vec<_>>();
        let support = vec![vec![0, 1, 2, 3]; 67];
        let base = 37;
        let create = |guard: &mut dyn FnMut(u128) -> Result<(), ExactMinimumCoverError>| {
            FixedCardinalityCoverSearchSession::try_new(
                &rows,
                &target,
                &support,
                vec![0, 1, 2],
                80,
                None,
                base,
                &mut |bytes| guard(bytes),
            )
        };
        let mut peak = 0;
        let session = create(&mut |bytes| {
            peak = peak.max(bytes);
            Ok(())
        })
        .unwrap()
        .unwrap();
        assert_eq!(session.singly_covered_words.len(), target.len());
        assert_eq!(
            peak,
            base + session.checked_retained_capacity_bytes().unwrap()
        );
        let capacity_sum = [
            &session.seed,
            &session.proposal,
            &session.pivot_candidates,
            &session.remove_candidates,
            &session.coverage_counts,
        ]
        .into_iter()
        .map(|values| values.capacity() as u128 * core::mem::size_of::<usize>() as u128)
        .sum::<u128>()
            + (session.replay.capacity() + session.singly_covered_words.capacity()) as u128 * 8
            + session.selected.capacity() as u128 * core::mem::size_of::<bool>() as u128
            + session.swap_candidates.capacity() as u128
                * core::mem::size_of::<BreakoutSwapCandidate>() as u128;
        assert_eq!(
            session.checked_retained_capacity_bytes().unwrap(),
            capacity_sum
        );
        let limited = |cap| {
            create(&mut |required_memory_bytes| {
                if required_memory_bytes <= cap {
                    Ok(())
                } else {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes,
                        max_memory_bytes: cap,
                    })
                }
            })
        };
        assert!(limited(peak).is_ok());
        assert!(matches!(limited(peak - 1),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded { required_memory_bytes, max_memory_bytes })
            if required_memory_bytes == peak && max_memory_bytes == peak - 1));
    }

    #[test]
    fn top_gain_insertion_matches_replace_and_sort_for_every_supported_width() {
        for width in 0..=MAX_TOP_GAIN_BOUND_SLOTS {
            let mut actual = vec![0; width];
            let mut expected = actual.clone();
            for gain in
                (0..257)
                    .map(|value| (value * 97) % 257)
                    .chain([0, 0, usize::MAX, 256, usize::MAX])
            {
                insert_descending_top_gain(&mut actual, gain);
                if let Some(last) = expected.last_mut() {
                    if gain > *last {
                        *last = gain;
                        expected.sort_unstable_by(|left, right| right.cmp(left));
                    }
                }
                assert_eq!(actual, expected, "width={width}, gain={gain}");
            }
        }
    }

    #[test]
    fn quotient_compaction_preserves_covers_and_exact_retained_accounting() {
        let target = vec![u64::MAX, u64::MAX, 3];
        let mut rows: Vec<DenseRow> = [1_u64, 2, 3, 4, 5, 7]
            .into_iter()
            .enumerate()
            .map(|(source_index, groups)| {
                let mut words = Vec::with_capacity(4 + source_index * 3);
                words.resize(3, 0);
                for pattern in 0..130 {
                    if groups & (1 << (pattern % 3)) != 0 {
                        words[pattern / 64] |= 1 << (pattern % 64);
                    }
                }
                DenseRow {
                    source_index,
                    words,
                }
            })
            .collect();
        let original_rows = rows.clone();
        let mut final_reported_live = 0;
        let compact = quotient_redundant_target_constraints_with_memory_guard(
            &mut rows,
            target.clone(),
            vec![1; 130],
            false,
            &mut |live| {
                final_reported_live = live;
                Ok(())
            },
        )
        .expect("lossless compact quotient");
        assert_eq!(compact.weights.iter().sum::<usize>(), 130);
        assert_eq!(compact.words, vec![7]);
        assert_eq!(
            final_reported_live,
            checked_dense_rows_retained_bytes(&rows).unwrap()
                + checked_vec_retained_bytes(&compact.words).unwrap()
                + checked_vec_retained_bytes(&compact.weights).unwrap()
        );
        for subset in 0..(1 << rows.len()) {
            let mut original_union = vec![0; target.len()];
            let mut compact_union = vec![0; compact.words.len()];
            for row in 0..rows.len() {
                if subset & (1 << row) != 0 {
                    union_words(&mut original_union, &original_rows[row].words);
                    union_words(&mut compact_union, &rows[row].words);
                }
            }
            assert_eq!(
                is_superset(&original_union, &target),
                is_superset(&compact_union, &compact.words),
                "cover semantics changed for subset {subset}"
            );
        }
    }

    #[test]
    fn covered_state_memo_growth_is_amortized_and_tight_memory_remains_exact() {
        let mut memo = ExactCoveredStateMemo::new();
        let mut capacity_changes = 0;
        for word in 0..128_u64 {
            let capacity_before = memo.entries.capacity();
            assert!(
                !memo
                    .should_prune_or_record(&[word], 3, 0, &mut |_| Ok(()))
                    .expect("new exact state")
            );
            capacity_changes += usize::from(memo.entries.capacity() != capacity_before);
            assert!(
                memo.should_prune_or_record(&[word], 3, 0, &mut |_| Ok(()))
                    .expect("same depth prunes")
            );
            assert!(
                !memo
                    .should_prune_or_record(&[word], 2, 0, &mut |_| Ok(()))
                    .expect("shallower depth remains searchable")
            );
        }
        assert!(
            capacity_changes <= 6,
            "memo reallocated {capacity_changes} times"
        );
        let expected_retained = checked_vec_retained_bytes(&memo.entries).unwrap()
            + checked_vec_retained_bytes(&memo.buckets).unwrap()
            + memo
                .entries
                .iter()
                .map(|entry| checked_vec_retained_bytes(&entry.words).unwrap())
                .sum::<u128>();
        assert_eq!(
            memo.checked_heap_retained_bytes().unwrap(),
            expected_retained
        );

        let mut capped = ExactCoveredStateMemo::new();
        let max_memory_bytes = (MIN_MEMO_BUCKET_COUNT * core::mem::size_of::<usize>()
            + core::mem::size_of::<u64>()
            + core::mem::size_of::<ExactCoveredStateMemoEntry>())
            as u128;
        let mut spare_capacity_denials = 0;
        assert!(
            !capped
                .should_prune_or_record(&[7], 1, 0, &mut |required_memory_bytes| {
                    if required_memory_bytes > max_memory_bytes {
                        spare_capacity_denials += 1;
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes,
                            max_memory_bytes,
                        })
                    } else {
                        Ok(())
                    }
                })
                .expect("exact next state fits without spare capacity")
        );
        assert_eq!(spare_capacity_denials, 1);
        assert_eq!(
            capped.checked_heap_retained_bytes().unwrap(),
            max_memory_bytes
        );
    }

    fn exact_fixture() -> (PatternBitSet, Vec<PatternBitSet>) {
        let required = PatternBitSet::from_patterns(
            4,
            [
                PatternId::new(0),
                PatternId::new(1),
                PatternId::new(2),
                PatternId::new(3),
            ],
        )
        .expect("required");
        let rows = vec![
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)]).expect("row 0"),
            PatternBitSet::from_patterns(4, [PatternId::new(2), PatternId::new(3)]).expect("row 1"),
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)]).expect("row 2"),
            PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(3)]).expect("row 3"),
        ];
        (required, rows)
    }

    fn resumable_branching_fixture() -> (PatternBitSet, Vec<PatternBitSet>) {
        let required = PatternBitSet::from_patterns(6, (0..6).map(PatternId::new))
            .expect("resumable required");
        let rows = [
            [0, 1, 2, 3].as_slice(),
            [0, 1, 4].as_slice(),
            [2, 3, 5].as_slice(),
            [0, 2, 4].as_slice(),
            [1, 3, 5].as_slice(),
        ]
        .into_iter()
        .map(|patterns| {
            PatternBitSet::from_patterns(6, patterns.iter().copied().map(PatternId::new))
                .expect("resumable row")
        })
        .collect();
        (required, rows)
    }

    #[test]
    fn resumable_minimum_cover_yields_on_node_budget_and_matches_blocking_authority() {
        let (required, rows) = resumable_branching_fixture();
        let blocking = exact_minimum_cover(&required, &rows).expect("blocking exact authority");
        let mut session =
            ExactMinimumCoverSession::new(&required, &rows).expect("resumable exact authority");
        let mut saw_budget_yield = false;
        let mut total_nodes = 0_u64;
        let resumed = loop {
            match session.advance(1).expect("bounded exact advance") {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                    assert_eq!(
                        visited_nodes, 1,
                        "a nonterminal slice consumes its node budget"
                    );
                    total_nodes += visited_nodes;
                    saw_budget_yield = true;
                }
                ExactMinimumCoverSessionAdvance::Found {
                    result,
                    visited_nodes,
                } => {
                    assert!(visited_nodes <= 1);
                    total_nodes += visited_nodes;
                    break result;
                }
                ExactMinimumCoverSessionAdvance::ProvedNone { .. } => {
                    panic!("minimum proof cannot report an infeasible at-most decision")
                }
                ExactMinimumCoverSessionAdvance::Cancelled { .. } => {
                    panic!("uncancelled minimum proof cannot cancel")
                }
                ExactMinimumCoverSessionAdvance::Finished => {
                    panic!("session must emit its terminal proof exactly once")
                }
            }
        };
        assert!(
            saw_budget_yield,
            "fixture must cross at least one node slice"
        );
        assert!(total_nodes > 1);
        assert_eq!(resumed, blocking);
        assert_eq!(
            session.advance(1).unwrap(),
            ExactMinimumCoverSessionAdvance::Finished
        );
    }

    #[test]
    fn resumable_minimum_cover_zero_budget_is_zero_work_and_cancel_is_terminal() {
        let (required, rows) = resumable_branching_fixture();
        let mut session =
            ExactMinimumCoverSession::new(&required, &rows).expect("resumable exact authority");
        assert_eq!(
            session.advance(0).expect("zero-work exact advance"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 0 }
        );
        let cancelled = session
            .advance_with_memory_guard_and_control(1, &mut |_| Ok(()), &mut || true)
            .expect("cancel exact session");
        assert_eq!(
            cancelled,
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes: 0 }
        );
        assert_eq!(
            session.advance(1).unwrap(),
            ExactMinimumCoverSessionAdvance::Finished
        );
    }

    #[test]
    fn resumable_preparation_cancels_between_lossless_reduction_phases() {
        let (required, rows) = resumable_branching_fixture();
        let mut session =
            ExactMinimumCoverSession::new(&required, &rows).expect("resumable exact authority");
        assert_eq!(
            session.advance(1).expect("first preparation phase"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
        );
        assert_eq!(
            session.advance(0).expect("zero budget between phases"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 0 }
        );
        assert_eq!(
            session
                .advance_with_memory_guard_and_control(1, &mut |_| Ok(()), &mut || true)
                .expect("cancel between preparation phases"),
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes: 0 }
        );
        assert_eq!(
            session.advance(1).unwrap(),
            ExactMinimumCoverSessionAdvance::Finished
        );
    }

    #[test]
    fn resumable_at_most_uses_the_blocking_greedy_identity_before_a_valid_hint() {
        let required = PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
            .expect("required");
        let rows = vec![
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("row 0"),
            PatternBitSet::from_patterns(2, [PatternId::new(1)]).expect("row 1"),
            PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)]).expect("row 2"),
        ];
        let blocking = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            2,
            &[0, 1],
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("blocking witness baseline");
        assert_eq!(
            drive_resumable_at_most(&required, &rows, 2, &[0, 1], 1),
            blocking
        );
        let ExactCoverAtMostDecision::Found(result) = blocking else {
            panic!("greedy baseline must find a cover")
        };
        assert_eq!(result.row_indices(), &[2]);
        assert_eq!(result.covered_patterns(), &rows[2]);
    }

    #[test]
    fn resumable_exact_session_clone_preserves_pending_dfs_and_is_guarded() {
        let (required, rows) = resumable_branching_fixture();
        let mut session =
            ExactMinimumCoverSession::new(&required, &rows).expect("resumable exact authority");
        assert!(matches!(
            session.advance(1).expect("first bounded node"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
        ));

        let retained = session
            .inner
            .checked_retained_capacity_bytes()
            .expect("session retained bytes");
        assert!(retained > 0);
        let mut observed_peak = 0_u128;
        let mut cloned = session
            .inner
            .try_clone_with_memory_guard(17, &mut |whole_live| {
                observed_peak = observed_peak.max(whole_live);
                Ok(())
            })
            .expect("guarded pending clone");
        assert!(observed_peak >= 17 + retained);

        let max_memory_bytes = 17 + retained - 1;
        assert!(matches!(
            session.inner.try_clone_with_memory_guard(17, &mut |whole_live| {
                if whole_live <= max_memory_bytes {
                    Ok(())
                } else {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: whole_live,
                        max_memory_bytes,
                    })
                }
            }),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes: rejected,
            }) if required_memory_bytes > rejected
        ));

        let finish = |inner: &mut ExactCoverSearchSession| loop {
            match inner
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("resume cloned proof")
            {
                ExactMinimumCoverSessionAdvance::Pending { .. } => {}
                ExactMinimumCoverSessionAdvance::Found { result, .. } => break result,
                other => panic!("unexpected exact terminal: {other:?}"),
            }
        };
        assert_eq!(finish(&mut session.inner), finish(&mut cloned));
    }

    fn dual_admission_fixture() -> (PatternBitSet, Vec<PatternBitSet>) {
        const ROW_COUNT: usize = 64;
        const CONSTRAINT_COUNT: usize = 128;
        let required = PatternBitSet::from_patterns(
            CONSTRAINT_COUNT,
            (0..CONSTRAINT_COUNT).map(PatternId::new),
        )
        .expect("required");
        let mut row_patterns = vec![Vec::new(); ROW_COUNT];
        for row in 0..ROW_COUNT {
            let adjacent_constraint = row;
            row_patterns[row].push(PatternId::new(adjacent_constraint));
            row_patterns[(row + 1) % ROW_COUNT].push(PatternId::new(adjacent_constraint));

            let distance_two_constraint = ROW_COUNT + row;
            row_patterns[row].push(PatternId::new(distance_two_constraint));
            row_patterns[(row + 2) % ROW_COUNT].push(PatternId::new(distance_two_constraint));
        }
        let rows = row_patterns
            .into_iter()
            .map(|patterns| {
                PatternBitSet::from_patterns(CONSTRAINT_COUNT, patterns)
                    .expect("dual admission row")
            })
            .collect();
        (required, rows)
    }

    fn witness_assisted_fixture() -> (PatternBitSet, Vec<PatternBitSet>, usize) {
        // One seven-vertex graph where deterministic maximum-degree greedy
        // needs five vertices although four suffice, plus thirty independent
        // four-cycles where greedy is exact. Every edge has a distinct
        // two-row support, so exact constraint quotienting preserves all 129
        // constraints and all 127 nondominated rows.
        const FILLER_CYCLES: usize = 30;
        const ROW_COUNT: usize = 7 + FILLER_CYCLES * 4;
        const PATTERN_COUNT: usize = 9 + FILLER_CYCLES * 4;
        let mut row_patterns = vec![Vec::new(); ROW_COUNT];
        let hard_edges = [
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 2),
            (1, 3),
            (2, 5),
            (3, 6),
            (4, 5),
            (4, 6),
        ];
        let mut pattern = 0_usize;
        for (left, right) in hard_edges {
            row_patterns[left].push(PatternId::new(pattern));
            row_patterns[right].push(PatternId::new(pattern));
            pattern += 1;
        }
        for cycle in 0..FILLER_CYCLES {
            let base = 7 + cycle * 4;
            for edge in 0..4 {
                let left = base + edge;
                let right = base + (edge + 1) % 4;
                row_patterns[left].push(PatternId::new(pattern));
                row_patterns[right].push(PatternId::new(pattern));
                pattern += 1;
            }
        }
        assert_eq!(pattern, PATTERN_COUNT);
        let required =
            PatternBitSet::from_patterns(PATTERN_COUNT, (0..PATTERN_COUNT).map(PatternId::new))
                .expect("required");
        let rows = row_patterns
            .into_iter()
            .map(|patterns| {
                PatternBitSet::from_patterns(PATTERN_COUNT, patterns).expect("fixture row")
            })
            .collect();
        (required, rows, 4 + FILLER_CYCLES * 2)
    }

    fn large_augmented_selector_fixture()
    -> (PatternBitSet, Vec<PatternBitSet>, usize, Vec<usize>, usize) {
        let (base_required, base_rows, limit) = witness_assisted_fixture();
        let base_pattern_count = base_required.pattern_count();
        let pattern_count = base_pattern_count + 1;
        let selector_word = base_pattern_count / u64::BITS as usize;
        let selector_bit = 1_u64 << (base_pattern_count % u64::BITS as usize);
        let word_count = pattern_count.div_ceil(u64::BITS as usize);
        let extend_row = |row: &PatternBitSet, carries_selector: bool| {
            let mut words = (0..word_count)
                .map(|word| row.word_at(word))
                .collect::<Vec<_>>();
            if carries_selector {
                words[selector_word] |= selector_bit;
            }
            PatternBitSet::from_words(pattern_count, words).expect("augmented selector row")
        };
        let mut rows = base_rows
            .iter()
            .map(|row| extend_row(row, false))
            .collect::<Vec<_>>();
        let first_selector_source_row = rows.len();
        // Eight late duplicates of the first hard-graph row carry the new
        // selector. Original constraints still have support two, so the old
        // global-rarest pivot does not choose this selector (support eight),
        // while replacing hint row zero by any duplicate gives a size-limit
        // cover.
        rows.extend((0..8).map(|_| extend_row(&base_rows[0], true)));

        let required =
            PatternBitSet::from_patterns(pattern_count, (0..pattern_count).map(PatternId::new))
                .expect("augmented selector required");
        let mut witness_hint = vec![0, 1, 5, 6];
        for cycle in 0..30 {
            let base = 7 + cycle * 4;
            witness_hint.extend([base, base + 2]);
        }
        assert_eq!(witness_hint.len(), limit);
        let mut base_replay = PatternBitSet::new(pattern_count);
        for row in witness_hint.iter().copied() {
            base_replay
                .union_with(&rows[row])
                .expect("augmented base replay");
        }
        let required_without_selector = PatternBitSet::from_patterns(
            pattern_count,
            (0..base_pattern_count).map(PatternId::new),
        )
        .expect("selector-free required");
        assert!(
            base_replay
                .is_superset(&required_without_selector)
                .expect("same augmented universe")
        );
        assert!(
            !base_replay
                .is_superset(&required)
                .expect("same augmented universe")
        );
        (
            required,
            rows,
            limit,
            witness_hint,
            first_selector_source_row,
        )
    }

    fn drive_resumable_at_most(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        limit: usize,
        witness_hint: &[usize],
        budget: u64,
    ) -> ExactCoverAtMostDecision {
        let mut session = ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
            required,
            rows,
            limit,
            Some(witness_hint),
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("resumable witness-assisted AtMost session");
        for _ in 0..100_000 {
            match session
                .advance(budget, &mut |_| Ok(()), &mut || false)
                .expect("resumable witness advance")
            {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                    assert!(visited_nodes > 0, "positive budget must make progress")
                }
                ExactMinimumCoverSessionAdvance::Found { result, .. } => {
                    return ExactCoverAtMostDecision::Found(ExactCoverAtMostResult {
                        row_indices: result.row_indices,
                        covered_patterns: result.covered_patterns,
                    });
                }
                ExactMinimumCoverSessionAdvance::ProvedNone { .. } => {
                    return ExactCoverAtMostDecision::ProvedNone;
                }
                ExactMinimumCoverSessionAdvance::Cancelled { .. } => {
                    return ExactCoverAtMostDecision::Cancelled;
                }
                ExactMinimumCoverSessionAdvance::Finished => {
                    panic!("session ended without its terminal decision")
                }
            }
        }
        panic!("resumable witness fixture exceeded its bounded step ceiling")
    }

    #[test]
    fn global_warm_stop_does_not_consume_the_worker_fallback() {
        for phase in [
            WitnessShortcutPhase::PrepareForcedSupporters,
            WitnessShortcutPhase::ForcedGreedy {
                supporter_position: 0,
            },
        ] {
            let mut shortcut = WitnessShortcutSession::new(
                PatternBitSet::from_words(3, vec![7]).unwrap(),
                vec![],
                ExactCoverSearchGoal::AtMost(1),
                vec![],
            );
            shortcut.phase = phase;
            let session = ExactCoverSearchSession {
                state: ExactCoverSearchSessionState::WitnessShortcut(shortcut),
            };
            assert!(session.witness_shortcut_exhausted());
            assert!(
                matches!(
                    session.state,
                    ExactCoverSearchSessionState::WitnessShortcut(_)
                ),
                "the read-only global policy does not mutate worker fallback state"
            );
        }
    }

    #[test]
    fn cooperative_witness_shortcut_matches_blocking_order_across_caller_budgets() {
        let (required, rows, limit) = witness_assisted_fixture();
        let blocking = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            limit,
            &[],
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("blocking witness baseline");
        for budget in [1, 16, 64, 8_192] {
            assert_eq!(
                drive_resumable_at_most(&required, &rows, limit, &[], budget),
                blocking,
                "caller budget {budget} must not change deterministic authority or identity"
            );
        }
    }

    #[test]
    fn cooperative_witness_matches_blocking_for_augmented_lex_selector() {
        // This is the smallest shape of a portfolio canonicalization query:
        // rows before selector_end carry a synthetic selector constraint,
        // while the known suffix witness deliberately does not.  The hint must
        // therefore seed the assisted search without being returned directly.
        let required = PatternBitSet::from_patterns(
            3,
            [PatternId::new(0), PatternId::new(1), PatternId::new(2)],
        )
        .expect("augmented required");
        let rows = vec![
            PatternBitSet::from_patterns(3, [PatternId::new(0), PatternId::new(2)])
                .expect("selector row 0"),
            PatternBitSet::from_patterns(3, [PatternId::new(0), PatternId::new(2)])
                .expect("selector row 1"),
            PatternBitSet::from_patterns(3, [PatternId::new(0)]).expect("known row 2"),
            PatternBitSet::from_patterns(3, [PatternId::new(1)]).expect("known row 3"),
        ];
        let blocking = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            2,
            &[2, 3],
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("blocking augmented-selector witness");
        let ExactCoverAtMostDecision::Found(blocking_result) = &blocking else {
            panic!("selector query must be feasible")
        };
        assert_eq!(blocking_result.row_indices(), &[0, 3]);
        for budget in [1, 16, 64, 8_192] {
            let resumed = drive_resumable_at_most(&required, &rows, 2, &[2, 3], budget);
            assert_eq!(resumed, blocking, "caller budget {budget}");
            let ExactCoverAtMostDecision::Found(result) = resumed else {
                panic!("selector query must remain feasible")
            };
            let mut replay = PatternBitSet::new(3);
            for row in result.row_indices() {
                replay.union_with(&rows[*row]).expect("selector replay");
            }
            assert_eq!(replay, required);
        }
    }

    #[test]
    fn large_augmented_selector_uses_preferred_pivot_across_caller_budgets() {
        let (required, rows, limit, witness_hint, first_selector_source_row) =
            large_augmented_selector_fixture();
        let dense_rows = rows
            .iter()
            .enumerate()
            .map(|(source_index, row)| DenseRow {
                source_index,
                words: (0..required.word_count())
                    .map(|word| row.word_at(word) & required.word_at(word))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let base_live = checked_dense_rows_retained_bytes(&dense_rows).expect("dense bytes");
        let mut shortcut_peak = 0_u128;
        let shortcut = witness_assisted_cover_before_dominance(
            &required,
            &dense_rows,
            limit,
            &witness_hint,
            base_live,
            &mut |whole_live| {
                shortcut_peak = shortcut_peak.max(whole_live);
                Ok(())
            },
            &mut || false,
        )
        .expect("large augmented positive shortcut");
        let WitnessAssistedCoverDecision::Found(selected) = shortcut else {
            panic!("preferred selector pivot must find a positive witness")
        };
        assert_eq!(selected.len(), limit);
        assert!(
            selected
                .iter()
                .any(|row| dense_rows[*row].source_index >= first_selector_source_row)
        );

        let mut cursor = ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
            &required,
            &rows,
            limit,
            Some(&witness_hint),
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("large selector resumable cursor");
        for _ in 0..64 {
            if matches!(
                cursor.state,
                ExactCoverSearchSessionState::WitnessShortcut(WitnessShortcutSession {
                    phase: WitnessShortcutPhase::Breakout { .. },
                    ..
                })
            ) {
                break;
            }
            assert!(matches!(
                cursor
                    .advance(1, &mut |_| Ok(()), &mut || false)
                    .expect("advance to preferred breakout"),
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
            ));
        }
        assert!(matches!(
            cursor.state,
            ExactCoverSearchSessionState::WitnessShortcut(WitnessShortcutSession {
                phase: WitnessShortcutPhase::Breakout { .. },
                ..
            })
        ));
        let retained_before_zero = cursor
            .checked_retained_capacity_bytes()
            .expect("preferred cursor retained bytes");
        assert_eq!(
            cursor
                .advance(0, &mut |_| Ok(()), &mut || false)
                .expect("preferred cursor zero budget"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 0 }
        );
        assert_eq!(
            cursor.checked_retained_capacity_bytes(),
            Some(retained_before_zero)
        );
        let mut cloned = cursor
            .try_clone_with_memory_guard(0, &mut |_| Ok(()))
            .expect("preferred cursor guarded clone");
        assert_eq!(
            cursor
                .advance(1, &mut |_| Ok(()), &mut || true)
                .expect("cancel preferred cursor"),
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes: 0 }
        );
        let cloned_rows = loop {
            match cloned
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("continue cloned preferred cursor")
            {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                    assert_eq!(visited_nodes, 1)
                }
                ExactMinimumCoverSessionAdvance::Found { result, .. } => break result.row_indices,
                terminal => panic!("unexpected preferred cursor terminal: {terminal:?}"),
            }
        };

        let blocking = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            limit,
            &witness_hint,
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("blocking large augmented selector");
        let ExactCoverAtMostDecision::Found(blocking_result) = &blocking else {
            panic!("large selector query must be feasible")
        };
        assert_eq!(blocking_result.row_indices().len(), limit);
        assert_eq!(cloned_rows, blocking_result.row_indices());
        for budget in [1, 16, 64, 8_192] {
            assert_eq!(
                drive_resumable_at_most(&required, &rows, limit, &witness_hint, budget),
                blocking,
                "preferred selector shortcut must be caller-budget invariant at {budget}"
            );
        }

        let max_memory_bytes = shortcut_peak - 1;
        assert!(matches!(
            witness_assisted_cover_before_dominance(
                &required,
                &dense_rows,
                limit,
                &witness_hint,
                base_live,
                &mut |whole_live| {
                    if whole_live <= max_memory_bytes {
                        Ok(())
                    } else {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: whole_live,
                            max_memory_bytes,
                        })
                    }
                },
                &mut || false,
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded { .. })
        ));
    }

    #[test]
    fn reduced_witness_breakout_is_resumable_cloneable_and_cancellable() {
        let (required, rows) = dual_admission_fixture();
        let mut session = prepare_lazy_exact_cover_search_session(
            &required,
            &rows,
            ExactCoverSearchGoal::AtMost(31),
            ExactCoverIncumbentPolicy::WitnessAssistedAfterRawSearch,
            None,
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("post-witness reduced search");
        for _ in 0..16 {
            if matches!(
                session.state,
                ExactCoverSearchSessionState::ImprovingBreakout { .. }
            ) {
                break;
            }
            let step = session
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("bounded reduction advance");
            assert!(
                matches!(
                    step,
                    ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
                ),
                "unexpected reduction step: {step:?}"
            );
        }
        assert!(matches!(
            session.state,
            ExactCoverSearchSessionState::ImprovingBreakout { .. }
        ));
        assert_eq!(
            session
                .advance(0, &mut |_| Ok(()), &mut || false)
                .expect("zero breakout budget"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 0 }
        );
        let retained = session
            .checked_retained_capacity_bytes()
            .expect("breakout retained bytes");
        let mut observed = 0_u128;
        let mut cloned = session
            .try_clone_with_memory_guard(11, &mut |whole_live| {
                observed = observed.max(whole_live);
                Ok(())
            })
            .expect("guarded breakout clone");
        assert!(observed >= 11 + retained);
        assert_eq!(
            session
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("original breakout step"),
            cloned
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("cloned breakout step")
        );
        assert_eq!(
            cloned
                .advance(1, &mut |_| Ok(()), &mut || true)
                .expect("cancel breakout"),
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes: 0 }
        );
    }

    #[test]
    fn cooperative_root_keeps_residual_workspace_and_caps_atomic_node_work() {
        let (required, rows) = dual_admission_fixture();
        let mut session = prepare_lazy_exact_cover_search_session(
            &required,
            &rows,
            ExactCoverSearchGoal::AtMost(0),
            ExactCoverIncumbentPolicy::Standard,
            None,
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("cooperative dual session");
        for _ in 0..16 {
            if matches!(
                session.state,
                ExactCoverSearchSessionState::Searching { .. }
            ) {
                break;
            }
            assert!(matches!(
                session
                    .advance(1, &mut |_| Ok(()), &mut || false)
                    .expect("bounded dual preparation"),
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
            ));
        }
        let ExactCoverSearchSessionState::Searching { search, .. } = &session.state else {
            panic!("dual session must reach exact search")
        };
        assert!(search.root_dual.is_some());
        assert!(search.dual_workspace.is_some());

        let outcome = session
            .advance(u64::MAX, &mut |_| Ok(()), &mut || false)
            .expect("workspace-backed bounded exact advance");
        let consumed = match outcome {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes }
            | ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes }
            | ExactMinimumCoverSessionAdvance::Found { visited_nodes, .. }
            | ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => visited_nodes,
            ExactMinimumCoverSessionAdvance::Finished => 0,
        };
        assert!(
            consumed <= 1,
            "one ABI call must enter at most one dual node"
        );
    }

    #[test]
    fn cooperative_exact_search_yields_after_one_actual_residual_proposal() {
        const ROW_COUNT: usize = 84;
        const CONSTRAINT_COUNT: usize = 128;
        let edges = [(1, 2), (3, 4), (0, 1), (2, 3), (4, 5), (1, 3)];
        let multiplicities = [21, 21, 21, 21, 21, 23];
        let mut rows = (0..ROW_COUNT)
            .map(|row| DenseRow {
                source_index: row,
                words: vec![0, 0],
            })
            .collect::<Vec<_>>();
        let mut pattern = 0_usize;
        for ((left, right), multiplicity) in edges.into_iter().zip(multiplicities) {
            for _ in 0..multiplicity {
                for (row_index, row) in rows.iter_mut().enumerate() {
                    let vertex = if row_index >= 78 {
                        usize::MAX
                    } else {
                        row_index % 6
                    };
                    if vertex == left || vertex == right {
                        row.words[pattern / u64::BITS as usize] |=
                            1_u64 << (pattern % u64::BITS as usize);
                    }
                }
                pattern += 1;
            }
        }
        assert_eq!(pattern, CONSTRAINT_COUNT);
        let rows_live = checked_dense_rows_retained_bytes(&rows).expect("rows live");
        let MinimumCoverSearchPreparation::Search(mut search) = MinimumCoverSearch::try_new(
            &rows,
            vec![u64::MAX, u64::MAX],
            vec![1; CONSTRAINT_COUNT],
            ExactCoverSearchGoal::Minimum,
            ExactCoverIncumbentPolicy::Standard,
            rows_live,
            &mut |_| Ok(()),
            &mut || false,
            false,
        )
        .expect("residual-yield search") else {
            panic!("residual-yield fixture must require exact search")
        };
        search.root_dual = None;
        search.support_pattern_order.clear();
        for row_index in 78..ROW_COUNT {
            search.selected[row_index] = true;
            search.current.push(row_index);
            union_words(&mut search.covered, &rows[row_index].words);
        }
        assert_eq!(search.current.len(), 6);
        assert!(search.best.capacity() >= 12);
        search.best.resize(12, 0);
        search.frames.push(MinimumCoverSearchFrame {
            saved_covered: Vec::new(),
            forced_rows: Vec::new(),
            branches: Vec::new(),
            next_branch: 0,
            active_branch: None,
        });
        let workspace = search
            .dual_workspace
            .as_mut()
            .expect("admitted residual workspace");
        workspace.set_remaining_iterations_for_test(1_000);

        let outcome = search
            .advance(
                &rows,
                MAX_EXACT_NODES_PER_ADVANCE,
                rows_live,
                &mut |_| Ok(()),
                &mut || false,
            )
            .expect("residual-yield advance");
        let remaining = search
            .dual_workspace
            .as_ref()
            .expect("retained residual workspace")
            .remaining_proposal_iterations();
        assert_eq!(
            outcome,
            MinimumCoverSearchAdvance::Pending {
                visited_nodes: 1,
                consumed_residual_dual: true,
            },
            "remaining proposal iterations: {remaining}"
        );
        assert_eq!(
            remaining, 800,
            "one full residual proposal is the only charged work"
        );
        let diagnostics = search.diagnostic_residual_progress();
        assert_eq!(diagnostics.proposal_attempts, 1);
        assert_eq!(diagnostics.proposal_iterations, 200);
        assert_eq!(
            diagnostics
                .proposal_attempts_by_dual_gap
                .iter()
                .sum::<u64>(),
            diagnostics.proposal_attempts
        );
        assert_eq!(
            diagnostics
                .proposal_iterations_by_dual_gap
                .iter()
                .sum::<u64>(),
            diagnostics.proposal_iterations
        );
        assert_eq!(
            diagnostics.certified_prunes_by_dual_gap.iter().sum::<u64>(),
            diagnostics.certified_prunes
        );
        assert_eq!(
            diagnostics.proposal_attempts_by_depth.iter().sum::<u64>(),
            diagnostics.proposal_attempts
        );
        assert_eq!(
            diagnostics.proposal_iterations_by_depth.iter().sum::<u64>(),
            diagnostics.proposal_iterations
        );
        assert_eq!(
            diagnostics.certified_prunes_by_depth.iter().sum::<u64>(),
            diagnostics.certified_prunes
        );
        assert_eq!(
            diagnostics
                .certified_prunes_by_checkpoint
                .iter()
                .sum::<u64>(),
            diagnostics.certified_prunes
        );
    }

    #[test]
    fn conditional_root_rows_filter_actual_pivot_without_changing_undo_or_witness() {
        let rows: Vec<_> = [0b0001, 0b0110, 0b1001, 0b1010, 0b0101, 0b0110, 0b1010]
            .into_iter()
            .enumerate()
            .map(|(source_index, word)| DenseRow {
                source_index,
                words: vec![word],
            })
            .collect();
        let rows_live = checked_dense_rows_retained_bytes(&rows).expect("rows live");
        let MinimumCoverSearchPreparation::Search(mut initial) = MinimumCoverSearch::try_new(
            &rows,
            vec![15],
            vec![1; 4],
            ExactCoverSearchGoal::Minimum,
            ExactCoverIncumbentPolicy::Standard,
            rows_live,
            &mut |_| Ok(()),
            &mut || false,
            false,
        )
        .expect("conditional pivot search") else {
            panic!("fixture must retain an exact search owner")
        };
        // Force the bounded decision through DFS instead of accepting the
        // constructor's greedy witness. This is test setup, never a product
        // incumbent or a claimed negative certificate.
        initial.goal = ExactCoverSearchGoal::AtMost(2);
        assert!(initial.best.capacity() >= 3);
        initial.best.resize(3, usize::MAX);
        let certificate = CertifiedResidualDual::from_checked_row_weights_for_test(
            &[0, 1, 1, 1],
            &rows
                .iter()
                .map(|row| row.words.as_slice())
                .collect::<Vec<_>>(),
        )
        .expect("all original rows satisfy the same integer capacity");
        initial.fixed_retained_bytes = initial
            .fixed_retained_bytes
            .checked_add(certificate.checked_retained_bytes().unwrap())
            .unwrap();
        assert!(initial.root_dual.is_none());
        initial.root_dual = Some(certificate);
        assert_eq!(initial.rarest_uncovered_pattern(&[0]), Some((0, 3)));

        let mut baseline = initial.clone();
        baseline.diagnostic_conditional_rows_enabled = false;
        let mut filtered = initial.clone();
        filtered.diagnostic_conditional_rows_enabled = true;
        let mut baseline_probe = baseline.clone();
        let mut filtered_probe = filtered.clone();
        let baseline_branches = baseline_probe
            .prepare_reduced_node(&rows, &[0], rows_live, 0, &mut |_| Ok(()))
            .unwrap()
            .expect("baseline pivot");
        let filtered_branches = filtered_probe
            .prepare_reduced_node(&rows, &[0], rows_live, 0, &mut |_| Ok(()))
            .unwrap()
            .expect("filtered pivot");
        assert_eq!(filtered_probe.diagnostic_conditional_rows.assessed_nodes, 1);
        assert_eq!(filtered_probe.diagnostic_conditional_rows.candidate_rows, 3);
        assert_eq!(filtered_probe.diagnostic_conditional_rows.pruned_rows, 1);
        assert!(!filtered_branches.contains(&0));
        // Existing dominance can independently remove this same row. Its
        // canonical survivor order must agree; no speedup is implied here.
        assert_eq!(filtered_branches, baseline_branches);
        assert_eq!(filtered_probe.excluded_rows, initial.excluded_rows);
        assert_eq!(filtered_probe.selected, initial.selected);
        assert_eq!(filtered_probe.current, initial.current);

        for search in [&mut baseline, &mut filtered] {
            for _ in 0..32 {
                if matches!(
                    search
                        .advance(&rows, 8, rows_live, &mut |_| Ok(()), &mut || false)
                        .unwrap(),
                    MinimumCoverSearchAdvance::Finished { .. }
                ) {
                    break;
                }
            }
            assert!(search.finished);
            assert_eq!(search.best.len(), 2);
            assert_eq!(
                search
                    .best
                    .iter()
                    .fold(0, |mask, row| mask | rows[*row].words[0]),
                15
            );
        }
        assert_eq!(filtered.best, baseline.best);
        let mut cancelled = initial;
        assert!(matches!(
            cancelled
                .advance(&rows, 8, rows_live, &mut |_| Ok(()), &mut || true)
                .unwrap(),
            MinimumCoverSearchAdvance::Cancelled {
                visited_nodes: 0,
                ..
            }
        ));
        assert!(
            !cancelled.finished,
            "cancellation cannot prove infeasibility"
        );
    }

    #[test]
    fn exact_branch_order_uses_quotient_multiplicity_weighted_residual_gain() {
        let rows = vec![
            DenseRow {
                source_index: 10,
                words: vec![0b011],
            },
            DenseRow {
                source_index: 0,
                words: vec![0b101],
            },
            DenseRow {
                source_index: 5,
                words: vec![0b110],
            },
        ];
        let rows_live = checked_dense_rows_retained_bytes(&rows).expect("rows live");
        let MinimumCoverSearchPreparation::Search(mut search) = MinimumCoverSearch::try_new(
            &rows,
            vec![0b111],
            vec![1, 100, 1],
            ExactCoverSearchGoal::Minimum,
            ExactCoverIncumbentPolicy::Standard,
            rows_live,
            &mut |_| Ok(()),
            &mut || false,
            false,
        )
        .expect("weighted branch search") else {
            panic!("weighted fixture must require exact search")
        };
        assert!(search.best.capacity() >= rows.len());
        search.best.resize(rows.len(), 0);
        let branches = search
            .prepare_reduced_node(&rows, &[0], rows_live, 0, &mut |_| Ok(()))
            .expect("weighted branch preparation")
            .expect("weighted fixture must branch");
        assert_eq!(branches, vec![0, 1]);
    }

    #[test]
    fn cooperative_exact_session_reverts_to_wide_batch_after_dual_exhaustion() {
        let (required, rows) = dual_admission_fixture();
        let mut session = prepare_lazy_exact_cover_search_session(
            &required,
            &rows,
            ExactCoverSearchGoal::AtMost(0),
            ExactCoverIncumbentPolicy::Standard,
            None,
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("cooperative dual session");
        for _ in 0..16 {
            if matches!(
                session.state,
                ExactCoverSearchSessionState::Searching { .. }
            ) {
                break;
            }
            assert!(matches!(
                session
                    .advance(1, &mut |_| Ok(()), &mut || false)
                    .expect("bounded dual preparation"),
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
            ));
        }
        let ExactCoverSearchSessionState::Searching { search, .. } = &mut session.state else {
            panic!("dual session must reach exact search")
        };
        search.root_dual = None;
        search.goal = ExactCoverSearchGoal::Minimum;
        let incumbent_capacity = search.best.capacity();
        assert!(incumbent_capacity >= rows.len());
        search.best.resize(rows.len(), 0);
        search
            .dual_workspace
            .as_mut()
            .expect("retained dual workspace")
            .set_remaining_iterations_for_test(0);

        let outcome = session
            .advance(u64::MAX, &mut |_| Ok(()), &mut || false)
            .expect("exhausted dual session advance");
        assert_eq!(
            outcome,
            ExactMinimumCoverSessionAdvance::Pending {
                visited_nodes: MAX_EXACT_NODES_PER_ADVANCE,
            },
            "an exhausted proposal owner must not serialize cheap DFS nodes"
        );
    }

    #[test]
    fn cooperative_witness_cursor_zero_budget_cancel_clone_and_guard_are_transactional() {
        let (required, rows, limit) = witness_assisted_fixture();
        let mut session = ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
            &required,
            &rows,
            limit,
            Some(&[]),
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("resumable witness session");
        assert_eq!(
            session
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("enter shortcut"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 1 }
        );
        let retained_before_zero = session
            .checked_retained_capacity_bytes()
            .expect("retained bytes");
        assert_eq!(
            session
                .advance(0, &mut |_| Ok(()), &mut || false)
                .expect("zero-budget cursor slice"),
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes: 0 }
        );
        assert_eq!(
            session.checked_retained_capacity_bytes(),
            Some(retained_before_zero),
            "zero budget must not mutate or restart the cursor"
        );

        for _ in 0..10_000 {
            if matches!(
                &session.state,
                ExactCoverSearchSessionState::WitnessShortcut(WitnessShortcutSession {
                    phase: WitnessShortcutPhase::Breakout { .. }
                        | WitnessShortcutPhase::Randomized { .. },
                    ..
                })
            ) {
                break;
            }
            assert!(matches!(
                session
                    .advance(1, &mut |_| Ok(()), &mut || false)
                    .expect("advance to retained heuristic cursor"),
                ExactMinimumCoverSessionAdvance::Pending { .. }
            ));
        }
        assert!(matches!(
            &session.state,
            ExactCoverSearchSessionState::WitnessShortcut(WitnessShortcutSession {
                phase: WitnessShortcutPhase::Breakout { .. }
                    | WitnessShortcutPhase::Randomized { .. },
                ..
            })
        ));

        let retained = session
            .checked_retained_capacity_bytes()
            .expect("cursor retained bytes");
        let external_live = 37_u128;
        let mut observed = 0_u128;
        let mut cloned = session
            .try_clone_with_memory_guard(external_live, &mut |whole_live| {
                observed = observed.max(whole_live);
                Ok(())
            })
            .expect("guarded cursor clone");
        assert!(observed >= external_live + retained);
        let max_memory_bytes = external_live + retained - 1;
        assert!(matches!(
            session.try_clone_with_memory_guard(external_live, &mut |whole_live| {
                if whole_live <= max_memory_bytes {
                    Ok(())
                } else {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: whole_live,
                        max_memory_bytes,
                    })
                }
            }),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded { .. })
        ));

        assert!(matches!(
            session
                .advance(1, &mut |_| Ok(()), &mut || true)
                .expect("cancel pending cursor"),
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes: 0 }
        ));
        assert_eq!(
            session
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("cancelled cursor is terminal"),
            ExactMinimumCoverSessionAdvance::Finished
        );

        let terminal = loop {
            match cloned
                .advance(1, &mut |_| Ok(()), &mut || false)
                .expect("cloned cursor continuation")
            {
                ExactMinimumCoverSessionAdvance::Pending { .. } => {}
                terminal => break terminal,
            }
        };
        assert!(matches!(
            terminal,
            ExactMinimumCoverSessionAdvance::Found { .. }
        ));
    }

    #[test]
    fn dual_preflight_capacity_rejection_falls_back_end_to_end() {
        let (required, rows) = dual_admission_fixture();
        let mut requests = Vec::new();
        let baseline = exact_cover_at_most_with_memory_guard(
            &required,
            &rows,
            rows.len(),
            &mut |owned_bytes| {
                requests.push(owned_bytes);
                Ok(())
            },
        )
        .expect("admitted dual baseline");
        assert!(baseline.is_some());

        let preflight_index = requests
            .iter()
            .enumerate()
            .max_by_key(|(_, owned_bytes)| *owned_bytes)
            .map(|(index, _)| index)
            .expect("memory requests");
        let preflight_peak = requests[preflight_index];
        assert_eq!(
            requests
                .iter()
                .filter(|owned_bytes| **owned_bytes == preflight_peak)
                .count(),
            1,
            "the conservative dual preflight must be distinguishable from actual capacities"
        );
        let mut call_index = 0_usize;
        let mut rejected = false;
        let fallback = exact_cover_at_most_with_memory_guard(
            &required,
            &rows,
            rows.len(),
            &mut |owned_bytes| {
                let reject = call_index == preflight_index;
                call_index += 1;
                if reject {
                    rejected = true;
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: owned_bytes,
                        max_memory_bytes: owned_bytes.saturating_sub(1),
                    })
                } else {
                    Ok(())
                }
            },
        )
        .expect("optional preflight rejection falls back");
        assert!(rejected);
        assert_eq!(fallback, baseline);

        // The workspace starts its first fallible reserve immediately after
        // the whole-workspace preflight: one guard call covers requested
        // capacity, and the next reports allocator-returned actual capacity.
        // Once allocation has begun, capacity rejection is intentionally not
        // optional and must abort the exact request fail-closed.
        let post_allocation_index = preflight_index + 2;
        let post_allocation_bytes = requests[post_allocation_index];
        let mut call_index = 0_usize;
        let post_allocation = exact_cover_at_most_with_memory_guard(
            &required,
            &rows,
            rows.len(),
            &mut |owned_bytes| {
                let reject = call_index == post_allocation_index;
                call_index += 1;
                if reject {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: owned_bytes,
                        max_memory_bytes: owned_bytes.saturating_sub(1),
                    })
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            post_allocation,
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes: post_allocation_bytes,
                max_memory_bytes: post_allocation_bytes - 1,
            })
        );
    }

    #[test]
    fn optional_dual_preflight_and_post_allocation_errors_have_distinct_contracts() {
        let allocation =
            optional_dual_acceleration::<()>(Err(ExactMinimumCoverError::AllocationFailed {
                component: "synthetic_dual",
            }));
        assert_eq!(allocation, Ok(None));
        assert_eq!(
            optional_dual_acceleration::<()>(Err(ExactMinimumCoverError::ProjectionOverflow)),
            Ok(None)
        );
        let capacity_error = ExactMinimumCoverError::MemoryCapacityExceeded {
            required_memory_bytes: 2,
            max_memory_bytes: 1,
        };
        assert_eq!(optional_dual_preflight(Err(capacity_error)), Ok(false));
        assert_eq!(
            optional_dual_acceleration::<()>(Err(capacity_error)),
            Err(capacity_error),
            "post-allocation actual-capacity rejection must remain fail-closed"
        );
        assert_eq!(
            optional_dual_preflight(Err(ExactMinimumCoverError::MemoryGuardRejected)),
            Err(ExactMinimumCoverError::MemoryGuardRejected)
        );
        assert_eq!(
            optional_dual_acceleration::<()>(Err(ExactMinimumCoverError::MemoryGuardRejected)),
            Err(ExactMinimumCoverError::MemoryGuardRejected)
        );
    }

    #[test]
    fn guarded_exact_solver_accepts_observed_peak_and_respects_tighter_capacity() {
        let (required, rows) = exact_fixture();
        let mut peak = 0_u128;
        let expected =
            exact_minimum_cover_with_memory_guard(&required, &rows, &mut |owned_bytes| {
                peak = peak.max(owned_bytes);
                Ok(())
            })
            .expect("dry run");
        assert!(peak > 0);

        let already_retained_bytes = 37_u128;
        let exact_cap = already_retained_bytes.checked_add(peak).expect("cap");
        let exact = exact_minimum_cover_with_memory_limit(
            &required,
            &rows,
            already_retained_bytes,
            exact_cap,
        )
        .expect("exact observed cap");
        assert_eq!(exact, expected);

        // The first pass includes optional geometric memo spare capacity.
        // Rejecting that preflight can legitimately fit the exact next entry;
        // every actual post-allocation capacity is still rechecked. The
        // dedicated memo test above verifies its retained byte count exactly.
        let mut rejected_preflights = 0;
        let mut admitted_peak = 0;
        let tighter = exact_minimum_cover_with_memory_guard(&required, &rows, &mut |owned| {
            let requested = already_retained_bytes + owned;
            if requested >= exact_cap {
                rejected_preflights += 1;
                return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                    required_memory_bytes: requested,
                    max_memory_bytes: exact_cap - 1,
                });
            }
            admitted_peak = admitted_peak.max(requested);
            Ok(())
        })
        .expect("required state fits after optional memo spare-capacity rejection");
        assert_eq!(tighter, expected);
        assert!(rejected_preflights > 0);
        assert!(admitted_peak < exact_cap);
        assert_eq!(
            exact_minimum_cover_with_memory_guard(&required, &rows, &mut |owned| {
                if owned >= peak {
                    Err(ExactMinimumCoverError::MemoryGuardRejected)
                } else {
                    Ok(())
                }
            }),
            Err(ExactMinimumCoverError::MemoryGuardRejected),
            "an explicit hard guard must not be swallowed as optional spare capacity"
        );
        assert!(matches!(
            exact_minimum_cover_with_memory_limit(
                &required,
                &rows,
                already_retained_bytes,
                already_retained_bytes
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded { .. }),
        ));
    }

    #[test]
    fn guarded_exact_solver_noop_reports_zero_future_only() {
        let required = PatternBitSet::new(0);
        let mut observed = Vec::new();
        let result = exact_minimum_cover_with_memory_guard(&required, &[], &mut |owned_bytes| {
            observed.push(owned_bytes);
            Ok(())
        })
        .expect("empty exact cover");
        assert!(result.complete());
        assert_eq!(observed, vec![0]);
    }

    #[test]
    fn state_projection_and_external_addition_overflow_fail_closed() {
        assert_eq!(
            checked_exact_minimum_cover_state_upper_bound(4, 9),
            Some(16)
        );
        assert_eq!(
            checked_exact_minimum_cover_state_upper_bound(u128::BITS as usize, usize::MAX),
            None
        );

        let (required, rows) = exact_fixture();
        assert_eq!(
            exact_minimum_cover_with_memory_limit(&required, &rows, u128::MAX, u128::MAX),
            Err(ExactMinimumCoverError::ProjectionOverflow)
        );
    }

    #[test]
    fn checked_projection_includes_every_reachable_memo_state_request() {
        let (required, rows) = exact_fixture();
        let projection = checked_exact_minimum_cover_memory_projection(&required, &rows)
            .expect("representable projection");
        assert_eq!(projection.memo_state_upper_bound, 16);
        assert!(projection.memo_state_bytes_upper_bound > 0);
        assert_eq!(
            projection.required_peak_bytes,
            projection
                .fixed_workspace_bytes
                .checked_add(projection.memo_state_bytes_upper_bound)
                .expect("projection sum")
        );
    }

    #[test]
    fn projection_covers_fixed_point_row_and_constraint_overlap_peak() {
        let required = PatternBitSet::from_patterns(
            4,
            [
                PatternId::new(0),
                PatternId::new(1),
                PatternId::new(2),
                PatternId::new(3),
            ],
        )
        .expect("required");
        // Constraint 1 makes 0 redundant and constraint 3 makes 2
        // redundant. Row 0 consequently becomes empty only after the first
        // column reduction, exercising the fixed-point overlap path.
        let rows = vec![
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)]).expect("row 0"),
            PatternBitSet::from_patterns(4, [PatternId::new(2), PatternId::new(3)]).expect("row 1"),
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)]).expect("row 2"),
        ];
        let projection = checked_exact_minimum_cover_memory_projection(&required, &rows)
            .expect("representable projection");
        let mut observed_peak = 0_u128;
        let result = exact_minimum_cover_with_memory_guard(&required, &rows, &mut |owned_bytes| {
            observed_peak = observed_peak.max(owned_bytes);
            Ok(())
        })
        .expect("fixed-point exact cover");

        assert_eq!(result.row_indices(), &[1, 2]);
        assert!(observed_peak <= projection.required_peak_bytes);
    }

    #[test]
    fn rare_bit_dominance_index_matches_naive_all_pairs_for_small_matrices() {
        const PATTERN_COUNT: usize = 3;
        const ROW_COUNT: usize = 4;
        const ROW_VARIANTS: usize = 1 << PATTERN_COUNT;
        let matrix_count = ROW_VARIANTS.pow(ROW_COUNT as u32);

        for encoded_rows in 0..matrix_count {
            let mut value = encoded_rows;
            let mut rows = Vec::new();
            for source_index in 0..ROW_COUNT {
                let row_mask = value % ROW_VARIANTS;
                value /= ROW_VARIANTS;
                if row_mask == 0 {
                    continue;
                }
                rows.push(DenseRow {
                    source_index,
                    words: vec![row_mask as u64],
                });
            }
            let mut expected = rows.clone();
            naive_remove_dominated_rows(&mut expected);
            let mut peak = 0_u128;

            remove_dominated_rows_with_memory_guard(&mut rows, &mut |owned_bytes| {
                peak = peak.max(owned_bytes);
                Ok(())
            })
            .expect("indexed dominance reduction");

            assert_eq!(
                rows.iter()
                    .map(|row| (row.source_index, row.words.clone()))
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|row| (row.source_index, row.words.clone()))
                    .collect::<Vec<_>>()
            );
            if !rows.is_empty() {
                assert!(peak > 0);
            }
        }
    }

    #[test]
    fn exact_solver_matches_brute_force_for_all_three_bit_four_row_matrices() {
        const PATTERN_COUNT: usize = 3;
        const ROW_COUNT: usize = 4;
        const ROW_VARIANTS: usize = 1 << PATTERN_COUNT;
        let required =
            PatternBitSet::from_patterns(PATTERN_COUNT, (0..PATTERN_COUNT).map(PatternId::new))
                .expect("required");

        for encoded_rows in 0..ROW_VARIANTS.pow(ROW_COUNT as u32) {
            let mut value = encoded_rows;
            let mut row_masks = [0_u64; ROW_COUNT];
            let mut rows = Vec::with_capacity(ROW_COUNT);
            for row_mask in &mut row_masks {
                *row_mask = (value % ROW_VARIANTS) as u64;
                value /= ROW_VARIANTS;
                rows.push(
                    PatternBitSet::from_patterns(
                        PATTERN_COUNT,
                        (0..PATTERN_COUNT)
                            .filter(|bit| *row_mask & (1_u64 << bit) != 0)
                            .map(PatternId::new),
                    )
                    .expect("row"),
                );
            }
            let brute = (0_usize..(1_usize << ROW_COUNT))
                .filter_map(|selected| {
                    let covered = row_masks.iter().copied().enumerate().fold(
                        0_u64,
                        |covered, (row_index, row)| {
                            if selected & (1_usize << row_index) == 0 {
                                covered
                            } else {
                                covered | row
                            }
                        },
                    );
                    (covered == (1_u64 << PATTERN_COUNT) - 1)
                        .then_some(selected.count_ones() as usize)
                })
                .min();
            let exact = exact_minimum_cover(&required, &rows).expect("exact solver");
            assert_eq!(
                exact.complete().then_some(exact.row_indices().len()),
                brute,
                "encoded_rows={encoded_rows} row_masks={row_masks:?} exact={exact:?}"
            );
        }
    }

    #[test]
    fn cover_at_most_matches_brute_force_and_replays_original_rows_exhaustively() {
        const PATTERN_COUNT: usize = 3;
        const ROW_COUNT: usize = 4;
        const ROW_VARIANTS: usize = 1 << PATTERN_COUNT;
        const REQUIRED_MASK: u64 = (1 << PATTERN_COUNT) - 1;
        let required =
            PatternBitSet::from_patterns(PATTERN_COUNT, (0..PATTERN_COUNT).map(PatternId::new))
                .expect("required");

        for encoded_rows in 0..ROW_VARIANTS.pow(ROW_COUNT as u32) {
            let mut value = encoded_rows;
            let mut row_masks = [0_u64; ROW_COUNT];
            let mut rows = Vec::with_capacity(ROW_COUNT);
            for row_mask in &mut row_masks {
                *row_mask = (value % ROW_VARIANTS) as u64;
                value /= ROW_VARIANTS;
                rows.push(
                    PatternBitSet::from_patterns(
                        PATTERN_COUNT,
                        (0..PATTERN_COUNT)
                            .filter(|bit| *row_mask & (1_u64 << bit) != 0)
                            .map(PatternId::new),
                    )
                    .expect("row"),
                );
            }
            for limit in 0..=ROW_COUNT {
                let brute_exists = (0_usize..(1_usize << ROW_COUNT)).any(|selected| {
                    if selected.count_ones() as usize > limit {
                        return false;
                    }
                    row_masks.iter().copied().enumerate().fold(
                        0_u64,
                        |covered, (row_index, row)| {
                            if selected & (1_usize << row_index) == 0 {
                                covered
                            } else {
                                covered | row
                            }
                        },
                    ) == REQUIRED_MASK
                });
                let exact = exact_cover_at_most(&required, &rows, limit)
                    .expect("bounded exact cover search");
                assert_eq!(
                    exact.is_some(),
                    brute_exists,
                    "encoded_rows={encoded_rows} row_masks={row_masks:?} limit={limit}"
                );
                if let Some(exact) = exact {
                    assert!(exact.row_indices().len() <= limit);
                    assert!(exact.row_indices().windows(2).all(|pair| pair[0] < pair[1]));
                    let replay = exact
                        .row_indices()
                        .iter()
                        .try_fold(PatternBitSet::new(PATTERN_COUNT), |mut replay, row| {
                            replay.union_with(&rows[*row]).ok()?;
                            Some(replay)
                        })
                        .expect("original-row replay");
                    assert!(replay.is_superset(&required).expect("same universe"));
                    assert_eq!(exact.covered_patterns(), &replay);
                }
            }
        }
    }

    #[test]
    fn cover_at_most_accepts_augmented_synthetic_constraints() {
        let required = PatternBitSet::from_patterns(
            4,
            [
                PatternId::new(0),
                PatternId::new(1),
                PatternId::new(2),
                PatternId::new(3),
            ],
        )
        .expect("augmented target");
        let rows = vec![
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(3)])
                .expect("range row 0"),
            PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(3)])
                .expect("range row 1"),
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
                .expect("outside range"),
            PatternBitSet::from_patterns(4, [PatternId::new(2)]).expect("tail row"),
        ];
        let hit = exact_cover_at_most(&required, &rows, 3)
            .expect("augmented exact search")
            .expect("three-row augmented cover");
        assert_eq!(hit.row_indices().len(), 3);
        assert!(hit.row_indices().iter().any(|row| *row <= 1));
        assert!(
            exact_cover_at_most(&required, &rows, 2)
                .expect("augmented negative proof")
                .is_none()
        );
    }

    #[test]
    fn cover_at_most_obeys_the_observed_memory_peak() {
        let (required, rows) = exact_fixture();
        let mut peak = 0_u128;
        let expected =
            exact_cover_at_most_with_memory_guard(&required, &rows, 2, &mut |owned_bytes| {
                peak = peak.max(owned_bytes);
                Ok(())
            })
            .expect("dry run");
        assert!(expected.is_some());
        assert!(peak > 0);

        let exact =
            exact_cover_at_most_with_memory_guard(&required, &rows, 2, &mut |owned_bytes| {
                if owned_bytes <= peak {
                    Ok(())
                } else {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: owned_bytes,
                        max_memory_bytes: peak,
                    })
                }
            })
            .expect("exact observed peak");
        assert_eq!(exact, expected);

        let max_memory_bytes = peak - 1;
        assert!(matches!(
            exact_cover_at_most_with_memory_guard(
                &required,
                &rows,
                2,
                &mut |owned_bytes| {
                    if owned_bytes <= max_memory_bytes {
                        Ok(())
                    } else {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: owned_bytes,
                            max_memory_bytes,
                        })
                    }
                },
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes: rejected,
            }) if required_memory_bytes > rejected
        ));
    }

    #[test]
    fn cancellable_cover_at_most_unwinds_and_retries_with_identical_authority() {
        let (required, rows) = exact_fixture();
        let baseline = exact_cover_at_most_with_control(&required, &rows, 1, &mut || false)
            .expect("negative baseline");
        assert_eq!(baseline, ExactCoverAtMostDecision::ProvedNone);

        let mut saw_cancelled = false;
        let mut saw_terminal = false;
        for cancellation_at in 1..=64 {
            let mut calls = 0_usize;
            let decision = exact_cover_at_most_with_control(&required, &rows, 1, &mut || {
                calls += 1;
                calls == cancellation_at
            })
            .expect("cancellable bounded decision");
            match decision {
                ExactCoverAtMostDecision::Cancelled => {
                    saw_cancelled = true;
                    let retried =
                        exact_cover_at_most_with_control(&required, &rows, 1, &mut || false)
                            .expect("retry after cancellation");
                    assert_eq!(retried, baseline);
                }
                terminal => {
                    assert_eq!(terminal, baseline);
                    saw_terminal = true;
                    break;
                }
            }
        }
        assert!(saw_cancelled);
        assert!(saw_terminal);
    }

    #[test]
    fn cancellation_cleans_unit_propagation_before_a_negative_retry() {
        let required =
            PatternBitSet::from_patterns(7, (0..7).map(PatternId::new)).expect("required");
        // Row 0 is forced by bit 6. Bits 0..5 are the six edges of K4 and the
        // other rows are its vertices. Their minimum cover is three although
        // the cheap gain bound is two. A total limit of three must therefore
        // branch after the forced-row mutation before proving infeasibility.
        let rows = vec![
            PatternBitSet::from_patterns(7, [PatternId::new(6)]).expect("forced row"),
            PatternBitSet::from_patterns(7, [0, 1, 2].into_iter().map(PatternId::new))
                .expect("vertex 0"),
            PatternBitSet::from_patterns(7, [0, 3, 4].into_iter().map(PatternId::new))
                .expect("vertex 1"),
            PatternBitSet::from_patterns(7, [1, 3, 5].into_iter().map(PatternId::new))
                .expect("vertex 2"),
            PatternBitSet::from_patterns(7, [2, 4, 5].into_iter().map(PatternId::new))
                .expect("vertex 3"),
        ];
        let mut callback_calls = 0_usize;
        let cancelled = exact_cover_at_most_with_control(&required, &rows, 3, &mut || {
            callback_calls += 1;
            callback_calls == 3
        })
        .expect("cancelled negative decision");
        assert_eq!(cancelled, ExactCoverAtMostDecision::Cancelled);

        let retried = exact_cover_at_most_with_control(&required, &rows, 3, &mut || false)
            .expect("negative retry");
        assert_eq!(retried, ExactCoverAtMostDecision::ProvedNone);
    }

    #[test]
    fn witness_assisted_at_most_is_deterministic_and_replays_original_rows() {
        let (required, rows, limit) = witness_assisted_fixture();
        let run = || {
            exact_cover_at_most_with_witness_search_memory_guard_and_control(
                &required,
                &rows,
                limit,
                &[],
                &mut |_| Ok(()),
                &mut || false,
            )
            .expect("witness-assisted decision")
        };
        let first = run();
        let second = run();
        assert_eq!(first, second);
        let ExactCoverAtMostDecision::Found(result) = first else {
            panic!("fixture has an at-most-{limit} cover");
        };
        assert!(result.row_indices().len() <= limit);
        let replay = result
            .row_indices()
            .iter()
            .try_fold(
                PatternBitSet::new(required.pattern_count()),
                |mut replay, row| {
                    replay.union_with(&rows[*row]).ok()?;
                    Some(replay)
                },
            )
            .expect("original-row replay");
        assert!(replay.is_superset(&required).expect("same universe"));
        assert_eq!(result.covered_patterns(), &replay);
    }

    #[test]
    fn witness_assisted_shortcut_hits_before_original_row_reduction() {
        let (required, rows, limit) = witness_assisted_fixture();
        let dense_rows = rows
            .iter()
            .enumerate()
            .map(|(source_index, row)| DenseRow {
                source_index,
                words: (0..required.word_count())
                    .map(|word| row.word_at(word) & required.word_at(word))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let base_live = checked_dense_rows_retained_bytes(&dense_rows).expect("dense bytes");
        let mut peak = 0_u128;
        let decision = witness_assisted_cover_before_dominance(
            &required,
            &dense_rows,
            limit,
            &[],
            base_live,
            &mut |whole_live| {
                peak = peak.max(whole_live);
                Ok(())
            },
            &mut || false,
        )
        .expect("original-row shortcut");
        let WitnessAssistedCoverDecision::Found(selected) = decision else {
            panic!("deterministic original-row shortcut must find the bounded witness");
        };
        assert!(selected.len() <= limit);
        let replay = selected.iter().fold(
            PatternBitSet::new(required.pattern_count()),
            |mut replay, row| {
                replay
                    .union_with(&rows[dense_rows[*row].source_index])
                    .expect("same universe");
                replay
            },
        );
        assert!(replay.is_superset(&required).expect("same universe"));
        let expected_forced_search_live = base_live
            + (required.word_count() * core::mem::size_of::<u64>()) as u128
            + (required.word_count() * u64::BITS as usize * core::mem::size_of::<usize>()) as u128
            + (rows.len() * core::mem::size_of::<usize>()) as u128
            + (2 * core::mem::size_of::<usize>()) as u128
            + (required.word_count() * core::mem::size_of::<u64>()) as u128
            + (rows.len() * core::mem::size_of::<bool>()) as u128
            + (rows.len() * core::mem::size_of::<usize>()) as u128;
        assert!(
            peak >= expected_forced_search_live,
            "the whole-live peak must include target/weights, incumbent, rare-pivot supporters, and the simultaneously live forced greedy scratch"
        );

        let max_memory_bytes = peak - 1;
        assert!(matches!(
            witness_assisted_cover_before_dominance(
                &required,
                &dense_rows,
                limit,
                &[],
                base_live,
                &mut |whole_live| {
                    if whole_live <= max_memory_bytes {
                        Ok(())
                    } else {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: whole_live,
                            max_memory_bytes,
                        })
                    }
                },
                &mut || false,
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes: rejected,
            }) if required_memory_bytes > rejected
        ));
    }

    #[test]
    fn rare_constraint_supporters_preserve_original_numeric_identity_order() {
        let rows = vec![
            DenseRow {
                source_index: 7,
                words: vec![0b10],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b11],
            },
            DenseRow {
                source_index: 5,
                words: vec![0b11],
            },
        ];
        let supporters =
            minimum_support_constraint_rows_with_memory_guard(&rows, &[0b11], 0, &mut |_| Ok(()))
                .expect("rarest support")
                .expect("non-empty target");
        assert_eq!(supporters.row_indices, vec![1, 2]);
        assert_eq!(
            supporters
                .row_indices
                .iter()
                .map(|row| rows[*row].source_index)
                .collect::<Vec<_>>(),
            vec![2, 5]
        );
    }

    #[test]
    fn witness_unique_missing_constraint_precedes_an_unrelated_rarer_constraint() {
        let rows = vec![
            DenseRow {
                source_index: 9,
                words: vec![0b100],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b011],
            },
            DenseRow {
                source_index: 5,
                words: vec![0b110],
            },
        ];
        let globally_rarest =
            minimum_support_constraint_rows_with_memory_guard(&rows, &[0b111], 0, &mut |_| Ok(()))
                .expect("global rarest support")
                .expect("non-empty target");
        assert_eq!(globally_rarest.bit, 0b001);

        let preferred = witness_unique_missing_constraint_rows_with_memory_guard(
            &rows,
            &[0b111],
            1,
            &[2],
            0,
            &mut |_| Ok(()),
        )
        .expect("witness missing support")
        .expect("one missing constraint");
        assert_eq!(preferred.word_index, 0);
        assert_eq!(preferred.bit, 0b100);
        assert_eq!(preferred.row_indices, vec![2, 0]);
        assert_eq!(
            preferred
                .row_indices
                .iter()
                .map(|row| rows[*row].source_index)
                .collect::<Vec<_>>(),
            vec![5, 9]
        );
    }

    #[test]
    fn witness_preferred_constraint_rejects_noncanonical_or_multi_missing_hints() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b100],
            },
        ];
        assert!(
            witness_unique_missing_constraint_rows_with_memory_guard(
                &rows,
                &[0b111],
                1,
                &[0],
                0,
                &mut |_| Ok(()),
            )
            .expect("multi-missing hint")
            .is_none()
        );
        assert!(
            witness_unique_missing_constraint_rows_with_memory_guard(
                &rows,
                &[0b111],
                2,
                &[1, 0],
                0,
                &mut |_| Ok(()),
            )
            .expect("out-of-order hint")
            .is_none()
        );
        assert!(
            witness_unique_missing_constraint_rows_with_memory_guard(
                &rows,
                &[0b111],
                1,
                &[3],
                0,
                &mut |_| Ok(()),
            )
            .expect("out-of-range hint")
            .is_none()
        );
    }

    #[test]
    fn witness_missing_constraint_masks_outside_bits_and_crosses_word_boundary() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![(1_u64 << 5) | (1_u64 << 20), 1_u64 << 1],
            },
            DenseRow {
                source_index: 1,
                words: vec![0, 1],
            },
            DenseRow {
                source_index: 2,
                words: vec![0, 1],
            },
        ];
        let preferred = witness_unique_missing_constraint_rows_with_memory_guard(
            &rows,
            &[1_u64 << 5, 1],
            1,
            &[0],
            0,
            &mut |_| Ok(()),
        )
        .expect("word-boundary witness")
        .expect("one required bit is missing");
        assert_eq!(preferred.word_index, 1);
        assert_eq!(preferred.bit, 1);
        assert_eq!(preferred.row_indices, vec![1, 2]);

        assert!(
            witness_unique_missing_constraint_rows_with_memory_guard(
                &rows,
                &[1_u64 << 5, 1_u64 << 1],
                1,
                &[0],
                0,
                &mut |_| Ok(()),
            )
            .expect("fully covered hint")
            .is_none()
        );
    }

    #[test]
    fn preferred_witness_breakout_has_a_global_large_support_work_ceiling() {
        let mut rows = vec![DenseRow {
            source_index: 0,
            words: vec![0b01],
        }];
        rows.extend((1..=256).map(|source_index| DenseRow {
            source_index,
            words: vec![0b10],
        }));
        let preferred = witness_unique_missing_constraint_rows_with_memory_guard(
            &rows,
            &[0b11],
            1,
            &[0],
            0,
            &mut |_| Ok(()),
        )
        .expect("large-support preferred pivot")
        .expect("one required bit is missing");
        assert_eq!(preferred.row_indices.len(), 256);
        let total = preferred
            .row_indices
            .iter()
            .enumerate()
            .map(|(position, _)| {
                preferred_witness_breakout_budget(preferred.row_indices.len(), position)
            })
            .sum::<usize>();
        assert_eq!(total, WITNESS_ASSISTED_PREFERRED_TOTAL_BREAKOUT_SWAP_BUDGET);
        assert!(
            preferred
                .row_indices
                .iter()
                .enumerate()
                .all(|(position, _)| preferred_witness_breakout_budget(
                    preferred.row_indices.len(),
                    position
                ) <= WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET)
        );
    }

    #[test]
    fn witness_assisted_uncoverable_constraint_falls_through_as_a_miss() {
        let required = PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
            .expect("required");
        let rows = vec![DenseRow {
            source_index: 0,
            words: vec![0b01],
        }];
        assert!(matches!(
            witness_assisted_cover_before_dominance(
                &required,
                &rows,
                1,
                &[],
                checked_dense_rows_retained_bytes(&rows).expect("dense bytes"),
                &mut |_| Ok(()),
                &mut || false,
            )
            .expect("non-authoritative shortcut"),
            WitnessAssistedCoverDecision::Miss
        ));
    }

    #[test]
    fn forced_randomized_completion_keeps_the_selected_supporter() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b100],
            },
            DenseRow {
                source_index: 3,
                words: vec![0b110],
            },
        ];
        let mut best = vec![0, 1, 2];
        let outcome = improve_randomized_compact_cover_with_memory_guard(
            &rows,
            &[0b111],
            &mut best,
            16,
            WITNESS_ASSISTED_RANDOM_SEED,
            Some(2),
            Some(0),
            0,
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("forced randomized completion");
        assert_eq!(outcome, IncumbentSearchOutcome::FoundAtMost);
        assert_eq!(best.len(), 2);
        assert!(best.contains(&0));
        let covered = best
            .iter()
            .fold(0_u64, |covered, row| covered | rows[*row].words[0]);
        assert_eq!(covered & 0b111, 0b111);
    }

    #[test]
    fn warm_hint_is_strictly_replayed_and_breakout_keeps_the_forced_supporter() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b101],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b110],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 3,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 4,
                words: vec![0b010],
            },
        ];
        let target = [0b111];
        let base_live = 31_u128;
        let mut warm_peak = 0_u128;
        let seed = warm_seed_with_forced_supporter_memory_guard(
            &rows,
            &target,
            2,
            &[2, 3],
            0,
            0b100,
            0,
            base_live,
            &mut |whole_live| {
                warm_peak = warm_peak.max(whole_live);
                Ok(())
            },
        )
        .expect("warm seed allocation")
        .expect("warm seed replay");
        assert_eq!(seed, vec![0, 2, 3]);
        assert!(warm_peak > base_live);

        assert!(
            warm_seed_with_forced_supporter_memory_guard(
                &rows,
                &target,
                2,
                &[3, 2],
                0,
                0b100,
                0,
                base_live,
                &mut |_| Ok(()),
            )
            .expect("invalid-order hint is non-authoritative")
            .is_none()
        );
        assert!(
            warm_seed_with_forced_supporter_memory_guard(
                &rows,
                &target,
                2,
                &[0, 2],
                0,
                0b100,
                0,
                base_live,
                &mut |_| Ok(()),
            )
            .expect("incomplete hint is non-authoritative")
            .is_none()
        );

        let max_memory_bytes = warm_peak - 1;
        assert!(matches!(
            warm_seed_with_forced_supporter_memory_guard(
                &rows,
                &target,
                2,
                &[2, 3],
                0,
                0b100,
                0,
                base_live,
                &mut |whole_live| {
                    if whole_live <= max_memory_bytes {
                        Ok(())
                    } else {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: whole_live,
                            max_memory_bytes,
                        })
                    }
                },
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes: rejected,
            }) if required_memory_bytes > rejected
        ));

        let support =
            build_support_by_pattern_with_memory_guard(&rows, &target, base_live, &mut |_| Ok(()))
                .expect("support index");
        let mut cancelled_seed = seed.clone();
        assert!(
            improve_fixed_cardinality_cover_with_memory_guard(
                &rows,
                &target,
                &support,
                &mut cancelled_seed,
                WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
                Some(0),
                base_live,
                &mut |_| Ok(()),
                &mut || true,
            )
            .expect("cancelled warm breakout")
        );
        assert_eq!(cancelled_seed, seed);

        let mut completed = seed;
        assert!(
            !improve_fixed_cardinality_cover_with_memory_guard(
                &rows,
                &target,
                &support,
                &mut completed,
                WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
                Some(0),
                base_live,
                &mut |_| Ok(()),
                &mut || false,
            )
            .expect("warm breakout retry")
        );
        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&0));
        assert_eq!(
            completed
                .iter()
                .fold(0_u64, |covered, row| covered | rows[*row].words[0])
                & target[0],
            target[0]
        );
    }

    #[test]
    fn witness_assisted_at_most_cancels_inside_incumbent_search_without_authority() {
        let (required, rows, limit) = witness_assisted_fixture();
        let mut polls = 0_usize;
        let cancelled = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            limit,
            &[],
            &mut |_| Ok(()),
            &mut || {
                polls += 1;
                polls == 2
            },
        )
        .expect("cancelled witness-assisted decision");
        assert_eq!(cancelled, ExactCoverAtMostDecision::Cancelled);

        let retried = exact_cover_at_most_with_witness_search_memory_guard_and_control(
            &required,
            &rows,
            limit,
            &[],
            &mut |_| Ok(()),
            &mut || false,
        )
        .expect("retry after witness-search cancellation");
        assert!(matches!(retried, ExactCoverAtMostDecision::Found(_)));
    }

    #[test]
    fn exact_solver_matches_brute_force_for_deterministic_larger_matrices() {
        const PATTERN_COUNT: usize = 6;
        const ROW_COUNT: usize = 8;
        const ALL_PATTERNS: u64 = (1_u64 << PATTERN_COUNT) - 1;
        let required =
            PatternBitSet::from_patterns(PATTERN_COUNT, (0..PATTERN_COUNT).map(PatternId::new))
                .expect("required");
        let mut random = 0x9e37_79b9_7f4a_7c15_u64;

        for case_index in 0..20_000_usize {
            let mut row_masks = [0_u64; ROW_COUNT];
            let mut rows = Vec::with_capacity(ROW_COUNT);
            for row_mask in &mut row_masks {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                *row_mask = random & ALL_PATTERNS;
                rows.push(
                    PatternBitSet::from_patterns(
                        PATTERN_COUNT,
                        (0..PATTERN_COUNT)
                            .filter(|bit| *row_mask & (1_u64 << bit) != 0)
                            .map(PatternId::new),
                    )
                    .expect("row"),
                );
            }
            let brute = (0_usize..(1_usize << ROW_COUNT))
                .filter_map(|selected| {
                    let covered = row_masks.iter().copied().enumerate().fold(
                        0_u64,
                        |covered, (row_index, row)| {
                            if selected & (1_usize << row_index) == 0 {
                                covered
                            } else {
                                covered | row
                            }
                        },
                    );
                    (covered == ALL_PATTERNS).then_some(selected.count_ones() as usize)
                })
                .min();
            let exact = exact_minimum_cover(&required, &rows).expect("exact solver");
            assert_eq!(
                exact.complete().then_some(exact.row_indices().len()),
                brute,
                "case_index={case_index} row_masks={row_masks:?} exact={exact:?}"
            );
        }
    }

    #[test]
    fn randomized_incumbent_improvement_is_deterministic_and_exactly_replayed() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b100],
            },
            DenseRow {
                source_index: 3,
                words: vec![0b011],
            },
        ];
        let target = [0b111];
        let improve = || {
            let mut best = vec![0, 1, 2];
            improve_randomized_compact_cover_with_memory_guard(
                &rows,
                &target,
                &mut best,
                RANDOMIZED_COMPACT_COVER_TRIALS,
                RANDOMIZED_COMPACT_COVER_SEED,
                None,
                None,
                0,
                &mut |_| Ok(()),
                &mut || false,
            )
            .expect("deterministic incumbent improvement");
            best
        };

        let first = improve();
        let second = improve();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        let covered = first.iter().fold(0_u64, |covered, row_index| {
            covered | rows[*row_index].words[0]
        });
        assert_eq!(covered & target[0], target[0]);
    }

    #[test]
    fn randomized_at_most_shortcut_stops_on_its_first_replayed_witness() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b100],
            },
            DenseRow {
                source_index: 3,
                words: vec![0b011],
            },
        ];
        let target = [0b111];
        let mut best = vec![0, 1, 2];
        let mut cancellation_polls = 0_usize;
        let outcome = improve_randomized_compact_cover_with_memory_guard(
            &rows,
            &target,
            &mut best,
            10_000,
            WITNESS_ASSISTED_RANDOM_SEED,
            Some(2),
            None,
            0,
            &mut |_| Ok(()),
            &mut || {
                cancellation_polls += 1;
                false
            },
        )
        .expect("bounded positive shortcut");
        assert_eq!(outcome, IncumbentSearchOutcome::FoundAtMost);
        assert_eq!(best.len(), 2);
        assert!(cancellation_polls < 10_000);
        let replay = best
            .iter()
            .fold(0_u64, |covered, row| covered | rows[*row].words[0]);
        assert_eq!(replay & target[0], target[0]);
    }

    #[test]
    fn breakout_incumbent_improvement_obeys_the_observed_memory_peak() {
        let rows = vec![
            DenseRow {
                source_index: 0,
                words: vec![0b001],
            },
            DenseRow {
                source_index: 1,
                words: vec![0b010],
            },
            DenseRow {
                source_index: 2,
                words: vec![0b100],
            },
            DenseRow {
                source_index: 3,
                words: vec![0b011],
            },
        ];
        let target = [0b111];
        let support_by_pattern = vec![vec![0, 3], vec![1, 3], vec![2]];
        let base_live_bytes = 37_u128;
        let mut best = vec![0, 1, 2];
        let mut observed_peak = 0_u128;
        improve_fixed_cardinality_cover_with_memory_guard(
            &rows,
            &target,
            &support_by_pattern,
            &mut best,
            BREAKOUT_TOTAL_SWAP_BUDGET,
            None,
            base_live_bytes,
            &mut |owned_bytes| {
                observed_peak = observed_peak.max(owned_bytes);
                Ok(())
            },
            &mut || false,
        )
        .expect("breakout dry run");
        assert_eq!(best.len(), 2);
        assert!(observed_peak > base_live_bytes);
        let covered = best.iter().fold(0_u64, |covered, row_index| {
            covered | rows[*row_index].words[0]
        });
        assert_eq!(covered & target[0], target[0]);

        let expected = best.clone();
        let mut exact_cap_best = vec![0, 1, 2];
        improve_fixed_cardinality_cover_with_memory_guard(
            &rows,
            &target,
            &support_by_pattern,
            &mut exact_cap_best,
            BREAKOUT_TOTAL_SWAP_BUDGET,
            None,
            base_live_bytes,
            &mut |owned_bytes| {
                if owned_bytes <= observed_peak {
                    Ok(())
                } else {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: owned_bytes,
                        max_memory_bytes: observed_peak,
                    })
                }
            },
            &mut || false,
        )
        .expect("observed peak must be sufficient");
        assert_eq!(exact_cap_best, expected);

        let max_memory_bytes = observed_peak - 1;
        let mut rejected_best = vec![0, 1, 2];
        assert!(matches!(
            improve_fixed_cardinality_cover_with_memory_guard(
                &rows,
                &target,
                &support_by_pattern,
                &mut rejected_best,
                BREAKOUT_TOTAL_SWAP_BUDGET,
                None,
                base_live_bytes,
                &mut |owned_bytes| {
                    if owned_bytes <= max_memory_bytes {
                        Ok(())
                    } else {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: owned_bytes,
                            max_memory_bytes,
                        })
                    }
                },
                &mut || false,
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes: rejected_max,
            }) if required_memory_bytes > rejected_max
        ));
    }

    fn naive_remove_dominated_rows(rows: &mut Vec<DenseRow>) {
        let mut dominated = vec![false; rows.len()];
        for left in 0..rows.len() {
            for right in 0..rows.len() {
                if left == right || dominated[left] {
                    continue;
                }
                if is_superset(&rows[right].words, &rows[left].words) {
                    let equal = rows[right].words == rows[left].words;
                    if !equal || rows[right].source_index < rows[left].source_index {
                        dominated[left] = true;
                    }
                }
            }
        }
        let mut index = 0;
        rows.retain(|_| {
            let keep = !dominated[index];
            index += 1;
            keep
        });
    }
}
