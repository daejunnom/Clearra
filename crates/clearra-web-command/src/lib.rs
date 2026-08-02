//! Browser command text to typed Clearra AppRequest.
//!
//! This crate intentionally does not reuse native CLI path/process semantics.

mod ctk3_mask_input;
mod sfinder_compat;
pub mod web_command_error;
pub mod web_command_parser;
pub mod web_command_request;
pub mod web_pc_scenario_input;
pub mod web_virtual_file;

pub use web_command_error::{WebCommandError, WebCommandErrorCode};
pub use web_command_parser::WebCommandParser;
pub use web_command_request::WebCommandRequest;
pub use web_pc_scenario_input::WebPcScenarioInput;
pub use web_virtual_file::WebVirtualFileHandle;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
mod web_build_probability_input;
pub use web_build_probability_input::WebBuildProbabilityInput;
