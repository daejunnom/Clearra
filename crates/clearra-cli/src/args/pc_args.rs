#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcArgs {
    lines: u8,
    queue: String,
    fixed_queue: bool,
    hold_enabled: bool,
    objective: String,
    score_requested: bool,
    score_profile: Option<String>,
    spin_profile: Option<String>,
    initial_b2b: Option<u32>,
    rule: Option<String>,
    kick_profile_json: Option<String>,
    backend: Option<String>,
    workers: Option<usize>,
    use_all_logical_processors: Option<bool>,
    cpu_warmup: Option<bool>,
    gpu_warmup: Option<bool>,
    deterministic: Option<bool>,
    max_frontier_states: Option<usize>,
    max_candidates: Option<usize>,
    max_patterns: Option<usize>,
    max_memory_mib: Option<usize>,
    gpu_device: Option<String>,
    allow_backend_fallback: Option<bool>,
    solution_probabilities: bool,
}

impl PcArgs {
    pub fn new(lines: u8) -> Self {
        Self {
            lines,
            queue: String::new(),
            fixed_queue: false,
            hold_enabled: true,
            objective: "all".to_owned(),
            score_requested: false,
            score_profile: None,
            spin_profile: None,
            initial_b2b: None,
            rule: None,
            kick_profile_json: None,
            backend: None,
            workers: None,
            use_all_logical_processors: None,
            cpu_warmup: None,
            gpu_warmup: None,
            deterministic: None,
            max_frontier_states: None,
            max_candidates: None,
            max_patterns: None,
            max_memory_mib: None,
            gpu_device: None,
            allow_backend_fallback: None,
            solution_probabilities: false,
        }
    }
}
impl PcArgs {
    pub fn lines(&self) -> u8 {
        self.lines
    }
}
impl PcArgs {
    pub fn queue(&self) -> &str {
        &self.queue
    }
}
impl PcArgs {
    pub fn fixed_queue(&self) -> bool {
        self.fixed_queue
    }
}
impl PcArgs {
    pub fn hold_enabled(&self) -> bool {
        self.hold_enabled
    }
}
impl PcArgs {
    pub fn objective(&self) -> &str {
        &self.objective
    }
}
impl PcArgs {
    pub fn score_requested(&self) -> bool {
        self.score_requested
    }

    pub fn initial_b2b(&self) -> Option<u32> {
        self.initial_b2b
    }

    pub fn score_profile(&self) -> Option<&str> {
        self.score_profile.as_deref()
    }

    pub fn spin_profile(&self) -> Option<&str> {
        self.spin_profile.as_deref()
    }
}
impl PcArgs {
    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }
}
impl PcArgs {
    pub fn kick_profile_json(&self) -> Option<&str> {
        self.kick_profile_json.as_deref()
    }
}
impl PcArgs {
    pub fn backend(&self) -> Option<&str> {
        self.backend.as_deref()
    }
}
impl PcArgs {
    pub fn workers(&self) -> Option<usize> {
        self.workers
    }
}
impl PcArgs {
    pub fn use_all_logical_processors(&self) -> Option<bool> {
        self.use_all_logical_processors
    }
}
impl PcArgs {
    pub fn cpu_warmup(&self) -> Option<bool> {
        self.cpu_warmup
    }
}
impl PcArgs {
    pub fn gpu_warmup(&self) -> Option<bool> {
        self.gpu_warmup
    }
}
impl PcArgs {
    pub fn deterministic(&self) -> Option<bool> {
        self.deterministic
    }
}
impl PcArgs {
    pub fn max_frontier_states(&self) -> Option<usize> {
        self.max_frontier_states
    }
}
impl PcArgs {
    pub fn max_candidates(&self) -> Option<usize> {
        self.max_candidates
    }
}
impl PcArgs {
    pub fn max_patterns(&self) -> Option<usize> {
        self.max_patterns
    }
}
impl PcArgs {
    pub fn max_memory_mib(&self) -> Option<usize> {
        self.max_memory_mib
    }
}
impl PcArgs {
    pub fn gpu_device(&self) -> Option<&str> {
        self.gpu_device.as_deref()
    }
}
impl PcArgs {
    pub fn allow_backend_fallback(&self) -> Option<bool> {
        self.allow_backend_fallback
    }

