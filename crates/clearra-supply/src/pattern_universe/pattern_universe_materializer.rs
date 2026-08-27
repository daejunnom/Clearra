use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use crate::{
    finite_allocation::{FiniteSupplyAllocationError, FiniteSupplyAllocationTransaction},
    normalize::observed_queue_expansion::{ObservedQueueExpansion, ObservedQueueExpansionError},
    piece_source::SupplyTruncationReason,
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        observed_queue::ObservedQueue, queue_pattern_expression::QueuePatternExpression,
    },
};

use super::flat_pattern_sequences::FlatPatternSequences;
use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseError,
};
use super::observed_standard_7_bag_sequence_space::{
    ObservedStandard7BagSequenceSpace, ObservedStandard7BagSequenceSpaceError,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PatternUniverseMaterializer;

impl PatternUniverseMaterializer {
    pub fn fixed_sequence(
        sequence: &FixedSequence,
        provenance_id: u64,
    ) -> MaterializedPatternUniverse {
        exact_single_pattern("fixed", sequence.pieces(), provenance_id)
    }

    pub fn bag_aligned_pattern(
        pattern: &BagAlignedPattern,
        provenance_id: u64,
    ) -> MaterializedPatternUniverse {
        exact_single_pattern("bag-aligned", pattern.pieces(), provenance_id)
    }

    pub fn queue_pattern_expression(
        expression: &QueuePatternExpression,
        provenance_id: u64,
    ) -> Result<MaterializedPatternUniverse, PatternUniverseMaterializationError> {
        let pattern_count = expression.pattern_count();
        if pattern_count == 0 {
            return Err(PatternUniverseMaterializationError::NoPatterns);
        }
        let probability = ProbabilityValue::new(1.0 / pattern_count as f64)
            .map_err(PatternUniverseMaterializationError::Probability)?;
        if expression.is_factorized() {
            let identities = factorized_expression_identities(
                provenance_id,
                expression.source(),
                expression.sequence_len(),
                pattern_count,
                probability,
            );
            return MaterializedPatternUniverse::from_factorized_queue_expression(
                identities.0,
                identities.1,
                expression.clone(),
                probability,
                pattern_count as u128,
            )
            .map_err(PatternUniverseMaterializationError::Universe);
        }
        let sequences = expression
            .explicit_sequences()
            .expect("non-factorized expression stores explicit sequences")
            .to_vec();
        let weights = vec![probability; pattern_count];
        let identities = pattern_identities(
            "queue-pattern-expression",
            provenance_id,
            sequences.iter().map(Vec::as_slice),
            &weights,
            pattern_count as u128,
        );
        MaterializedPatternUniverse::from_sequences(
            identities.0,
            identities.1,
            sequences,
            weights,
            pattern_count as u128,
            true,
            None,
        )
        .map_err(PatternUniverseMaterializationError::Universe)
    }

    pub fn standard_7_bag(
        minimum_len: usize,
        max_patterns: usize,
        provenance_id: u64,
    ) -> Result<MaterializedPatternUniverse, PatternUniverseMaterializationError> {
        if minimum_len == 0 {
            return Err(PatternUniverseMaterializationError::NoPatterns);
        }
        let total_pattern_count = standard_7_bag_pattern_count(minimum_len);
        let representable_pattern_count = usize::try_from(total_pattern_count)
            .map_err(|_| PatternUniverseMaterializationError::PatternCountOverflow)?;
        let materialization_limit = if max_patterns == 0 {
            representable_pattern_count
        } else {
            max_patterns.min(representable_pattern_count)
        };
        let probability = ProbabilityValue::new(1.0 / total_pattern_count as f64)
            .map_err(PatternUniverseMaterializationError::Probability)?;
        let complete = materialization_limit as u128 == total_pattern_count;
        let identities = standard_7_bag_identities(
            provenance_id,
            minimum_len,
            materialization_limit,
            total_pattern_count,
            probability,
        );
        MaterializedPatternUniverse::from_standard_7_bag_lexicographic(
            identities.0,
            identities.1,
            u16::try_from(minimum_len)
                .map_err(|_| PatternUniverseMaterializationError::SequenceStorageOverflow)?,
            materialization_limit,
            probability,
            total_pattern_count,
            complete,
            (!complete).then_some(SupplyTruncationReason::MaterializedPatternBudgetExceeded),
        )
        .map_err(PatternUniverseMaterializationError::Universe)
    }

    pub(crate) fn validate_queue_pattern_expression_finite(
        expression: &QueuePatternExpression,
        visible_sequence_len: usize,
    ) -> Result<(), PatternUniverseMaterializationError> {
        if visible_sequence_len > expression.sequence_len() {
            return Err(PatternUniverseMaterializationError::SequenceStorageOverflow);
        }
        let pattern_count = expression.pattern_count();
        if pattern_count == 0 {
            return Err(PatternUniverseMaterializationError::NoPatterns);
        }
        ProbabilityValue::new(1.0 / pattern_count as f64)
            .map_err(PatternUniverseMaterializationError::Probability)?;
        if expression.is_factorized() {
            u16::try_from(visible_sequence_len)
                .map_err(|_| PatternUniverseMaterializationError::SequenceStorageOverflow)?;
        } else {
            let sequences = expression
                .explicit_sequences()
                .ok_or(PatternUniverseMaterializationError::SequenceStorageOverflow)?;
            sequences
                .len()
                .checked_add(1)
                .ok_or(PatternUniverseMaterializationError::SequenceStorageOverflow)?;
            sequences
                .iter()
                .try_fold(0usize, |total, sequence| {
                    total.checked_add(visible_sequence_len.min(sequence.len()))
                })
                .ok_or(PatternUniverseMaterializationError::SequenceStorageOverflow)?;
        }
        Ok(())
    }

    pub(crate) fn fixed_sequence_finite(
        sequence: &[clearra_core_domain::piece::piece_kind::PieceKind],
        provenance_id: u64,
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<MaterializedPatternUniverse, FinitePatternUniverseMaterializationError> {
        let identities = uniform_pattern_identities(
            "fixed",
            provenance_id,
            core::iter::once(sequence),
            ProbabilityValue::ONE,
            1,
            1,
        );
        let sequences = FlatPatternSequences::from_single_slice_finite(sequence, transaction)
            .map_err(FinitePatternUniverseMaterializationError::Allocation)?;
        MaterializedPatternUniverse::from_flat_sequences_uniform(
            identities.0,
            identities.1,
            sequences,
            ProbabilityValue::ONE,
            1,
            true,
            None,
        )
        .map_err(|error| {
            FinitePatternUniverseMaterializationError::Materialization(
                PatternUniverseMaterializationError::Universe(error),
            )
        })
    }

    pub(crate) fn queue_pattern_expression_finite(
        expression: &QueuePatternExpression,
        visible_sequence_len: usize,
        provenance_id: u64,
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<MaterializedPatternUniverse, FinitePatternUniverseMaterializationError> {
        Self::validate_queue_pattern_expression_finite(expression, visible_sequence_len)
            .map_err(FinitePatternUniverseMaterializationError::Materialization)?;
        let pattern_count = expression.pattern_count();
        let probability = ProbabilityValue::new(1.0 / pattern_count as f64)
            .map_err(PatternUniverseMaterializationError::Probability)
            .map_err(FinitePatternUniverseMaterializationError::Materialization)?;

        if expression.is_factorized() {
            let identities = factorized_expression_identities(
                provenance_id,
                expression.source(),
                visible_sequence_len,
                pattern_count,
                probability,
            );
            let expression = expression
                .duplicate_finite(visible_sequence_len, transaction)
                .map_err(FinitePatternUniverseMaterializationError::Allocation)?;
            return MaterializedPatternUniverse::from_factorized_queue_expression(
                identities.0,
                identities.1,
                expression,
                probability,
                pattern_count as u128,
            )
            .map_err(|error| {
                FinitePatternUniverseMaterializationError::Materialization(
                    PatternUniverseMaterializationError::Universe(error),
                )
            });
        }

        let explicit_sequences = expression
            .explicit_sequences()
            .expect("validated explicit expression storage");
        let identities = uniform_pattern_identities(
            "queue-pattern-expression",
            provenance_id,
            explicit_sequences
                .iter()
                .map(|sequence| &sequence[..visible_sequence_len.min(sequence.len())]),
            probability,
            pattern_count,
            pattern_count as u128,
        );
        let sequences = FlatPatternSequences::from_nested_prefix_finite(
            explicit_sequences,
            visible_sequence_len,
            transaction,
        )
        .map_err(FinitePatternUniverseMaterializationError::Allocation)?;
        MaterializedPatternUniverse::from_flat_sequences_uniform(
            identities.0,
            identities.1,
            sequences,
            probability,
            pattern_count as u128,
            true,
            None,
        )
        .map_err(|error| {
            FinitePatternUniverseMaterializationError::Materialization(
                PatternUniverseMaterializationError::Universe(error),
            )
        })
    }

    pub fn observed(
        queue: &ObservedQueue,
        minimum_len: usize,
        max_patterns: usize,
        provenance_id: u64,
    ) -> Result<MaterializedPatternUniverse, PatternUniverseMaterializationError> {
        if max_patterns == 0 {
            return lazy_observed_standard_7_bag(queue, minimum_len, provenance_id);
        }
        let materialization_limit = max_patterns;
        let expansion = ObservedQueueExpansion::expand(queue, minimum_len, materialization_limit)
            .map_err(PatternUniverseMaterializationError::Observed)?;
        let sequences = expansion
            .patterns()
            .iter()
            .map(|pattern| pattern.queue_pattern().pieces().to_vec())
            .collect::<Vec<_>>();
        let weights = expansion
            .patterns()
            .iter()
            .map(|pattern| pattern.probability().value())
            .collect::<Vec<_>>();
        let identities = if expansion.probability_complete() {
            let sequence_len = u16::try_from(minimum_len.max(queue.len()))
                .map_err(|_| PatternUniverseMaterializationError::SequenceStorageOverflow)?;
            let probability = ProbabilityValue::new(1.0 / expansion.total_pattern_count() as f64)
                .map_err(PatternUniverseMaterializationError::Probability)?;
            lazy_observed_standard_7_bag_identities(
                provenance_id,
                queue.pieces(),
                sequence_len,
                sequences.len(),
                expansion.total_pattern_count(),
                probability,
            )
        } else {
            pattern_identities(
                "observed",
                provenance_id,
                sequences.iter().map(Vec::as_slice),
                &weights,
                expansion.total_pattern_count(),
            )
        };
        MaterializedPatternUniverse::from_sequences(
            identities.0,
            identities.1,
            sequences,
            weights,
            expansion.total_pattern_count(),
            expansion.probability_complete(),
            expansion
                .is_truncated()
                .then_some(SupplyTruncationReason::ObservedWindowBudgetExceeded),
        )
        .map_err(PatternUniverseMaterializationError::Universe)
    }
}

fn uniform_pattern_identities<'a>(
    label: &str,
    provenance_id: u64,
    sequences: impl IntoIterator<Item = &'a [clearra_core_domain::piece::piece_kind::PieceKind]>,
    weight: ProbabilityValue,
    pattern_count: usize,
    total_possible_pattern_count: u128,
) -> (PatternUniverseId, PatternWeightModelId) {
    let mut universe_hash = stable_hash(&["clearra-pattern-universe-v1", label]);
    universe_hash = mix_decimal_terminated(universe_hash, provenance_id as u128);
    universe_hash = mix_decimal_terminated(universe_hash, total_possible_pattern_count);
    for sequence in sequences {
        for piece in sequence {
            universe_hash = mix(universe_hash, piece.as_ascii() as u8);
        }
        universe_hash = mix(universe_hash, 0xff);
    }

    let mut weight_hash = stable_hash(&["clearra-pattern-weight-model-v1", label]);
    weight_hash = mix_decimal_terminated(weight_hash, provenance_id as u128);
    for _ in 0..pattern_count {
        for byte in weight.get().to_bits().to_le_bytes() {
            weight_hash = mix(weight_hash, byte);
        }
    }
    (
        PatternUniverseId::new(universe_hash.max(1)),
        PatternWeightModelId::new(weight_hash.max(1)),
    )
}

fn mix_decimal_terminated(mut hash: u64, mut value: u128) -> u64 {
    let mut digits = [0_u8; 39];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in &digits[start..] {
        hash = mix(hash, *digit);
    }
    mix(hash, 0)
}

fn lazy_observed_standard_7_bag(
    queue: &ObservedQueue,
    minimum_len: usize,
    provenance_id: u64,
) -> Result<MaterializedPatternUniverse, PatternUniverseMaterializationError> {
    let sequence_len = minimum_len.max(queue.len());
    let sequence_len = u16::try_from(sequence_len)
        .map_err(|_| PatternUniverseMaterializationError::SequenceStorageOverflow)?;
    let sequences = ObservedStandard7BagSequenceSpace::new(queue.pieces(), sequence_len)
        .map_err(map_lazy_observed_error)?;
    let total_pattern_count = sequences.total_pattern_count();
    let probability = ProbabilityValue::new(1.0 / total_pattern_count as f64)
        .map_err(PatternUniverseMaterializationError::Probability)?;
    let identities = lazy_observed_standard_7_bag_identities(
        provenance_id,
        queue.pieces(),
        sequence_len,
        sequences.len(),
        total_pattern_count,
        probability,
    );
    MaterializedPatternUniverse::from_observed_standard_7_bag_lexicographic(
        identities.0,
        identities.1,
        sequences,
        probability,
        total_pattern_count,
    )
    .map_err(PatternUniverseMaterializationError::Universe)
}

fn map_lazy_observed_error(
    error: ObservedStandard7BagSequenceSpaceError,
) -> PatternUniverseMaterializationError {
    match error {
        ObservedStandard7BagSequenceSpaceError::IncompatibleBoundary => {
            PatternUniverseMaterializationError::Observed(
                ObservedQueueExpansionError::IncompatibleBoundary,
            )
        }
        ObservedStandard7BagSequenceSpaceError::NoPatterns => {
            PatternUniverseMaterializationError::Observed(ObservedQueueExpansionError::NoPatterns)
        }
        ObservedStandard7BagSequenceSpaceError::PatternCountOverflow => {
            PatternUniverseMaterializationError::PatternCountOverflow
        }
        ObservedStandard7BagSequenceSpaceError::SequenceTooShort
        | ObservedStandard7BagSequenceSpaceError::InvalidBoundaryOffset
        | ObservedStandard7BagSequenceSpaceError::PatternIndexOutOfRange
        | ObservedStandard7BagSequenceSpaceError::RankInvariantViolated => {
            PatternUniverseMaterializationError::SequenceStorageOverflow
        }
    }
}

fn factorized_expression_identities(
    provenance_id: u64,
    source: &str,
    sequence_len: usize,
    pattern_count: usize,
    probability: ProbabilityValue,
) -> (PatternUniverseId, PatternWeightModelId) {
    let mut universe_hash = stable_hash(&[
        "clearra-pattern-universe-v2",
        "factorized-queue-pattern-expression",
        source,
    ]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain((sequence_len as u64).to_le_bytes())
        .chain((pattern_count as u64).to_le_bytes())
    {
        universe_hash = mix(universe_hash, byte);
    }

    let mut weight_hash = stable_hash(&[
        "clearra-pattern-weight-model-v2",
        "uniform-factorized-queue-pattern-expression",
    ]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain((pattern_count as u64).to_le_bytes())
        .chain(probability.get().to_bits().to_le_bytes())
    {
        weight_hash = mix(weight_hash, byte);
    }
    (
        PatternUniverseId::new(universe_hash.max(1)),
        PatternWeightModelId::new(weight_hash.max(1)),
    )
}

fn lazy_observed_standard_7_bag_identities(
    provenance_id: u64,
    observed: &[clearra_core_domain::piece::piece_kind::PieceKind],
    sequence_len: u16,
    pattern_count: usize,
    total_possible_pattern_count: u128,
    probability: ProbabilityValue,
) -> (PatternUniverseId, PatternWeightModelId) {
    let mut universe_hash = stable_hash(&[
        "clearra-pattern-universe-v2",
        "lazy-observed-standard-7-bag-lexicographic",
    ]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain(sequence_len.to_le_bytes())
        .chain((observed.len() as u64).to_le_bytes())
        .chain((pattern_count as u64).to_le_bytes())
        .chain(total_possible_pattern_count.to_le_bytes())
    {
        universe_hash = mix(universe_hash, byte);
    }
    for piece in observed.iter().copied() {
        universe_hash = mix(universe_hash, piece.as_ascii() as u8);
    }

    let mut weight_hash = stable_hash(&[
        "clearra-pattern-weight-model-v2",
        "lazy-observed-standard-7-bag-terminal-remainder",
    ]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain((pattern_count as u64).to_le_bytes())
        .chain(total_possible_pattern_count.to_le_bytes())
        .chain(probability.get().to_bits().to_le_bytes())
    {
        weight_hash = mix(weight_hash, byte);
    }
    (
        PatternUniverseId::new(universe_hash.max(1)),
        PatternWeightModelId::new(weight_hash.max(1)),
    )
}

fn standard_7_bag_identities(
    provenance_id: u64,
    sequence_len: usize,
    materialized_count: usize,
    total_possible_pattern_count: u128,
    probability: ProbabilityValue,
) -> (PatternUniverseId, PatternWeightModelId) {
    let mut universe_hash = stable_hash(&[
        "clearra-pattern-universe-v2",
        "standard-7-bag-lexicographic",
    ]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain((sequence_len as u64).to_le_bytes())
        .chain((materialized_count as u64).to_le_bytes())
        .chain(total_possible_pattern_count.to_le_bytes())
    {
        universe_hash = mix(universe_hash, byte);
    }

    let mut weight_hash =
        stable_hash(&["clearra-pattern-weight-model-v2", "uniform-standard-7-bag"]);
    for byte in provenance_id
        .to_le_bytes()
        .into_iter()
        .chain(total_possible_pattern_count.to_le_bytes())
        .chain(probability.get().to_bits().to_le_bytes())
    {
        weight_hash = mix(weight_hash, byte);
    }
    (
        PatternUniverseId::new(universe_hash.max(1)),
        PatternWeightModelId::new(weight_hash.max(1)),
    )
}

fn standard_7_bag_pattern_count(sequence_len: usize) -> u128 {
    let full_bags = sequence_len / 7;
    let remainder = sequence_len % 7;
    let full_bag_permutations = factorial(7);
    full_bag_permutations
        .saturating_pow(full_bags as u32)
        .saturating_mul(falling_factorial(7, remainder))
}

const fn factorial(value: usize) -> u128 {
    falling_factorial(value, value)
}

const fn falling_factorial(value: usize, count: usize) -> u128 {
    let mut product = 1u128;
    let mut index = 0usize;
    while index < count {
        product *= (value - index) as u128;
        index += 1;
    }
    product
}

fn exact_single_pattern(
    label: &str,
    pieces: &[clearra_core_domain::piece::piece_kind::PieceKind],
    provenance_id: u64,
) -> MaterializedPatternUniverse {
    let sequences = vec![pieces.to_vec()];
    let weights = vec![ProbabilityValue::ONE];
    let identities = pattern_identities(
        label,
        provenance_id,
        sequences.iter().map(Vec::as_slice),
        &weights,
        1,
    );
    MaterializedPatternUniverse::from_sequences(
        identities.0,
        identities.1,
        sequences,
        weights,
        1,
        true,
        None,
    )
    .expect("one exact sequence with unit weight is a valid universe")
}

fn pattern_identities<'a>(
    label: &str,
    provenance_id: u64,
    sequences: impl IntoIterator<Item = &'a [clearra_core_domain::piece::piece_kind::PieceKind]>,
    weights: &[ProbabilityValue],
    total_possible_pattern_count: u128,
) -> (PatternUniverseId, PatternWeightModelId) {
    let mut universe_hash = stable_hash(&[
        "clearra-pattern-universe-v1",
        label,
        &provenance_id.to_string(),
        &total_possible_pattern_count.to_string(),
    ]);
    for sequence in sequences {
        for piece in sequence {
            universe_hash = mix(universe_hash, piece.as_ascii() as u8);
        }
        universe_hash = mix(universe_hash, 0xff);
    }

    let mut weight_hash = stable_hash(&[
        "clearra-pattern-weight-model-v1",
        label,
        &provenance_id.to_string(),
    ]);
    for weight in weights {
        for byte in weight.get().to_bits().to_le_bytes() {
            weight_hash = mix(weight_hash, byte);
        }
    }
    (
        PatternUniverseId::new(universe_hash.max(1)),
        PatternWeightModelId::new(weight_hash.max(1)),
    )
}

fn stable_hash(values: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for value in values {
        for byte in value.as_bytes() {
            hash = mix(hash, *byte);
        }
        hash = mix(hash, 0);
    }
    hash
}

const fn mix(hash: u64, byte: u8) -> u64 {
    (hash ^ byte as u64).wrapping_mul(0x100000001b3)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternUniverseMaterializationError {
    NoPatterns,
    PatternCountOverflow,
    SequenceStorageOverflow,
    Probability(clearra_core_domain::probability::probability_value::ProbabilityValueError),
    Observed(ObservedQueueExpansionError),
    Universe(MaterializedPatternUniverseError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FinitePatternUniverseMaterializationError {
    Allocation(FiniteSupplyAllocationError),
    Materialization(PatternUniverseMaterializationError),
}
