use crate::{
    assemble::{PcQueryAssemblyError, PercentQueryAssemblyError},
    error::CliErrorCode,
    output::CliOutput,
};

pub(crate) fn pc_assembly_error(error: PcQueryAssemblyError) -> CliOutput {
    match error {
        PcQueryAssemblyError::InvalidTarget(error) => {
            CliOutput::error(CliErrorCode::PcTargetInvalid, format!("{error:?}"))
        }
        PcQueryAssemblyError::UnsupportedMvpTarget { lines } => CliOutput::error(
            CliErrorCode::PcTargetUnsupportedMvp,
            format!("only 2L, 4L, and 6L PC targets are supported in MVP1 (lines={lines})"),
        ),
        PcQueryAssemblyError::UnknownPiece { index, value } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("unknown queue piece '{value}' at index {index}"),
        ),
        PcQueryAssemblyError::UnsupportedObjective { value } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("unsupported PC objective '{value}'"),
        ),
        PcQueryAssemblyError::UnsupportedScoreProfile { value } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("unsupported PC score profile '{value}'"),
        ),
        PcQueryAssemblyError::UnsupportedSpinProfile { value } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("unsupported PC spin profile '{value}'"),
        ),
        PcQueryAssemblyError::IncompatibleTilingOnlyOption { option } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("{option} is not available with tiling-only search"),
        ),
        PcQueryAssemblyError::UnknownRuleProfile { value } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("unknown rule profile '{value}'"),
        ),
        PcQueryAssemblyError::InvalidKickProfileJson { code } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("invalid kick profile JSON: {code}"),
        ),
        PcQueryAssemblyError::InvalidExecutionPolicy { message } => {
            CliOutput::error(CliErrorCode::PcQueryInvalid, message)
        }
        PcQueryAssemblyError::UnverifiedKickProfile {
            issue_count,
            missing_transition_count,
            duplicate_transition_count,
            unsupported_annotation_count,
        } => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!(
                "kick profile must be verified before search: issue_count={issue_count}, missing_transition_count={missing_transition_count}, duplicate_transition_count={duplicate_transition_count}, unsupported_annotation_count={unsupported_annotation_count}"
            ),
        ),
    }
}

pub(crate) fn percent_assembly_error(error: PercentQueryAssemblyError) -> CliOutput {
    let message = match error {
        PercentQueryAssemblyError::InvalidObservedQueue => "invalid observed queue",
        PercentQueryAssemblyError::InvalidBagAlignedPattern => "invalid bag-aligned pattern",
        PercentQueryAssemblyError::InvalidFixedSequence => "invalid fixed sequence",
    };
    CliOutput::error(CliErrorCode::PercentQueryInvalid, message)
}
