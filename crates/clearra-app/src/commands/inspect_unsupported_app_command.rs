use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InspectUnsupportedAppCommand {
    command: String,
}

impl InspectUnsupportedAppCommand {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }
}

impl RunnableAppCommand for InspectUnsupportedAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        if self.command == "inspect" {
            return AppResponse::failed(
                AppStatus::Unsupported,
                AppError::new(
                    AppErrorCode::CliCommandUnsupported,
                    "inspect is unsupported; use rules inspect or scoring inspect for profile inspection",
                ),
            );
        }

        AppResponse::failed(
            AppStatus::Unsupported,
            AppError::new(
                AppErrorCode::CliCommandUnsupported,
                format!(
                    "command '{}' is outside the MVP1 executable CLI path",
                    self.command
                ),
            ),
        )
    }
}
