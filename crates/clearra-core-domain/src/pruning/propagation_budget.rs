#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropagationBudget {
    pub max_cpu_time_per_batch_ms: u64,
    pub max_components_per_batch: usize,
    pub max_component_cells: usize,
    pub max_candidate_domains: usize,
    pub max_clear_states: usize,
    pub min_expected_reduction_ratio: f64,
}

impl PropagationBudget {
    pub const fn product_default() -> Self {
        Self {
            max_cpu_time_per_batch_ms: 10,
            max_components_per_batch: 32,
            max_component_cells: 12,
            max_candidate_domains: 2048,
            max_clear_states: 64,
            min_expected_reduction_ratio: 0.05,
        }
    }
}
impl PropagationBudget {
    pub fn component_exact_cover_allowed(
        self,
        component_count: usize,
        component_cells: usize,
        candidate_domains: usize,
        clear_states: usize,
    ) -> bool {
        component_count <= self.max_components_per_batch
            && component_cells <= self.max_component_cells
            && candidate_domains <= self.max_candidate_domains
            && clear_states <= self.max_clear_states
    }
}

impl Default for PropagationBudget {
    fn default() -> Self {
        Self::product_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_exact_cover_runs_only_under_budget() {
        let budget = PropagationBudget {
            max_component_cells: 4,
            ..PropagationBudget::default()
        };

        assert!(budget.component_exact_cover_allowed(1, 4, 8, 2));
        assert!(!budget.component_exact_cover_allowed(1, 5, 8, 2));
    }
}
