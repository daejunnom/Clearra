mod backend_fallback_policy {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BackendFallbackPolicy {
        Allow,
        Deny,
    }

    impl BackendFallbackPolicy {
        pub fn is_allowed(self) -> bool {
            matches!(self, Self::Allow)
        }
    }
    impl BackendFallbackPolicy {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Allow => "allow",
                Self::Deny => "deny",
            }
        }
    }

    impl Default for BackendFallbackPolicy {
        fn default() -> Self {
            Self::Allow
        }
    }
}
mod gpu_device_selection {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub enum GpuDeviceSelection {
        #[default]
        Auto,
        Index(u8),
    }

    impl GpuDeviceSelection {
        pub fn parse(value: &str) -> Option<Self> {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.is_empty() || normalized == "auto" {
                return Some(Self::Auto);
            }
            normalized.parse::<u8>().ok().map(Self::Index)
        }
    }
    impl GpuDeviceSelection {
        pub fn as_display_string(&self) -> String {
            match self {
                Self::Auto => "auto".to_owned(),
                Self::Index(index) => index.to_string(),
            }
        }
    }
}
mod policy {
    use clearra_profiles::search::search_defaults::SearchDefaults;

    use super::{BackendFallbackPolicy, GpuDeviceSelection, RequestedSearchBackend, WorkerPolicy};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PcExecutionPolicy {
        requested_backend: RequestedSearchBackend,
        worker_policy: WorkerPolicy,
        automatic_worker_limit: Option<usize>,
        worker_hardware_limit: usize,
        runtime_webgpu_available: bool,
        use_all_logical_processors: bool,
        cpu_warmup: bool,
        gpu_warmup: bool,
        tablebase_requested: bool,
        precompute_build_dependencies: bool,
        deterministic: bool,
        max_nodes: usize,
        max_frontier_states: usize,
        max_candidates: usize,
        max_patterns: usize,
        max_memory_mib: Option<u64>,
        gpu_device: GpuDeviceSelection,
        backend_fallback: BackendFallbackPolicy,
    }

