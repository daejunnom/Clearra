use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_profiles::{
    bag::bag_profile::BagProfileId, board::board_profile::BoardProfileId,
    pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

use super::{
    continuation_token_error::PcContinuationTokenError,
    continuation_token_segments::{
        format_hold_piece, format_piece_sequence, objective_name, opening_hold_policy,
        parse_bool_digit_prefixed, parse_hold_piece, parse_objective, parse_queue,
        parse_rule_profile, parse_target, prefixed_value, require_value,
    },
    opening_pc_search_query::OpeningPcSearchQuery,
    pc_queue_input::PcQueueInput,
};

pub(crate) fn encode_opening_continuation(
    query: &OpeningPcSearchQuery,
    hold_piece: Option<PieceKind>,
    pieces: &[PieceKind],
) -> String {
    format!(
        "pc2:l{}:bd{}:ps{}:bg{}:r{}:o{}:e{}:h{}:q{}:qk{}",
        query.target().lines(),
        query.board().id().as_str(),
        query.piece_set().id().as_str(),
        query.bag().id().as_str(),
        query.rule().id().as_str(),
        objective_name(query.objective()),
        if query.hold_policy().is_enabled() {
            1
        } else {
            0
        },
        format_hold_piece(hold_piece),
        format_piece_sequence(pieces),
        query.queue_observation_policy().keyword(),
    )
}

pub(crate) fn parse_opening_v2(
    token: &str,
) -> Result<OpeningPcSearchQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    if !(parts.len() == 10 || parts.len() == 11) || parts[0] != "pc2" {
        return Err(PcContinuationTokenError::new(
            "continuation token must use pc2:lN:bdPROFILE:psPROFILE:bgPROFILE:rRULE:oOBJECTIVE:e0|1:hX:qPIECES:qkoracle|visible-7 format",
        ));
    }
    require_value(
        prefixed_value(parts[2], "bd")?,
        BoardProfileId::Standard10.as_str(),
        "opening board profile",
    )?;
    require_value(
        prefixed_value(parts[3], "ps")?,
        PieceSetProfileId::StandardTetrominoes.as_str(),
        "opening piece set profile",
    )?;
    require_value(
        prefixed_value(parts[4], "bg")?,
        BagProfileId::Standard7Bag.as_str(),
        "opening bag profile",
    )?;

    let target = parse_target(parts[1])?;
    let rule = parse_rule_profile(prefixed_value(parts[5], "r")?)?;
    let objective = parse_objective(prefixed_value(parts[6], "o")?)?;
    let hold_enabled = parse_bool_digit_prefixed(parts[7], "e")?;
    let hold_piece = parse_hold_piece(parts[8])?;
    if !hold_enabled && hold_piece.is_some() {
        return Err(PcContinuationTokenError::new(
            "disabled hold token cannot carry a hold piece",
        ));
    }
    let queue = parse_queue(parts[9])?;
    let queue_observation_policy = if parts.len() == 11 {
        QueueObservationPolicy::from_keyword(prefixed_value(parts[10], "qk")?).ok_or_else(|| {
            PcContinuationTokenError::new("unsupported opening queue observation policy")
        })?
    } else {
        QueueObservationPolicy::default()
    };
    Ok(OpeningPcSearchQuery::new(target)
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(queue)))
        .with_hold_policy(opening_hold_policy(hold_enabled, hold_piece))
        .with_rule(rule)
        .with_objective(objective)
        .with_queue_observation_policy(queue_observation_policy))
}
