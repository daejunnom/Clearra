// SRP rationale: this test module has one behavior-level change reason: verifying the complete public WASM command and JSON envelope contract.

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
fn unavailable_gpu_and_hybrid_keep_distinct_cpu_selection_semantics() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));

    let gpu = runtime
        .run_command_text(
            "clearra pc --lines 2 --backend gpu --allow-backend-fallback \
             --workers 1 --queue IIOOO",
        )
        .expect("explicit GPU fallback result");
    assert_eq!(gpu.app_response().status(), AppStatus::Success);
    assert!(gpu.app_response().backend_report().fallback_used());
    assert_eq!(
        gpu.app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_device_not_found")
    );
    assert!(gpu.webgpu_backend().fallback_used);
    assert_eq!(
        gpu.webgpu_backend().fallback_backend.as_deref(),
        Some("wasm-cpu")
    );
    assert_eq!(
        gpu.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );

    let hybrid = runtime
        .run_command_text(
            "clearra pc --lines 2 --backend hybrid --no-backend-fallback \
             --workers 1 --queue IIOOO",
        )
        .expect("hybrid CPU selection result");
    assert_eq!(hybrid.app_response().status(), AppStatus::Success);
    assert_eq!(
        hybrid.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(!hybrid.app_response().backend_report().fallback_used());
    assert!(!hybrid.webgpu_backend().fallback_used);
    assert_eq!(hybrid.webgpu_backend().fallback_backend, None);
    assert_eq!(
        hybrid.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn tiling_only_returns_exact_geometry_without_buildup_or_probability() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc --lines 2 --queue IIOOO --tiling-only \
             --backend cpu --workers 1 --no-hold",
        )
        .expect("tiling-only search");
    let report = result.search_report().expect("tiling-only report");

    assert!(report.unique_solution_count > 0);
    assert!(!report.buildability_verified);
    assert!(!report.coverage_calculated);
    assert!(!report.probability_calculated);
    assert_eq!(report.coverage_probability, "not-calculated");
    assert_eq!(report.total_build_order_nodes, 0);
    assert_eq!(report.coverage_product_edge_checks, 0);
    assert!(report.count_complete);
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
fn distributed_setup_finalize_preserves_the_cancellation_reason() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra setup-finder --remaining IOT --workers 2",
    )
    .expect("distributed setup preparation");
    let coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("setup search must use the distributed coordinator"),
    };
    coordinator.cancel();

    let error = match coordinator.finish(2) {
        Ok(_) => panic!("cancelled setup finalize must not complete"),
        Err(error) => error,
    };

    assert_eq!(error.code(), "E_WASM_DISTRIBUTED_SETUP_FINISH");
    assert_eq!(error.message(), "wasm_cpu_search_cancelled");
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
fn finesse_fixed_queue_witness_reaches_the_typed_wasm_json_contract() {
    const COMMAND: &str = "clearra finesse search --base-mask 0x0 --target-mask 0xf --height 1 \
         --queue I --no-hold --pattern-knowledge oracle --rule srs-plus";
    let result = WasmCommandRuntime::default()
        .run_command_text(COMMAND)
        .expect("fixed-queue finesse search");
    let report = result.search_report().expect("finesse search report");
    let finesse = report
        .finesse_report
        .as_ref()
        .expect("typed finesse report");
    let witness = finesse
        .representative_witness
        .as_ref()
        .expect("fixed queue witness");
    assert_eq!(witness.policy, "oracle");
    assert_eq!(witness.queue, ["I"]);
    assert_eq!(
        finesse
            .exact_total_inputs
            .as_deref()
            .and_then(|value| value.parse::<u32>().ok()),
        Some(witness.total_inputs)
    );
    assert_eq!(witness.input_sequence.len(), witness.total_inputs as usize);
    assert_eq!(
        witness.input_sequence.last().map(String::as_str),
        Some("hard-drop")
    );
    assert_eq!(witness.placements.len(), 1);
    assert_eq!(witness.placements[0].piece, "I");
    assert_eq!(witness.placements[0].rotation, 0);
    assert_eq!((witness.placements[0].x, witness.placements[0].y), (0, 0));

    let request = WasmCommandRuntime::default()
        .compile_command_text(COMMAND)
        .expect("finesse AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let json = serialize_search_report_from_app_response(&app_response)
        .expect("serialized finesse search report");
    let value: Value = serde_json::from_str(&json).expect("valid search report JSON");
    let json_witness = &value["finesse_report"]["representative_witness"];
    assert_eq!(json_witness["queue"], serde_json::json!(["I"]));
    assert_eq!(json_witness["total_inputs"], witness.total_inputs);
    assert_eq!(
        json_witness["input_sequence"].as_array().map(Vec::len),
        Some(witness.total_inputs as usize)
    );
    assert_eq!(
        json_witness["placements"],
        serde_json::json!([{"piece":"I","rotation":0,"x":0,"y":0}])
    );
}

#[test]
fn finesse_fixed_queue_score_reaches_the_typed_wasm_report_contract() {
    const COMMAND: &str = "clearra finesse score --initial-mask 0 --height 4 \
         --placements O:spawn:4:0 --queue O --no-hold --pattern-knowledge both \
         --rule srs-plus --workers 2";
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let result = runtime
        .run_command_text(COMMAND)
        .expect("fixed-queue finesse score");
    let report = result.search_report().expect("finesse score report");
    let finesse = report
        .finesse_report
        .as_ref()
        .expect("typed finesse score report");
    let witness = finesse
        .representative_witness
        .as_ref()
        .expect("fixed score witness");

    assert_eq!(report.workers_used, 1, "score remains globally serial");
    assert!(!report.cpu_parallel_execution);
    assert_eq!(finesse.mode, "score");
    assert_eq!(finesse.exact_total_inputs.as_deref(), Some("1"));
    assert_eq!(witness.total_inputs, 1);
    assert_eq!(witness.input_sequence, ["hard-drop"]);
    assert_eq!(witness.placements.len(), 1);

    let request = runtime
        .compile_command_text(COMMAND)
        .expect("finesse score AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("score core result");
    assert_eq!(core_result.field("backend_selected"), None);
    assert_eq!(core_result.field("workers_used"), None);
    let json = serialize_search_report_from_app_response(&app_response)
        .expect("serialized finesse score report");
    let value: Value = serde_json::from_str(&json).expect("valid score report JSON");
    assert_eq!(value["workers_used"], 1);
    assert_eq!(value["finesse_report"]["mode"], "score");
    assert_eq!(value["finesse_report"]["exact_total_inputs"], "1");
    assert_eq!(
        value["finesse_report"]["representative_witness"]["input_sequence"],
        serde_json::json!(["hard-drop"])
    );
}

#[test]
fn browser_worker_final_event_keeps_the_fixed_score_typed_report() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job(
            "clearra finesse score --initial-mask 0 --height 4 \
             --placements O:spawn:4:0 --queue O --no-hold \
             --pattern-knowledge both --rule srs-plus --workers 2",
        )
        .expect("browser score job");
    while !runtime
        .advance_job(job_id, 4096)
        .expect("advance browser score job")
        .is_terminal()
    {}
    let json = runtime
        .drain_events_json(job_id)
        .expect("browser event JSON");
    let events: Value = serde_json::from_str(&json).expect("valid browser events");
    let final_event = events
        .as_array()
        .and_then(|events| {
            events
                .iter()
                .find(|event| event["event"] == "final_response")
        })
        .expect("final browser response");

    assert_eq!(final_event["response"]["status"], "success");
    assert_eq!(
        final_event["response"]["result"],
        serde_json::json!({"kind": "build-probability"})
    );
    assert_eq!(final_event["search_report"]["workers_used"], 1);
    assert_eq!(
        final_event["search_report"]["finesse_report"]["mode"],
        "score"
    );
    assert_eq!(
        final_event["search_report"]["finesse_report"]["exact_total_inputs"],
        "1"
    );
    assert_eq!(
        final_event["search_report"]["finesse_report"]["representative_witness"]["total_inputs"],
        1
    );
}

#[test]
fn build_probability_tiling_only_returns_geometry_without_buildup_or_coverage() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --workers 1",
        )
        .expect("build-probability tiling-only result");
    let report = result.search_report().expect("tiling-only report");

    assert_eq!(report.unique_solution_count, 1);
    assert!(!report.buildability_verified);
    assert!(!report.coverage_calculated);
    assert!(!report.probability_calculated);
    assert_eq!(report.coverage_probability, "not-calculated");
    assert_eq!(report.total_build_order_nodes, 0);
    assert_eq!(report.coverage_product_edge_checks, 0);
    assert!(report.count_complete);
}

