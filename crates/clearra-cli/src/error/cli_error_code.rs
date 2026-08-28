use crate::exit::ExitCode;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CliErrorCode {
    CliMissingValue,
    CliInvalidValue,
    CliOutputFormatUnsupported,
    CliOutputLimitExceeded,
    CliArtifactInvalid,
    CliArtifactPublishFailed,
    CliArtifactDurabilityUncertain,
    CliArtifactCommittedButOutputFailed,
    CliCommandUnknown,
    CliUnknownOption,
    CliCommandUnsupported,
    CliProductThreadUnavailable,
    ProductRuntimeUnsupported,
    NativeCoreUnavailable,
    BackendGpuUnavailable,
    TablebaseInstallFailed,
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
    TieSnapshotUnsafePath,
    TieSnapshotTargetExists,
    TieSnapshotLocked,
    TieSnapshotIo,
    TieSnapshotInvalid,
    TieSnapshotTampered,
    TieSnapshotStale,
    TieSnapshotQueryMismatch,
    TieSnapshotBuildMismatch,
    TieSnapshotCandidateMismatch,
    VerifyTargetUnknown,
    VerifyKicksFailed,
    OperationSequenceInvalid,
    OperationSequenceCancelled,
    OperationSequenceTimedOut,
    OperationSequenceIncomplete,
    UtilityParityInvalid,
    UtilityFumenInvalid,
    UtilityRenderInvalid,
    UtilityRenderLimitExceeded,
    UtilityToGrayInvalid,
    UtilityMirrorInvalid,
}

