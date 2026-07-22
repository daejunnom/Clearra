#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBudget {
    pub max_frontier_states: usize,
    pub max_candidate_rows: usize,
    pub max_hash_buckets: usize,
    pub max_gpu_batch_bytes: usize,
    pub max_readback_bytes: usize,
    pub max_build_worker_backlog: usize,
    pub max_coverage_rows: usize,
    pub max_pattern_bits: usize,
    pub max_cpu_time_per_batch_ms: u64,
    pub max_memory_mib: Option<u32>,
}

impl ResourceBudget {
    pub const fn product_default() -> Self {
        Self {
            max_frontier_states: 65_536,
            max_candidate_rows: 65_536,
            max_hash_buckets: 65_536,
            max_gpu_batch_bytes: 64 * 1024 * 1024,
            max_readback_bytes: 64 * 1024 * 1024,
            max_build_worker_backlog: 16_384,
            max_coverage_rows: 65_536,
            max_pattern_bits: 1_048_576,
            max_cpu_time_per_batch_ms: 1_000,
            max_memory_mib: None,
        }
    }
}
impl ResourceBudget {
    pub const fn with_candidate_rows(mut self, max_candidate_rows: usize) -> Self {
        self.max_candidate_rows = max_candidate_rows;
        self
    }
}
impl ResourceBudget {
    pub const fn with_frontier_states(mut self, max_frontier_states: usize) -> Self {
        self.max_frontier_states = max_frontier_states;
        self
    }
}
impl ResourceBudget {
    pub const fn with_coverage_rows(mut self, max_coverage_rows: usize) -> Self {
        self.max_coverage_rows = max_coverage_rows;
        self
    }
}
impl ResourceBudget {
    pub const fn with_pattern_bits(mut self, max_pattern_bits: usize) -> Self {
        self.max_pattern_bits = max_pattern_bits;
        self
    }
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self::product_default()
    }
}
