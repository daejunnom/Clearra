#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObjectiveKind {
    #[default]
    All,
    Unique,
    MinimumCover,
    Tiling,
}
