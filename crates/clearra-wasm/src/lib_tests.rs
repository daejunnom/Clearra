use clearra_app::{AppCommand, AppContext, AppCoreExecutorService, AppServices};
use clearra_host_contract::AppStatus;
use serde_json::Value;

use super::*;

#[test]
fn wasm_command_compiles_to_app_request() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend cpu")
        .expect("AppRequest");

    match request.command() {
        AppCommand::Pc(command) => {
            assert_eq!(command.query().target().lines(), 2);
            assert_eq!(command.query().execution_policy().backend().as_str(), "cpu");
        }
        _ => panic!("expected pc command"),
    }

    let result = WasmCommandRuntime::default()
        .run_command_text("clearra pc --lines 2 --backend auto --queue IIOOO")
        .expect("general WASM CPU search backend");
    assert_eq!(
        result.app_response().status(),
        AppStatus::Success,
        "{:?}",
        result.app_response()
    );
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(result.app_response().resource_report().solver_executed());
    assert!(result
        .app_response()
        .resource_report()
        .probability_complete());
    assert_eq!(
        result.webgpu_backend().outcome_state,
        WebGpuBackendOutcomeState::Unavailable
    );
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(result.webgpu_backend().fallback_backend, None);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("webgpu_not_selected")
    );

    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend auto --queue IIOOO")
        .expect("AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("WASM CPU CoreExecutionResult");
    assert!(core_result.solution_found());
    assert_eq!(core_result.field("count_complete"), Some("true"));
}

#[test]
fn wasm_setup_command_preserves_the_exact_residue_contract() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra setup --remaining IOTS")
        .expect("setup AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected setup command");
    };
    assert_eq!(command.query().residue().remaining_count(), 4);
    assert_eq!(command.query().residue().cycle(), Some(2));
    assert_eq!(command.query().residue().duplicate_piece(), None);
    assert_eq!(
        command.query().hold_policy(),
        clearra_problem::SetupHoldPolicy::EnabledEmpty
    );

    let cycle_boundary = WasmCommandRuntime::default()
        .compile_command_text("clearra setup --remaining IOT --allow-post-cycle-borrow")
        .expect("cycle-seven setup AppRequest");
    let AppCommand::Setup(command) = cycle_boundary.command() else {
        panic!("expected setup command");
    };
    assert_eq!(command.query().residue().cycle(), Some(7));
    assert_eq!(
        command.query().cycle_reset_borrow_policy(),
        clearra_problem::SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn wasm_setup_command_preserves_observed_qb_and_next_cycle_inventory() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(
            "clearra setup --remaining TI --mode qb --qb OS \
             --next-cycle-remaining OOSITZ",
        )
        .expect("QB setup AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        command.query().search_mode(),
        clearra_problem::SetupSearchMode::QueueBased
    );
    assert_eq!(
        command.query().residue().pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::T,
            clearra_core_domain::piece::piece_kind::PieceKind::I,
        ]
    );
    assert_eq!(
        command
            .query()
            .queue()
            .as_fixed_sequence()
            .expect("fixed QB queue")
            .pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::O,
            clearra_core_domain::piece::piece_kind::PieceKind::S,
        ]
    );
    assert_eq!(
        command.query().next_cycle_remaining_pieces(),
        Some(
            &[
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::S,
                clearra_core_domain::piece::piece_kind::PieceKind::I,
                clearra_core_domain::piece::piece_kind::PieceKind::T,
                clearra_core_domain::piece::piece_kind::PieceKind::Z,
            ][..]
        )
    );
}

#[test]
fn occupied_initial_hold_plus_p7_solves_eight_piece_scenario() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --backend cpu --workers 1",
        )
        .expect("WASM scenario result");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert!(
        report.solution_found,
        "initial hold S and the seven P7 pieces form the eight placed pieces"
    );
    assert!(report.count_complete);
    assert!(report.projects_unplaced_lookahead);
    assert_eq!(report.source_sequence_length, 7);
}

#[test]
fn finite_pattern_releases_terminal_hold_for_complete_build_coverage() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xe0380e0380 --height 4 --patterns [LOJ]! --hold empty --no-mirror --workers 1",
        )
        .expect("finite-pattern build probability result");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert!(report.count_complete);
    assert!(report.probability_complete);
    assert!(report.projects_unplaced_lookahead);
    assert_eq!(report.source_sequence_length, 3);
    assert_eq!(report.covered_pattern_count, 6);
    assert_eq!(report.unique_solution_count, 2);
    assert_eq!(report.normalized_solution_set_hash, "cts1:2770e9c1ff9a940e");
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 1.0)
            .abs()
            <= f64::EPSILON
    );
}

