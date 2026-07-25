use clearra_core_executor::CoreExecutionError;

pub(crate) fn scenario_error_reason(error: CoreExecutionError) -> &'static str {
    match error {
        CoreExecutionError::UnsupportedProblem => "scenario PC execution backend unsupported",
        CoreExecutionError::RuntimeUnavailable { .. } => {
            "scenario PC execution runtime unavailable"
        }
        CoreExecutionError::ResourceIncomplete { .. } => {
            "scenario PC execution resource incomplete"
        }
        CoreExecutionError::Pc(_) | CoreExecutionError::Cover(_) => "scenario PC execution failed",
        CoreExecutionError::Cancelled => "scenario PC execution cancelled",
    }
}
