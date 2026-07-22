use clearra_rules::{
    kicks::KickTableProfile,
    profile::{rule_capability::RuleCapability, rule_profile::RuleProfile},
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(super) fn supported_rule_diagnostic(
    rule: RuleProfile,
    capability: &RuleCapability,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IRuleMvpSupported,
        "rule profile is supported by MVP2 search capability",
    )
    .with_location(EvidenceLocation::new("rules.profile"))
    .with_evidence(ValidationEvidence::new("rule", format!("{:?}", rule.id())))
    .with_evidence(ValidationEvidence::new(
        "effective_kick_model",
        capability.kick_model().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "supports_180",
        capability.supports_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "supports_exact_180",
        capability.supports_exact_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "c_compact_descriptor_ready",
        capability.search_backend_supported().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "requires_spawn_reachability",
        capability.requires_spawn_reachability().to_string(),
    ))
}

pub(super) fn unsupported_rule_diagnostic(
    rule: RuleProfile,
    capability: &RuleCapability,
) -> Diagnostic {
    let unsupported_reason = capability
        .unsupported_reason()
        .unwrap_or("rule_profile_unsupported");
    Diagnostic::new(
        DiagnosticCode::ERuleUnsupportedMvp,
        format!(
            "custom or unsupported rule profiles require an imported kick profile or backend support ({unsupported_reason})"
        ),
    )
    .with_location(EvidenceLocation::new("rules.profile"))
    .with_evidence(ValidationEvidence::new("rule", format!("{:?}", rule.id())))
    .with_evidence(ValidationEvidence::new(
        "supports_180",
        capability.supports_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "supports_exact_180",
        capability.supports_exact_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "c_compact_descriptor_ready",
        capability.search_backend_supported().to_string(),
    ))
    .with_evidence(ValidationEvidence::new("reason", unsupported_reason))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Use SRS+, SRS, or NoKick, or import a verified KickTableProfile for extension rules.",
    ))
}

pub(super) fn verified_profile_rule_mismatch_diagnostic(
    rule: RuleProfile,
    profile: &KickTableProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ERuleUnsupportedMvp,
        "verified kick profile source rule must match the query rule profile",
    )
    .with_location(EvidenceLocation::new("rules.verified_kick_profile"))
    .with_evidence(ValidationEvidence::new("rule", rule.id().as_str()))
    .with_evidence(ValidationEvidence::new(
        "profile_source_rule",
        profile.source_rule().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "reason",
        "verified_profile_rule_mismatch",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Attach a verified KickTableProfile whose source_rule matches the query rule.",
    ))
}

pub(super) fn spawn_aware_verified_profile_unsupported_diagnostic(
    rule: RuleProfile,
    profile: &KickTableProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ERuleUnsupportedMvp,
        "verified kick profile uses a spawn-aware rule profile that the current search backend does not support",
    )
    .with_location(EvidenceLocation::new("rules.verified_kick_profile"))
    .with_evidence(ValidationEvidence::new("rule", rule.id().as_str()))
    .with_evidence(ValidationEvidence::new("kick_profile", profile.id().as_str()))
    .with_evidence(ValidationEvidence::new(
        "reason",
        "spawn_aware_profile_unsupported",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Use SRS, SRS+, NoKick, SRS-X with a verified non-spawn-aware kick table, or wait for spawn-aware search support.",
    ))
}

pub(super) fn verified_profile_missing_required_180_diagnostic(
    rule: RuleProfile,
    profile: &KickTableProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ERuleUnsupportedMvp,
        "verified kick profile must include 180-degree transitions for this rule profile",
    )
    .with_location(EvidenceLocation::new("rules.verified_kick_profile"))
    .with_evidence(ValidationEvidence::new("rule", rule.id().as_str()))
    .with_evidence(ValidationEvidence::new(
        "kick_profile",
        profile.id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new("supports_180", "false"))
    .with_evidence(ValidationEvidence::new("supports_exact_180", "false"))
    .with_evidence(ValidationEvidence::new(
        "reason",
        "verified_profile_missing_required_180",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Import a verified exact 180 kick profile before enabling this rule in search.",
    ))
}

pub(super) fn verified_profile_supported_diagnostic(
    rule: RuleProfile,
    profile: &KickTableProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IRuleMvpSupported,
        "verified imported kick profile is supported by the MVP2 search policy",
    )
    .with_location(EvidenceLocation::new("rules.verified_kick_profile"))
    .with_evidence(ValidationEvidence::new("rule", rule.id().as_str()))
    .with_evidence(ValidationEvidence::new(
        "kick_profile",
        profile.id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "effective_kick_model",
        "imported-kick-profile",
    ))
    .with_evidence(ValidationEvidence::new("verified_profile", "true"))
    .with_evidence(ValidationEvidence::new(
        "supports_180",
        profile.supports_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "supports_exact_180",
        profile.supports_180().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "c_compact_descriptor_ready",
        "true",
    ))
}
