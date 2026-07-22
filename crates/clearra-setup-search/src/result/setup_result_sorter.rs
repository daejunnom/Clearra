use crate::result::setup_result::SetupResult;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupResultSorter;

impl SetupResultSorter {
    pub fn sort_by_probability_desc(results: &mut [SetupResult]) {
        results.sort_by(|left, right| {
            right
                .probability()
                .partial_cmp(&left.probability())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.family_id().cmp(&right.family_id()))
        });
    }
}
