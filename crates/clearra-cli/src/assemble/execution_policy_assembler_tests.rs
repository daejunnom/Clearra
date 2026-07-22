use super::*;

#[test]
fn assembles_execution_policy_from_raw_cli_values() {
    let workers = clearra_pc_graph::request::WorkerPolicy::default_worker_limit().min(4);
    let args = PcArgs::new(2)
        .with_backend(Some("hybrid".to_owned()))
        .with_workers(Some(workers))
        .with_deterministic(Some(true))
        .with_max_frontier_states(Some(128))
        .with_max_candidates(Some(96))
        .with_max_patterns(Some(64))
        .with_max_memory_mib(Some(512))
        .with_gpu_device(Some("auto".to_owned()))
        .with_allow_backend_fallback(Some(false));

    let policy = ExecutionPolicyAssembler::from_pc_args(&args).expect("policy");

    assert_eq!(policy.requested_backend(), RequestedSearchBackend::Hybrid);
    assert_eq!(policy.workers(), workers);
    assert!(policy.deterministic());
    assert_eq!(policy.max_frontier_states(), 128);
    assert_eq!(policy.max_candidates(), 96);
    assert_eq!(policy.max_patterns(), 64);
    assert_eq!(policy.max_memory_mib(), Some(512));
    assert_eq!(policy.gpu_device(), &GpuDeviceSelection::Auto);
    assert!(!policy.allow_backend_fallback());
}

#[test]
fn all_logical_processors_require_the_explicit_cli_opt_in() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    if hardware <= 1 {
        return;
    }
    let without_opt_in = PcArgs::new(2).with_workers(Some(hardware));
    assert!(matches!(
        ExecutionPolicyAssembler::from_pc_args(&without_opt_in),
        Err(ExecutionPolicyAssemblyError::WorkerCountRequiresAllLogicalProcessorsOptIn { .. })
    ));

    let with_opt_in = without_opt_in.with_use_all_logical_processors(Some(true));
    let policy = ExecutionPolicyAssembler::from_pc_args(&with_opt_in).expect("all-CPU policy");
    assert_eq!(policy.workers(), hardware);
    assert!(policy.use_all_logical_processors());
}

#[test]
fn rejects_unknown_backend_before_query_validation() {
    let args = PcArgs::new(2).with_backend(Some("quantum".to_owned()));

    assert_eq!(
        ExecutionPolicyAssembler::from_pc_args(&args),
        Err(ExecutionPolicyAssemblyError::UnknownBackend {
            value: "quantum".to_owned()
        })
    );
}

#[test]
fn rejects_internal_backend_names_from_cli_surface() {
    let args = PcArgs::new(2).with_backend(Some("gpu-bfs".to_owned()));

    assert_eq!(
        ExecutionPolicyAssembler::from_pc_args(&args),
        Err(ExecutionPolicyAssemblyError::UnknownBackend {
            value: "gpu-bfs".to_owned()
        })
    );
}

#[test]
fn assembles_m19_user_facing_backend_policy_options() {
    let args = PcArgs::new(2)
        .with_backend(Some("gpu".to_owned()))
        .with_max_candidates(Some(128))
        .with_max_patterns(Some(256))
        .with_gpu_device(Some("1".to_owned()))
        .with_allow_backend_fallback(Some(true));

    let policy = ExecutionPolicyAssembler::from_pc_args(&args).expect("policy");

    assert_eq!(policy.requested_backend(), RequestedSearchBackend::Gpu);
    assert_eq!(policy.max_candidates(), 128);
    assert_eq!(policy.max_patterns(), 256);
    assert_eq!(policy.gpu_device(), &GpuDeviceSelection::Index(1));
    assert!(policy.allow_backend_fallback());
}
