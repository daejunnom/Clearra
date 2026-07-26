use clearra_rules::{
    kicks::{KickContractReport, KickImport, KickProfileCapability, KickProfileRegistry},
    profile::{rule_capability::RuleCapability, rule_profile::RuleProfile},
};

use crate::{
    app_error::AppErrorCode,
    app_response::AppResponse,
    commands::rules_app_output::{
        capability_fields, rules_error, rules_success, unsupported_backend_reason,
        verification_status,
    },
};

pub(super) fn list_fields() -> Vec<(String, String)> {
    let mut fields = vec![("action".to_owned(), "list".to_owned())];
    let profiles = KickProfileRegistry::builtin_profiles();
    fields.push(("profile_count".to_owned(), profiles.len().to_string()));
    for (index, descriptor) in profiles.iter().enumerate() {
        let prefix = format!("profile_{index}_");
        fields.push((
            format!("{prefix}id"),
            descriptor.rule_profile_id().as_str().to_owned(),
        ));
        fields.push((
            format!("{prefix}kick_profile"),
            descriptor.id().as_str().to_owned(),
        ));
        fields.push((format!("{prefix}label"), descriptor.label().to_owned()));
        fields.push((
            format!("{prefix}source_kind"),
            descriptor.source_kind().as_str().to_owned(),
        ));
        fields.push((
            format!("{prefix}source_description"),
            descriptor.source_description().to_owned(),
        ));
        fields.extend(capability_fields(&prefix, descriptor.capability()));
    }
    fields
}

pub(super) fn inspect_rules(profile_id: Option<&str>) -> AppResponse {
    let Some(profile_id) = profile_id else {
        return rules_error(
            AppErrorCode::RulesProfileUnknown,
            "rules inspect requires --profile <id>",
        );
    };
    let Some(descriptor) = KickProfileRegistry::builtin_profiles()
        .into_iter()
        .find(|descriptor| {
            descriptor.rule_profile_id().as_str() == profile_id
                || descriptor.id().as_str() == profile_id
        })
    else {
        return rules_error(
            AppErrorCode::RulesProfileUnknown,
            format!("unknown rule profile '{profile_id}'"),
        );
    };
    let capability = RuleCapability::from_rule(RuleProfile::new(descriptor.rule_profile_id()));
    let kick_capability = descriptor.capability();
    let mut fields = vec![
        ("action".to_owned(), "inspect".to_owned()),
        (
            "rule_profile".to_owned(),
            descriptor.rule_profile_id().as_str().to_owned(),
        ),
        (
            "kick_profile".to_owned(),
            descriptor.id().as_str().to_owned(),
        ),
        ("label".to_owned(), descriptor.label().to_owned()),
        (
            "source_kind".to_owned(),
            descriptor.source_kind().as_str().to_owned(),
        ),
        (
            "source_description".to_owned(),
            descriptor.source_description().to_owned(),
        ),
        (
            "effective_kick_model".to_owned(),
            capability.kick_model().as_str().to_owned(),
        ),
        (
            "supports_180".to_owned(),
            capability.supports_180().to_string(),
        ),
        (
            "supports_exact_180".to_owned(),
            kick_capability.supports_exact_180().to_string(),
        ),
        (
            "requires_lock_reachability".to_owned(),
            capability.requires_lock_reachability().to_string(),
        ),
        (
            "requires_spawn_reachability".to_owned(),
            capability.requires_spawn_reachability().to_string(),
        ),
        (
            "search_backend_supported".to_owned(),
            capability.search_backend_supported().to_string(),
        ),
        (
            "c_compact_descriptor_ready".to_owned(),
            kick_capability.c_compact_descriptor_ready().to_string(),
        ),
        (
            "unsupported_backend_reason".to_owned(),
            kick_capability
                .unsupported_reason()
                .unwrap_or("none")
                .to_owned(),
        ),
    ];
    if let Some(reason) = capability.unsupported_reason() {
        fields.push(("unsupported_reason".to_owned(), reason.to_owned()));
    }
    rules_success(fields)
}

pub(super) fn verify_rules(input: Option<&str>) -> AppResponse {
    if let Some(input) = input {
        let profile = match KickImport::from_json(input) {
            Ok(profile) => profile,
            Err(error) => {
                return rules_error(
                    AppErrorCode::RulesInputInvalid,
                    format!("invalid kick profile JSON: {}", error.code()),
                )
            }
        };
        let report = KickImport::verify_imported_profile(&profile);
        let capability =
            KickProfileCapability::imported_verified(profile.source_rule(), report.supports_180());
        let verified_profile = report.issue_count() == 0;
        return rules_success(vec![
            ("action".to_owned(), "verify".to_owned()),
            ("profile".to_owned(), profile.id().as_str().to_owned()),
            (
                "source_rule".to_owned(),
                profile.source_rule().as_str().to_owned(),
            ),
            (
                "verification_status".to_owned(),
                verification_status(&report).to_owned(),
            ),
            ("issue_count".to_owned(), report.issue_count().to_string()),
            (
                "missing_transition_count".to_owned(),
                report.missing_transition_count().to_string(),
            ),
            (
                "duplicate_transition_count".to_owned(),
                report.duplicate_transition_count().to_string(),
            ),
            (
                "unsupported_annotation_count".to_owned(),
                report.unsupported_annotation_count().to_string(),
            ),
            ("supports_180".to_owned(), report.supports_180().to_string()),
            (
                "transition_complete".to_owned(),
                report.transition_complete().to_string(),
            ),
            ("verified_profile".to_owned(), verified_profile.to_string()),
            (
                "supports_exact_180".to_owned(),
                (verified_profile && capability.supports_exact_180()).to_string(),
            ),
            (
                "c_compact_descriptor_ready".to_owned(),
                (verified_profile && capability.c_compact_descriptor_ready()).to_string(),
            ),
            (
                "unsupported_backend_reason".to_owned(),
                unsupported_backend_reason(verified_profile, capability).to_owned(),
            ),
        ]);
    }

    let report = KickContractReport::verify_builtin_contracts();
    rules_success(vec![
        ("action".to_owned(), "verify".to_owned()),
        ("profile".to_owned(), "builtins".to_owned()),
        (
            "kick_profile_registry_count".to_owned(),
            report.profile_registry_count().to_string(),
        ),
        (
            "kick_verification_cases".to_owned(),
            report.verification_case_count().to_string(),
        ),
        (
            "kick_verification_failures".to_owned(),
            report.verification_failure_count().to_string(),
        ),
        (
            "srs_plus_180_transitions".to_owned(),
            report.srs_plus_180_transition_count().to_string(),
        ),
        (
            "jstris_180_transitions".to_owned(),
            report.jstris_180_transition_count().to_string(),
        ),
    ])
}
