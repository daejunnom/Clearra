use clearra_profiles::search::search_defaults::SearchDefaults;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildCoverageLimits {
    max_assignments: usize,
    max_patterns: usize,
}

impl BuildCoverageLimits {
    pub fn new(max_assignments: usize, max_patterns: usize) -> Self {
        Self {
            max_assignments,
            max_patterns,
        }
    }
}
impl BuildCoverageLimits {
    pub fn max_assignments(self) -> usize {
        self.max_assignments
    }
}
impl BuildCoverageLimits {
    pub fn max_patterns(self) -> usize {
        self.max_patterns
    }
}

impl Default for BuildCoverageLimits {
    fn default() -> Self {
        SearchDefaults::MVP1.into()
    }
}

impl From<SearchDefaults> for BuildCoverageLimits {
    fn from(defaults: SearchDefaults) -> Self {
        Self {
            max_assignments: defaults.build_max_assignments(),
            max_patterns: defaults.build_max_patterns(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_profile_defaults_into_build_coverage_limits() {
        let limits = BuildCoverageLimits::from(SearchDefaults::MVP1);

        assert_eq!(limits.max_assignments(), 1024);
        assert_eq!(limits.max_patterns(), 4096);
    }
}
