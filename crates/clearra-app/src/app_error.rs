#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppErrorCode {
    MissingInput,
    InvalidInput,
    ProblemCompileFailed,
    ExecutionFailed,
    TraceUnavailable,
    NoSolution,
    Unsupported,
    NativeCoreUnavailable,
    BackendGpuUnavailable,
    CliCommandUnsupported,
    PcScenarioExpectedMismatch,
    RulesProfileUnknown,
    RulesInputRequired,
    RulesInputInvalid,
    RulesExportUnsupported,
    ScoringProfileUnknown,
    ScoringInputRequired,
    ScoringInputInvalid,
    ConvertInputRequired,
    ConvertDirectionUnsupported,
    ConvertInputInvalid,
    ContinueTokenRequired,
    ContinueTokenInvalid,
    VerifyTargetUnknown,
    VerifyKicksFailed,
    OperationSequenceInvalid,
    OperationSequenceCancelled,
    OperationSequenceTimedOut,
    OperationSequenceIncomplete,
    SequenceDependenciesInvalid,
    SequenceDependenciesCancelled,
    SequenceDependenciesTimedOut,
    SequenceDependenciesIncomplete,
    UtilityParityInvalid,
    UtilityFumenInvalid,
    UtilityRenderInvalid,
    UtilityRenderLimitExceeded,
    UtilityToGrayInvalid,
    UtilityMirrorInvalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppError {
    code: AppErrorCode,
    message: String,
}

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl AppError {
    pub fn code(&self) -> AppErrorCode {
        self.code
    }
}
impl AppError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
