use clearra_pc_graph::request::{GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::pc_execution_policy_diagnostic_builder::invalid_execution_policy;

pub(crate) fn validate_execution_policy_fields(
    policy: &PcExecutionPolicy,
    location: &'static str,
    report: &mut DiagnosticReport,
) {
    if let Some(requested) = policy.workers_requested() {
        let available = policy.worker_hardware_limit();
        let default_limit =
            clearra_pc_graph::request::WorkerPolicy::default_worker_limit_for_hardware(available);
        if requested == 0 {
            report.push(invalid_execution_policy(
                location,
                "PC execution workers must be at least 1",
                "execution_workers_zero",
            ));
        } else if requested > available {
            report.push(invalid_execution_policy(
                location,
                "PC execution workers exceed the logical processor hard limit",
                "execution_workers_exceed_hardware",
            ));
        } else if requested > default_limit && !policy.use_all_logical_processors() {
            report.push(invalid_execution_policy(
                location,
                "Using the reserved logical processor requires explicit all-CPU opt-in",
                "execution_workers_require_all_cpu_opt_in",
            ));
        }
    }

    if policy.max_memory_mib() == Some(0) {
        report.push(invalid_execution_policy(
            location,
            "PC execution memory budget must be at least 1 MiB when provided",
            "execution_max_memory_mib_zero",
        ));
    }

    if !matches!(
        policy.requested_backend(),
        RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
    ) && !matches!(policy.gpu_device(), GpuDeviceSelection::Auto)
    {
        report.push(invalid_execution_policy(
            location,
            "PC execution GPU device can be selected only with --backend gpu or --backend hybrid",
            "gpu_device_requires_gpu_backend",
        ));
    }
}
