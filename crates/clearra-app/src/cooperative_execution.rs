use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    CoreExecutionError, WasmBuildProbabilityAdvance, WasmBuildProbabilitySession,
    WasmCpuSearchAdvance, WasmCpuSearchError, WasmCpuSearchSession, WasmSetupSearchAdvance,
    WasmSetupSearchSession,
};
use clearra_forward_search::{
    ForwardSearchAdvance, ForwardSearchError, ForwardSearchReport, ForwardSearchSession,
};
use clearra_host_contract::{AppCommandKind, BackendReport};
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::{AppCommand, RunnableAppCommand},
    app_context::AppContext,
    app_error::{AppError, AppErrorCode},
    app_request::{AppOutputPolicy, AppRequest},
    app_response::{AppResponse, AppStatus},
    commands::{core_execution_error_response, path_app_command::path_response},
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CooperativeAppAdvance {
    Pending,
    Progress,
    Completed(AppResponse),
    Cancelled,
}

pub struct CooperativeAppExecution {
    context: AppContext,
    state: CooperativeExecutionState,
}

enum CooperativeExecutionState {
    Immediate(Option<AppRequest>),
    Ready(Option<AppResponse>),
    Search(CooperativeSearchExecution),
    Postprocess(CooperativePostprocessExecution),
    Finished,
}

struct CooperativeSearchExecution {
    session: CooperativeSearchSession,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    backend_requested: String,
    gpu_device_requested: Option<String>,
}

struct CooperativePostprocessExecution {
    result: Option<clearra_core_executor::CoreExecutionResult>,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
}

enum CooperativeSearchSession {
    Pc(WasmCpuSearchSession),
    Setup(WasmSetupSearchSession),
    BuildProbability(WasmBuildProbabilitySession),
    Forward(ForwardSearchSession),
}

const FORWARD_SEARCH_COOPERATIVE_WORK_BUDGET: usize = 256;

#[derive(Clone)]
pub(crate) enum CooperativeSearchResponseKind {
    Pc,
    Path(OpeningPcSearchQuery),
    Scenario(Option<crate::commands::ScenarioAppRenderContract>),
    Setup,
    BuildProbability {
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
    },
    Damage,
    SpinFinder,
}

impl AppContext {
    pub fn start_cooperative_execution(&self, request: AppRequest) -> CooperativeAppExecution {
        self.start_cooperative_execution_inner(request)
    }

    fn start_cooperative_execution_inner(&self, request: AppRequest) -> CooperativeAppExecution {
        let forward = matches!(
            request.command(),
            AppCommand::Damage(_) | AppCommand::SpinFinder(_)
        );
        let core_search = matches!(
            request.command(),
            AppCommand::Pc(_)
                | AppCommand::Path(_)
                | AppCommand::Scenario(_)
                | AppCommand::Setup(_)
                | AppCommand::BuildProbability(_)
        );
        if (!forward && !core_search)
            || (core_search
                && !self
                    .services()
                    .core_executor()
                    .supports_cooperative_wasm_search())
        {
            return CooperativeAppExecution {
                context: self.clone(),
                state: CooperativeExecutionState::Immediate(Some(request)),
            };
        }

        let command_kind = request.command_kind();
        let (command, output_policy, _, _) = request.into_parts();
        let backend_policy = command.backend_policy();
        let backend_requested = backend_policy.backend_requested().to_owned();
        let gpu_device_requested = command.gpu_device_requested();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return CooperativeAppExecution {
                context: self.clone(),
                state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                    response,
                    command_kind,
                    &output_policy,
                ))),
            };
        }

        let command = match command {
            AppCommand::Damage(command) => {
                let session = ForwardSearchSession::new(command.into_query());
                let response_kind = CooperativeSearchResponseKind::Damage;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Forward(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                        }),
                    },
                    Err(error) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                            forward_search_error_response(error),
                            command_kind,
                            &output_policy,
                        ))),
                    },
                };
            }
            AppCommand::SpinFinder(command) => {
                let session = ForwardSearchSession::new(command.into_query());
                let response_kind = CooperativeSearchResponseKind::SpinFinder;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Forward(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                        }),
                    },
                    Err(error) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                            forward_search_error_response(error),
                            command_kind,
                            &output_policy,
                        ))),
                    },
                };
            }
            AppCommand::Setup(command) => {
                let session = WasmSetupSearchSession::new(command.query());
                let response_kind = CooperativeSearchResponseKind::Setup;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Setup(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                        }),
                    },
                    Err(error) => CooperativeAppExecution {
                        context: self.clone(),
                        state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                            wasm_search_error_response(
                                error,
                                &backend_requested,
                                gpu_device_requested,
                            ),
                            command_kind,
                            &output_policy,
                        ))),
                    },
                };
            }
            command => command,
        };

        let compiled = compile_search_command(command);
        let (problem, response_kind) = match compiled {
            Ok(compiled) => compiled,
            Err(response) => {
                return CooperativeAppExecution {
                    context: self.clone(),
                    state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                        response,
                        command_kind,
                        &output_policy,
                    ))),
                }
            }
        };
        let session = match &response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                field,
                aggregation,
                finesse,
            } => WasmBuildProbabilitySession::new(&problem, *field, *aggregation, finesse.clone())
                .map(CooperativeSearchSession::BuildProbability),
            _ => WasmCpuSearchSession::new(&problem).map(CooperativeSearchSession::Pc),
        };
        let session = match session {
            Ok(session) => session,
            Err(error) => {
                let response = wasm_search_error_response(
                    error,
                    &backend_requested,
                    gpu_device_requested.clone(),
                );
                return CooperativeAppExecution {
                    context: self.clone(),
                    state: CooperativeExecutionState::Ready(Some(self.finalize_response(
                        response,
                        command_kind,
                        &output_policy,
                    ))),
                };
            }
        };
        CooperativeAppExecution {
            context: self.clone(),
            state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                session,
                response_kind,
                command_kind,
                output_policy,
                validation_report,
                backend_requested,
                gpu_device_requested,
            }),
        }
    }
}

