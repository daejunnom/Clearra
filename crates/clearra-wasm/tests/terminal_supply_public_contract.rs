// SRP rationale: this integration test has one behavior-level change reason:
// preserving the finite terminal-hold solution set across the public CLI/App/WASM boundary.

use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppServices, AppStatus,
    DistributedSearchPreparation,
};
use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSet,
    },
};
use clearra_host_contract::ProductBuildIdentity;
use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend, WorkerPolicy};
use clearra_wasm::{
    serialize_search_report_from_app_response, WasmCommandRuntime, WasmHostCapabilities,
    WasmWorkerAdvanceStatus, WasmWorkerJobRuntime,
};
use serde_json::Value;

const TERMINAL_SUPPLY_P0_COMMAND: &str = "clearra pc \
    --board-mask 0x1c0701c07 --height 4 --pieces 7 \
    --queue STOILJZ --hold empty --count unique --objective all \
    --backend cpu --workers 1";
const TERMINAL_SUPPLY_P0_INITIAL_MASK: u64 = 0x1c0701c07;
const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT: usize = 18;
const TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH: &str = "cts1:8a7fc484d9b49994";

#[test]
fn typed_tiling_fifteen_locks_with_initial_hold_and_long_source_prepare_exactly() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(8, false, false));
    let app = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for workers in [1, 8] {
        for source_draws in [14, 16] {
            let command = format!(
                "clearra pc tiling --board-mask 0 --height 6 --pieces 15 \
                 --queue {} --hold O --backend cpu --workers {workers} --use-all-cpu-threads",
                "O".repeat(source_draws)
            );
            let request = runtime
                .compile_command_text(&command)
                .unwrap_or_else(|error| panic!("typed tiling ingress: {command}: {error:?}"));
            assert!(request.product_capability_contract().is_some());
            let AppCommand::Scenario(scenario) = request.command() else {
                panic!("explicit geometry must remain a scenario: {command}");
            };
            let query = scenario.query();
            assert_eq!(query.exact_pieces(), Some(15));
            assert_eq!(query.hold_state().piece(), Some(PieceKind::O));
            assert!(query.allow_hold());
            assert_eq!(query.execution_policy().workers(), workers);
            assert_eq!(
                query
                    .remaining_queue()
                    .as_fixed_sequence()
                    .expect("fixed complete source")
                    .len(),
                source_draws
            );

            // This is the shared product preparation boundary, before any
            // geometry enumeration. The initial held piece and extra source
            // draws must not replace the explicit fifteen-lock geometry.
            let prepared = match app.prepare_distributed_search(request) {
                DistributedSearchPreparation::Search(prepared) => prepared,
                DistributedSearchPreparation::Ready(response) => {
                    panic!("typed tiling preparation rejected {command}: {response:?}")
                }
            };
            let problem = prepared.problem();
            assert_eq!(problem.exact_pieces(), Some(15));
            assert_eq!(problem.visible_height(), 6);
            assert_eq!(problem.objective().kind(), ObjectiveKind::Tiling);
            assert_eq!(problem.initial_hold().hold_piece(), Some(PieceKind::O));
            let hardware_limit = if cfg!(target_family = "wasm") {
                8
            } else {
                8.min(WorkerPolicy::hardware_worker_limit())
            };
            assert_eq!(
                problem.backend_policy().workers(),
                WorkerPolicy::Fixed(workers).effective_for_hardware_limit(true, hardware_limit)
            );
            assert_eq!(
                problem
                    .piece_source()
                    .materialized_universe()
                    .expect("compiled fixed source")
                    .pattern_count(),
                1
            );
            assert!(prepared
                .pc_tiling_terminal_resource_authority()
                .expect("checked typed tiling producer envelope")
                .is_some());
        }
    }
}

