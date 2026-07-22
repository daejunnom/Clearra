use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use crate::{
    normalize::observed_queue_expansion::{ObservedQueueExpansion, ObservedQueueExpansionError},
    piece_source::SupplyTruncationReason,
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        observed_queue::ObservedQueue, queue_pattern_expression::QueuePatternExpression,
    },
};

use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseError,
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

    pub fn observed(
        queue: &ObservedQueue,
        minimum_len: usize,
        max_patterns: usize,
        provenance_id: u64,
    ) -> Result<MaterializedPatternUniverse, PatternUniverseMaterializationError> {
        let materialization_limit = if max_patterns == 0 {
            usize::MAX
        } else {
            max_patterns
        };
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
        let identities = pattern_identities(
            "observed",
            provenance_id,
            sequences.iter().map(Vec::as_slice),
            &weights,
            expansion.total_pattern_count(),
        );
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
