//! Canonical Clearra CLI command text to typed [`clearra_app::AppRequest`].
//!
//! Native CLI, WASM, and desktop adapters share this frontend-neutral grammar
//! and request lowering. Native path and process semantics remain outside this
//! crate so browser and GUI hosts cannot accidentally acquire process authority.

mod ctk3_mask_input;
mod operation_document;
mod sfinder_compat;
mod web_build_v2_input;
pub mod web_command_error;
pub mod web_command_parser;
pub mod web_command_request;
pub mod web_pc_scenario_input;
mod web_setup_score_input;
pub mod web_virtual_file;

pub use operation_document::{
    operation_sequence_request_from_document, sequence_dependencies_request_from_document,
};
pub use web_build_v2_input::{WebBuildV2Capability, WebBuildV2Input};
pub use web_command_error::{
    WebCommandError as CliCommandError, WebCommandErrorCode as CliCommandErrorCode,
};
pub use web_command_error::{WebCommandError, WebCommandErrorCode};
pub use web_command_parser::WebCommandParser as CliCommandParser;
pub use web_command_parser::{WebCommandParser, WebCompatibilityAuthority};
pub use web_command_request::WebCommandRequest as CliCommandRequest;
pub use web_command_request::WebCommandRequest;
pub use web_pc_scenario_input::WebPcScenarioInput;
pub use web_setup_score_input::{WebSetupScoreInput, WebSetupScoreQueueInput};
pub use web_virtual_file::WebVirtualFileHandle;

#[cfg(test)]
mod build_v2_ingress_tests;
#[cfg(test)]
mod queue_parser_contract_tests;
#[cfg(test)]
mod search_option_contract_tests;
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
mod web_build_probability_input;
pub use web_build_probability_input::WebBuildProbabilityInput;
