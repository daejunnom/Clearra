use std::sync::Arc;

use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;
use clearra_validation::validators::pc_query_validator::validate_opening_pc_search_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    pc_allspin_result::project_pc_allspin_result,
    pc_chance_probability_result::PcChanceCompiledAuthority,
    pc_result_projection::{
        validate_opening_pc_result_projection, PcResultProjection, ValidatedPcResultProjection,
    },
    pc_save_result::{PcSaveCompiledAuthority, PcSaveCompiledAuthorityError},
    pc_score_summary_result::{PcScoreCompiledAuthority, PcScoreCompiledAuthorityError},
    pc_tiling_family_result::{PcTilingCompiledAuthority, PcTilingCompiledAuthorityError},
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcAppCommand {
    query: Arc<OpeningPcSearchQuery>,
    result_projection: PcResultProjection,
}

impl PcAppCommand {
    pub fn new(query: OpeningPcSearchQuery) -> Self {
        Self {
            query: Arc::new(query),
            result_projection: PcResultProjection::Standard,
        }
    }

    pub const fn with_result_projection(mut self, result_projection: PcResultProjection) -> Self {
        self.result_projection = result_projection;
        self
    }

    pub const fn with_score_minimals_result(self) -> Self {
        self.with_result_projection(PcResultProjection::pc_score_minimals())
    }
}
impl PcAppCommand {
    pub fn query(&self) -> &OpeningPcSearchQuery {
        self.query.as_ref()
    }

    pub(crate) fn query_arc(&self) -> Arc<OpeningPcSearchQuery> {
        Arc::clone(&self.query)
    }

    pub const fn result_projection(&self) -> PcResultProjection {
        self.result_projection
    }

    pub const fn score_minimals_requested(&self) -> bool {
        self.result_projection.score_minimals_origin().is_some()
    }

    /// Validates that the independently constructible projection belongs to
    /// this query. Text parsing is not an authority boundary for typed callers.
    pub fn validate_result_projection(&self) -> Result<(), &'static str> {
        self.validated_result_projection().map(|_| ())
    }

    pub(crate) fn validated_result_projection(
        &self,
    ) -> Result<ValidatedPcResultProjection, &'static str> {
        validate_opening_pc_result_projection(&self.query, self.result_projection)
    }

    pub(crate) fn into_validated_search_parts(
        self,
    ) -> Result<(Arc<OpeningPcSearchQuery>, ValidatedPcResultProjection), &'static str> {
        let result_projection = self.validated_result_projection()?;
        Ok((self.query, result_projection))
    }
}

