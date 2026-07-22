pub mod area_component;
pub mod area_decomposition;
pub mod area_multiset_feasibility;
pub mod area_tileability;
pub mod scenario_area_pruner;

pub use area_component::{AreaComponent, RegionKind};
pub use area_decomposition::{AreaDecomposer, AreaDecomposition, AreaScope};
pub use area_multiset_feasibility::{AreaMultisetError, AreaMultisetFeasibility};
pub use area_tileability::{
    AreaTileabilityError, AreaTileabilityFailure, AreaTileabilityReport, AreaTileabilityRules,
};
pub use scenario_area_pruner::{AreaPruneDecision, ScenarioAreaPruner};
