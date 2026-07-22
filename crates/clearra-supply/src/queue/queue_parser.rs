use clearra_core_domain::piece::piece_kind::{PieceKind, UnknownPieceKind};

use super::{
    bag_aligned_pattern::BagAlignedPattern, fixed_queue::FixedQueue, fixed_sequence::FixedSequence,
    observed_queue::ObservedQueue,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueParseError {
    UnknownPiece { index: usize, value: char },
}

pub fn parse_piece_sequence(input: &str) -> Result<Vec<PieceKind>, QueueParseError> {
    let mut pieces = Vec::new();

    for (index, value) in input.chars().enumerate() {
        if value.is_whitespace() || matches!(value, ',' | '-' | '_' | '|') {
            continue;
        }

        let piece = PieceKind::from_ascii(value)
            .map_err(|UnknownPieceKind| QueueParseError::UnknownPiece { index, value })?;
        pieces.push(piece);
    }

    Ok(pieces)
}

pub fn parse_fixed_queue(input: &str) -> Result<FixedQueue, QueueParseError> {
    parse_fixed_sequence(input)
}

pub fn parse_fixed_sequence(input: &str) -> Result<FixedSequence, QueueParseError> {
    parse_piece_sequence(input).map(FixedSequence::new)
}

pub fn parse_bag_aligned_pattern(input: &str) -> Result<BagAlignedPattern, QueueParseError> {
    parse_piece_sequence(input).map(BagAlignedPattern::new)
}

pub fn parse_observed_queue(input: &str) -> Result<ObservedQueue, QueueParseError> {
    parse_piece_sequence(input).map(ObservedQueue::new)
}

#[cfg(test)]
#[path = "queue_parser_tests.rs"]
mod tests;
