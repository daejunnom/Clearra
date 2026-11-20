use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::PcScenarioQuery;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostPcContinuationStatus {
    available: bool,
    complete: bool,
}

impl PostPcContinuationStatus {
    pub(crate) fn from_query_result(
        query: &PcScenarioQuery,
        result: &CoreExecutionResult,
        remaining_queue_len: Option<usize>,
    ) -> Self {
        let available = result.solution_found()
            && remaining_queue_len
                .is_some_and(|remaining| remaining >= query.min_remaining_queue());
        Self {
            available,
            complete: available
                || (!result.solution_found() && result.bool_field("count_complete") == Some(true)),
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
