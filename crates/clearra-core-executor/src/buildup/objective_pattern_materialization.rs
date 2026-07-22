use crate::buildup::{
    objective_incomplete_reason::ObjectiveIncompleteReason,
    objective_pattern_inputs::ObjectivePatternInputs,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ObjectivePatternMaterialization {
    Ready(ObjectivePatternInputs),
    Incomplete(ObjectiveIncompleteReason),
}