    pub fn solution_probabilities(&self) -> bool {
        self.solution_probabilities
    }
}
impl PcArgs {
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
impl PcArgs {
    pub fn with_queue(mut self, queue: impl Into<String>, fixed_queue: bool) -> Self {
        self.queue = queue.into();
        self.fixed_queue = fixed_queue;
        self
    }
}
impl PcArgs {
    pub fn with_hold_enabled(mut self, hold_enabled: bool) -> Self {
        self.hold_enabled = hold_enabled;
        self
    }
}
impl PcArgs {
    pub fn with_objective(mut self, objective: impl Into<String>) -> Self {
        self.objective = objective.into();
        self
    }
}
impl PcArgs {
    pub fn with_score_requested(mut self, score_requested: bool) -> Self {
        self.score_requested = score_requested;
        self
    }

    pub fn with_initial_b2b(mut self, initial_b2b: Option<u32>) -> Self {
        self.initial_b2b = initial_b2b;
        self
    }

    pub fn with_score_profile(mut self, score_profile: Option<String>) -> Self {
        self.score_profile = score_profile;
        self
    }

    pub fn with_spin_profile(mut self, spin_profile: Option<String>) -> Self {
        self.spin_profile = spin_profile;
        self
    }
}
impl PcArgs {
    pub fn with_rule(mut self, rule: Option<String>) -> Self {
        self.rule = rule;
        self
    }
}
impl PcArgs {
    pub fn with_kick_profile_json(mut self, kick_profile_json: Option<String>) -> Self {
        self.kick_profile_json = kick_profile_json;
        self
    }
}
impl PcArgs {
    pub fn with_backend(mut self, backend: Option<String>) -> Self {
        self.backend = backend;
        self
    }
}
impl PcArgs {
    pub fn with_workers(mut self, workers: Option<usize>) -> Self {
        self.workers = workers;
        self
    }
}
impl PcArgs {
    pub fn with_use_all_logical_processors(mut self, value: Option<bool>) -> Self {
        self.use_all_logical_processors = value;
        self
    }
}
impl PcArgs {
    pub fn with_cpu_warmup(mut self, value: Option<bool>) -> Self {
        self.cpu_warmup = value;
        self
    }
}
impl PcArgs {
    pub fn with_gpu_warmup(mut self, value: Option<bool>) -> Self {
        self.gpu_warmup = value;
        self
    }
}
impl PcArgs {
    pub fn with_deterministic(mut self, deterministic: Option<bool>) -> Self {
        self.deterministic = deterministic;
        self
    }
}
impl PcArgs {
    pub fn with_max_frontier_states(mut self, max_frontier_states: Option<usize>) -> Self {
        self.max_frontier_states = max_frontier_states;
        self
    }
}
impl PcArgs {
    pub fn with_max_candidates(mut self, max_candidates: Option<usize>) -> Self {
        self.max_candidates = max_candidates;
        self
    }
}
impl PcArgs {
    pub fn with_max_patterns(mut self, max_patterns: Option<usize>) -> Self {
        self.max_patterns = max_patterns;
        self
    }
}
impl PcArgs {
    pub fn with_max_memory_mib(mut self, max_memory_mib: Option<usize>) -> Self {
        self.max_memory_mib = max_memory_mib;
        self
    }
}
impl PcArgs {
    pub fn with_gpu_device(mut self, gpu_device: Option<String>) -> Self {
        self.gpu_device = gpu_device;
        self
    }
}
impl PcArgs {
    pub fn with_allow_backend_fallback(mut self, allow_backend_fallback: Option<bool>) -> Self {
        self.allow_backend_fallback = allow_backend_fallback;
        self
    }

    pub fn with_solution_probabilities(mut self, value: bool) -> Self {
        self.solution_probabilities = value;
        self
    }
}

impl Default for PcArgs {
    fn default() -> Self {
        Self::new(2)
    }
}
