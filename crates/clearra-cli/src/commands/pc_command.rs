use clearra_app::{AppCommand, AppContext, AppRequest, PcAppCommand};

use crate::{
    args::pc_args::PcArgs,
    assemble::pc_query_assembler::{PcQueryAssembler, PcQueryAssemblyError},
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcCommand;

impl PcCommand {
    pub fn run(args: PcArgs, format: RenderFormat) -> CliOutput {
        let query = match PcQueryAssembler::assemble(&args) {
            Ok(query) => query,
            Err(PcQueryAssemblyError::InvalidTarget(error)) => {
                return CliOutput::error(CliErrorCode::PcTargetInvalid, format!("{error:?}"));
            }
            Err(PcQueryAssemblyError::UnsupportedMvpTarget { lines }) => {
                return CliOutput::error(
                    CliErrorCode::PcTargetUnsupportedMvp,
                    format!("only 2L, 4L, and 6L PC targets are supported in MVP1 (lines={lines})"),
                );
            }
            Err(PcQueryAssemblyError::UnknownPiece { index, value }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("unknown queue piece '{value}' at index {index}"),
                );
            }
            Err(PcQueryAssemblyError::UnsupportedObjective { value }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("unsupported PC objective '{value}'"),
                );
            }
            Err(PcQueryAssemblyError::UnsupportedScoreProfile { value }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("unsupported PC score profile '{value}'"),
                );
            }
            Err(PcQueryAssemblyError::UnsupportedSpinProfile { value }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("unsupported PC spin profile '{value}'"),
                );
            }
            Err(PcQueryAssemblyError::UnknownRuleProfile { value }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("unknown rule profile '{value}'"),
                );
            }
            Err(PcQueryAssemblyError::InvalidKickProfileJson { code }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!("invalid kick profile JSON: {code}"),
                );
            }
            Err(PcQueryAssemblyError::InvalidExecutionPolicy { message }) => {
                return CliOutput::error(CliErrorCode::PcQueryInvalid, message);
            }
            Err(PcQueryAssemblyError::UnverifiedKickProfile {
                issue_count,
                missing_transition_count,
                duplicate_transition_count,
                unsupported_annotation_count,
            }) => {
                return CliOutput::error(
                    CliErrorCode::PcQueryInvalid,
                    format!(
                        "kick profile must be verified before search: issue_count={issue_count}, missing_transition_count={missing_transition_count}, duplicate_transition_count={duplicate_transition_count}, unsupported_annotation_count={unsupported_annotation_count}"
                    ),
                );
            }
        };
        AppResponseRenderer::render(
            AppContext::default().run(AppRequest::new(AppCommand::Pc(PcAppCommand::new(query)))),
            format,
            CliErrorCode::PcSearchInternal,
        )
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "pc_command_tests.rs"]
mod tests;
