use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_rules::profile::rule_profile::{RuleProfile, RuleProfileId};

use super::{
    continuation_token_error::PcContinuationTokenError, pc_hold_policy::PcHoldPolicy,
    pc_scenario_query::PcCompletionGoal, PcCountPolicy,
};

pub(crate) fn parse_target(part: &str) -> Result<PcTarget, PcContinuationTokenError> {
    match part {
        "l2" => Ok(PcTarget::two_lines()),
        "l4" => Ok(PcTarget::four_lines()),
        "l6" => Ok(PcTarget::six_lines()),
        _ => Err(PcContinuationTokenError::new(format!(
            "unsupported continuation target '{part}'"
        ))),
    }
}

pub(crate) fn parse_hold_piece(part: &str) -> Result<Option<PieceKind>, PcContinuationTokenError> {
    let value = prefixed_value(part, "h")?;
    if value == "none" {
        return Ok(None);
    }
    let mut chars = value.chars();
    let Some(piece) = chars.next() else {
        return Err(PcContinuationTokenError::new(
            "empty continuation hold piece",
        ));
    };
    if chars.next().is_some() {
        return Err(PcContinuationTokenError::new(format!(
            "invalid continuation hold piece '{value}'"
        )));
    }
    PieceKind::from_ascii(piece).map(Some).map_err(|_| {
        PcContinuationTokenError::new(format!("unknown continuation hold piece '{value}'"))
    })
}

pub(crate) fn parse_queue(part: &str) -> Result<Vec<PieceKind>, PcContinuationTokenError> {
    let value = prefixed_value(part, "q")?;
    if value.is_empty() {
        return Err(PcContinuationTokenError::new(
            "continuation queue must not be empty",
        ));
    }
    value
        .chars()
        .enumerate()
        .map(|(index, piece)| {
            PieceKind::from_ascii(piece).map_err(|_| {
                PcContinuationTokenError::new(format!(
                    "unknown continuation queue piece '{piece}' at index {index}"
                ))
            })
        })
        .collect()
}

pub(crate) fn parse_rule_profile(value: &str) -> Result<RuleProfile, PcContinuationTokenError> {
    let id = if value == "srs-90" {
        Some(RuleProfileId::Srs)
    } else {
        RuleProfileId::parse(value)
    }
    .ok_or_else(|| PcContinuationTokenError::new(format!("unsupported rule profile '{value}'")))?;
    Ok(RuleProfile::new(id))
}

pub(crate) fn parse_objective(value: &str) -> Result<ObjectivePolicy, PcContinuationTokenError> {
    match value {
        "all" => Ok(ObjectivePolicy::all()),
        "unique" => Ok(ObjectivePolicy::unique()),
        "min-cover" | "minimum-cover" => Ok(ObjectivePolicy::minimum_cover()),
        _ => Err(PcContinuationTokenError::new(format!(
            "unsupported objective '{value}'"
        ))),
    }
}

pub(crate) fn parse_count_policy(value: &str) -> Result<PcCountPolicy, PcContinuationTokenError> {
    match value {
        "first-solution" | "first" => Ok(PcCountPolicy::FirstSolution),
        "count-all" | "all" => Ok(PcCountPolicy::CountAll),
        "count-unique" | "unique" => Ok(PcCountPolicy::CountUnique),
        _ => Err(PcContinuationTokenError::new(format!(
            "unsupported count policy '{value}'"
        ))),
    }
}

pub(crate) fn parse_completion_goal(
    value: &str,
) -> Result<PcCompletionGoal, PcContinuationTokenError> {
    match value {
        "clear-to-empty" => Ok(PcCompletionGoal::ClearToEmpty),
        _ => Err(PcContinuationTokenError::new(format!(
            "unsupported completion goal '{value}'"
        ))),
    }
}

pub(crate) fn parse_u16_prefixed(
    part: &str,
    prefix: &str,
) -> Result<u16, PcContinuationTokenError> {
    let value = prefixed_value(part, prefix)?;
    value.parse::<u16>().map_err(|_| {
        PcContinuationTokenError::new(format!("invalid continuation integer '{value}'"))
    })
}

pub(crate) fn parse_usize_prefixed(
    part: &str,
    prefix: &str,
) -> Result<usize, PcContinuationTokenError> {
    let value = prefixed_value(part, prefix)?;
    value.parse::<usize>().map_err(|_| {
        PcContinuationTokenError::new(format!("invalid continuation integer '{value}'"))
    })
}

pub(crate) fn parse_optional_usize_prefixed(
    part: &str,
    prefix: &str,
) -> Result<Option<usize>, PcContinuationTokenError> {
    let value = prefixed_value(part, prefix)?;
    if value == "none" {
        return Ok(None);
    }
    value.parse::<usize>().map(Some).map_err(|_| {
        PcContinuationTokenError::new(format!("invalid continuation integer '{value}'"))
    })
}

pub(crate) fn parse_bool_digit_prefixed(
    part: &str,
    prefix: &str,
) -> Result<bool, PcContinuationTokenError> {
    match prefixed_value(part, prefix)? {
        "0" => Ok(false),
        "1" => Ok(true),
        value => Err(PcContinuationTokenError::new(format!(
            "invalid continuation bool digit '{value}'"
        ))),
    }
}

pub(crate) fn parse_mask_prefixed(part: &str) -> Result<u64, PcContinuationTokenError> {
    let value = prefixed_value(part, "m")?;
    let digits = value.strip_prefix("0x").ok_or_else(|| {
        PcContinuationTokenError::new("scenario continuation mask must start with 0x")
    })?;
    u64::from_str_radix(digits, 16)
        .map_err(|error| PcContinuationTokenError::new(error.to_string()))
}

pub(crate) fn prefixed_value<'a>(
    part: &'a str,
    prefix: &str,
) -> Result<&'a str, PcContinuationTokenError> {
    part.strip_prefix(prefix).ok_or_else(|| {
        PcContinuationTokenError::new(format!("invalid continuation field '{part}'"))
    })
}

pub(crate) fn require_value(
    value: &str,
    expected: &str,
    label: &str,
) -> Result<(), PcContinuationTokenError> {
    if value == expected {
        return Ok(());
    }
    Err(PcContinuationTokenError::new(format!(
        "{label} must be '{expected}', got '{value}'"
    )))
}

pub(crate) fn opening_hold_policy(
    hold_enabled: bool,
    hold_piece: Option<PieceKind>,
) -> PcHoldPolicy {
    if !hold_enabled {
        return PcHoldPolicy::Disabled;
    }
    hold_piece
        .map(PcHoldPolicy::EnabledWithPiece)
        .unwrap_or(PcHoldPolicy::EnabledEmpty)
}

pub(crate) fn objective_name(objective: ObjectivePolicy) -> &'static str {
    match objective.kind() {
        ObjectiveKind::All => "all",
        ObjectiveKind::Unique => "unique",
        ObjectiveKind::MinimumCover => "min-cover",
    }
}

pub(crate) fn count_policy_name(count_policy: PcCountPolicy) -> &'static str {
    match count_policy {
        PcCountPolicy::FirstSolution => "first-solution",
        PcCountPolicy::CountAll => "count-all",
        PcCountPolicy::CountUnique => "count-unique",
    }
}

pub(crate) fn format_optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

pub(crate) fn bool_digit(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

pub(crate) fn format_piece_sequence(pieces: &[PieceKind]) -> String {
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}

pub(crate) fn format_hold_piece(piece: Option<PieceKind>) -> String {
    piece
        .map(|piece| piece.as_ascii().to_string())
        .unwrap_or_else(|| "none".to_owned())
}
