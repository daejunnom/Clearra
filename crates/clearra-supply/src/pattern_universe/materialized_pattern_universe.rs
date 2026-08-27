use std::borrow::Cow;

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::{
        pattern_id::PatternId,
        weighted_pattern_set::{WeightedPatternSet, WeightedPatternSetError},
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::{
    piece_source::SupplyTruncationReason, queue::queue_pattern_expression::QueuePatternExpression,
};

use super::{
    flat_pattern_sequences::FlatPatternSequences,
    observed_standard_7_bag_sequence_space::ObservedStandard7BagSequenceSpace,
    pattern_sequence_reader::{PatternSequenceReader, ProbabilityWeight},
    standard_7_bag_sequence_space::Standard7BagSequenceSpace,
};

#[derive(Clone, Debug, PartialEq)]
pub struct MaterializedPatternUniverse {
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    sequences: PatternSequenceStorage,
    weights: WeightedPatternSet,
    total_possible_pattern_count: u128,
    complete: bool,
    truncation_reason: Option<SupplyTruncationReason>,
    structure: MaterializedPatternUniverseStructure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PatternSequenceStorage {
    Explicit(FlatPatternSequences),
    Standard7BagLexicographic(Standard7BagSequenceSpace),
    ObservedStandard7BagLexicographic(ObservedStandard7BagSequenceSpace),
    FactorizedQueueExpression(QueuePatternExpression),
}

impl PatternSequenceStorage {
    fn len(&self) -> usize {
        match self {
            Self::Explicit(sequences) => sequences.len(),
            Self::Standard7BagLexicographic(sequences) => sequences.len(),
            Self::ObservedStandard7BagLexicographic(sequences) => sequences.len(),
            Self::FactorizedQueueExpression(expression) => expression.pattern_count(),
        }
    }

    fn get(&self, index: usize) -> Cow<'_, [PieceKind]> {
        match self {
            Self::Explicit(sequences) => Cow::Borrowed(sequences.get(index)),
            Self::Standard7BagLexicographic(sequences) => Cow::Owned(sequences.sequence(index)),
            Self::ObservedStandard7BagLexicographic(sequences) => Cow::Owned(
                sequences
                    .sequence(index)
                    .expect("pattern index belongs to observed 7-bag universe"),
            ),
            Self::FactorizedQueueExpression(expression) => expression.sequence_at(index),
        }
    }

    fn sequence_len_at(&self, index: usize) -> usize {
        match self {
            Self::Explicit(sequences) => sequences.get(index).len(),
            Self::Standard7BagLexicographic(sequences) => sequences.sequence_len(),
            Self::ObservedStandard7BagLexicographic(sequences) => sequences.sequence_len(),
            Self::FactorizedQueueExpression(expression) => expression.sequence_len(),
        }
    }

    fn write_sequence_at(&self, index: usize, output: &mut Vec<PieceKind>) {
        match self {
            Self::Explicit(sequences) => {
                output.clear();
                output.extend_from_slice(sequences.get(index));
            }
            Self::Standard7BagLexicographic(sequences) => sequences.write_sequence(index, output),
            Self::ObservedStandard7BagLexicographic(sequences) => sequences
                .write_sequence(index, output)
                .expect("pattern index belongs to observed 7-bag universe"),
            Self::FactorizedQueueExpression(expression) => {
                expression.write_sequence_at(index, output)
            }
        }
    }

    fn try_get(&self, index: usize) -> Option<Cow<'_, [PieceKind]>> {
        if index >= self.len() {
            return None;
        }
        match self {
            Self::Explicit(sequences) => Some(Cow::Borrowed(sequences.get(index))),
            Self::Standard7BagLexicographic(sequences) => {
                Some(Cow::Owned(sequences.sequence(index)))
            }
            Self::ObservedStandard7BagLexicographic(sequences) => {
                sequences.sequence(index).ok().map(Cow::Owned)
            }
            Self::FactorizedQueueExpression(expression) => Some(expression.sequence_at(index)),
        }
    }

    fn try_write_sequence_at(&self, index: usize, output: &mut Vec<PieceKind>) -> bool {
        if index >= self.len() {
            output.clear();
            return false;
        }
        match self {
            Self::ObservedStandard7BagLexicographic(sequences) => {
                sequences.write_sequence(index, output).is_ok()
            }
            _ => {
                self.write_sequence_at(index, output);
                true
            }
        }
    }

    /// Returns only heap payload retained by the selected sequence
    /// representation. The inline enum and variant owners are excluded.
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Explicit(sequences) => sequences.checked_retained_capacity_bytes(),
            Self::Standard7BagLexicographic(_) => Some(0),
            Self::ObservedStandard7BagLexicographic(sequences) => {
                sequences.checked_retained_capacity_bytes()
            }
            Self::FactorizedQueueExpression(expression) => {
                expression.checked_retained_capacity_bytes()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterializedPatternUniverseStructure {
    Explicit,
    Standard7BagLexicographic {
        sequence_len: u16,
    },
    ObservedStandard7BagLexicographic {
        sequence_len: u16,
        observed_len: u16,
        boundary_candidate_count: u8,
    },
    FactorizedQueueExpression {
        sequence_len: u16,
    },
}

impl MaterializedPatternUniverse {
    pub fn from_sequences(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequences: Vec<Vec<PieceKind>>,
        weights: Vec<ProbabilityValue>,
        total_possible_pattern_count: u128,
        complete: bool,
        truncation_reason: Option<SupplyTruncationReason>,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        let sequences = FlatPatternSequences::from_nested(sequences)
            .ok_or(MaterializedPatternUniverseError::SequenceStorageOverflow)?;
        Self::from_flat_sequences(
            pattern_universe_id,
            pattern_weight_model_id,
            sequences,
            weights,
            total_possible_pattern_count,
            complete,
            truncation_reason,
        )
    }

    pub(super) fn from_flat_sequences(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequences: FlatPatternSequences,
        weights: Vec<ProbabilityValue>,
        total_possible_pattern_count: u128,
        complete: bool,
        truncation_reason: Option<SupplyTruncationReason>,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        Self::from_flat_sequences_with_structure(
            pattern_universe_id,
            pattern_weight_model_id,
            sequences,
            weights,
            total_possible_pattern_count,
            complete,
            truncation_reason,
            MaterializedPatternUniverseStructure::Explicit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_flat_sequences_uniform(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequences: FlatPatternSequences,
        weight: ProbabilityValue,
        total_possible_pattern_count: u128,
        complete: bool,
        truncation_reason: Option<SupplyTruncationReason>,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        if sequences.len() == 0 {
            return Err(MaterializedPatternUniverseError::Empty);
        }
        if total_possible_pattern_count < sequences.len() as u128 {
            return Err(
                MaterializedPatternUniverseError::MaterializedCountExceedsTotal {
                    materialized: sequences.len(),
                    total: total_possible_pattern_count,
                },
            );
        }
        if complete && truncation_reason.is_some() {
            return Err(MaterializedPatternUniverseError::CompleteWithTruncationReason);
        }
        let weights = WeightedPatternSet::uniform_with_weight(sequences.len(), weight)
            .map_err(MaterializedPatternUniverseError::Weights)?;
        Ok(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            sequences: PatternSequenceStorage::Explicit(sequences),
            weights,
            total_possible_pattern_count,
            complete,
            truncation_reason,
            structure: MaterializedPatternUniverseStructure::Explicit,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_standard_7_bag_lexicographic(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequence_len: u16,
        pattern_count: usize,
        weight: ProbabilityValue,
        total_possible_pattern_count: u128,
        complete: bool,
        truncation_reason: Option<SupplyTruncationReason>,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        if pattern_count == 0 {
            return Err(MaterializedPatternUniverseError::Empty);
        }
        if total_possible_pattern_count < pattern_count as u128 {
            return Err(
                MaterializedPatternUniverseError::MaterializedCountExceedsTotal {
                    materialized: pattern_count,
                    total: total_possible_pattern_count,
                },
            );
        }
        if complete && truncation_reason.is_some() {
            return Err(MaterializedPatternUniverseError::CompleteWithTruncationReason);
        }
        let weights = WeightedPatternSet::uniform_with_weight(pattern_count, weight)
            .map_err(MaterializedPatternUniverseError::Weights)?;
        Ok(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            sequences: PatternSequenceStorage::Standard7BagLexicographic(
                Standard7BagSequenceSpace::new(sequence_len, pattern_count),
            ),
            weights,
            total_possible_pattern_count,
            complete,
            truncation_reason,
            structure: MaterializedPatternUniverseStructure::Standard7BagLexicographic {
                sequence_len,
            },
        })
    }

    pub(super) fn from_factorized_queue_expression(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        expression: QueuePatternExpression,
        weight: ProbabilityValue,
        total_possible_pattern_count: u128,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        let pattern_count = expression.pattern_count();
        if pattern_count == 0 {
            return Err(MaterializedPatternUniverseError::Empty);
        }
        if total_possible_pattern_count < pattern_count as u128 {
            return Err(
                MaterializedPatternUniverseError::MaterializedCountExceedsTotal {
                    materialized: pattern_count,
                    total: total_possible_pattern_count,
                },
            );
        }
        let sequence_len = u16::try_from(expression.sequence_len())
            .map_err(|_| MaterializedPatternUniverseError::SequenceStorageOverflow)?;
        let weights = WeightedPatternSet::uniform_with_weight(pattern_count, weight)
            .map_err(MaterializedPatternUniverseError::Weights)?;
        Ok(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            sequences: PatternSequenceStorage::FactorizedQueueExpression(expression),
            weights,
            total_possible_pattern_count,
            complete: true,
            truncation_reason: None,
            structure: MaterializedPatternUniverseStructure::FactorizedQueueExpression {
                sequence_len,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_observed_standard_7_bag_lexicographic(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequences: ObservedStandard7BagSequenceSpace,
        weight: ProbabilityValue,
        total_possible_pattern_count: u128,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        let pattern_count = sequences.len();
        if pattern_count == 0 {
            return Err(MaterializedPatternUniverseError::Empty);
        }
        if sequences.total_pattern_count() != total_possible_pattern_count
            || total_possible_pattern_count != pattern_count as u128
        {
            return Err(
                MaterializedPatternUniverseError::MaterializedCountExceedsTotal {
                    materialized: pattern_count,
                    total: total_possible_pattern_count,
                },
            );
        }
        let sequence_len = u16::try_from(sequences.sequence_len())
            .map_err(|_| MaterializedPatternUniverseError::SequenceStorageOverflow)?;
        let observed_len = u16::try_from(sequences.observed_len())
            .map_err(|_| MaterializedPatternUniverseError::SequenceStorageOverflow)?;
        let boundary_candidate_count = sequences.boundary_candidate_count();
        let weights = WeightedPatternSet::uniform_with_terminal_remainder(pattern_count, weight)
            .map_err(MaterializedPatternUniverseError::Weights)?;
        Ok(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            sequences: PatternSequenceStorage::ObservedStandard7BagLexicographic(sequences),
            weights,
            total_possible_pattern_count,
            complete: true,
            truncation_reason: None,
            structure: MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
                sequence_len,
                observed_len,
                boundary_candidate_count,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_flat_sequences_with_structure(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        sequences: FlatPatternSequences,
        weights: Vec<ProbabilityValue>,
        total_possible_pattern_count: u128,
        complete: bool,
        truncation_reason: Option<SupplyTruncationReason>,
        structure: MaterializedPatternUniverseStructure,
    ) -> Result<Self, MaterializedPatternUniverseError> {
        if sequences.len() == 0 {
            return Err(MaterializedPatternUniverseError::Empty);
        }
        if sequences.len() != weights.len() {
            return Err(
                MaterializedPatternUniverseError::SequenceWeightCountMismatch {
                    sequence_count: sequences.len(),
                    weight_count: weights.len(),
                },
            );
        }
        if total_possible_pattern_count < sequences.len() as u128 {
            return Err(
                MaterializedPatternUniverseError::MaterializedCountExceedsTotal {
                    materialized: sequences.len(),
                    total: total_possible_pattern_count,
                },
            );
        }
        if complete && truncation_reason.is_some() {
            return Err(MaterializedPatternUniverseError::CompleteWithTruncationReason);
        }
        let weights =
            WeightedPatternSet::new(weights).map_err(MaterializedPatternUniverseError::Weights)?;
        Ok(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            sequences: PatternSequenceStorage::Explicit(sequences),
            weights,
            total_possible_pattern_count,
            complete,
            truncation_reason,
            structure,
        })
    }

    pub const fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }

    pub fn pattern_count(&self) -> usize {
        self.sequences.len()
    }

    pub fn sequence(&self, pattern_id: PatternId) -> Cow<'_, [PieceKind]> {
        self.sequence_at(pattern_id.index())
    }

    pub fn weight(&self, pattern_id: PatternId) -> ProbabilityWeight {
        self.weight_at(pattern_id.index())
    }

    pub fn sequence_at(&self, pattern_index: usize) -> Cow<'_, [PieceKind]> {
        self.sequences.get(pattern_index)
    }

    pub fn try_sequence_at(&self, pattern_index: usize) -> Option<Cow<'_, [PieceKind]>> {
        self.sequences.try_get(pattern_index)
    }

    pub fn sequence_len_at(&self, pattern_index: usize) -> usize {
        self.sequences.sequence_len_at(pattern_index)
    }

    pub fn write_sequence_at(&self, pattern_index: usize, output: &mut Vec<PieceKind>) {
        self.sequences.write_sequence_at(pattern_index, output);
    }

    pub fn try_write_sequence_at(&self, pattern_index: usize, output: &mut Vec<PieceKind>) -> bool {
        self.sequences.try_write_sequence_at(pattern_index, output)
    }

    pub fn weight_at(&self, pattern_index: usize) -> ProbabilityWeight {
        self.weights
            .weight(PatternId::new(pattern_index))
            .expect("materialized pattern sequence and weight counts match")
    }

    pub const fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }

    pub fn weights(&self) -> &WeightedPatternSet {
        &self.weights
    }

    pub const fn total_possible_pattern_count(&self) -> u128 {
        self.total_possible_pattern_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub const fn truncation_reason(&self) -> Option<SupplyTruncationReason> {
        self.truncation_reason
    }

    pub const fn structure(&self) -> MaterializedPatternUniverseStructure {
        self.structure
    }

    pub fn lazy_sequence_storage_retained_bytes(&self) -> Option<usize> {
        match &self.sequences {
            PatternSequenceStorage::ObservedStandard7BagLexicographic(sequences) => {
                Some(sequences.retained_bytes())
            }
            PatternSequenceStorage::Explicit(_)
            | PatternSequenceStorage::Standard7BagLexicographic(_)
            | PatternSequenceStorage::FactorizedQueueExpression(_) => None,
        }
    }

    /// Returns only heap payload retained by the canonical sequence and weight
    /// representations.
    ///
    /// The inline `MaterializedPatternUniverse`, its sequence enum, and the
    /// inline uniform-weight variants are excluded. Explicit and factorized
    /// buffers are measured by allocation capacity; an explicit
    /// `Arc<[ProbabilityValue]>` is measured by its fixed payload length.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_add_bytes(
            self.sequences.checked_retained_capacity_bytes()?,
            self.weights.checked_storage_retained_bytes()?,
        )
    }

    pub fn materialized_probability_mass(&self) -> ProbabilityValue {
        self.weights.total_weight()
    }
}

fn checked_add_bytes(left: u128, right: u128) -> Option<u128> {
    left.checked_add(right)
}

impl PatternSequenceReader for MaterializedPatternUniverse {
    fn pattern_count(&self) -> usize {
        self.sequences.len()
    }

    fn sequence(&self, pattern_id: PatternId) -> Cow<'_, [PieceKind]> {
        self.sequences.get(pattern_id.index())
    }

    fn weight(&self, pattern_id: PatternId) -> ProbabilityWeight {
        self.weights
            .weight(pattern_id)
            .expect("materialized pattern sequence and weight counts match")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterializedPatternUniverseError {
    Empty,
    SequenceWeightCountMismatch {
        sequence_count: usize,
        weight_count: usize,
    },
    MaterializedCountExceedsTotal {
        materialized: usize,
        total: u128,
    },
    CompleteWithTruncationReason,
    SequenceStorageOverflow,
    Weights(WeightedPatternSetError),
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };

    use super::{checked_add_bytes, MaterializedPatternUniverse, PatternSequenceStorage};
    use crate::queue::queue_pattern_expression::QueuePatternExpression;

    #[test]
    fn explicit_retained_capacity_is_flat_sequences_plus_weight_payload() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(2),
            vec![
                vec![PieceKind::I, PieceKind::O],
                vec![PieceKind::T, PieceKind::S],
            ],
            vec![
                ProbabilityValue::new(0.5).expect("weight"),
                ProbabilityValue::new(0.5).expect("weight"),
            ],
            2,
            true,
            None,
        )
        .expect("explicit universe");
        let sequence_bytes = match &universe.sequences {
            PatternSequenceStorage::Explicit(sequences) => sequences
                .checked_retained_capacity_bytes()
                .expect("sequence storage fits u128"),
            _ => panic!("explicit constructor keeps flat storage"),
        };
        let weight_bytes = universe
            .weights
            .checked_storage_retained_bytes()
            .expect("weight storage fits u128");

        assert_eq!(
            universe.checked_retained_capacity_bytes(),
            sequence_bytes.checked_add(weight_bytes)
        );
        assert!(weight_bytes > 0);
    }

    #[test]
    fn p7_p7_p2_factorized_universe_retains_compact_expression_storage() {
        const PATTERN_COUNT: usize = 1_066_867_200;
        let expression = QueuePatternExpression::parse("P7P7P2", PATTERN_COUNT)
            .expect("bounded factorized expression");
        assert!(expression.is_factorized());
        assert_eq!(expression.pattern_count(), PATTERN_COUNT);
        let expected = expression
            .checked_retained_capacity_bytes()
            .expect("expression storage fits u128");
        let probability = ProbabilityValue::new(1.0 / PATTERN_COUNT as f64).expect("weight");
        let universe = MaterializedPatternUniverse::from_factorized_queue_expression(
            PatternUniverseId::new(3),
            PatternWeightModelId::new(4),
            expression,
            probability,
            PATTERN_COUNT as u128,
        )
        .expect("factorized universe");

        assert_eq!(universe.pattern_count(), PATTERN_COUNT);
        assert_eq!(universe.checked_retained_capacity_bytes(), Some(expected));
        assert!(expected < 1024 * 1024);
    }

    #[test]
    fn retained_capacity_addition_fails_closed_on_overflow() {
        assert_eq!(checked_add_bytes(u128::MAX, 1), None);
    }
}
