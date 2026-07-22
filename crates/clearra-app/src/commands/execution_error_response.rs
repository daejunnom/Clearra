use clearra_core_executor::{CoreExecutionError, PercentServiceError};

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    resource_contract::resource_report_from_core_domain,
};

pub(crate) fn core_execution_error_response(error: CoreExecutionError) -> AppResponse {
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

pub(crate) fn percent_execution_error_response(error: PercentServiceError) -> AppResponse {
    if let Some((stage, status, resource_report)) = error.resource_incomplete() {
        let reason = resource_report
            .truncation_reason
            .map(|value| value.as_str())
            .unwrap_or("resource_incomplete");
        let status_label = if status == 6 {
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
        .with_resource_report(resource_report_from_core_domain(&resource_report));
    }
    match error.unsupported_reason() {
        Some(reason) => unsupported_runtime_response(reason),
        None => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, format!("{error:?}")),
        ),
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
