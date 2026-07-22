use clearra_app::{AppCommand, AppContext, AppRequest, CoverAppCommand};

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
            AppContext::default().run(AppRequest::new(AppCommand::Cover(
                CoverAppCommand::new(query).with_export_template_json(args.export_template_json()),
            ))),
            format,
            CliErrorCode::CoverQueryInvalid,
        )
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "cover_command_tests.rs"]
mod tests;
