pub mod hold_automaton_descriptor;
pub mod piece_source_descriptor;
pub mod supply_descriptor_compiler;

pub use hold_automaton_descriptor::{
    CHoldAutomatonStateDescriptor, HoldAutomatonDescriptorCompiler,
    C_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT, C_HOLD_TRANSITION_SWAP_HELD,
    C_HOLD_TRANSITION_USE_CURRENT,
};
pub use piece_source_descriptor::{
    CPieceSourceDescriptor, PieceSourceDescriptorCompiler, PieceSourceDescriptorError,
    C_PIECE_SOURCE_BAG_UNIVERSE, C_PIECE_SOURCE_FIXED_QUEUE,
    C_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE, C_PIECE_SOURCE_OBSERVED_WINDOW,
    C_SUPPLY_TRUNCATION_MATERIALIZED_PATTERN_BUDGET_EXCEEDED, C_SUPPLY_TRUNCATION_NONE,
    C_SUPPLY_TRUNCATION_OBSERVED_WINDOW_BUDGET_EXCEEDED,
};
pub use supply_descriptor_compiler::{CompactSupplyDescriptors, SupplyDescriptorCompiler};
