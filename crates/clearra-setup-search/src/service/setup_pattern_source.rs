use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_supply::{
    mixed::supply_provenance::{BagBoundaryEvidence, SupplyProvenance},
    MaterializedPatternUniverse, PatternUniverseMaterializer,
};

use crate::query::{SetupQueueInput, SetupSearchQuery};

use super::setup_search_service::SetupSearchExecutionError;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetupPatternSource {
    pub(crate) mode: &'static str,
    pub(crate) patterns: Vec<SetupSourcePattern>,
    pub(crate) pattern_count: usize,
    pub(crate) total_pattern_count: u128,
    pub(crate) weights: WeightedPatternSet,
    pub(crate) probability_complete: bool,
    pub(crate) expansion_truncated: bool,
}

impl SetupPatternSource {
    pub(crate) fn from_query(query: &SetupSearchQuery) -> Result<Self, SetupSearchExecutionError> {
        if query.queue().is_empty() && !matches!(query.queue(), SetupQueueInput::Observed(_)) {
            return Err(SetupSearchExecutionError::EmptyQueue);
        }
        let provenance_id = standard_supply_provenance(query).supply_provenance_id();
        let (mode, universe) = match query.queue() {
            SetupQueueInput::Observed(queue) => ("observed", {
                let empty_hold_lookahead = usize::from(
                    query.hold_policy().is_enabled()
                        && query.hold_policy().initial_piece().is_none(),
                );
                let minimum_len =
                    query.piece_budget().max_piece_count() as usize + empty_hold_lookahead;
                PatternUniverseMaterializer::observed(
                    queue,
                    minimum_len,
                    query.limits().max_patterns(),
                    provenance_id,
                )
                .map_err(|_| SetupSearchExecutionError::ExpandObservedQueue)?
            }),
            SetupQueueInput::FixedSequence(sequence) => (
                "fixed",
                PatternUniverseMaterializer::fixed_sequence(sequence, provenance_id),
            ),
            SetupQueueInput::BagAlignedPattern(pattern) => (
                "bag-aligned",
                PatternUniverseMaterializer::bag_aligned_pattern(pattern, provenance_id),
            ),
        };
        Ok(from_materialized_universe(mode, universe))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SetupSourcePattern {
    pub(crate) pattern_index: usize,
    pub(crate) queue_pieces: Vec<PieceKind>,
}

impl SetupSourcePattern {
    fn new(pattern_index: usize, queue_pieces: Vec<PieceKind>) -> Self {
        Self {
            pattern_index,
            queue_pieces,
        }
    }
}

fn from_materialized_universe(
    mode: &'static str,
    universe: MaterializedPatternUniverse,
) -> SetupPatternSource {
    let pattern_count = universe.pattern_count();
    SetupPatternSource {
        mode,
        patterns: (0..pattern_count)
            .map(|pattern_index| {
                SetupSourcePattern::new(pattern_index, universe.sequence_at(pattern_index).to_vec())
            })
            .collect(),
        pattern_count,
        total_pattern_count: universe.total_possible_pattern_count(),
        weights: universe.weights().clone(),
        probability_complete: universe.complete(),
        expansion_truncated: !universe.complete(),
    }
}

fn standard_supply_provenance(query: &SetupSearchQuery) -> SupplyProvenance {
    let observed_window_id = match query.queue() {
        SetupQueueInput::Observed(observed) => Some(format!(
            "observed:{}:{}:{}",
            observed.len(),
            query.queue().mode(),
            observed
                .pieces()
                .iter()
                .map(|piece| piece.as_ascii())
                .collect::<String>()
        )),
        SetupQueueInput::FixedSequence(_) | SetupQueueInput::BagAlignedPattern(_) => None,
    };
    SupplyProvenance::new(
        "standard-7-bag",
        "standard-tetrominoes",
        observed_window_id,
        if matches!(query.queue(), SetupQueueInput::Observed(_)) {
            BagBoundaryEvidence::ObservedCompatible
        } else {
            BagBoundaryEvidence::FixedBoundary
        },
        false,
        matches!(query.queue(), SetupQueueInput::Observed(_)),
    )
    .expect("standard setup supply provenance is valid")
}
