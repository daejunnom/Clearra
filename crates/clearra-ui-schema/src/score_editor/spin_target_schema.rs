use clearra_i18n::TranslationKey;
use clearra_scoring::spin::{RequiredSpinKind, SpinMiniPolicy};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{dropdown::DropdownOption, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinTargetSchema {
    options: Vec<SpinTargetOptionSchema>,
    mini_policy_options: Vec<DropdownOption>,
    accuracy_options: Vec<DropdownOption>,
    result_contract_keys: Vec<&'static str>,
}

impl SpinTargetSchema {
    pub fn mvp2() -> Self {
        Self {
            options: spin_target_options(),
            mini_policy_options: mini_policy_options(),
            accuracy_options: spin_target_accuracy_options(),
            result_contract_keys: vec![
                "spin_target_id",
                "spin_target_name",
                "covered_pattern_count",
                "pattern_count",
                "pattern_universe_id",
                "pattern_weight_model_id",
                "probability",
                "probability_complete",
                "materialized_probability_mass",
                "renormalized",
                "truncation_reason",
                "spin_accuracy",
                "trace_completeness",
                "score_profile_id",
            ],
        }
    }
}
impl SpinTargetSchema {
    pub fn options(&self) -> &[SpinTargetOptionSchema] {
        &self.options
    }
}
impl SpinTargetSchema {
    pub fn mini_policy_options(&self) -> &[DropdownOption] {
        &self.mini_policy_options
    }
}
impl SpinTargetSchema {
    pub fn accuracy_options(&self) -> &[DropdownOption] {
        &self.accuracy_options
    }
}
impl SpinTargetSchema {
    pub fn result_contract_keys(&self) -> &[&'static str] {
        &self.result_contract_keys
    }
}

impl Default for SpinTargetSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinTargetOptionSchema {
    id: &'static str,
    label: &'static str,
    localized_label: LocalizedLabelSchema,
    spin_kind: RequiredSpinKind,
    default_clear_lines: Option<u8>,
    requires_score_profile: bool,
    requires_kick_evidence: bool,
}

impl SpinTargetOptionSchema {
    pub fn new(
        id: &'static str,
        label: &'static str,
        spin_kind: RequiredSpinKind,
        default_clear_lines: Option<u8>,
    ) -> Self {
        Self {
            id,
            label,
            localized_label: LocalizedLabelSchema::new(
                TranslationKey::new(format!("ui.spin_target.{id}.label")),
                label,
            ),
            spin_kind,
            default_clear_lines,
            requires_score_profile: false,
            requires_kick_evidence: false,
        }
    }
}
impl SpinTargetOptionSchema {
    pub fn requiring_score_profile(mut self) -> Self {
        self.requires_score_profile = true;
        self
    }
}
impl SpinTargetOptionSchema {
    pub fn requiring_kick_evidence(mut self) -> Self {
        self.requires_kick_evidence = true;
        self
    }
}
impl SpinTargetOptionSchema {
    pub fn id(&self) -> &'static str {
        self.id
    }
}
impl SpinTargetOptionSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl SpinTargetOptionSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl SpinTargetOptionSchema {
    pub fn spin_kind(&self) -> RequiredSpinKind {
        self.spin_kind
    }
}
impl SpinTargetOptionSchema {
    pub fn default_clear_lines(&self) -> Option<u8> {
        self.default_clear_lines
    }
}
impl SpinTargetOptionSchema {
    pub fn requires_score_profile(&self) -> bool {
        self.requires_score_profile
    }
}
impl SpinTargetOptionSchema {
    pub fn requires_kick_evidence(&self) -> bool {
        self.requires_kick_evidence
    }
}

