use clearra_output::fumen_like::{FumenLikeReadError, FumenLikeReader};

use crate::{
    args::convert_args::ConvertArgs,
    error::CliErrorCode,
    output::{CliOutput, CommandRenderer, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConvertCommand;

impl ConvertCommand {
    pub fn run(args: &ConvertArgs, default_format: RenderFormat) -> CliOutput {
        let source = args.from().unwrap_or("fumen-like");
        if !is_supported_source(source) {
            return unsupported_direction(
                source,
                args.to().unwrap_or(default_format_name(default_format)),
            );
        }

        let target = match target_format(args.to(), default_format) {
            Ok(target) => target,
            Err(output) => return output,
        };

        let Some(input) = args.input() else {
            return CliOutput::error(
                CliErrorCode::ConvertInputRequired,
                "convert requires --input for MVP1",
            );
        };

        let trace = match FumenLikeReader::read(input) {
            Ok(trace) => trace,
            Err(error) => return invalid_input(error),
        };

        let mut fields = vec![
            ("from".to_owned(), "fumen-like".to_owned()),
            ("to".to_owned(), default_format_name(target).to_owned()),
            ("page_count".to_owned(), trace.pages().len().to_string()),
        ];
        for (index, page) in trace.pages().iter().enumerate() {
            fields.push((format!("page_{index}"), page.clone()));
        }

        CliOutput::success(CommandRenderer::render(
            "convert",
            crate::output::SummaryRenderContract::render_fields(fields),
            target,
        ))
    }
}

fn is_supported_source(source: &str) -> bool {
    matches!(source, "fumen-like" | "fumen")
}

fn target_format(
    target: Option<&str>,
    default_format: RenderFormat,
) -> Result<RenderFormat, CliOutput> {
    match target {
        Some("text") => Ok(RenderFormat::Text),
        Some("json") => Ok(RenderFormat::Json),
        Some(value) => Err(unsupported_direction("fumen-like", value)),
        None => match default_format {
            RenderFormat::Text
            | RenderFormat::TextVerbose
            | RenderFormat::TextDiagnostics
            | RenderFormat::Json => Ok(default_format),
            RenderFormat::FumenLike => Err(unsupported_direction("fumen-like", "fumen-like")),
        },
    }
}

fn unsupported_direction(source: &str, target: &str) -> CliOutput {
    CliOutput::error(
        CliErrorCode::ConvertDirectionUnsupported,
        format!(
            "convert supports fumen-like -> text/json only in MVP1 (from={source}, to={target})"
        ),
    )
}

fn invalid_input(error: FumenLikeReadError) -> CliOutput {
    CliOutput::error(
        CliErrorCode::ConvertInputInvalid,
        format!("failed to read fumen-like input: {error:?}"),
    )
}

fn default_format_name(format: RenderFormat) -> &'static str {
    match format {
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => "text",
        RenderFormat::Json => "json",
        RenderFormat::FumenLike => "fumen-like",
    }
}

#[cfg(test)]
#[path = "convert_command_tests.rs"]
mod tests;
