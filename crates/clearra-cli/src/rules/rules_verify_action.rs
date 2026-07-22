use clearra_rules::kicks::{
    KickContractReport, KickImport, KickProfileCapability, KickProfileVerificationReport,
};

use crate::{
    args::RulesArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    rules::rules_output_fields::render_rules,
};

pub(crate) struct RulesVerifyAction;

impl RulesVerifyAction {
    pub(crate) fn run(args: &RulesArgs, format: RenderFormat) -> CliOutput {
        if let Some(input) = args.input() {
            let profile = match KickImport::from_json(input) {
                Ok(profile) => profile,
                Err(error) => {
                    return CliOutput::error(
                        CliErrorCode::RulesInputInvalid,
                        format!("invalid kick profile JSON: {}", error.code()),
                    );
                }
            };
            let report = KickImport::verify_imported_profile(&profile);
            let capability = KickProfileCapability::imported_verified(
                profile.source_rule(),
                report.supports_180(),
            );
            let verified_profile = report.issue_count() == 0;
            return render_rules(
                vec![
                    ("action", "verify".to_owned()),
                    ("profile", profile.id().as_str().to_owned()),
                    ("source_rule", profile.source_rule().as_str().to_owned()),
                    (
                        "verification_status",
                        verification_status(&report).to_owned(),
                    ),
                    ("issue_count", report.issue_count().to_string()),
                    (
                        "missing_transition_count",
                        report.missing_transition_count().to_string(),
                    ),
                    (
                        "duplicate_transition_count",
                        report.duplicate_transition_count().to_string(),
                    ),
                    (
                        "unsupported_annotation_count",
                        report.unsupported_annotation_count().to_string(),
                    ),
                    ("supports_180", report.supports_180().to_string()),
                    (
                        "transition_complete",
                        report.transition_complete().to_string(),
                    ),
                    ("verified_profile", verified_profile.to_string()),
                    (
                        "supports_exact_180",
                        (verified_profile && capability.supports_exact_180()).to_string(),
                    ),
                    (
                        "c_compact_descriptor_ready",
                        (verified_profile && capability.c_compact_descriptor_ready()).to_string(),
                    ),
                    (
                        "unsupported_backend_reason",
                        unsupported_backend_reason(verified_profile, capability).to_owned(),
                    ),
                ],
                format,
            );
        }

        let report = KickContractReport::verify_builtin_contracts();
        render_rules(
            vec![
                ("action", "verify".to_owned()),
                ("profile", "builtins".to_owned()),
                (
                    "kick_profile_registry_count",
                    report.profile_registry_count().to_string(),
                ),
                (
                    "kick_verification_cases",
                    report.verification_case_count().to_string(),
                ),
                (
                    "kick_verification_failures",
                    report.verification_failure_count().to_string(),
                ),
                (
                    "srs_plus_180_transitions",
                    report.srs_plus_180_transition_count().to_string(),
                ),
            ],
            format,
        )
    }
}

fn verification_status(report: &KickProfileVerificationReport) -> &'static str {
    if report.issue_count() == 0 {
        "verified"
    } else {
        "issues"
    }
}

fn unsupported_backend_reason(
    verified_profile: bool,
    capability: KickProfileCapability,
) -> &'static str {
    if !verified_profile {
        return "kick_profile_verification_failed";
    }

    capability.unsupported_reason().unwrap_or("none")
}
