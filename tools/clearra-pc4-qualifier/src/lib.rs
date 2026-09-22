//! Reusable, local-only data-product generation boundaries.
//!
//! Product search never links these routines into a hot path. The native CLI
//! may expose them only through an explicit `legal-board generate` request.

// `domain.rs` is also compiled by the full qualification binary.  The small
// reusable legal-board library intentionally consumes only the forward/legal
// derivations, so the binary-only validation helpers are dead in this crate
// compilation unit.
mod conditioned_reachability_generation;
#[allow(dead_code)]
mod domain;
mod legal_board_generation;

pub use conditioned_reachability_generation::{
    generate_conditioned_reachability, ConditionedReachabilityGenerationOptions,
};
pub use legal_board_generation::{generate_legal_board, LegalBoardGenerationOptions};
