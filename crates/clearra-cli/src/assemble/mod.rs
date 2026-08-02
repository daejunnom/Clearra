pub mod app_request_assembler;
mod app_request_error_render;
mod app_request_format;
mod app_request_rules;
mod app_request_scenario_contract;
pub mod cover_query_assembler;
pub mod execution_policy_assembler;
pub mod pc_query_assembler;
mod pc_scenario_fixture_assembler;
mod pc_scenario_policy_assembler;
pub mod pc_scenario_query_assembler;
mod pc_scenario_supply_assembler;
mod pc_scenario_validation_material;
pub mod percent_query_assembler;
pub mod piece_sequence_assembler;
pub mod profile_assembler;
pub mod rule_profile_assembler;
pub mod setup_query_assembler;
mod setup_resource_budget;

pub(crate) use app_request_assembler::CliAppRequestAssembler;
pub use cover_query_assembler::CoverQueryAssembler;
pub use execution_policy_assembler::{ExecutionPolicyAssembler, ExecutionPolicyAssemblyError};
pub use pc_query_assembler::{PcQueryAssembler, PcQueryAssemblyError};
pub use pc_scenario_query_assembler::PcScenarioQueryAssembler;
pub use pc_scenario_validation_material::{PcScenarioAssembly, PcScenarioQueryAssemblyError};
pub use percent_query_assembler::{
    PercentQueryAssembler, PercentQueryAssembly, PercentQueryAssemblyError,
};
pub use piece_sequence_assembler::{PieceSequenceAssembler, PieceSequenceAssemblyError};
pub use profile_assembler::{CliProfileSet, ProfileAssembler};
pub use rule_profile_assembler::{RuleProfileAssembler, RuleProfileAssemblyError};
pub use setup_query_assembler::{SetupQueryAssembler, SetupQueryAssemblyError};
pub(crate) use setup_resource_budget::setup_resource_budget;

pub(super) fn parse_hex_mask(mask: &str) -> Result<u64, String> {
    let digits = mask
        .strip_prefix("0x")
        .ok_or_else(|| "initial_board_mask must start with 0x".to_owned())?;
    u64::from_str_radix(digits, 16).map_err(|error| error.to_string())
}
