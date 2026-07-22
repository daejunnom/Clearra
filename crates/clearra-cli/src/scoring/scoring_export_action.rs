use clearra_scoring::{export::ScoreProfileExport, profile::ScoreProfileRegistry};

use crate::{
    args::ScoringArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    scoring::scoring_output_fields::render_scoring,
};

pub(crate) struct ScoringExportAction;

impl ScoringExportAction {
    pub(crate) fn run(args: &ScoringArgs, format: RenderFormat) -> CliOutput {
        let Some(profile_id) = args.profile() else {
            return CliOutput::error(
                CliErrorCode::ScoringProfileUnknown,
                "scoring export requires --profile <id>",
            );
        };
        let registry = ScoreProfileRegistry::builtins();
        let Some(profile) = registry.get(profile_id) else {
            return CliOutput::error(
                CliErrorCode::ScoringProfileUnknown,
                format!("unknown scoring profile '{profile_id}'"),
            );
        };
        let json = match ScoreProfileExport::to_json(profile) {
            Ok(json) => json,
            Err(error) => {
                return CliOutput::error(
                    CliErrorCode::ScoringInputInvalid,
                    format!("failed to export score profile: {error:?}"),
                );
            }
        };

        render_scoring(
            vec![
                ("action", "export".to_owned()),
                ("profile", profile.id().to_owned()),
                ("json", json),
            ],
            format,
        )
    }
}
