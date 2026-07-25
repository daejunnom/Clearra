pub mod cover_service;
pub mod pc_backend_report_adapter;
pub mod pc_checkpoint_metadata;
pub mod pc_continuation_fields;
pub mod pc_output_model_builder;
pub mod pc_pipeline_fields;
pub mod pc_policy_labels;
pub mod pc_service;
#[cfg(test)]
mod pc_service_tests;
pub mod pc_summary_builder;
pub mod percent_service;

pub use cover_service::{CoverService, CoverServiceError};
pub use pc_service::{PcService, PcServiceError};
pub use percent_service::{PercentService, PercentServiceError};

pub(crate) fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}