#[test]
fn build_probability_explicit_gpu_fallback_is_reported_as_an_unsupported_kernel() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend gpu \
             --allow-backend-fallback --workers 1",
        )
        .expect("build-probability CPU fallback result");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu-build-probability"
    );
    assert!(result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_kernel_unavailable")
    );
    assert_eq!(
        result.app_response().backend_report().fallback_backend(),
        Some("wasm-cpu-build-probability")
    );
    assert!(result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_kernel_unavailable")
    );
}

#[test]
fn build_probability_explicit_gpu_without_fallback_is_unsupported() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend gpu \
             --no-backend-fallback --workers 1",
        )
        .expect("unsupported build-probability response");

    assert_eq!(result.app_response().status(), AppStatus::Unsupported);
    let diagnostic = result
        .app_response()
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == "E_PRODUCT_RUNTIME_UNSUPPORTED")
        .expect("unsupported build-probability diagnostic");
    assert!(diagnostic.message().contains("webgpu_backend_unavailable"));
    assert!(!result.app_response().backend_report().fallback_used());
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "none"
    );
}

#[test]
fn build_probability_hybrid_unavailable_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend hybrid \
             --no-backend-fallback --workers 1",
        )
        .expect("hybrid build-probability CPU result");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu-build-probability"
    );
    assert!(!result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        None
    );
    assert_eq!(
        result.app_response().backend_report().fallback_backend(),
        None
    );
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_kernel_unavailable")
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

