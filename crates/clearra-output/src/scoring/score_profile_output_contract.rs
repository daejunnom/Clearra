#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileOutputContract {
    score_profile_id: String,
    score_model_id: String,
    attack_model_id: String,
    spin_rule_id: String,
    spin_award_policy: String,
    drop_score_policy: String,
    level_policy: String,
    combo_policy: String,
    b2b_policy: String,
    pc_bonus_policy: String,
    accuracy_level: String,
    accuracy_reason: String,
    trace_requirement: String,
    profile_specific_exact: bool,
}

impl ScoreProfileOutputContract {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        score_profile_id: impl Into<String>,
        score_model_id: impl Into<String>,
        attack_model_id: impl Into<String>,
        spin_rule_id: impl Into<String>,
        spin_award_policy: impl Into<String>,
        drop_score_policy: impl Into<String>,
        level_policy: impl Into<String>,
        combo_policy: impl Into<String>,
        b2b_policy: impl Into<String>,
        pc_bonus_policy: impl Into<String>,
        accuracy_level: impl Into<String>,
        accuracy_reason: impl Into<String>,
        trace_requirement: impl Into<String>,
        profile_specific_exact: bool,
    ) -> Self {
        Self {
            score_profile_id: score_profile_id.into(),
            score_model_id: score_model_id.into(),
            attack_model_id: attack_model_id.into(),
            spin_rule_id: spin_rule_id.into(),
            spin_award_policy: spin_award_policy.into(),
            drop_score_policy: drop_score_policy.into(),
            level_policy: level_policy.into(),
            combo_policy: combo_policy.into(),
            b2b_policy: b2b_policy.into(),
            pc_bonus_policy: pc_bonus_policy.into(),
            accuracy_level: accuracy_level.into(),
            accuracy_reason: accuracy_reason.into(),
            trace_requirement: trace_requirement.into(),
            profile_specific_exact,
        }
    }
}
impl ScoreProfileOutputContract {
    pub fn basic_approximation(
        score_profile_id: impl Into<String>,
        score_model_id: impl Into<String>,
        attack_model_id: impl Into<String>,
        spin_rule_id: impl Into<String>,
        accuracy_reason: impl Into<String>,
    ) -> Self {
        Self::new(
            score_profile_id,
            score_model_id,
            attack_model_id,
            spin_rule_id,
            "t-spins-only",
            "disabled",
            "disabled",
            "disabled",
            "disabled",
            "disabled",
            "basic-approximation",
            accuracy_reason,
            "none",
            false,
        )
    }
}
impl ScoreProfileOutputContract {
    pub fn score_profile_id(&self) -> &str {
        &self.score_profile_id
    }
}
impl ScoreProfileOutputContract {
    pub fn score_model_id(&self) -> &str {
        &self.score_model_id
    }
}
impl ScoreProfileOutputContract {
    pub fn attack_model_id(&self) -> &str {
        &self.attack_model_id
    }
}
impl ScoreProfileOutputContract {
    pub fn spin_rule_id(&self) -> &str {
        &self.spin_rule_id
    }
}
impl ScoreProfileOutputContract {
    pub fn spin_award_policy(&self) -> &str {
        &self.spin_award_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn drop_score_policy(&self) -> &str {
        &self.drop_score_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn level_policy(&self) -> &str {
        &self.level_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn combo_policy(&self) -> &str {
        &self.combo_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn b2b_policy(&self) -> &str {
        &self.b2b_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn pc_bonus_policy(&self) -> &str {
        &self.pc_bonus_policy
    }
}
impl ScoreProfileOutputContract {
    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }
}
impl ScoreProfileOutputContract {
    pub fn accuracy_reason(&self) -> &str {
        &self.accuracy_reason
    }
}
impl ScoreProfileOutputContract {
    pub fn trace_requirement(&self) -> &str {
        &self.trace_requirement
    }
}
impl ScoreProfileOutputContract {
    pub fn profile_specific_exact(&self) -> bool {
        self.profile_specific_exact
    }
}
impl ScoreProfileOutputContract {
    pub fn exact_claim_allowed(&self) -> bool {
        self.profile_specific_exact && self.accuracy_level == "profile-specific-exact"
    }
}

#[cfg(test)]
#[path = "score_profile_output_contract_tests.rs"]
mod tests;
