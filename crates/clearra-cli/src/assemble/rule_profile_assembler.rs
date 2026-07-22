use clearra_rules::{
    kicks::{KickImport, KickProfileVerificationReport, VerifiedKickTableProfile},
    profile::rule_profile::{RuleProfile, RuleProfileId},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleProfileAssemblyError {
    UnknownRuleProfile {
        value: String,
    },
    InvalidKickProfileJson {
        code: &'static str,
    },
    UnverifiedKickProfile {
        issue_count: usize,
        missing_transition_count: usize,
        duplicate_transition_count: usize,
        unsupported_annotation_count: usize,
    },
}

impl RuleProfileAssemblyError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownRuleProfile { value } => format!("unsupported rule '{value}'"),
            Self::InvalidKickProfileJson { code } => {
                format!("invalid kick profile JSON: {code}")
            }
            Self::UnverifiedKickProfile {
                issue_count,
                missing_transition_count,
                duplicate_transition_count,
                unsupported_annotation_count,
            } => format!(
                "kick profile must be verified before search: issue_count={issue_count}, missing_transition_count={missing_transition_count}, duplicate_transition_count={duplicate_transition_count}, unsupported_annotation_count={unsupported_annotation_count}"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuleProfileAssembler;

impl RuleProfileAssembler {
    pub fn parse_rule(value: &str) -> Result<RuleProfile, RuleProfileAssemblyError> {
        RuleProfileId::parse(normalize_rule(value).as_str())
            .map(RuleProfile::new)
            .ok_or_else(|| RuleProfileAssemblyError::UnknownRuleProfile {
                value: value.to_owned(),
            })
    }
}
impl RuleProfileAssembler {
    pub fn parse_optional_rule(
        value: Option<&str>,
        default: RuleProfile,
    ) -> Result<RuleProfile, RuleProfileAssemblyError> {
        value.map_or(Ok(default), Self::parse_rule)
    }
}
impl RuleProfileAssembler {
    pub fn parse_verified_kick_profile(
        value: Option<&str>,
    ) -> Result<Option<VerifiedKickTableProfile>, RuleProfileAssemblyError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let profile = KickImport::from_json(value).map_err(|error| {
            RuleProfileAssemblyError::InvalidKickProfileJson { code: error.code() }
        })?;
        VerifiedKickTableProfile::try_new(profile)
            .map(Some)
            .map_err(unverified_kick_profile_error)
    }
}

fn normalize_rule(value: &str) -> String {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "srs-90" => "srs".to_owned(),
        other => other.to_owned(),
    }
}

fn unverified_kick_profile_error(
    report: KickProfileVerificationReport,
) -> RuleProfileAssemblyError {
    RuleProfileAssemblyError::UnverifiedKickProfile {
        issue_count: report.issue_count(),
        missing_transition_count: report.missing_transition_count(),
        duplicate_transition_count: report.duplicate_transition_count(),
        unsupported_annotation_count: report.unsupported_annotation_count(),
    }
}

#[cfg(test)]
#[path = "rule_profile_assembler_tests.rs"]
mod tests;
