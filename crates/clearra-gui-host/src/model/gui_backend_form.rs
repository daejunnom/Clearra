#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiBackendChoice {
    #[default]
    Auto,
    Cpu,
    Gpu,
    Hybrid,
}

impl GuiBackendChoice {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Cpu, Self::Gpu, Self::Hybrid];
}
impl GuiBackendChoice {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Hybrid => "hybrid",
        }
    }
}
impl GuiBackendChoice {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "cpu" => Some(Self::Cpu),
            "gpu" => Some(Self::Gpu),
            "hybrid" => Some(Self::Hybrid),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiBackendForm {
    backend: GuiBackendChoice,
    backend_id: String,
    gpu_device: Option<String>,
    allow_fallback: bool,
    workers: u16,
    workers_explicit: bool,
    use_all_logical_processors: bool,
    deterministic: bool,
    precompute_build_dependencies: bool,
    tablebase_requested: bool,
    memory_budget_mb: u32,
    candidate_budget: u32,
    pattern_budget: u32,
}

impl GuiBackendForm {
    pub fn new(backend: GuiBackendChoice) -> Self {
        Self {
            backend,
            backend_id: backend.as_str().to_owned(),
            ..Self::default()
        }
    }
}
impl GuiBackendForm {
    pub fn from_backend_id(backend_id: impl Into<String>) -> Self {
        let backend_id = backend_id.into();
        Self {
            backend: GuiBackendChoice::parse(&backend_id).unwrap_or_default(),
            backend_id,
            ..Self::default()
        }
    }
}
impl GuiBackendForm {
    pub fn with_backend(mut self, backend: GuiBackendChoice) -> Self {
        self.backend = backend;
        self.backend_id = backend.as_str().to_owned();
        self
    }
}
impl GuiBackendForm {
    pub fn with_gpu_device(mut self, gpu_device: impl Into<String>) -> Self {
        self.gpu_device = Some(gpu_device.into());
        self
    }
}
impl GuiBackendForm {
    pub const fn with_allow_fallback(mut self, allow_fallback: bool) -> Self {
        self.allow_fallback = allow_fallback;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_workers(mut self, workers: u16) -> Self {
        self.workers = workers;
        self.workers_explicit = workers > 0;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_use_all_logical_processors(mut self, value: bool) -> Self {
        self.use_all_logical_processors = value;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_deterministic(mut self, deterministic: bool) -> Self {
        self.deterministic = deterministic;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_precompute_build_dependencies(mut self, value: bool) -> Self {
        self.precompute_build_dependencies = value;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_memory_budget_mb(mut self, memory_budget_mb: u32) -> Self {
        self.memory_budget_mb = memory_budget_mb;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_candidate_budget(mut self, candidate_budget: u32) -> Self {
        self.candidate_budget = candidate_budget;
        self
    }
}
impl GuiBackendForm {
    pub const fn with_pattern_budget(mut self, pattern_budget: u32) -> Self {
        self.pattern_budget = pattern_budget;
        self
    }
}
impl GuiBackendForm {
    pub const fn backend(&self) -> GuiBackendChoice {
        self.backend
    }
}
impl GuiBackendForm {
    pub fn backend_id(&self) -> &str {
        &self.backend_id
    }
}
impl GuiBackendForm {
    pub fn gpu_device(&self) -> Option<&str> {
        self.gpu_device.as_deref()
    }
}
impl GuiBackendForm {
    pub const fn allow_fallback(&self) -> bool {
        self.allow_fallback
    }
}
impl GuiBackendForm {
    pub const fn workers(&self) -> u16 {
        self.workers
    }
}
impl GuiBackendForm {
    pub const fn with_tablebase_requested(mut self, value: bool) -> Self {
        self.tablebase_requested = value;
        self
    }
}
impl GuiBackendForm {
    pub const fn workers_requested(&self) -> Option<u16> {
        if self.workers_explicit {
            Some(self.workers)
        } else {
            None
        }
    }
}
impl GuiBackendForm {
    pub const fn use_all_logical_processors(&self) -> bool {
        self.use_all_logical_processors
    }
}
impl GuiBackendForm {
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
}
impl GuiBackendForm {
    pub const fn precompute_build_dependencies(&self) -> bool {
        self.precompute_build_dependencies
    }
}
impl GuiBackendForm {
    pub const fn tablebase_requested(&self) -> bool {
        self.tablebase_requested
    }
}
impl GuiBackendForm {
    pub const fn memory_budget_mb(&self) -> u32 {
        self.memory_budget_mb
    }
}
impl GuiBackendForm {
    pub const fn candidate_budget(&self) -> u32 {
        self.candidate_budget
    }
}
impl GuiBackendForm {
    pub const fn pattern_budget(&self) -> u32 {
        self.pattern_budget
    }
}

impl Default for GuiBackendForm {
    fn default() -> Self {
        Self {
            backend: GuiBackendChoice::Auto,
            backend_id: GuiBackendChoice::Auto.as_str().to_owned(),
            gpu_device: None,
            allow_fallback: true,
            workers: 0,
            workers_explicit: false,
            use_all_logical_processors: false,
            deterministic: true,
            precompute_build_dependencies: false,
            tablebase_requested: false,
            memory_budget_mb: 0,
            candidate_budget: 4096,
            pattern_budget: 1024,
        }
    }
}
