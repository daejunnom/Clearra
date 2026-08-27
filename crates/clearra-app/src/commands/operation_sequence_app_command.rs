use clearra_core_executor::order_language::{
    operation_sequence::{
        NormalizedOperationSequenceStep, OperationSequenceAnalyzer, OperationSequenceError,
    },
    sequence_dependencies::OperationDocumentProblem,
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{bool_field, number_field, string_field},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSequenceAppCommand {
    problem: OperationDocumentProblem,
}

impl OperationSequenceAppCommand {
    pub fn new(problem: OperationDocumentProblem) -> Self {
        Self { problem }
    }

    pub fn problem(&self) -> &OperationDocumentProblem {
        &self.problem
    }
}

impl RunnableAppCommand for OperationSequenceAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report =
            match OperationSequenceAnalyzer::analyze(&self.problem, context.execution_control) {
                Ok(report) => report,
                Err(error) => return error_response(error),
            };
        let replay_evidence = report
            .steps
            .iter()
            .map(replay_evidence_record)
            .collect::<Vec<_>>()
            .join(";");
        let fields = vec![
            string_field("contract_id", "operation-sequence.v1"),
            bool_field("complete", true),
            number_field("width", report.width),
            number_field("height", report.height),
            string_field("initial_board", format!("{:016x}", report.initial_board)),
            string_field("final_board", format!("{:016x}", report.final_board)),
            number_field("operation_count", report.steps.len()),
            number_field("cleared_line_count", report.cleared_line_count),
            string_field("trace_key", format!("{:016x}", report.trace_key)),
            string_field("rule_profile", report.rule_profile.as_str()),
            string_field("kick_profile", report.kick_profile.as_str()),
            string_field("normalized_trace", report.canonical_trace()),
            string_field("replay_evidence", replay_evidence),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(
            AppResultKind::Sequence,
            fields,
        )))
    }
}

fn replay_evidence_record(step: &NormalizedOperationSequenceStep) -> String {
    format!(
        "{}:{:016x}:{:016x}:{:016x}:{:016x}:{}:{}",
        step.operation_id.0,
        step.board_before,
        step.lock_mask,
        step.board_after,
        step.cleared_row_mask,
        step.visited_state_count,
        u8::from(step.used_first_success_kick),
    )
}

fn error_response(error: OperationSequenceError) -> AppResponse {
    let (code, status) = match error {
        OperationSequenceError::InvalidInput(_) => (
            AppErrorCode::OperationSequenceInvalid,
            AppStatus::ValidationFailed,
        ),
        OperationSequenceError::Cancelled => (
            AppErrorCode::OperationSequenceCancelled,
            AppStatus::ExecutionFailed,
        ),
        OperationSequenceError::TimedOut { .. } => (
            AppErrorCode::OperationSequenceTimedOut,
            AppStatus::ExecutionFailed,
        ),
        OperationSequenceError::Incomplete { .. } => (
            AppErrorCode::OperationSequenceIncomplete,
            AppStatus::ExecutionFailed,
        ),
    };
    AppResponse::failed(status, AppError::new(code, error.to_string()))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        operation::operation::OperationId,
        piece::{piece_kind::PieceKind, rotation::RotationState},
    };
    use clearra_core_executor::order_language::sequence_dependencies::ConcreteDocumentOperation;

    use super::*;

    #[test]
    fn app_report_exposes_operation_sequence_contract() {
        let operation = ConcreteDocumentOperation::from_centered(
            OperationId(0),
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )
        .unwrap();
        let mut problem = OperationDocumentProblem::canonical(10, 4, 0, vec![operation]);
        problem.document_boards = Some(vec![0]);
        let response = crate::AppContext::default().run(crate::AppRequest::new(
            crate::AppCommand::UtilitySequence(OperationSequenceAppCommand::new(problem)),
        ));
        assert_eq!(response.status(), AppStatus::Success);
        assert_eq!(
            response.render_model().unwrap().kind(),
            AppResultKind::Sequence
        );
        let fields = response.render_model().unwrap().message().unwrap().fields();
        assert!(fields.iter().any(|field| field.key() == "contract_id"
            && field.value().as_text() == "operation-sequence.v1"));
    }
}
