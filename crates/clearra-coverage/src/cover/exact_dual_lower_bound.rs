// SRP rationale: this module has one change reason: propose deterministic
// fractional set-cover duals and reduce them to independently checked integer
// certificates under the exact solver's governed-memory contract.
//
// Floating-point Mirror-Prox state is never proof authority. A prune is
// permitted only after this module floors the proposal to nonnegative integer
// weights, recomputes every eligible row load with checked `u128` arithmetic,
// and derives a lower bound from that exact certificate.

use super::exact_minimum_cover::ExactMinimumCoverError;

#[cfg(feature = "diagnostic-probes")]
use std::time::Instant;

const CERTIFICATE_SCALE: u128 = 1_000_000_000;
const MIRROR_PROX_ETA: f64 = 12.0;
const MIRROR_PROX_ITERATIONS: usize = 200;
const MIRROR_PROX_BURN_IN: usize = MIRROR_PROX_ITERATIONS / 10;
const MIRROR_PROX_CERTIFICATE_INTERVAL: usize = 25;
const MAX_PROPOSAL_ITERATIONS_PER_PROOF: usize = 2_000_000;
// Omitting one term at this cutoff changes only the floating proposal. Even at
// the maximum admitted 4,096 constraints, the discarded unnormalized mass is
// below 1e-12. Exact pruning authority remains the checked-u128 certificate.
const PROPOSAL_LOG_CUTOFF: f64 = -36.0;
const UNUSED_ROW: usize = usize::MAX;

pub(super) const MIN_DUAL_ROW_COUNT: usize = 64;
pub(super) const MAX_DUAL_ROW_COUNT: usize = 256;
pub(super) const MIN_DUAL_CONSTRAINT_COUNT: usize = 128;
pub(super) const MAX_DUAL_CONSTRAINT_COUNT: usize = 4_096;
pub(super) const MAX_DUAL_INCIDENCE_COUNT: usize = 262_144;
pub(super) const MIN_DUAL_SEARCH_DEPTH: usize = 6;
pub(super) const MAX_DUAL_ROWS_TO_IMPROVE: usize = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ResidualDualMemoryProjection {
    pub index_bytes: u128,
    pub floating_workspace_bytes: u128,
    pub certificate_bytes: u128,
    pub required_peak_bytes: u128,
}

#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ResidualDualHotCostDiagnostics {
    pub prepare_calls: u64,
    pub prepare_nanoseconds: u128,
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
    pub certificate_calls: u64,
    pub certificate_nanoseconds: u128,
}

/// A root dual whose exact row-capacity checks have already succeeded.
///
/// The private fields prevent floating proposals or unchecked weights from
/// entering the minimum-cover proof.
#[derive(Clone, Debug)]
pub(super) struct CertifiedResidualDual {
    patterns: Vec<usize>,
    weights: Vec<u128>,
    denominator: u128,
}

impl CertifiedResidualDual {
    pub(super) fn certified_lower_bound_for_uncovered(
        &self,
        target_words: &[u64],
        covered_words: &[u64],
    ) -> Option<usize> {
        if target_words.len() != covered_words.len()
            || self.patterns.len() != self.weights.len()
            || self.denominator == 0
        {
            return None;
        }
        let mut numerator = 0_u128;
        for (pattern, weight) in self.patterns.iter().copied().zip(&self.weights) {
            let word_index = pattern / u64::BITS as usize;
            let bit = pattern % u64::BITS as usize;
            let target = *target_words.get(word_index)?;
            let covered = *covered_words.get(word_index)?;
            if target & !covered & (1_u64 << bit) != 0 {
                numerator = numerator.checked_add(*weight)?;
            }
        }
        usize::try_from(numerator.div_ceil(self.denominator)).ok()
    }

    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        checked_vec_retained_bytes(&self.patterns)?
            .checked_add(checked_vec_retained_bytes(&self.weights)?)
    }
}

pub(super) const fn should_attempt_residual_dual(
    row_count: usize,
    constraint_count: usize,
    search_depth: usize,
    maximum_rows_to_improve: usize,
) -> bool {
    row_count >= MIN_DUAL_ROW_COUNT
        && row_count <= MAX_DUAL_ROW_COUNT
        && constraint_count >= MIN_DUAL_CONSTRAINT_COUNT
        && constraint_count <= MAX_DUAL_CONSTRAINT_COUNT
        && search_depth >= MIN_DUAL_SEARCH_DEPTH
        && maximum_rows_to_improve <= MAX_DUAL_ROWS_TO_IMPROVE
}

pub(super) const fn should_prepare_root_dual(row_count: usize, constraint_count: usize) -> bool {
    row_count >= MIN_DUAL_ROW_COUNT
        && row_count <= MAX_DUAL_ROW_COUNT
        && constraint_count >= MIN_DUAL_CONSTRAINT_COUNT
        && constraint_count <= MAX_DUAL_CONSTRAINT_COUNT
}

/// Maximum requested capacities of the fully owned proposal workspace.
///
/// Every production allocation is fallible and the allocator-returned
/// capacity is reported to the caller immediately after each reserve. The
/// factor of two here is the same conservative capacity-rounding allowance
/// used by the exact solver's other input-only projections; actual retained
/// capacities remain authoritative at runtime.
pub(super) fn checked_residual_dual_memory_projection(
    row_count: usize,
    constraint_count: usize,
    incidence_count: usize,
) -> Option<ResidualDualMemoryProjection> {
    let rows = row_count as u128;
    let constraints = constraint_count as u128;
    let incidences = incidence_count as u128;
    let usize_bytes = core::mem::size_of::<usize>() as u128;
    let float_bytes = core::mem::size_of::<f64>() as u128;
    let integer_bytes = core::mem::size_of::<u128>() as u128;

    // Current and cached pattern IDs + row_counts + eligible_rows + source_to_eligible +
    // row_offsets (R+1) + cursors + flattened row-to-constraint CSR.
    let index_slots = constraints
        .checked_mul(2)?
        .checked_add(rows.checked_mul(5)?)?
        .checked_add(1)?
        .checked_add(incidences)?;
    let index_bytes = index_slots.checked_mul(usize_bytes)?.checked_mul(2)?;

    // Eight constraint vectors and seven row vectors. Keeping these buffers
    // in one reusable owner avoids thousands of residual-node allocations.
    let floating_slots = constraints
        .checked_mul(8)?
        .checked_add(rows.checked_mul(7)?)?;
    let floating_workspace_bytes = floating_slots.checked_mul(float_bytes)?.checked_mul(2)?;

    // Candidate, current best, and cached sparse weights coexist with exact row loads.
    let certificate_slots = constraints.checked_mul(3)?.checked_add(rows)?;
    let certificate_bytes = certificate_slots
        .checked_mul(integer_bytes)?
        .checked_mul(2)?;
    let required_peak_bytes = index_bytes
        .checked_add(floating_workspace_bytes)?
        .checked_add(certificate_bytes)?;
    Some(ResidualDualMemoryProjection {
        index_bytes,
        floating_workspace_bytes,
        certificate_bytes,
        required_peak_bytes,
    })
}

pub(super) fn checked_maximum_residual_dual_workspace_bytes(
    row_count: usize,
    constraint_count: usize,
) -> Option<u128> {
    if row_count < MIN_DUAL_ROW_COUNT || constraint_count < MIN_DUAL_CONSTRAINT_COUNT {
        return Some(0);
    }
    let admitted_rows = row_count.min(MAX_DUAL_ROW_COUNT);
    let admitted_constraints = constraint_count.min(MAX_DUAL_CONSTRAINT_COUNT);
    let admitted_incidences = admitted_rows
        .checked_mul(admitted_constraints)?
        .min(MAX_DUAL_INCIDENCE_COUNT);
    checked_residual_dual_memory_projection(
        admitted_rows,
        admitted_constraints,
        admitted_incidences,
    )
    .map(|projection| projection.required_peak_bytes)
}

pub(super) fn checked_maximum_persistent_dual_certificate_bytes(
    constraint_count: usize,
) -> Option<u128> {
    (constraint_count as u128)
        .checked_mul((core::mem::size_of::<usize>() + core::mem::size_of::<u128>()) as u128)?
        .checked_mul(2)
}

/// Reusable, fully memory-accounted Mirror-Prox proposal state.
#[derive(Debug)]
pub(super) struct DualProposalWorkspace {
    max_rows: usize,
    max_constraints: usize,
    max_incidences: usize,
    retained_bytes: u128,
    remaining_iterations: usize,

    patterns: Vec<usize>,
    row_counts: Vec<usize>,
    eligible_rows: Vec<usize>,
    source_to_eligible: Vec<usize>,
    row_offsets: Vec<usize>,
    cursors: Vec<usize>,
    row_constraints: Vec<usize>,

    log_p: Vec<f64>,
    p: Vec<f64>,
    middle_log_p: Vec<f64>,
    middle_p: Vec<f64>,
    gradient_p: Vec<f64>,
    middle_gradient_p: Vec<f64>,
    average_p: Vec<f64>,
    proposed: Vec<f64>,

    log_q: Vec<f64>,
    q: Vec<f64>,
    middle_log_q: Vec<f64>,
    middle_q: Vec<f64>,
    gradient_q: Vec<f64>,
    middle_gradient_q: Vec<f64>,
    floating_row_loads: Vec<f64>,

    candidate_weights: Vec<u128>,
    best_weights: Vec<u128>,
    exact_row_loads: Vec<u128>,
    best_denominator: u128,
    best_numerator: u128,

