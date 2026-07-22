use clearra_core_domain::piece::piece_kind::{PieceKind, UnknownPieceKind};
use clearra_setup_search::query::{SetupQueueInput, SetupSearchQuery};
use clearra_supply::queue::{fixed_sequence::FixedSequence, observed_queue::ObservedQueue};

use crate::args::setup_args::SetupArgs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupQueryAssemblyError {
    UnknownPiece { value: char },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupQueryAssembler;

impl SetupQueryAssembler {
    pub fn assemble(args: &SetupArgs) -> Result<SetupSearchQuery, SetupQueryAssemblyError> {
        let pieces = parse_queue(args.queue())?;
        let queue = if args.fixed_queue() {
            SetupQueueInput::fixed_sequence(FixedSequence::new(pieces))
        } else {
            SetupQueueInput::observed(ObservedQueue::new(pieces))
        };

        Ok(SetupSearchQuery::default().with_queue(queue))
    }
}

fn parse_queue(queue: &str) -> Result<Vec<PieceKind>, SetupQueryAssemblyError> {
    queue
        .chars()
        .filter(|value| !value.is_whitespace() && *value != ',')
        .map(parse_piece)
        .collect()
}

fn parse_piece(value: char) -> Result<PieceKind, SetupQueryAssemblyError> {
    PieceKind::from_ascii(value)
        .map_err(|UnknownPieceKind| SetupQueryAssemblyError::UnknownPiece { value })
}

#[cfg(test)]
#[path = "setup_query_assembler_tests.rs"]
mod tests;
