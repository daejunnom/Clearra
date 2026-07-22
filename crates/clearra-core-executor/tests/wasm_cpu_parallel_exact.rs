#![cfg(feature = "parallel")]

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
};
use clearra_core_executor::WasmCpuSearchBackend;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
    WorkerPolicy,
};
use clearra_problem::ProblemCompiler;

const P7P4_UNIQUE_TILING_COUNT: usize = 456_923;
const P7P4_NORMALIZED_SET_HASH: &str = "cts1:98ebe8726537b29f";

#[test]
fn small_exact_search_does_not_start_pool_for_serial_workload() {
    let workers = WorkerPolicy::default_worker_limit();
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_cpu_warmup(true);
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("small exact problem");
    let result = WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("small exact CPU search");

    assert_eq!(result.usize_field("workers_used"), Some(1));
    assert_eq!(result.bool_field("cpu_parallel_execution"), Some(false));
    assert_eq!(
        result.field("cpu_parallel_decision_reason"),
        Some("small-piece-count")
    );
    assert_eq!(result.bool_field("cpu_warmup_requested"), Some(true));
    assert_eq!(result.bool_field("cpu_warmup_performed"), Some(false));
    assert_eq!(result.bool_field("count_complete"), Some(true));
}

#[test]
#[ignore = "full P7P4 parallel exact-set regression"]
fn p7p4_parallel_matches_exact_solution_oracle() {
    let parallel_workers = WorkerPolicy::default_worker_limit();
    if parallel_workers <= 1 {
        return;
    }
    let parallel = execute_p7p4(parallel_workers, true);

    eprintln!(
        "P7P4 normalized set: count={:?}, hash={:?}, active={:?}, min={:?}, max={:?}",
        parallel.usize_field("normalized_unique_solution_count"),
        parallel.field("normalized_solution_set_hash"),
        parallel.usize_field("parallel_active_workers"),
        parallel.usize_field("parallel_minimum_worker_candidates"),
        parallel.usize_field("parallel_maximum_worker_candidates")
    );

    assert_eq!(
        parallel.usize_field("normalized_unique_solution_count"),
        Some(P7P4_UNIQUE_TILING_COUNT)
    );
    assert_eq!(
        parallel.field("normalized_solution_set_hash"),
        Some(P7P4_NORMALIZED_SET_HASH)
    );
    assert_eq!(parallel.bool_field("count_complete"), Some(true));
    assert_eq!(parallel.bool_field("probability_complete"), Some(true));
    assert_eq!(parallel.bool_field("cpu_parallel_execution"), Some(true));
    assert_eq!(
        parallel.field("cpu_parallel_decision_reason"),
        Some("parallel-immutable-family-queue")
    );
    assert_eq!(parallel.bool_field("cpu_warmup_performed"), Some(true));
    assert_eq!(parallel.usize_field("workers_used"), Some(parallel_workers));
    assert_eq!(
        parallel.usize_field("parallel_active_workers"),
        Some(parallel_workers)
    );
    assert!(parallel
        .usize_field("parallel_minimum_worker_candidates")
        .is_some_and(|candidates| candidates > 0));
}

fn execute_p7p4(workers: usize, warmup: bool) -> clearra_core_executor::CoreExecutionResult {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_cpu_warmup(warmup);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("P7P4 problem");
    let cancellation = ExecutionCancellationToken::new();
    WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::new(cancellation))
        .expect("exact CPU search")
}