    // Advisory weights survive residual preparation. IDs are actual pattern
    // indices, not positions in the previous uncovered-constraint vector.
    // Every reuse replays all currently eligible row capacities from scratch.
    cached_patterns: Vec<usize>,
    cached_weights: Vec<u128>,
    #[cfg(feature = "diagnostic-probes")]
    diagnostic_hot_cost: ResidualDualHotCostDiagnostics,
    #[cfg(feature = "diagnostic-probes")]
    diagnostic_sparse_proposal_softmax: bool,
}

// `DualProposalWorkspace` owns maximum-size reusable buffers.  A derived
// `Vec::clone` is allowed to retain only `len()` slots, which is insufficient
// here: a portfolio transaction can clone a workspace after a small residual
// query and the next (larger) query must not allocate behind the memory guard.
// Preserve every admitted capacity so `retained_bytes` remains authoritative
// and all later proposal runs stay allocation-free.
impl Clone for DualProposalWorkspace {
    fn clone(&self) -> Self {
        fn with_retained_capacity<T: Clone>(source: &Vec<T>) -> Vec<T> {
            let mut cloned = Vec::with_capacity(source.capacity());
            cloned.extend_from_slice(source);
            cloned
        }

        let cloned = Self {
            max_rows: self.max_rows,
            max_constraints: self.max_constraints,
            max_incidences: self.max_incidences,
            retained_bytes: self.retained_bytes,
            remaining_iterations: self.remaining_iterations,
            patterns: with_retained_capacity(&self.patterns),
            row_counts: with_retained_capacity(&self.row_counts),
            eligible_rows: with_retained_capacity(&self.eligible_rows),
            source_to_eligible: with_retained_capacity(&self.source_to_eligible),
            row_offsets: with_retained_capacity(&self.row_offsets),
            cursors: with_retained_capacity(&self.cursors),
            row_constraints: with_retained_capacity(&self.row_constraints),
            log_p: with_retained_capacity(&self.log_p),
            p: with_retained_capacity(&self.p),
            middle_log_p: with_retained_capacity(&self.middle_log_p),
            middle_p: with_retained_capacity(&self.middle_p),
            gradient_p: with_retained_capacity(&self.gradient_p),
            middle_gradient_p: with_retained_capacity(&self.middle_gradient_p),
            average_p: with_retained_capacity(&self.average_p),
            proposed: with_retained_capacity(&self.proposed),
            log_q: with_retained_capacity(&self.log_q),
            q: with_retained_capacity(&self.q),
            middle_log_q: with_retained_capacity(&self.middle_log_q),
            middle_q: with_retained_capacity(&self.middle_q),
            gradient_q: with_retained_capacity(&self.gradient_q),
            middle_gradient_q: with_retained_capacity(&self.middle_gradient_q),
            floating_row_loads: with_retained_capacity(&self.floating_row_loads),
            candidate_weights: with_retained_capacity(&self.candidate_weights),
            best_weights: with_retained_capacity(&self.best_weights),
            exact_row_loads: with_retained_capacity(&self.exact_row_loads),
            best_denominator: self.best_denominator,
            best_numerator: self.best_numerator,
            cached_patterns: with_retained_capacity(&self.cached_patterns),
            cached_weights: with_retained_capacity(&self.cached_weights),
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_hot_cost: self.diagnostic_hot_cost,
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_sparse_proposal_softmax: self.diagnostic_sparse_proposal_softmax,
        };
        debug_assert_eq!(cloned.actual_retained_bytes(), Some(self.retained_bytes));
        cloned
    }
}

impl DualProposalWorkspace {
    fn actual_retained_bytes(&self) -> Option<u128> {
        macro_rules! add_capacity {
            ($total:ident, $field:expr, $type:ty) => {
                $total = $total.checked_add(
                    ($field.capacity() as u128)
                        .checked_mul(core::mem::size_of::<$type>() as u128)?,
                )?;
            };
        }

        let mut total = 0_u128;
        add_capacity!(total, self.patterns, usize);
        add_capacity!(total, self.row_counts, usize);
        add_capacity!(total, self.eligible_rows, usize);
        add_capacity!(total, self.source_to_eligible, usize);
        add_capacity!(total, self.row_offsets, usize);
        add_capacity!(total, self.cursors, usize);
        add_capacity!(total, self.row_constraints, usize);
        add_capacity!(total, self.log_p, f64);
        add_capacity!(total, self.p, f64);
        add_capacity!(total, self.middle_log_p, f64);
        add_capacity!(total, self.middle_p, f64);
        add_capacity!(total, self.gradient_p, f64);
        add_capacity!(total, self.middle_gradient_p, f64);
        add_capacity!(total, self.average_p, f64);
        add_capacity!(total, self.proposed, f64);
        add_capacity!(total, self.log_q, f64);
        add_capacity!(total, self.q, f64);
        add_capacity!(total, self.middle_log_q, f64);
        add_capacity!(total, self.middle_q, f64);
        add_capacity!(total, self.gradient_q, f64);
        add_capacity!(total, self.middle_gradient_q, f64);
        add_capacity!(total, self.floating_row_loads, f64);
        add_capacity!(total, self.candidate_weights, u128);
        add_capacity!(total, self.best_weights, u128);
        add_capacity!(total, self.exact_row_loads, u128);
        add_capacity!(total, self.cached_patterns, usize);
        add_capacity!(total, self.cached_weights, u128);
        Some(total)
    }

