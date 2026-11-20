use clearra_app::{AppCommand, AppRequest, CoverAppCommand};

use crate::{
    args::cover_args::CoverArgs,
    assemble::CoverQueryAssembler,
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoverCommand;

impl CoverCommand {
    pub fn run(args: &CoverArgs, format: RenderFormat) -> CliOutput {
        let query = match CoverQueryAssembler::assemble(args) {
            Ok(query) => query,
            Err(error) => {
                return CliOutput::error(CliErrorCode::CoverQueryInvalid, error.to_string())
            }
        };
        AppResponseRenderer::render(
            crate::cli_routing::product_app_context().run(AppRequest::new(AppCommand::Cover(
                CoverAppCommand::new(query).with_export_template_json(args.export_template_json()),
            ))),
            format,
            CliErrorCode::CoverQueryInvalid,
        )
    }
}

#[cfg(test)]
#[path = "cover_command_tests.rs"]
mod tests;
