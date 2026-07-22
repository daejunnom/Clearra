use clearra_problem::SearchProblem;

use crate::supply::{CompactSupplyDescriptors, SupplyDescriptorCompiler};

use super::FfiProblemError;

pub(crate) fn supply_descriptor(
    problem: &SearchProblem,
) -> Result<CompactSupplyDescriptors, FfiProblemError> {
    SupplyDescriptorCompiler::compile(problem)
}