impl CooperativeAppExecution {
    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> CooperativeAppAdvance {
        let state = std::mem::replace(&mut self.state, CooperativeExecutionState::Finished);
        match state {
            CooperativeExecutionState::Immediate(mut request) => {
                if control.is_cancelled() {
                    return CooperativeAppAdvance::Cancelled;
                }
                CooperativeAppAdvance::Completed(self.context.run_with_execution_control(
                    request.take().expect("immediate request exists"),
                    control,
                ))
            }
            CooperativeExecutionState::Ready(mut response) => {
                CooperativeAppAdvance::Completed(response.take().expect("ready response exists"))
            }
            CooperativeExecutionState::Search(mut search) => {
                match advance_search_session(&mut search.session, work_budget, control) {
                    Ok(CooperativeBackendAdvance::Pending) => {
                        self.state = CooperativeExecutionState::Search(search);
                        CooperativeAppAdvance::Pending
                    }
                    Ok(CooperativeBackendAdvance::Cancelled)
                    | Err(WasmCpuSearchError::Cancelled) => CooperativeAppAdvance::Cancelled,
                    Ok(CooperativeBackendAdvance::CompletedCore(result)) => {
                        if matches!(&search.response_kind, CooperativeSearchResponseKind::Setup) {
                            let response = response_from_search(search.response_kind, result);
                            let response = if search.validation_report.is_empty() {
                                response
                            } else {
                                response.with_validation_diagnostics(search.validation_report)
                            };
                            CooperativeAppAdvance::Completed(self.context.finalize_response(
                                response,
                                search.command_kind,
                                &search.output_policy,
                            ))
                        } else {
                            control.report_progress("postprocess", 0, Some(1));
                            self.state = CooperativeExecutionState::Postprocess(
                                CooperativePostprocessExecution {
                                    result: Some(result),
                                    response_kind: search.response_kind,
                                    command_kind: search.command_kind,
                                    output_policy: search.output_policy,
                                    validation_report: search.validation_report,
                                },
                            );
                            CooperativeAppAdvance::Progress
                        }
                    }
                    Ok(CooperativeBackendAdvance::CompletedForward(report)) => {
                        let response = match search.response_kind {
                            CooperativeSearchResponseKind::Damage => {
                                AppResponse::success(AppRenderModel::Damage(report))
                            }
                            CooperativeSearchResponseKind::SpinFinder => {
                                AppResponse::success(AppRenderModel::SpinFinder(report))
                            }
                            _ => AppResponse::failed(
                                AppStatus::ExecutionFailed,
                                AppError::new(
                                    AppErrorCode::ExecutionFailed,
                                    "forward search response kind mismatch",
                                ),
                            ),
                        };
                        CooperativeAppAdvance::Completed(self.context.finalize_response(
                            response,
                            search.command_kind,
                            &search.output_policy,
                        ))
                    }
                    Err(error) => CooperativeAppAdvance::Completed(self.context.finalize_response(
                        wasm_search_error_response(
                            error,
                            &search.backend_requested,
                            search.gpu_device_requested.clone(),
                        ),
                        search.command_kind,
                        &search.output_policy,
                    )),
                }
            }
            CooperativeExecutionState::Postprocess(mut postprocess) => {
                if control.is_cancelled() {
                    return CooperativeAppAdvance::Cancelled;
                }
                let result = self
                    .context
                    .services()
                    .core_executor()
                    .postprocess_search_result(
                        postprocess
                            .result
                            .take()
                            .expect("postprocess result exists"),
                        control,
                    );
                control.report_progress("postprocess", 1, Some(1));
                let response = match result {
                    Ok(result) => response_from_search(postprocess.response_kind, result),
                    Err(CoreExecutionError::Cancelled) => return CooperativeAppAdvance::Cancelled,
                    Err(error) => core_execution_error_response(error),
                };
                let response = if postprocess.validation_report.is_empty() {
                    response
                } else {
                    response.with_validation_diagnostics(postprocess.validation_report)
                };
                CooperativeAppAdvance::Completed(self.context.finalize_response(
                    response,
                    postprocess.command_kind,
                    &postprocess.output_policy,
                ))
            }
            CooperativeExecutionState::Finished => {
                CooperativeAppAdvance::Completed(AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::ExecutionFailed,
                        "cooperative execution already finished",
                    ),
                ))
            }
        }
    }
}