impl RunnableAppCommand for PcAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let result_projection = match self.validated_result_projection() {
            Ok(result_projection) => result_projection,
            Err(reason) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(AppErrorCode::InvalidInput, reason),
                )
            }
        };
        let report = validate_opening_pc_search_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let chance_origin = result_projection.projection().chance_origin();
        let minimals_origin = result_projection.projection().minimals_origin();
        let path_origin = result_projection.projection().path_origin();
        let score_origin = result_projection.projection().score_origin();
        let score_minimals_origin = result_projection.projection().score_minimals_origin();
        if let Some(origin) = result_projection.projection().tiling_origin() {
            let wasm_terminal = context
                .services()
                .core_executor()
                .supports_cooperative_wasm_search();
            let compiled_authority = if wasm_terminal {
                PcTilingCompiledAuthority::compile_opening_under_terminal_authority(
                    Arc::clone(&self.query),
                    origin,
                )
            } else {
                PcTilingCompiledAuthority::compile_opening(Arc::clone(&self.query), origin)
            };
            let authority = match compiled_authority {
                Ok(authority) => authority,
                Err(PcTilingCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            resource_report,
                        ),
                    )
                }
                Err(PcTilingCompiledAuthorityError::ProblemCompile(error)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                    )
                }
                Err(PcTilingCompiledAuthorityError::Contract(reason)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ExecutionFailed, reason),
                    )
                }
            };
            let execution = if wasm_terminal {
                context
                    .services()
                    .core_executor()
                    .execute_pc_tiling_with_control(
                        &authority,
                        context.pc_tiling_external_retained_context_bytes(),
                        context.execution_control(),
                    )
            } else {
                context
                    .services()
                    .core_executor()
                    .execute_native_pc_tiling_with_control(&authority, context.execution_control())
            };
            return match execution {
                Ok(result) => {
                    match authority.validate_execution_result(authority.problem(), &result) {
                        Ok(evidence) => AppResponse::success(AppRenderModel::Pc(result))
                            .with_pc_tiling_execution_evidence(evidence),
                        Err(error) => AppResponse::failed(
                            AppStatus::ExecutionFailed,
                            AppError::new(
                                AppErrorCode::ExecutionFailed,
                                format!("pc tiling result rejected: {error}"),
                            ),
                        ),
                    }
                }
                Err(error) => core_execution_error_response(error),
            };
        }
        if let Some(origin) = result_projection.projection().save_origin() {
            let authority =
                match PcSaveCompiledAuthority::compile_opening(Arc::clone(&self.query), origin) {
                    Ok(authority) => authority,
                    Err(PcSaveCompiledAuthorityError::ProblemCompile(error)) => {
                        return AppResponse::failed(
                            AppStatus::ExecutionFailed,
                            AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                        )
                    }
                    Err(PcSaveCompiledAuthorityError::Contract(error)) => {
                        return AppResponse::failed(
                            AppStatus::ExecutionFailed,
                            AppError::new(
                                AppErrorCode::ExecutionFailed,
                                format!("pc save compiled authority rejected: {error}"),
                            ),
                        )
                    }
                };
            let problem = authority.problem_arc();
            let execution = context
                .services()
                .core_executor()
                .execute_with_control(problem.as_ref(), context.execution_control());
            return match execution {
                Ok(result) => match authority.validate_execution_result(&problem, &result) {
                    Ok(evidence) => AppResponse::success(AppRenderModel::Pc(result))
                        .with_pc_save_execution_evidence(evidence),
                    Err(error) => AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            format!("pc save result rejected: {error}"),
                        ),
                    ),
                },
                Err(error) => core_execution_error_response(error),
            };
        }
        if let Some(origin) = score_minimals_origin {
            let authority = match PcScoreCompiledAuthority::compile_score_minimals_opening(
                Arc::clone(&self.query),
                origin,
            ) {
                Ok(authority) => authority,
                Err(PcScoreCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            resource_report,
                        ),
                    )
                }
                Err(PcScoreCompiledAuthorityError::ProblemCompile(error)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                    )
                }
                Err(PcScoreCompiledAuthorityError::Contract(error)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            format!("pc score-minimals compiled authority rejected: {error}"),
                        ),
                    )
                }
            };
            let execution = context
                .services()
                .core_executor()
                .execute_pc_score_minimals_with_control(
                    &authority,
                    context.pc_score_external_retained_context_bytes(),
                    context.execution_control(),
                );
            drop(authority);
            return match execution {
                Ok((result, evidence)) => AppResponse::success(AppRenderModel::Pc(
                    project_pc_allspin_result(result, result_projection),
                ))
                .with_pc_score_portfolio_execution_evidence(evidence),
                Err(error) => core_execution_error_response(error),
            };
        }
        if let Some(origin) = score_origin {
            let authority = match PcScoreCompiledAuthority::compile_opening(self.query, origin) {
                Ok(authority) => authority,
                Err(PcScoreCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            resource_report,
                        ),
                    )
                }
                Err(PcScoreCompiledAuthorityError::ProblemCompile(error)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                    )
                }
                Err(PcScoreCompiledAuthorityError::Contract(error)) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            format!("pc score compiled authority rejected: {error}"),
                        ),
                    )
                }
            };
            let execution = context
                .services()
                .core_executor()
                .execute_pc_score_with_control(
                    &authority,
                    context.pc_score_external_retained_context_bytes(),
                    context.execution_control(),
                );
            drop(authority);
            return match execution {
                Ok((result, evidence)) => AppResponse::success(AppRenderModel::Pc(
                    project_pc_allspin_result(result, result_projection),
                ))
                .with_pc_score_execution_evidence(evidence),
                Err(error) => core_execution_error_response(error),
            };
        }
        let compiled = match chance_origin {
            Some(_) => ProblemCompiler::compile_opening_percent(self.query.as_ref()),
            None => ProblemCompiler::compile_opening_pc(self.query.as_ref()),
        };
        let problem = match compiled {
            Ok(problem) => problem,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                )
            }
        };
        let problem = if chance_origin.is_some() {
            problem.with_pc_chance_probability_v2_evidence()
        } else if minimals_origin.is_some() {
            problem.with_pc_minimum_cover_v2_evidence()
        } else if path_origin.is_some() {
            problem.with_pc_path_v2_evidence()
        } else {
            problem
        };
        let chance_authority = match chance_origin {
            Some(origin) => {
                match PcChanceCompiledAuthority::opening(self.query.as_ref(), origin, &problem) {
                    Ok(authority) => Some(authority),
                    Err(error) => {
                        return AppResponse::failed(
                            AppStatus::ExecutionFailed,
                            AppError::new(
                                AppErrorCode::ExecutionFailed,
                                format!("pc chance compiled authority rejected: {error}"),
                            ),
                        )
                    }
                }
            }
            None => None,
        };
        let execution = match (chance_authority.as_ref(), minimals_origin) {
            (Some(_), _) | (None, Some(_)) => context
                .services()
                .core_executor()
                .execute_pc_chance_with_control(&problem, context.execution_control()),
            (None, None) => context
                .services()
                .core_executor()
                .execute_with_control(&problem, context.execution_control()),
        };
        match execution {
            Ok(result) => {
                let chance_evidence = match chance_authority {
                    Some(authority) => match authority.validate_execution_result(&problem, &result)
                    {
                        Ok(evidence) => Some(evidence),
                        Err(error) => {
                            return AppResponse::failed(
                                AppStatus::ExecutionFailed,
                                AppError::new(
                                    AppErrorCode::ExecutionFailed,
                                    format!("pc chance result rejected: {error}"),
                                ),
                            )
                        }
                    },
                    None => None,
                };
                let mut response = AppResponse::success(AppRenderModel::Pc(
                    project_pc_allspin_result(result, result_projection),
                ));
                if let Some(evidence) = chance_evidence {
                    response = response.with_pc_chance_execution_evidence(evidence);
                }
                response
            }
            Err(error) => core_execution_error_response(error),
        }
    }
}
