#![cfg(not(target_arch = "wasm32"))]

use clearra_wasm::{
    WasmWorkerAdvanceStatus, WasmWorkerJobEvent, WasmWorkerJobId, WasmWorkerJobRuntime,
    WasmWorkerJobStatus,
};

const NATIVE_INTEGRATION_TEST_STACK_BYTES: usize = 16 * 1024 * 1024;
const PC_MINIMALS_COMMAND: &str = "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 \
     --pieces 1 --queue I --hold empty --backend cpu --workers 1";

#[test]
fn workers_one_pc_minimals_exposes_cancellable_app_exact_finalization_boundary() {
    let handle = std::thread::Builder::new()
        .name("pc-minimals-serial-postprocess".to_owned())
        .stack_size(NATIVE_INTEGRATION_TEST_STACK_BYTES)
        .spawn(|| {
            assert_workers_one_pc_minimals_postprocess_boundary();
            assert_pc_minimals_cancellation_before_finalizer_drops_authority();
        })
        .expect("spawn serial pc.minimals contract thread");

    handle
        .join()
        .expect("serial pc.minimals integration contract thread");
}

fn assert_workers_one_pc_minimals_postprocess_boundary() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = advance_to_postprocess_boundary(&mut runtime);

    let finalize_ready_status = runtime
        .advance_job(job_id, 2_048)
        .expect("materialize typed pc.minimals response before exact App finalization");
    assert_eq!(finalize_ready_status, WasmWorkerAdvanceStatus::Progress);
    let finalize_ready_events = runtime.drain_events(job_id);
    assert!(
        finalize_ready_events
            .iter()
            .all(|event| !is_terminal_event(event)),
        "typed pc.minimals validation must not run in its postprocess advance"
    );
    assert!(finalize_ready_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Progress { progress, .. }
            if progress.label == "postprocess"
                && progress.done == 1
                && progress.total == 1
    )));
    assert!(finalize_ready_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Progress { progress, .. }
            if progress.label == "pc-minimals-finalize"
                && progress.done == 0
                && progress.total == 1
    )));
    assert_eq!(runtime.status(job_id), Some(WasmWorkerJobStatus::Running));

    let terminal_status = runtime
        .advance_job(job_id, 2_048)
        .expect("run exact App finalization after the host-visible boundary");
    assert_eq!(terminal_status, WasmWorkerAdvanceStatus::Completed);
    let terminal_events = runtime.drain_events(job_id);
    assert!(terminal_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Progress { progress, .. }
            if progress.label == "pc-minimals-finalize"
                && progress.done == 1
                && progress.total == 1
    )));
    assert!(terminal_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::FinalResponse { response, .. }
            if response.status() == clearra_host_contract::AppStatus::Success
    )));
}

fn assert_pc_minimals_cancellation_before_finalizer_drops_authority() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = advance_to_postprocess_boundary(&mut runtime);
    assert_eq!(
        runtime
            .advance_job(job_id, 2_048)
            .expect("reach the typed pc.minimals finalizer boundary"),
        WasmWorkerAdvanceStatus::Progress
    );
    let boundary_events = runtime.drain_events(job_id);
    assert!(boundary_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Progress { progress, .. }
            if progress.label == "pc-minimals-finalize"
                && progress.done == 0
                && progress.total == 1
    )));

    runtime
        .cancellation_token(job_id)
        .expect("running pc.minimals cancellation authority")
        .cancel();
    assert_eq!(
        runtime
            .advance_job(job_id, 2_048)
            .expect("observe cancellation before exact App validation"),
        WasmWorkerAdvanceStatus::Cancelled
    );
    let cancelled_events = runtime.drain_events(job_id);
    assert!(cancelled_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Cancelled {
            scope_released: true,
            ..
        }
    )));
    assert!(cancelled_events
        .iter()
        .all(|event| !matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));
    assert_eq!(runtime.status(job_id), Some(WasmWorkerJobStatus::Cancelled));
    assert!(
        runtime.take_completed_product_page_source_owner().is_none(),
        "cancellation before the finalizer must not produce paging authority"
    );
}

fn advance_to_postprocess_boundary(runtime: &mut WasmWorkerJobRuntime) -> WasmWorkerJobId {
    let job_id = runtime
        .start_job(PC_MINIMALS_COMMAND)
        .expect("serial browser pc.minimals job");

    assert_eq!(
        runtime
            .advance_job(job_id, 2_048)
            .expect("prepare serial pc.minimals"),
        WasmWorkerAdvanceStatus::Pending
    );
    let prepared_events = runtime.drain_events(job_id);
    assert!(prepared_events
        .iter()
        .all(|event| !is_terminal_event(event)));

    let mut observed_postprocess = false;
    for _ in 0..128 {
        let status = runtime
            .advance_job(job_id, 2_048)
            .expect("advance bounded serial pc.minimals search");
        let events = runtime.drain_events(job_id);
        assert!(
            events.iter().all(|event| !is_terminal_event(event)),
            "the exact App result must not become terminal before the postprocess host boundary"
        );
        if status == WasmWorkerAdvanceStatus::Progress {
            observed_postprocess = events.iter().any(|event| {
                matches!(
                    event,
                    WasmWorkerJobEvent::Progress { progress, .. }
                        if progress.label == "postprocess"
                            && progress.done == 0
                            && progress.total == 1
                )
            });
            break;
        }
        assert_eq!(status, WasmWorkerAdvanceStatus::Pending);
    }

    assert!(
        observed_postprocess,
        "serial pc.minimals must expose its finalizing boundary before exact App validation"
    );
    assert_eq!(runtime.status(job_id), Some(WasmWorkerJobStatus::Running));
    job_id
}

fn is_terminal_event(event: &WasmWorkerJobEvent) -> bool {
    matches!(
        event,
        WasmWorkerJobEvent::FinalResponse { .. }
            | WasmWorkerJobEvent::Failed { .. }
            | WasmWorkerJobEvent::Cancelled { .. }
    )
}
