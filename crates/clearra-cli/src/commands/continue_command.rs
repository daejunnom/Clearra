use clearra_app::{AppCommand, AppContext, AppRequest, ContinueAppCommand};

use crate::{
    args::continue_args::ContinueArgs,
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContinueCommand;

impl ContinueCommand {
    pub fn run(args: &ContinueArgs, format: RenderFormat) -> CliOutput {
        let response = AppContext::default().run(AppRequest::new(AppCommand::Continue(
            ContinueAppCommand::new(args.token().map(ToOwned::to_owned)),
        )));
        let default_error = match response.error().map(|error| error.code()) {
            Some(clearra_app::AppErrorCode::MissingInput) => CliErrorCode::ContinueTokenRequired,
            Some(clearra_app::AppErrorCode::InvalidInput) => CliErrorCode::ContinueTokenInvalid,
            _ => CliErrorCode::ContinueSearchInternal,
        };
        AppResponseRenderer::render(response, format, default_error)
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "continue_command_tests.rs"]
mod tests;
