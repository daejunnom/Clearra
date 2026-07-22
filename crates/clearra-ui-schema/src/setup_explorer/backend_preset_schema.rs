use clearra_i18n::TranslationKey;
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{dropdown::DropdownOption, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendPresetSchema {
    id: String,
    label: String,
    localized_label: LocalizedLabelSchema,
    description: String,
    localized_description: LocalizedLabelSchema,
    enabled: bool,
    disabled_reason_code: Option<String>,
    disabled_diagnostic_code: Option<DiagnosticCode>,
    requires_feature: Option<String>,
    supports_trace_enumeration: bool,
    supports_total_count: bool,
    supports_sample_traces: bool,
    supports_deterministic: bool,
    supports_frontier_budget: bool,
}

impl BackendPresetSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BackendPresetSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl BackendPresetSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl BackendPresetSchema {
    pub fn description(&self) -> &str {
        &self.description
    }
}
impl BackendPresetSchema {
    pub fn localized_description(&self) -> &LocalizedLabelSchema {
        &self.localized_description
    }
}
impl BackendPresetSchema {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl BackendPresetSchema {
    pub fn disabled_reason_code(&self) -> Option<&str> {
        self.disabled_reason_code.as_deref()
    }
}
impl BackendPresetSchema {
    pub fn disabled_diagnostic_code(&self) -> Option<DiagnosticCode> {
        self.disabled_diagnostic_code
    }
}
impl BackendPresetSchema {
    pub fn requires_feature(&self) -> Option<&str> {
        self.requires_feature.as_deref()
    }
}
impl BackendPresetSchema {
    pub fn supports_trace_enumeration(&self) -> bool {
        self.supports_trace_enumeration
    }
}
impl BackendPresetSchema {
    pub fn supports_total_count(&self) -> bool {
        self.supports_total_count
    }
}
impl BackendPresetSchema {
    pub fn supports_sample_traces(&self) -> bool {
        self.supports_sample_traces
    }
}
impl BackendPresetSchema {
    pub fn supports_deterministic(&self) -> bool {
        self.supports_deterministic
    }
}
impl BackendPresetSchema {
    pub fn supports_frontier_budget(&self) -> bool {
        self.supports_frontier_budget
    }
}
impl BackendPresetSchema {
    pub(crate) fn option(&self) -> DropdownOption {
        let option = DropdownOption::new(&self.id, &self.label)
            .with_localized_label(self.localized_label.clone());
        match (
            self.disabled_diagnostic_code,
            self.disabled_reason_code.as_deref(),
        ) {
            (Some(code), Some(reason)) => option.disabled_for(code, reason),
            _ => option,
        }
    }
}

pub(crate) fn backend_presets() -> Vec<BackendPresetSchema> {
    [
        RequestedSearchBackend::Auto,
        RequestedSearchBackend::Cpu,
        RequestedSearchBackend::Gpu,
        RequestedSearchBackend::Hybrid,
    ]
    .into_iter()
    .map(backend_preset)
    .collect()
}

pub(crate) fn backend_options(presets: &[BackendPresetSchema]) -> Vec<DropdownOption> {
    presets.iter().map(BackendPresetSchema::option).collect()
}

fn backend_preset(backend: RequestedSearchBackend) -> BackendPresetSchema {
    let capability = backend_preset_capability(backend);
    BackendPresetSchema {
        id: backend.as_str().to_owned(),
        label: backend_preset_label(backend).to_owned(),
        localized_label: LocalizedLabelSchema::new(
            TranslationKey::ui_backend_label(backend.as_str()),
            backend_preset_label(backend),
        ),
        description: capability.description.to_owned(),
        localized_description: LocalizedLabelSchema::new(
            TranslationKey::ui_backend_description(backend.as_str()),
            capability.description,
        ),
        enabled: capability.enabled,
        disabled_reason_code: capability.disabled_reason_code.map(ToOwned::to_owned),
        disabled_diagnostic_code: capability.disabled_diagnostic_code,
        requires_feature: capability.requires_feature.map(ToOwned::to_owned),
        supports_trace_enumeration: capability.supports_trace_enumeration,
        supports_total_count: capability.supports_total_count,
        supports_sample_traces: capability.supports_sample_traces,
        supports_deterministic: true,
        supports_frontier_budget: capability.supports_frontier_budget,
    }
}

struct BackendPresetCapability {
    description: &'static str,
    enabled: bool,
    disabled_reason_code: Option<&'static str>,
    disabled_diagnostic_code: Option<DiagnosticCode>,
    requires_feature: Option<&'static str>,
    supports_trace_enumeration: bool,
    supports_total_count: bool,
    supports_sample_traces: bool,
    supports_frontier_budget: bool,
}

fn backend_preset_capability(backend: RequestedSearchBackend) -> BackendPresetCapability {
    match backend {
        RequestedSearchBackend::Auto => BackendPresetCapability {
            description: "Selects the safest available backend for the query.",
            enabled: true,
            disabled_reason_code: None,
            disabled_diagnostic_code: None,
            requires_feature: None,
            supports_trace_enumeration: true,
            supports_total_count: true,
            supports_sample_traces: true,
            supports_frontier_budget: false,
        },
        RequestedSearchBackend::Cpu => BackendPresetCapability {
            description: "Exact layered BFS on CPU with automatic deterministic parallelism.",
            enabled: true,
            disabled_reason_code: None,
            disabled_diagnostic_code: None,
            requires_feature: None,
            supports_trace_enumeration: true,
            supports_total_count: true,
            supports_sample_traces: true,
            supports_frontier_budget: true,
        },
        RequestedSearchBackend::Gpu => BackendPresetCapability {
            description: "Uses the connected exact GPU backend; runtime capability and fallback policy decide availability.",
            enabled: true,
            disabled_reason_code: None,
            disabled_diagnostic_code: None,
            requires_feature: None,
            supports_trace_enumeration: false,
            supports_total_count: true,
            supports_sample_traces: true,
            supports_frontier_budget: true,
        },
        RequestedSearchBackend::Hybrid => BackendPresetCapability {
            description: "Uses GPU-only packing when the GPU is prepared and CPU-only packing otherwise; results are never merged.",
            enabled: true,
            disabled_reason_code: None,
            disabled_diagnostic_code: None,
            requires_feature: None,
            supports_trace_enumeration: false,
            supports_total_count: true,
            supports_sample_traces: true,
            supports_frontier_budget: true,
        },
    }
}

fn backend_preset_label(backend: RequestedSearchBackend) -> &'static str {
    match backend {
        RequestedSearchBackend::Auto => "Auto",
        RequestedSearchBackend::Cpu => "CPU",
        RequestedSearchBackend::Gpu => "GPU",
        RequestedSearchBackend::Hybrid => "Hybrid",
    }
}
