use clearra_core_executor::order_language::{
    build_order_language::OperationDependencyEdge,
    sequence_dependencies::{
        OperationDocumentProblem, SequenceDependenciesAnalyzer, SequenceDependenciesError,
    },
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{bool_field, number_field, string_field},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

const PUBLIC_EDGE_PREVIEW_LIMIT: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceDependenciesAppCommand {
    problem: OperationDocumentProblem,
}

impl SequenceDependenciesAppCommand {
    pub fn new(problem: OperationDocumentProblem) -> Self {
        Self { problem }
    }
    pub fn problem(&self) -> &OperationDocumentProblem {
        &self.problem
    }
}

impl RunnableAppCommand for SequenceDependenciesAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report =
            match SequenceDependenciesAnalyzer::analyze(&self.problem, context.execution_control) {
                Ok(report) => report,
                Err(error) => return error_response(error),
            };
        let graph = &report.language.dependency_constraints;
        let closure = graph.universal_precedence_closure();
        let reduction = graph.transitive_reduction();
        let representative = report.representative_order.as_ref().map_or_else(
            || "none".to_owned(),
            |order| {
                order
                    .iter()
                    .map(|operation| operation.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let reachability = report
            .reachability_evidence
            .iter()
            .map(|evidence| {
                format!(
                    "{}:{}:{}:{}",
                    evidence.operation_id.0,
                    evidence.visited_state_count,
                    evidence.used_first_success_kick as u8,
                    evidence.line_clear_adjusted as u8
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let fields = vec![
            string_field("contract_id", "operation-dependency-report.v1"),
            bool_field("complete", graph.complete()),
            number_field("candidate_id", report.language.candidate_id.0),
            string_field(
                "operation_set_key",
                format!("{:016x}", report.language.operation_set_key.0),
            ),
            number_field("operation_count", graph.operation_ids().len()),
            string_field("exact_order_count", graph.exact_order_count().to_string()),
            string_field("solution_count", graph.exact_order_count().to_string()),
            number_field("universal_dependency_count", closure.len()),
            number_field("transitive_reduction_count", reduction.len()),
            number_field("independent_pair_count", graph.independent_pair_count()),
            number_field("explored_state_count", graph.explored_state_count()),
            number_field("live_transition_count", graph.live_transition_count()),
            string_field("rule_profile", report.rule_profile.as_str()),
            string_field("kick_profile", report.kick_profile.as_str()),
            string_field("representative_order", representative),
            string_field(
                "universal_dependencies_preview",
                edge_list(closure, PUBLIC_EDGE_PREVIEW_LIMIT),
            ),
            bool_field(
                "universal_dependencies_preview_truncated",
                closure.len() > PUBLIC_EDGE_PREVIEW_LIMIT,
            ),
            string_field("transitive_reduction", edge_list(reduction, usize::MAX)),
            string_field("reachability_evidence", reachability),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(
            AppResultKind::SequenceDependencies,
            fields,
        )))
    }
}

fn edge_list(edges: &[OperationDependencyEdge], limit: usize) -> String {
    edges
        .iter()
        .take(limit)
        .map(|edge| format!("{}>{}", edge.predecessor.0, edge.successor.0))
        .collect::<Vec<_>>()
        .join(",")
}

fn error_response(error: SequenceDependenciesError) -> AppResponse {
    let (code, status) = match error {
        SequenceDependenciesError::InvalidInput(_) | SequenceDependenciesError::Language(_) => (
            AppErrorCode::SequenceDependenciesInvalid,
            AppStatus::ValidationFailed,
        ),
        SequenceDependenciesError::Cancelled => (
            AppErrorCode::SequenceDependenciesCancelled,
            AppStatus::ExecutionFailed,
        ),
        SequenceDependenciesError::TimedOut { .. } => (
            AppErrorCode::SequenceDependenciesTimedOut,
            AppStatus::ExecutionFailed,
        ),
        SequenceDependenciesError::Incomplete { .. } => (
            AppErrorCode::SequenceDependenciesIncomplete,
            AppStatus::ExecutionFailed,
        ),
    };
    AppResponse::failed(status, AppError::new(code, error.to_string()))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{operation::operation::OperationId, piece::piece_kind::PieceKind};
    use clearra_core_executor::order_language::sequence_dependencies::ConcreteDocumentOperation;

    use super::*;

    #[test]
    fn app_report_exposes_exact_contract() {
        let problem = OperationDocumentProblem::canonical(
            10,
            4,
            0,
            vec![ConcreteDocumentOperation::from_centered(
                OperationId(0),
                PieceKind::O,
                clearra_core_domain::piece::rotation::RotationState::Zero,
                0,
                0,
            )
            .unwrap()],
        );
        let response = crate::AppContext::default().run(crate::AppRequest::new(
            crate::AppCommand::UtilitySequenceDependencies(SequenceDependenciesAppCommand::new(
                problem,
            )),
        ));
        assert_eq!(response.status(), AppStatus::Success);
        assert_eq!(
            response.render_model().unwrap().kind(),
            AppResultKind::SequenceDependencies
        );
    }
}
