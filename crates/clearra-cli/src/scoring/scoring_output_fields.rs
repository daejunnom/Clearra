use clearra_scoring::profile::ScoreProfile;

use crate::output::{CliOutput, CommandRenderer, RenderFormat, SummaryRenderContract};

pub(crate) fn render_scoring(
    fields: Vec<(impl Into<String>, String)>,
    format: RenderFormat,
) -> CliOutput {
    CommandRenderer::render_output(
        "scoring",
        SummaryRenderContract::render_fields(fields),
        format,
    )
}

pub(crate) fn profile_fields(
    profile: &ScoreProfile,
    indexed: Option<usize>,
) -> Vec<(String, String)> {
    let prefix = indexed
        .map(|index| format!("profile_{index}_"))
        .unwrap_or_default();
    let combo = profile.combo_policy();
    let b2b = profile.b2b_policy();
    vec![
        (format!("{prefix}id"), profile.id().to_owned()),
        (
            format!("{prefix}display_name"),
            profile.display_name().to_owned(),
        ),
        (
            format!("{prefix}score_model"),
            profile.score_model().as_str().to_owned(),
        ),
        (
            format!("{prefix}attack_model"),
            profile.attack_model().as_str().to_owned(),
        ),
        (
            format!("{prefix}spin_rule"),
            profile.spin_rule().as_str().to_owned(),
        ),
        (
            format!("{prefix}accuracy_level"),
            profile.accuracy_level().as_str().to_owned(),
        ),
        (
            format!("{prefix}profile_specific_exact"),
            profile.profile_specific_exact().to_string(),
        ),
        (
            format!("{prefix}accuracy_reason"),
            profile.accuracy_reason().to_owned(),
        ),
        (
            format!("{prefix}combo_enabled"),
            combo.enabled().to_string(),
        ),
        (
            format!("{prefix}combo_score_bonus_per_combo"),
            combo.score_bonus_per_combo().to_string(),
        ),
        (
            format!("{prefix}combo_attack_bonus_per_combo"),
            combo.attack_bonus_per_combo().to_string(),
        ),
        (format!("{prefix}b2b_enabled"), b2b.enabled().to_string()),
        (
            format!("{prefix}b2b_score_bonus"),
            b2b.score_bonus().to_string(),
        ),
        (
            format!("{prefix}b2b_attack_bonus"),
            b2b.attack_bonus().to_string(),
        ),
    ]
}
