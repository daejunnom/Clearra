use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    resource::ResourceReport,
};
use clearra_core_ffi::NativeCoreError;
use clearra_problem::{SearchProblem, SearchProblemPreset};

use crate::{
    buildup::BuildUpRunnerError,
    core_execution_result::CoreExecutionResult,
    packing::PackingRunnerError,
    service::{CoverService, CoverServiceError, PcService, PcServiceError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreExecutionError {
    UnsupportedProblem,
    RuntimeUnavailable {
        component: &'static str,
    },
    ResourceIncomplete {
        stage: &'static str,
        status: i32,
        resource_report: ResourceReport,
    },
    Pc(String),
    Cover(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoreExecutor;

impl CoreExecutor {
    pub fn execute(problem: &SearchProblem) -> Result<CoreExecutionResult, CoreExecutionError> {
        Self::execute_with_cancellation(problem, &ExecutionCancellationToken::new())
    }

    pub fn execute_with_cancellation(
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        Self::execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
    }

    pub fn execute_with_control(
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        control.report_progress("core-executor", 0, None);
        match problem.preset() {
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc => {
                PcService::execute_with_control(problem, control).map_err(core_error_from_pc)
            }
            SearchProblemPreset::Setup => Err(CoreExecutionError::UnsupportedProblem),
            SearchProblemPreset::Build => {
                CoverService::execute_with_control(problem, control).map_err(core_error_from_cover)
            }
        }
    }
}
impl CoreExecutor {
    pub fn execute_build_coverage(
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        Self::execute_build_coverage_with_cancellation(
            problem,
            query,
            &ExecutionCancellationToken::new(),
        )
    }

    pub fn execute_build_coverage_with_cancellation(
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        Self::execute_build_coverage_with_control(
            problem,
            query,
            &ExecutionControl::new(cancellation.clone()),
        )
    }

    pub fn execute_build_coverage_with_control(
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        CoverService::execute_build_coverage_with_control(problem, query, control)
            .map_err(core_error_from_cover)
    }
}

impl CoreExecutionError {
    pub const fn unsupported_reason(&self) -> Option<&'static str> {
        match self {
            Self::UnsupportedProblem => Some("problem_runtime_unsupported"),
            Self::RuntimeUnavailable { component } => Some(component),
            Self::ResourceIncomplete { .. } | Self::Pc(_) | Self::Cover(_) | Self::Cancelled => {
                None
            }
        }
    }
}

impl CoreExecutionError {
    pub const fn resource_incomplete(
        stage: &'static str,
        status: i32,
        resource_report: ResourceReport,
    ) -> Self {
        Self::ResourceIncomplete {
            stage,
            status,
            resource_report,
        }
    }
}

fn core_error_from_pc(error: PcServiceError) -> CoreExecutionError {
    match error {
        PcServiceError::UnsupportedPreset => CoreExecutionError::UnsupportedProblem,
        PcServiceError::Packing(PackingRunnerError::ExecutionCancelled)
        | PcServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        ))
        | PcServiceError::BuildUp(BuildUpRunnerError::ExecutionCancelled)
        | PcServiceError::BuildUp(BuildUpRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        )) => CoreExecutionError::Cancelled,
        PcServiceError::Packing(PackingRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_packing_runtime_unavailable",
            }
        }
        PcServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::PackingIncomplete {
                status,
                resource_report,
            },
        )) => CoreExecutionError::resource_incomplete("packing", status, resource_report),
        PcServiceError::Packing(PackingRunnerError::Backend(error)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: error.reason(),
            }
        }
        PcServiceError::Packing(PackingRunnerError::BackendExecutorUnavailable {
            reason, ..
        }) => CoreExecutionError::RuntimeUnavailable { component: reason },
        PcServiceError::BuildUp(BuildUpRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_buildup_runtime_unavailable",
            }
        }
        PcServiceError::BuildUp(BuildUpRunnerError::UnsupportedPieceSource { reason }) => {
            CoreExecutionError::RuntimeUnavailable { component: reason }
        }
        other => CoreExecutionError::Pc(format!("{other:?}")),
    }
}

fn core_error_from_cover(error: CoverServiceError) -> CoreExecutionError {
    match error {
        CoverServiceError::UnsupportedPreset => CoreExecutionError::UnsupportedProblem,
        CoverServiceError::Packing(PackingRunnerError::ExecutionCancelled)
        | CoverServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        ))
        | CoverServiceError::BuildUp(BuildUpRunnerError::ExecutionCancelled)
        | CoverServiceError::BuildUp(BuildUpRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        )) => CoreExecutionError::Cancelled,
        CoverServiceError::Packing(PackingRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_packing_runtime_unavailable",
            }
        }
        CoverServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::PackingIncomplete {
                status,
                resource_report,
            },
        )) => CoreExecutionError::resource_incomplete("packing", status, resource_report),
        CoverServiceError::Packing(PackingRunnerError::Backend(error)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: error.reason(),
            }
        }
        CoverServiceError::Packing(PackingRunnerError::BackendExecutorUnavailable {
            reason,
            ..
        }) => CoreExecutionError::RuntimeUnavailable { component: reason },
        CoverServiceError::BuildUp(BuildUpRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_buildup_runtime_unavailable",
            }
        }
        CoverServiceError::BuildUp(BuildUpRunnerError::UnsupportedPieceSource { reason }) => {
            CoreExecutionError::RuntimeUnavailable { component: reason }
        }
        other => CoreExecutionError::Cover(format!("{other:?}")),
    }
}

#[cfg(test)]
#[path = "core_executor_tests.rs"]
mod tests;
