//! A local evidence producer, not a user-visible backend selector.
//!
//! The C reference and Rust CPU consume the same compiled PC request, with
//! external accelerators disabled. Do not link this target into a release.

use std::{env, thread, time::Instant};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind::{I, O},
};
use clearra_core_executor::{CoreExecutionResult, PcService, WasmCpuSearchBackend};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompileError, ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
enum Arm {
    NativeC,
    RustCpu,
}

impl Arm {
    fn parse(arg: &str) -> Result<Self, String> {
        match arg {
            "native-c" => Ok(Self::NativeC),
            "rust-cpu" => Ok(Self::RustCpu),
            _ => Err("arm must be native-c or rust-cpu".into()),
        }
    }

    fn execute(
        self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, String> {
        match self {
            // PcService is the independent native-C packing and BuildUp
            // reference; do not route A through a switchable product default.
            Self::NativeC => PcService::execute_with_control(problem, control)
                .map_err(|error| format!("{error:?}")),
            Self::RustCpu => WasmCpuSearchBackend::execute_with_control(problem, control)
                .map_err(|error| format!("{error:?}")),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::NativeC => "native-c",
            Self::RustCpu => "rust-cpu",
        }
    }
}

fn compile_case(case: &str, workers: usize) -> Result<SearchProblem, String> {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_worker_hardware_limit(workers)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(false)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    match case {
        "fixed-2l" => ProblemCompiler::compile_opening_pc(
            &OpeningPcSearchQuery::new(PcTarget::two_lines())
                .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                    I, I, O, O, O,
                ])))
                .with_hold_policy(PcHoldPolicy::Disabled)
                .with_rule(srs_plus())
                .with_objective(ObjectivePolicy::all())
                .with_execution_policy(policy),
        ),
        "existing-p7" => compile_standard_bag_case(0x3_c0f0_3c0f, 6, policy),
        "full-p7p4" => compile_standard_bag_case(0, 10, policy),
        _ => return Err("case must be fixed-2l, existing-p7, or full-p7p4".into()),
    }
    .map_err(|error| format!("compile PC request: {error:?}"))
}

fn compile_standard_bag_case(
    initial_mask: u64,
    placements: usize,
    policy: PcExecutionPolicy,
) -> Result<SearchProblem, ProblemCompileError> {
    // Matches the historical CLI --board-mask/--height/--pieces route.
    // Bare P7/P7P4 are standard bag descriptors, with one lookahead for
    // empty hold and the product scenario's single retained witness.
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial_mask),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(placements),
        )
        .with_exact_pieces(Some(placements))
        .with_min_remaining_queue(0)
        .with_allow_hold(true)
        .with_rule(srs_plus())
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_supply_window_size(SupplyWindowSize::new(placements + 1))
        .with_retained_trace_limit(1)
        .with_execution_policy(policy),
    )
}

fn normalized_key_sequence_sha256(result: &CoreExecutionResult) -> Option<String> {
    let keys = result.normalized_solution_keys();
    // This is optional evidence. If a backend retains compact identities
    // only, its authoritative normalized-set hash is still reported as such.
    if keys.is_empty() || result.usize_field("normalized_unique_solution_count") != Some(keys.len())
    {
        return None;
    }
    let mut hash = Sha256::new();
    for key in keys {
        hash.update((key.len() as u64).to_le_bytes());
        hash.update(key.as_bytes());
    }
    Some(format!("{:x}", hash.finalize()))
}

fn coverage_sha256(result: &CoreExecutionResult) -> Option<String> {
    let words = result.coverage_pattern_words();
    if words.is_empty() {
        return None;
    }
    let mut hash = Sha256::new();
    for word in words {
        hash.update(word.to_le_bytes());
    }
    Some(format!("{:x}", hash.finalize()))
}

