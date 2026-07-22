pub mod gpu_backend_capability;
pub mod gpu_backend_error;
pub mod gpu_backend_kind;
pub mod gpu_backend_selection;
pub mod gpu_candidate_hash;
pub mod gpu_coverage_bitset_or_helper;
pub mod gpu_cpu_confirm_bridge;
pub mod gpu_cpu_exact_confirm_optimizer;
pub mod gpu_device_selector;
pub mod gpu_dominance_prefilter;
pub mod gpu_fence_epoch;
pub mod gpu_larger_batch_planner;
pub mod gpu_memory_ticket;
pub mod gpu_packing_candidate;
pub mod gpu_packing_strengthening;
pub mod gpu_readback_compression;
pub mod gpu_worker_autotune;
pub mod gpu_worker_backend_report;
pub mod gpu_worker_backpressure;
pub mod gpu_worker_batch_sizer;
pub mod gpu_worker_budget;
pub mod gpu_worker_build_result_bridge;
pub mod gpu_worker_coverage_bridge;
pub mod gpu_worker_error;
pub mod gpu_worker_exactness_gate;
pub mod gpu_worker_memory_pressure;
pub mod gpu_worker_metrics;
pub mod gpu_worker_product_report;
pub mod gpu_worker_request;
pub mod gpu_worker_result;
pub mod gpu_worker_result_reducer;
pub mod gpu_worker_result_validation;
pub mod gpu_worker_state;
pub mod gpu_worker_submission;
pub mod packing_batch_descriptor;
pub mod packing_batch_descriptor_builder;
pub mod packing_batch_from_candidate_region;
pub mod packing_batch_from_problem;
pub mod packing_batch_id;
pub mod packing_batch_source;
pub mod packing_batch_source_error;
pub mod packing_batch_validation;

pub use gpu_backend_capability::GpuBackendCapability;
pub use gpu_backend_error::GpuBackendError;
pub use gpu_backend_kind::GpuBackendKind;
pub use gpu_backend_selection::GpuBackendSelection;
pub use gpu_candidate_hash::GpuCandidateHash;
pub use gpu_coverage_bitset_or_helper::{GpuCoverageBitsetOrError, GpuCoverageBitsetOrHelper};
pub use gpu_cpu_confirm_bridge::{
    GpuCpuConfirmBridge, GpuCpuConfirmBridgeDecision, GpuCpuConfirmBridgeError,
};
pub use gpu_cpu_exact_confirm_optimizer::{GpuCpuExactConfirmOptimizer, GpuCpuExactConfirmReport};
pub use gpu_device_selector::GpuDeviceSelector;
pub use gpu_dominance_prefilter::{GpuDominancePrefilter, GpuDominancePrefilterReport};
pub use gpu_fence_epoch::GpuFenceEpoch;
pub use gpu_larger_batch_planner::{GpuLargerBatchPlan, GpuLargerBatchPlanner};
pub use gpu_memory_ticket::GpuMemoryTicket;
pub use gpu_packing_candidate::GpuPackingCandidate;
pub use gpu_packing_strengthening::{GpuPackingStrengthening, GpuPackingStrengtheningReport};
pub use gpu_readback_compression::{GpuReadbackCompression, GpuReadbackCompressionError};
pub use gpu_worker_autotune::{GpuWorkerAutotune, GpuWorkerAutotuneDecision};
pub use gpu_worker_backend_report::GpuWorkerBackendReport;
pub use gpu_worker_backpressure::GpuWorkerBackpressure;
pub use gpu_worker_batch_sizer::{GpuWorkerBatchSizeDecision, GpuWorkerBatchSizer};
pub use gpu_worker_budget::GpuWorkerBudget;
pub use gpu_worker_build_result_bridge::{
    GpuWorkerBuildResultBridge, GpuWorkerBuildResultBridgeError, GpuWorkerBuildUpMode,
};
pub use gpu_worker_coverage_bridge::{
    GpuWorkerBuildVariantCoverageInput, GpuWorkerCoverageBridge, GpuWorkerCoverageBridgeError,
    GpuWorkerCoverageBridgeReport,
};
pub use gpu_worker_error::GpuWorkerError;
pub use gpu_worker_exactness_gate::GpuWorkerExactnessGate;
pub use gpu_worker_memory_pressure::{GpuWorkerMemoryPressure, GpuWorkerMemoryPressureLevel};
pub use gpu_worker_metrics::GpuWorkerMetrics;
pub use gpu_worker_product_report::GpuWorkerProductReport;
pub use gpu_worker_request::GpuWorkerRequest;
pub use gpu_worker_result::{GpuExecutionCompletion, GpuWorkerResult};
pub use gpu_worker_result_reducer::{GpuWorkerReduction, GpuWorkerResultReducer};
pub use gpu_worker_result_validation::{
    unavailable_reason_label, validate_gpu_worker_result, GpuWorkerResultValidationError,
};
pub use gpu_worker_state::GpuWorkerState;
pub use gpu_worker_submission::{GpuWorkerSubmission, GpuWorkerSubmissionStatus};
pub use packing_batch_descriptor::PackingBatchDescriptor;
pub use packing_batch_descriptor_builder::PackingBatchDescriptorBuilder;
pub use packing_batch_id::PackingBatchId;
pub use packing_batch_source::PackingBatchSource;
pub use packing_batch_source_error::PackingBatchSourceError;
pub use packing_batch_validation::PackingBatchValidationError;

#[cfg(test)]
mod gpu_worker_batch_descriptor_contract_tests;
#[cfg(test)]
mod gpu_worker_contract_tests;
#[cfg(test)]
mod gpu_worker_external_pc_contract_tests;
#[cfg(test)]
mod gpu_worker_packing_backend_contract_tests;
#[cfg(test)]
mod gpu_worker_product_path_tests;
