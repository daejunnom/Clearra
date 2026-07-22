use clearra_problem::SearchProblem;

use crate::rules::RuleDescriptorCompiler;

use super::{CRuleProfileDescriptor, FfiProblemError};

pub(crate) fn rule_descriptor(
    problem: &SearchProblem,
) -> Result<CRuleProfileDescriptor, FfiProblemError> {
    RuleDescriptorCompiler::compile(problem)
}
