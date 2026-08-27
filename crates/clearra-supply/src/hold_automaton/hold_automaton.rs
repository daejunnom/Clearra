pub use crate::execution_automaton::{HoldTransition, SupplyProvenanceId};

pub type HoldAutomatonState = crate::execution_automaton::SupplyExecutionState;
pub type HoldAutomatonMemoKey = crate::execution_automaton::SupplyExecutionMemoKey;
pub type HoldAutomatonStep = crate::execution_automaton::SupplyExecutionStep;
pub type HoldTransitionError = crate::execution_automaton::SupplyExecutionError;