#[test]
fn distributed_build_probability_finesse_matches_serial_report_and_witness() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // Seven O pieces keep the distributed eligibility threshold while making the
    // exact geometry catalog deliberately small and deterministic.
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold --no-mirror --workers 1 --finesse inputs --pattern-knowledge both";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold --no-mirror --workers 2 --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial finesse build probability result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let serial_result = serial.search_report().expect("serial search report");
    let distributed_result = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(
        distributed_result.normalized_solution_keys,
        serial_result.normalized_solution_keys
    );
    assert_eq!(
        distributed_result.normalized_solution_set_hash,
        serial_result.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_result.finesse_report,
        serial_result.finesse_report
    );
    assert!(serial_result
        .finesse_report
        .as_ref()
        .and_then(|report| report.representative_witness.as_ref())
        .is_some());
    assert_eq!(distributed_result.workers_used, 2);
}

#[test]
fn ctk3_spawn_blocked_finesse_matches_serial_instead_of_failing_distribution() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // ctk3_w0kEaIIDmggnun6Vo_iPi8HogDAUR74DBhQocwCBgAEDCBQocODAAQQCDBAghACBAAMGiIkwGuQ
    // Page one is the occupied base. Page two contributes the colorless target delta.
    const COMMAND: &str = "clearra build-probability \
        --base-mask 0x3effbfeffbfeffbfeffbfeffbfeffbfeffbfeffbfeffbfef \
        --target-mask 0xa07e1fffe3c00000000000000000000000000000000000000000000000 \
        --height 24 --patterns P7 --hold empty --no-mirror \
        --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial CTK3 finesse build probability");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_build_probability_semantics_match(serial_report, distributed_report);
    assert!(!serial_report.solution_found);
    assert_eq!(serial_report.covered_pattern_count, 0);
    assert!(serial_report
        .finesse_report
        .as_ref()
        .is_some_and(|report| report.representative_witness.is_none()));
}