    impl PcExecutionPolicy {
        pub fn mvp_default() -> Self {
            SearchDefaults::MVP1.into()
        }
    }
    impl PcExecutionPolicy {
        pub fn requested_backend(&self) -> RequestedSearchBackend {
            self.requested_backend
        }
    }
    impl PcExecutionPolicy {
        pub fn backend(&self) -> RequestedSearchBackend {
            self.requested_backend()
        }
    }
    impl PcExecutionPolicy {
        pub fn worker_policy(&self) -> WorkerPolicy {
            self.worker_policy
        }
    }
    impl PcExecutionPolicy {
        pub fn workers(&self) -> usize {
            let workers = self.worker_policy.effective_for_hardware_limit(
                self.use_all_logical_processors,
                self.worker_hardware_limit,
            );
            self.automatic_worker_limit
                .map_or(workers, |limit| workers.min(limit.max(1)))
        }
    }
    impl PcExecutionPolicy {
        pub fn worker_hardware_limit(&self) -> usize {
            self.worker_hardware_limit
        }
    }
    impl PcExecutionPolicy {
        pub fn runtime_webgpu_available(&self) -> bool {
            self.runtime_webgpu_available
        }
    }
    impl PcExecutionPolicy {
        pub fn use_all_logical_processors(&self) -> bool {
            self.use_all_logical_processors
        }
    }
    impl PcExecutionPolicy {
        pub fn cpu_warmup(&self) -> bool {
            self.cpu_warmup
        }
    }
    impl PcExecutionPolicy {
        pub fn gpu_warmup(&self) -> bool {
            self.gpu_warmup
        }
    }
    impl PcExecutionPolicy {
        pub fn tablebase_requested(&self) -> bool {
            self.tablebase_requested
        }
    }
    impl PcExecutionPolicy {
        pub fn precompute_build_dependencies(&self) -> bool {
            self.precompute_build_dependencies
        }
    }
    impl PcExecutionPolicy {
        pub fn workers_requested(&self) -> Option<usize> {
            self.worker_policy.requested_workers()
        }
    }
    impl PcExecutionPolicy {
        pub fn deterministic(&self) -> bool {
            self.deterministic
        }
    }
    impl PcExecutionPolicy {
        pub fn max_nodes(&self) -> usize {
            self.max_nodes
        }
    }
    impl PcExecutionPolicy {
        pub fn max_frontier_states(&self) -> usize {
            self.max_frontier_states
        }
    }
    impl PcExecutionPolicy {
        pub fn max_candidates(&self) -> usize {
            self.max_candidates
        }
    }
    impl PcExecutionPolicy {
        pub fn max_patterns(&self) -> usize {
            self.max_patterns
        }
    }
    impl PcExecutionPolicy {
        pub fn max_memory_mib(&self) -> Option<u64> {
            self.max_memory_mib
        }
    }
    impl PcExecutionPolicy {
        pub fn gpu_device(&self) -> &GpuDeviceSelection {
            &self.gpu_device
        }
    }
    impl PcExecutionPolicy {
        pub fn backend_fallback(&self) -> BackendFallbackPolicy {
            self.backend_fallback
        }
    }
    impl PcExecutionPolicy {
        pub fn allow_backend_fallback(&self) -> bool {
            self.backend_fallback.is_allowed()
        }
    }
    impl PcExecutionPolicy {
        pub fn with_requested_backend(mut self, backend: RequestedSearchBackend) -> Self {
            self.requested_backend = backend;
            self.backend_fallback = if matches!(backend, RequestedSearchBackend::Auto) {
                BackendFallbackPolicy::Allow
            } else {
                BackendFallbackPolicy::Deny
            };
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_backend(self, backend: RequestedSearchBackend) -> Self {
            self.with_requested_backend(backend)
        }
    }
    impl PcExecutionPolicy {
        pub fn with_worker_policy(mut self, worker_policy: WorkerPolicy) -> Self {
            self.worker_policy = worker_policy;
            self.automatic_worker_limit = None;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_workers(mut self, workers: usize) -> Self {
            self.worker_policy = WorkerPolicy::Fixed(workers);
            self.automatic_worker_limit = None;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_automatic_worker_limit(mut self, workers: usize) -> Self {
            self.worker_policy = WorkerPolicy::Auto;
            self.automatic_worker_limit = Some(workers.max(1));
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_worker_hardware_limit(mut self, workers: usize) -> Self {
            self.worker_hardware_limit = workers.max(1);
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_runtime_webgpu_available(mut self, available: bool) -> Self {
            self.runtime_webgpu_available = available;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_use_all_logical_processors(mut self, value: bool) -> Self {
            self.use_all_logical_processors = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_cpu_warmup(mut self, value: bool) -> Self {
            self.cpu_warmup = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_gpu_warmup(mut self, value: bool) -> Self {
            self.gpu_warmup = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_tablebase_requested(mut self, value: bool) -> Self {
            self.tablebase_requested = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_precompute_build_dependencies(mut self, value: bool) -> Self {
            self.precompute_build_dependencies = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_deterministic(mut self, deterministic: bool) -> Self {
            self.deterministic = deterministic;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_max_nodes(mut self, value: usize) -> Self {
            self.max_nodes = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_max_frontier_states(mut self, value: usize) -> Self {
            self.max_frontier_states = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_max_candidates(mut self, value: usize) -> Self {
            self.max_candidates = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_max_patterns(mut self, value: usize) -> Self {
            self.max_patterns = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_max_memory_mib(mut self, value: Option<u64>) -> Self {
            self.max_memory_mib = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_gpu_device(mut self, value: GpuDeviceSelection) -> Self {
            self.gpu_device = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_backend_fallback(mut self, value: BackendFallbackPolicy) -> Self {
            self.backend_fallback = value;
            self
        }
    }
    impl PcExecutionPolicy {
        pub fn with_allow_backend_fallback(mut self, value: bool) -> Self {
            self.backend_fallback = if value {
                BackendFallbackPolicy::Allow
            } else {
                BackendFallbackPolicy::Deny
            };
            self
        }
    }

    impl From<SearchDefaults> for PcExecutionPolicy {
        fn from(defaults: SearchDefaults) -> Self {
            Self {
                requested_backend: RequestedSearchBackend::Auto,
                worker_policy: WorkerPolicy::Auto,
                automatic_worker_limit: None,
                worker_hardware_limit: WorkerPolicy::hardware_worker_limit(),
                runtime_webgpu_available: true,
                use_all_logical_processors: false,
                cpu_warmup: false,
                gpu_warmup: false,
                tablebase_requested: false,
                precompute_build_dependencies: false,
                deterministic: defaults.execution_deterministic(),
                max_nodes: defaults.max_nodes(),
                max_frontier_states: defaults.execution_max_frontier_states(),
                max_candidates: defaults.execution_max_candidates(),
                max_patterns: defaults.execution_max_patterns(),
                max_memory_mib: defaults.execution_max_memory_mib(),
                gpu_device: GpuDeviceSelection::Auto,
                backend_fallback: BackendFallbackPolicy::Allow,
            }
        }
    }

    impl Default for PcExecutionPolicy {
        fn default() -> Self {
            Self::mvp_default()
        }
    }
}
mod requested_search_backend {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum RequestedSearchBackend {
        #[default]
        Auto,
        Cpu,
        Gpu,
        Hybrid,
    }

    impl RequestedSearchBackend {
        pub const ALL: [Self; 4] = [Self::Auto, Self::Cpu, Self::Gpu, Self::Hybrid];
    }
    impl RequestedSearchBackend {
        pub fn parse(value: &str) -> Option<Self> {
            match value.trim().to_ascii_lowercase().as_str() {
                "" | "auto" => Some(Self::Auto),
                "cpu" => Some(Self::Cpu),
                "gpu" => Some(Self::Gpu),
                "hybrid" => Some(Self::Hybrid),
                _ => None,
            }
        }
    }
    impl RequestedSearchBackend {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Auto => "auto",
                Self::Cpu => "cpu",
                Self::Gpu => "gpu",
                Self::Hybrid => "hybrid",
            }
        }
    }
    impl RequestedSearchBackend {
        pub fn label(self) -> &'static str {
            match self {
                Self::Auto => "Auto",
                Self::Cpu => "CPU",
                Self::Gpu => "GPU",
                Self::Hybrid => "Hybrid",
            }
        }
    }
    impl RequestedSearchBackend {
        pub fn requires_gpu(self) -> bool {
            matches!(self, Self::Gpu | Self::Hybrid)
        }
    }
}
mod worker_policy {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WorkerPolicy {
        Fixed(usize),
        Auto,
    }

    impl WorkerPolicy {
        pub fn fixed(workers: usize) -> Self {
            Self::Fixed(workers)
        }
    }
    impl WorkerPolicy {
        pub fn requested_workers(self) -> Option<usize> {
            match self {
                Self::Fixed(workers) => Some(workers),
                Self::Auto => None,
            }
        }
    }
    impl WorkerPolicy {
        pub fn hardware_worker_limit() -> usize {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .max(1)
        }
    }
    impl WorkerPolicy {
        pub fn default_worker_limit() -> usize {
            Self::default_worker_limit_for_hardware(Self::hardware_worker_limit())
        }

        pub fn default_worker_limit_for_hardware(hardware_limit: usize) -> usize {
            hardware_limit.max(1).saturating_sub(1).max(1)
        }
    }
    impl WorkerPolicy {
        pub fn clamp_requested(workers: usize, use_all_logical_processors: bool) -> usize {
            Self::clamp_requested_for_hardware(
                workers,
                use_all_logical_processors,
                Self::hardware_worker_limit(),
            )
        }

        pub fn clamp_requested_for_hardware(
            workers: usize,
            use_all_logical_processors: bool,
            hardware_limit: usize,
        ) -> usize {
            let hardware_limit = hardware_limit.max(1);
            let limit = if use_all_logical_processors {
                hardware_limit
            } else {
                Self::default_worker_limit_for_hardware(hardware_limit)
            };
            workers.max(1).min(limit)
        }
    }
    impl WorkerPolicy {
        pub fn effective(self, use_all_logical_processors: bool) -> usize {
            self.effective_for_hardware_limit(
                use_all_logical_processors,
                Self::hardware_worker_limit(),
            )
        }

        pub fn effective_for_hardware_limit(
            self,
            use_all_logical_processors: bool,
            hardware_limit: usize,
        ) -> usize {
            let hardware_limit = hardware_limit.max(1);
            match self {
                Self::Fixed(workers) => Self::clamp_requested_for_hardware(
                    workers,
                    use_all_logical_processors,
                    hardware_limit,
                ),
                Self::Auto if use_all_logical_processors => hardware_limit,
                Self::Auto => Self::default_worker_limit_for_hardware(hardware_limit),
            }
        }
    }
    impl WorkerPolicy {
        pub fn as_str(self) -> String {
            match self {
                Self::Fixed(workers) => workers.to_string(),
                Self::Auto => "auto".to_owned(),
            }
        }
    }

    impl Default for WorkerPolicy {
        fn default() -> Self {
            Self::Auto
        }
    }
}

pub use backend_fallback_policy::BackendFallbackPolicy;
pub use gpu_device_selection::GpuDeviceSelection;
pub use policy::PcExecutionPolicy;
pub use requested_search_backend::RequestedSearchBackend;
pub use worker_policy::WorkerPolicy;

pub type PcExecutionBackend = RequestedSearchBackend;
pub type PcGpuDevice = GpuDeviceSelection;

#[cfg(test)]
use clearra_profiles::search::search_defaults::SearchDefaults;

#[cfg(test)]
#[path = "pc_execution_policy_tests.rs"]
mod tests;
