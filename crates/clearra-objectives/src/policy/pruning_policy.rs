#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PruningPolicy {
    #[default]
    None,
    FeasibilityOnly,
}
