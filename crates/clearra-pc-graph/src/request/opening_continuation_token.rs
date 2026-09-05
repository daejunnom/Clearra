use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_profiles::{
    bag::bag_profile::BagProfileId, board::board_profile::BoardProfileId,
    pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

use super::{
    continuation_token_error::PcContinuationTokenError,
    continuation_token_segments::{
        count_policy_name, format_hold_piece, format_piece_sequence, objective_name,
        opening_hold_policy, parse_bool_digit_prefixed, parse_count_policy, parse_hold_piece,
        parse_objective, parse_queue, parse_rule_profile, parse_target, prefixed_value,
        require_value,
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
        "pc2:l{}:bd{}:ps{}:bg{}:r{}:o{}:e{}:h{}:q{}:qk{}:c{}",
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
        count_policy_name(query.count_policy()),
    )
}

pub(crate) fn parse_opening_v2(
    token: &str,
) -> Result<OpeningPcSearchQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    if !(10..=12).contains(&parts.len()) || parts[0] != "pc2" {
        return Err(PcContinuationTokenError::new(
            "continuation token must use pc2:lN:bdPROFILE:psPROFILE:bgPROFILE:rRULE:oOBJECTIVE:e0|1:hX:qPIECES[:qkoracle|visible-7][:cfirst-solution|count-all|count-unique] format",
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
    let mut queue_observation_policy = QueueObservationPolicy::default();
    let mut has_queue_observation_policy = false;
    let mut count_policy = None;
    for part in &parts[10..] {
        if part.starts_with("qk") {
            if has_queue_observation_policy {
                return Err(PcContinuationTokenError::new(
                    "opening continuation token repeats queue observation policy",
                ));
            }
            has_queue_observation_policy = true;
            queue_observation_policy = QueueObservationPolicy::from_keyword(prefixed_value(
                part, "qk",
            )?)
            .ok_or_else(|| {
                PcContinuationTokenError::new("unsupported opening queue observation policy")
            })?;
        } else if part.starts_with('c') {
            if count_policy.is_some() {
                return Err(PcContinuationTokenError::new(
                    "opening continuation token repeats count policy",
                ));
            }
            count_policy = Some(parse_count_policy(prefixed_value(part, "c")?)?);
        } else {
            return Err(PcContinuationTokenError::new(format!(
                "unsupported opening continuation field '{part}'"
            )));
        }
    }
    let mut query = OpeningPcSearchQuery::new(target)
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(queue)))
        .with_hold_policy(opening_hold_policy(hold_enabled, hold_piece))
        .with_rule(rule)
        .with_objective(objective)
        .with_queue_observation_policy(queue_observation_policy);
    if let Some(count_policy) = count_policy {
        query = query.with_count_policy(count_policy);
    }
    Ok(query)
}
