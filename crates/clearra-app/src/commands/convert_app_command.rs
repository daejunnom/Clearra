use clearra_output::fumen_like::{FumenLikeReadError, FumenLikeReader};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{number_field, string_field},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConvertAppCommand {
    input: Option<String>,
    from: String,
    to: String,
}

impl ConvertAppCommand {
    pub fn new(direction: impl Into<String>) -> Self {
        let direction = direction.into();
        let (from, to) = direction
            .split_once("->")
            .map(|(from, to)| (from.trim().to_owned(), to.trim().to_owned()))
            .unwrap_or_else(|| ("fumen-like".to_owned(), direction));
        Self {
            input: None,
            from,
            to,
        }
    }
}
impl ConvertAppCommand {
    pub fn from_parts(
        input: Option<String>,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        Self {
            input,
            from: from.into(),
            to: to.into(),
        }
    }
}

impl RunnableAppCommand for ConvertAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        if !is_supported_source(&self.from) || !is_supported_target(&self.to) {
            return unsupported_direction(&self.from, &self.to);
        }

        let Some(input) = self.input else {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ConvertInputRequired,
                    "convert requires --input for MVP1",
                ),
            );
        };
        let trace = match FumenLikeReader::read(&input) {
            Ok(trace) => trace,
            Err(error) => return invalid_input(error),
        };

        let mut fields = vec![
            string_field("from", "fumen-like"),
            string_field("to", &self.to),
            number_field("page_count", trace.pages().len()),
        ];
        for (index, page) in trace.pages().iter().enumerate() {
            fields.push(string_field(format!("page_{index}"), page));
        }

        AppResponse::success(AppRenderModel::Convert(AppMessage::new(
            AppResultKind::Convert,
            fields,
        )))
    }
}

fn is_supported_source(source: &str) -> bool {
    matches!(source, "fumen-like" | "fumen")
}

fn is_supported_target(target: &str) -> bool {
    matches!(target, "text" | "json")
}

fn unsupported_direction(source: &str, target: &str) -> AppResponse {
    AppResponse::failed(
        AppStatus::Unsupported,
        AppError::new(
            AppErrorCode::ConvertDirectionUnsupported,
            format!(
                "convert supports fumen-like -> text/json only in MVP1 (from={source}, to={target})"
            ),
        ),
    )
}

fn invalid_input(error: FumenLikeReadError) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::ConvertInputInvalid,
            format!("failed to read fumen-like input: {error:?}"),
        ),
    )
}