pub(crate) fn compile_search_command(
    command: AppCommand,
) -> Result<
    (
        clearra_problem::SearchProblem,
        CooperativeSearchResponseKind,
    ),
    AppResponse,
> {
    let compiled = match command {
        AppCommand::Pc(command) => ProblemCompiler::compile_opening_pc(command.query())
            .map(|problem| (problem, CooperativeSearchResponseKind::Pc)),
        AppCommand::Path(command) => {
            let query = command.query().clone();
            ProblemCompiler::compile_opening_pc(&query)
                .map(|problem| (problem, CooperativeSearchResponseKind::Path(query)))
        }
        AppCommand::Scenario(command) => {
            let (query, render_contract) = command.into_search_parts();
            ProblemCompiler::compile_scenario_pc(&query).map(|problem| {
                (
                    problem,
                    CooperativeSearchResponseKind::Scenario(render_contract),
                )
            })
        }
        AppCommand::BuildProbability(command) => {
            if let Some(reason) =
                crate::commands::build_probability_app_command::invalid_query_reason(
                    command.query(),
                )
            {
                return Err(AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(AppErrorCode::InvalidInput, reason),
                ));
            }
            let field = command.query().field();
            let aggregation = command.query().aggregation();
            let finesse = command.query().finesse_request().clone();
            ProblemCompiler::compile_scenario_pc(command.query().core_query()).map(|problem| {
                (
                    problem,
                    CooperativeSearchResponseKind::BuildProbability {
                        field,
                        aggregation,
                        finesse,
                    },
                )
            })
        }
        _ => {
            return Err(AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "partitioned cooperative result requires a PC or path command",
                ),
            ))
        }
    };
    compiled.map_err(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
        )
    })
}

pub(crate) fn response_from_search(
    response_kind: CooperativeSearchResponseKind,
    result: clearra_core_executor::CoreExecutionResult,
) -> AppResponse {
    match response_kind {
        CooperativeSearchResponseKind::Pc => AppResponse::success(AppRenderModel::Pc(result)),
        CooperativeSearchResponseKind::Path(query) => path_response(&query, result),
        CooperativeSearchResponseKind::Scenario(Some(contract)) => {
            contract.success_response(result)
        }
        CooperativeSearchResponseKind::Scenario(None) => {
            AppResponse::success(AppRenderModel::Scenario(result))
        }
        CooperativeSearchResponseKind::Setup => AppResponse::success(AppRenderModel::Setup(result)),
        CooperativeSearchResponseKind::BuildProbability { .. } => {
            AppResponse::success(AppRenderModel::BuildProbability(result))
        }
        CooperativeSearchResponseKind::Damage | CooperativeSearchResponseKind::SpinFinder => {
            AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(AppErrorCode::ExecutionFailed, "forward result expected"),
            )
        }
    }
}

enum CooperativeBackendAdvance {
    Pending,
    CompletedCore(clearra_core_executor::CoreExecutionResult),
    CompletedForward(ForwardSearchReport),
    Cancelled,
}