#[cfg(target_os = "linux")]
fn process_rss_peak_bytes() -> Option<u64> {
    // All solver workers are threads of this process. VmHWM is a kernel
    // process high-water mark; it is not the supervisor's sampled tree RSS.
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let mut fields = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_ascii_whitespace();
    let kibibytes = fields.next()?.parse::<u64>().ok()?;
    (fields.next() == Some("kB") && fields.next().is_none())
        .then_some(kibibytes)?
        .checked_mul(1024)
}

#[cfg(not(target_os = "linux"))]
fn process_rss_peak_bytes() -> Option<u64> {
    None
}

fn report(
    case: &str,
    arm: Arm,
    workers: usize,
    available_workers: usize,
    solve_ns: u128,
    result: &CoreExecutionResult,
) -> Value {
    let ordered_key_hash = normalized_key_sequence_sha256(result);
    let coverage_hash = coverage_sha256(result);
    json!({
        "schema": "clearra.native-rust-cpu-abba.sample.v1",
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
        "case": case,
        "arm": arm.label(),
        "requested_workers": workers,
        "available_parallelism": available_workers,
        "cpu_warmup": false,
        "solver_elapsed_ns": solve_ns.to_string(),
        "linux_process_rss_hwm_bytes": process_rss_peak_bytes(),
        "semantic": {
            "count_complete": result.bool_field("count_complete"),
            "probability_complete": result.bool_field("probability_complete"),
            "resource_truncated": result.bool_field("resource_truncated"),
            "solution_count": result.usize_field("normalized_unique_solution_count"),
            "solution_set_hash": result.field("normalized_solution_set_hash"),
            "ordered_key_sha256": ordered_key_hash,
            "pattern_count": result.usize_field("coverage_pattern_count"),
            "covered_pattern_count": result.usize_field("covered_pattern_count"),
            "coverage_sha256": coverage_hash,
            "build_variant_count": result.field("build_variant_count"),
        },
        "backend": {
            "selected": result.field("backend_selected"),
            "packing": result.field("candidate_backend"),
            "packing_execution_source": result.field("packing_execution_source"),
            "buildup": result.field("buildup_backend"),
            "buildup_execution_source": result.field("buildup_execution_source"),
            "fallback": result.bool_field("backend_fallback_used"),
            "reported_workers": result.usize_field("workers_used"),
            "reported_active_workers": result.usize_field("parallel_active_workers"),
            "native_geometry_dag_workers": result.usize_field("native_cpu_geometry_dag_workers"),
            "native_candidate_workers": result.usize_field("native_cpu_candidate_workers"),
            "native_buildup_workers": result.usize_field("native_cpu_buildup_workers"),
            "parallel": result.bool_field("cpu_parallel_execution"),
            "decision_reason": result.field("cpu_parallel_decision_reason"),
        },
        "work": {
            "geometry_searched_nodes": result.usize_field("searched_nodes"),
            "packing_candidates": result.usize_field("packing_candidate_count"),
            "build_order_nodes": result.usize_field("total_build_order_nodes"),
        },
    })
}

fn main() -> Result<(), String> {
    if env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
        || env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
    {
        return Err(
            "run this example under clearra-manage runtime run --profile benchmark-search".into(),
        );
    }
    let [arm, case, workers]: [String; 3] = env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "expected <arm> <case> <workers>".to_owned())?;
    let arm = Arm::parse(&arm)?;
    let available_workers = thread::available_parallelism().map_or(1, |available| available.get());
    let workers = workers
        .parse::<usize>()
        .ok()
        .filter(|&workers| (1..=available_workers).contains(&workers))
        .ok_or_else(|| format!("workers must be 1..={available_workers}"))?;
    let problem = compile_case(&case, workers)?;
    let control = ExecutionControl::default();
    let started = Instant::now();
    let result = arm.execute(&problem, &control)?;
    let solve_ns = started.elapsed().as_nanos();
    println!(
        "{}",
        report(&case, arm, workers, available_workers, solve_ns, &result)
    );
    Ok(())
}
