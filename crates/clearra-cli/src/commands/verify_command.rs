use crate::{
    args::verify_args::VerifyArgs,
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};
use clearra_app::{AppCommand, AppContext, AppRequest, VerifyAppCommand};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VerifyCommand;

impl VerifyCommand {
    pub fn run(args: &VerifyArgs, format: RenderFormat) -> CliOutput {
        let command = if matches!(args.target(), Some("kicks")) {
            AppCommand::VerifyKicks(VerifyAppCommand::kicks())
        } else {
            AppCommand::Verify(VerifyAppCommand::with_scope(
                args.target().map(ToOwned::to_owned),
            ))
        };
        let response = AppContext::default().run(AppRequest::new(command));
        let default_error = match response.error().map(|error| error.code()) {
            Some(clearra_app::AppErrorCode::VerifyTargetUnknown) => {
                CliErrorCode::VerifyTargetUnknown
            }
            Some(clearra_app::AppErrorCode::VerifyKicksFailed) => CliErrorCode::VerifyKicksFailed,
            _ => CliErrorCode::VerifyKicksFailed,
        };
        AppResponseRenderer::render(response, format, default_error)
    }
}

#[cfg(test)]
#[path = "verify_command_tests.rs"]
mod tests;
