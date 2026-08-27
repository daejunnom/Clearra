use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    resource::ResourceReport,
};
use clearra_core_ffi::NativeCoreError;
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};

use crate::{
    buildup::BuildUpRunnerError,
    core_execution_result::CoreExecutionResult,
    packing::PackingRunnerError,
    service::{
        CoverService, CoverServiceError, PcService, PcServiceError, PcTilingMaterializationError,
        PercentService, PercentServiceError,
    },
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
        match (problem.preset(), problem.output_policy()) {
            (
                SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc,
                SearchOutputPolicy::CoverageSummary,
            ) => PercentService::execute_with_control(problem, control)
                .map_err(core_error_from_percent),
            (SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc, _) => {
                PcService::execute_with_control(problem, control).map_err(core_error_from_pc)
            }
            (SearchProblemPreset::Setup, _) => Err(CoreExecutionError::UnsupportedProblem),
            (SearchProblemPreset::Build, _) => {
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
        ))
        | PcServiceError::TilingMaterialization(PcTilingMaterializationError::ExecutionCancelled) => {
            CoreExecutionError::Cancelled
        }
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
        PcServiceError::TilingMaterialization(PcTilingMaterializationError::AllocationFailed) => {
            CoreExecutionError::Pc("tiling_materialization_allocation_failed".to_owned())
        }
        PcServiceError::TilingMaterialization(
            PcTilingMaterializationError::MemoryAccountingUnavailable,
        ) => CoreExecutionError::Pc(
            "tiling_materialization_memory_accounting_unavailable".to_owned(),
        ),
        PcServiceError::TilingMaterialization(
            PcTilingMaterializationError::ResourceIncomplete(resource_report),
        ) => CoreExecutionError::resource_incomplete("tiling-materialization", 0, resource_report),
        PcServiceError::TilingMaterialization(PcTilingMaterializationError::PageStore(reason)) => {
            CoreExecutionError::Pc(reason.to_owned())
        }
        PcServiceError::TilingMaterialization(
            PcTilingMaterializationError::CandidateUnavailable { .. },
        ) => CoreExecutionError::Pc("tiling_materialization_candidate_unavailable".to_owned()),
        PcServiceError::TilingMaterialization(PcTilingMaterializationError::CandidateIdentity(
            _,
        )) => {
            CoreExecutionError::Pc("tiling_materialization_candidate_identity_invalid".to_owned())
        }
        other => CoreExecutionError::Pc(format!("{other:?}")),
    }
}

fn core_error_from_percent(error: PercentServiceError) -> CoreExecutionError {
    match error {
        PercentServiceError::UnsupportedPreset => CoreExecutionError::UnsupportedProblem,
        PercentServiceError::Packing(PackingRunnerError::ExecutionCancelled)
        | PercentServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        ))
        | PercentServiceError::BuildUp(BuildUpRunnerError::ExecutionCancelled)
        | PercentServiceError::BuildUp(BuildUpRunnerError::Native(
            NativeCoreError::ExecutionCancelled,
        )) => CoreExecutionError::Cancelled,
        PercentServiceError::Packing(PackingRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_packing_runtime_unavailable",
            }
        }
        PercentServiceError::Packing(PackingRunnerError::Native(
            NativeCoreError::PackingIncomplete {
                status,
                resource_report,
            },
        )) => CoreExecutionError::resource_incomplete("packing", status, resource_report),
        PercentServiceError::Packing(PackingRunnerError::Backend(error)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: error.reason(),
            }
        }
        PercentServiceError::Packing(PackingRunnerError::BackendExecutorUnavailable {
            reason,
            ..
        }) => CoreExecutionError::RuntimeUnavailable { component: reason },
        PercentServiceError::BuildUp(BuildUpRunnerError::Native(NativeCoreError::Unavailable)) => {
            CoreExecutionError::RuntimeUnavailable {
                component: "core_c_buildup_runtime_unavailable",
            }
        }
        PercentServiceError::BuildUp(BuildUpRunnerError::UnsupportedPieceSource { reason }) => {
            CoreExecutionError::RuntimeUnavailable { component: reason }
        }
        other => CoreExecutionError::Pc(format!("Percent({other:?})")),
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
mod pc_tiling_materialization_error_tests {
    use clearra_core_ffi::PackingCandidateIdentityError;

    use super::{core_error_from_pc, CoreExecutionError};
    use crate::service::{PcServiceError, PcTilingMaterializationError};

    #[test]
    fn cancellation_is_preserved_as_core_cancelled() {
        assert_eq!(
            core_error_from_pc(PcServiceError::TilingMaterialization(
                PcTilingMaterializationError::ExecutionCancelled,
            )),
            CoreExecutionError::Cancelled
        );
    }

    #[test]
    fn non_cancellation_materialization_failures_keep_distinct_stable_reasons() {
        let cases = [
            (
                PcTilingMaterializationError::AllocationFailed,
                "tiling_materialization_allocation_failed",
            ),
            (
                PcTilingMaterializationError::CandidateUnavailable { candidate_index: 7 },
                "tiling_materialization_candidate_unavailable",
            ),
            (
                PcTilingMaterializationError::CandidateIdentity(
                    PackingCandidateIdentityError::UnknownPieceCode(0),
                ),
                "tiling_materialization_candidate_identity_invalid",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(
                core_error_from_pc(PcServiceError::TilingMaterialization(error)),
                CoreExecutionError::Pc(expected.to_owned())
            );
        }
    }
}

#[cfg(test)]
#[path = "core_executor_tests.rs"]
mod tests;
