use crate::dropdown::DropdownOption;

use super::{
    backend_preset_schema::BackendPresetSchema, execution_options_schema::ExecutionOptionsSchema,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendOptionsSchema {
    options: Vec<DropdownOption>,
    presets: Vec<BackendPresetSchema>,
    result_contract_fields: Vec<String>,
}

impl BackendOptionsSchema {
    pub fn m28() -> Self {
        let execution = ExecutionOptionsSchema::mvp2();
        Self::from_execution_options(&execution)
    }
}
impl BackendOptionsSchema {
    pub fn from_execution_options(execution: &ExecutionOptionsSchema) -> Self {
        Self {
            options: execution.backend_options().to_vec(),
            presets: execution.backend_presets().to_vec(),
            result_contract_fields: backend_result_contract_fields(),
        }
    }
}
impl BackendOptionsSchema {
    pub fn options(&self) -> &[DropdownOption] {
        &self.options
    }
}
impl BackendOptionsSchema {
    pub fn presets(&self) -> &[BackendPresetSchema] {
        &self.presets
    }
}
impl BackendOptionsSchema {
    pub fn result_contract_fields(&self) -> &[String] {
        &self.result_contract_fields
    }
}

fn backend_result_contract_fields() -> Vec<String> {
    [
        "backend_requested",
        "backend_selected",
        "backend_fallback_reason",
        "backend_selection_reason",
        "candidate_backend",
        "buildup_backend",
        "gpu_confirmed",
        "cpu_confirmed",
        "gpu_status",
        "gpu_trust_state",
        "gpu_unavailable_reason",
        "gpu_larger_batch_planner",
        "gpu_dominance_prefilter",
        "gpu_shape_union_mask",
        "gpu_candidate_hash",
        "gpu_readback_compression",
        "gpu_result_deterministic",
        "gpu_result_cpu_confirmed",
        "gpu_cpu_reference_match",
        "hybrid_candidate_queue_len",
        "hybrid_candidate_queue_capacity",
        "hybrid_cpu_worker_backlog",
        "hybrid_gpu_readback_backlog",
        "hybrid_gpu_batch_in_flight",
        "hybrid_backpressure_active",
        "hybrid_deferred_batch_count",
        "hybrid_truncated_batch_count",
        "hybrid_memory_pressure_level",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_options_schema_exposes_m28_backend_contract() {
        let schema = BackendOptionsSchema::m28();
        let options = schema
            .options()
            .iter()
            .map(DropdownOption::value)
            .collect::<Vec<_>>();

        assert_eq!(options, ["auto", "cpu", "gpu", "hybrid"]);
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "backend_fallback_reason"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "candidate_backend"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "buildup_backend"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "gpu_result_deterministic"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "gpu_result_cpu_confirmed"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "gpu_status"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "gpu_trust_state"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "hybrid_backpressure_active"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "hybrid_memory_pressure_level"));
    }
}
