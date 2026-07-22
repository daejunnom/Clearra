pub use clearra_core_domain::ids::{ScoreObjectiveCellId, SpinTargetId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageRowKind {
    Pc,
    Setup,
    Build,
    SpinTarget(SpinTargetId),
    ScoreCell(ScoreObjectiveCellId),
}
