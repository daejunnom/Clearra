use super::pc_scenario_args::PcScenarioArgs;

impl PcScenarioArgs {
    pub fn with_verify_expected(mut self, verify_expected: bool) -> Self {
        self.verify_expected = verify_expected;
        self
    }

    pub fn with_solution_probabilities(mut self, value: bool) -> Self {
        self.solution_probabilities = value;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_backend(mut self, backend: Option<String>) -> Self {
        self.backend = backend;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_workers(mut self, workers: Option<usize>) -> Self {
        self.workers = workers;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_use_all_logical_processors(mut self, value: Option<bool>) -> Self {
        self.use_all_logical_processors = value;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_cpu_warmup(mut self, value: Option<bool>) -> Self {
        self.cpu_warmup = value;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_gpu_warmup(mut self, value: Option<bool>) -> Self {
        self.gpu_warmup = value;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_deterministic(mut self, deterministic: Option<bool>) -> Self {
        self.deterministic = deterministic;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_max_frontier_states(mut self, max_frontier_states: Option<usize>) -> Self {
        self.max_frontier_states = max_frontier_states;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_max_candidates(mut self, max_candidates: Option<usize>) -> Self {
        self.max_candidates = max_candidates;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_max_patterns(mut self, max_patterns: Option<usize>) -> Self {
        self.max_patterns = max_patterns;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_max_memory_mib(mut self, max_memory_mib: Option<usize>) -> Self {
        self.max_memory_mib = max_memory_mib;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_gpu_device(mut self, gpu_device: Option<String>) -> Self {
        self.gpu_device = gpu_device;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_allow_backend_fallback(mut self, allow_backend_fallback: Option<bool>) -> Self {
        self.allow_backend_fallback = allow_backend_fallback;
        self
    }
}
impl PcScenarioArgs {
    pub fn backend(&self) -> Option<&str> {
        self.backend.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn workers(&self) -> Option<usize> {
        self.workers
    }
}
impl PcScenarioArgs {
    pub fn use_all_logical_processors(&self) -> Option<bool> {
        self.use_all_logical_processors
    }
}
impl PcScenarioArgs {
    pub fn cpu_warmup(&self) -> Option<bool> {
        self.cpu_warmup
    }
}
impl PcScenarioArgs {
    pub fn gpu_warmup(&self) -> Option<bool> {
        self.gpu_warmup
    }
}
impl PcScenarioArgs {
    pub fn deterministic(&self) -> Option<bool> {
        self.deterministic
    }
}
impl PcScenarioArgs {
    pub fn max_frontier_states(&self) -> Option<usize> {
        self.max_frontier_states
    }
}
impl PcScenarioArgs {
    pub fn max_candidates(&self) -> Option<usize> {
        self.max_candidates
    }
}
impl PcScenarioArgs {
    pub fn max_patterns(&self) -> Option<usize> {
        self.max_patterns
    }
}
impl PcScenarioArgs {
    pub fn max_memory_mib(&self) -> Option<usize> {
        self.max_memory_mib
    }
}
impl PcScenarioArgs {
    pub fn gpu_device(&self) -> Option<&str> {
        self.gpu_device.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn allow_backend_fallback(&self) -> Option<bool> {
        self.allow_backend_fallback
    }
}
impl PcScenarioArgs {
    pub fn has_execution_options(&self) -> bool {
        self.backend.is_some()
            || self.workers.is_some()
            || self.use_all_logical_processors.is_some()
            || self.cpu_warmup.is_some()
            || self.gpu_warmup.is_some()
            || self.deterministic.is_some()
            || self.max_frontier_states.is_some()
            || self.max_candidates.is_some()
            || self.max_patterns.is_some()
            || self.max_memory_mib.is_some()
            || self.gpu_device.is_some()
            || self.allow_backend_fallback.is_some()
    }
}
impl PcScenarioArgs {
    pub fn verify_expected(&self) -> bool {
        self.verify_expected
    }

    pub fn solution_probabilities(&self) -> bool {
        self.solution_probabilities
    }
}
