use clearra_coverage::{
    pattern::weighted_pattern_set::WeightedPatternSet,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::{
    mixed::supply_provenance::SupplyProvenance,
    pattern_universe::{
        MaterializedPatternUniverse, PatternUniverseMaterializationError,
        PatternUniverseMaterializer,
    },
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        observed_queue::ObservedQueue, queue_pattern_expression::QueuePatternExpression,
    },
};

use super::{
    piece_source_identity::piece_source_id, BagUniverseDescriptor, FixedPieceSequence,
    ObservedWindowDescriptor, PieceSetId, PieceSourceId, PieceSourceKind, SupplyTruncationReason,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PieceSource {
    id: PieceSourceId,
    kind: PieceSourceKind,
    piece_set_id: PieceSetId,
    provenance: SupplyProvenance,
    fixed_sequence: Option<FixedPieceSequence>,
    bag_universe: Option<BagUniverseDescriptor>,
    observed_window: Option<ObservedWindowDescriptor>,
    materialized_universe: MaterializedPatternUniverse,
}

impl PieceSource {
    pub fn fixed_queue(sequence: FixedSequence, provenance: SupplyProvenance) -> Self {
        let universe = PatternUniverseMaterializer::fixed_sequence(
            &sequence,
            provenance.supply_provenance_id(),
        );
        let fixed_sequence = FixedPieceSequence::new(sequence.into_pieces());
        Self::from_parts(
            PieceSourceKind::FixedQueue,
            provenance,
            Some(fixed_sequence),
            None,
            None,
            universe,
        )
    }

    pub fn bag_universe(pattern: BagAlignedPattern, provenance: SupplyProvenance) -> Self {
        let universe = PatternUniverseMaterializer::bag_aligned_pattern(
            &pattern,
            provenance.supply_provenance_id(),
        );
        let bag_universe = BagUniverseDescriptor::new(pattern.into_pieces());
        Self::from_parts(
            PieceSourceKind::BagUniverse,
            provenance,
            None,
            Some(bag_universe),
            None,
            universe,
        )
    }

    pub fn standard_7_bag(
        provenance: SupplyProvenance,
        minimum_sequence_len: usize,
        max_patterns: usize,
    ) -> Result<Self, PatternUniverseMaterializationError> {
        let universe = PatternUniverseMaterializer::standard_7_bag(
            minimum_sequence_len,
            max_patterns,
            provenance.supply_provenance_id(),
        )?;
        let bag_universe = BagUniverseDescriptor::new(
            clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES.to_vec(),
        );
        Ok(Self::from_parts(
            PieceSourceKind::BagUniverse,
            provenance,
            None,
            Some(bag_universe),
            None,
            universe,
        ))
    }

    pub fn queue_pattern_expression(
        expression: QueuePatternExpression,
        provenance: SupplyProvenance,
    ) -> Result<Self, PatternUniverseMaterializationError> {
        let universe = PatternUniverseMaterializer::queue_pattern_expression(
            &expression,
            provenance.supply_provenance_id(),
        )?;
        Ok(Self::materialized_pattern_universe(universe, provenance))
    }

    pub fn observed_window(
        observed: ObservedQueue,
        provenance: SupplyProvenance,
        minimum_sequence_len: usize,
        max_patterns: usize,
    ) -> Result<Self, PatternUniverseMaterializationError> {
        let universe = PatternUniverseMaterializer::observed(
            &observed,
            minimum_sequence_len,
            max_patterns,
            provenance.supply_provenance_id(),
        )?;
        let observed_window =
            ObservedWindowDescriptor::new(observed.pieces().to_vec(), max_patterns);
        Ok(Self::from_parts(
            PieceSourceKind::ObservedWindow,
            provenance,
            None,
            None,
            Some(observed_window),
            universe,
        ))
    }

    pub fn materialized_pattern_universe(
        materialized_universe: MaterializedPatternUniverse,
        provenance: SupplyProvenance,
    ) -> Self {
        Self::from_parts(
            PieceSourceKind::MaterializedPatternUniverse,
            provenance,
            None,
            None,
            None,
            materialized_universe,
        )
    }

    fn from_parts(
        kind: PieceSourceKind,
        provenance: SupplyProvenance,
        fixed_sequence: Option<FixedPieceSequence>,
        bag_universe: Option<BagUniverseDescriptor>,
        observed_window: Option<ObservedWindowDescriptor>,
        materialized_universe: MaterializedPatternUniverse,
    ) -> Self {
        let piece_set_id = PieceSetId::STANDARD_TETROMINOES;
        let id = piece_source_id(
            kind,
            piece_set_id,
            provenance.supply_provenance_id(),
            Some(materialized_universe.pattern_universe_id()),
            Some(materialized_universe.pattern_weight_model_id()),
            materialized_universe.pattern_count() as u64,
            materialized_universe.pattern_universe_id().get(),
        );
        Self {
            id,
            kind,
            piece_set_id,
            provenance,
            fixed_sequence,
            bag_universe,
            observed_window,
            materialized_universe,
        }
    }

    pub const fn id(&self) -> PieceSourceId {
        self.id
    }

    pub const fn kind(&self) -> PieceSourceKind {
        self.kind
    }

    pub const fn piece_set_id(&self) -> PieceSetId {
        self.piece_set_id
    }

    pub const fn provenance(&self) -> &SupplyProvenance {
        &self.provenance
    }

    pub fn pattern_universe_id(&self) -> Option<PatternUniverseId> {
        Some(self.materialized_universe.pattern_universe_id())
    }

    pub fn pattern_weight_model_id(&self) -> Option<PatternWeightModelId> {
        Some(self.materialized_universe.pattern_weight_model_id())
    }

    pub fn materialized_pattern_weights(&self) -> Option<&WeightedPatternSet> {
        Some(self.materialized_universe.weights())
    }

    pub fn fixed_sequence(&self) -> Option<&FixedPieceSequence> {
        self.fixed_sequence.as_ref()
    }

    pub fn bag_universe_descriptor(&self) -> Option<&BagUniverseDescriptor> {
        self.bag_universe.as_ref()
    }

    pub fn observed_window_descriptor(&self) -> Option<&ObservedWindowDescriptor> {
        self.observed_window.as_ref()
    }

    pub fn materialized_universe(&self) -> Option<&MaterializedPatternUniverse> {
        Some(&self.materialized_universe)
    }

    pub const fn complete(&self) -> bool {
        self.materialized_universe.complete()
    }

    pub const fn truncation_reason(&self) -> Option<SupplyTruncationReason> {
        self.materialized_universe.truncation_reason()
    }

    pub fn piece_source_is_immutable_shared_source(&self) -> bool {
        self.id.get() != 0 && self.provenance.supply_provenance_id() != 0
    }
}

#[cfg(test)]
#[path = "piece_source_tests.rs"]
mod tests;
