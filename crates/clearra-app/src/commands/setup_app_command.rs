use clearra_core_executor::CoreExecutionResult;
use clearra_problem::{SetupCandidatePriority, SetupSearchQuery};
use clearra_validation::validators::setup_query_validator::validate_setup_search_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_response::AppResponse,
    commands::execution_error_response::core_execution_error_response,
    ranked_family_product_projection::project_setup_ranked_family,
    render::{AppRenderModel, SetupRenderModel},
    setup_ranking_contract::{SetupRankingContract, SetupRankingKind},
    setup_ranking_facade::SetupRankingFacade,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupAppCommand {
    query: SetupSearchQuery,
}

impl SetupAppCommand {
    pub fn new(query: SetupSearchQuery) -> Self {
        Self { query }
    }
}
impl SetupAppCommand {
    pub fn query(&self) -> &SetupSearchQuery {
        &self.query
    }

    pub fn into_query(self) -> SetupSearchQuery {
        self.query
    }
}

impl RunnableAppCommand for SetupAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_setup_search_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        match context
            .services()
            .core_executor()
            .execute_setup_with_workers_and_control(
                &self.query,
                usize::from(context.resource_budget().workers()).max(1),
                context.execution_control(),
            ) {
            Ok(result) => setup_success_response(&self.query, result),
            Err(error) => core_execution_error_response(error),
        }
    }
}

/// Shared terminal promotion for direct, distributed, and cooperative Setup
/// execution. Path-detail requests retain the legacy unranked shape; every
/// family request must preserve a validated ranking snapshot or fail closed.
pub(crate) fn setup_success_response(
    query: &SetupSearchQuery,
    result: CoreExecutionResult,
) -> AppResponse {
    if query.path_detail().is_some() {
        return AppResponse::success(AppRenderModel::Setup(SetupRenderModel::unranked(result)));
    }
    let kind = match query.candidate_priority() {
        SetupCandidatePriority::All => SetupRankingKind::Joint,
        SetupCandidatePriority::BuildProbabilityFirst => SetupRankingKind::Build,
        SetupCandidatePriority::PcProbabilityFirst => SetupRankingKind::ConditionalPc,
    };
    let contract = match SetupRankingContract::bind(kind, query) {
        Ok(contract) => contract,
        Err(error) => {
            return ranked_family_rejection(format!(
                "setup ranked-family contract rejected: {error:?}"
            ))
        }
    };
    match SetupRankingFacade::promote(contract, query, result) {
        Ok(result) => {
            let (core_result, snapshot) = result.into_core_result_and_snapshot();
            let payload = match project_setup_ranked_family(&snapshot) {
                Ok(payload) => payload,
                Err(error) => {
                    return ranked_family_rejection(format!(
                        "setup ranked-family Host projection rejected: {error:?}"
                    ))
                }
            };
            AppResponse::success(AppRenderModel::Setup(SetupRenderModel::ranked(
                core_result,
                snapshot,
            )))
            .with_public_product_result(payload, None)
        }
        Err(error) => {
            ranked_family_rejection(format!("setup ranked-family result rejected: {error:?}"))
        }
    }
}

fn ranked_family_rejection(message: String) -> AppResponse {
    AppResponse::failed(
        crate::app_response::AppStatus::ExecutionFailed,
        crate::app_error::AppError::new(crate::app_error::AppErrorCode::ExecutionFailed, message),
    )
}

#[cfg(test)]
mod tests {
    use clearra_problem::SetupCandidatePriority;

    use super::setup_success_response;
    use crate::{app_response::AppStatus, setup_ranked_fixture};

    #[test]
    fn shared_setup_completion_preserves_all_three_ranked_family_contracts() {
        for (priority, capability_id, result_schema) in [
            (
                SetupCandidatePriority::All,
                "setup.joint",
                "setup-joint-ranking.v2",
            ),
            (
                SetupCandidatePriority::BuildProbabilityFirst,
                "setup.build",
                "setup-build-ranking.v2",
            ),
            (
                SetupCandidatePriority::PcProbabilityFirst,
                "setup.pc",
                "setup-pc-ranking.v2",
            ),
        ] {
            let query = setup_ranked_fixture::query(priority);
            let response =
                setup_success_response(&query, setup_ranked_fixture::core_result(&query));
            assert_eq!(response.status(), AppStatus::Success);
            let render_model = response.render_model().expect("Setup render model");
            let snapshot = render_model
                .setup_ranked_family_snapshot()
                .expect("validated Setup ranked-family snapshot");
            assert_eq!(snapshot.capability_id(), capability_id);
            assert_eq!(snapshot.result_schema(), result_schema);
            assert_eq!(snapshot.candidate_count(), 1);
            assert_eq!(snapshot.identities().query_sha256().len(), 64);
            assert_eq!(snapshot.identities().supply_sha256().len(), 64);
            assert_eq!(snapshot.identities().universe_sha256().len(), 64);
            assert_eq!(
                render_model
                    .core_result()
                    .and_then(clearra_core_executor::CoreExecutionResult::setup_finder_report)
                    .expect("Setup report")
                    .hold_conditions()
                    .len(),
                1
            );
        }
    }
}
