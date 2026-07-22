use clearra_scoring::{
    model::{AttackModelRegistry, ScoreModelRegistry},
    profile::{
        B2BPolicy, ComboPolicy, DropScorePolicyRegistry, LevelPolicyRegistry,
        PcBonusPolicyRegistry, SpinProfileRegistry, SpinRuleId, TraceRequirement,
    },
};

use super::scoring_field_schema::{ScoringFieldSchema, ScoringFieldType};

pub fn default_score_policy_markers() -> (ComboPolicy, B2BPolicy) {
    (ComboPolicy::DISABLED, B2BPolicy::DISABLED)
}

pub(crate) fn profile_fields() -> Vec<ScoringFieldSchema> {
    vec![
        ScoringFieldSchema::typed_field(
            "id",
            "Profile id",
            ScoringFieldType::Text,
            true,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "display_name",
            "Display name",
            ScoringFieldType::Text,
            true,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "accuracy_level",
            "Accuracy level",
            ScoringFieldType::Select,
            true,
            vec!["basic-approximation".to_owned()],
        ),
        ScoringFieldSchema::typed_field(
            "profile_specific_exact",
            "Profile-specific exact",
            ScoringFieldType::Toggle,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "accuracy_reason",
            "Accuracy reason",
            ScoringFieldType::Text,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "trace_requirement",
            "Trace requirement",
            ScoringFieldType::Select,
            false,
            trace_requirement_options(),
        ),
    ]
}

pub(crate) fn score_fields() -> Vec<ScoringFieldSchema> {
    vec![
        ScoringFieldSchema::typed_field(
            "score_model",
            "Score model",
            ScoringFieldType::Select,
            true,
            score_model_options(),
        ),
        ScoringFieldSchema::typed_field(
            "drop_score_policy",
            "Drop score",
            ScoringFieldType::Select,
            false,
            drop_score_policy_options(),
        ),
        ScoringFieldSchema::typed_field(
            "level_policy",
            "Level policy",
            ScoringFieldType::Select,
            false,
            level_policy_options(),
        ),
        ScoringFieldSchema::typed_field(
            "pc_bonus_policy",
            "Perfect clear bonus",
            ScoringFieldType::Select,
            false,
            pc_bonus_policy_options(),
        ),
    ]
}

pub(crate) fn attack_fields() -> Vec<ScoringFieldSchema> {
    vec![ScoringFieldSchema::typed_field(
        "attack_model",
        "Attack model",
        ScoringFieldType::Select,
        true,
        attack_model_options(),
    )]
}

pub(crate) fn spin_fields() -> Vec<ScoringFieldSchema> {
    vec![
        ScoringFieldSchema::typed_field(
            "spin_rule",
            "Spin rule",
            ScoringFieldType::Select,
            false,
            spin_rule_options(),
        ),
        ScoringFieldSchema::typed_field(
            "spin_award_policy",
            "Spin award",
            ScoringFieldType::Select,
            false,
            vec![
                "disabled".to_owned(),
                "t-spins-only".to_owned(),
                "all-spins".to_owned(),
                "all-mini".to_owned(),
                "all-spin-as-t-spin-mini".to_owned(),
            ],
        ),
    ]
}

pub(crate) fn combo_fields() -> Vec<ScoringFieldSchema> {
    vec![
        ScoringFieldSchema::typed_field(
            "combo.enabled",
            "Combo enabled",
            ScoringFieldType::Toggle,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "combo.score_bonus_per_combo",
            "Combo score bonus",
            ScoringFieldType::Number,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "combo.attack_bonus_per_combo",
            "Combo attack bonus",
            ScoringFieldType::Number,
            false,
            Vec::new(),
        ),
    ]
}

pub(crate) fn b2b_fields() -> Vec<ScoringFieldSchema> {
    vec![
        ScoringFieldSchema::typed_field(
            "b2b.enabled",
            "B2B enabled",
            ScoringFieldType::Toggle,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "b2b.score_bonus",
            "B2B score bonus",
            ScoringFieldType::Number,
            false,
            Vec::new(),
        ),
        ScoringFieldSchema::typed_field(
            "b2b.attack_bonus",
            "B2B attack bonus",
            ScoringFieldType::Number,
            false,
            Vec::new(),
        ),
    ]
}

fn score_model_options() -> Vec<String> {
    ScoreModelRegistry::builtins()
        .into_iter()
        .map(|model| model.id().as_str().to_owned())
        .collect()
}

fn attack_model_options() -> Vec<String> {
    AttackModelRegistry::builtins()
        .into_iter()
        .map(|model| model.id().as_str().to_owned())
        .collect()
}

fn spin_rule_options() -> Vec<String> {
    let mut options = vec![SpinRuleId::Disabled, SpinRuleId::TSpinSimple]
        .into_iter()
        .map(|rule| rule.as_str().to_owned())
        .collect::<Vec<_>>();
    options.extend(
        SpinProfileRegistry::builtins()
            .profiles()
            .iter()
            .map(|profile| profile.id().as_str().to_owned()),
    );
    options
}

fn drop_score_policy_options() -> Vec<String> {
    DropScorePolicyRegistry::builtins()
        .into_iter()
        .map(|policy| policy.id().as_str().to_owned())
        .collect()
}

fn level_policy_options() -> Vec<String> {
    LevelPolicyRegistry::builtins()
        .into_iter()
        .map(|policy| policy.id().as_str().to_owned())
        .collect()
}

fn pc_bonus_policy_options() -> Vec<String> {
    PcBonusPolicyRegistry::builtins()
        .into_iter()
        .map(|policy| policy.id().as_str().to_owned())
        .collect()
}

fn trace_requirement_options() -> Vec<String> {
    [
        TraceRequirement::None,
        TraceRequirement::PlacementTrace,
        TraceRequirement::FullDropTrace,
        TraceRequirement::KickEvidenceTrace,
    ]
    .into_iter()
    .map(|requirement| requirement.as_str().to_owned())
    .collect()
}
