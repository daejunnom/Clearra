use clearra_rules::kicks::{
    KickProfileCapability, KickProfileVerificationReport, KickTableProfile, KickTableProfileId,
    NoKick, SrsKicks,
};

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::string_field,
    render::{AppMessage, AppRenderModel, AppResultKind},
};

pub(super) fn rules_success(fields: Vec<(String, String)>) -> AppResponse {
    AppResponse::success(AppRenderModel::Rules(AppMessage::new(
        AppResultKind::Rules,
        fields
            .into_iter()
            .map(|(key, value)| string_field(key, value))
            .collect(),
    )))
}

pub(super) fn rules_error(code: AppErrorCode, message: impl Into<String>) -> AppResponse {
    AppResponse::failed(AppStatus::ExecutionFailed, AppError::new(code, message))
}

pub(super) fn capability_fields(
    prefix: &str,
    capability: KickProfileCapability,
) -> Vec<(String, String)> {
    let mut fields = vec![
        (
            format!("{prefix}supports_180"),
            capability.supports_180().to_string(),
        ),
        (
            format!("{prefix}supports_exact_180"),
            capability.supports_exact_180().to_string(),
        ),
        (
            format!("{prefix}search_backend_supported"),
            capability.search_backend_supported().to_string(),
        ),
        (
            format!("{prefix}c_compact_descriptor_ready"),
            capability.c_compact_descriptor_ready().to_string(),
        ),
        (
            format!("{prefix}unsupported_backend_reason"),
            capability.unsupported_reason().unwrap_or("none").to_owned(),
        ),
    ];
    if let Some(reason) = capability.unsupported_reason() {
        fields.push((format!("{prefix}unsupported_reason"), reason.to_owned()));
    }
    fields
}

pub(super) fn builtin_kick_profile(profile_id: &str) -> Option<KickTableProfile> {
    match KickTableProfileId::parse(profile_id)? {
        KickTableProfileId::Srs90 => Some(SrsKicks::profile()),
        KickTableProfileId::SrsPlus => Some(SrsKicks::srs_plus_profile()),
        KickTableProfileId::SrsX => Some(SrsKicks::srs_x_profile()),
        KickTableProfileId::Jstris180 => Some(SrsKicks::jstris_180_profile()),
        KickTableProfileId::NoKick => Some(NoKick::profile()),
        KickTableProfileId::Asc
        | KickTableProfileId::Ars
        | KickTableProfileId::Imported
        | KickTableProfileId::Custom => None,
    }
}

pub(super) fn invalid_import_profile(report: KickProfileVerificationReport) -> AppResponse {
    rules_error(
        AppErrorCode::RulesInputInvalid,
        format!(
            "imported kick profile is not verified: issue_count={}, missing_transition_count={}, duplicate_transition_count={}, unsupported_annotation_count={}",
            report.issue_count(),
            report.missing_transition_count(),
            report.duplicate_transition_count(),
            report.unsupported_annotation_count()
        ),
    )
}

pub(super) fn verification_status(report: &KickProfileVerificationReport) -> &'static str {
    if report.issue_count() == 0 {
        "verified"
    } else {
        "issues"
    }
}

pub(super) fn unsupported_backend_reason(
    verified_profile: bool,
    capability: KickProfileCapability,
) -> &'static str {
    if !verified_profile {
        return "kick_profile_verification_failed";
    }
    capability.unsupported_reason().unwrap_or("none")
}
