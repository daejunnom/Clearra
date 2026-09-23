#![cfg(all(feature = "parallel", feature = "local-search-ab"))]

//! Local-only, full-family differential for a generated legal-board candidate.
//! The generator's internal proof chain is not used as the result oracle here:
//! the ordinary exact PC search must return the same complete canonical set
//! with and without the candidate filter. This is one KAT, not profile-wide
//! asset qualification or a performance A/B.

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Instant};

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
};
use clearra_core_executor::{
    built_in_legal_board_binding, install_local_pc4_legal_board_index,
    set_local_search_prune_policy, CoreExecutionResult, LegalBoardExpectation,
    LocalPc4LegalBoardIndex, LocalSearchPrunePolicy, WasmCpuSearchBackend,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::{
    kicks::KickTableProfileId,
    profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
};

#[test]
#[ignore = "requires one local generated profile bundle and a full 4L P7P4 exact search"]
fn generated_bundle_preserves_complete_p7p4_solution_identity_set() {
    let profile_name = std::env::var("CLEARRA_LOCAL_PC4_LEGAL_BOARD_PROFILE")
        .expect("set CLEARRA_LOCAL_PC4_LEGAL_BOARD_PROFILE explicitly");
    let profile = KickTableProfileId::parse(&profile_name).expect("known kick-table profile");
    let workers = std::env::var("CLEARRA_LOCAL_PC4_KAT_WORKERS")
        .ok()
        .map(|raw| raw.parse::<usize>().expect("KAT worker count is numeric"))
        .unwrap_or(4);
    assert!((1..=64).contains(&workers), "KAT worker count is bounded");
    let baseline_only = std::env::var("CLEARRA_LOCAL_PC4_KAT_MODE")
        .ok()
        .is_some_and(|value| value == "baseline-only");
    let previous = set_local_search_prune_policy(LocalSearchPrunePolicy::product_default());
    let started = Instant::now();
    let baseline = execute_p7p4(profile, workers);
    report_result(
        "baseline",
        workers,
        started.elapsed().as_millis(),
        &baseline,
    );
    assert_complete_flags(&baseline);
    if baseline_only {
        set_local_search_prune_policy(previous);
        assert_known_count(&baseline, profile);
        return;
    }

    let bundle_path = PathBuf::from(
        std::env::var_os("CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE")
            .expect("set CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE to the generated bundle"),
    );

    let bytes = std::fs::read(&bundle_path).expect("read local candidate bundle");
    let binding = built_in_legal_board_binding(profile).expect("legal-board profile binding");
    let index = LocalPc4LegalBoardIndex::load_bundle(
        Arc::from(bytes),
        LegalBoardExpectation {
            binding,
            generation_identity: None,
        },
    )
    .expect("generated exact-intersection bundle parses for its own profile");
    install_local_pc4_legal_board_index(index).expect("isolated test has one installed bundle");
    set_local_search_prune_policy(LocalSearchPrunePolicy::product_default().with_legal_board(true));
    let started = Instant::now();
    let filtered = execute_p7p4(profile, workers);
    set_local_search_prune_policy(previous);

    report_result(
        "filtered",
        workers,
        started.elapsed().as_millis(),
        &filtered,
    );
    assert_complete_flags(&filtered);
    if baseline.normalized_solution_identities() != filtered.normalized_solution_identities() {
        let baseline_set: HashSet<_> = baseline
            .normalized_solution_identities()
            .iter()
            .copied()
            .collect();
        let filtered_set: HashSet<_> = filtered
            .normalized_solution_identities()
            .iter()
            .copied()
            .collect();
        let missing: Vec<_> = baseline_set.difference(&filtered_set).take(3).collect();
        let extra: Vec<_> = filtered_set.difference(&baseline_set).take(3).collect();
        eprintln!("candidate legal-board identity delta: missing={missing:?} extra={extra:?}");
    }
    assert_eq!(
        filtered.usize_field("normalized_unique_solution_count"),
        baseline.usize_field("normalized_unique_solution_count")
    );
    assert_eq!(
        filtered.field("normalized_solution_set_hash"),
        baseline.field("normalized_solution_set_hash")
    );
    assert!(
        filtered.normalized_solution_identities() == baseline.normalized_solution_identities(),
        "candidate legal-board changed the complete canonical solution identity set"
    );
    assert!(
        filtered.normalized_solution_coverages() == baseline.normalized_solution_coverages(),
        "candidate legal-board changed complete solution coverage"
    );
    assert_known_count(&baseline, profile);
}

fn execute_p7p4(profile: KickTableProfileId, workers: usize) -> CoreExecutionResult {
    let rule = match profile {
        KickTableProfileId::Srs90 => srs(),
        KickTableProfileId::SrsPlus => srs_plus(),
        KickTableProfileId::SrsX => srs_x(),
        KickTableProfileId::Jstris180 => jstris_180(),
        KickTableProfileId::NoKick => no_kick(),
        _ => panic!("unsupported legal-board profile"),
    };
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_worker_hardware_limit(workers)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(true);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_rule(rule)
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("exact P7P4 problem");
    WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("exact P7P4 CPU search")
}

fn report_result(
    stage: &str,
    requested_workers: usize,
    elapsed_ms: u128,
    result: &CoreExecutionResult,
) {
    eprintln!(
        "candidate legal-board {stage}: elapsed_ms={elapsed_ms} requested_workers={requested_workers} workers_used={:?} active_workers={:?} cpu_parallel={:?} decision={:?} warmup={:?} count={:?} hash={:?} geometry_nodes={:?} build_order_nodes={:?} worker_min_candidates={:?} worker_max_candidates={:?}",
        result.usize_field("workers_used"),
        result.usize_field("parallel_active_workers"),
        result.bool_field("cpu_parallel_execution"),
        result.field("cpu_parallel_decision_reason"),
        result.bool_field("cpu_warmup_performed"),
        result.usize_field("normalized_unique_solution_count"),
        result.field("normalized_solution_set_hash"),
        result.usize_field("searched_nodes"),
        result.usize_field("total_build_order_nodes"),
        result.usize_field("parallel_minimum_worker_candidates"),
        result.usize_field("parallel_maximum_worker_candidates"),
    );
}

fn assert_complete_flags(result: &CoreExecutionResult) {
    assert_eq!(result.bool_field("count_complete"), Some(true));
    assert_eq!(result.bool_field("probability_complete"), Some(true));
}

fn assert_known_count(result: &CoreExecutionResult, profile: KickTableProfileId) {
    let known_count = match profile {
        KickTableProfileId::SrsPlus => Some(456_923),
        KickTableProfileId::Jstris180 => Some(456_459),
        _ => None,
    };
    if let Some(known_count) = known_count {
        assert_eq!(
            result.usize_field("normalized_unique_solution_count"),
            Some(known_count)
        );
    }
}
