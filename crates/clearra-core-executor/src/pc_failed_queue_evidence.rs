// SRP rationale: this module has one change reason: exact failed-queue evidence ownership for PC execution.
use std::sync::Arc;

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    resource::ResourceReport,
};
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    reducer::pattern_coverage_aggregation::{
        PatternCoverageAggregation, PatternCoverageCompleteness,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        coverage_universe_guard::CoverageUniverseGuard, pattern_universe_id::PatternUniverseId,
        pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_problem::{SearchProblem, SearchProblemPreset};

use crate::resource::ExecutionMemoryBound;

#[derive(Clone)]
pub struct PcFailedQueueExecutionAuthority {
    inner: Arc<PcFailedQueueExecutionAuthorityInner>,
}

struct PcFailedQueueExecutionAuthorityInner {
    problem: Arc<SearchProblem>,
}

impl PcFailedQueueExecutionAuthority {
    pub(crate) fn new(problem: Arc<SearchProblem>) -> Self {
        Self {
            inner: Arc::new(PcFailedQueueExecutionAuthorityInner { problem }),
        }
    }

    pub fn problem(&self) -> &SearchProblem {
        self.inner.problem.as_ref()
    }

    pub fn matches_problem_owner(&self, problem: &Arc<SearchProblem>) -> bool {
        Arc::ptr_eq(&self.inner.problem, problem)
    }

    pub fn same_execution(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl core::fmt::Debug for PcFailedQueueExecutionAuthority {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("PcFailedQueueExecutionAuthority")
            .field("problem_id", &self.problem().problem_id())
            .finish_non_exhaustive()
    }
}

impl PartialEq for PcFailedQueueExecutionAuthority {
    fn eq(&self, other: &Self) -> bool {
        self.same_execution(other)
    }
}

impl Eq for PcFailedQueueExecutionAuthority {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcFailedQueueExampleEvidence {
    pattern_index: usize,
    pieces: Vec<PieceKind>,
}

impl PcFailedQueueExampleEvidence {
    pub const fn pattern_index(&self) -> usize {
        self.pattern_index
    }

    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcFailedQueueMemoryReport {
    admission_cap_bytes: u128,
    observed_execution_bytes: u128,
    source_word_materialization_upper_bound_bytes: u128,
    union_constructor_peak_bytes: u128,
    example_materialization_upper_bound_bytes: u128,
    admitted_producer_peak_bytes: u128,
    retained_union_bytes: u128,
    retained_example_bytes: u128,
    retained_producer_bytes: u128,
}

impl PcFailedQueueMemoryReport {
    pub const fn admission_cap_bytes(self) -> u128 {
        self.admission_cap_bytes
    }

    pub const fn observed_execution_bytes(self) -> u128 {
        self.observed_execution_bytes
    }

    pub const fn source_word_materialization_upper_bound_bytes(self) -> u128 {
        self.source_word_materialization_upper_bound_bytes
    }

    pub const fn union_constructor_peak_bytes(self) -> u128 {
        self.union_constructor_peak_bytes
    }

    pub const fn example_materialization_upper_bound_bytes(self) -> u128 {
        self.example_materialization_upper_bound_bytes
    }

    pub const fn admitted_producer_peak_bytes(self) -> u128 {
        self.admitted_producer_peak_bytes
    }

    pub const fn retained_union_bytes(self) -> u128 {
        self.retained_union_bytes
    }

    pub const fn retained_example_bytes(self) -> u128 {
        self.retained_example_bytes
    }

    pub const fn retained_producer_bytes(self) -> u128 {
        self.retained_producer_bytes
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PcFailedQueueEvidence {
    authority: PcFailedQueueExecutionAuthority,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    source_row_count: usize,
    success_coverage: PatternBitSet,
    success_pattern_count: usize,
    failed_pattern_count: usize,
    success_probability: ProbabilityValue,
    failed_probability: ProbabilityValue,
    materialized_probability_mass: ProbabilityValue,
    examples: Vec<PcFailedQueueExampleEvidence>,
    memory_report: PcFailedQueueMemoryReport,
}

impl PcFailedQueueEvidence {
    pub fn authority(&self) -> &PcFailedQueueExecutionAuthority {
        &self.authority
    }

    pub fn problem(&self) -> &SearchProblem {
        self.authority.problem()
    }

    pub fn matches_problem_owner(&self, problem: &Arc<SearchProblem>) -> bool {
        self.authority.matches_problem_owner(problem)
    }

    pub const fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }

    pub const fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }

    pub const fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn source_row_count(&self) -> usize {
        self.source_row_count
    }

    pub fn success_coverage(&self) -> &PatternBitSet {
        &self.success_coverage
    }

    pub const fn success_pattern_count(&self) -> usize {
        self.success_pattern_count
    }

    pub const fn failed_pattern_count(&self) -> usize {
        self.failed_pattern_count
    }

    pub const fn success_probability(&self) -> ProbabilityValue {
        self.success_probability
    }

    pub const fn failed_probability(&self) -> ProbabilityValue {
        self.failed_probability
    }

    pub const fn materialized_probability_mass(&self) -> ProbabilityValue {
        self.materialized_probability_mass
    }

    pub fn examples(&self) -> &[PcFailedQueueExampleEvidence] {
        &self.examples
    }

    pub const fn memory_report(&self) -> PcFailedQueueMemoryReport {
        self.memory_report
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcFailedQueueSourceCompleteness {
    packing_complete: bool,
    buildup_count_complete: bool,
    materialized_coverage_complete: bool,
    objective_complete: bool,
}

impl PcFailedQueueSourceCompleteness {
    pub(crate) const fn new(
        packing_complete: bool,
        buildup_count_complete: bool,
        materialized_coverage_complete: bool,
        objective_complete: bool,
    ) -> Self {
        Self {
            packing_complete,
            buildup_count_complete,
            materialized_coverage_complete,
            objective_complete,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PcFailedQueueProducerAdmission {
    memory_bound: ExecutionMemoryBound,
    observed_execution_bytes: u128,
}

impl PcFailedQueueProducerAdmission {
    pub(crate) const fn new(
        memory_bound: ExecutionMemoryBound,
        observed_execution_bytes: u128,
    ) -> Self {
        Self {
            memory_bound,
            observed_execution_bytes,
        }
    }

    fn ensure(&self, producer_peak_bytes: u128) -> Result<(), PcFailedQueueEvidenceError> {
        self.memory_bound
            .ensure(self.observed_execution_bytes, producer_peak_bytes)
            .map_err(PcFailedQueueEvidenceError::MemoryAdmission)
    }
}

pub(crate) struct PcFailedQueueEvidenceProducer;

impl PcFailedQueueEvidenceProducer {
    pub(crate) fn produce(
        authority: PcFailedQueueExecutionAuthority,
        rows: &[CoverageRow],
        example_limit: usize,
        completeness: PcFailedQueueSourceCompleteness,
        admission: PcFailedQueueProducerAdmission,
    ) -> Result<PcFailedQueueEvidence, PcFailedQueueEvidenceError> {
        produce_from_raw_rows(authority, rows, example_limit, completeness, admission)
    }
}

trait RawFailedQueueCoverageRow {
    fn candidate_id(&self) -> u64;
    fn row_kind(&self) -> &CoverageRowKind;
    fn piece_source_id(&self) -> u64;
    fn pattern_universe_id(&self) -> PatternUniverseId;
    fn pattern_weight_model_id(&self) -> PatternWeightModelId;
    fn pattern_count(&self) -> usize;
    fn raw_word_count(&self) -> usize;
    fn projected_word_materialization_bytes(&self, dense_word_bytes: u128) -> Option<u128>;
    fn raw_words(&self) -> &[u64];
}

impl RawFailedQueueCoverageRow for CoverageRow {
    fn candidate_id(&self) -> u64 {
        self.candidate_id()
    }

    fn row_kind(&self) -> &CoverageRowKind {
        self.row_kind()
    }

    fn piece_source_id(&self) -> u64 {
        self.piece_source_id()
    }

    fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id()
    }

    fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id()
    }

    fn pattern_count(&self) -> usize {
        self.pattern_count()
    }

    fn raw_word_count(&self) -> usize {
        self.coverage_bits().word_count()
    }

    fn projected_word_materialization_bytes(&self, dense_word_bytes: u128) -> Option<u128> {
        // `PatternBitSet::words` can populate a sparse row's dense OnceLock. Count
        // one full dense payload for every row before asking any row for words.
        Some(dense_word_bytes)
    }

    fn raw_words(&self) -> &[u64] {
        self.coverage_bits().words()
    }
}

fn produce_from_raw_rows<R: RawFailedQueueCoverageRow>(
    authority: PcFailedQueueExecutionAuthority,
    rows: &[R],
    example_limit: usize,
    completeness: PcFailedQueueSourceCompleteness,
    admission: PcFailedQueueProducerAdmission,
) -> Result<PcFailedQueueEvidence, PcFailedQueueEvidenceError> {
    validate_completeness(completeness)?;
    let problem = authority.problem();
    if !matches!(
        problem.preset(),
        SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc
    ) {
        return Err(PcFailedQueueEvidenceError::UnsupportedPreset);
    }

    let piece_source = problem.piece_source();
    let piece_source_id = piece_source.id().get();
    if piece_source_id == 0 {
        return Err(PcFailedQueueEvidenceError::ZeroPieceSourceId);
    }
    let universe = piece_source
        .materialized_universe()
        .ok_or(PcFailedQueueEvidenceError::MissingMaterializedPatternUniverse)?;
    if !piece_source.complete() || !universe.complete() {
        return Err(PcFailedQueueEvidenceError::IncompleteSourceUniverse);
    }
    let pattern_universe_id = universe.pattern_universe_id();
    if pattern_universe_id.get() == 0 {
        return Err(PcFailedQueueEvidenceError::ZeroPatternUniverseId);
    }
    let pattern_weight_model_id = universe.pattern_weight_model_id();
    if pattern_weight_model_id.get() == 0 {
        return Err(PcFailedQueueEvidenceError::ZeroPatternWeightModelId);
    }
    let pattern_count = universe.pattern_count();
    if pattern_count == 0 {
        return Err(PcFailedQueueEvidenceError::EmptyPatternUniverse);
    }
    let materialized_pattern_count = pattern_count as u128;
    if universe.total_possible_pattern_count() != materialized_pattern_count {
        return Err(
            PcFailedQueueEvidenceError::CompleteUniverseCardinalityMismatch {
                materialized: pattern_count,
                total_possible: universe.total_possible_pattern_count(),
            },
        );
    }
    if universe.weights().len() != pattern_count {
        return Err(PcFailedQueueEvidenceError::PatternWeightCountMismatch {
            expected: pattern_count,
            actual: universe.weights().len(),
        });
    }

    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let dense_word_bytes = checked_bytes::<u64>(word_count)?;
    let mut source_word_materialization_upper_bound_bytes = 0_u128;
    let mut previous_candidate_id = None;
    for (row_index, row) in rows.iter().enumerate() {
        validate_row_metadata(
            row_index,
            row,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            word_count,
            previous_candidate_id,
        )?;
        previous_candidate_id = Some(row.candidate_id());
        source_word_materialization_upper_bound_bytes =
            source_word_materialization_upper_bound_bytes
                .checked_add(
                    row.projected_word_materialization_bytes(dense_word_bytes)
                        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?,
                )
                .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    }

    let mut union_constructor_peak_bytes =
        checked_union_constructor_peak_bytes(word_count, word_count)?;
    let maximum_example_count = example_limit.min(pattern_count);
    let maximum_sequence_len = (0..pattern_count)
        .map(|pattern_index| universe.sequence_len_at(pattern_index))
        .max()
        .unwrap_or(0);
    let maximum_example_materialization_bytes = checked_example_bytes(
        maximum_example_count,
        maximum_example_count
            .checked_mul(maximum_sequence_len)
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?,
    )?;
    let preflight_peak_bytes = checked_producer_peak_bytes(
        source_word_materialization_upper_bound_bytes,
        union_constructor_peak_bytes,
        maximum_example_materialization_bytes,
    )?;
    admission.ensure(preflight_peak_bytes)?;
    let mut admitted_producer_peak_bytes = preflight_peak_bytes;

    let mut union_words = Vec::new();
    union_words
        .try_reserve_exact(word_count)
        .map_err(|_| PcFailedQueueEvidenceError::AllocationUnavailable)?;
    // `try_reserve_exact` is permitted to retain more capacity than requested.
    // Rebind the already-proven Arc conversion peak to that actual capacity
    // before reading any source row or allocating the final shared payload.
    union_constructor_peak_bytes =
        checked_union_constructor_peak_bytes(union_words.capacity(), word_count)?;
    let capacity_aware_preflight_peak_bytes = checked_producer_peak_bytes(
        source_word_materialization_upper_bound_bytes,
        union_constructor_peak_bytes,
        maximum_example_materialization_bytes,
    )?;
    admission.ensure(capacity_aware_preflight_peak_bytes)?;
    admitted_producer_peak_bytes =
        admitted_producer_peak_bytes.max(capacity_aware_preflight_peak_bytes);
    union_words.resize(word_count, 0_u64);
    for (row_index, row) in rows.iter().enumerate() {
        let raw_words = row.raw_words();
        validate_raw_words(row_index, pattern_count, word_count, raw_words)?;
        for (target, source) in union_words.iter_mut().zip(raw_words) {
            *target |= *source;
        }
    }
    validate_raw_words(usize::MAX, pattern_count, word_count, &union_words)?;

    let shared_union_words: Arc<[u64]> = union_words.into();
    let success_coverage = PatternBitSet::from_shared_words(pattern_count, shared_union_words)
        .map_err(PcFailedQueueEvidenceError::InvalidCoverageBitSet)?;
    let aggregation = PatternCoverageAggregation::from_success_coverage(
        CoverageUniverseGuard::new(pattern_universe_id, pattern_weight_model_id, pattern_count),
        rows.len(),
        &success_coverage,
        universe.weights(),
        PatternCoverageCompleteness::complete(),
    )
    .map_err(|_| PcFailedQueueEvidenceError::SharedCoverageAggregationInvalid)?;
    let success_pattern_count = aggregation.success_pattern_count();
    let failed_pattern_count = aggregation.failed_pattern_count();
    let success_probability = aggregation.success_probability();
    let failed_probability = aggregation.failed_probability();
    let materialized_probability_mass = aggregation.materialized_probability_mass();
    validate_complete_materialized_probability_mass(materialized_probability_mass)?;

    let example_count = example_limit.min(failed_pattern_count);
    let mut example_piece_count = 0_usize;
    let mut remaining_examples = example_count;
    for pattern_index in 0..pattern_count {
        if remaining_examples == 0 {
            break;
        }
        if success_coverage.contains(PatternId::new(pattern_index)) {
            continue;
        }
        example_piece_count = example_piece_count
            .checked_add(universe.sequence_len_at(pattern_index))
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
        remaining_examples -= 1;
    }
    if remaining_examples != 0 {
        return Err(PcFailedQueueEvidenceError::FailedExampleCountMismatch {
            expected: example_count,
            actual: example_count - remaining_examples,
        });
    }
    let exact_example_materialization_bytes =
        checked_example_bytes(example_count, example_piece_count)?;
    let exact_peak_bytes = checked_producer_peak_bytes(
        source_word_materialization_upper_bound_bytes,
        union_constructor_peak_bytes,
        exact_example_materialization_bytes,
    )?;
    admission.ensure(exact_peak_bytes)?;
    admitted_producer_peak_bytes = admitted_producer_peak_bytes.max(exact_peak_bytes);

    let producer_base_peak_bytes = source_word_materialization_upper_bound_bytes
        .checked_add(union_constructor_peak_bytes)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    let examples = materialize_examples(
        universe,
        success_coverage.words(),
        example_count,
        admission,
        producer_base_peak_bytes,
    )?;
    if examples.len() != example_count {
        return Err(PcFailedQueueEvidenceError::FailedExampleCountMismatch {
            expected: example_count,
            actual: examples.len(),
        });
    }

    let retained_union_bytes = success_coverage
        .checked_storage_retained_bytes()
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    let retained_example_bytes = checked_retained_example_bytes(&examples)?;
    let retained_producer_bytes = retained_union_bytes
        .checked_add(retained_example_bytes)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    let capacity_aware_peak_bytes = producer_base_peak_bytes
        .checked_add(retained_example_bytes)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    admission.ensure(capacity_aware_peak_bytes)?;
    admitted_producer_peak_bytes = admitted_producer_peak_bytes.max(capacity_aware_peak_bytes);

    Ok(PcFailedQueueEvidence {
        authority,
        piece_source_id,
        pattern_universe_id,
        pattern_weight_model_id,
        pattern_count,
        source_row_count: rows.len(),
        success_coverage,
        success_pattern_count,
        failed_pattern_count,
        success_probability,
        failed_probability,
        materialized_probability_mass,
        examples,
        memory_report: PcFailedQueueMemoryReport {
            admission_cap_bytes: admission.memory_bound.cap_bytes(),
            observed_execution_bytes: admission.observed_execution_bytes,
            source_word_materialization_upper_bound_bytes,
            union_constructor_peak_bytes,
            example_materialization_upper_bound_bytes: maximum_example_materialization_bytes
                .max(retained_example_bytes),
            admitted_producer_peak_bytes,
            retained_union_bytes,
            retained_example_bytes,
            retained_producer_bytes,
        },
    })
}

fn validate_completeness(
    completeness: PcFailedQueueSourceCompleteness,
) -> Result<(), PcFailedQueueEvidenceError> {
    let incomplete_stage = if !completeness.packing_complete {
        Some(PcFailedQueueIncompleteStage::Packing)
    } else if !completeness.buildup_count_complete {
        Some(PcFailedQueueIncompleteStage::BuildUpCount)
    } else if !completeness.materialized_coverage_complete {
        Some(PcFailedQueueIncompleteStage::MaterializedCoverage)
    } else if !completeness.objective_complete {
        Some(PcFailedQueueIncompleteStage::Objective)
    } else {
        None
    };
    match incomplete_stage {
        Some(stage) => Err(PcFailedQueueEvidenceError::IncompleteExecution { stage }),
        None => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_row_metadata<R: RawFailedQueueCoverageRow>(
    row_index: usize,
    row: &R,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    word_count: usize,
    previous_candidate_id: Option<u64>,
) -> Result<(), PcFailedQueueEvidenceError> {
    if row.row_kind() != &CoverageRowKind::Build {
        return Err(PcFailedQueueEvidenceError::RowKindMismatch { row_index });
    }
    if row.piece_source_id() != piece_source_id {
        return Err(PcFailedQueueEvidenceError::PieceSourceMismatch {
            row_index,
            expected: piece_source_id,
            actual: row.piece_source_id(),
        });
    }
    if row.pattern_universe_id() != pattern_universe_id {
        return Err(PcFailedQueueEvidenceError::PatternUniverseMismatch {
            row_index,
            expected: pattern_universe_id,
            actual: row.pattern_universe_id(),
        });
    }
    if row.pattern_weight_model_id() != pattern_weight_model_id {
        return Err(PcFailedQueueEvidenceError::PatternWeightModelMismatch {
            row_index,
            expected: pattern_weight_model_id,
            actual: row.pattern_weight_model_id(),
        });
    }
    if row.pattern_count() != pattern_count {
        return Err(PcFailedQueueEvidenceError::PatternCountMismatch {
            row_index,
            expected: pattern_count,
            actual: row.pattern_count(),
        });
    }
    if row.raw_word_count() != word_count {
        return Err(PcFailedQueueEvidenceError::RawWordCountMismatch {
            row_index,
            expected: word_count,
            actual: row.raw_word_count(),
        });
    }
    let candidate_id = row.candidate_id();
    if candidate_id == 0 {
        return Err(PcFailedQueueEvidenceError::ZeroCandidateId { row_index });
    }
    if let Some(previous) = previous_candidate_id {
        if candidate_id == previous {
            return Err(PcFailedQueueEvidenceError::DuplicateCandidateId {
                row_index,
                candidate_id,
            });
        }
        if candidate_id < previous {
            return Err(PcFailedQueueEvidenceError::CandidateOrderViolation {
                row_index,
                previous_candidate_id: previous,
                candidate_id,
            });
        }
    }
    Ok(())
}

fn validate_raw_words(
    row_index: usize,
    pattern_count: usize,
    expected_word_count: usize,
    words: &[u64],
) -> Result<(), PcFailedQueueEvidenceError> {
    if words.len() != expected_word_count {
        return Err(PcFailedQueueEvidenceError::RawWordCountMismatch {
            row_index,
            expected: expected_word_count,
            actual: words.len(),
        });
    }
    let remainder = pattern_count % u64::BITS as usize;
    if remainder == 0 {
        return Ok(());
    }
    let allowed_mask = (1_u64 << remainder) - 1;
    let invalid_bits = words.last().copied().unwrap_or(0) & !allowed_mask;
    if invalid_bits != 0 {
        return Err(PcFailedQueueEvidenceError::NonZeroRawPaddingBits {
            row_index,
            word_index: expected_word_count - 1,
            invalid_bits,
        });
    }
    Ok(())
}

#[cfg(test)]
fn checked_failed_count(
    pattern_count: usize,
    success_pattern_count: usize,
) -> Result<usize, PcFailedQueueEvidenceError> {
    pattern_count.checked_sub(success_pattern_count).ok_or(
        PcFailedQueueEvidenceError::FailedCountUnderflow {
            pattern_count,
            success_pattern_count,
        },
    )
}

#[cfg(test)]
fn directly_sum_probabilities(
    weights: &clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet,
    success_words: &[u64],
    pattern_count: usize,
) -> Result<(ProbabilityValue, ProbabilityValue, ProbabilityValue), PcFailedQueueEvidenceError> {
    let mut success = 0.0_f64;
    let mut failed = 0.0_f64;
    let mut total = 0.0_f64;
    let mut success_terms = 0_usize;
    let mut failed_terms = 0_usize;
    for pattern_index in 0..pattern_count {
        let weight = weights
            .weight(PatternId::new(pattern_index))
            .ok_or(PcFailedQueueEvidenceError::MissingPatternWeight { pattern_index })?
            .get();
        total += weight;
        if word_contains(success_words, pattern_index) {
            success += weight;
            success_terms = success_terms
                .checked_add(1)
                .ok_or(PcFailedQueueEvidenceError::CountOverflow)?;
        } else {
            failed += weight;
            failed_terms = failed_terms
                .checked_add(1)
                .ok_or(PcFailedQueueEvidenceError::CountOverflow)?;
        }
        if !success.is_finite() || !failed.is_finite() || !total.is_finite() {
            return Err(PcFailedQueueEvidenceError::InvalidProbabilitySum {
                class: PcFailedQueueProbabilityClass::MaterializedMass,
                value_bits: total.to_bits(),
            });
        }
    }

    let success_probability = checked_probability_from_direct_sum(
        PcFailedQueueProbabilityClass::Success,
        success,
        success_terms,
    )?;
    let failed_probability = checked_probability_from_direct_sum(
        PcFailedQueueProbabilityClass::Failed,
        failed,
        failed_terms,
    )?;
    let materialized_probability_mass = checked_probability_from_direct_sum(
        PcFailedQueueProbabilityClass::MaterializedMass,
        total,
        pattern_count,
    )?;
    let partitioned = success_probability.get() + failed_probability.get();
    let tolerance = summation_tolerance(pattern_count);
    if !partitioned.is_finite()
        || (partitioned - materialized_probability_mass.get()).abs() > tolerance
    {
        return Err(PcFailedQueueEvidenceError::ProbabilityPartitionMismatch {
            partitioned_bits: partitioned.to_bits(),
            materialized_mass_bits: materialized_probability_mass.get().to_bits(),
        });
    }
    Ok((
        success_probability,
        failed_probability,
        materialized_probability_mass,
    ))
}

#[cfg(test)]
fn checked_probability_from_direct_sum(
    class: PcFailedQueueProbabilityClass,
    value: f64,
    term_count: usize,
) -> Result<ProbabilityValue, PcFailedQueueEvidenceError> {
    if !value.is_finite() {
        return Err(PcFailedQueueEvidenceError::InvalidProbabilitySum {
            class,
            value_bits: value.to_bits(),
        });
    }
    let normalized = match class {
        PcFailedQueueProbabilityClass::MaterializedMass
            if (value - 1.0).abs() <= summation_tolerance(term_count) =>
        {
            1.0
        }
        PcFailedQueueProbabilityClass::Success
        | PcFailedQueueProbabilityClass::Failed
        | PcFailedQueueProbabilityClass::MaterializedMass => value,
    };
    ProbabilityValue::new(normalized).map_err(|_| {
        PcFailedQueueEvidenceError::InvalidProbabilitySum {
            class,
            value_bits: value.to_bits(),
        }
    })
}

fn validate_complete_materialized_probability_mass(
    probability: ProbabilityValue,
) -> Result<(), PcFailedQueueEvidenceError> {
    if probability.get().to_bits() == 1.0_f64.to_bits() {
        return Ok(());
    }
    Err(
        PcFailedQueueEvidenceError::CompleteProbabilityMassMismatch {
            actual_bits: probability.get().to_bits(),
        },
    )
}

#[cfg(test)]
fn summation_tolerance(term_count: usize) -> f64 {
    (f64::EPSILON * term_count.max(1) as f64 * 2.0).min(1.0e-9)
}

fn materialize_examples(
    universe: &clearra_supply::MaterializedPatternUniverse,
    success_words: &[u64],
    example_count: usize,
    admission: PcFailedQueueProducerAdmission,
    producer_base_peak_bytes: u128,
) -> Result<Vec<PcFailedQueueExampleEvidence>, PcFailedQueueEvidenceError> {
    let mut examples = Vec::new();
    examples
        .try_reserve_exact(example_count)
        .map_err(|_| PcFailedQueueEvidenceError::AllocationUnavailable)?;
    let mut retained_piece_capacity_bytes = 0_u128;
    let actual_example_bytes =
        checked_example_capacity_bytes(examples.capacity(), retained_piece_capacity_bytes)?;
    admission.ensure(
        producer_base_peak_bytes
            .checked_add(actual_example_bytes)
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?,
    )?;
    for pattern_index in 0..universe.pattern_count() {
        if examples.len() == example_count {
            break;
        }
        if word_contains(success_words, pattern_index) {
            continue;
        }
        let expected_piece_count = universe.sequence_len_at(pattern_index);
        let mut pieces = Vec::new();
        pieces
            .try_reserve_exact(expected_piece_count)
            .map_err(|_| PcFailedQueueEvidenceError::AllocationUnavailable)?;
        let reserved_piece_bytes = checked_bytes::<PieceKind>(pieces.capacity())?;
        let prospective_piece_capacity_bytes = retained_piece_capacity_bytes
            .checked_add(reserved_piece_bytes)
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
        let prospective_example_bytes =
            checked_example_capacity_bytes(examples.capacity(), prospective_piece_capacity_bytes)?;
        admission.ensure(
            producer_base_peak_bytes
                .checked_add(prospective_example_bytes)
                .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?,
        )?;
        if !universe.try_write_sequence_at(pattern_index, &mut pieces) {
            return Err(PcFailedQueueEvidenceError::PatternSequenceUnavailable { pattern_index });
        }
        if pieces.len() != expected_piece_count {
            return Err(PcFailedQueueEvidenceError::PatternSequenceLengthMismatch {
                pattern_index,
                expected: expected_piece_count,
                actual: pieces.len(),
            });
        }
        // The universe writer is not allowed to grow the admitted buffer, but
        // bind the proof to the capacity it actually retained before the value
        // can enter the evidence object.
        let written_piece_bytes = checked_bytes::<PieceKind>(pieces.capacity())?;
        let written_piece_capacity_bytes = retained_piece_capacity_bytes
            .checked_add(written_piece_bytes)
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
        let written_example_bytes =
            checked_example_capacity_bytes(examples.capacity(), written_piece_capacity_bytes)?;
        admission.ensure(
            producer_base_peak_bytes
                .checked_add(written_example_bytes)
                .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?,
        )?;
        examples.push(PcFailedQueueExampleEvidence {
            pattern_index,
            pieces,
        });
        retained_piece_capacity_bytes = written_piece_capacity_bytes;
    }
    Ok(examples)
}

fn word_contains(words: &[u64], pattern_index: usize) -> bool {
    words
        .get(pattern_index / u64::BITS as usize)
        .is_some_and(|word| word & (1_u64 << (pattern_index % u64::BITS as usize)) != 0)
}

fn checked_bytes<T>(count: usize) -> Result<u128, PcFailedQueueEvidenceError> {
    (count as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)
}

fn checked_example_bytes(
    example_count: usize,
    piece_count: usize,
) -> Result<u128, PcFailedQueueEvidenceError> {
    checked_bytes::<PcFailedQueueExampleEvidence>(example_count)?
        .checked_add(checked_bytes::<PieceKind>(piece_count)?)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)
}

fn checked_union_constructor_peak_bytes(
    vector_capacity: usize,
    final_word_count: usize,
) -> Result<u128, PcFailedQueueEvidenceError> {
    checked_bytes::<u64>(vector_capacity)?
        .checked_add(checked_bytes::<u64>(final_word_count)?)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)
}

fn checked_example_capacity_bytes(
    example_capacity: usize,
    piece_capacity_bytes: u128,
) -> Result<u128, PcFailedQueueEvidenceError> {
    checked_bytes::<PcFailedQueueExampleEvidence>(example_capacity)?
        .checked_add(piece_capacity_bytes)
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)
}

fn checked_producer_peak_bytes(
    source_word_materialization_upper_bound_bytes: u128,
    union_constructor_peak_bytes: u128,
    example_materialization_bytes: u128,
) -> Result<u128, PcFailedQueueEvidenceError> {
    source_word_materialization_upper_bound_bytes
        .checked_add(union_constructor_peak_bytes)
        .and_then(|bytes| bytes.checked_add(example_materialization_bytes))
        .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)
}

fn checked_retained_example_bytes(
    examples: &Vec<PcFailedQueueExampleEvidence>,
) -> Result<u128, PcFailedQueueEvidenceError> {
    let mut piece_capacity_bytes = 0_u128;
    for example in examples {
        piece_capacity_bytes = piece_capacity_bytes
            .checked_add(checked_bytes::<PieceKind>(example.pieces.capacity())?)
            .ok_or(PcFailedQueueEvidenceError::MemoryProjectionOverflow)?;
    }
    checked_example_capacity_bytes(examples.capacity(), piece_capacity_bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcFailedQueueIncompleteStage {
    Packing,
    BuildUpCount,
    MaterializedCoverage,
    Objective,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcFailedQueueProbabilityClass {
    Success,
    Failed,
    MaterializedMass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcFailedQueueEvidenceError {
    UnsupportedPreset,
    MissingMaterializedPatternUniverse,
    EmptyPatternUniverse,
    IncompleteSourceUniverse,
    IncompleteExecution {
        stage: PcFailedQueueIncompleteStage,
    },
    ZeroPieceSourceId,
    ZeroPatternUniverseId,
    ZeroPatternWeightModelId,
    CompleteUniverseCardinalityMismatch {
        materialized: usize,
        total_possible: u128,
    },
    PatternWeightCountMismatch {
        expected: usize,
        actual: usize,
    },
    RowKindMismatch {
        row_index: usize,
    },
    PieceSourceMismatch {
        row_index: usize,
        expected: u64,
        actual: u64,
    },
    PatternUniverseMismatch {
        row_index: usize,
        expected: PatternUniverseId,
        actual: PatternUniverseId,
    },
    PatternWeightModelMismatch {
        row_index: usize,
        expected: PatternWeightModelId,
        actual: PatternWeightModelId,
    },
    PatternCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    RawWordCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    NonZeroRawPaddingBits {
        row_index: usize,
        word_index: usize,
        invalid_bits: u64,
    },
    ZeroCandidateId {
        row_index: usize,
    },
    DuplicateCandidateId {
        row_index: usize,
        candidate_id: u64,
    },
    CandidateOrderViolation {
        row_index: usize,
        previous_candidate_id: u64,
        candidate_id: u64,
    },
    CountOverflow,
    FailedCountUnderflow {
        pattern_count: usize,
        success_pattern_count: usize,
    },
    MissingPatternWeight {
        pattern_index: usize,
    },
    InvalidProbabilitySum {
        class: PcFailedQueueProbabilityClass,
        value_bits: u64,
    },
    ProbabilityPartitionMismatch {
        partitioned_bits: u64,
        materialized_mass_bits: u64,
    },
    CompleteProbabilityMassMismatch {
        actual_bits: u64,
    },
    PatternSequenceUnavailable {
        pattern_index: usize,
    },
    PatternSequenceLengthMismatch {
        pattern_index: usize,
        expected: usize,
        actual: usize,
    },
    FailedExampleCountMismatch {
        expected: usize,
        actual: usize,
    },
    MemoryAuthorityUnavailable,
    MemoryProjectionOverflow,
    MemoryAdmission(ResourceReport),
    AllocationUnavailable,
    InvalidCoverageBitSet(clearra_coverage::pattern::pattern_bitset::PatternBitSetError),
    SharedCoverageAggregationInvalid,
}

#[cfg(test)]
#[path = "pc_failed_queue_evidence_tests.rs"]
mod tests;
