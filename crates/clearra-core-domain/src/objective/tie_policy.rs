#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TiePolicy {
    #[default]
    StableInputOrder,
    LowestCandidateId,
}
