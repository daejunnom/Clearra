use clearra_scoring::import::ScoreProfileImport;
use clearra_validation::validators::score_profile_validator::validate_score_profile;

use crate::{
    args::ScoringArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    scoring::scoring_output_fields::{profile_fields, render_scoring},
};

pub(crate) struct ScoringImportAction;

impl ScoringImportAction {
    pub(crate) fn run(args: &ScoringArgs, format: RenderFormat) -> CliOutput {
        let Some(input) = args.input() else {
            return CliOutput::error(
                CliErrorCode::ScoringInputRequired,
                "scoring import requires --input <json>",
            );
        };
        let profile = match ScoreProfileImport::from_json(input) {
            Ok(profile) => profile,
            Err(error) => {
                return CliOutput::error(
                    CliErrorCode::ScoringInputInvalid,
                    format!("invalid score profile JSON: {}", error.code()),
                );
            }
        };
        let report = validate_score_profile(&profile);
        if report.has_errors() {
            return CliOutput::validation_failed_with_format(&report, format);
        }

        let mut fields = vec![
            ("action".to_owned(), "import".to_owned()),
            (
                "diagnostic_count".to_owned(),
                report.diagnostics().len().to_string(),
            ),
        ];
        fields.extend(profile_fields(&profile, None));
        render_scoring(fields, format)
    }
}
