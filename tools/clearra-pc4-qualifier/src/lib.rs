//! Reusable, local-only data-product generation boundaries.
//!
//! Product search never links these routines into a hot path. The native CLI
//! exposes them only through explicit legal-board or reachability-pack
//! candidate-generation requests.

// `domain.rs` is also compiled by the full qualification binary.  The small
// reusable legal-board library intentionally consumes only the forward/legal
// derivations, so the binary-only validation helpers are dead in this crate
// compilation unit.
mod conditioned_local_relation_generation;
mod conditioned_reachability_generation;
#[allow(dead_code)]
mod domain;
mod legal_board_generation;

pub use conditioned_local_relation_generation::{
    generate_conditioned_local_relation, structurally_valid_conditioned_local_candidate,
    ConditionedLocalRelationGenerationOptions,
};
pub use conditioned_reachability_generation::{
    generate_conditioned_reachability, ConditionedReachabilityGenerationOptions,
};
pub use legal_board_generation::{generate_legal_board, LegalBoardGenerationOptions};
