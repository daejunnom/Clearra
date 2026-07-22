use clearra_profiles::{
    bag::bag_profile::BagProfileId, pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    continuation_kick_profile_codec::{format_kick_profile, parse_kick_profile},
    continuation_token_error::PcContinuationTokenError,
    continuation_token_segments::{
        bool_digit, count_policy_name, format_hold_piece, format_optional_usize,
        format_piece_sequence, parse_bool_digit_prefixed, parse_completion_goal,
        parse_count_policy, parse_hold_piece, parse_mask_prefixed, parse_optional_usize_prefixed,
        parse_queue, parse_rule_profile, parse_u16_prefixed, parse_usize_prefixed, prefixed_value,
        require_value,
    },
    pc_queue_input::PcQueueInput,
    pc_scenario_query::{PcCompletionGoal, PcScenarioBoard, PcScenarioQuery, PieceWindow},
    PcSolutionProbabilityPolicy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScenarioContinuationTokenKind {
    Continuation,
    Replay,
}

impl ScenarioContinuationTokenKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Continuation => "sc2",
            Self::Replay => "sr2",
        }
    }
}
impl ScenarioContinuationTokenKind {
    fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "sc2" => Some(Self::Continuation),
            "sr2" => Some(Self::Replay),
            _ => None,
        }
    }
}

pub(crate) fn encode_scenario_continuation(
    query: &PcScenarioQuery,
) -> Result<String, PcContinuationTokenError> {
    encode_scenario_query_with_kind(ScenarioContinuationTokenKind::Continuation, query)
}

pub(crate) fn encode_scenario_replay(
    query: &PcScenarioQuery,
) -> Result<String, PcContinuationTokenError> {
    encode_scenario_query_with_kind(ScenarioContinuationTokenKind::Replay, query)
}

pub(crate) fn parse_scenario_continuation_v2(
    token: &str,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    parse_scenario_v2(token, ScenarioContinuationTokenKind::Continuation)
}

pub(crate) fn parse_scenario_replay_v2(
    token: &str,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    parse_scenario_v2(token, ScenarioContinuationTokenKind::Replay)
}

fn encode_scenario_query_with_kind(
    kind: ScenarioContinuationTokenKind,
    query: &PcScenarioQuery,
) -> Result<String, PcContinuationTokenError> {
    let pieces = query
        .remaining_queue()
        .as_fixed_sequence()
        .ok_or_else(|| {
            PcContinuationTokenError::new(
                "scenario continuation token requires a fixed remaining queue",
            )
        })?
        .pieces();
    let prefix = kind.prefix();
    let kick_profile = format_kick_profile(query.verified_kick_profile())?;
    Ok(format!(
        "{prefix}:w{}:v{}:m0x{:016x}:ps{}:bg{}:r{}:h{}:q{}:p{}:x{}:n{}:a{}:z{}:g{}:c{}:t{}:k{}:u{}",
        query.initial_board().width(),
        query.initial_board().visible_height(),
        query.initial_board().occupied_mask(),
        query.piece_set().id().as_str(),
        query.bag().id().as_str(),
        query.rule().id().as_str(),
        format_hold_piece(query.hold_state().piece()),
        format_piece_sequence(pieces),
        query.piece_window().max_pieces(),
        format_optional_usize(query.exact_pieces()),
        query.min_remaining_queue(),
        bool_digit(query.allow_hold()),
        bool_digit(query.requires_180()),
        query.completion_goal().as_str(),
        count_policy_name(query.count_policy()),
        query.retained_trace_limit(),
        kick_profile,
        bool_digit(query.solution_probability_policy().requested()),
    ))
}

fn parse_scenario_v2(
    token: &str,
    expected_kind: ScenarioContinuationTokenKind,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    if !(parts.len() == 17 || parts.len() == 18 || parts.len() == 19)
        || ScenarioContinuationTokenKind::from_prefix(parts[0]) != Some(expected_kind)
    {
        return Err(PcContinuationTokenError::new(
            "scenario token must use sc2|sr2:w10:v2:m0x...:psPROFILE:bgPROFILE:rRULE:hnone:qPIECES:pN:xN|none:nN:a0|1:z0|1:gclear-to-empty:cPOLICY:tN:kPROFILE:u0|1 format",
        ));
    }
    require_value(
        prefixed_value(parts[4], "ps")?,
        PieceSetProfileId::StandardTetrominoes.as_str(),
        "scenario piece set profile",
    )?;
    require_value(
        prefixed_value(parts[5], "bg")?,
        BagProfileId::Standard7Bag.as_str(),
        "scenario bag profile",
    )?;
    let completion_goal = parse_completion_goal(prefixed_value(parts[14], "g")?)?;

    let mut query = PcScenarioQuery::new(
        PcScenarioBoard::new(
            parse_u16_prefixed(parts[1], "w")?,
            parse_u16_prefixed(parts[2], "v")?,
            parse_mask_prefixed(parts[3])?,
        ),
        PcQueueInput::fixed_sequence(FixedSequence::new(parse_queue(parts[8])?)),
        PieceWindow::new(parse_usize_prefixed(parts[9], "p")?),
    )
    .with_rule(parse_rule_profile(prefixed_value(parts[6], "r")?)?)
    .with_hold_piece(parse_hold_piece(parts[7])?)
    .with_exact_pieces(parse_optional_usize_prefixed(parts[10], "x")?)
    .with_min_remaining_queue(parse_usize_prefixed(parts[11], "n")?)
    .with_allow_hold(parse_bool_digit_prefixed(parts[12], "a")?)
    .with_requires_180(parse_bool_digit_prefixed(parts[13], "z")?)
    .with_count_policy(parse_count_policy(prefixed_value(parts[15], "c")?)?)
    .with_retained_trace_limit(parse_usize_prefixed(parts[16], "t")?);

    if parts.len() >= 18 {
        if let Some(profile) = parse_kick_profile(parts[17])? {
            query = query.with_verified_kick_table_profile(profile);
        }
    }
    if parts.len() == 19 && parse_bool_digit_prefixed(parts[18], "u")? {
        query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
    }

    if completion_goal != PcCompletionGoal::ClearToEmpty {
        return Err(PcContinuationTokenError::new(
            "unsupported scenario completion goal",
        ));
    }

    Ok(query)
}
