use clearra_core_ffi::{CPackingProblem, CPackingProblemBuilder, FfiProblemError};
use clearra_problem::SearchProblem;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackingProblemLoweringError {
    Ffi(FfiProblemError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackingProblemLowering;

impl PackingProblemLowering {
    pub fn lower(problem: &SearchProblem) -> Result<CPackingProblem, PackingProblemLoweringError> {
        CPackingProblemBuilder::from_search_problem(problem)
            .map_err(PackingProblemLoweringError::Ffi)
    }
}

#[cfg(test)]
#[path = "packing_problem_lowering_tests.rs"]
mod tests;
