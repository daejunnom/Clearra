use clearra_core_domain::piece::piece_kind::{PieceKind, UnknownPieceKind};
use clearra_setup_search::query::{SetupCycleResetBorrowPolicy, SetupSearchQuery};

use crate::args::setup_args::SetupArgs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupQueryAssemblyError {
    UnknownPiece { value: char },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupQueryAssembler;

impl SetupQueryAssembler {
    pub fn assemble(args: &SetupArgs) -> Result<SetupSearchQuery, SetupQueryAssemblyError> {
        let pieces = parse_remaining(args.remaining())?;
        let borrow_policy = if args.allow_post_cycle_borrow() {
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        } else {
            SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse
        };

        Ok(SetupSearchQuery::default()
            .with_remaining_pieces(pieces)
            .with_cycle_reset_borrow_policy(borrow_policy))
    }
}

fn parse_remaining(remaining: &str) -> Result<Vec<PieceKind>, SetupQueryAssemblyError> {
    remaining
        .chars()
        .filter(|value| !value.is_whitespace() && *value != ',')
        .map(parse_piece)
        .collect()
}

fn parse_piece(value: char) -> Result<PieceKind, SetupQueryAssemblyError> {
    PieceKind::from_ascii(value.to_ascii_uppercase())
        .map_err(|UnknownPieceKind| SetupQueryAssemblyError::UnknownPiece { value })
}

#[cfg(test)]
#[path = "setup_query_assembler_tests.rs"]
mod tests;
