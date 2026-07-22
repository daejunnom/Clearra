use crate::{
    diagnostic::{diagnostic::Diagnostic, diagnostic_code::DiagnosticCode},
    evidence::validation_evidence::ValidationEvidence,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuWorkerDiagnosticInput {
    pub state: String,
    pub trust_state: String,
    pub fallback_reason: Option<String>,
    pub unavailable_reason: Option<String>,
    pub throttle_reason: Option<String>,
    pub memory_ticket_id: Option<u64>,
    pub fence_epoch: Option<u64>,
    pub scope_epoch: Option<u64>,
    pub byte_budget: Option<u64>,
    pub pending_release_queue: Option<u64>,
    pub pending_gpu_buffer_releases: Option<u64>,
    pub memory_pressure_level: Option<String>,
}

impl GpuWorkerDiagnosticInput {
    pub fn new(state: impl Into<String>, trust_state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            trust_state: trust_state.into(),
            fallback_reason: None,
            unavailable_reason: None,
            throttle_reason: None,
            memory_ticket_id: None,
            fence_epoch: None,
            scope_epoch: None,
            byte_budget: None,
            pending_release_queue: None,
            pending_gpu_buffer_releases: None,
            memory_pressure_level: None,
        }
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_fallback_reason(mut self, reason: impl Into<String>) -> Self {
        self.fallback_reason = Some(reason.into());
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_unavailable_reason(mut self, reason: impl Into<String>) -> Self {
        self.unavailable_reason = Some(reason.into());
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_throttle_reason(mut self, reason: impl Into<String>) -> Self {
        self.throttle_reason = Some(reason.into());
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_memory_ticket(mut self, memory_ticket_id: u64, fence_epoch: u64) -> Self {
        self.memory_ticket_id = Some(memory_ticket_id);
        self.fence_epoch = Some(fence_epoch);
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_scope_epoch(mut self, scope_epoch: u64) -> Self {
        self.scope_epoch = Some(scope_epoch);
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_byte_budget(mut self, byte_budget: u64) -> Self {
        self.byte_budget = Some(byte_budget);
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_pending_release_queue(mut self, pending_release_queue: u64) -> Self {
        self.pending_release_queue = Some(pending_release_queue);
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_pending_gpu_buffer_releases(mut self, pending_gpu_buffer_releases: u64) -> Self {
        self.pending_gpu_buffer_releases = Some(pending_gpu_buffer_releases);
        self
    }
}
impl GpuWorkerDiagnosticInput {
    pub fn with_memory_pressure_level(mut self, memory_pressure_level: impl Into<String>) -> Self {
        self.memory_pressure_level = Some(memory_pressure_level.into());
        self
    }
}

pub fn gpu_backend_fallback_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WGpuBackendFallback,
        "GPU backend fallback was used",
        input,
    )
}

pub fn gpu_device_unavailable_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WGpuDeviceUnavailable,
        "GPU device is unavailable",
        input,
    )
}

pub fn hybrid_backpressure_active_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WHybridBackpressureActive,
        "Hybrid backend backpressure is active",
        input,
    )
}

pub fn gpu_worker_trust_mismatch_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::EGpuWorkerTrustMismatch,
        "GPU worker trust state rejected exact output",
        input,
    )
}

pub fn gpu_worker_memory_ticket_missing_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::EGpuWorkerMemoryTicketMissing,
        "GPU worker result is missing a memory ticket",
        input,
    )
}

pub fn gpu_buffer_fence_missing_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::EGpuBufferFenceMissing,
        "GPU buffer release is missing a fence epoch",
        input,
    )
}

pub fn gpu_buffer_release_deferred_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WGpuBufferReleaseDeferred,
        "GPU buffer release is deferred until the safe epoch",
        input,
    )
}

pub fn pending_release_queue_not_drained_diagnostic(
    input: &GpuWorkerDiagnosticInput,
) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WPendingReleaseQueueNotDrained,
        "Pending release queue was not drained",
        input,
    )
}

pub fn memory_pressure_high_diagnostic(input: &GpuWorkerDiagnosticInput) -> Diagnostic {
    gpu_worker_diagnostic(
        DiagnosticCode::WMemoryPressureHigh,
        "GPU worker memory pressure is high",
        input,
    )
}

fn gpu_worker_diagnostic(
    code: DiagnosticCode,
    message: &'static str,
    input: &GpuWorkerDiagnosticInput,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(code, message)
        .with_evidence(ValidationEvidence::new("gpu_worker_state", &input.state))
        .with_evidence(ValidationEvidence::new(
            "gpu_worker_trust_state",
            &input.trust_state,
        ));

    if let Some(reason) = &input.fallback_reason {
        diagnostic = diagnostic.with_evidence(ValidationEvidence::new("fallback_reason", reason));
    }
    if let Some(reason) = &input.unavailable_reason {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("unavailable_reason", reason));
    }
    if let Some(reason) = &input.throttle_reason {
        diagnostic = diagnostic.with_evidence(ValidationEvidence::new("throttle_reason", reason));
    }
    if let Some(id) = input.memory_ticket_id {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("memory_ticket_id", id.to_string()));
    }
    if let Some(epoch) = input.fence_epoch {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("fence_epoch", epoch.to_string()));
    }
    if let Some(epoch) = input.scope_epoch {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("scope_epoch", epoch.to_string()));
    }
    if let Some(budget) = input.byte_budget {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("byte_budget", budget.to_string()));
    }
    if let Some(pending) = input.pending_release_queue {
        diagnostic = diagnostic.with_evidence(ValidationEvidence::new(
            "pending_release_queue",
            pending.to_string(),
        ));
    }
    if let Some(pending) = input.pending_gpu_buffer_releases {
        diagnostic = diagnostic.with_evidence(ValidationEvidence::new(
            "pending_gpu_buffer_releases",
            pending.to_string(),
        ));
    }
    if let Some(level) = &input.memory_pressure_level {
        diagnostic =
            diagnostic.with_evidence(ValidationEvidence::new("memory_pressure_level", level));
    }

    diagnostic
}

#[cfg(test)]
#[path = "gpu_worker_diagnostic_tests.rs"]
mod tests;
