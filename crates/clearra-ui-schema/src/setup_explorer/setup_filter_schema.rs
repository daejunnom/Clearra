use clearra_profiles::search::SearchDefaults;
use clearra_setup_search::query::{GroupingMode, SetupLimits};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupFilterSchema {
    probability_filter_enabled: bool,
    grouping_modes: Vec<String>,
    max_results: usize,
}

impl SetupFilterSchema {
    pub fn mvp() -> Self {
        let limits = SetupLimits::from(SearchDefaults::MVP1);
        Self {
            probability_filter_enabled: true,
            grouping_modes: GroupingMode::MVP1_SUPPORTED
                .iter()
                .map(|mode| mode.as_str().to_owned())
                .collect(),
            max_results: limits.max_results(),
        }
    }
}
impl SetupFilterSchema {
    pub fn probability_filter_enabled(&self) -> bool {
        self.probability_filter_enabled
    }
}
impl SetupFilterSchema {
    pub fn grouping_modes(&self) -> &[String] {
        &self.grouping_modes
    }
}
impl SetupFilterSchema {
    pub fn max_results(&self) -> usize {
        self.max_results
    }
}

impl Default for SetupFilterSchema {
    fn default() -> Self {
        Self::mvp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_filter_schema_uses_canonical_grouping_modes_and_limits() {
        let schema = SetupFilterSchema::mvp();
        let expected_modes = GroupingMode::MVP1_SUPPORTED
            .iter()
            .map(|mode| mode.as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(schema.grouping_modes(), expected_modes.as_slice());
        assert_eq!(
            schema.max_results(),
            SetupLimits::from(SearchDefaults::MVP1).max_results()
        );
    }
}
