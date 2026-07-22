#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TracePolicy {
    #[default]
    Keep,
    Discard,
}
