use super::rule_profile::{RuleProfile, RuleProfileId};

pub fn srs_plus() -> RuleProfile {
    RuleProfile::new(RuleProfileId::SrsPlus)
}

pub fn srs() -> RuleProfile {
    RuleProfile::new(RuleProfileId::Srs)
}

pub fn srs_x() -> RuleProfile {
    RuleProfile::new(RuleProfileId::SrsX)
}

pub fn asc() -> RuleProfile {
    RuleProfile::new(RuleProfileId::Asc)
}

pub fn ars() -> RuleProfile {
    RuleProfile::new(RuleProfileId::Ars)
}

pub fn no_kick() -> RuleProfile {
    RuleProfile::new(RuleProfileId::NoKick)
}

pub fn custom_rule() -> RuleProfile {
    RuleProfile::new(RuleProfileId::Custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_report_two_line_support() {
        assert!(srs_plus().is_two_line_supported());
        assert!(srs().is_two_line_supported());
        assert!(srs_x().is_two_line_supported());
        assert!(asc().is_two_line_supported());
        assert!(ars().is_two_line_supported());
        assert!(no_kick().is_two_line_supported());
        assert!(!custom_rule().is_two_line_supported());
    }
}
