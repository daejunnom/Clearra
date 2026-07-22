use clearra_app::{RulesAppCommand, ScoringAppCommand};

use crate::args::{RulesArgs, ScoringArgs};

pub(crate) fn rules_command(args: &RulesArgs) -> RulesAppCommand {
    RulesAppCommand::new(args.action().as_str())
        .with_profile(args.profile().map(ToOwned::to_owned))
        .with_input(args.input().map(ToOwned::to_owned))
}

pub(crate) fn scoring_command(args: &ScoringArgs) -> ScoringAppCommand {
    ScoringAppCommand::new(args.action().as_str())
        .with_profile(args.profile().map(ToOwned::to_owned))
        .with_input(args.input().map(ToOwned::to_owned))
}
