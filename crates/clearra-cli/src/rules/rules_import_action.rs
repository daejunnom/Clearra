use clearra_rules::kicks::{
    KickImport, KickProfileCapability, KickProfileVerificationReport, VerifiedKickTableProfile,
};

use crate::{
    args::RulesArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    rules::rules_output_fields::render_rules,
};

pub(crate) struct RulesImportAction;

impl RulesImportAction {
    pub(crate) fn run(args: &RulesArgs, format: RenderFormat) -> CliOutput {
        let Some(input) = args.input() else {
            return CliOutput::error(
                CliErrorCode::RulesInputRequired,
                "rules import requires --input <json>",
            );
        };
        let profile = match KickImport::from_json(input) {
            Ok(profile) => profile,
            Err(error) => {
                return CliOutput::error(
                    CliErrorCode::RulesInputInvalid,
                    format!("invalid kick profile JSON: {}", error.code()),
                );
            }
        };
        let verified = match VerifiedKickTableProfile::try_new(profile) {
            Ok(verified) => verified,
            Err(report) => return invalid_import_profile(report),
        };
        let profile = verified.profile();
        let report = verified.report();
        let capability =
            KickProfileCapability::imported_verified(profile.source_rule(), profile.supports_180());
        render_rules(
            vec![
                ("action", "import".to_owned()),
                ("profile", profile.id().as_str().to_owned()),
                ("source_rule", profile.source_rule().as_str().to_owned()),
                ("transition_count", profile.transition_count().to_string()),
                ("issue_count", report.issue_count().to_string()),
                (
                    "transition_complete",
                    report.transition_complete().to_string(),
                ),
                ("verified_profile", "true".to_owned()),
                ("supports_180", report.supports_180().to_string()),
                ("supports_exact_180", profile.supports_180().to_string()),
                (
                    "search_backend_supported",
                    capability.search_backend_supported().to_string(),
                ),
                (
                    "c_compact_descriptor_ready",
                    capability.c_compact_descriptor_ready().to_string(),
                ),
                ("c_kick_profile_id", profile.id().as_str().to_owned()),
                (
                    "c_verified_transition_count",
                    profile.transition_count().to_string(),
                ),
                (
                    "c_verified_supports_180",
                    profile.supports_180().to_string(),
                ),
                (
                    "unsupported_backend_reason",
                    capability.unsupported_reason().unwrap_or("none").to_owned(),
                ),
            ],
            format,
        )
    }
}

fn invalid_import_profile(report: KickProfileVerificationReport) -> CliOutput {
    CliOutput::error(
        CliErrorCode::RulesInputInvalid,
        format!(
            "imported kick profile is not verified: issue_count={}, missing_transition_count={}, duplicate_transition_count={}, unsupported_annotation_count={}",
            report.issue_count(),
            report.missing_transition_count(),
            report.duplicate_transition_count(),
            report.unsupported_annotation_count()
        ),
    )
}
