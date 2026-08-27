use crate::{
    backend::{BackendFallback, SearchBackendReport, SelectedSearchBackend},
    buildup::BuildUpRunResult,
    packing::PackingRunResult,
    service::field,
};

pub(crate) fn backend_fields(
    report: &SearchBackendReport,
    policy: &clearra_pc_graph::request::PcExecutionPolicy,
    packing: &PackingRunResult,
    buildup: &BuildUpRunResult,
) -> Vec<(String, String)> {
    backend_fields_with_buildup(report, policy, packing, buildup.buildup_backend(), "cpu")
}

pub(crate) fn tiling_backend_fields(
    report: &SearchBackendReport,
    policy: &clearra_pc_graph::request::PcExecutionPolicy,
    packing: &PackingRunResult,
    packing_source_raw_geometry: bool,
) -> Vec<(String, String)> {
    if packing_source_raw_geometry {
        backend_fields_with_buildup(report, policy, packing, "none", "none")
    } else {
        backend_fields_with_buildup(
            report,
            policy,
            packing,
            "embedded-in-packing",
            "packing-runner",
        )
    }
}

fn backend_fields_with_buildup(
    report: &SearchBackendReport,
    policy: &clearra_pc_graph::request::PcExecutionPolicy,
    packing: &PackingRunResult,
    buildup_backend: &'static str,
    buildup_backend_owner: &'static str,
) -> Vec<(String, String)> {
    let fallback = BackendFallback::from_report(report);
    let candidate_backend = packing.execution_source().candidate_backend();
    let hybrid_scheduler = packing.hybrid_scheduler_report();
    let actual_backend = packing.actual_backend();
    let trust = packing.trust_report();
    let gpu_executed = matches!(
        actual_backend,
        SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid
    );
    let gpu_available = report.gpu_available();
    let gpu_disabled_reason = gpu_disabled_reason(report, fallback);
    let gpu_failure = report.gpu_failure();
    let gpu_device = report.gpu_device();
    vec![
        field("requested_backend", policy.requested_backend().as_str()),
        field("backend_requested", report.requested_backend().as_str()),
        field("selected_backend", report.selected_backend().as_str()),
        field(
            "backend_selected",
            backend_surface(report.selected_backend()),
        ),
        field("effective_backend", report.selected_backend().as_str()),
        field(
            "packing_backend",
            backend_surface(report.selected_backend()),
        ),
        field("candidate_backend", candidate_backend),
        field("buildup_backend", buildup_backend),
        field("buildup_backend_owner", buildup_backend_owner),
        field("gpu_confirmed", gpu_executed && trust.cpu_confirmed()),
        field("cpu_confirmed", trust.cpu_confirmed()),
        field("selected_model", report.selected_model().as_str()),
        field("compute_device", report.compute_device().as_str()),
        field("search_result_model", report.result_model().as_str()),
        field(
            "backend_selection_reason",
            report.selection_reason().as_str(),
        ),
        field("backend_fallback_allowed", policy.allow_backend_fallback()),
        field("backend_fallback_used", fallback.used()),
        field("fallback_used", fallback.used()),
        field(
            "fallback_backend",
            if fallback.used() {
                backend_surface(report.selected_backend())
            } else {
                "none"
            },
        ),
        field("backend_fallback_reason", fallback.reason_label()),
        field(
            "gpu_failure_class",
            gpu_failure.map_or("none", |failure| failure.class().as_str()),
        ),
        field(
            "gpu_failure_stage",
            gpu_failure.map_or("none", |failure| failure.stage().as_str()),
        ),
        field(
            "discarded_partial_gpu_result",
            gpu_failure.is_some_and(|failure| failure.discarded_partial_gpu_result()),
        ),
        field("gpu_available", gpu_available),
        field("gpu_disabled_reason", gpu_disabled_reason),
        field("gpu_worker_state", gpu_worker_state(report, gpu_executed)),
        field(
            "gpu_trust_state",
            gpu_trust_state(report, gpu_executed, trust.state().as_str()),
        ),
        field(
            "gpu_memory_ticket_id",
            hybrid_scheduler.gpu_worker_memory_ticket_id(),
        ),
        field("gpu_fence_epoch", hybrid_scheduler.gpu_worker_fence_epoch()),
        field(
            "cpu_confirm_required",
            gpu_executed && !trust.deterministic_reference_matched(),
        ),
        field(
            "gpu_can_source_exact_probability",
            gpu_executed && trust.can_source_exact_probability(),
        ),
        field(
            "deterministic_reference_matched",
            gpu_executed && trust.deterministic_reference_matched(),
        ),
        field(
            "cpu_reference_matched",
            gpu_executed && trust.cpu_confirmed(),
        ),
        field("gpu_worker_fallback_reason", fallback.reason_label()),
        field("hybrid_status", hybrid_status(report)),
        field("hybrid_disabled_reason", hybrid_disabled_reason(report)),
        field(
            "backpressure",
            hybrid_scheduler.gpu_worker_backpressure_throttle_reason(),
        ),
        field(
            "gpu_backpressure_gpu_queue_depth",
            hybrid_scheduler.gpu_worker_backpressure_gpu_queue_depth(),
        ),
        field(
            "gpu_backpressure_cpu_worker_queue_depth",
            hybrid_scheduler.gpu_worker_backpressure_cpu_worker_queue_depth(),
        ),
        field(
            "gpu_backpressure_readback_pending_batches",
            hybrid_scheduler.gpu_worker_backpressure_readback_pending_batches(),
        ),
        field(
            "gpu_backpressure_build_variant_buffer_pressure",
            hybrid_scheduler.gpu_worker_backpressure_build_variant_buffer_pressure(),
        ),
        field(
            "gpu_backpressure_coverage_row_buffer_pressure",
            hybrid_scheduler.gpu_worker_backpressure_coverage_row_buffer_pressure(),
        ),
        field(
            "gpu_backpressure_throttled_backend",
            hybrid_scheduler.gpu_worker_backpressure_throttled_backend(),
        ),
        field(
            "gpu_backpressure_throttle_reason",
            hybrid_scheduler.gpu_worker_backpressure_throttle_reason(),
        ),
        field("execution_workers", report.workers_used()),
        field(
            "workers_requested",
            worker_requested(policy.workers_requested()),
        ),
        field("workers_used", report.workers_used()),
        field("execution_deterministic", report.deterministic_order()),
        field(
            "execution_max_frontier_states",
            report.max_frontier_states(),
        ),
        field("execution_max_candidates", report.max_candidates()),
        field("execution_max_patterns", policy.max_patterns()),
        field(
            "execution_max_memory_mib",
            memory_mib(report.max_memory_mib()),
        ),
        field("gpu_device", policy.gpu_device().as_display_string()),
        field(
            "gpu_device_selected_index",
            gpu_device
                .and_then(|device| device.selected_index())
                .map_or_else(|| "none".to_owned(), |index| index.to_string()),
        ),
        field(
            "gpu_device_selected_name",
            gpu_device
                .and_then(|device| device.selected_name())
                .unwrap_or("none"),
        ),
        field(
            "gpu_device_selected_type",
            gpu_device
                .and_then(|device| device.selected_device_type())
                .unwrap_or("none"),
        ),
        field(
            "gpu_device_selected_backend",
            gpu_device
                .and_then(|device| device.selected_backend())
                .unwrap_or("none"),
        ),
        field(
            "gpu_device_selected_vendor",
            gpu_device
                .and_then(|device| device.selected_vendor())
                .map_or_else(|| "none".to_owned(), |vendor| format!("{vendor:04x}")),
        ),
        field(
            "gpu_device_selected_device",
            gpu_device
                .and_then(|device| device.selected_device())
                .map_or_else(|| "none".to_owned(), |device| format!("{device:04x}")),
        ),
        field(
            "gpu_unavailable_reason",
            report
                .gpu_unavailable_reason()
                .map_or("none", |reason| reason.as_str()),
        ),
    ]
}

