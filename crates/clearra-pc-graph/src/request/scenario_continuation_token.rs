use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_profiles::{
    bag::bag_profile::BagProfileId, pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

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
    PcCountPolicy, PcSolutionProbabilityPolicy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScenarioContinuationTokenKind {
    Continuation,
    Replay,
}

impl ScenarioContinuationTokenKind {
    fn prefix(self, extended: bool) -> &'static str {
        match (self, extended) {
            (Self::Continuation, false) => "sc2",
            (Self::Continuation, true) => "sc3",
            (Self::Replay, false) => "sr2",
            (Self::Replay, true) => "sr3",
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

pub(crate) fn parse_scenario_continuation(
    token: &str,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    parse_scenario(token, ScenarioContinuationTokenKind::Continuation)
}

pub(crate) fn parse_scenario_replay(
    token: &str,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    parse_scenario(token, ScenarioContinuationTokenKind::Replay)
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
    let board = query.initial_board();
    let extended = board.has_extended_occupancy();
    let prefix = kind.prefix(extended);
    let board_mask = if extended {
        // A token replays scenario semantics, not execution preferences. Do
        // not offer a full-height token for a query with additional search
        // constraints this wire format cannot reproduce.
        let objective = match query.count_policy() {
            PcCountPolicy::CountUnique => ObjectivePolicy::unique(),
            PcCountPolicy::FirstSolution | PcCountPolicy::CountAll => ObjectivePolicy::all(),
        };
        if query.objective() != objective
            || query.supply_window_size().is_some()
            || query.allowed_colored_solution_identities().is_some()
        {
            return Err(PcContinuationTokenError::new(
                "full-height scenario token cannot preserve this scenario's search constraints",
            ));
        }
        let [word0, word1, word2, word3] = board.occupied_words();
        format!("m0x{word0:016x},0x{word1:016x},0x{word2:016x},0x{word3:016x}")
    } else {
        format!("m0x{:016x}", board.occupied_mask())
    };
    let kick_profile = format_kick_profile(query.verified_kick_profile())?;
    Ok(format!(
        "{prefix}:w{}:v{}:{board_mask}:ps{}:bg{}:r{}:h{}:q{}:p{}:x{}:n{}:a{}:z{}:g{}:c{}:t{}:k{}:u{}:o{}",
        board.width(),
        board.visible_height(),
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
        query.queue_observation_policy().keyword(),
    ))
}

fn parse_scenario(
    token: &str,
    expected_kind: ScenarioContinuationTokenKind,
) -> Result<PcScenarioQuery, PcContinuationTokenError> {
    let parts = token.split(':').collect::<Vec<_>>();
    let extended = parts[0] == expected_kind.prefix(true);
    let min_fields = if extended { 20 } else { 17 };
    if !(min_fields..=20).contains(&parts.len()) || parts[0] != expected_kind.prefix(extended) {
        return Err(PcContinuationTokenError::new(
            "invalid scenario token version or field count (expected sc2|sc3 or sr2|sr3)",
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

    let width = parse_u16_prefixed(parts[1], "w")?;
    let height = parse_u16_prefixed(parts[2], "v")?;
    let board = if extended {
        parse_extended_scenario_board(width, height, parts[3])?
    } else {
        PcScenarioBoard::new(width, height, parse_mask_prefixed(parts[3])?)
    };
    let mut query = PcScenarioQuery::new(
        board,
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
    if parts.len() >= 19 && parse_bool_digit_prefixed(parts[18], "u")? {
        query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
    }
    if parts.len() == 20 {
        let value = prefixed_value(parts[19], "o")?;
        let policy = QueueObservationPolicy::from_keyword(value).ok_or_else(|| {
            PcContinuationTokenError::new("unsupported scenario queue observation policy")
        })?;
        query = query.with_queue_observation_policy(policy);
    }

    if completion_goal != PcCompletionGoal::ClearToEmpty {
        return Err(PcContinuationTokenError::new(
            "unsupported scenario completion goal",
        ));
    }

    Ok(query)
}

fn parse_extended_scenario_board(
    width: u16,
    height: u16,
    part: &str,
) -> Result<PcScenarioBoard, PcContinuationTokenError> {
    let invalid_words = || {
        PcContinuationTokenError::new("full-height scenario mask requires four 16-digit hex words")
    };
    let mut parts = prefixed_value(part, "m")?.split(',');
    let mut words = [0; 4];
    for word in &mut words {
        let digits = parts
            .next()
            .and_then(|value| value.strip_prefix("0x"))
            .filter(|digits| {
                digits.len() == 16 && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .ok_or_else(invalid_words)?;
        *word = u64::from_str_radix(digits, 16).map_err(|_| invalid_words())?;
    }
    if parts.next().is_some() {
        return Err(invalid_words());
    }
    if width != 10 || matches!(words, [_, 0, 0, 0]) {
        return Err(PcContinuationTokenError::new(
            "full-height scenario mask requires a standard-width board with high occupancy",
        ));
    }
    PcScenarioBoard::standard_10_from_words(height, words).map_err(|_| {
        PcContinuationTokenError::new(
            "full-height scenario occupancy does not fit the declared field",
        )
    })
}
