use clearra_objectives::reducer::objective_reducer::ObjectiveReductionResult;

use crate::buildup::objective_incomplete_reason::ObjectiveIncompleteReason;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObjectiveReductionOutcome {
    result: Option<ObjectiveReductionResult>,
    complete: bool,
    incomplete_reason: Option<ObjectiveIncompleteReason>,
}

impl ObjectiveReductionOutcome {
    pub(crate) fn complete(result: Option<ObjectiveReductionResult>) -> Self {
        Self {
            result,
            complete: true,
            incomplete_reason: None,
        }
    }
}
impl ObjectiveReductionOutcome {
    pub(crate) fn incomplete(reason: ObjectiveIncompleteReason) -> Self {
        Self {
            result: None,
            complete: false,
            incomplete_reason: Some(reason),
        }
    }
}
impl ObjectiveReductionOutcome {
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<ObjectiveReductionResult>,
        bool,
        Option<ObjectiveIncompleteReason>,
    ) {
        (self.result, self.complete, self.incomplete_reason)
    }
}