#[test]
fn inverse_b2b_constraint_removes_a_normal_non_pc_line_clear() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(
            "clearra build-probability --base-mask 0x803f0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --preserve-b2b --spin-profile t-spins",
        )
        .expect("constrained request");
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core = response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("core result");

    assert_eq!(
        core.field("execution_constraint_materialized"),
        Some("true")
    );
    assert_eq!(core.field("unique_solution_count"), Some("0"));
    assert_eq!(core.field("covered_pattern_count"), Some("0"));
    assert_eq!(core.field("solution_found"), Some("false"));
}

#[test]
fn all_piece_spin_profiles_do_not_promote_an_upward_mobile_o_clear() {
    let runtime = WasmCommandRuntime::default();
    let base_command = "clearra pc --lines 4 --board-mask 0xf3fcff3fcf --height 4 --pieces 2 --queue OO --no-hold --backend cpu --workers 1";
    let unconstrained = runtime
        .run_command_text(base_command)
        .expect("unconstrained O-piece perfect clear");
    assert!(
        unconstrained
            .search_report()
            .expect("unconstrained search report")
            .solution_found
    );

    for profile in ["all-spin", "all-spin-plus", "all-mini", "all-mini-plus"] {
        let constrained = runtime
            .run_command_text(&format!(
                "{base_command} --preserve-b2b --spin-profile {profile}"
            ))
            .unwrap_or_else(|error| panic!("{profile} constrained search failed: {error:?}"));
        let report = constrained
            .search_report()
            .unwrap_or_else(|| panic!("{profile} search report"));
        assert!(
            !report.solution_found,
            "{profile} must reject the first ordinary O double before the final perfect clear"
        );
        assert_eq!(report.unique_solution_count, 0, "{profile}");
        assert_eq!(report.covered_pattern_count, 0, "{profile}");
    }
}

#[test]
fn all_mini_plus_b2b_build_probability_matches_the_93_percent_reference() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xe81a06fffbf --height 8 --patterns P7 --hold empty --aggregate build --rule srs-plus --spin-profile all-mini-plus --preserve-b2b --include-mirror --workers 1",
        )
        .expect("All-Mini+ B2B build probability");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert_eq!(report.materialized_pattern_count, 5_040);
    assert_eq!(report.covered_pattern_count, 4_704);
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 4_704.0 / 5_040.0)
            .abs()
            <= 1.0e-12
    );
}

#[test]
fn all_mini_plus_b2b_pc_preserves_asymmetric_srs_plus_hold_paths() {
    let runtime = WasmCommandRuntime::default();
    let result = runtime
        .run_command_text(
            "clearra pc --lines 5 --board-mask 0xf01e0783f0f --height 5 --pieces 7 --patterns P7 --hold empty --backend cpu --workers 1 --preserve-b2b --spin-profile all-mini-plus",
        )
        .expect("All-Mini+ B2B 5L PC probability");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert_eq!(report.materialized_pattern_count, 5_040);
    // ISOTZLJ and its hold-equivalent patterns use an asymmetric first-success
    // I-kick predecessor; reverse kick lookup must not discard those 18 queues.
    assert_eq!(report.covered_pattern_count, 4_032);
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 4_032.0 / 5_040.0)
            .abs()
            <= 1.0e-12
    );

    let fixed_queue = runtime
        .run_command_text(
            "clearra pc --lines 5 --board-mask 0xf01e0783f0f --height 5 --pieces 7 --patterns ISOTZLJ --hold empty --backend cpu --workers 1 --preserve-b2b --spin-profile all-mini-plus",
        )
        .expect("asymmetric SRS+ hold path");
    let fixed_queue_report = fixed_queue
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", fixed_queue.app_response()));

    assert_eq!(fixed_queue_report.materialized_pattern_count, 1);
    assert_eq!(fixed_queue_report.covered_pattern_count, 1);
}

#[test]
fn wasm_runtime_does_not_use_native_path_semantics() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --fixture C:\\field.json")
        .expect_err("native paths are rejected");

    assert_eq!(error.code(), "E_WASM_NATIVE_PATH_FORBIDDEN");
}

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 | verify")
        .expect_err("process syntax is rejected");

    assert_eq!(error.code(), "E_WASM_PROCESS_SEMANTICS_FORBIDDEN");
}

