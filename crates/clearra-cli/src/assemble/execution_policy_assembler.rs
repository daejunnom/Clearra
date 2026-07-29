use clearra_pc_graph::request::{GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend};

use crate::args::{pc_args::PcArgs, pc_scenario_args::PcScenarioArgs};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionPolicyAssemblyError {
    UnknownBackend {
        value: String,
    },
    InvalidGpuDevice {
        value: String,
    },
    WorkerCountRequiresAllLogicalProcessorsOptIn {
        requested: usize,
        default_limit: usize,
        available: usize,
    },
    WorkerCountExceedsHardware {
        requested: usize,
        available: usize,
    },
}

impl ExecutionPolicyAssemblyError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownBackend { value } => {
                format!("unsupported execution backend '{value}'")
            }
            Self::InvalidGpuDevice { value } => {
                format!("unsupported gpu device selector '{value}'")
            }
            Self::WorkerCountRequiresAllLogicalProcessorsOptIn {
                requested,
                default_limit,
                available,
            } => format!(
                "requested {requested} workers; the default limit is {default_limit} of {available} logical processors, so pass --use-all-cpu-threads to use the reserved processor"
            ),
            Self::WorkerCountExceedsHardware {
                requested,
                available,
            } => format!(
                "requested {requested} workers but the hard limit is {available} logical processors"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutionPolicyAssembler;

impl ExecutionPolicyAssembler {
    pub fn from_pc_args(args: &PcArgs) -> Result<PcExecutionPolicy, ExecutionPolicyAssemblyError> {
        assemble_policy(ExecutionPolicyInput {
            backend: args.backend(),
            workers: args.workers(),
            use_all_logical_processors: args.use_all_logical_processors(),
            cpu_warmup: args.cpu_warmup(),
            gpu_warmup: args.gpu_warmup(),
            tablebase_requested: args.tablebase_requested(),
            precompute_build_dependencies: args.precompute_build_dependencies(),
            deterministic: args.deterministic(),
            max_frontier_states: args.max_frontier_states(),
            max_candidates: args.max_candidates(),
            max_patterns: args.max_patterns(),
            max_memory_mib: args.max_memory_mib(),
            gpu_device: args.gpu_device(),
            allow_backend_fallback: args.allow_backend_fallback(),
        })
    }
}
impl ExecutionPolicyAssembler {
    pub fn from_pc_scenario_args(
        args: &PcScenarioArgs,
    ) -> Result<PcExecutionPolicy, ExecutionPolicyAssemblyError> {
        assemble_policy(ExecutionPolicyInput {
            backend: args.backend(),
            workers: args.workers(),
            use_all_logical_processors: args.use_all_logical_processors(),
            cpu_warmup: args.cpu_warmup(),
            gpu_warmup: args.gpu_warmup(),
            tablebase_requested: None,
            precompute_build_dependencies: None,
            deterministic: args.deterministic(),
            max_frontier_states: args.max_frontier_states(),
            max_candidates: args.max_candidates(),
            max_patterns: args.max_patterns(),
            max_memory_mib: args.max_memory_mib(),
            gpu_device: args.gpu_device(),
            allow_backend_fallback: args.allow_backend_fallback(),
        })
    }

    pub fn overlay_pc_scenario_args(
        base: PcExecutionPolicy,
        args: &PcScenarioArgs,
    ) -> Result<PcExecutionPolicy, ExecutionPolicyAssemblyError> {
        assemble_policy_on(
            base,
            ExecutionPolicyInput {
                backend: args.backend(),
                workers: args.workers(),
                use_all_logical_processors: args.use_all_logical_processors(),
                cpu_warmup: args.cpu_warmup(),
                gpu_warmup: args.gpu_warmup(),
                tablebase_requested: None,
                precompute_build_dependencies: None,
                deterministic: args.deterministic(),
                max_frontier_states: args.max_frontier_states(),
                max_candidates: args.max_candidates(),
                max_patterns: args.max_patterns(),
                max_memory_mib: args.max_memory_mib(),
                gpu_device: args.gpu_device(),
                allow_backend_fallback: args.allow_backend_fallback(),
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutionPolicyInput<'a> {
    pub backend: Option<&'a str>,
    pub workers: Option<usize>,
    pub use_all_logical_processors: Option<bool>,
    pub cpu_warmup: Option<bool>,
    pub gpu_warmup: Option<bool>,
    pub tablebase_requested: Option<bool>,
    pub precompute_build_dependencies: Option<bool>,
    pub deterministic: Option<bool>,
    pub max_frontier_states: Option<usize>,
    pub max_candidates: Option<usize>,
    pub max_patterns: Option<usize>,
    pub max_memory_mib: Option<usize>,
    pub gpu_device: Option<&'a str>,
    pub allow_backend_fallback: Option<bool>,
}

pub fn assemble_policy(
    input: ExecutionPolicyInput<'_>,
) -> Result<PcExecutionPolicy, ExecutionPolicyAssemblyError> {
    assemble_policy_on(PcExecutionPolicy::mvp_default(), input)
}

fn assemble_policy_on(
    mut policy: PcExecutionPolicy,
    input: ExecutionPolicyInput<'_>,
) -> Result<PcExecutionPolicy, ExecutionPolicyAssemblyError> {
    if let Some(value) = input.backend {
        let backend = RequestedSearchBackend::parse(value).ok_or_else(|| {
            ExecutionPolicyAssemblyError::UnknownBackend {
                value: value.to_owned(),
            }
        })?;
        if !matches!(
            backend,
            RequestedSearchBackend::Auto
                | RequestedSearchBackend::Cpu
                | RequestedSearchBackend::Gpu
                | RequestedSearchBackend::Hybrid
        ) {
            return Err(ExecutionPolicyAssemblyError::UnknownBackend {
                value: value.to_owned(),
            });
        }
        policy = policy.with_requested_backend(backend);
    }
    let use_all_logical_processors = input
        .use_all_logical_processors
        .unwrap_or(policy.use_all_logical_processors());
    if let Some(workers) = input.workers {
        let available = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
        let default_limit = clearra_pc_graph::request::WorkerPolicy::default_worker_limit();
        if workers > available {
            return Err(ExecutionPolicyAssemblyError::WorkerCountExceedsHardware {
                requested: workers,
                available,
            });
        }
        if workers > default_limit && !use_all_logical_processors {
            return Err(
                ExecutionPolicyAssemblyError::WorkerCountRequiresAllLogicalProcessorsOptIn {
                    requested: workers,
                    default_limit,
                    available,
                },
            );
        }
        policy = policy.with_workers(workers);
    }
    policy = policy.with_use_all_logical_processors(use_all_logical_processors);
    if let Some(cpu_warmup) = input.cpu_warmup {
        policy = policy.with_cpu_warmup(cpu_warmup);
    }
    if let Some(gpu_warmup) = input.gpu_warmup {
        policy = policy.with_gpu_warmup(gpu_warmup);
    }
    if let Some(tablebase_requested) = input.tablebase_requested {
        policy = policy.with_tablebase_requested(tablebase_requested);
    }
    if let Some(precompute_build_dependencies) = input.precompute_build_dependencies {
        policy = policy.with_precompute_build_dependencies(precompute_build_dependencies);
    }
    if let Some(deterministic) = input.deterministic {
        policy = policy.with_deterministic(deterministic);
    }
    if let Some(max_frontier_states) = input.max_frontier_states {
        policy = policy.with_max_frontier_states(max_frontier_states);
    }
    if let Some(max_candidates) = input.max_candidates {
        policy = policy.with_max_candidates(max_candidates);
    }
    if let Some(max_patterns) = input.max_patterns {
        policy = policy.with_max_patterns(max_patterns);
    }
    if let Some(max_memory_mib) = input.max_memory_mib {
        policy = policy.with_max_memory_mib(Some(max_memory_mib as u64));
    }
    if let Some(value) = input.gpu_device {
        let gpu_device = GpuDeviceSelection::parse(value).ok_or_else(|| {
            ExecutionPolicyAssemblyError::InvalidGpuDevice {
                value: value.to_owned(),
            }
        })?;
        policy = policy.with_gpu_device(gpu_device);
    }
    if let Some(allow_backend_fallback) = input.allow_backend_fallback {
        policy = policy.with_allow_backend_fallback(allow_backend_fallback);
    }

    Ok(policy)
}

#[cfg(test)]
#[path = "execution_policy_assembler_tests.rs"]
mod tests;
