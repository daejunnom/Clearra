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
fn distributed_cpu_product_path_matches_serial_exact_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial = runtime
        .run_command_text(
            "clearra pc --lines 4 --count unique --backend cpu --workers 1 --queue IOTSZJLIOTS",
        )
        .expect("serial exact result");
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue IOTSZJLIOTS",
    )
    .expect("distributed preparation");
    let mut coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("4L two-worker request must use the distributed product path"),
    };
    let mut verifier = WasmDistributedVerifierRuntime::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue IOTSZJLIOTS",
    )
    .expect("distributed verifier");
    loop {
        match coordinator
            .advance_producer(16_384, 16)
            .expect("geometry producer")
        {
            WasmDistributedProducerAdvance::Pending => {}
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
    let distributed = coordinator.finish(2).expect("distributed exact result");

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