pub(crate) fn solver_backend(packing: &PackingRunResult) -> &'static str {
    match packing.actual_backend() {
        SelectedSearchBackend::Gpu => "core-c-gpu-packing-cpu-buildup",
        SelectedSearchBackend::Hybrid => "core-c-hybrid-packing-cpu-buildup",
        SelectedSearchBackend::None
        | SelectedSearchBackend::CpuGeometryExactCover
        | SelectedSearchBackend::CpuParallelGeometryExactCover => "core-c-cpu-packing-cpu-buildup",
    }
}

pub(crate) fn tiling_solver_backend(
    packing: &PackingRunResult,
    packing_source_raw_geometry: bool,
) -> &'static str {
    match (packing.actual_backend(), packing_source_raw_geometry) {
        (SelectedSearchBackend::Gpu, true) => "core-c-gpu-packing-no-buildup",
        (SelectedSearchBackend::Hybrid, true) => "core-c-hybrid-packing-no-buildup",
        (
            SelectedSearchBackend::None
            | SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover,
            true,
        ) => "core-c-cpu-packing-no-buildup",
        (SelectedSearchBackend::Gpu, false) => "core-c-gpu-packing-embedded-buildability-subset",
        (SelectedSearchBackend::Hybrid, false) => {
            "core-c-hybrid-packing-embedded-buildability-subset"
        }
        (
            SelectedSearchBackend::None
            | SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover,
            false,
        ) => "core-c-cpu-packing-embedded-buildability-subset",
    }
}

