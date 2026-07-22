use clearra_core_domain::pc::pc_target::PcTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChainClass {
    Opening2L,
    Opening4L,
    Opening6L,
    Scenario,
    UnsupportedOpening,
}

impl ChainClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opening2L => "opening-2l",
            Self::Opening4L => "opening-4l",
            Self::Opening6L => "opening-6l",
            Self::Scenario => "scenario",
            Self::UnsupportedOpening => "unsupported-opening",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChainClassifier;

impl ChainClassifier {
    pub fn opening(target: PcTarget) -> ChainClass {
        match target.lines() {
            2 => ChainClass::Opening2L,
            4 => ChainClass::Opening4L,
            6 => ChainClass::Opening6L,
            _ => ChainClass::UnsupportedOpening,
        }
    }
}
impl ChainClassifier {
    pub fn scenario() -> ChainClass {
        ChainClass::Scenario
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::pc::pc_target::PcTarget;

    use super::*;

    #[test]
    fn chain_classifier_keeps_opening_and_scenario_as_labels() {
        assert_eq!(
            ChainClassifier::opening(PcTarget::two_lines()).as_str(),
            "opening-2l"
        );
        assert_eq!(ChainClassifier::scenario().as_str(), "scenario");
    }
}