fn spin_target_options() -> Vec<SpinTargetOptionSchema> {
    vec![
        SpinTargetOptionSchema::new(
            "t-spin-double",
            "T-spin Double",
            RequiredSpinKind::TSpin,
            Some(2),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "t-spin-triple",
            "T-spin Triple",
            RequiredSpinKind::TSpin,
            Some(3),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "t-spin-mini-double",
            "T-spin Mini Double",
            RequiredSpinKind::TSpinMini,
            Some(2),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "all-spin-single",
            "All-spin Single",
            RequiredSpinKind::AllSpin,
            Some(1),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "all-spin-double",
            "All-spin Double",
            RequiredSpinKind::AllSpin,
            Some(2),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "all-spin-triple",
            "All-spin Triple",
            RequiredSpinKind::AllSpin,
            Some(3),
        )
        .requiring_kick_evidence(),
        SpinTargetOptionSchema::new(
            "profile-specific-spin",
            "Profile-specific spin",
            RequiredSpinKind::ProfileSpecific("profile-specific"),
            None,
        )
        .requiring_score_profile()
        .requiring_kick_evidence(),
    ]
}

fn mini_policy_options() -> Vec<DropdownOption> {
    [
        ("regular-only", "Regular only", SpinMiniPolicy::RegularOnly),
        ("mini-allowed", "Mini allowed", SpinMiniPolicy::MiniAllowed),
        ("mini-only", "Mini only", SpinMiniPolicy::MiniOnly),
        (
            "all-spin-as-mini",
            "All-spin as mini",
            SpinMiniPolicy::AllSpinAsMini,
        ),
    ]
    .into_iter()
    .map(|(id, label, _policy)| {
        DropdownOption::new(id, label).with_localized_label(LocalizedLabelSchema::new(
            TranslationKey::new(format!("ui.spin_target.mini_policy.{id}.label")),
            label,
        ))
    })
    .collect()
}

fn spin_target_accuracy_options() -> Vec<DropdownOption> {
    vec![
        DropdownOption::new("exact-only", "Exact only").with_localized_label(
            LocalizedLabelSchema::new(
                TranslationKey::new("ui.spin_target.accuracy.exact_only.label"),
                "Exact only",
            ),
        ),
        DropdownOption::new("allow-estimate", "Allow estimate").with_localized_label(
            LocalizedLabelSchema::new(
                TranslationKey::new("ui.spin_target.accuracy.allow_estimate.label"),
                "Allow estimate",
            ),
        ),
        DropdownOption::new("require-kick-evidence", "Require kick evidence")
            .with_localized_label(LocalizedLabelSchema::new(
                TranslationKey::new("ui.spin_target.accuracy.require_kick_evidence.label"),
                "Require kick evidence",
            ))
            .disabled_for(
                DiagnosticCode::ESpinKickEvidenceMissing,
                "kick_evidence_required_for_exact_spin",
            ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_schema_exposes_spin_target_options() {
        let schema = SpinTargetSchema::mvp2();
        let ids = schema
            .options()
            .iter()
            .map(SpinTargetOptionSchema::id)
            .collect::<Vec<_>>();
        let mini_policy_ids = schema
            .mini_policy_options()
            .iter()
            .map(DropdownOption::value)
            .collect::<Vec<_>>();
        let accuracy_ids = schema
            .accuracy_options()
            .iter()
            .map(DropdownOption::value)
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            [
                "t-spin-double",
                "t-spin-triple",
                "t-spin-mini-double",
                "all-spin-single",
                "all-spin-double",
                "all-spin-triple",
                "profile-specific-spin"
            ]
        );
        assert_eq!(
            mini_policy_ids,
            [
                "regular-only",
                "mini-allowed",
                "mini-only",
                "all-spin-as-mini"
            ]
        );
        assert_eq!(
            accuracy_ids,
            ["exact-only", "allow-estimate", "require-kick-evidence"]
        );
        assert!(schema
            .options()
            .iter()
            .any(SpinTargetOptionSchema::requires_kick_evidence));
        assert!(schema
            .result_contract_keys()
            .contains(&"pattern_universe_id"));
    }
}
