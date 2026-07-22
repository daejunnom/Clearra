use clearra_rules::profile::rule_profile::RuleProfileId;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleBuildTable {
    supported_rules: Vec<RuleProfileId>,
}

impl RuleBuildTable {
    pub fn new(supported_rules: Vec<RuleProfileId>) -> Self {
        Self { supported_rules }
    }
}
impl RuleBuildTable {
    pub fn supports(&self, rule: RuleProfileId) -> bool {
        self.supported_rules.contains(&rule)
    }
}
