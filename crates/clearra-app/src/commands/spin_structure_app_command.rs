use clearra_spin_structure_search::{SpinStructureError, SpinStructureQuery};

#[cfg(target_arch = "wasm32")]
use clearra_spin_structure_search::SpinStructureSearcher;

#[cfg(not(target_arch = "wasm32"))]
use crate::native_spin_structure_execution::run_native_spin_structure_search;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureAppCommand {
    query: SpinStructureQuery,
}

impl SpinStructureAppCommand {
    pub const fn new(query: SpinStructureQuery) -> Self {
        Self { query }
    }

    pub const fn query(&self) -> &SpinStructureQuery {
        &self.query
    }

    pub fn into_query(self) -> SpinStructureQuery {
        self.query
    }
}

impl RunnableAppCommand for SpinStructureAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        #[cfg(target_arch = "wasm32")]
        let result = SpinStructureSearcher::run(self.query);
        #[cfg(not(target_arch = "wasm32"))]
        let result = run_native_spin_structure_search(
            self.query,
            usize::from(context.resource_budget().workers()).max(1),
            context.execution_control,
        );

        #[cfg(target_arch = "wasm32")]
        return render_result(result);
        #[cfg(not(target_arch = "wasm32"))]
        match result {
            Ok(report) => AppResponse::success(AppRenderModel::SpinStructure(report)),
            Err(error) if error.is_cancelled() => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "spin-structure search cancelled",
                ),
            ),
            Err(crate::native_spin_structure_execution::NativeSpinStructureError::Search(
                error,
            )) => render_error(error),
            Err(error) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("spin-structure search failed: {}", error.reason()),
                ),
            ),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn render_result(
    result: Result<clearra_spin_structure_search::SpinStructureReport, SpinStructureError>,
) -> AppResponse {
    match result {
        Ok(report) => AppResponse::success(AppRenderModel::SpinStructure(report)),
        Err(error) => render_error(error),
    }
}

fn render_error(error: SpinStructureError) -> AppResponse {
    AppResponse::failed(
        AppStatus::ValidationFailed,
        AppError::new(
            AppErrorCode::InvalidInput,
            format!("invalid spin-structure request: {error}"),
        ),
    )
}
