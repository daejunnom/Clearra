use clearra_rules::kicks::{KickImport, KickProfileCapability, VerifiedKickTableProfile};

use crate::{
    app_error::AppErrorCode,
    app_response::AppResponse,
    commands::rules_app_output::{
        builtin_kick_profile, invalid_import_profile, rules_error, rules_success,
    },
};

pub(super) fn import_rules(input: Option<&str>) -> AppResponse {
    let Some(input) = input else {
        return rules_error(
            AppErrorCode::RulesInputRequired,
            "rules import requires --input <json>",
        );
    };
    let profile = match KickImport::from_json(input) {
        Ok(profile) => profile,
        Err(error) => {
            return rules_error(
                AppErrorCode::RulesInputInvalid,
                format!("invalid kick profile JSON: {}", error.code()),
            )
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
    rules_success(vec![
        ("action".to_owned(), "import".to_owned()),
        ("profile".to_owned(), profile.id().as_str().to_owned()),
        (
            "source_rule".to_owned(),
            profile.source_rule().as_str().to_owned(),
        ),
        (
            "transition_count".to_owned(),
            profile.transition_count().to_string(),
        ),
        ("issue_count".to_owned(), report.issue_count().to_string()),
        (
            "transition_complete".to_owned(),
            report.transition_complete().to_string(),
        ),
        ("verified_profile".to_owned(), "true".to_owned()),
        ("supports_180".to_owned(), report.supports_180().to_string()),
        (
            "supports_exact_180".to_owned(),
            profile.supports_180().to_string(),
        ),
        (
            "search_backend_supported".to_owned(),
            capability.search_backend_supported().to_string(),
        ),
        (
            "c_compact_descriptor_ready".to_owned(),
            capability.c_compact_descriptor_ready().to_string(),
        ),
        (
            "c_kick_profile_id".to_owned(),
            profile.id().as_str().to_owned(),
        ),
        (
            "c_verified_transition_count".to_owned(),
            profile.transition_count().to_string(),
        ),
        (
            "c_verified_supports_180".to_owned(),
            profile.supports_180().to_string(),
        ),
        (
            "unsupported_backend_reason".to_owned(),
            capability.unsupported_reason().unwrap_or("none").to_owned(),
        ),
    ])
}

pub(super) fn export_rules(profile_id: Option<&str>) -> AppResponse {
    let Some(profile_id) = profile_id else {
        return rules_error(
            AppErrorCode::RulesProfileUnknown,
            "rules export requires --profile <id>",
        );
    };
    let Some(profile) = builtin_kick_profile(profile_id) else {
        return rules_error(
            AppErrorCode::RulesExportUnsupported,
            format!("kick profile '{profile_id}' is not exportable as a built-in profile"),
        );
    };
    let json = match KickImport::to_json(&profile) {
        Ok(json) => json,
        Err(error) => {
            return rules_error(
                AppErrorCode::RulesInputInvalid,
                format!("failed to export kick profile: {}", error.code()),
            )
        }
    };
    rules_success(vec![
        ("action".to_owned(), "export".to_owned()),
        ("profile".to_owned(), profile.id().as_str().to_owned()),
        ("json".to_owned(), json),
    ])
}
