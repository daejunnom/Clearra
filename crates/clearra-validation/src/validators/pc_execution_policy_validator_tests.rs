use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn default_execution_policy_is_valid() {
    let report = validate_pc_execution_policy(
        &PcExecutionPolicy::mvp_default(),
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::WPcBackendFallback));
}

#[test]
fn gpu_backend_capability_is_deferred_to_the_executor_without_fallback() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Gpu)
        .with_allow_backend_fallback(false);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::EBackendGpuFeatureDisabled));
    assert!(!report.contains_code(DiagnosticCode::EBackendGpuUnavailable));
}

#[test]
fn gpu_backend_capability_is_deferred_to_the_executor_with_fallback() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Gpu)
        .with_allow_backend_fallback(true);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::WBackendFallbackUsed));
}

#[test]
fn user_facing_gpu_backend_is_not_statically_classified_as_fallback() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Gpu)
        .with_allow_backend_fallback(true);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::CountAll),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::WBackendFallbackUsed));
}

#[test]
fn user_facing_hybrid_backend_is_not_statically_classified_as_fallback() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Hybrid)
        .with_allow_backend_fallback(true);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::CountAll),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::WPcBackendFallback));
    assert!(!report.contains_code(DiagnosticCode::WBackendFallbackUsed));
}

#[test]
fn zero_search_limits_select_automatic_demand_growing_budgets() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Cpu)
        .with_max_frontier_states(0)
        .with_max_candidates(0)
        .with_max_patterns(0)
        .with_allow_backend_fallback(true);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::EBackendFrontierBudgetRequired));
}

#[test]
fn typed_policy_rejects_worker_requests_above_hardware() {
    let requested =
        clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit().saturating_add(1);
    let policy = PcExecutionPolicy::mvp_default()
        .with_workers(requested)
        .with_use_all_logical_processors(true);

    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(report.has_errors());
}

#[test]
fn typed_policy_rejects_zero_workers_before_clamping() {
    let policy = PcExecutionPolicy::mvp_default().with_workers(0);
    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(report.has_errors());
}

#[test]
fn typed_policy_requires_opt_in_for_the_reserved_logical_processor() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    if hardware <= 1 {
        return;
    }
    let policy = PcExecutionPolicy::mvp_default().with_workers(hardware);

    let report = validate_pc_execution_policy(
        &policy,
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    );

    assert!(report.has_errors());
    assert!(!validate_pc_execution_policy(
        &policy.with_use_all_logical_processors(true),
        PcBackendCompatibilityContext::scenario(PcCountPolicy::FirstSolution),
        "pc.execution",
    )
    .has_errors());
}
