mod setup_candidate_enumerator;
mod setup_core_buildup_gate;
mod setup_coverage_plan;
mod setup_family_grouper;
mod setup_pattern_source;
mod setup_post_pc_adapter;
pub mod setup_search_service;
mod setup_shape_packer;
mod setup_summary_builder;

pub use setup_search_service::{
    SetupSearchExecutionError, SetupSearchExecutionResult, SetupSearchService,
};
