use clearra_app::{AppCommand, AppContext, AppRequest, PercentAppCommand};

use crate::{
    args::PercentArgs,
    assemble::{PercentQueryAssembler, PercentQueryAssemblyError},
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PercentCommand;

impl PercentCommand {
    pub fn run(args: &PercentArgs, format: RenderFormat) -> CliOutput {
        let assembly = match PercentQueryAssembler::assemble(args) {
            Ok(assembly) => assembly,
            Err(error) => return percent_assembly_error(error),
        };
        AppResponseRenderer::render(
            AppContext::default().run(AppRequest::new(AppCommand::Percent(
                PercentAppCommand::new(assembly.query().clone()),
            ))),
            format,
            CliErrorCode::PercentQueryInvalid,
        )
    }
}

fn percent_assembly_error(error: PercentQueryAssemblyError) -> CliOutput {
    let message = match error {
        PercentQueryAssemblyError::InvalidObservedQueue => "invalid observed queue",
        PercentQueryAssemblyError::InvalidBagAlignedPattern => "invalid bag-aligned pattern",
        PercentQueryAssemblyError::InvalidFixedSequence => "invalid fixed sequence",
    };
    CliOutput::error(CliErrorCode::PercentQueryInvalid, message)
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "percent_command_tests.rs"]
mod tests;
