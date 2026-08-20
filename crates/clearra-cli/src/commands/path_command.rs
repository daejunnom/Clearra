use clearra_app::{AppCommand, AppContext, AppRequest, PathAppCommand};

use crate::{
    args::PathArgs,
    assemble::pc_query_assembler::{PcQueryAssembler, PcQueryAssemblyError},
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PathCommand;

impl PathCommand {
    pub fn run(args: &PathArgs, format: RenderFormat) -> CliOutput {
        let query = match PcQueryAssembler::assemble(args.pc()) {
            Ok(query) => query,
            Err(error) => return pc_assembly_error(error),
        };
        AppResponseRenderer::render(
            AppContext::default().run(AppRequest::new(AppCommand::Path(PathAppCommand::new(
                query,
            )))),
            format,
            CliErrorCode::PathSearchInternal,
        )
    }
}

fn pc_assembly_error(error: PcQueryAssemblyError) -> CliOutput {
    match error {
        PcQueryAssemblyError::SearchContract(error) => CliOutput::error(
            CliErrorCode::PcQueryInvalid,
            format!("{}: {}", error.code(), error.message()),
        ),
        PcQueryAssemblyError::InvalidTarget(error) => {
            CliOutput::error(CliErrorCode::PcTargetInvalid, format!("{error:?}"))
        }
        PcQueryAssemblyError::UnsupportedMvpTarget { lines } => CliOutput::error(
            CliErrorCode::PcTargetUnsupportedMvp,
            format!("only 2L, 4L, and 6L PC targets are supported in MVP2 (lines={lines})"),
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

#[cfg(all(test, feature = "native-c-core"))]
#[path = "path_command_tests.rs"]
mod tests;
