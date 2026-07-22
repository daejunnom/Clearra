pub mod area_model;
pub mod area_multiset_feasibility;
pub mod area_scope;
pub mod standard_tetromino_area_rule;

pub use area_model::AreaModel;
pub use area_multiset_feasibility::{
    area_multiset_feasibility_uses_piece_area_multiset, AreaFeasibilityDecision,
    AreaMultisetFeasibility, AreaMultisetFeasibilityError,
};
pub use area_scope::{
    scenario_area_pruner_requires_explicit_area_scope, AreaScopeDescriptor, AreaScopeError,
};
pub use standard_tetromino_area_rule::{
    standard_area4_fast_path_unchanged, StandardTetrominoAreaRule,
};
