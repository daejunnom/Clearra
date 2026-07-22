use clearra_app::{AppCommand, AppContext, AppRequest, SetupAppCommand};

use crate::{
    args::setup_args::SetupArgs,
    assemble::SetupQueryAssembler,
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
        AppResponseRenderer::render(
            AppContext::default().run(AppRequest::new(AppCommand::Setup(SetupAppCommand::new(
                query,
            )))),
            format,
            CliErrorCode::SetupQueryInvalid,
        )
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "setup_command_tests.rs"]
mod tests;
