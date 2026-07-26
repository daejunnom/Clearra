use clearra_core_domain::piece::piece_kind::{PieceKind, UnknownPieceKind};
use clearra_setup_search::query::{
    SetupCycleResetBorrowPolicy, SetupPathDetail, SetupSearchMode, SetupSearchQuery,
};

use crate::args::setup_args::SetupArgs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupQueryAssemblyError {
    UnknownPiece { value: char },
    QueueBasedPiecesMissing,
    QueueBasedPiecesUnexpected,
    PathDetailInvalid,
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

        let mut query = SetupSearchQuery::default().with_remaining_pieces(pieces);
        match (args.search_mode(), args.queue_based_pieces()) {
            (SetupSearchMode::ShapeOracle, None) => {}
            (SetupSearchMode::ShapeOracle, Some(_)) => {
                return Err(SetupQueryAssemblyError::QueueBasedPiecesUnexpected);
            }
            (SetupSearchMode::QueueBased, None) => {
                return Err(SetupQueryAssemblyError::QueueBasedPiecesMissing);
            }
            (SetupSearchMode::QueueBased, Some(value)) => {
                query = query.with_queue_based_pieces(parse_remaining(value)?);
            }
        }

        let mut query = query
            .with_cycle_reset_borrow_policy(borrow_policy)
            .with_candidate_priority(args.candidate_priority())
            .with_length_preference(args.length_preference());
        match (args.path_detail_setup_id(), args.path_detail_condition_id()) {
            (Some(setup_id), Some(condition_id)) => {
                let board_mask =
                    parse_setup_id(setup_id).ok_or(SetupQueryAssemblyError::PathDetailInvalid)?;
                let detail = SetupPathDetail::new(board_mask, condition_id)
                    .ok_or(SetupQueryAssemblyError::PathDetailInvalid)?;
                query = query.with_path_detail(detail);
            }
            (None, None) => {}
            _ => return Err(SetupQueryAssemblyError::PathDetailInvalid),
        }
        Ok(query)
    }
}

fn parse_setup_id(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("setup-")?;
    u64::from_str_radix(digits, 16)
        .ok()
        .filter(|mask| *mask >> 40 == 0)
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
