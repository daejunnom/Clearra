use crate::exit::ExitCode;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CliErrorCode {
    CliMissingValue,
    CliInvalidValue,
    CliOutputFormatUnsupported,
    CliCommandUnknown,
    CliUnknownOption,
    CliCommandUnsupported,
    ProductRuntimeUnsupported,
    NativeCoreUnavailable,
    BackendGpuUnavailable,
    PcTargetInvalid,
    PcTargetUnsupportedMvp,
    PcQueryInvalid,
    PcSearchInternal,
    PathSearchInternal,
    PathNoSolution,
    PathTraceUnavailable,
    PercentQueryInvalid,
    PcScenarioFixtureRequired,
    PcScenarioFixtureInvalid,
    PcScenarioExpectedMismatch,
    PcScenarioSearchInternal,
    SetupQueryInvalid,
    CoverQueryInvalid,
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
    ContinueSearchInternal,
    VerifyTargetUnknown,
    VerifyKicksFailed,
}

impl CliErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CliMissingValue => "E_CLI_MISSING_VALUE",
            Self::CliInvalidValue => "E_CLI_INVALID_VALUE",
            Self::CliOutputFormatUnsupported => "E_CLI_OUTPUT_FORMAT_UNSUPPORTED",
            Self::CliCommandUnknown => "E_CLI_COMMAND_UNKNOWN",
            Self::CliUnknownOption => "E_CLI_UNKNOWN_OPTION",
            Self::CliCommandUnsupported => "E_CLI_COMMAND_UNSUPPORTED",
            Self::ProductRuntimeUnsupported => "E_PRODUCT_RUNTIME_UNSUPPORTED",
            Self::NativeCoreUnavailable => "E_NATIVE_CORE_UNAVAILABLE",
            Self::BackendGpuUnavailable => "E_BACKEND_GPU_UNAVAILABLE",
            Self::PcTargetInvalid => "E_PC_TARGET_INVALID",
            Self::PcTargetUnsupportedMvp => "E_PC_TARGET_UNSUPPORTED_MVP",
            Self::PcQueryInvalid => "E_PC_QUERY_INVALID",
            Self::PcSearchInternal => "E_PC_SEARCH_INTERNAL",
            Self::PathSearchInternal => "E_PATH_SEARCH_INTERNAL",
            Self::PathNoSolution => "E_PATH_NO_SOLUTION",
            Self::PathTraceUnavailable => "E_PATH_TRACE_UNAVAILABLE",
            Self::PercentQueryInvalid => "E_PERCENT_QUERY_INVALID",
            Self::PcScenarioFixtureRequired => "E_PC_SCENARIO_FIXTURE_REQUIRED",
            Self::PcScenarioFixtureInvalid => "E_PC_SCENARIO_FIXTURE_INVALID",
            Self::PcScenarioExpectedMismatch => "E_PC_SCENARIO_EXPECTED_MISMATCH",
            Self::PcScenarioSearchInternal => "E_PC_SCENARIO_SEARCH_INTERNAL",
            Self::SetupQueryInvalid => "E_SETUP_QUERY_INVALID",
            Self::CoverQueryInvalid => "E_COVER_QUERY_INVALID",
            Self::RulesProfileUnknown => "E_RULES_PROFILE_UNKNOWN",
            Self::RulesInputRequired => "E_RULES_INPUT_REQUIRED",
            Self::RulesInputInvalid => "E_RULES_INPUT_INVALID",
            Self::RulesExportUnsupported => "E_RULES_EXPORT_UNSUPPORTED",
            Self::ScoringProfileUnknown => "E_SCORING_PROFILE_UNKNOWN",
            Self::ScoringInputRequired => "E_SCORING_INPUT_REQUIRED",
            Self::ScoringInputInvalid => "E_SCORING_INPUT_INVALID",
            Self::ConvertInputRequired => "E_CONVERT_INPUT_REQUIRED",
            Self::ConvertDirectionUnsupported => "E_CONVERT_DIRECTION_UNSUPPORTED",
            Self::ConvertInputInvalid => "E_CONVERT_INPUT_INVALID",
            Self::ContinueTokenRequired => "E_CONTINUE_TOKEN_REQUIRED",
            Self::ContinueTokenInvalid => "E_CONTINUE_TOKEN_INVALID",
            Self::ContinueSearchInternal => "E_CONTINUE_SEARCH_INTERNAL",
            Self::VerifyTargetUnknown => "E_VERIFY_TARGET_UNKNOWN",
            Self::VerifyKicksFailed => "E_VERIFY_KICKS_FAILED",
        }
    }
}
impl CliErrorCode {
    pub const fn default_exit_code(self) -> ExitCode {
        match self {
            Self::CliCommandUnsupported
            | Self::ProductRuntimeUnsupported
            | Self::NativeCoreUnavailable
            | Self::BackendGpuUnavailable
            | Self::ConvertDirectionUnsupported => ExitCode::Unsupported,
            Self::PcSearchInternal
            | Self::PathSearchInternal
            | Self::PcScenarioSearchInternal
            | Self::ContinueSearchInternal
            | Self::VerifyKicksFailed => ExitCode::InternalError,
            Self::CliMissingValue
            | Self::CliInvalidValue
            | Self::CliOutputFormatUnsupported
            | Self::CliCommandUnknown
            | Self::CliUnknownOption
            | Self::PcTargetInvalid
            | Self::PcTargetUnsupportedMvp
            | Self::PcQueryInvalid
            | Self::PathNoSolution
            | Self::PathTraceUnavailable
            | Self::PercentQueryInvalid
            | Self::PcScenarioFixtureRequired
            | Self::PcScenarioFixtureInvalid
            | Self::PcScenarioExpectedMismatch
            | Self::SetupQueryInvalid
            | Self::CoverQueryInvalid
            | Self::RulesProfileUnknown
            | Self::RulesInputRequired
            | Self::RulesInputInvalid
            | Self::RulesExportUnsupported
            | Self::ScoringProfileUnknown
            | Self::ScoringInputRequired
            | Self::ScoringInputInvalid
            | Self::ConvertInputRequired
            | Self::ConvertInputInvalid
            | Self::ContinueTokenRequired
            | Self::ContinueTokenInvalid
            | Self::VerifyTargetUnknown => ExitCode::ValidationFailed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_adapter_error_codes_have_stable_strings_and_exit_codes() {
        assert_eq!(
            CliErrorCode::CliMissingValue.as_str(),
            "E_CLI_MISSING_VALUE"
        );
        assert_eq!(
            CliErrorCode::ConvertDirectionUnsupported.as_str(),
            "E_CONVERT_DIRECTION_UNSUPPORTED"
        );
        assert_eq!(
            CliErrorCode::ContinueTokenInvalid.as_str(),
            "E_CONTINUE_TOKEN_INVALID"
        );
        assert_eq!(
            CliErrorCode::PcScenarioFixtureInvalid.as_str(),
            "E_PC_SCENARIO_FIXTURE_INVALID"
        );
        assert_eq!(
            CliErrorCode::PcScenarioExpectedMismatch.as_str(),
            "E_PC_SCENARIO_EXPECTED_MISMATCH"
        );
        assert_eq!(
            CliErrorCode::VerifyKicksFailed.as_str(),
            "E_VERIFY_KICKS_FAILED"
        );
        assert_eq!(
            CliErrorCode::PathTraceUnavailable.as_str(),
            "E_PATH_TRACE_UNAVAILABLE"
        );
        assert_eq!(
            CliErrorCode::CliMissingValue.default_exit_code(),
            ExitCode::ValidationFailed
        );
        assert_eq!(
            CliErrorCode::ConvertDirectionUnsupported.default_exit_code(),
            ExitCode::Unsupported
        );
        assert_eq!(
            CliErrorCode::PcSearchInternal.default_exit_code(),
            ExitCode::InternalError
        );
    }
}
