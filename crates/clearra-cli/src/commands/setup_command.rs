use clearra_app::{AppCommand, AppRequest, SetupAppCommand};

use crate::{
    args::setup_args::SetupArgs,
    assemble::{setup_resource_budget, SetupQueryAssembler},
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupCommand;

impl SetupCommand {
    pub fn run(args: &SetupArgs, format: RenderFormat) -> CliOutput {
        let query = match SetupQueryAssembler::assemble(args) {
            Ok(query) => query,
            Err(error) => {
                return CliOutput::error(CliErrorCode::SetupQueryInvalid, format!("{error:?}"));
            }
        };
        let request = AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query)))
            .with_resource_budget(setup_resource_budget(args));
        AppResponseRenderer::render(
            crate::cli_routing::product_app_context().run(request),
            format,
            CliErrorCode::SetupQueryInvalid,
        )
    }
}

#[cfg(test)]
#[path = "setup_command_tests.rs"]
mod tests;
