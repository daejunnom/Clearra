//! Stable decode-only support for version 1 continuation tokens.
//!
//! Current encoders emit version 2 tokens. This module is isolated from solver
//! execution and only upgrades explicit `pc1` and `sc1` inputs into canonical
//! query types.

use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    continuation_token_error::PcContinuationTokenError,
    continuation_token_segments::{
        opening_hold_policy, parse_bool_digit_prefixed, parse_hold_piece, parse_mask_prefixed,
        parse_queue, parse_target, parse_u16_prefixed, parse_usize_prefixed,
    },
    opening_pc_search_query::OpeningPcSearchQuery,
    pc_queue_input::PcQueueInput,
    pc_scenario_query::{PcScenarioBoard, PcScenarioQuery, PieceWindow},
};

pub(crate) fn parse_opening_v1(
    token: &str,
) -> Result<OpeningPcSearchQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    if parts.len() != 5 || parts[0] != "pc1" {
        return Err(PcContinuationTokenError::new(
            "continuation token must use pc1:lN:e0|1:hX:qPIECES format",
        ));
    }

    let target = parse_target(parts[1])?;
    let hold_enabled = parse_bool_digit_prefixed(parts[2], "e")?;
    let hold_piece = parse_hold_piece(parts[3])?;
    if !hold_enabled && hold_piece.is_some() {
        return Err(PcContinuationTokenError::new(
            "disabled hold token cannot carry a hold piece",
        ));
    }
    let queue = parse_queue(parts[4])?;

    Ok(OpeningPcSearchQuery::new(target)
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(queue)))
        .with_hold_policy(opening_hold_policy(hold_enabled, hold_piece)))
}

pub(crate) fn parse_scenario_v1(token: &str) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    if parts.len() != 7 || parts[0] != "sc1" {
        return Err(PcContinuationTokenError::new(
            "scenario continuation token must use sc1:w10:v2:m0x...:hnone:qPIECES:pN format",
        ));
    }
    Ok(PcScenarioQuery::new(
        PcScenarioBoard::new(
            parse_u16_prefixed(parts[1], "w")?,
            parse_u16_prefixed(parts[2], "v")?,
            parse_mask_prefixed(parts[3])?,
        ),
        PcQueueInput::fixed_sequence(FixedSequence::new(parse_queue(parts[5])?)),
        PieceWindow::new(parse_usize_prefixed(parts[6], "p")?),
    )
    .with_hold_piece(parse_hold_piece(parts[4])?))
}
