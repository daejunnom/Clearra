// SRP rationale: this integration test has one behavior-level change reason:
// preserving the finite terminal-hold solution set across the public Web/App/WASM boundary.

use clearra_app::{AppCommand, AppContext, AppCoreExecutorService, AppServices, AppStatus};
use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSet,
    },
};
use clearra_host_contract::ProductBuildIdentity;
use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend};
use clearra_wasm::{
    serialize_search_report_from_app_response, WasmCommandRuntime, WasmWorkerJobRuntime,
};
use serde_json::Value;

const TERMINAL_SUPPLY_P0_COMMAND: &str = "clearra pc \
    --board-mask 0x1c0701c07 --height 4 --pieces 7 \
    --queue STOILJZ --hold empty --count unique --objective all \
    --backend cpu --workers 1";
const TERMINAL_SUPPLY_P0_INITIAL_MASK: u64 = 0x1c0701c07;
const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT: usize = 18;
const TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH: &str = "cts1:8a7fc484d9b49994";

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
fn terminal_supply_p0_web_app_and_wasm_json_preserve_the_exact_solution_set() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(TERMINAL_SUPPLY_P0_COMMAND)
        .expect("terminal-supply Web command must compile");

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

    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

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

    let mut worker = WasmWorkerJobRuntime::default();
    let job_id = worker
        .start_job(TERMINAL_SUPPLY_P0_COMMAND)
        .expect("terminal-supply browser worker job");
    while !worker
        .advance_job(job_id, 16_384)
        .expect("advance terminal-supply browser worker job")
        .is_terminal()
    {}
    let events_json = worker
        .drain_events_json(job_id)
        .expect("terminal-supply browser worker event JSON");
    let events: Value = serde_json::from_str(&events_json).expect("valid browser event JSON");
    let final_response = events
        .as_array()
        .and_then(|events| {
            events
                .iter()
                .find(|event| event["event"] == "final_response")
        })
        .map(|event| &event["response"])
        .expect("terminal-supply final Host response");
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