impl CliErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CliMissingValue => "E_CLI_MISSING_VALUE",
            Self::CliInvalidValue => "E_CLI_INVALID_VALUE",
            Self::CliOutputFormatUnsupported => "E_CLI_OUTPUT_FORMAT_UNSUPPORTED",
            Self::CliOutputLimitExceeded => "E_CLI_OUTPUT_LIMIT_EXCEEDED",
            Self::CliArtifactInvalid => "E_CLI_ARTIFACT_INVALID",
            Self::CliArtifactPublishFailed => "E_CLI_ARTIFACT_PUBLISH_FAILED",
            Self::CliArtifactDurabilityUncertain => "E_CLI_ARTIFACT_DURABILITY_UNCERTAIN",
            Self::CliArtifactCommittedButOutputFailed => {
                "E_CLI_ARTIFACT_COMMITTED_BUT_OUTPUT_FAILED"
            }
            Self::CliCommandUnknown => "E_CLI_COMMAND_UNKNOWN",
            Self::CliUnknownOption => "E_CLI_UNKNOWN_OPTION",
            Self::CliCommandUnsupported => "E_CLI_COMMAND_UNSUPPORTED",
            Self::CliProductThreadUnavailable => "E_CLI_PRODUCT_THREAD_UNAVAILABLE",
            Self::ProductRuntimeUnsupported => "E_PRODUCT_RUNTIME_UNSUPPORTED",
            Self::NativeCoreUnavailable => "E_NATIVE_CORE_UNAVAILABLE",
            Self::BackendGpuUnavailable => "E_BACKEND_GPU_UNAVAILABLE",
            Self::TablebaseInstallFailed => "E_TABLEBASE_INSTALL_FAILED",
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
            Self::TieSnapshotUnsafePath => "E_TIE_SNAPSHOT_PATH_UNSAFE",
            Self::TieSnapshotTargetExists => "E_TIE_SNAPSHOT_TARGET_EXISTS",
            Self::TieSnapshotLocked => "E_TIE_SNAPSHOT_LOCKED",
            Self::TieSnapshotIo => "E_TIE_SNAPSHOT_IO",
            Self::TieSnapshotInvalid => "E_TIE_SNAPSHOT_INVALID",
            Self::TieSnapshotTampered => "E_TIE_SNAPSHOT_TAMPERED",
            Self::TieSnapshotStale => "E_TIE_SNAPSHOT_STALE",
            Self::TieSnapshotQueryMismatch => "E_TIE_SNAPSHOT_QUERY_MISMATCH",
            Self::TieSnapshotBuildMismatch => "E_TIE_SNAPSHOT_BUILD_MISMATCH",
            Self::TieSnapshotCandidateMismatch => "E_TIE_SNAPSHOT_CANDIDATE_MISMATCH",
            Self::VerifyTargetUnknown => "E_VERIFY_TARGET_UNKNOWN",
            Self::VerifyKicksFailed => "E_VERIFY_KICKS_FAILED",
            Self::OperationSequenceInvalid => "E_OPERATION_SEQUENCE_INPUT_INVALID",
            Self::OperationSequenceCancelled => "E_OPERATION_SEQUENCE_CANCELLED",
            Self::OperationSequenceTimedOut => "E_OPERATION_SEQUENCE_TIMED_OUT",
            Self::OperationSequenceIncomplete => "E_OPERATION_SEQUENCE_INCOMPLETE",
            Self::UtilityParityInvalid => "E_UTILITY_PARITY_INPUT_INVALID",
            Self::UtilityFumenInvalid => "E_UTILITY_FUMEN_INPUT_INVALID",
            Self::UtilityRenderInvalid => "E_UTILITY_RENDER_INPUT_INVALID",
            Self::UtilityRenderLimitExceeded => "E_UTILITY_RENDER_LIMIT_EXCEEDED",
            Self::UtilityToGrayInvalid => "E_UTILITY_TO_GRAY_INPUT_INVALID",
            Self::UtilityMirrorInvalid => "E_UTILITY_MIRROR_INPUT_INVALID",
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
            | Self::CliProductThreadUnavailable
            | Self::PathSearchInternal
            | Self::PcScenarioSearchInternal
            | Self::ContinueSearchInternal
            | Self::TablebaseInstallFailed
            | Self::CliArtifactPublishFailed
            | Self::CliArtifactDurabilityUncertain
            | Self::CliArtifactCommittedButOutputFailed
            | Self::TieSnapshotIo
            | Self::VerifyKicksFailed
            | Self::OperationSequenceCancelled
            | Self::OperationSequenceTimedOut
            | Self::OperationSequenceIncomplete => ExitCode::InternalError,
            Self::CliMissingValue
            | Self::CliInvalidValue
            | Self::CliOutputFormatUnsupported
            | Self::CliOutputLimitExceeded
            | Self::CliArtifactInvalid
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
            | Self::TieSnapshotUnsafePath
            | Self::TieSnapshotTargetExists
            | Self::TieSnapshotLocked
            | Self::TieSnapshotInvalid
            | Self::TieSnapshotTampered
            | Self::TieSnapshotStale
            | Self::TieSnapshotQueryMismatch
            | Self::TieSnapshotBuildMismatch
            | Self::TieSnapshotCandidateMismatch
            | Self::VerifyTargetUnknown
            | Self::OperationSequenceInvalid
            | Self::UtilityParityInvalid
            | Self::UtilityFumenInvalid
            | Self::UtilityRenderInvalid
            | Self::UtilityRenderLimitExceeded
            | Self::UtilityToGrayInvalid
            | Self::UtilityMirrorInvalid => ExitCode::ValidationFailed,
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
            CliErrorCode::CliArtifactInvalid.as_str(),
            "E_CLI_ARTIFACT_INVALID"
        );
        assert_eq!(
            CliErrorCode::CliArtifactPublishFailed.as_str(),
            "E_CLI_ARTIFACT_PUBLISH_FAILED"
        );
        assert_eq!(
            CliErrorCode::CliArtifactDurabilityUncertain.as_str(),
            "E_CLI_ARTIFACT_DURABILITY_UNCERTAIN"
        );
        assert_eq!(
            CliErrorCode::CliArtifactCommittedButOutputFailed.as_str(),
            "E_CLI_ARTIFACT_COMMITTED_BUT_OUTPUT_FAILED"
        );
        assert_eq!(
            CliErrorCode::CliProductThreadUnavailable.as_str(),
            "E_CLI_PRODUCT_THREAD_UNAVAILABLE"
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
        assert_eq!(
            CliErrorCode::CliArtifactPublishFailed.default_exit_code(),
            ExitCode::InternalError
        );
        assert_eq!(
            CliErrorCode::CliProductThreadUnavailable.default_exit_code(),
            ExitCode::InternalError
        );
    }
}
