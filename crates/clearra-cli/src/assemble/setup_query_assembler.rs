use clearra_core_domain::piece::piece_kind::{PieceKind, UnknownPieceKind};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_setup_search::query::{
    SetupCycleResetBorrowPolicy, SetupHoldPolicy, SetupPathDetail, SetupSearchMode,
    SetupSearchQuery,
};

use crate::{
    args::setup_args::SetupArgs,
    assemble::rule_profile_assembler::{RuleProfileAssembler, RuleProfileAssemblyError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupQueryAssemblyError {
    UnknownPiece { value: char },
    QueueBasedPiecesMissing,
    QueueBasedPiecesUnexpected,
    InitialHoldInvalid,
    PathDetailInvalid,
    RuleProfile(RuleProfileAssemblyError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupQueryAssembler;

impl SetupQueryAssembler {
    pub fn assemble(args: &SetupArgs) -> Result<SetupSearchQuery, SetupQueryAssemblyError> {
        let pieces = parse_remaining(args.remaining())?;
        let rule = RuleProfileAssembler::parse_optional_rule(args.rule(), srs_plus())
            .map_err(SetupQueryAssemblyError::RuleProfile)?;
        let borrow_policy = if args.allow_post_cycle_borrow() {
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        } else {
            SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse
        };

        let mut query = SetupSearchQuery::default()
            .with_rule(rule)
            .with_queue_observation_policy(args.queue_observation_policy())
            .with_remaining_pieces(pieces);
        if let Some(value) = args.initial_hold() {
            query = query.with_hold_policy(parse_initial_hold(value)?);
        }
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
        if let Some(value) = args.next_cycle_remaining_pieces() {
            query = query.with_next_cycle_remaining_pieces(parse_remaining(value)?);
        }

        let mut query = query
            .with_cycle_reset_borrow_policy(borrow_policy)
            .with_candidate_priority(args.candidate_priority())
            .with_length_preference(args.length_preference())
            .with_max_setup_pieces(args.max_setup_pieces());
        match (args.path_detail_setup_id(), args.path_detail_condition_id()) {
            (Some(setup_id), Some(condition_id)) => {
                let detail = SetupPathDetail::from_setup_id(setup_id, condition_id)
                    .ok_or(SetupQueryAssemblyError::PathDetailInvalid)?;
                query = query.with_path_detail(detail);
            }
            (None, None) => {}
            _ => return Err(SetupQueryAssemblyError::PathDetailInvalid),
        }
        Ok(query)
    }
}

fn parse_remaining(remaining: &str) -> Result<Vec<PieceKind>, SetupQueryAssemblyError> {
    remaining
        .chars()
        .filter(|value| !value.is_whitespace() && *value != ',')
        .map(parse_piece)
        .collect()
}

fn parse_initial_hold(value: &str) -> Result<SetupHoldPolicy, SetupQueryAssemblyError> {
    if value.eq_ignore_ascii_case("empty") {
        return Ok(SetupHoldPolicy::EnabledEmpty);
    }
    let mut pieces = parse_remaining(value)?;
    if pieces.len() != 1 {
        return Err(SetupQueryAssemblyError::InitialHoldInvalid);
    }
    Ok(SetupHoldPolicy::EnabledWithPiece(pieces.remove(0)))
}

fn parse_piece(value: char) -> Result<PieceKind, SetupQueryAssemblyError> {
    PieceKind::from_ascii(value.to_ascii_uppercase())
        .map_err(|UnknownPieceKind| SetupQueryAssemblyError::UnknownPiece { value })
}

#[cfg(test)]
#[path = "setup_query_assembler_tests.rs"]
mod tests;
