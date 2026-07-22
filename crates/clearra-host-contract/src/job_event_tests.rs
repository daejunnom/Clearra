use crate::{AppCommandKind, AppResult, AppStatus, QueryEnvelope};

use super::*;

#[test]
fn cli_gui_wasm_share_app_request_schema() {
    let request = crate::AppRequest::new(AppCommandKind::Pc, QueryEnvelope::PcOpening)
        .with_resource_budget(ResourceBudget::new(6, Some(1024), Some(256)));

    let encoded = serde_json::to_value(&request).expect("request json");

    assert_eq!(encoded["command"], "pc");
    assert_eq!(encoded["query"], "pc-opening");
    assert_eq!(encoded["backend_policy"]["backend_requested"], "auto");
    assert_eq!(encoded["resource_budget"]["workers"], 6);
}

#[test]
fn job_event_reports_resource_budget() {
    let budget = ResourceBudget::new(4, Some(2048), Some(512));
    let progress = JobProgress::new(
        7,
        1,
        3,
        "compile AppRequest",
        budget,
        BackendStatusReport::wasm_cpu(7),
    );
    let event = JobEvent::Progress(progress);

    match event {
        JobEvent::Progress(progress) => {
            assert_eq!(progress.resource_budget().workers(), 4);
            assert_eq!(progress.resource_budget().candidate_budget(), Some(2048));
        }
        _ => panic!("expected progress event"),
    }
}

#[test]
fn job_event_reports_search_and_post_backend() {
    let backend_status = BackendStatusReport::new(
        9,
        "wasm-cpu",
        "webgpu",
        BackendReport::new("auto", "clearra-wasm", Some("webgpu_adapter_unavailable")),
    );

    assert_eq!(backend_status.search_backend(), "wasm-cpu");
    assert_eq!(backend_status.post_backend(), "webgpu");
    assert_eq!(
        backend_status.backend_report().fallback_reason(),
        Some("webgpu_adapter_unavailable")
    );
}

#[test]
fn completed_job_event_carries_app_response_contract() {
    let response = crate::AppResponse::success(AppCommandKind::Pc, AppResult::new("pc"))
        .with_backend_report(BackendReport::new("cpu", "cpu", None::<String>));
    let event = JobEvent::Completed(response);

    match event {
        JobEvent::Completed(response) => {
            assert_eq!(response.command(), Some(AppCommandKind::Pc));
            assert_eq!(response.status(), AppStatus::Success);
            assert_eq!(response.result().expect("result").kind(), "pc");
        }
        _ => panic!("expected completed event"),
    }
}
