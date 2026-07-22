use clearra_profiles::bundle::standard_profile_bundle::{
    standard_profile_bundle, StandardProfileBundle,
};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CliProfileSet {
    standard: StandardProfileBundle,
    rule: RuleProfile,
}

impl CliProfileSet {
    pub fn standard(&self) -> StandardProfileBundle {
        self.standard
    }
}
impl CliProfileSet {
    pub fn rule(&self) -> RuleProfile {
        self.rule
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProfileAssembler;

impl ProfileAssembler {
    pub fn standard_mvp() -> CliProfileSet {
        CliProfileSet {
            standard: standard_profile_bundle(),
            rule: srs_plus(),
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_profiles::board::standard10::STANDARD_10_WIDTH;

    use super::*;

    #[test]
    fn assembles_standard_mvp_profiles() {
        let profiles = ProfileAssembler::standard_mvp();

        assert_eq!(
            profiles.standard().board().size().width(),
            STANDARD_10_WIDTH
        );
        assert!(profiles.rule().is_two_line_supported());
    }
}
