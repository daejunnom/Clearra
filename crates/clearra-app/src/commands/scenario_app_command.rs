use std::sync::Arc;

use clearra_pc_graph::request::PcScenarioQuery;
use clearra_problem::ProblemCompiler;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;
use clearra_validation::validators::pc_query_validator::validate_pc_scenario_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{
        execution_error_response::core_execution_error_response, ScenarioAppRenderContract,
    },
    pc_allspin_result::project_pc_allspin_result,
    pc_chance_probability_result::PcChanceCompiledAuthority,
    pc_result_projection::{
        validate_scenario_pc_result_projection, PcResultProjection, ValidatedPcResultProjection,
    },
    pc_save_result::{PcSaveCompiledAuthority, PcSaveCompiledAuthorityError},
    pc_score_summary_result::{PcScoreCompiledAuthority, PcScoreCompiledAuthorityError},
    pc_tiling_family_result::{PcTilingCompiledAuthority, PcTilingCompiledAuthorityError},
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioAppCommand {
    query: Arc<PcScenarioQuery>,
    render_contract: Option<ScenarioAppRenderContract>,
    result_projection: PcResultProjection,
}

impl ScenarioAppCommand {
    pub fn new(query: PcScenarioQuery) -> Self {
        Self {
            query: Arc::new(query),
            render_contract: None,
            result_projection: PcResultProjection::Standard,
        }
    }
}
impl ScenarioAppCommand {
    pub fn with_render_contract(mut self, render_contract: ScenarioAppRenderContract) -> Self {
        self.render_contract = Some(render_contract);
        self
    }

    pub const fn with_result_projection(mut self, result_projection: PcResultProjection) -> Self {
        self.result_projection = result_projection;
        self
    }

    pub const fn with_score_minimals_result(self) -> Self {
        self.with_result_projection(PcResultProjection::pc_score_minimals())
    }
}
impl ScenarioAppCommand {
    pub fn query(&self) -> &PcScenarioQuery {
        self.query.as_ref()
    }

    pub(crate) fn query_arc(&self) -> Arc<PcScenarioQuery> {
        Arc::clone(&self.query)
    }

    pub const fn result_projection(&self) -> PcResultProjection {
        self.result_projection
    }

    pub const fn score_minimals_requested(&self) -> bool {
        self.result_projection.score_minimals_origin().is_some()
    }

    /// Validates that the independently constructible projection belongs to
    /// this scenario query.
    pub fn validate_result_projection(&self) -> Result<(), &'static str> {
        self.validated_result_projection().map(|_| ())
    }

    pub(crate) fn validated_result_projection(
        &self,
    ) -> Result<ValidatedPcResultProjection, &'static str> {
        if !self.result_projection.is_standard() && self.render_contract.is_some() {
            return Err(if self.result_projection.chance_origin().is_some() {
                "pc chance does not accept a scenario render contract"
            } else if self.result_projection.minimals_origin().is_some() {
                "pc minimals does not accept a scenario render contract"
            } else if self.result_projection.score_origin().is_some() {
                "pc score does not accept a scenario render contract"
            } else if self.result_projection.score_minimals_origin().is_some() {
                "pc score-minimals does not accept a scenario render contract"
            } else if self.result_projection.tiling_origin().is_some() {
                "pc tiling does not accept a scenario render contract"
            } else if self.result_projection.save_origin().is_some() {
                "pc saves does not accept a scenario render contract"
            } else {
                "pc All-Spin does not accept a scenario render contract"
            });
        }
        validate_scenario_pc_result_projection(&self.query, self.result_projection)
    }

    pub(crate) fn into_validated_search_parts(
        self,
    ) -> Result<
        (
            Arc<PcScenarioQuery>,
            Option<ScenarioAppRenderContract>,
            ValidatedPcResultProjection,
        ),
        &'static str,
    > {
        let result_projection = self.validated_result_projection()?;
        Ok((self.query, self.render_contract, result_projection))
    }
}

impl RunnableAppCommand for ScenarioAppCommand {
    fn validation_failed_response(&self, report: DiagnosticReport) -> Option<AppResponse> {
        if !self.result_projection.is_standard() {
            return None;
        }
        self.render_contract
            .as_ref()
            .and_then(|contract| contract.validation_failed_response(report))
    }

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
        let report = validate_pc_scenario_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let chance_origin = result_projection.projection().chance_origin();
        let minimals_origin = result_projection.projection().minimals_origin();
        let path_origin = result_projection.projection().path_origin();
        let score_origin = result_projection.projection().score_origin();
        let score_minimals_origin = result_projection.projection().score_minimals_origin();
        if let Some(origin) = result_projection.projection().tiling_origin() {
            debug_assert!(self.render_contract.is_none());
            let wasm_terminal = context
                .services()
                .core_executor()
                .supports_cooperative_wasm_search();
            let compiled_authority = if wasm_terminal {
                PcTilingCompiledAuthority::compile_scenario_under_terminal_authority(
                    Arc::clone(&self.query),
                    origin,
                )
            } else {
                PcTilingCompiledAuthority::compile_scenario(Arc::clone(&self.query), origin)
            };
            let authority = match compiled_authority {
                Ok(authority) => authority,
                Err(PcTilingCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            *resource_report,
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
                        Ok(evidence) => AppResponse::success(AppRenderModel::Scenario(result))
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
            debug_assert!(self.render_contract.is_none());
            let authority =
                match PcSaveCompiledAuthority::compile_scenario(Arc::clone(&self.query), origin) {
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
                    Ok(evidence) => AppResponse::success(AppRenderModel::Scenario(result))
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
            debug_assert!(self.render_contract.is_none());
            let authority = match PcScoreCompiledAuthority::compile_score_minimals_scenario(
                Arc::clone(&self.query),
                origin,
            ) {
                Ok(authority) => authority,
                Err(PcScoreCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            *resource_report,
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
                Ok((result, evidence)) => AppResponse::success(AppRenderModel::Scenario(
                    project_pc_allspin_result(result, result_projection),
                ))
                .with_pc_score_portfolio_execution_evidence(evidence),
                Err(error) => core_execution_error_response(error),
            };
        }
        if let Some(origin) = score_origin {
            debug_assert!(self.render_contract.is_none());
            let authority = match PcScoreCompiledAuthority::compile_scenario(self.query, origin) {
                Ok(authority) => authority,
                Err(PcScoreCompiledAuthorityError::ResourceAdmission(resource_report)) => {
                    return core_execution_error_response(
                        clearra_core_executor::CoreExecutionError::resource_incomplete(
                            "execution-admission",
                            0,
                            *resource_report,
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
                Ok((result, evidence)) => AppResponse::success(AppRenderModel::Scenario(
                    project_pc_allspin_result(result, result_projection),
                ))
                .with_pc_score_execution_evidence(evidence),
                Err(error) => core_execution_error_response(error),
            };
        }
        let compiled = match chance_origin {
            Some(_) => ProblemCompiler::compile_scenario_percent(self.query.as_ref()),
            None => ProblemCompiler::compile_scenario_pc(self.query.as_ref()),
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
                match PcChanceCompiledAuthority::scenario(self.query.as_ref(), origin, &problem) {
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
                let result = project_pc_allspin_result(result, result_projection);
                if let Some(contract) = self.render_contract {
                    return contract.success_response(result);
                }
                let mut response = AppResponse::success(AppRenderModel::Scenario(result));
                if let Some(evidence) = chance_evidence {
                    response = response.with_pc_chance_execution_evidence(evidence);
                }
                response
            }
            Err(error) => core_execution_error_response(error),
        }
    }
}
