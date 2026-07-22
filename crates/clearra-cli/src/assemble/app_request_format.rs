use crate::output::RenderFormat;

pub(crate) fn target_render_format(target: &str) -> Option<RenderFormat> {
    match target {
        "text" => Some(RenderFormat::Text),
        "json" => Some(RenderFormat::Json),
        _ => None,
    }
}

pub(crate) fn default_format_name(format: RenderFormat) -> &'static str {
    match format {
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => "text",
        RenderFormat::Json => "json",
        RenderFormat::FumenLike => "fumen-like",
    }
}
