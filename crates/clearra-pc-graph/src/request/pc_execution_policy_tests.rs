use super::*;

#[test]
fn mvp_execution_policy_comes_from_profile_defaults() {
    let policy = PcExecutionPolicy::mvp_default();

    assert_eq!(policy.requested_backend(), RequestedSearchBackend::Auto);
    assert_eq!(policy.backend(), RequestedSearchBackend::Auto);
    assert_eq!(policy.worker_policy(), WorkerPolicy::Auto);
    assert!(policy.workers() >= 1);
    assert_eq!(policy.workers_requested(), None);
    assert!(policy.deterministic());
    assert_eq!(
        policy.max_frontier_states(),
        SearchDefaults::MVP1.execution_max_frontier_states()
    );
    assert_eq!(
        policy.max_candidates(),
        SearchDefaults::MVP1.execution_max_candidates()
    );
    assert_eq!(
        policy.max_patterns(),
        SearchDefaults::MVP1.execution_max_patterns()
    );
    assert_eq!(
        policy.max_memory_mib(),
        SearchDefaults::MVP1.execution_max_memory_mib()
    );
    assert_eq!(policy.gpu_device(), &GpuDeviceSelection::Auto);
    assert_eq!(policy.backend_fallback(), BackendFallbackPolicy::Allow);
    assert!(policy.allow_backend_fallback());
    assert!(!policy.precompute_build_dependencies());
}

#[test]
fn build_dependency_precomputation_requires_explicit_opt_in() {
    let enabled = PcExecutionPolicy::mvp_default().with_precompute_build_dependencies(true);

    assert!(enabled.precompute_build_dependencies());
    assert!(!PcExecutionPolicy::mvp_default().precompute_build_dependencies());
}

#[test]
fn worker_policy_reserves_one_logical_processor_unless_explicitly_requested() {
    let hardware = WorkerPolicy::hardware_worker_limit();
    let default_limit = hardware.saturating_sub(1).max(1);

    assert_eq!(WorkerPolicy::default_worker_limit(), default_limit);
    assert_eq!(PcExecutionPolicy::mvp_default().workers(), default_limit);
    assert_eq!(
        PcExecutionPolicy::mvp_default()
            .with_use_all_logical_processors(true)
            .workers(),
        hardware
    );
    assert_eq!(
        PcExecutionPolicy::mvp_default()
            .with_workers(hardware.saturating_add(1))
            .with_use_all_logical_processors(true)
            .workers(),
        hardware
    );
}

#[test]
fn execution_backend_strings_are_canonical() {
    assert_eq!(
        RequestedSearchBackend::ALL.map(RequestedSearchBackend::as_str),
        ["auto", "cpu", "gpu", "hybrid"]
    );
    assert_eq!(
        RequestedSearchBackend::parse("cpu"),
        Some(RequestedSearchBackend::Cpu)
    );
    assert_eq!(
        RequestedSearchBackend::parse("gpu"),
        Some(RequestedSearchBackend::Gpu)
    );
    assert_eq!(
        RequestedSearchBackend::parse("hybrid"),
        Some(RequestedSearchBackend::Hybrid)
    );
    assert_eq!(RequestedSearchBackend::parse("cpu_layered_bfs"), None);
    assert_eq!(
        RequestedSearchBackend::parse("cpu_parallel_layered_bfs"),
        None
    );
    assert_eq!(RequestedSearchBackend::parse("gpu-bfs"), None);
    assert_eq!(RequestedSearchBackend::parse("dfs"), None);
    assert_eq!(RequestedSearchBackend::parse("gpu-dfs"), None);
    assert_eq!(RequestedSearchBackend::parse("cpu-gpu-mixed"), None);
    assert_eq!(RequestedSearchBackend::parse("unknown"), None);
}

#[test]
fn explicit_backend_disables_fallback_until_user_allows_it() {
    let policy = PcExecutionPolicy::mvp_default().with_backend(RequestedSearchBackend::Gpu);

    assert_eq!(policy.backend_fallback(), BackendFallbackPolicy::Deny);
    assert!(!policy.allow_backend_fallback());
    assert!(policy
        .with_allow_backend_fallback(true)
        .allow_backend_fallback());
}
