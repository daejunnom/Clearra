use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::PcScenarioQuery;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostPcContinuationStatus {
    available: bool,
    complete: bool,
}

impl PostPcContinuationStatus {
    pub(crate) fn from_query_result(query: &PcScenarioQuery, result: &CoreExecutionResult) -> Self {
        let best_remaining_queue_len = result.usize_field("best_remaining_queue_len").unwrap_or(0);
        let available =
            result.solution_found() && best_remaining_queue_len >= query.min_remaining_queue();
        Self {
            available,
            complete: result.bool_field("count_complete").unwrap_or(false) || available,
        }
    }
}
impl PostPcContinuationStatus {
    pub(crate) fn available(self) -> bool {
        self.available
    }
}
impl PostPcContinuationStatus {
    pub(crate) fn complete(self) -> bool {
        self.complete
    }
}
