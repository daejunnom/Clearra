pub mod search_goal_request;
pub mod spin_target_goal;

pub use search_goal_request::{BuildTemplateGoal, CompositeGoal, SearchGoal};
pub use spin_target_goal::{
    spin_target_requires_score_profile, RequiredClearKind, RequiredClearLines, RequiredSpinKind,
    SpinMiniPolicy, SpinPieceSelector, SpinTargetRequest,
};