    pub(super) fn try_new<F>(
        max_rows: usize,
        max_constraints: usize,
        max_incidences: usize,
        external_live_bytes: u128,
        memory_guard: &mut F,
    ) -> Result<Self, ExactMinimumCoverError>
    where
        F: FnMut(u128) -> Result<(), ExactMinimumCoverError> + ?Sized,
    {
        if max_rows > MAX_DUAL_ROW_COUNT
            || max_constraints > MAX_DUAL_CONSTRAINT_COUNT
            || max_incidences > MAX_DUAL_INCIDENCE_COUNT
        {
            return Err(ExactMinimumCoverError::ProjectionOverflow);
        }
        let mut retained_bytes = 0_u128;
        macro_rules! reserved {
            ($type:ty, $capacity:expr, $component:literal) => {
                try_reserved_vec::<$type, F>(
                    $capacity,
                    external_live_bytes,
                    &mut retained_bytes,
                    memory_guard,
                    $component,
                )?
            };
        }

        let patterns = reserved!(usize, max_constraints, "exact_dual_patterns");
        let mut row_counts = reserved!(usize, max_rows, "exact_dual_row_counts");
        row_counts.resize(max_rows, 0);
        let eligible_rows = reserved!(usize, max_rows, "exact_dual_eligible_rows");
        let mut source_to_eligible = reserved!(usize, max_rows, "exact_dual_source_to_eligible");
        source_to_eligible.resize(max_rows, UNUSED_ROW);
        let row_offsets = reserved!(usize, max_rows + 1, "exact_dual_row_offsets");
        let cursors = reserved!(usize, max_rows, "exact_dual_cursors");
        let row_constraints = reserved!(usize, max_incidences, "exact_dual_row_constraints");

        let log_p = reserved!(f64, max_constraints, "exact_dual_log_p");
        let p = reserved!(f64, max_constraints, "exact_dual_p");
        let middle_log_p = reserved!(f64, max_constraints, "exact_dual_middle_log_p");
        let middle_p = reserved!(f64, max_constraints, "exact_dual_middle_p");
        let gradient_p = reserved!(f64, max_constraints, "exact_dual_gradient_p");
        let middle_gradient_p = reserved!(f64, max_constraints, "exact_dual_middle_gradient_p");
        let average_p = reserved!(f64, max_constraints, "exact_dual_average_p");
        let proposed = reserved!(f64, max_constraints, "exact_dual_proposed");

        let log_q = reserved!(f64, max_rows, "exact_dual_log_q");
        let q = reserved!(f64, max_rows, "exact_dual_q");
        let middle_log_q = reserved!(f64, max_rows, "exact_dual_middle_log_q");
        let middle_q = reserved!(f64, max_rows, "exact_dual_middle_q");
        let gradient_q = reserved!(f64, max_rows, "exact_dual_gradient_q");
        let middle_gradient_q = reserved!(f64, max_rows, "exact_dual_middle_gradient_q");
        let floating_row_loads = reserved!(f64, max_rows, "exact_dual_floating_row_loads");

        let candidate_weights = reserved!(u128, max_constraints, "exact_dual_candidate_weights");
        let best_weights = reserved!(u128, max_constraints, "exact_dual_best_weights");
        let exact_row_loads = reserved!(u128, max_rows, "exact_dual_exact_row_loads");
        let cached_patterns = reserved!(usize, max_constraints, "exact_dual_cached_patterns");
        let cached_weights = reserved!(u128, max_constraints, "exact_dual_cached_weights");
        memory_guard(
            external_live_bytes
                .checked_add(retained_bytes)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;

        Ok(Self {
            max_rows,
            max_constraints,
            max_incidences,
            retained_bytes,
            remaining_iterations: MAX_PROPOSAL_ITERATIONS_PER_PROOF,
            patterns,
            row_counts,
            eligible_rows,
            source_to_eligible,
            row_offsets,
            cursors,
            row_constraints,
            log_p,
            p,
            middle_log_p,
            middle_p,
            gradient_p,
            middle_gradient_p,
            average_p,
            proposed,
            log_q,
            q,
            middle_log_q,
            middle_q,
            gradient_q,
            middle_gradient_q,
            floating_row_loads,
            candidate_weights,
            best_weights,
            exact_row_loads,
            best_denominator: 0,
            best_numerator: 0,
            cached_patterns,
            cached_weights,
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_hot_cost: ResidualDualHotCostDiagnostics::default(),
            #[cfg(feature = "diagnostic-probes")]
            diagnostic_sparse_proposal_softmax: true,
        })
    }

    pub(super) const fn retained_bytes(&self) -> u128 {
        self.retained_bytes
    }

    pub(super) const fn remaining_proposal_iterations(&self) -> usize {
        self.remaining_iterations
    }

    #[cfg(feature = "diagnostic-probes")]
    pub(super) const fn diagnostic_hot_cost(&self) -> ResidualDualHotCostDiagnostics {
        self.diagnostic_hot_cost
    }

    #[cfg(feature = "diagnostic-probes")]
    pub(super) fn reset_diagnostic_hot_cost(&mut self) {
        self.diagnostic_hot_cost = ResidualDualHotCostDiagnostics::default();
    }

    #[cfg(feature = "diagnostic-probes")]
    pub(super) fn set_diagnostic_sparse_proposal_softmax(&mut self, enabled: bool) {
        self.diagnostic_sparse_proposal_softmax = enabled;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_root_certificate_with_memory_guard<F>(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        external_live_bytes: u128,
        memory_guard: &mut F,
    ) -> Result<Option<CertifiedResidualDual>, ExactMinimumCoverError>
    where
        F: FnMut(u128) -> Result<(), ExactMinimumCoverError> + ?Sized,
    {
        self.prepare_root_certificate_with_memory_guard_and_iteration_limit(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            external_live_bytes,
            MIRROR_PROX_ITERATIONS,
            memory_guard,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_root_certificate_with_memory_guard_and_iteration_limit<F>(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        external_live_bytes: u128,
        iteration_limit: usize,
        memory_guard: &mut F,
    ) -> Result<Option<CertifiedResidualDual>, ExactMinimumCoverError>
    where
        F: FnMut(u128) -> Result<(), ExactMinimumCoverError> + ?Sized,
    {
        let Some(_) = self.certified_residual_lower_bound_inner_with_iteration_limit(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            usize::MAX,
            true,
            iteration_limit,
        ) else {
            return Ok(None);
        };
        if self.best_numerator == 0 || self.best_denominator == 0 {
            return Ok(None);
        }

        let workspace_live = external_live_bytes
            .checked_add(self.retained_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut certificate_live = workspace_live;
        let mut patterns = try_reserved_vec::<usize, F>(
            self.patterns.len(),
            0,
            &mut certificate_live,
            memory_guard,
            "exact_dual_root_certificate_patterns",
        )?;
        patterns.extend_from_slice(&self.patterns);
        let mut weights = try_reserved_vec::<u128, F>(
            self.best_weights.len(),
            0,
            &mut certificate_live,
            memory_guard,
            "exact_dual_root_certificate_weights",
        )?;
        weights.extend_from_slice(&self.best_weights);
        Ok(Some(CertifiedResidualDual {
            patterns,
            weights,
            denominator: self.best_denominator,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn certified_residual_lower_bound(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        row_limit: usize,
    ) -> Option<usize> {
        self.certified_residual_lower_bound_inner(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            row_limit,
            true,
        )
    }

    /// Probe-only cap for comparing the value of late Mirror-Prox
    /// checkpoints. Exact integer recertification is unchanged, so this can
    /// only omit an optional proposal; it cannot create proof authority.
    #[cfg(feature = "diagnostic-probes")]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn diagnostic_certified_residual_lower_bound_with_iteration_limit(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        row_limit: usize,
        iteration_limit: usize,
    ) -> Option<usize> {
        if iteration_limit >= MIRROR_PROX_ITERATIONS {
            return self.certified_residual_lower_bound(
                support_by_pattern,
                target_words,
                covered_words,
                selected_rows,
                excluded_row_words,
                row_limit,
            );
        }
        self.certified_residual_lower_bound_inner_with_iteration_limit(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            row_limit,
            true,
            iteration_limit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn certified_residual_lower_bound_inner(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        row_limit: usize,
        enforce_minimum_dimensions: bool,
    ) -> Option<usize> {
        self.certified_residual_lower_bound_inner_with_iteration_limit(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            row_limit,
            enforce_minimum_dimensions,
            MIRROR_PROX_ITERATIONS,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn certified_residual_lower_bound_inner_with_iteration_limit(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        row_limit: usize,
        enforce_minimum_dimensions: bool,
        iteration_limit: usize,
    ) -> Option<usize> {
        // Root export passes usize::MAX and must still execute its original
        // proposal/certificate path. A weaker cached bound must not replace
        // the existing Mirror-Prox proposal or consume any of its budget.
        if row_limit != usize::MAX {
            if let Some(bound) = self.recertify_cached_residual_bound(
                support_by_pattern,
                target_words,
                covered_words,
                selected_rows,
                excluded_row_words,
            ) {
                if bound > row_limit {
                    return Some(bound);
                }
            }
        }
        if self.remaining_iterations == 0 {
            return None;
        }
        #[cfg(feature = "diagnostic-probes")]
        let prepare_started = Instant::now();
        let prepared = self.prepare_residual(
            support_by_pattern,
            target_words,
            covered_words,
            selected_rows,
            excluded_row_words,
            enforce_minimum_dimensions,
        );
        #[cfg(feature = "diagnostic-probes")]
        {
            self.diagnostic_hot_cost.prepare_calls =
                self.diagnostic_hot_cost.prepare_calls.saturating_add(1);
            self.diagnostic_hot_cost.prepare_nanoseconds = self
                .diagnostic_hot_cost
                .prepare_nanoseconds
                .saturating_add(prepare_started.elapsed().as_nanos());
        }
        if !prepared {
            return None;
        }
        if self.patterns.is_empty() {
            return Some(0);
        }
        let iterations = iteration_limit.min(self.remaining_iterations);
        if iterations == 0 {
            return None;
        }
        self.reset_proposal_state();
        let mut accumulated_samples = 0_usize;

        for iteration in 0..iterations {
            #[cfg(feature = "diagnostic-probes")]
            let iteration_started = Instant::now();
            // Charge only work that this invocation actually enters. A
            // certified prune may return at the first 25-iteration checkpoint;
            // reserving the entire 200-iteration allowance up front depleted
            // the proof-global budget up to eight times too quickly.
            self.remaining_iterations = self.remaining_iterations.checked_sub(1)?;
            #[cfg(feature = "diagnostic-probes")]
            let softmax_p_started = Instant::now();
            #[cfg(feature = "diagnostic-probes")]
            let (valid_p, softmax_p_density) = diagnostic_softmax_cutoff_density(
                &self.log_p,
                &mut self.p,
                None,
                self.diagnostic_sparse_proposal_softmax,
            );
            #[cfg(not(feature = "diagnostic-probes"))]
            let valid_p = softmax(&self.log_p, &mut self.p);
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.softmax_p_nanoseconds = self
                    .diagnostic_hot_cost
                    .softmax_p_nanoseconds
                    .saturating_add(softmax_p_started.elapsed().as_nanos());
                self.diagnostic_hot_cost.softmax_p_entries = self
                    .diagnostic_hot_cost
                    .softmax_p_entries
                    .saturating_add(softmax_p_density.entries);
                self.diagnostic_hot_cost.softmax_p_cutoff_entries = self
                    .diagnostic_hot_cost
                    .softmax_p_cutoff_entries
                    .saturating_add(softmax_p_density.cutoff_entries);
            }
            #[cfg(feature = "diagnostic-probes")]
            let softmax_q_started = Instant::now();
            #[cfg(feature = "diagnostic-probes")]
            let (valid_q, softmax_q_density) = diagnostic_softmax_cutoff_density(
                &self.log_q,
                &mut self.q,
                Some(&self.row_offsets),
                self.diagnostic_sparse_proposal_softmax,
            );
            #[cfg(not(feature = "diagnostic-probes"))]
            let valid_q = softmax(&self.log_q, &mut self.q);
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.softmax_q_nanoseconds = self
                    .diagnostic_hot_cost
                    .softmax_q_nanoseconds
                    .saturating_add(softmax_q_started.elapsed().as_nanos());
                self.diagnostic_hot_cost.softmax_q_entries = self
                    .diagnostic_hot_cost
                    .softmax_q_entries
                    .saturating_add(softmax_q_density.entries);
                self.diagnostic_hot_cost.softmax_q_cutoff_entries = self
                    .diagnostic_hot_cost
                    .softmax_q_cutoff_entries
                    .saturating_add(softmax_q_density.cutoff_entries);
                self.diagnostic_hot_cost.softmax_q_row_incidences = self
                    .diagnostic_hot_cost
                    .softmax_q_row_incidences
                    .saturating_add(softmax_q_density.row_incidences);
                self.diagnostic_hot_cost.softmax_q_cutoff_row_incidences = self
                    .diagnostic_hot_cost
                    .softmax_q_cutoff_row_incidences
                    .saturating_add(softmax_q_density.cutoff_row_incidences);
            }
            if !valid_p || !valid_q {
                return None;
            }
            #[cfg(feature = "diagnostic-probes")]
            let first_gradient_started = Instant::now();
            saddle_gradients(
                &self.row_offsets,
                &self.row_constraints,
                &self.p,
                &self.q,
                &mut self.gradient_p,
                &mut self.gradient_q,
            )?;
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.first_gradient_nanoseconds = self
                    .diagnostic_hot_cost
                    .first_gradient_nanoseconds
                    .saturating_add(first_gradient_started.elapsed().as_nanos());
            }
            #[cfg(feature = "diagnostic-probes")]
            let first_log_update_started = Instant::now();
            for index in 0..self.patterns.len() {
                self.middle_log_p[index] =
                    self.log_p[index] - MIRROR_PROX_ETA * self.gradient_p[index];
            }
            for index in 0..self.eligible_rows.len() {
                self.middle_log_q[index] =
                    self.log_q[index] + MIRROR_PROX_ETA * self.gradient_q[index];
            }
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.log_update_nanoseconds = self
                    .diagnostic_hot_cost
                    .log_update_nanoseconds
                    .saturating_add(first_log_update_started.elapsed().as_nanos());
            }
            #[cfg(feature = "diagnostic-probes")]
            let softmax_middle_p_started = Instant::now();
            #[cfg(feature = "diagnostic-probes")]
            let (valid_middle_p, softmax_middle_p_density) = diagnostic_softmax_cutoff_density(
                &self.middle_log_p,
                &mut self.middle_p,
                None,
                self.diagnostic_sparse_proposal_softmax,
            );
            #[cfg(not(feature = "diagnostic-probes"))]
            let valid_middle_p = softmax(&self.middle_log_p, &mut self.middle_p);
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.softmax_middle_p_nanoseconds = self
                    .diagnostic_hot_cost
                    .softmax_middle_p_nanoseconds
                    .saturating_add(softmax_middle_p_started.elapsed().as_nanos());
                self.diagnostic_hot_cost.softmax_middle_p_entries = self
                    .diagnostic_hot_cost
                    .softmax_middle_p_entries
                    .saturating_add(softmax_middle_p_density.entries);
                self.diagnostic_hot_cost.softmax_middle_p_cutoff_entries = self
                    .diagnostic_hot_cost
                    .softmax_middle_p_cutoff_entries
                    .saturating_add(softmax_middle_p_density.cutoff_entries);
            }
            #[cfg(feature = "diagnostic-probes")]
            let softmax_middle_q_started = Instant::now();
            #[cfg(feature = "diagnostic-probes")]
            let (valid_middle_q, softmax_middle_q_density) = diagnostic_softmax_cutoff_density(
                &self.middle_log_q,
                &mut self.middle_q,
                Some(&self.row_offsets),
                self.diagnostic_sparse_proposal_softmax,
            );
            #[cfg(not(feature = "diagnostic-probes"))]
            let valid_middle_q = softmax(&self.middle_log_q, &mut self.middle_q);
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.softmax_middle_q_nanoseconds = self
                    .diagnostic_hot_cost
                    .softmax_middle_q_nanoseconds
                    .saturating_add(softmax_middle_q_started.elapsed().as_nanos());
                self.diagnostic_hot_cost.softmax_middle_q_entries = self
                    .diagnostic_hot_cost
                    .softmax_middle_q_entries
                    .saturating_add(softmax_middle_q_density.entries);
                self.diagnostic_hot_cost.softmax_middle_q_cutoff_entries = self
                    .diagnostic_hot_cost
                    .softmax_middle_q_cutoff_entries
                    .saturating_add(softmax_middle_q_density.cutoff_entries);
                self.diagnostic_hot_cost.softmax_middle_q_row_incidences = self
                    .diagnostic_hot_cost
                    .softmax_middle_q_row_incidences
                    .saturating_add(softmax_middle_q_density.row_incidences);
                self.diagnostic_hot_cost
                    .softmax_middle_q_cutoff_row_incidences = self
                    .diagnostic_hot_cost
                    .softmax_middle_q_cutoff_row_incidences
                    .saturating_add(softmax_middle_q_density.cutoff_row_incidences);
            }
            if !valid_middle_p || !valid_middle_q {
                return None;
            }
            #[cfg(feature = "diagnostic-probes")]
            let middle_gradient_started = Instant::now();
            saddle_gradients(
                &self.row_offsets,
                &self.row_constraints,
                &self.middle_p,
                &self.middle_q,
                &mut self.middle_gradient_p,
                &mut self.middle_gradient_q,
            )?;
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.middle_gradient_nanoseconds = self
                    .diagnostic_hot_cost
                    .middle_gradient_nanoseconds
                    .saturating_add(middle_gradient_started.elapsed().as_nanos());
            }
            #[cfg(feature = "diagnostic-probes")]
            let second_log_update_started = Instant::now();
            for index in 0..self.patterns.len() {
                self.log_p[index] -= MIRROR_PROX_ETA * self.middle_gradient_p[index];
            }
            for index in 0..self.eligible_rows.len() {
                self.log_q[index] += MIRROR_PROX_ETA * self.middle_gradient_q[index];
            }
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.log_update_nanoseconds = self
                    .diagnostic_hot_cost
                    .log_update_nanoseconds
                    .saturating_add(second_log_update_started.elapsed().as_nanos());
            }

            if iteration >= MIRROR_PROX_BURN_IN {
                #[cfg(feature = "diagnostic-probes")]
                let averaging_started = Instant::now();
                accumulated_samples = accumulated_samples.checked_add(1)?;
                for index in 0..self.patterns.len() {
                    self.average_p[index] += self.middle_p[index];
                }
                #[cfg(feature = "diagnostic-probes")]
                {
                    self.diagnostic_hot_cost.averaging_nanoseconds = self
                        .diagnostic_hot_cost
                        .averaging_nanoseconds
                        .saturating_add(averaging_started.elapsed().as_nanos());
                }
            }
            #[cfg(feature = "diagnostic-probes")]
            {
                self.diagnostic_hot_cost.mirror_prox_iterations = self
                    .diagnostic_hot_cost
                    .mirror_prox_iterations
                    .saturating_add(1);
                self.diagnostic_hot_cost.mirror_prox_nanoseconds = self
                    .diagnostic_hot_cost
                    .mirror_prox_nanoseconds
                    .saturating_add(iteration_started.elapsed().as_nanos());
            }
            if accumulated_samples != 0
                && ((iteration + 1) % MIRROR_PROX_CERTIFICATE_INTERVAL == 0
                    || iteration + 1 == iterations)
            {
                #[cfg(feature = "diagnostic-probes")]
                let certificate_started = Instant::now();
                let certified = self.certify_average_proposal();
                #[cfg(feature = "diagnostic-probes")]
                {
                    self.diagnostic_hot_cost.certificate_calls =
                        self.diagnostic_hot_cost.certificate_calls.saturating_add(1);
                    self.diagnostic_hot_cost.certificate_nanoseconds = self
                        .diagnostic_hot_cost
                        .certificate_nanoseconds
                        .saturating_add(certificate_started.elapsed().as_nanos());
                }
                if let Some(bound) = certified {
                    if bound > row_limit {
                        return Some(bound);
                    }
                }
            }
        }
        (self.best_numerator != 0 && self.best_denominator != 0)
            .then(|| usize::try_from(self.best_numerator.div_ceil(self.best_denominator)).ok())
            .flatten()
    }

    fn prepare_residual(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
        enforce_minimum_dimensions: bool,
    ) -> bool {
        if target_words.len() != covered_words.len()
            || selected_rows.len() > self.max_rows
            || excluded_row_words.len() < selected_rows.len().div_ceil(u64::BITS as usize)
        {
            return false;
        }
        self.patterns.clear();
        self.eligible_rows.clear();
        self.row_offsets.clear();
        self.cursors.clear();
        self.row_constraints.clear();
        self.row_counts.fill(0);
        self.source_to_eligible.fill(UNUSED_ROW);

        let mut incidence_count = 0_usize;
        for (word_index, (target, covered)) in target_words.iter().zip(covered_words).enumerate() {
            let mut uncovered = target & !covered;
            while uncovered != 0 {
                let bit = uncovered.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                let Some(support) = support_by_pattern.get(pattern) else {
                    return false;
                };
                let mut eligible_support_count = 0_usize;
                for row in support.iter().copied() {
                    let Some(selected) = selected_rows.get(row) else {
                        return false;
                    };
                    if !*selected && !row_is_excluded(excluded_row_words, row) {
                        let Some(count) = self.row_counts.get_mut(row) else {
                            return false;
                        };
                        *count = match count.checked_add(1) {
                            Some(count) => count,
                            None => return false,
                        };
                        eligible_support_count += 1;
                    }
                }
                if eligible_support_count == 0
                    || self.patterns.len() == self.max_constraints
                    || incidence_count
                        .checked_add(eligible_support_count)
                        .is_none_or(|count| count > self.max_incidences)
                {
                    return false;
                }
                incidence_count += eligible_support_count;
                self.patterns.push(pattern);
                uncovered &= uncovered - 1;
            }
        }
        for (row, count) in self.row_counts.iter().copied().enumerate() {
            if count != 0 {
                self.source_to_eligible[row] = self.eligible_rows.len();
                self.eligible_rows.push(row);
            }
        }
        if self.patterns.len() > MAX_DUAL_CONSTRAINT_COUNT
            || self.eligible_rows.len() > MAX_DUAL_ROW_COUNT
            || incidence_count > MAX_DUAL_INCIDENCE_COUNT
            || (enforce_minimum_dimensions
                && (self.patterns.len() < MIN_DUAL_CONSTRAINT_COUNT
                    || self.eligible_rows.len() < MIN_DUAL_ROW_COUNT))
        {
            return false;
        }

        self.row_offsets.push(0);
        let mut offset = 0_usize;
        for row in self.eligible_rows.iter().copied() {
            offset = match offset.checked_add(self.row_counts[row]) {
                Some(offset) => offset,
                None => return false,
            };
            self.row_offsets.push(offset);
        }
        if offset != incidence_count {
            return false;
        }
        self.row_constraints.resize(incidence_count, 0);
        self.cursors
            .extend_from_slice(&self.row_offsets[..self.eligible_rows.len()]);
        for (constraint, pattern) in self.patterns.iter().copied().enumerate() {
            for source_row in support_by_pattern[pattern].iter().copied() {
                let Some(&eligible_row) = self.source_to_eligible.get(source_row) else {
                    return false;
                };
                if eligible_row == UNUSED_ROW {
                    continue;
                }
                let slot = self.cursors[eligible_row];
                self.row_constraints[slot] = constraint;
                self.cursors[eligible_row] += 1;
            }
        }
        true
    }

    fn reset_proposal_state(&mut self) {
        let constraints = self.patterns.len();
        let rows = self.eligible_rows.len();
        resize_and_fill(&mut self.log_p, constraints, 0.0);
        resize_and_fill(&mut self.p, constraints, 0.0);
        resize_and_fill(&mut self.middle_log_p, constraints, 0.0);
        resize_and_fill(&mut self.middle_p, constraints, 0.0);
        resize_and_fill(&mut self.gradient_p, constraints, 0.0);
        resize_and_fill(&mut self.middle_gradient_p, constraints, 0.0);
        resize_and_fill(&mut self.average_p, constraints, 0.0);
        resize_and_fill(&mut self.proposed, constraints, 0.0);
        resize_and_fill(&mut self.log_q, rows, 0.0);
        resize_and_fill(&mut self.q, rows, 0.0);
        resize_and_fill(&mut self.middle_log_q, rows, 0.0);
        resize_and_fill(&mut self.middle_q, rows, 0.0);
        resize_and_fill(&mut self.gradient_q, rows, 0.0);
        resize_and_fill(&mut self.middle_gradient_q, rows, 0.0);
        resize_and_fill(&mut self.floating_row_loads, rows, 0.0);
        resize_and_fill(&mut self.candidate_weights, constraints, 0);
        resize_and_fill(&mut self.best_weights, constraints, 0);
        resize_and_fill(&mut self.exact_row_loads, rows, 0);
        self.best_denominator = 0;
        self.best_numerator = 0;
    }

    /// Reuse is a new certificate, not authority inherited from a sibling.
    /// Cached weights absent from the current residual contribute zero; current
    /// constraints absent from the cache receive zero weight. A malformed
    /// identity, missing support, or arithmetic overflow omits this optional
    /// prune and leaves the ordinary proposal path available.
    fn recertify_cached_residual_bound(
        &mut self,
        support_by_pattern: &[Vec<usize>],
        target_words: &[u64],
        covered_words: &[u64],
        selected_rows: &[bool],
        excluded_row_words: &[u64],
    ) -> Option<usize> {
        if self.cached_patterns.is_empty()
            || self.cached_patterns.len() != self.cached_weights.len()
            || self.cached_patterns.len() > self.max_constraints
            || !self
                .cached_patterns
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            || target_words.len() != covered_words.len()
            || selected_rows.len() > self.max_rows
            || excluded_row_words.len() < selected_rows.len().div_ceil(u64::BITS as usize)
            || self.cached_patterns.iter().any(|&pattern| {
                pattern / u64::BITS as usize >= target_words.len()
                    || pattern >= support_by_pattern.len()
            })
        {
            return None;
        }

        // Scratch was reserved by try_new, alongside the cached, current-best,
        // and candidate vectors. No allocation or old/new-owner overlap is
        // hidden behind this otherwise ungoverned hot-path method.
        resize_and_fill(&mut self.exact_row_loads, selected_rows.len(), 0);
        let mut numerator = 0_u128;
        let mut constraints = 0_usize;
        let mut incidences = 0_usize;
        for (word_index, (target, covered)) in target_words.iter().zip(covered_words).enumerate() {
            let mut uncovered = target & !covered;
            while uncovered != 0 {
                let bit = uncovered.trailing_zeros() as usize;
                let pattern = word_index
                    .checked_mul(u64::BITS as usize)?
                    .checked_add(bit)?;
                let support = support_by_pattern.get(pattern)?;
                constraints = constraints.checked_add(1)?;
                if constraints > self.max_constraints {
                    return None;
                }
                let weight = self
                    .cached_patterns
                    .binary_search(&pattern)
                    .ok()
                    .map_or(0, |index| self.cached_weights[index]);
                numerator = numerator.checked_add(weight)?;
                let mut eligible = false;
                for &row in support {
                    let selected = *selected_rows.get(row)?;
                    if selected || row_is_excluded(excluded_row_words, row) {
                        continue;
                    }
                    eligible = true;
                    incidences = incidences.checked_add(1)?;
                    if incidences > self.max_incidences {
                        return None;
                    }
                    let load = self.exact_row_loads.get_mut(row)?;
                    *load = load.checked_add(weight)?;
                }
                // An uncovered constraint with no eligible support is left to
                // the existing exact infeasibility authority, not this cache.
                if !eligible {
                    return None;
                }
                uncovered &= uncovered - 1;
            }
        }
        if numerator == 0 {
            return None;
        }
        // Restored sibling rows participate in this maximum even if they were
        // absent when the weights were cached. Reusing the old denominator
        // here would make a previously valid lower bound unsound.
        let denominator =
            CERTIFICATE_SCALE.max(self.exact_row_loads.iter().copied().max().unwrap_or(0));
        usize::try_from(numerator.div_ceil(denominator)).ok()
    }

    fn remember_best_certificate(&mut self) {
        if self.best_numerator == 0
            || self.best_denominator == 0
            || self.patterns.len() != self.best_weights.len()
            || self.patterns.len() > self.cached_patterns.capacity()
            || self.best_weights.len() > self.cached_weights.capacity()
            || !self.patterns.windows(2).all(|pair| pair[0] < pair[1])
        {
            return;
        }
        // This is called only from a successful exact-certificate checkpoint.
        // A failed/partial prepare_residual must never relabel old weights.
        self.cached_patterns.clear();
        self.cached_weights.clear();
        for (&pattern, &weight) in self.patterns.iter().zip(&self.best_weights) {
            if weight != 0 {
                self.cached_patterns.push(pattern);
                self.cached_weights.push(weight);
            }
        }
    }

    fn certify_average_proposal(&mut self) -> Option<usize> {
        self.floating_row_loads.fill(0.0);
        for row in 0..self.eligible_rows.len() {
            let mut load = 0.0;
            for constraint in self.row_constraints[self.row_offsets[row]..self.row_offsets[row + 1]]
                .iter()
                .copied()
            {
                load += self.average_p[constraint];
            }
            if !load.is_finite() || load < 0.0 {
                return None;
            }
            self.floating_row_loads[row] = load;
        }
        let maximum_load = self
            .floating_row_loads
            .iter()
            .copied()
            .fold(0.0_f64, f64::max);
        if !maximum_load.is_finite() || maximum_load <= 0.0 {
            return None;
        }
        for constraint in 0..self.patterns.len() {
            let value = self.average_p[constraint] / maximum_load;
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            self.proposed[constraint] = value;
            let scaled = value * CERTIFICATE_SCALE as f64;
            if !scaled.is_finite() || scaled >= u128::MAX as f64 {
                return None;
            }
            self.candidate_weights[constraint] = scaled.floor() as u128;
        }
        self.exact_row_loads.fill(0);
        for row in 0..self.eligible_rows.len() {
            let mut load = 0_u128;
            for constraint in self.row_constraints[self.row_offsets[row]..self.row_offsets[row + 1]]
                .iter()
                .copied()
            {
                load = load.checked_add(self.candidate_weights[constraint])?;
            }
            self.exact_row_loads[row] = load;
        }
        let maximum_exact_load = self.exact_row_loads.iter().copied().max().unwrap_or(0);
        let denominator = CERTIFICATE_SCALE.max(maximum_exact_load);
        let numerator = self
            .candidate_weights
            .iter()
            .try_fold(0_u128, |sum, weight| sum.checked_add(*weight))?;
        if denominator == 0 || numerator == 0 {
            return None;
        }
        let bound = usize::try_from(numerator.div_ceil(denominator)).ok()?;
        if self.best_denominator == 0
            || numerator.checked_mul(self.best_denominator)?
                > self.best_numerator.checked_mul(denominator)?
        {
            self.best_weights.copy_from_slice(&self.candidate_weights);
            self.best_denominator = denominator;
            self.best_numerator = numerator;
            self.remember_best_certificate();
        }
        Some(bound)
    }

    #[cfg(any(test, feature = "diagnostic-probes"))]
    pub(super) fn set_remaining_iterations_for_test(&mut self, remaining: usize) {
        self.remaining_iterations = remaining;
    }
}

#[cfg(feature = "diagnostic-probes")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SoftmaxCutoffDensity {
    entries: u64,
    cutoff_entries: u64,
    row_incidences: u64,
    cutoff_row_incidences: u64,
}

#[cfg(feature = "diagnostic-probes")]
fn diagnostic_softmax_cutoff_density(
    log_values: &[f64],
    output: &mut [f64],
    row_offsets: Option<&[usize]>,
    sparse_proposal_softmax: bool,
) -> (bool, SoftmaxCutoffDensity) {
    let mut density = SoftmaxCutoffDensity::default();
    if log_values.is_empty() || log_values.len() != output.len() {
        return (false, density);
    }
    if let Some(offsets) = row_offsets {
        if offsets.len() != log_values.len() + 1 {
            return (false, density);
        }
        density.row_incidences = u64::try_from(*offsets.last().unwrap_or(&0)).unwrap_or(u64::MAX);
    }
    density.entries = u64::try_from(log_values.len()).unwrap_or(u64::MAX);
    let maximum = log_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !maximum.is_finite() {
        return (false, density);
    }
    let mut total = 0.0;
    for (index, (output, log_value)) in output.iter_mut().zip(log_values).enumerate() {
        let delta = *log_value - maximum;
        let below_cutoff = delta <= PROPOSAL_LOG_CUTOFF;
        if below_cutoff {
            density.cutoff_entries = density.cutoff_entries.saturating_add(1);
            if let Some(offsets) = row_offsets {
                let incidences = offsets[index + 1].saturating_sub(offsets[index]);
                density.cutoff_row_incidences = density
                    .cutoff_row_incidences
                    .saturating_add(u64::try_from(incidences).unwrap_or(u64::MAX));
            }
        }
        // This cutoff changes only the floating proposal. Every accepted
        // lower bound is still independently recertified from checked-u128
        // weights and exact eligible-row loads before it can prune.
        let value = if sparse_proposal_softmax && below_cutoff {
            0.0
        } else {
            delta.exp()
        };
        if !value.is_finite() || value < 0.0 {
            return (false, density);
        }
        *output = value;
        total += value;
    }
    if !total.is_finite() || total <= 0.0 {
        return (false, density);
    }
    for value in output {
        *value /= total;
    }
    (true, density)
}

#[cfg(any(test, not(feature = "diagnostic-probes")))]
fn softmax(log_values: &[f64], output: &mut [f64]) -> bool {
    if log_values.is_empty() || log_values.len() != output.len() {
        return false;
    }
    let maximum = log_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !maximum.is_finite() {
        return false;
    }
    let mut total = 0.0;
    for (output, log_value) in output.iter_mut().zip(log_values) {
        let delta = *log_value - maximum;
        let value = if delta <= PROPOSAL_LOG_CUTOFF {
            0.0
        } else {
            delta.exp()
        };
        if !value.is_finite() || value < 0.0 {
            return false;
        }
        *output = value;
        total += value;
    }
    if !total.is_finite() || total <= 0.0 {
        return false;
    }
    for value in output {
        *value /= total;
    }
    true
}

fn saddle_gradients(
    row_offsets: &[usize],
    row_constraints: &[usize],
    constraint_distribution: &[f64],
    row_distribution: &[f64],
    constraint_gradient: &mut [f64],
    row_gradient: &mut [f64],
) -> Option<()> {
    if row_offsets.len() != row_distribution.len() + 1
        || constraint_gradient.len() != constraint_distribution.len()
        || row_gradient.len() != row_distribution.len()
    {
        return None;
    }
    constraint_gradient.fill(0.0);
    row_gradient.fill(0.0);
    for row in 0..row_distribution.len() {
        let start = *row_offsets.get(row)?;
        let end = *row_offsets.get(row + 1)?;
        let incidence = row_constraints.get(start..end)?;
        let mut load = 0.0;
        for constraint in incidence.iter().copied() {
            *constraint_gradient.get_mut(constraint)? += row_distribution[row];
            load += *constraint_distribution.get(constraint)?;
        }
        if !load.is_finite() {
            return None;
        }
        row_gradient[row] = load;
    }
    constraint_gradient
        .iter()
        .all(|value| value.is_finite())
        .then_some(())
}

fn resize_and_fill<T: Copy>(values: &mut Vec<T>, length: usize, value: T) {
    values.resize(length, value);
    values.fill(value);
}

fn try_reserved_vec<T, F>(
    capacity: usize,
    external_live_bytes: u128,
    owned_live_bytes: &mut u128,
    memory_guard: &mut F,
    component: &'static str,
) -> Result<Vec<T>, ExactMinimumCoverError>
where
    F: FnMut(u128) -> Result<(), ExactMinimumCoverError> + ?Sized,
{
    let requested = (capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        external_live_bytes
            .checked_add(*owned_live_bytes)
            .and_then(|bytes| bytes.checked_add(requested))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ExactMinimumCoverError::AllocationFailed { component })?;
    let actual =
        checked_vec_retained_bytes(&values).ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    *owned_live_bytes = owned_live_bytes
        .checked_add(actual)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        external_live_bytes
            .checked_add(*owned_live_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(values)
}

fn checked_vec_retained_bytes<T>(values: &Vec<T>) -> Option<u128> {
    (values.capacity() as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn row_is_excluded(excluded_row_words: &[u64], row: usize) -> bool {
    excluded_row_words
        .get(row / u64::BITS as usize)
        .is_some_and(|word| word & (1_u64 << (row % u64::BITS as usize)) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(rows: usize, constraints: usize, incidences: usize) -> DualProposalWorkspace {
        DualProposalWorkspace::try_new(rows, constraints, incidences, 0, &mut |_| Ok(()))
            .expect("workspace")
    }

    fn exact_optimum(support_masks: &[usize], row_count: usize) -> usize {
        (0..(1_usize << row_count))
            .filter(|chosen| support_masks.iter().all(|support| support & chosen != 0))
            .map(|chosen| chosen.count_ones() as usize)
            .min()
            .expect("coverable")
    }

    fn seed_cached_weights(workspace: &mut DualProposalWorkspace, entries: &[(usize, u128)]) {
        workspace.cached_patterns.clear();
        workspace.cached_weights.clear();
        for &(pattern, weight) in entries {
            workspace.cached_patterns.push(pattern);
            workspace.cached_weights.push(weight);
        }
    }

    #[test]
    fn residual_cache_recertifies_rows_restored_after_sibling_exclusion() {
        let supports = vec![vec![0, 2], vec![1, 2]];
        let selected = [false; 3];
        let mut workspace = workspace(3, 2, 4);
        seed_cached_weights(
            &mut workspace,
            &[(0, CERTIFICATE_SCALE), (1, CERTIFICATE_SCALE)],
        );
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[3], &[0], &selected, &[4]),
            Some(2),
        );
        // Restoring row 2 doubles the exact capacity denominator. Keeping the
        // former denominator would incorrectly prune the valid one-row cover.
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[3], &[0], &selected, &[0]),
            Some(1),
        );
        assert_eq!(workspace.exact_row_loads[2], 2 * CERTIFICATE_SCALE);
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[3], &[0], &selected, &[4]),
            Some(2),
        );
    }

    #[test]
    fn residual_cache_remaps_actual_ids_and_rejects_missing_identity_or_overflow() {
        let supports = vec![vec![0], vec![1], vec![2]];
        let selected = [false; 3];
        let mut workspace = workspace(3, 3, 6);
        seed_cached_weights(
            &mut workspace,
            &[(0, CERTIFICATE_SCALE), (2, CERTIFICATE_SCALE)],
        );
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[7], &[1], &selected, &[0]),
            Some(1),
            "new uncovered IDs [1,2] receive weights [0,S], not old positions [S,S]",
        );
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports[..2], &[7], &[1], &selected, &[0]),
            None,
        );
        seed_cached_weights(&mut workspace, &[(usize::MAX, CERTIFICATE_SCALE)]);
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[7], &[0], &selected, &[0]),
            None,
        );
        seed_cached_weights(&mut workspace, &[(0, u128::MAX), (1, 1)]);
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[3], &[0], &selected, &[0]),
            None,
        );
        workspace.cached_weights.pop();
        assert_eq!(
            workspace.recertify_cached_residual_bound(&supports, &[3], &[0], &selected, &[0]),
            None,
        );
    }

    #[test]
    fn strong_cache_precedes_iteration_exhaustion_but_weak_cache_preserves_proposals() {
        let supports = vec![vec![0, 2], vec![1, 2]];
        let selected = [false; 3];
        let mut workspace = workspace(3, 2, 4);
        seed_cached_weights(
            &mut workspace,
            &[(0, CERTIFICATE_SCALE), (1, CERTIFICATE_SCALE)],
        );
        workspace.set_remaining_iterations_for_test(0);
        assert_eq!(
            workspace.certified_residual_lower_bound(&supports, &[3], &[0], &selected, &[4], 1),
            Some(2),
            "an exact cached prune needs neither a minimum dimension nor fresh proposal iterations",
        );
        assert_eq!(workspace.remaining_proposal_iterations(), 0);
        assert_eq!(
            workspace.certified_residual_lower_bound_inner(
                &supports,
                &[3],
                &[0],
                &selected,
                &[0],
                1,
                false,
            ),
            None,
            "a weak cache must not masquerade as a fresh proposal after budget exhaustion",
        );
        assert_eq!(
            workspace.certified_residual_lower_bound_inner(
                &supports,
                &[3],
                &[0],
                &selected,
                &[4],
                usize::MAX,
                false,
            ),
            None,
            "root-style exports retain the original iteration policy",
        );
        workspace.set_remaining_iterations_for_test(MIRROR_PROX_ITERATIONS);
        let bound = workspace.certified_residual_lower_bound_inner(
            &supports,
            &[3],
            &[0],
            &selected,
            &[0],
            1,
            false,
        );
        assert_eq!(bound, Some(1));
        assert_eq!(workspace.remaining_proposal_iterations(), 0);
    }

    #[test]
    fn residual_cache_checkpoint_and_failed_prepare_do_not_relabel_weights() {
        let supports = vec![vec![0], vec![1], vec![0, 1]];
        let mut workspace = workspace(2, 3, 4);
        assert!(workspace
            .certified_residual_lower_bound_inner(
                &supports,
                &[7],
                &[0],
                &[false; 2],
                &[0],
                usize::MAX,
                false,
            )
            .is_some());
        assert!(!workspace.cached_patterns.is_empty());
        assert_eq!(
            workspace.cached_patterns.len(),
            workspace.cached_weights.len()
        );
        assert!(workspace.cached_weights.iter().all(|&weight| weight != 0));
        let previous_patterns = workspace.cached_patterns.clone();
        let previous_weights = workspace.cached_weights.clone();
        assert_eq!(
            workspace.certified_residual_lower_bound_inner(
                &supports[..1],
                &[7],
                &[0],
                &[false; 2],
                &[0],
                usize::MAX,
                false,
            ),
            None,
        );
        assert_eq!(workspace.cached_patterns, previous_patterns);
        assert_eq!(workspace.cached_weights, previous_weights);
    }

    #[test]
    fn residual_cache_retains_guarded_old_new_and_clone_capacities_without_hot_allocations() {
        let mut workspace = workspace(3, 3, 9);
        let retained = workspace.retained_bytes();
        let cache_capacity = (
            workspace.cached_patterns.capacity(),
            workspace.cached_weights.capacity(),
        );
        assert!(cache_capacity.0 >= 3 && cache_capacity.1 >= 3);
        seed_cached_weights(
            &mut workspace,
            &[(0, CERTIFICATE_SCALE), (2, CERTIFICATE_SCALE)],
        );
        let mut cloned = workspace.clone();
        assert_eq!(cloned.cached_patterns, workspace.cached_patterns);
        assert_eq!(cloned.cached_weights, workspace.cached_weights);
        assert_eq!(cloned.cached_patterns.capacity(), cache_capacity.0);
        assert_eq!(cloned.cached_weights.capacity(), cache_capacity.1);
        let supports = vec![vec![0], vec![1], vec![2]];
        assert_eq!(
            cloned.recertify_cached_residual_bound(&supports, &[7], &[0], &[false; 3], &[0]),
            Some(2),
        );
        assert_eq!(cloned.actual_retained_bytes(), Some(retained));
        assert_eq!(cloned.retained_bytes(), retained);
        let cap = 37 + retained - 1;
        assert!(matches!(
            DualProposalWorkspace::try_new(3, 3, 9, 37, &mut |bytes| {
                if bytes > cap {
                    Err(ExactMinimumCoverError::MemoryGuardRejected)
                } else {
                    Ok(())
                }
            }),
            Err(ExactMinimumCoverError::MemoryGuardRejected),
        ));
    }

    #[test]
    fn residual_cache_bound_matches_tiny_exhaustive_residual_and_sibling_parity() {
        const ROWS: usize = 3;
        let selected = [false; ROWS];
        let mut workspace = workspace(ROWS, 3, ROWS * 3);
        seed_cached_weights(
            &mut workspace,
            &[
                (0, CERTIFICATE_SCALE),
                (1, CERTIFICATE_SCALE / 2),
                (2, CERTIFICATE_SCALE * 2),
            ],
        );
        let mut certified_cases = 0;
        for first in 1_usize..8 {
            for second in 1_usize..8 {
                for third in 1_usize..8 {
                    let masks = [first, second, third];
                    let supports: Vec<_> = masks
                        .iter()
                        .map(|mask| {
                            (0..ROWS)
                                .filter(|row| mask & (1 << row) != 0)
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    for excluded in (0_u64..8).rev() {
                        for covered in 0_u64..8 {
                            let optimum = (0_usize..8)
                                .filter(|chosen| *chosen as u64 & excluded == 0)
                                .filter(|chosen| {
                                    masks.iter().enumerate().all(|(pattern, mask)| {
                                        covered & (1 << pattern) != 0 || mask & chosen != 0
                                    })
                                })
                                .map(|chosen| chosen.count_ones() as usize)
                                .min();
                            let bound = workspace.recertify_cached_residual_bound(
                                &supports,
                                &[7],
                                &[covered],
                                &selected,
                                &[excluded],
                            );
                            if let (Some(bound), Some(optimum)) = (bound, optimum) {
                                assert!(bound <= optimum, "cached certificate must not over-prune");
                                certified_cases += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(certified_cases > 1_000);
    }

    #[test]
    fn residual_cache_keeps_selected_rows_and_covered_constraints_consistent() {
        // Cover masks are derived from selected rows, as in the production
        // DFS. Selected rows cannot be charged again to the residual optimum.
        let masks = [0b011_u64, 0b110, 0b101];
        let supports = vec![vec![0, 2], vec![0, 1], vec![1, 2]];
        let mut workspace = workspace(3, 3, 6);
        seed_cached_weights(
            &mut workspace,
            &[
                (0, CERTIFICATE_SCALE),
                (1, CERTIFICATE_SCALE / 2),
                (2, CERTIFICATE_SCALE * 2),
            ],
        );
        let mut checked = 0;
        for selected_mask in 0_u64..8 {
            let selected = std::array::from_fn::<_, 3, _>(|row| selected_mask & (1 << row) != 0);
            let covered = masks.iter().enumerate().fold(0, |covered, (row, mask)| {
                covered | if selected[row] { *mask } else { 0 }
            });
            for excluded in 0_u64..8 {
                if selected_mask & excluded != 0 {
                    continue;
                }
                let optimum = (0_u64..8)
                    .filter(|chosen| chosen & (excluded | selected_mask) == 0)
                    .filter(|chosen| {
                        masks
                            .iter()
                            .enumerate()
                            .fold(covered, |covered, (row, mask)| {
                                covered | if chosen & (1 << row) != 0 { *mask } else { 0 }
                            })
                            == 7
                    })
                    .map(u64::count_ones)
                    .min();
                let bound = workspace.recertify_cached_residual_bound(
                    &supports,
                    &[7],
                    &[covered],
                    &selected,
                    &[excluded],
                );
                if let (Some(bound), Some(optimum)) = (bound, optimum) {
                    assert!(bound <= optimum as usize);
                    checked += 1;
                }
            }
        }
        assert!(checked >= 10);
    }

    #[test]
    fn production_admission_has_explicit_dimension_bounds() {
        assert!(!should_attempt_residual_dual(4, 3, 6, 2));
        assert!(!should_attempt_residual_dual(64, 128, 5, 18));
        assert!(!should_attempt_residual_dual(257, 128, 6, 18));
        assert!(!should_attempt_residual_dual(64, 4_097, 6, 18));
        assert!(!should_attempt_residual_dual(64, 128, 6, 19));
        assert!(should_attempt_residual_dual(64, 128, 6, 18));
        assert!(should_prepare_root_dual(64, 128));
    }

    #[test]
    fn projection_accounts_for_every_owned_vector_family() {
        let projection =
            checked_residual_dual_memory_projection(158, 1_456, 16_078).expect("projection");
        assert!(projection.index_bytes > 0);
        assert!(projection.floating_workspace_bytes > 0);
        assert!(projection.certificate_bytes > 0);
        assert_eq!(
            projection.required_peak_bytes,
            projection.index_bytes
                + projection.floating_workspace_bytes
                + projection.certificate_bytes
        );
    }

    #[test]
    fn allocator_capacities_are_reported_after_every_reserve() {
        let projection = checked_residual_dual_memory_projection(64, 128, 256)
            .expect("projection")
            .required_peak_bytes;
        let mut observed = 0_u128;
        let workspace = DualProposalWorkspace::try_new(64, 128, 256, 37, &mut |bytes| {
            observed = observed.max(bytes);
            Ok(())
        })
        .expect("workspace");
        assert_eq!(observed, 37 + workspace.retained_bytes());
        assert!(workspace.retained_bytes() <= projection);
    }

    #[test]
    fn clone_preserves_admitted_reusable_buffer_capacities() {
        let workspace = workspace(64, 128, 256);
        #[cfg(feature = "diagnostic-probes")]
        let workspace = {
            let mut workspace = workspace;
            workspace.set_diagnostic_sparse_proposal_softmax(true);
            workspace
        };
        assert!(workspace.patterns.is_empty());
        assert!(workspace.row_constraints.is_empty());
        let cloned = workspace.clone();
        assert_eq!(cloned.patterns.capacity(), workspace.patterns.capacity());
        assert_eq!(
            cloned.row_constraints.capacity(),
            workspace.row_constraints.capacity()
        );
        assert_eq!(cloned.log_p.capacity(), workspace.log_p.capacity());
        assert_eq!(cloned.log_q.capacity(), workspace.log_q.capacity());
        assert_eq!(cloned.retained_bytes(), workspace.retained_bytes());
        assert_eq!(
            cloned.actual_retained_bytes(),
            Some(cloned.retained_bytes())
        );
        #[cfg(feature = "diagnostic-probes")]
        assert!(cloned.diagnostic_sparse_proposal_softmax);
    }

    #[cfg(feature = "diagnostic-probes")]
    #[test]
    fn diagnostic_sparse_softmax_skips_only_proposal_tail() {
        let log_values = [0.0, -1.0, -35.999, -36.0, -100.0];
        let mut production_sparse = [0.0; 5];
        let mut profiled_dense = [0.0; 5];
        let mut sparse = [0.0; 5];
        assert!(softmax(&log_values, &mut production_sparse));
        let (dense_valid, dense_density) =
            diagnostic_softmax_cutoff_density(&log_values, &mut profiled_dense, None, false);
        assert!(dense_valid);
        assert_eq!(dense_density.entries, 5);
        assert_eq!(dense_density.cutoff_entries, 2);
        assert!(profiled_dense[3] > 0.0);
        assert!(profiled_dense[4] > 0.0);

        let (sparse_valid, sparse_density) =
            diagnostic_softmax_cutoff_density(&log_values, &mut sparse, None, true);
        assert!(sparse_valid);
        assert_eq!(sparse_density, dense_density);
        assert_eq!(sparse, production_sparse);
        assert!(sparse[0] > 0.0);
        assert!(sparse[1] > 0.0);
        assert!(sparse[2] > 0.0);
        assert_eq!(sparse[3], 0.0);
        assert_eq!(sparse[4], 0.0);
        assert!((sparse.iter().sum::<f64>() - 1.0).abs() <= f64::EPSILON * 4.0);
    }

    #[cfg(feature = "diagnostic-probes")]
    #[test]
    fn diagnostic_sparse_proposal_remains_exactly_recertified_and_admissible() {
        let supports = vec![vec![0, 1], vec![1, 2], vec![0, 2], vec![2, 3]];
        let target = [0b1111_u64];
        let covered = [0_u64];
        let selected = [false; 4];
        let excluded = [0_u64];
        let masks = [0b0011_usize, 0b0110, 0b0101, 0b1100];
        let optimum = exact_optimum(&masks, 4);
        let mut workspace = workspace(4, 4, 8);
        let retained = workspace.retained_bytes();
        workspace.set_diagnostic_sparse_proposal_softmax(true);
        assert_eq!(workspace.retained_bytes(), retained);
        let bound = workspace
            .certified_residual_lower_bound_inner(
                &supports,
                &target,
                &covered,
                &selected,
                &excluded,
                usize::MAX,
                false,
            )
            .expect("checked-u128 sparse proposal certificate");
        assert!(bound <= optimum);
    }

    #[test]
    fn actual_residual_admission_counts_incident_rows() {
        let supports = vec![vec![0]; MIN_DUAL_CONSTRAINT_COUNT];
        let target = vec![u64::MAX; MIN_DUAL_CONSTRAINT_COUNT / u64::BITS as usize];
        let covered = vec![0_u64; target.len()];
        let selected = vec![false; MIN_DUAL_ROW_COUNT];
        let excluded = vec![0_u64];
        let mut workspace = workspace(
            MIN_DUAL_ROW_COUNT,
            MIN_DUAL_CONSTRAINT_COUNT,
            MIN_DUAL_CONSTRAINT_COUNT,
        );
        assert_eq!(
            workspace.certified_residual_lower_bound(
                &supports, &target, &covered, &selected, &excluded, 18,
            ),
            None
        );
    }

    #[test]
    fn exact_integer_certificate_rejects_nonfinite_zero_and_overflow() {
        assert!(!softmax(&[f64::NAN], &mut [0.0]));
        assert!(!softmax(&[f64::INFINITY], &mut [0.0]));

        let mut workspace = workspace(1, 2, 2);
        workspace.patterns.extend([0, 1]);
        workspace.eligible_rows.push(0);
        workspace.row_offsets.extend([0, 2]);
        workspace.row_constraints.extend([0, 1]);
        resize_and_fill(&mut workspace.average_p, 2, 0.0);
        resize_and_fill(&mut workspace.candidate_weights, 2, 0);
        resize_and_fill(&mut workspace.best_weights, 2, 0);
        resize_and_fill(&mut workspace.exact_row_loads, 1, 0);
        resize_and_fill(&mut workspace.floating_row_loads, 1, 0.0);
        assert_eq!(workspace.certify_average_proposal(), None);
        workspace.average_p.fill(f64::MAX);
        assert_eq!(workspace.certify_average_proposal(), None);
    }

    #[test]
    fn proposal_bound_is_admissible_for_every_small_nonempty_support_matrix() {
        const ROWS: usize = 4;
        const CONSTRAINTS: usize = 3;
        const SUPPORT_VARIANTS: usize = 1 << ROWS;
        let target = vec![(1_u64 << CONSTRAINTS) - 1];
        let covered = vec![0_u64];
        let selected = vec![false; ROWS];
        let excluded = vec![0_u64];
        let mut cases = 0;
        for first in 1..SUPPORT_VARIANTS {
            for second in 1..SUPPORT_VARIANTS {
                for third in 1..SUPPORT_VARIANTS {
                    let masks = [first, second, third];
                    let supports = masks
                        .iter()
                        .map(|mask| {
                            (0..ROWS)
                                .filter(|row| mask & (1 << row) != 0)
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let incidences = supports.iter().map(Vec::len).sum();
                    let mut workspace = workspace(ROWS, CONSTRAINTS, incidences);
                    let bound = workspace
                        .certified_residual_lower_bound_inner(
                            &supports,
                            &target,
                            &covered,
                            &selected,
                            &excluded,
                            usize::MAX,
                            false,
                        )
                        .expect("certificate");
                    assert!(bound <= exact_optimum(&masks, ROWS));
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 3_375);
    }

    #[test]
    fn root_certificate_remains_admissible_after_selection_and_exclusion() {
        const ROWS: usize = 4;
        let supports = vec![vec![0, 1], vec![1, 2], vec![0, 2], vec![2, 3]];
        let target = vec![0b1111_u64];
        let root_covered = vec![0_u64];
        let root_selected = vec![false; ROWS];
        let root_excluded = vec![0_u64];
        let mut workspace = workspace(ROWS, 4, 8);
        let certificate = workspace
            .certified_residual_lower_bound_inner(
                &supports,
                &target,
                &root_covered,
                &root_selected,
                &root_excluded,
                usize::MAX,
                false,
            )
            .map(|_| CertifiedResidualDual {
                patterns: workspace.patterns.clone(),
                weights: workspace.best_weights.clone(),
                denominator: workspace.best_denominator,
            })
            .expect("root certificate");

        for selected_mask in 0_usize..(1 << ROWS) {
            let covered = supports
                .iter()
                .enumerate()
                .fold(0_u64, |bits, (pattern, support)| {
                    if support.iter().any(|row| selected_mask & (1 << row) != 0) {
                        bits | (1 << pattern)
                    } else {
                        bits
                    }
                });
            for excluded_mask in 0_usize..(1 << ROWS) {
                if selected_mask & excluded_mask != 0 {
                    continue;
                }
                let available = ((1 << ROWS) - 1) & !selected_mask & !excluded_mask;
                let residual = (0_usize..(1 << ROWS))
                    .filter(|chosen| chosen & !available == 0)
                    .filter(|chosen| {
                        supports.iter().enumerate().all(|(pattern, support)| {
                            covered & (1 << pattern) != 0
                                || support.iter().any(|row| chosen & (1 << row) != 0)
                        })
                    })
                    .map(|chosen| chosen.count_ones() as usize)
                    .min();
                let Some(residual) = residual else { continue };
                let bound = certificate
                    .certified_lower_bound_for_uncovered(&target, &[covered])
                    .expect("bound");
                assert!(bound <= residual);
            }
        }
    }

    #[test]
    fn global_iteration_budget_stops_optional_proposals() {
        let supports = vec![vec![0], vec![1], vec![0, 1]];
        let mut workspace = workspace(2, 3, 4);
        workspace.set_remaining_iterations_for_test(0);
        assert_eq!(
            workspace.certified_residual_lower_bound_inner(
                &supports,
                &[0b111],
                &[0],
                &[false, false],
                &[0],
                usize::MAX,
                false,
            ),
            None
        );
    }

    #[test]
    fn proposal_budget_charges_executed_iterations_not_reserved_allowance() {
        let supports = vec![vec![0], vec![1], vec![0, 1]];
        let target = [0b111];
        let covered = [0];
        let selected = [false, false];
        let excluded = [0];

        let mut early_prune = workspace(2, 3, 4);
        early_prune.set_remaining_iterations_for_test(1_000);
        let bound = early_prune
            .certified_residual_lower_bound_inner_with_iteration_limit(
                &supports,
                &target,
                &covered,
                &selected,
                &excluded,
                0,
                false,
                MIRROR_PROX_ITERATIONS,
            )
            .expect("first certificate checkpoint must prove a positive bound");
        assert!(bound > 0);
        assert_eq!(early_prune.remaining_proposal_iterations(), 975);

        let mut full_call = workspace(2, 3, 4);
        full_call.set_remaining_iterations_for_test(1_000);
        assert!(full_call
            .certified_residual_lower_bound_inner_with_iteration_limit(
                &supports,
                &target,
                &covered,
                &selected,
                &excluded,
                usize::MAX,
                false,
                MIRROR_PROX_ITERATIONS,
            )
            .is_some());
        assert_eq!(full_call.remaining_proposal_iterations(), 800);
    }
}
