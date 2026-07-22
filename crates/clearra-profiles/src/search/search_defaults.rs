#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchDefaults {
    max_nodes: usize,
    max_seconds: u64,
    setup_max_shape_families: usize,
    setup_max_tiling_variants_per_family: usize,
    setup_max_build_variants_per_tiling: usize,
    setup_max_results: usize,
    setup_max_patterns: usize,
    build_max_assignments: usize,
    build_max_patterns: usize,
    observed_max_suffix_patterns: usize,
    scenario_retained_trace_limit: usize,
    execution_deterministic: bool,
    execution_max_frontier_states: usize,
    execution_max_candidates: usize,
    execution_max_patterns: usize,
    execution_max_memory_mib: Option<u64>,
}

impl SearchDefaults {
    pub const MVP1: Self = Self {
        max_nodes: 0,
        max_seconds: 0,
        setup_max_shape_families: 4096,
        setup_max_tiling_variants_per_family: 1024,
        setup_max_build_variants_per_tiling: 1024,
        setup_max_results: 256,
        setup_max_patterns: 4096,
        build_max_assignments: 1024,
        build_max_patterns: 4096,
        observed_max_suffix_patterns: 16,
        scenario_retained_trace_limit: 64,
        execution_deterministic: true,
        execution_max_frontier_states: 0,
        execution_max_candidates: 0,
        execution_max_patterns: 0,
        execution_max_memory_mib: None,
    };
}
impl SearchDefaults {
    pub fn max_nodes(self) -> usize {
        self.max_nodes
    }
}
impl SearchDefaults {
    pub fn max_seconds(self) -> u64 {
        self.max_seconds
    }
}
impl SearchDefaults {
    pub fn setup_max_shape_families(self) -> usize {
        self.setup_max_shape_families
    }
}
impl SearchDefaults {
    pub fn setup_max_tiling_variants_per_family(self) -> usize {
        self.setup_max_tiling_variants_per_family
    }
}
impl SearchDefaults {
    pub fn setup_max_build_variants_per_tiling(self) -> usize {
        self.setup_max_build_variants_per_tiling
    }
}
impl SearchDefaults {
    pub fn setup_max_results(self) -> usize {
        self.setup_max_results
    }
}
impl SearchDefaults {
    pub fn setup_max_patterns(self) -> usize {
        self.setup_max_patterns
    }
}
impl SearchDefaults {
    pub fn build_max_assignments(self) -> usize {
        self.build_max_assignments
    }
}
impl SearchDefaults {
    pub fn build_max_patterns(self) -> usize {
        self.build_max_patterns
    }
}
impl SearchDefaults {
    pub fn observed_max_suffix_patterns(self) -> usize {
        self.observed_max_suffix_patterns
    }
}
impl SearchDefaults {
    pub fn scenario_retained_trace_limit(self) -> usize {
        self.scenario_retained_trace_limit
    }
}
impl SearchDefaults {
    pub fn execution_deterministic(self) -> bool {
        self.execution_deterministic
    }
}
impl SearchDefaults {
    pub fn execution_max_frontier_states(self) -> usize {
        self.execution_max_frontier_states
    }
}
impl SearchDefaults {
    pub fn execution_max_candidates(self) -> usize {
        self.execution_max_candidates
    }
}
impl SearchDefaults {
    pub fn execution_max_patterns(self) -> usize {
        self.execution_max_patterns
    }
}
impl SearchDefaults {
    pub fn execution_max_memory_mib(self) -> Option<u64> {
        self.execution_max_memory_mib
    }
}

impl Default for SearchDefaults {
    fn default() -> Self {
        Self::MVP1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp1_defaults_expose_runtime_budget_values() {
        let defaults = SearchDefaults::MVP1;

        assert_eq!(defaults.setup_max_results(), 256);
        assert_eq!(defaults.build_max_patterns(), 4096);
        assert_eq!(defaults.observed_max_suffix_patterns(), 16);
        assert_eq!(defaults.scenario_retained_trace_limit(), 64);
        assert!(defaults.execution_deterministic());
        assert_eq!(defaults.max_nodes(), 0);
        assert_eq!(defaults.max_seconds(), 0);
        assert_eq!(defaults.execution_max_frontier_states(), 0);
        assert_eq!(defaults.execution_max_candidates(), 0);
        assert_eq!(defaults.execution_max_patterns(), 0);
        assert_eq!(defaults.execution_max_memory_mib(), None);
    }
}