fn backend_surface(backend: crate::backend::SelectedSearchBackend) -> &'static str {
    match backend {
        crate::backend::SelectedSearchBackend::None => "none",
        crate::backend::SelectedSearchBackend::CpuGeometryExactCover
        | crate::backend::SelectedSearchBackend::CpuParallelGeometryExactCover => "cpu",
        crate::backend::SelectedSearchBackend::Gpu => "gpu",
        crate::backend::SelectedSearchBackend::Hybrid => "hybrid",
    }
}

fn gpu_disabled_reason(report: &SearchBackendReport, fallback: BackendFallback) -> &'static str {
    if let Some(reason) = report.gpu_unavailable_reason() {
        return reason.as_str();
    }

    if report.requested_backend().requires_gpu() {
        return fallback.reason_label();
    }

    "not_requested"
}

fn gpu_worker_state(report: &SearchBackendReport, gpu_executed: bool) -> &'static str {
    if gpu_executed {
        return "completed";
    }
    if let Some(failure) = report.gpu_failure() {
        return if failure.class().as_str() == "unavailable" {
            "unavailable"
        } else {
            "failed"
        };
    }
    "not-used"
}

fn gpu_trust_state(
    report: &SearchBackendReport,
    gpu_executed: bool,
    executed_trust_state: &'static str,
) -> &'static str {
    if gpu_executed {
        executed_trust_state
    } else if report.backend_fallback_used() && report.gpu_failure().is_some() {
        "fallback-used"
    } else {
        "not-used"
    }
}

fn hybrid_status(report: &SearchBackendReport) -> &'static str {
    if report.requested_backend().as_str() != "hybrid" {
        return "not-requested";
    }

    match report.selected_backend() {
        SelectedSearchBackend::Gpu => "gpu-ready",
        SelectedSearchBackend::CpuGeometryExactCover
        | SelectedSearchBackend::CpuParallelGeometryExactCover => "cpu-selected",
        SelectedSearchBackend::None | SelectedSearchBackend::Hybrid => "unavailable",
    }
}

fn hybrid_disabled_reason(report: &SearchBackendReport) -> &'static str {
    if report.requested_backend().as_str() != "hybrid" {
        return "not_requested";
    }

    report
        .gpu_unavailable_reason()
        .map_or("none", |reason| reason.as_str())
}

fn worker_requested(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "auto".to_owned())
}

fn memory_mib(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}
