use clearra_core_executor::CoreExecutionError;

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    resource_contract::resource_report_from_core_domain,
};

pub(crate) fn core_execution_error_response(error: CoreExecutionError) -> AppResponse {
    // Replay materialization and minimum selection follow a supported search.
    // Their resource/evidence failure must not claim the solver never executed
    // or that the user's queue/pattern contract is unsupported. Keep the
    // minimum marker exact: a missing runtime before search stays unsupported.
    if matches!(&error, CoreExecutionError::RuntimeUnavailable { component }
        if component.starts_with("complete_replay_")
            || *component == "minimum_product_memory_limit_exceeded")
    {
        return AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, format!("{error:?}")),
        );
    }
    if let CoreExecutionError::ResourceIncomplete {
        stage,
        status,
        resource_report,
    } = &error
    {
        let reason = resource_report
            .truncation_reason
            .map(|value| value.as_str())
            .unwrap_or("resource_incomplete");
        let status_label = if *status == 6 {
            "CLEARRA_PACKING_CAPACITY_EXCEEDED"
        } else {
            "CLEARRA_PACKING_INCOMPLETE"
        };
        return AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!(
                    "{status_label}: stage={stage}, status={status}, truncation_reason={reason}"
                ),
            ),
        )
        .with_resource_report(resource_report_from_core_domain(resource_report));
    }
    match error.unsupported_reason() {
        Some(reason) => unsupported_runtime_response(reason),
        None => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, format!("{error:?}")),
        ),
    }
}

#[cfg(test)]
mod replay_failure_tests {
    use super::*;

    #[test]
    fn terminal_replay_failures_preserve_executed_incomplete_search() {
        for component in [
            "complete_replay_memory_limit_exceeded",
            "complete_replay_whole_live_limit_exceeded",
            "complete_replay_execution_limit_exceeded",
            "complete_replay_evidence_invalid",
            "minimum_product_memory_limit_exceeded",
        ] {
            let response =
                core_execution_error_response(CoreExecutionError::RuntimeUnavailable { component });
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
            assert_eq!(
                response.error().unwrap().code(),
                AppErrorCode::ExecutionFailed
            );
            assert!(response.error().unwrap().message().contains(component));
            assert!(response.resource_report().solver_executed());
            assert_eq!(
                response.resource_report().result_completeness(),
                clearra_host_contract::ExecutionCompletenessState::Incomplete
            );
        }
        let unsupported = core_execution_error_response(CoreExecutionError::UnsupportedProblem);
        assert_eq!(unsupported.status(), AppStatus::Unsupported);
        assert!(!unsupported.resource_report().solver_executed());
        let missing_runtime =
            core_execution_error_response(CoreExecutionError::RuntimeUnavailable {
                component: "minimum_product_runtime_missing",
            });
        assert_eq!(missing_runtime.status(), AppStatus::Unsupported);
        assert!(!missing_runtime.resource_report().solver_executed());
    }
}

fn unsupported_runtime_response(reason: &'static str) -> AppResponse {
    let code = if reason == "core_c_packing_runtime_unavailable"
        || reason == "core_c_buildup_runtime_unavailable"
    {
        AppErrorCode::NativeCoreUnavailable
    } else if reason.starts_with("gpu_") {
        AppErrorCode::BackendGpuUnavailable
    } else {
        AppErrorCode::Unsupported
    };
    AppResponse::failed(
        AppStatus::Unsupported,
        AppError::new(
            code,
            format!("requested execution runtime is unsupported: {reason}"),
        ),
    )
}