#[test]
fn initial_hold_cannot_bypass_a_blocked_current_piece_in_serial_or_distributed_finesse() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra build-probability \
        --base-mask 0x400000000000000000000000000000000000000000000000000000 \
        --target-mask 0xf --height 24 --queue OI --hold empty --no-mirror \
        --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial blocked-current-piece finesse build probability");
    let distributed = runtime
        .run_command_text(&format!("{COMMAND} --workers 2"))
        .expect("two-worker blocked-current-piece finesse build probability");
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_build_probability_semantics_match(serial_report, distributed_report);
    assert!(!serial_report.solution_found);
    assert_eq!(serial_report.covered_pattern_count, 0);
    assert!(serial_report
        .finesse_report
        .as_ref()
        .is_some_and(|report| report.representative_witness.is_none()));
}

#[cfg(feature = "stage-profiling")]
#[test]
fn distributed_finesse_finalizer_records_every_coordinator_profile_stage() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold \
        --no-mirror --workers 2 --finesse inputs --pattern-knowledge both";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("distributed preparation");
    let mut coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("finesse search must use the distributed coordinator"),
    };
    let mut verifier =
        WasmDistributedVerifierRuntime::prepare(&runtime, command).expect("distributed verifier");
    loop {
        match coordinator
            .advance_producer(16_384, 16)
            .expect("geometry producer")
        {
            WasmDistributedProducerAdvance::Pending
            | WasmDistributedProducerAdvance::Initialization(_) => {}
            WasmDistributedProducerAdvance::Batch(batch) => {
                let mut consumed = verifier.consume(&batch).expect("candidate batch");
                if let Some(partial) = consumed.partial.take() {
                    coordinator
                        .absorb_partial(&partial)
                        .expect("merge streamed partial result");
                }
                while consumed.has_pending_work {
                    consumed = verifier.continue_work().expect("continue worker task");
                    if let Some(partial) = consumed.partial.take() {
                        coordinator
                            .absorb_partial(&partial)
                            .expect("merge streamed partial result");
                    }
                }
            }
            WasmDistributedProducerAdvance::Completed => break,
            WasmDistributedProducerAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let partial = verifier.finish().expect("partial exact result");
    if !partial.is_empty() {
        coordinator
            .absorb_partial(&partial)
            .expect("merge partial exact result");
    }

    // Start after the worker has finished: every recorded finesse span below
    // must therefore belong to coordinator-side reconstruction and aggregation.
    let profile = ExecutorSearchProfileSession::start().expect("profile session");
    let result = coordinator.finish(2).expect("distributed exact result");
    let stages = profile.finish();
    assert_eq!(result.app_response().status(), AppStatus::Success);
    for required in [
        "finesse.geometry",
        "finesse.target_grouping",
        "finesse.movement_bfs",
        "finesse.annotation_prune",
        "finesse.product_dp",
        "finesse.aggregation",
    ] {
        let stage = stages
            .iter()
            .find(|stage| stage.name == required)
            .unwrap_or_else(|| panic!("missing coordinator profile stage {required}"));
        assert!(
            stage.invocation_count > 0,
            "coordinator profile stage {required} was not invoked"
        );
    }
}

#[cfg(feature = "stage-profiling")]
#[test]
fn fixed_queue_finesse_score_records_all_seven_profile_stages_serially() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let profile = ExecutorSearchProfileSession::start().expect("profile session");
    let result = runtime
        .run_command_text(
            "clearra finesse score --initial-mask 0 --height 4 \
             --placements O:spawn:4:0 --queue O --no-hold --workers 2 \
             --pattern-knowledge both",
        )
        .expect("serial fixed-queue finesse score");
    let stages = profile.finish();
    assert_eq!(result.app_response().status(), AppStatus::Success);
    for required in [
        "finesse.geometry",
        "finesse.target_grouping",
        "finesse.movement_bfs",
        "finesse.annotation_prune",
        "finesse.product_dp",
        "finesse.aggregation",
        "finesse.witness",
    ] {
        let stage = stages
            .iter()
            .find(|stage| stage.name == required)
            .unwrap_or_else(|| panic!("missing score profile stage {required}"));
        assert!(
            stage.invocation_count > 0,
            "score profile stage {required} was not invoked"
        );
    }
}

