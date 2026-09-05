// Preserve the public `hold_automaton::hold_automaton` domain path.
#[allow(clippy::module_inception)]
pub mod hold_automaton;

pub use hold_automaton::{
    HoldAutomatonMemoKey, HoldAutomatonState, HoldAutomatonStep, HoldTransition,
    HoldTransitionError, SupplyProvenanceId,
};
