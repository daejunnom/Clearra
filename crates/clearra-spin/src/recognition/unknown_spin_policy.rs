use crate::target::PredicateResult;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnknownSpinPolicy {
    #[default]
    PreserveUnknown,
    ExcludeAndMarkIncomplete,
}

impl UnknownSpinPolicy {
    pub const fn as_predicate(self) -> PredicateResult {
        match self {
            Self::PreserveUnknown | Self::ExcludeAndMarkIncomplete => PredicateResult::Unknown,
        }
    }
}