#[test]
fn distributed_build_probability_tiling_matches_serial_without_buildup() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --tiling-only --workers 1";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --tiling-only --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial build-probability tiling result");
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
    assert_eq!(distributed_report.total_build_order_nodes, 0);
    assert_eq!(distributed_report.coverage_product_edge_checks, 0);
    assert!(!distributed_report.buildability_verified);
    assert_eq!(distributed_report.workers_used, 2);
}

#[test]
fn distributed_build_probability_tiling_unions_distinct_mirror_passes() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xcc33fffff --height 4 --queue OOOOOOO --no-hold --include-mirror --tiling-only --workers 1";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xcc33fffff --height 4 --queue OOOOOOO --no-hold --include-mirror --tiling-only --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial mirrored build-probability tiling result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert!(serial_report.unique_solution_count > 0);
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "build_mirror_distinct_target" && value == "true" }));
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "build_mirror_search_executed" && value == "true" }));
    assert_eq!(distributed_report.total_build_order_nodes, 0);
    assert!(!distributed_report.buildability_verified);
}

#[test]
fn distributed_tiling_root_tasks_match_serial_hold_supply_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --tiling-only --backend cpu --workers 1";
    let distributed_command = "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --tiling-only --backend cpu --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial tiling result");
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
    assert!(!distributed_report.buildability_verified);
    assert_eq!(distributed_report.workers_used, 2);
}

fn assert_build_probability_semantics_match(
    serial: &WasmSearchReport,
    distributed: &WasmSearchReport,
) {
    assert_eq!(
        distributed.supply_window_resolution,
        serial.supply_window_resolution
    );
    assert_eq!(
        distributed.projects_unplaced_lookahead,
        serial.projects_unplaced_lookahead
    );
    assert_eq!(
        distributed.source_sequence_length,
        serial.source_sequence_length
    );
    assert_eq!(
        distributed.total_possible_pattern_count,
        serial.total_possible_pattern_count
    );
    assert_eq!(distributed.solution_found, serial.solution_found);
    assert_eq!(
        distributed.packing_candidate_count,
        serial.packing_candidate_count
    );
    assert_eq!(
        distributed.geometry_candidate_family_count,
        serial.geometry_candidate_family_count
    );
    assert_eq!(
        distributed.packing_candidate_set_digest,
        serial.packing_candidate_set_digest
    );
    assert_eq!(
        distributed.packing_candidate_keys,
        serial.packing_candidate_keys
    );
    assert_eq!(
        distributed.unique_solution_count,
        serial.unique_solution_count
    );
    assert_eq!(
        distributed.normalized_solution_set_hash,
        serial.normalized_solution_set_hash
    );
    assert_eq!(
        distributed.normalized_solution_keys,
        serial.normalized_solution_keys
    );
    assert_eq!(
        distributed.solution_probabilities,
        serial.solution_probabilities
    );
    assert_eq!(
        distributed.solution_average_scores,
        serial.solution_average_scores
    );
    assert_eq!(distributed.finesse_report, serial.finesse_report);
    assert_eq!(distributed.build_variant_count, serial.build_variant_count);
    assert_eq!(
        distributed.build_variant_count_exact,
        serial.build_variant_count_exact
    );
    assert_eq!(
        distributed.buildability_verified,
        serial.buildability_verified
    );
    assert_eq!(distributed.coverage_calculated, serial.coverage_calculated);
    assert_eq!(
        distributed.probability_calculated,
        serial.probability_calculated
    );
    assert_eq!(
        distributed.materialized_pattern_count,
        serial.materialized_pattern_count
    );
    assert_eq!(
        distributed.covered_pattern_count,
        serial.covered_pattern_count
    );
    assert_eq!(
        distributed.coverage_probability,
        serial.coverage_probability
    );
    assert_eq!(
        distributed.probability_complete,
        serial.probability_complete
    );
    assert_eq!(distributed.count_complete, serial.count_complete);
    assert_eq!(distributed.resource_truncated, serial.resource_truncated);
    assert_eq!(
        distributed.resource_truncation_reason,
        serial.resource_truncation_reason
    );
    assert_eq!(
        distributed.representative_pattern_id,
        serial.representative_pattern_id
    );
    assert_eq!(distributed.representative_path, serial.representative_path);
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
                let mut consumed = verifier.consume(&batch).expect("candidate batch");
                if let Some(partial) = consumed.partial.take() {
                    coordinator
                        .absorb_partial(&partial)
                        .expect("merge streamed partial result");
                }
                while consumed.has_pending_work {
                    consumed = verifier.continue_work().expect("continue worker task");
                    if let Some(partial) = consumed.partial.take() {
                        coordinator
                            .absorb_partial(&partial)
                            .expect("merge streamed partial result");
                    }
                }
            }
            WasmDistributedProducerAdvance::Completed => break,
            WasmDistributedProducerAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let partial = verifier.finish().expect("partial exact result");
    if !partial.is_empty() {
        coordinator
            .absorb_partial(&partial)
            .expect("merge partial exact result");
    }
    coordinator.finish(2).expect("distributed exact result")
}

