#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResourceBudget {
    workers: u16,
    candidate_budget: Option<u64>,
    memory_mib: Option<u32>,
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
    pub const fn new(workers: u16, candidate_budget: Option<u64>, memory_mib: Option<u64>) -> Self {
        let candidate_limit = match candidate_budget {
            Some(value) => value as usize,
            None => 65_536,
        };
        let memory_mib_u32 = match memory_mib {
            Some(value) if value > u32::MAX as u64 => Some(u32::MAX),
            Some(value) => Some(value as u32),
            None => None,
        };
        Self {
            workers,
            candidate_budget,
            memory_mib: memory_mib_u32,
            max_frontier_states: candidate_limit,
            max_candidate_rows: candidate_limit,
            max_hash_buckets: candidate_limit,
            max_gpu_batch_bytes: 64 * 1024 * 1024,
            max_readback_bytes: 64 * 1024 * 1024,
            max_build_worker_backlog: 16_384,
            max_coverage_rows: 65_536,
            max_pattern_bits: 1_048_576,
            max_cpu_time_per_batch_ms: 1_000,
            max_memory_mib: memory_mib_u32,
        }
    }
}
impl ResourceBudget {
    pub const fn workers(self) -> u16 {
        self.workers
    }
}
impl ResourceBudget {
    pub const fn candidate_budget(self) -> Option<u64> {
        self.candidate_budget
    }
}
impl ResourceBudget {
    pub const fn memory_mib(self) -> Option<u64> {
        match self.memory_mib {
            Some(value) => Some(value as u64),
            None => None,
        }
    }
}
impl ResourceBudget {
    pub const fn max_frontier_states(self) -> usize {
        self.max_frontier_states
    }
}
impl ResourceBudget {
    pub const fn max_candidate_rows(self) -> usize {
        self.max_candidate_rows
    }
}
impl ResourceBudget {
    pub const fn max_hash_buckets(self) -> usize {
        self.max_hash_buckets
    }
}
impl ResourceBudget {
    pub const fn max_gpu_batch_bytes(self) -> usize {
        self.max_gpu_batch_bytes
    }
}
impl ResourceBudget {
    pub const fn max_readback_bytes(self) -> usize {
        self.max_readback_bytes
    }
}
impl ResourceBudget {
    pub const fn max_build_worker_backlog(self) -> usize {
        self.max_build_worker_backlog
    }
}
impl ResourceBudget {
    pub const fn max_coverage_rows(self) -> usize {
        self.max_coverage_rows
    }
}
impl ResourceBudget {
    pub const fn max_pattern_bits(self) -> usize {
        self.max_pattern_bits
    }
}
impl ResourceBudget {
    pub const fn max_cpu_time_per_batch_ms(self) -> u64 {
        self.max_cpu_time_per_batch_ms
    }
}
impl ResourceBudget {
    pub const fn max_memory_mib(self) -> Option<u32> {
        self.max_memory_mib
    }
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self::new(1, None, None)
    }
}
