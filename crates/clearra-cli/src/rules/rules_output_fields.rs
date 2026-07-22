use clearra_rules::kicks::{
    KickProfileCapability, KickTableProfile, KickTableProfileId, NoKick, SrsKicks,
};

use crate::output::{CliOutput, CommandRenderer, RenderFormat, SummaryRenderContract};

pub(crate) fn render_rules(
    fields: Vec<(impl Into<String>, String)>,
    format: RenderFormat,
) -> CliOutput {
    CliOutput::success(CommandRenderer::render(
        "rules",
        SummaryRenderContract::render_fields(fields),
        format,
    ))
}

pub(crate) fn capability_fields(
    prefix: &str,
    capability: KickProfileCapability,
) -> Vec<(String, String)> {
    let mut fields = vec![
        (
            format!("{prefix}supports_180"),
            capability.supports_180().to_string(),
        ),
        (
            format!("{prefix}supports_exact_180"),
            capability.supports_exact_180().to_string(),
        ),
        (
            format!("{prefix}search_backend_supported"),
            capability.search_backend_supported().to_string(),
        ),
        (
            format!("{prefix}c_compact_descriptor_ready"),
            capability.c_compact_descriptor_ready().to_string(),
        ),
        (
            format!("{prefix}unsupported_backend_reason"),
            capability.unsupported_reason().unwrap_or("none").to_owned(),
        ),
    ];
    if let Some(reason) = capability.unsupported_reason() {
        fields.push((format!("{prefix}unsupported_reason"), reason.to_owned()));
    }
    fields
}

pub(crate) fn builtin_kick_profile(profile_id: &str) -> Option<KickTableProfile> {
    match KickTableProfileId::parse(profile_id)? {
        KickTableProfileId::Srs90 => Some(SrsKicks::profile()),
        KickTableProfileId::SrsPlus => Some(SrsKicks::srs_plus_profile()),
        KickTableProfileId::NoKick => Some(NoKick::profile()),
        KickTableProfileId::SrsX
        | KickTableProfileId::Asc
        | KickTableProfileId::Ars
        | KickTableProfileId::Imported
        | KickTableProfileId::Custom => None,
    }
}
