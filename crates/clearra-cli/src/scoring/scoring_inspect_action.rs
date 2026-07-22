use clearra_scoring::profile::ScoreProfileRegistry;

use crate::{
    args::ScoringArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    scoring::scoring_output_fields::{profile_fields, render_scoring},
};

pub(crate) struct ScoringInspectAction;

impl ScoringInspectAction {
    pub(crate) fn run(args: &ScoringArgs, format: RenderFormat) -> CliOutput {
        let Some(profile_id) = args.profile() else {
            return CliOutput::error(
                CliErrorCode::ScoringProfileUnknown,
                "scoring inspect requires --profile <id>",
            );
        };
        let registry = ScoreProfileRegistry::builtins();
        let Some(profile) = registry.get(profile_id) else {
            return CliOutput::error(
                CliErrorCode::ScoringProfileUnknown,
                format!("unknown scoring profile '{profile_id}'"),
            );
        };
        let mut fields = vec![("action".to_owned(), "inspect".to_owned())];
        fields.extend(profile_fields(profile, None));
        render_scoring(fields, format)
    }
}
