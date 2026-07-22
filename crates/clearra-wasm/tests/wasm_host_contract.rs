use clearra_host_contract::{AppCommandKind, AppStatus, JobEvent, QueryEnvelope};
use clearra_wasm::{
    wasm_worker_event_to_host_contract, WasmCommandRuntime, WasmWorkerAdvanceStatus,
    WasmWorkerJobEvent, WasmWorkerJobRuntime,
};

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job("clearra pc --lines 2 | clearra verify")
        .expect("job starts before parser phase");
    while !runtime
        .advance_job(job_id, 64)
        .expect("parser failure becomes event")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Failed(report)
            if report.diagnostics().iter().any(|diagnostic|
                diagnostic.code() == "E_WASM_PROCESS_SEMANTICS_FORBIDDEN")
    )));
}

#[test]
fn cli_gui_wasm_share_app_request_schema() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend cpu")
        .expect("AppRequest");

    assert_eq!(request.query(), &QueryEnvelope::PcOpening);
    assert_eq!(request.backend_policy().backend_requested(), "cpu");
}

#[test]
fn wasm_worker_event_maps_to_host_contract_job_event() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime.start_job("clearra verify kicks").expect("job");
    while !runtime
        .advance_job(job_id, 64)
        .expect("job runs")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Progress(_)
    )));
    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Completed(_)
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::FinalResponse {
            response,
            ..
        } if response.command() == Some(AppCommandKind::VerifyKicks)
            && response.status() == AppStatus::Success
            && response.result().is_some_and(|result| result.kind() == "verify-kicks")
    )));

    let cancelled_job = runtime.start_job("clearra verify kicks").expect("job");
    assert_eq!(
        runtime
            .advance_job(cancelled_job, 64)
            .expect("prepare active computation"),
        WasmWorkerAdvanceStatus::Pending
    );
    assert_eq!(
        runtime.status(cancelled_job),
        Some(clearra_wasm::WasmWorkerJobStatus::Running)
    );
    let cancellation = runtime
        .cancellation_token(cancelled_job)
        .expect("active computation scope");
    runtime.cancel_job(cancelled_job).expect("cancel job");
    assert!(cancellation.is_cancelled());
    assert!(runtime.drain_events(cancelled_job).iter().any(|event| {
        matches!(
            event,
            WasmWorkerJobEvent::Cancelled {
                scope_released: true,
                ..
            }
        )
    }));
}
