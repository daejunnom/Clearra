use clearra_output::RenderFormat;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderFormatSelector;

impl RenderFormatSelector {
    pub fn parse(value: Option<&str>) -> Result<RenderFormat, RenderFormatSelectionError> {
        match value.unwrap_or("text") {
            "text" => Ok(RenderFormat::Text),
            "json" => Ok(RenderFormat::Json),
            "fumen" | "fumen-like" => Ok(RenderFormat::FumenLike),
            value => Err(RenderFormatSelectionError::UnsupportedFormat {
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderFormatSelectionError {
    UnsupportedFormat { value: String },
}

#[cfg(test)]
#[path = "render_format_selector_tests.rs"]
mod tests;