#[test]
fn cancel_long_6l_stops_before_natural_completion_and_releases_scope() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime.start_job("clearra verify kicks").expect("job");
    while !runtime
        .advance_job(job_id, 64)
        .expect("advance job")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::Progress { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));

    let cancel_id = runtime
        .start_job("clearra pc --lines 6 --backend cpu --queue IOTSZJLIOTSZJLI")
        .expect("active job");
    assert_eq!(
        runtime.advance_job(cancel_id, 64).expect("prepare job"),
        WasmWorkerAdvanceStatus::Pending
    );
    assert_eq!(
        runtime
            .advance_job(cancel_id, 1)
            .expect("advance one exact search slice"),
        WasmWorkerAdvanceStatus::Pending,
        "6L search must yield before natural completion"
    );
    runtime.cancel_job(cancel_id).expect("cancel");
    let cancelled_events = runtime.drain_events(cancel_id);
    assert!(cancelled_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Cancelled {
            scope_released: true,
            ..
        }
    )));
    assert!(!cancelled_events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));
}

#[test]
fn wasm_output_keys_are_not_localized() {
    let result = WasmCommandRuntime::default()
        .run_command_text("clearra verify kicks")
        .expect("runtime output");
    let value: Value = serde_json::to_value(result.app_response()).expect("AppResponse json");

    assert_eq!(value["command"], "verify-kicks");
    assert_eq!(value["status"], "success");
    assert!(value["diagnostics"].is_array());
    assert!(value["backend_report"].is_object());
    assert!(value["resource_report"].is_object());
}

#[test]
fn wasm_user_shader_rejected() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --wgsl user-shader.wgsl")
        .expect_err("user WGSL must not enter the typed command runtime");

    assert_eq!(error.code(), "E_WASM_COMMAND_UNSUPPORTED");
    let report = WebGpuBackendReport::not_requested();
    assert!(!report.shader.user_shader_allowed);
    assert!(!report.shader.runtime_shader_injection_allowed);
    assert!(report.shader.shader_hash.is_none());
}

#[test]
fn distributed_b2b_constraint_matches_serial_exact_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra pc --lines 4 --count unique --backend cpu --workers 1 --queue IOTSZJLIOTS --preserve-b2b --spin-profile t-spins";
    let distributed_command = "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue IOTSZJLIOTS --preserve-b2b --spin-profile t-spins";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial exact result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);

    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_report.covered_pattern_count,
        serial_report.covered_pattern_count
    );
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(distributed_report.workers_used, 2);
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "execution_constraint_materialized" && value == "true" }));
}

#[test]
fn distributed_build_probability_b2b_constraint_matches_serial_exact_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --workers 1 --preserve-b2b --spin-profile t-spins";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --workers 2 --preserve-b2b --spin-profile t-spins";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial build probability result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(serial_report.unique_solution_count, 8);
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_report.covered_pattern_count,
        serial_report.covered_pattern_count
    );
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(distributed_report.workers_used, 2);
}

fn run_distributed_cpu(runtime: &WasmCommandRuntime, command: &str) -> WasmExecutionResult {
    let preparation =
        WasmDistributedCoordinator::prepare(runtime, command).expect("distributed preparation");
    let mut coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("two-worker request must use the distributed product path"),
    };
    let mut verifier =
        WasmDistributedVerifierRuntime::prepare(runtime, command).expect("distributed verifier");
    loop {
        match coordinator
            .advance_producer(16_384, 16)
            .expect("geometry producer")
        {
            WasmDistributedProducerAdvance::Pending => {}
            WasmDistributedProducerAdvance::Initialization(_) => {}
            WasmDistributedProducerAdvance::Batch(batch) => {
                verifier.consume(&batch).expect("candidate batch");
            }
            WasmDistributedProducerAdvance::Completed => break,
            WasmDistributedProducerAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let partial = verifier.finish().expect("partial exact result");
    coordinator
        .absorb_partial(&partial)
        .expect("merge partial exact result");
    coordinator.finish(2).expect("distributed exact result")
}

#[cfg(feature = "webgpu-search")]
#[test]
fn gpu_multi_request_selects_the_webgpu_distributed_product_path() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend gpu --workers 2 --queue IOTSZJLIOTS",
    )
    .expect("WebGPU distributed preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::WebGpuMulti);
            assert_eq!(coordinator.worker_count(), 2);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("4L two-worker GPU request must select gpu-multi"),
    }
}