fn assert_terminal_supply_solution_set(keys: &[String]) {
    assert_eq!(keys.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(keys
        .iter()
        .all(|key| { key.starts_with("ctk1|initial=00000001c0701c07|placements=") }));

    let normalized = NormalizedTilingSolutionSet::new(keys.iter().map(|key| {
        NormalizedTilingSolutionKey::parse_canonical(key)
            .unwrap_or_else(|error| panic!("non-canonical public solution key {key:?}: {error:?}"))
    }));
    assert_eq!(normalized.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
    assert_eq!(
        normalized.hash(),
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
}

#[test]
fn terminal_supply_p0_cli_app_and_wasm_json_preserve_the_exact_solution_set() {
    std::thread::Builder::new()
        .name("terminal-supply-wasm-runtime-2mib".to_owned())
        .stack_size(2 * 1024 * 1024)
        .spawn(assert_terminal_supply_contract)
        .expect("spawn the explicit two MiB terminal-supply contract")
        .join()
        .expect("terminal-supply public runtime exceeded its two MiB stack contract");
}

fn assert_terminal_supply_contract() {
    // Keep the two independent public owners out of the same native debug
    // stack frame. Their product/runtime stack budgets remain unchanged.
    eprintln!("terminal_supply_phase=direct-app-and-json");
    assert_terminal_supply_app_and_json();
    eprintln!("terminal_supply_phase=worker-job-and-build-identity");
    assert_terminal_supply_worker_identity();
}

#[inline(never)]
fn assert_terminal_supply_app_and_json() {
    eprintln!("terminal_supply_phase=compile-command");
    let request = WasmCommandRuntime::default()
        .compile_command_text(TERMINAL_SUPPLY_P0_COMMAND)
        .expect("terminal-supply CLI command must compile");

    let scenario = match request.command() {
        AppCommand::Scenario(command) => command.query(),
        command => panic!("expected scenario AppRequest, got {command:?}"),
    };
    assert_eq!(scenario.initial_board().width(), 10);
    assert_eq!(scenario.initial_board().visible_height(), 4);
    assert_eq!(
        scenario.initial_board().occupied_mask(),
        TERMINAL_SUPPLY_P0_INITIAL_MASK
    );
    assert_eq!(scenario.piece_window().max_pieces(), 7);
    assert_eq!(scenario.exact_pieces(), Some(7));
    assert!(scenario.allow_hold());
    assert_eq!(scenario.count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(scenario.objective().kind(), ObjectiveKind::All);
    assert_eq!(
        scenario.execution_policy().requested_backend(),
        RequestedSearchBackend::Cpu
    );
    assert_eq!(scenario.execution_policy().workers(), 1);
    assert_eq!(
        scenario
            .remaining_queue()
            .as_fixed_sequence()
            .expect("fixed STOILJZ queue")
            .pieces(),
        &[
            PieceKind::S,
            PieceKind::T,
            PieceKind::O,
            PieceKind::I,
            PieceKind::L,
            PieceKind::J,
            PieceKind::Z,
        ]
    );

    eprintln!("terminal_supply_phase=direct-app-run");
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    eprintln!("terminal_supply_phase=app-result-and-json");

    let result = response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("public App core result");
    let availability = result.execution_report().solution_set_availability();
    assert!(availability.contract_valid());
    assert!(availability.solution_count_calculated());
    assert!(availability.solution_set_materialized());
    assert!(availability.solution_keys_complete());
    assert_eq!(
        availability.solution_keys_materialized_count(),
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(
        result.usize_field("unique_solution_count"),
        Some(TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT)
    );
    assert_eq!(
        result.usize_field("normalized_unique_solution_count"),
        Some(TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT)
    );
    assert_eq!(
        result.usize_field("actual_normalized_unique_solution_count"),
        Some(TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT)
    );
    assert_eq!(result.bool_field("projects_unplaced_lookahead"), Some(true));
    assert_eq!(
        result.bool_field("projects_standard_bag_lookahead"),
        Some(false)
    );
    assert_eq!(result.usize_field("source_sequence_length"), Some(7));
    assert_eq!(
        result.field("supply_window_resolution"),
        Some("projected-terminal-lookahead")
    );
    assert_eq!(result.field("total_possible_pattern_count"), Some("1"));
    assert_eq!(
        result.field("normalized_solution_set_hash"),
        Some(TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH)
    );
    assert_eq!(
        result.field("actual_normalized_solution_set_hash"),
        Some(TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH)
    );
    assert_terminal_supply_solution_set(result.normalized_solution_keys());

    let report_json = serialize_search_report_from_app_response(&response)
        .expect("public WASM search report JSON");
    let report: Value = serde_json::from_str(&report_json).expect("valid WASM report JSON");
    assert_eq!(
        report["unique_solution_count"],
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(report["solution_count_calculated"], true);
    assert_eq!(report["solution_set_materialized"], true);
    assert_eq!(
        report["solution_keys_materialized_count"],
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(report["solution_keys_complete"], true);
    assert_eq!(
        report["normalized_solution_set_hash"],
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
    assert_eq!(report["projects_unplaced_lookahead"], true);
    assert_eq!(report["projects_standard_bag_lookahead"], false);
    assert_eq!(report["source_sequence_length"], 7);
    assert_eq!(
        report["supply_window_resolution"],
        "projected-terminal-lookahead"
    );
    assert_eq!(report["total_possible_pattern_count"], "1");
    let report_keys = report["normalized_solution_keys"]
        .as_array()
        .expect("WASM normalized solution keys")
        .iter()
        .map(|key| key.as_str().expect("WASM solution key string").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(report_keys, result.normalized_solution_keys());
    assert_terminal_supply_solution_set(&report_keys);
}

#[inline(never)]
fn assert_terminal_supply_worker_identity() {
    eprintln!("terminal_supply_phase=worker-start");
    let mut worker = WasmWorkerJobRuntime::default();
    let job_id = worker
        .start_job(TERMINAL_SUPPLY_P0_COMMAND)
        .expect("terminal-supply browser worker job");
    eprintln!("terminal_supply_phase=worker-advance");
    let mut events = Vec::new();
    let mut last_progress = None;
    let mut completed = false;
    for advance in 0..10_000 {
        let status = worker
            .advance_job(job_id, 16_384)
            .expect("advance terminal-supply browser worker job");
        // The actual host drains progress after each cooperative turn. Keep
        // those real JSON envelopes, including the one final response, while
        // printing only stage changes for a native stack-abort diagnostic.
        let events_json = worker
            .drain_events_json(job_id)
            .expect("terminal-supply browser worker event JSON");
        let batch: Vec<Value> =
            serde_json::from_str(&events_json).expect("valid browser event JSON");
        for event in &batch {
            if let Some(label) = event["progress"]["label"].as_str() {
                if last_progress.as_deref() != Some(label) {
                    eprintln!("terminal_supply_worker_stage={label} advance={advance}");
                    last_progress = Some(label.to_owned());
                }
            }
        }
        events.extend(batch);
        if status.is_terminal() {
            assert_eq!(status, WasmWorkerAdvanceStatus::Completed);
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "tiny terminal-supply fixture exceeded its test-only work bound"
    );
    eprintln!("terminal_supply_phase=worker-terminal-json");
    let final_events = events
        .iter()
        .filter(|event| event["event"] == "final_response")
        .collect::<Vec<_>>();
    assert_eq!(
        final_events.len(),
        1,
        "exactly one terminal response is required"
    );
    let final_response = &final_events[0]["response"];
    let worker_report = &final_events[0]["search_report"];
    let worker_keys = worker_report["normalized_solution_keys"]
        .as_array()
        .expect("complete actual Worker solution keys")
        .iter()
        .map(|key| key.as_str().expect("Worker solution key string").to_owned())
        .collect::<Vec<_>>();
    assert_terminal_supply_solution_set(&worker_keys);
    assert_eq!(
        worker_report["normalized_solution_set_hash"],
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
    assert_eq!(worker_report["solution_keys_complete"], true);
    let identity = final_response["runtime_identity"]
        .as_object()
        .expect("WASM Host runtime_identity object");
    let expected = ProductBuildIdentity::current();

    assert_eq!(identity.len(), 5);
    assert_eq!(identity["engine_build_id"], expected.engine_build_id());
    assert_eq!(identity["source_commit"], expected.source_commit());
    assert_eq!(
        identity["contract_schema_version"],
        expected.contract_schema_version()
    );
    assert_eq!(
        identity["supply_semantics_id"],
        expected.supply_semantics_id()
    );
    assert_eq!(
        identity["artifact_schema_version"],
        expected.artifact_schema_version()
    );
    if let Some(expected) = option_env!("CLEARRA_SOURCE_COMMIT") {
        assert_eq!(identity["source_commit"], expected);
    }
    if let Some(expected) = option_env!("CLEARRA_ENGINE_BUILD_ID") {
        assert_eq!(identity["engine_build_id"], expected);
    }
}
