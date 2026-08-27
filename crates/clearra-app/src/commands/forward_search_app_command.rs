use clearra_forward_search::{ForwardSearchMode, ForwardSearchQuery};

#[cfg(target_arch = "wasm32")]
use clearra_forward_search::{ForwardSearchError, ForwardSearchSession};

#[cfg(not(target_arch = "wasm32"))]
use crate::native_forward_execution::run_native_forward_search;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DamageAppCommand {
    query: ForwardSearchQuery,
}

impl DamageAppCommand {
    pub fn new(query: ForwardSearchQuery) -> Self {
        debug_assert!(query.mode().is_damage());
        Self { query }
    }

    pub const fn query(&self) -> &ForwardSearchQuery {
        &self.query
    }

    pub fn into_query(self) -> ForwardSearchQuery {
        self.query
    }
}

impl RunnableAppCommand for DamageAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        run_forward_search(self.query, context, ForwardResponseKind::Damage)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinFinderAppCommand {
    query: ForwardSearchQuery,
}

impl SpinFinderAppCommand {
    pub fn new(query: ForwardSearchQuery) -> Self {
        debug_assert!(matches!(query.mode(), ForwardSearchMode::SpinFinder(_)));
        Self { query }
    }

    pub const fn query(&self) -> &ForwardSearchQuery {
        &self.query
    }

    pub fn into_query(self) -> ForwardSearchQuery {
        self.query
    }
}

impl RunnableAppCommand for SpinFinderAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        run_forward_search(self.query, context, ForwardResponseKind::Spin)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenAppCommand {
    query: ForwardSearchQuery,
}

impl RenAppCommand {
    pub fn new(query: ForwardSearchQuery) -> Self {
        debug_assert!(matches!(query.mode(), ForwardSearchMode::MaximumRen));
        Self { query }
    }

    pub const fn query(&self) -> &ForwardSearchQuery {
        &self.query
    }

    pub fn into_query(self) -> ForwardSearchQuery {
        self.query
    }
}

impl RunnableAppCommand for RenAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        run_forward_search(self.query, context, ForwardResponseKind::Ren)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ForwardResponseKind {
    Damage,
    Spin,
    Ren,
}

fn run_forward_search(
    query: ForwardSearchQuery,
    context: &AppExecutionContext<'_>,
    response_kind: ForwardResponseKind,
) -> AppResponse {
    #[cfg(target_arch = "wasm32")]
    {
        let report = ForwardSearchSession::new(query)
            .and_then(|session| session.run_to_completion(context.execution_control));
        return match report {
            Ok(report) => forward_search_response(report, response_kind),
            Err(ForwardSearchError::Cancelled) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(AppErrorCode::ExecutionFailed, "forward search cancelled"),
            ),
            Err(error) => AppResponse::failed(
                AppStatus::ValidationFailed,
                AppError::new(
                    AppErrorCode::InvalidInput,
                    format!("invalid forward-search request: {error:?}"),
                ),
            ),
        };
    }
    #[cfg(not(target_arch = "wasm32"))]
    let report = run_native_forward_search(
        query,
        usize::from(context.resource_budget().workers()).max(1),
        context.execution_control,
    );
    #[cfg(not(target_arch = "wasm32"))]
    match report {
        Ok(report) => forward_search_response(report, response_kind),
        Err(error) if error.is_cancelled() => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, "forward search cancelled"),
        ),
        Err(error) if error.is_request_error() => AppResponse::failed(
            AppStatus::ValidationFailed,
            AppError::new(
                AppErrorCode::InvalidInput,
                format!(
                    "invalid forward-search request: {:?}",
                    error
                        .request_error()
                        .expect("request-error guard must retain the search error")
                ),
            ),
        ),
        Err(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("forward search failed: {}", error.reason()),
            ),
        ),
    }
}

pub(crate) fn forward_search_response(
    report: clearra_forward_search::ForwardSearchReport,
    response_kind: ForwardResponseKind,
) -> AppResponse {
    match response_kind {
        ForwardResponseKind::Damage => AppResponse::success(AppRenderModel::Damage(report)),
        ForwardResponseKind::Spin => AppResponse::success(AppRenderModel::SpinFinder(report)),
        ForwardResponseKind::Ren => AppResponse::success(AppRenderModel::Ren(report)),
    }
}
