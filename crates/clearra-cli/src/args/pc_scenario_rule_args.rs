use super::pc_scenario_args::PcScenarioArgs;

impl PcScenarioArgs {
    pub fn with_rule(mut self, rule: Option<String>) -> Self {
        self.rule = rule;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_kick_profile_json(mut self, kick_profile_json: Option<String>) -> Self {
        self.kick_profile_json = kick_profile_json;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_requires_180(mut self, requires_180: bool) -> Self {
        self.requires_180 = requires_180;
        self
    }
}
impl PcScenarioArgs {
    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn kick_profile_json(&self) -> Option<&str> {
        self.kick_profile_json.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn requires_180(&self) -> bool {
        self.requires_180
    }
}