#[test]
fn visible_seven_pc_uses_the_global_serial_policy_finalizer() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue-knowledge visible-7",
    )
    .expect("visible-seven distributed preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn finesse_score_remains_serial_when_multiple_workers_are_requested() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra finesse score --initial-mask 0 --height 4 \
         --placements I:spawn:3:0 --queue I --no-hold --workers 2",
    )
    .expect("finesse score preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn tiling_only_finesse_search_is_rejected_before_distribution() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let error = match WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra build-probability --base-mask 0 --target-mask 0xfc3f3fcff \
         --height 4 --queue OOOOOOO --no-hold --no-mirror --tiling-only \
         --finesse inputs --pattern-knowledge both --workers 2",
    ) {
        Err(error) => error,
        Ok(_) => panic!("tiling-only finesse must not enter a root-only worker path"),
    };

    assert_eq!(error.code(), "E_WASM_COMMAND_INVALID_VALUE");
}

#[test]
fn unavailable_gpu_distributed_preparation_is_an_explicit_cpu_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra pc --lines 4 --count unique --backend gpu \
                   --allow-backend-fallback --workers 2 --queue IOTSZJLIOTS";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("GPU CPU fallback");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::GpuDeviceNotFound
            );
        }
        _ => panic!("fallback-enabled GPU request must preserve the distributed CPU path"),
    }

    let result = run_distributed_cpu(&runtime, command);
    assert!(result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_device_not_found")
    );
    assert!(result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn unavailable_hybrid_distributed_preparation_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra pc --lines 4 --count unique --backend hybrid \
                   --no-backend-fallback --workers 2 --queue IOTSZJLIOTS";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("hybrid CPU selection");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Hybrid
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("hybrid request must preserve the distributed CPU path"),
    }

    let result = run_distributed_cpu(&runtime, command);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(!result.app_response().backend_report().fallback_used());
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(result.webgpu_backend().fallback_backend, None);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn build_probability_gpu_distributed_preparation_uses_kernel_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend gpu \
                   --allow-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("build-probability GPU fallback preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::GpuKernelUnavailable
            );
        }
        _ => panic!("fallback-enabled build-probability must preserve the CPU distributed path"),
    }
}

#[test]
fn build_probability_gpu_distributed_preparation_defers_denied_fallback_to_serial_contract() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend gpu \
                   --no-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("serial unsupported contract preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn build_probability_hybrid_distributed_preparation_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend hybrid \
                   --no-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("hybrid build-probability preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Hybrid
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("hybrid build-probability must preserve the CPU distributed path"),
    }
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
