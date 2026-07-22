use crate::{
    args::{RulesAction, RulesArgs},
    output::{CliOutput, RenderFormat},
    rules::{
        RulesExportAction, RulesImportAction, RulesInspectAction, RulesListAction,
        RulesVerifyAction,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RulesCommand;

impl RulesCommand {
    pub fn run(args: &RulesArgs, format: RenderFormat) -> CliOutput {
        match args.action() {
            RulesAction::List => RulesListAction::run(format),
            RulesAction::Inspect => RulesInspectAction::run(args, format),
            RulesAction::Verify => RulesVerifyAction::run(args, format),
            RulesAction::Import => RulesImportAction::run(args, format),
            RulesAction::Export => RulesExportAction::run(args, format),
        }
    }
}

#[cfg(test)]
#[path = "rules_command_tests.rs"]
mod tests;
