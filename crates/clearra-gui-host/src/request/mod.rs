mod backend_request_builder;
mod cover_request_builder;
#[cfg(test)]
mod gui_to_app_request;
mod output_request_builder;
mod pc_request_builder;
mod request_build_error;
mod scenario_request_builder;
mod setup_request_builder;

pub use backend_request_builder::BackendRequestBuilder;
pub use cover_request_builder::CoverRequestBuilder;
#[cfg(test)]
pub use gui_to_app_request::{GuiAppRequestBuild, GuiToAppRequest};
pub use output_request_builder::{GuiOutputRequestBuild, OutputRequestBuilder};
pub use pc_request_builder::PcRequestBuilder;
pub use request_build_error::{RequestBuildError, RequestBuildErrorCode};
pub use scenario_request_builder::ScenarioRequestBuilder;
pub use setup_request_builder::SetupRequestBuilder;

pub(crate) fn parse_piece_sequence(
    value: &str,
    field_name: &str,
) -> Result<clearra_supply::queue::fixed_sequence::FixedSequence, RequestBuildError> {
    let mut pieces = Vec::new();
    for (index, character) in value
        .chars()
        .filter(|character| !character.is_whitespace() && *character != ',')
        .enumerate()
    {
        let piece = clearra_core_domain::piece::piece_kind::PieceKind::from_ascii(character)
            .map_err(|_| {
                RequestBuildError::new(
                    RequestBuildErrorCode::UnknownPiece,
                    format!("unknown piece '{character}' at {field_name} index {index}"),
                )
            })?;
        pieces.push(piece);
    }

    Ok(clearra_supply::queue::fixed_sequence::FixedSequence::new(
        pieces,
    ))
}

pub(crate) fn score_objective_policy(
    mode: &str,
    score_profile: &str,
    spin_profile: &str,
    initial_b2b: u32,
    base: clearra_objectives::policy::objective_policy::ObjectivePolicy,
) -> Result<clearra_objectives::policy::objective_policy::ObjectivePolicy, RequestBuildError> {
    // Named GUI score products own the same all-solution objective as the
    // canonical CLI subcommands. The legacy scenario count field is inactive
    // for these products and must not leak back into objective selection.
    let base = if matches!(mode, "summary" | "score-finder") {
        clearra_objectives::policy::objective_policy::ObjectivePolicy::all()
    } else {
        base
    };
    let objective_kind = score_mode_objective_kind(mode, base.kind())?;
    let objective = match objective_kind {
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling => {
            return Ok(clearra_objectives::policy::objective_policy::ObjectivePolicy::tiling())
        }
        clearra_core_domain::objective::objective_kind::ObjectiveKind::MinimumCover => {
            let objective =
                clearra_objectives::policy::objective_policy::ObjectivePolicy::minimum_cover();
            if mode == "score-minimals" {
                objective.with_score_summary()
            } else {
                return Ok(objective);
            }
        }
        clearra_core_domain::objective::objective_kind::ObjectiveKind::All
        | clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique => {
            if matches!(mode, "summary" | "score-finder") {
                base.with_score_summary()
            } else {
                return Ok(base);
            }
        }
    };
    let score_profile =
        clearra_objectives::policy::score_objective_policy::ScoreProfileSelection::parse(
            score_profile,
        )
        .ok_or_else(|| {
            RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                format!("invalid GUI score profile '{score_profile}'"),
            )
        })?;
    let spin_profile =
        clearra_objectives::policy::score_objective_policy::SpinProfileSelection::parse(
            spin_profile,
        )
        .ok_or_else(|| {
            RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                format!("invalid GUI spin profile '{spin_profile}'"),
            )
        })?;
    Ok(objective
        .with_score_profile(score_profile)
        .with_spin_profile(spin_profile)
        .with_initial_b2b(initial_b2b))
}

pub(crate) fn score_mode_objective_kind(
    mode: &str,
    base_kind: clearra_core_domain::objective::objective_kind::ObjectiveKind,
) -> Result<clearra_core_domain::objective::objective_kind::ObjectiveKind, RequestBuildError> {
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;

    match mode {
        "off" | "disabled" | "failed-queue" | "saves" | "best-save" | "" | "summary"
        | "score-finder" => Ok(base_kind),
        "path" => Ok(ObjectiveKind::All),
        "tiling" | "tiling-only" => Ok(ObjectiveKind::Tiling),
        "minimum-cover" | "minimum" | "score-minimals" => Ok(ObjectiveKind::MinimumCover),
        value => Err(RequestBuildError::new(
            RequestBuildErrorCode::ValidationFailed,
            format!("invalid GUI score mode '{value}'"),
        )),
    }
}

pub(crate) fn execution_constraint_objective_policy(
    preserve_b2b: bool,
    spin_profile: &str,
    base: clearra_objectives::policy::objective_policy::ObjectivePolicy,
) -> Result<clearra_objectives::policy::objective_policy::ObjectivePolicy, RequestBuildError> {
    if !preserve_b2b {
        return Ok(base);
    }
    let spin_profile =
        clearra_objectives::policy::score_objective_policy::SpinProfileSelection::parse(
            spin_profile,
        )
        .ok_or_else(|| {
            RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                format!("invalid GUI spin profile '{spin_profile}'"),
            )
        })?;
    Ok(base.with_back_to_back_preservation(spin_profile))
}

pub(crate) fn parse_rule_profile(
    value: &str,
) -> Result<clearra_rules::profile::rule_profile::RuleProfile, RequestBuildError> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    clearra_rules::profile::rule_profile::RuleProfileId::parse(&normalized)
        .map(clearra_rules::profile::rule_profile::RuleProfile::new)
        .ok_or_else(|| {
            RequestBuildError::new(
                RequestBuildErrorCode::UnsupportedRule,
                format!("unsupported GUI rule '{value}'"),
            )
        })
}

pub(crate) fn parse_queue_pattern(
    value: &str,
    max_patterns: usize,
    field_name: &str,
) -> Result<
    clearra_supply::queue::queue_pattern_expression::QueuePatternExpression,
    RequestBuildError,
> {
    clearra_supply::queue::queue_pattern_expression::QueuePatternExpression::parse(
        value,
        max_patterns,
    )
    .map_err(|error| {
        RequestBuildError::new(
            RequestBuildErrorCode::ValidationFailed,
            format!("invalid {field_name}: {error}"),
        )
    })
}
