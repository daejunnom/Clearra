use clearra_rules::kicks::KickImport;

use crate::{
    args::RulesArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    rules::rules_output_fields::{builtin_kick_profile, render_rules},
};

pub(crate) struct RulesExportAction;

impl RulesExportAction {
    pub(crate) fn run(args: &RulesArgs, format: RenderFormat) -> CliOutput {
        let Some(profile_id) = args.profile() else {
            return CliOutput::error(
                CliErrorCode::RulesProfileUnknown,
                "rules export requires --profile <id>",
            );
        };
        let Some(profile) = builtin_kick_profile(profile_id) else {
            return CliOutput::error(
                CliErrorCode::RulesExportUnsupported,
                format!("kick profile '{profile_id}' is not exportable as a built-in profile"),
            );
        };
        let json = match KickImport::to_json(&profile) {
            Ok(json) => json,
            Err(error) => {
                return CliOutput::error(
                    CliErrorCode::RulesInputInvalid,
                    format!("failed to export kick profile: {}", error.code()),
                );
            }
        };

        render_rules(
            vec![
                ("action", "export".to_owned()),
                ("profile", profile.id().as_str().to_owned()),
                ("json", json),
            ],
            format,
        )
    }
}
