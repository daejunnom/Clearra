use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_spin_structure_search::{
    analyze_spin_structure_coverage, guaranteed_spin_structure_family, SpinStructureError,
    SpinStructureQuery, SpinStructureSearcher, DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::native_spin_structure_execution::run_native_spin_structure_search;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    ranked_family_product_projection::{
        project_spin_structure_family, project_spin_structure_guaranteed_family,
    },
    render::AppRenderModel,
    spin_structure_coverage_result::{page_source, project_spin_structure_coverage},
    spin_structure_search_result::SpinStructureSearchResult,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinStructureProductMode {
    #[default]
    Search,
    Cover {
        max_patterns: usize,
    },
    Guaranteed {
        final_piece: PieceKind,
        max_patterns: usize,
        dependency_report: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureAppCommand {
    query: SpinStructureQuery,
    product_mode: SpinStructureProductMode,
}

impl SpinStructureAppCommand {
    pub const fn new(query: SpinStructureQuery) -> Self {
        Self {
            query,
            product_mode: SpinStructureProductMode::Search,
        }
    }

    pub const fn cover(query: SpinStructureQuery, max_patterns: usize) -> Self {
        Self {
            query,
            product_mode: SpinStructureProductMode::Cover { max_patterns },
        }
    }

    pub const fn guaranteed(
        query: SpinStructureQuery,
        final_piece: PieceKind,
        max_patterns: usize,
        dependency_report: bool,
    ) -> Self {
        Self {
            query,
            product_mode: SpinStructureProductMode::Guaranteed {
                final_piece,
                max_patterns,
                dependency_report,
            },
        }
    }

    pub const fn query(&self) -> &SpinStructureQuery {
        &self.query
    }

    pub const fn product_mode(&self) -> SpinStructureProductMode {
        self.product_mode
    }

    pub fn into_query(self) -> SpinStructureQuery {
        self.query
    }
}

impl RunnableAppCommand for SpinStructureAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let product_mode = self.product_mode;
        let query = match SpinStructureSearcher::normalize_query(self.query) {
            Ok(query) => query,
            Err(error) => return render_error(error),
        };
        // The browser path is currently a synchronous in-worker search and has
        // no cooperative-control entry point; retain the shared command seam.
        #[cfg(target_arch = "wasm32")]
        let _ = context;
        #[cfg(target_arch = "wasm32")]
        let result = SpinStructureSearcher::run(query.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let result = run_native_spin_structure_search(
            query.clone(),
            usize::from(context.resource_budget().workers()).max(1),
            context.execution_control,
        );

        #[cfg(target_arch = "wasm32")]
        return render_result(&query, product_mode, result);
        #[cfg(not(target_arch = "wasm32"))]
        match result {
            Ok(report) => promote_result(&query, product_mode, report),
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
    query: &SpinStructureQuery,
    product_mode: SpinStructureProductMode,
    result: Result<clearra_spin_structure_search::SpinStructureReport, SpinStructureError>,
) -> AppResponse {
    match result {
        Ok(report) => promote_result(query, product_mode, report),
        Err(error) => render_error(error),
    }
}

fn promote_result(
    query: &SpinStructureQuery,
    product_mode: SpinStructureProductMode,
    report: clearra_spin_structure_search::SpinStructureReport,
) -> AppResponse {
    match product_mode {
        SpinStructureProductMode::Search => promote_search_result(query, report),
        SpinStructureProductMode::Cover { max_patterns } => {
            promote_coverage_result(query, report, max_patterns)
        }
        SpinStructureProductMode::Guaranteed {
            final_piece,
            max_patterns,
            dependency_report,
        } => promote_guaranteed_result(query, report, final_piece, max_patterns, dependency_report),
    }
}

fn promote_search_result(
    query: &SpinStructureQuery,
    report: clearra_spin_structure_search::SpinStructureReport,
) -> AppResponse {
    match SpinStructureSearchResult::promote(query, report) {
        Ok(result) => match project_spin_structure_family(&result) {
            Ok(payload) => {
                AppResponse::success(AppRenderModel::SpinStructure(result.into_report()))
                    .with_public_product_result(payload, None)
            }
            Err(error) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("spin-structure Host projection rejected: {error:?}"),
                ),
            ),
        },
        Err(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("spin-structure ranked-family result rejected: {error:?}"),
            ),
        ),
    }
}

fn promote_coverage_result(
    query: &SpinStructureQuery,
    report: clearra_spin_structure_search::SpinStructureReport,
    max_patterns: usize,
) -> AppResponse {
    let coverage = match analyze_spin_structure_coverage(query, &report, max_patterns) {
        Ok(coverage) => coverage,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("spin-structure exact coverage failed: {error:?}"),
                ),
            );
        }
    };
    let result = match SpinStructureSearchResult::promote(query, report) {
        Ok(result) => result,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("spin-structure coverage source rejected: {error:?}"),
                ),
            );
        }
    };
    let projected = match project_spin_structure_coverage(&result, &coverage) {
        Ok(projected) => projected,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(AppErrorCode::ExecutionFailed, error),
            );
        }
    };
    let report = result.into_report();
    AppResponse::success(AppRenderModel::SpinStructure(report))
        .with_public_product_result(projected.payload, Some(page_source(projected.owner)))
}

fn promote_guaranteed_result(
    query: &SpinStructureQuery,
    report: clearra_spin_structure_search::SpinStructureReport,
    final_piece: PieceKind,
    max_patterns: usize,
    dependency_report: bool,
) -> AppResponse {
    let report = match guaranteed_spin_structure_family(query, &report, final_piece, max_patterns) {
        Ok(report) => report,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("spin-structure guaranteed analysis failed: {error:?}"),
                ),
            );
        }
    };
    match SpinStructureSearchResult::promote(query, report) {
        Ok(result) => {
            match project_spin_structure_guaranteed_family(&result, final_piece, dependency_report)
            {
                Ok(payload) => {
                    AppResponse::success(AppRenderModel::SpinStructure(result.into_report()))
                        .with_public_product_result(payload, None)
                }
                Err(error) => AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::ExecutionFailed,
                        format!("spin-structure guaranteed Host projection rejected: {error:?}"),
                    ),
                ),
            }
        }
        Err(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("spin-structure guaranteed family rejected: {error:?}"),
            ),
        ),
    }
}

impl SpinStructureProductMode {
    pub const fn default_max_patterns() -> usize {
        DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS
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
