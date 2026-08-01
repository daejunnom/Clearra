use clearra_app::{AppCommand, AppContext, AppRequest, ResourceBudget, SetupAppCommand};
use clearra_pc_graph::request::WorkerPolicy;

use crate::{
    args::setup_args::SetupArgs,
    assemble::SetupQueryAssembler,
    error::CliErrorCode,
    output::{AppResponseRenderer, CliOutput, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupCommand;

impl SetupCommand {
    pub fn run(args: &SetupArgs, format: RenderFormat) -> CliOutput {
        let query = match SetupQueryAssembler::assemble(args) {
            Ok(query) => query,
            Err(error) => {
                return CliOutput::error(CliErrorCode::SetupQueryInvalid, format!("{error:?}"));
            }
        };
        let workers = match args.workers() {
            Some(workers) => {
                WorkerPolicy::clamp_requested(workers, args.use_all_logical_processors())
            }
            None if args.use_all_logical_processors() => WorkerPolicy::hardware_worker_limit(),
            None => WorkerPolicy::default_worker_limit(),
        };
        let request =
            AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query))).with_resource_budget(
                ResourceBudget::new(u16::try_from(workers).unwrap_or(u16::MAX), None, None),
            );
        AppResponseRenderer::render(
            AppContext::default().run(request),
            format,
            CliErrorCode::SetupQueryInvalid,
        )
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "setup_command_tests.rs"]
mod tests;