fn advance_search_session(
    session: &mut CooperativeSearchSession,
    work_budget: usize,
    control: &ExecutionControl,
) -> Result<CooperativeBackendAdvance, WasmCpuSearchError> {
    match session {
        CooperativeSearchSession::Pc(session) => match session.advance(work_budget, control)? {
            WasmCpuSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            WasmCpuSearchAdvance::Completed(result) => {
                Ok(CooperativeBackendAdvance::CompletedCore(result))
            }
            WasmCpuSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
        CooperativeSearchSession::Setup(session) => match session.advance(work_budget, control)? {
            WasmSetupSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            WasmSetupSearchAdvance::Completed(result) => {
                Ok(CooperativeBackendAdvance::CompletedCore(result))
            }
            WasmSetupSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
        CooperativeSearchSession::BuildProbability(session) => {
            match session.advance(work_budget, control)? {
                WasmBuildProbabilityAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
                WasmBuildProbabilityAdvance::Completed(result) => {
                    Ok(CooperativeBackendAdvance::CompletedCore(result))
                }
                WasmBuildProbabilityAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
            }
        }
        CooperativeSearchSession::Forward(session) => match session
            .advance(
                work_budget.min(FORWARD_SEARCH_COOPERATIVE_WORK_BUDGET),
                control,
            )
            .map_err(forward_error_as_wasm)?
        {
            ForwardSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            ForwardSearchAdvance::Completed(report) => {
                Ok(CooperativeBackendAdvance::CompletedForward(report))
            }
            ForwardSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
    }
}

fn forward_error_as_wasm(error: ForwardSearchError) -> WasmCpuSearchError {
    match error {
        ForwardSearchError::Cancelled => WasmCpuSearchError::Cancelled,
        ForwardSearchError::UnsupportedRuleProfile(reason) => {
            WasmCpuSearchError::Unsupported { reason }
        }
        ForwardSearchError::EmptyQueue => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_empty_queue",
        },
        ForwardSearchError::InvalidHeight => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_invalid_height",
        },
        ForwardSearchError::BoardOutsideField => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_board_outside_field",
        },
        ForwardSearchError::PatternRequiresSpinFinder => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_pattern_requires_spin_finder",
        },
        ForwardSearchError::SpinProfileDisabled => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_spin_profile_disabled",
        },
    }
}

fn forward_search_error_response(error: ForwardSearchError) -> AppResponse {
    let error = forward_error_as_wasm(error);
    wasm_search_error_response(error, "cpu", None)
}

fn core_error_from_wasm(error: WasmCpuSearchError) -> CoreExecutionError {
    match error {
        WasmCpuSearchError::Unsupported { reason } => {
            CoreExecutionError::RuntimeUnavailable { component: reason }
        }
        WasmCpuSearchError::WorkerPoolUnavailable => CoreExecutionError::RuntimeUnavailable {
            component: "wasm_cpu_worker_pool_unavailable",
        },
        WasmCpuSearchError::InvalidProblem { reason } => CoreExecutionError::Pc(reason.to_owned()),
        WasmCpuSearchError::Cancelled => CoreExecutionError::Cancelled,
    }
}

fn wasm_search_error_response(
    error: WasmCpuSearchError,
    backend_requested: &str,
    gpu_device_requested: Option<String>,
) -> AppResponse {
    let reason = error.reason();
    let response = core_execution_error_response(core_error_from_wasm(error));
    let mut report = BackendReport::new(backend_requested, "none", None::<String>).with_gpu_device(
        gpu_device_requested,
        None,
        None,
        None,
        None,
    );
    if let Some((failure_class, failure_stage)) = gpu_failure_contract(reason) {
        report = report.with_gpu_execution_failure(
            Some(failure_class.to_owned()),
            Some(failure_stage.to_owned()),
            None,
            false,
        );
    }
    response.with_backend_report(report)
}

fn gpu_failure_contract(reason: &str) -> Option<(&'static str, &'static str)> {
    match reason {
        "webgpu_backend_unavailable" => Some(("unavailable", "capability-query")),
        "webgpu_transient_before_commit" => Some(("transient-before-commit", "readback")),
        "webgpu_resource_incomplete" => Some(("resource-incomplete", "host-reduction")),
        "webgpu_invalid_request" => Some(("invalid-request", "batch-planning")),
        "webgpu_trust_mismatch_invalid_result" => Some(("trust-mismatch", "exact-confirm")),
        "webgpu_trust_mismatch"
        | "webgpu_trust_mismatch_buffer_shape"
        | "webgpu_trust_mismatch_edge_count"
        | "webgpu_trust_mismatch_operation_index"
        | "webgpu_trust_mismatch_child_state"
        | "webgpu_trust_mismatch_no_confirmed_dispatch"
        | "webgpu_trust_mismatch_no_confirmed_parent"
        | "webgpu_trust_mismatch_unconfirmed_result" => {
            Some(("trust-mismatch", "cpu-reference-confirm"))
        }
        "webgpu_fatal_internal" => Some(("fatal-internal", "readback")),
        _ => None,
    }
}
