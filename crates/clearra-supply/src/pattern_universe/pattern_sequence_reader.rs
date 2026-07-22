use std::borrow::Cow;

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_coverage::pattern::pattern_id::PatternId;

pub type ProbabilityWeight = ProbabilityValue;

pub trait PatternSequenceReader {
    fn pattern_count(&self) -> usize;
    fn sequence(&self, pattern_id: PatternId) -> Cow<'_, [PieceKind]>;
    fn weight(&self, pattern_id: PatternId) -> ProbabilityWeight;
}
