use crate::{
    args::{ScoringAction, ScoringArgs},
    output::{CliOutput, RenderFormat},
    scoring::{ScoringExportAction, ScoringImportAction, ScoringInspectAction, ScoringListAction},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoringCommand;

impl ScoringCommand {
    pub fn run(args: &ScoringArgs, format: RenderFormat) -> CliOutput {
        match args.action() {
            ScoringAction::List => ScoringListAction::run(format),
            ScoringAction::Inspect => ScoringInspectAction::run(args, format),
            ScoringAction::Import => ScoringImportAction::run(args, format),
            ScoringAction::Export => ScoringExportAction::run(args, format),
        }
    }
}

#[cfg(test)]
#[path = "scoring_command_tests.rs"]
mod tests;
