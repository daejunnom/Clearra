use clearra_host_contract::{
    BackendReport, BackendStatusReport, CancelledReport, DiagnosticEvent, JobEvent,
    JobProgress as HostJobProgress, JobStarted, PartialResult as HostPartialResult, ResourceBudget,
};

use crate::wasm_worker_job::WasmWorkerJobEvent;

// R2 flow marker: command text -> WebCommandParser -> AppRequest -> Web Worker
// -> WASM CPU or WebGPU -> AppResponse / JobEvent.
pub fn wasm_worker_event_to_host_contract(event: &WasmWorkerJobEvent) -> JobEvent {
    match event {
        WasmWorkerJobEvent::Started { job_id } => JobEvent::Started(JobStarted::new(job_id.get())),
        WasmWorkerJobEvent::Progress { job_id, progress } => {
            let backend_status = BackendStatusReport::new(
                job_id.get(),
                "wasm-cpu",
                "auto",
                BackendReport::new(
                    progress.backend_status.backend_requested.as_str(),
                    progress.backend_status.backend_selected.as_str(),
                    progress.backend_status.fallback_reason.clone(),
                ),
            );
            JobEvent::Progress(HostJobProgress::new(
                job_id.get(),
                progress.done,
                progress.total,
                progress.label.as_str(),
                ResourceBudget::default(),
                backend_status,
            ))
        }
        WasmWorkerJobEvent::Diagnostic { job_id, diagnostic } => {
            JobEvent::Diagnostic(DiagnosticEvent::new(job_id.get(), diagnostic.clone()))
        }
        WasmWorkerJobEvent::PartialResult {
            job_id,
            partial,
            label,
            final_result,
        } => JobEvent::PartialResult(HostPartialResult::new(
            job_id.get(),
            label.as_str(),
            *partial,
            *final_result,
        )),
        WasmWorkerJobEvent::FinalResponse { response, .. } => JobEvent::Completed(response.clone()),
        WasmWorkerJobEvent::Failed { diagnostics, .. } => JobEvent::Failed(diagnostics.clone()),
        WasmWorkerJobEvent::Cancelled {
            job_id,
            scope_released,
        } => JobEvent::Cancelled(CancelledReport::new(job_id.get(), *scope_released)),
    }
}
