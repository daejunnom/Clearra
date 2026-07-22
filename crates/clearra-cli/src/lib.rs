pub mod args;
pub mod assemble;
mod cli_entry;
mod cli_routing;
pub mod commands;
pub mod error;
pub mod exit;
pub mod fixture;
mod input;
pub mod output;
mod rules;
mod scoring;

pub use cli_entry::{run, run_with_args};

#[cfg(all(test, feature = "native-c-core"))]
#[path = "../tests/product_cli_surface_contract.rs"]
mod product_cli_surface_contract;
#[cfg(all(test, feature = "native-c-core"))]
#[path = "../tests/product_contract_e2e.rs"]
mod product_contract_e2e;
#[cfg(all(test, feature = "native-c-core"))]
#[path = "../tests/product_golden_t4_contract.rs"]
mod product_golden_t4_contract;
