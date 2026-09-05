#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuBackendOutcomeState {
    NotRequested,
    Connected,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuReportTrustState {
    NotUsed,
    TrustedCpuSampleConfirmed,
    Unavailable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebGpuLimitsReport {
    pub max_storage_buffer_binding_size: u64,
    pub max_compute_workgroup_storage_size: u64,
    pub max_compute_invocations_per_workgroup: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuMemoryReport {
    pub wasm_memory_usage: String,
    pub wasm_memory_pressure: String,
}

impl WebGpuMemoryReport {
    fn not_reported() -> Self {
        Self {
            wasm_memory_usage: "not-reported-backend-unavailable".to_owned(),
            wasm_memory_pressure: "not-reported-backend-unavailable".to_owned(),
        }
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.wasm_memory_usage.capacity() as u128)
            .checked_add(self.wasm_memory_pressure.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuShaderReport {
    pub shader_compile_status: String,
    pub shader_hash: Option<String>,
    pub shader_version: Option<String>,
    pub embedded_reviewed: bool,
    pub user_shader_allowed: bool,
    pub runtime_shader_injection_allowed: bool,
}

impl WebGpuShaderReport {
    fn backend_unavailable(status: &str) -> Self {
        Self {
            shader_compile_status: status.to_owned(),
            shader_hash: None,
            shader_version: None,
            embedded_reviewed: false,
            user_shader_allowed: false,
            runtime_shader_injection_allowed: false,
        }
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.shader_compile_status.capacity() as u128;
        for value in [&self.shader_hash, &self.shader_version]
            .into_iter()
            .flatten()
        {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuBackendReport {
    pub outcome_state: WebGpuBackendOutcomeState,
    pub webgpu_available: bool,
    pub webgpu_adapter_label_or_redacted: String,
    pub webgpu_limits: WebGpuLimitsReport,
    pub webgpu_required_limits: WebGpuLimitsReport,
    pub webgpu_unavailable_reason: Option<String>,
    pub expected_digest: Option<String>,
    pub actual_digest: Option<String>,
    pub shader: WebGpuShaderReport,
    pub memory: WebGpuMemoryReport,
    pub fallback_used: bool,
    pub fallback_backend: Option<String>,
    pub gpu_warmup_requested: bool,
    pub gpu_warmup_performed: bool,
    pub gpu_session_reused: bool,
    pub gpu_trust_state: WebGpuReportTrustState,
    pub cpu_confirmed: bool,
    pub can_source_exact_probability: bool,
}

impl WebGpuBackendReport {
    pub fn not_requested() -> Self {
        Self::without_backend(
            WebGpuBackendOutcomeState::NotRequested,
            None,
            "not-requested",
        )
    }

    pub fn search_unavailable(reason: &str, fallback_to_wasm_cpu: bool) -> Self {
        let mut report = Self::without_backend(
            WebGpuBackendOutcomeState::Unavailable,
            Some(reason),
            "search-backend-unavailable",
        );
        report.fallback_used = fallback_to_wasm_cpu;
        report.fallback_backend = fallback_to_wasm_cpu.then(|| "wasm-cpu".to_owned());
        report
    }

    pub(crate) fn from_app_response(response: &clearra_app::AppResponse, requested: bool) -> Self {
        if !requested {
            return Self::not_requested();
        }
        let Some(result) = response
            .render_model()
            .and_then(|model| model.core_result())
        else {
            let backend = response.backend_report();
            let reason = backend
                .backend_fallback_reason()
                .map(ToOwned::to_owned)
                .or_else(
                    || match (backend.gpu_failure_class(), backend.gpu_failure_stage()) {
                        (Some(class), Some(stage)) => Some(format!("{class}:{stage}")),
                        (Some(class), None) => Some(class.to_owned()),
                        (None, Some(stage)) => Some(stage.to_owned()),
                        (None, None) => None,
                    },
                )
                .unwrap_or_else(|| "webgpu_result_not_materialized".to_owned());
            return Self::search_unavailable(&reason, backend.fallback_used());
        };
        let backend = result.field("backend_selected").unwrap_or("none");
        if backend == "webgpu" {
            return Self {
                outcome_state: WebGpuBackendOutcomeState::Connected,
                webgpu_available: true,
                webgpu_adapter_label_or_redacted: result
                    .field("gpu_adapter")
                    .unwrap_or("redacted")
                    .to_owned(),
                webgpu_limits: WebGpuLimitsReport::default(),
                webgpu_required_limits: WebGpuLimitsReport::default(),
                webgpu_unavailable_reason: None,
                expected_digest: result
                    .field("packing_candidate_set_digest")
                    .map(ToOwned::to_owned),
                actual_digest: result
                    .field("packing_candidate_set_digest")
                    .map(ToOwned::to_owned),
                shader: WebGpuShaderReport {
                    shader_compile_status: "connected".to_owned(),
                    shader_hash: result.field("gpu_shader_hash").map(ToOwned::to_owned),
                    shader_version: result.field("gpu_shader_version").map(ToOwned::to_owned),
                    embedded_reviewed: true,
                    user_shader_allowed: false,
                    runtime_shader_injection_allowed: false,
                },
                memory: WebGpuMemoryReport {
                    wasm_memory_usage: result.field("gpu_peak_bytes").unwrap_or("0").to_owned(),
                    wasm_memory_pressure: "within-reported-budget".to_owned(),
                },
                fallback_used: false,
                fallback_backend: None,
                gpu_warmup_requested: result.bool_field("gpu_warmup_requested").unwrap_or(false),
                gpu_warmup_performed: result.bool_field("gpu_warmup_performed").unwrap_or(false),
                gpu_session_reused: result.bool_field("gpu_session_reused").unwrap_or(false),
                gpu_trust_state: WebGpuReportTrustState::TrustedCpuSampleConfirmed,
                cpu_confirmed: true,
                can_source_exact_probability: result
                    .bool_field("probability_complete")
                    .unwrap_or(false),
            };
        }
        let fallback_used = result.bool_field("backend_fallback_used").unwrap_or(false);
        let reason = result
            .field("gpu_disabled_reason")
            .filter(|reason| !matches!(*reason, "none" | "not_requested"))
            .or_else(|| {
                result
                    .field("backend_fallback_reason")
                    .filter(|reason| *reason != "none")
            })
            .unwrap_or("webgpu_not_selected");
        let mut report = Self::search_unavailable(reason, fallback_used);
        report.gpu_warmup_requested = result.bool_field("gpu_warmup_requested").unwrap_or(false);
        report.gpu_warmup_performed = result.bool_field("gpu_warmup_performed").unwrap_or(false);
        report.gpu_session_reused = result.bool_field("gpu_session_reused").unwrap_or(false);
        report
    }

    fn without_backend(
        outcome_state: WebGpuBackendOutcomeState,
        reason: Option<&str>,
        shader_status: &str,
    ) -> Self {
        Self {
            outcome_state,
            webgpu_available: false,
            webgpu_adapter_label_or_redacted: "redacted".to_owned(),
            webgpu_limits: WebGpuLimitsReport::default(),
            webgpu_required_limits: WebGpuLimitsReport::default(),
            webgpu_unavailable_reason: reason.map(ToOwned::to_owned),
            expected_digest: None,
            actual_digest: None,
            shader: WebGpuShaderReport::backend_unavailable(shader_status),
            memory: WebGpuMemoryReport::not_reported(),
            fallback_used: false,
            fallback_backend: None,
            gpu_warmup_requested: false,
            gpu_warmup_performed: false,
            gpu_session_reused: false,
            gpu_trust_state: if reason.is_some() {
                WebGpuReportTrustState::Unavailable
            } else {
                WebGpuReportTrustState::NotUsed
            },
            cpu_confirmed: false,
            can_source_exact_probability: false,
        }
    }

    /// Returns every heap allocation retained by the WebGPU report, using
    /// actual allocator capacities. Inline limits, flags, and enums are
    /// excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.webgpu_adapter_label_or_redacted.capacity() as u128;
        for value in [
            &self.webgpu_unavailable_reason,
            &self.expected_digest,
            &self.actual_digest,
            &self.fallback_backend,
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(self.shader.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(self.memory.checked_retained_capacity_bytes()?)?;
        Some(bytes)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::*;

    fn allocated(capacity: usize, value: &str) -> String {
        let mut output = String::with_capacity(capacity);
        output.push_str(value);
        output
    }

    #[test]
    fn report_counts_nested_string_slack_fieldwise() {
        let report = WebGpuBackendReport {
            outcome_state: WebGpuBackendOutcomeState::Unavailable,
            webgpu_available: false,
            webgpu_adapter_label_or_redacted: allocated(32, "redacted"),
            webgpu_limits: WebGpuLimitsReport::default(),
            webgpu_required_limits: WebGpuLimitsReport::default(),
            webgpu_unavailable_reason: Some(allocated(48, "unavailable")),
            expected_digest: Some(allocated(64, "expected")),
            actual_digest: Some(allocated(80, "actual")),
            shader: WebGpuShaderReport {
                shader_compile_status: allocated(96, "not-compiled"),
                shader_hash: Some(allocated(112, "hash")),
                shader_version: Some(allocated(128, "version")),
                embedded_reviewed: false,
                user_shader_allowed: false,
                runtime_shader_injection_allowed: false,
            },
            memory: WebGpuMemoryReport {
                wasm_memory_usage: allocated(144, "0"),
                wasm_memory_pressure: allocated(160, "not-reported"),
            },
            fallback_used: true,
            fallback_backend: Some(allocated(176, "wasm-cpu")),
            gpu_warmup_requested: false,
            gpu_warmup_performed: false,
            gpu_session_reused: false,
            gpu_trust_state: WebGpuReportTrustState::Unavailable,
            cpu_confirmed: false,
            can_source_exact_probability: false,
        };
        let expected = [
            Some(&report.webgpu_adapter_label_or_redacted),
            report.webgpu_unavailable_reason.as_ref(),
            report.expected_digest.as_ref(),
            report.actual_digest.as_ref(),
            Some(&report.shader.shader_compile_status),
            report.shader.shader_hash.as_ref(),
            report.shader.shader_version.as_ref(),
            Some(&report.memory.wasm_memory_usage),
            Some(&report.memory.wasm_memory_pressure),
            report.fallback_backend.as_ref(),
        ]
        .into_iter()
        .flatten()
        .try_fold(0_u128, |bytes, value| {
            bytes.checked_add(value.capacity() as u128)
        });

        assert_eq!(report.checked_retained_capacity_bytes(), expected);
    }
}
