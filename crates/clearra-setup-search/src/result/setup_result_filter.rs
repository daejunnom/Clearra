use crate::{query::SetupProbabilityFilter, result::setup_result::SetupResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupResultFilter {
    probability_filter: SetupProbabilityFilter,
}

impl SetupResultFilter {
    pub fn new(probability_filter: SetupProbabilityFilter) -> Self {
        Self { probability_filter }
    }
}
impl SetupResultFilter {
    pub fn accepts(self, result: &SetupResult) -> bool {
        self.probability_filter.accepts(result.probability())
    }
}
