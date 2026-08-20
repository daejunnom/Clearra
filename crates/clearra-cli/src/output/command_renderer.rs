use clearra_app::ProductBuildIdentity;
use clearra_output::{
    fumen_like::FumenLikeWriteError,
    model::{RenderField, RenderMessage},
    RenderFormat, RenderFormatDispatcher,
};

use crate::{error::CliErrorCode, output::CliOutput};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommandRenderer;

impl CommandRenderer {
    pub fn render<I>(
        kind: impl Into<String>,
        fields: I,
        format: RenderFormat,
    ) -> Result<String, FumenLikeWriteError>
    where
        I: IntoIterator<Item = RenderField>,
    {
        let mut message =
            RenderMessage::new(kind).with_runtime_identity(ProductBuildIdentity::current());
        for field in fields {
            message = message.with_value(field.key().to_owned(), field.value().clone());
        }
        RenderFormatDispatcher::render(&message, format)
    }
}

impl CommandRenderer {
    pub fn render_output<I>(kind: impl Into<String>, fields: I, format: RenderFormat) -> CliOutput
    where
        I: IntoIterator<Item = RenderField>,
    {
        match Self::render(kind, fields, format) {
            Ok(rendered) => CliOutput::success(rendered),
            Err(error) => CliOutput::error(CliErrorCode::CliOutputLimitExceeded, error.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "command_renderer_tests.rs"]
mod tests;
