use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    build_solution_probability_result::build_v2_facade::{
        BuildCongruentCoverV1Request, BuildCongruentV1Request, BuildCoverV2Request,
        BuildEvaluateB2bCoverV1Request, BuildEvaluateCoverPercentV1Request,
        BuildEvaluateCoverV1Request, BuildEvaluateMinimalsV1Request, BuildEvaluateScoreV1Request,
        BuildSetupCoverPercentV1Request, BuildSetupCoverScoreV1Request, BuildSetupCoverV1Request,
        BuildSetupV1Request, BuildV2RequestProfileQuery,
    },
    build_v2_product_projection::{
        project_build_congruent_cover_v1, project_build_congruent_v1,
        project_build_setup_cover_percent_v1, project_build_setup_cover_score_v1,
        project_build_setup_cover_v1, project_build_supplied_cover_percent_v1,
        project_build_supplied_coverage_v1, project_build_supplied_minimum_cover_v1,
        project_build_supplied_score_v1, BuildV2ProductProjectionError, ProjectedBuildV2Product,
    },
    render::{AppMessage, AppRenderModel, AppResultKind},
};
use clearra_host_contract::{ExecutionCompletenessState, ResourceReport};

#[derive(Clone, Debug, PartialEq)]
pub enum BuildV2AppRequest {
    BuildCover(BuildCoverV2Request),
    BuildSetup(BuildSetupV1Request),
    BuildCongruent(BuildCongruentV1Request),
    BuildCongruentCover(BuildCongruentCoverV1Request),
    BuildSetupCover(BuildSetupCoverV1Request),
    BuildSetupCoverPercent(BuildSetupCoverPercentV1Request),
    BuildSetupCoverScore(BuildSetupCoverScoreV1Request),
    BuildEvaluateCover(BuildEvaluateCoverV1Request),
    BuildEvaluateMinimals(BuildEvaluateMinimalsV1Request),
    BuildEvaluateScore(BuildEvaluateScoreV1Request),
    BuildEvaluateB2bCover(BuildEvaluateB2bCoverV1Request),
    BuildEvaluateCoverPercent(BuildEvaluateCoverPercentV1Request),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildV2AppCommand {
    request: BuildV2AppRequest,
}

impl BuildV2AppCommand {
    pub fn build_cover(request: BuildCoverV2Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildCover(request),
        }
    }

    pub fn build_setup(request: BuildSetupV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildSetup(request),
        }
    }

    pub fn build_congruent(request: BuildCongruentV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildCongruent(request),
        }
    }

    pub fn build_congruent_cover(request: BuildCongruentCoverV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildCongruentCover(request),
        }
    }

    pub fn build_setup_cover(request: BuildSetupCoverV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildSetupCover(request),
        }
    }

    pub fn build_setup_cover_percent(request: BuildSetupCoverPercentV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildSetupCoverPercent(request),
        }
    }

    pub fn build_setup_cover_score(request: BuildSetupCoverScoreV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildSetupCoverScore(request),
        }
    }

    pub fn build_evaluate_cover(request: BuildEvaluateCoverV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildEvaluateCover(request),
        }
    }

    pub fn build_evaluate_minimals(request: BuildEvaluateMinimalsV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildEvaluateMinimals(request),
        }
    }

    pub fn build_evaluate_score(request: BuildEvaluateScoreV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildEvaluateScore(request),
        }
    }

    pub fn build_evaluate_b2b_cover(request: BuildEvaluateB2bCoverV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildEvaluateB2bCover(request),
        }
    }

    pub fn build_evaluate_cover_percent(request: BuildEvaluateCoverPercentV1Request) -> Self {
        Self {
            request: BuildV2AppRequest::BuildEvaluateCoverPercent(request),
        }
    }

    pub const fn request(&self) -> &BuildV2AppRequest {
        &self.request
    }

    pub(crate) fn into_request(self) -> BuildV2AppRequest {
        self.request
    }

    pub(crate) fn request_profile_query(&self) -> &clearra_problem::BuildProbabilityQuery {
        match &self.request {
            BuildV2AppRequest::BuildCover(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildSetup(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildCongruent(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildCongruentCover(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildSetupCover(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildSetupCoverPercent(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildSetupCoverScore(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildEvaluateCover(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildEvaluateMinimals(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildEvaluateScore(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildEvaluateB2bCover(request) => request.request_profile_query(),
            BuildV2AppRequest::BuildEvaluateCoverPercent(request) => {
                request.request_profile_query()
            }
        }
    }
}

impl RunnableAppCommand for BuildV2AppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        match self.request {
            BuildV2AppRequest::BuildSetup(request) => match request.execute(
                context.services().core_executor(),
                context.execution_control(),
            ) {
                Ok(report) => AppResponse::success(AppRenderModel::Verify(AppMessage::new(
                    AppResultKind::Verify,
                    Vec::new(),
                )))
                .with_build_setup_v1(report)
                .map(with_complete_build_product_resources)
                .unwrap_or_else(|error| {
                    AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            format!("Build v2 result rejected: {error}"),
                        ),
                    )
                }),
                Err(error) => AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::ExecutionFailed,
                        format!("Build v2 execution failed: {error:?}"),
                    ),
                ),
            },
            BuildV2AppRequest::BuildCover(request) => match request.execute(
                context.services().core_executor(),
                context.execution_control(),
            ) {
                Ok(report) => build_cover_success_response(report),
                Err(error) => AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::ExecutionFailed,
                        format!("Build v2 execution failed: {error:?}"),
                    ),
                ),
            },
            BuildV2AppRequest::BuildCongruent(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_congruent_v1,
            ),
            BuildV2AppRequest::BuildCongruentCover(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_congruent_cover_v1,
            ),
            BuildV2AppRequest::BuildSetupCover(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_setup_cover_v1,
            ),
            BuildV2AppRequest::BuildSetupCoverPercent(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_setup_cover_percent_v1,
            ),
            BuildV2AppRequest::BuildSetupCoverScore(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_setup_cover_score_v1,
            ),
            BuildV2AppRequest::BuildEvaluateCover(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_supplied_coverage_v1,
            ),
            BuildV2AppRequest::BuildEvaluateMinimals(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_supplied_minimum_cover_v1,
            ),
            BuildV2AppRequest::BuildEvaluateScore(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_supplied_score_v1,
            ),
            BuildV2AppRequest::BuildEvaluateB2bCover(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_supplied_coverage_v1,
            ),
            BuildV2AppRequest::BuildEvaluateCoverPercent(request) => projected_build_response(
                request.execute(
                    context.services().core_executor(),
                    context.execution_control(),
                ),
                project_build_supplied_cover_percent_v1,
            ),
        }
    }
}

/// Both direct CLI completion and the cooperative host seal the same nominal
/// Build portfolio through this response projection.
pub(crate) fn build_cover_success_response(
    report: crate::build_solution_probability_result::build_v2_facade::BuildCoveragePortfolioV2,
) -> AppResponse {
    AppResponse::success(AppRenderModel::Verify(AppMessage::new(
        AppResultKind::Verify,
        Vec::new(),
    )))
    .with_build_coverage_portfolio_v2(report)
    .map(with_complete_build_product_resources)
    .unwrap_or_else(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("Build v2 result rejected: {error}"),
            ),
        )
    })
}

fn projected_build_response<Report, ExecutionError>(
    execution: Result<Report, ExecutionError>,
    projection: fn(&Report) -> Result<ProjectedBuildV2Product, BuildV2ProductProjectionError>,
) -> AppResponse
where
    ExecutionError: core::fmt::Debug,
{
    match execution {
        Ok(report) => match projection(&report) {
            Ok(projected) => {
                let (payload, page_source_owner) = projected.into_parts();
                with_complete_build_product_resources(
                    AppResponse::success(AppRenderModel::Verify(AppMessage::new(
                        AppResultKind::Verify,
                        Vec::new(),
                    )))
                    .with_public_product_result(payload, page_source_owner)
                    .with_contract_context(clearra_host_contract::AppCommandKind::BuildProbability),
                )
            }
            Err(error) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("Build v2 result rejected: {error:?}"),
                ),
            ),
        },
        Err(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("Build v2 execution failed: {error:?}"),
            ),
        ),
    }
}

/// Build v2 result constructors admit only execution-backed, complete,
/// non-truncated evidence. The compact public DTO intentionally drops the
/// producer-private `CoreExecutionResult`; this report preserves the validated
/// lifecycle facts without inventing discarded high-water counters.
fn with_complete_build_product_resources(response: AppResponse) -> AppResponse {
    let mut report = ResourceReport::new(true, "reported");
    report.probability_complete = true;
    report.set_result_completeness(ExecutionCompletenessState::Complete);
    response.with_resource_report(report)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_host_contract::{
        BuildV2PayloadKind, ProductResultPayloadContent, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::BuildV2AppCommand;
    use crate::{
        build_solution_probability_result::{
            build_probability_resource_test_guard,
            build_v2_facade::{
                BuildColoredTargetSetV1, BuildCongruentCoverV1Request, BuildCongruentV1Request,
                BuildCoverV2Request, BuildEvaluateB2bCoverV1Request,
                BuildEvaluateCoverPercentV1Request, BuildEvaluateCoverV1Request,
                BuildEvaluateMinimalsV1Request, BuildEvaluateScoreV1Request, BuildObjective,
                BuildScoreProfile, BuildSetupCoverPercentV1Request, BuildSetupCoverScoreV1Request,
                BuildSetupCoverV1Request, BuildSetupV1Request, BuildSuppliedSolutionSetV1,
            },
        },
        AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppServices, AppStatus,
        ProductPageSourceOwner,
    };

    fn one_piece_query() -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("canonical target");
        BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include)
    }

    fn colored_identity(piece_index: usize, cells: u64) -> StandardBoard64ColoredTilingIdentity {
        let mut piece_masks = [0_u64; 7];
        piece_masks[piece_index] = cells;
        StandardBoard64ColoredTilingIdentity::from_piece_masks(0, piece_masks)
            .expect("four colored cells")
    }

    fn colored_target(label: &str) -> BuildColoredTargetSetV1 {
        BuildColoredTargetSetV1::new(
            4,
            2,
            label,
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask target candidates")
    }

    fn supplied(label: &str) -> BuildSuppliedSolutionSetV1 {
        BuildSuppliedSolutionSetV1::new(
            4,
            2,
            label,
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask supplied candidates")
    }

    #[test]
    fn every_remaining_build_v2_command_executes_into_the_closed_host_payload() {
        let _resource_guard = build_probability_resource_test_guard();
        let app = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let cases = vec![
            (
                BuildV2AppCommand::build_congruent(
                    BuildCongruentV1Request::new(
                        one_piece_query(),
                        colored_target("kat:congruent"),
                        BuildObjective::All,
                    )
                    .expect("congruent request"),
                ),
                "build.congruent",
                "build-congruence-family.v1",
                BuildV2PayloadKind::CandidateFamily,
                None,
            ),
            (
                BuildV2AppCommand::build_congruent_cover(
                    BuildCongruentCoverV1Request::new(
                        one_piece_query(),
                        colored_target("kat:congruent-cover"),
                        BuildObjective::MinCover,
                    )
                    .expect("congruent-cover request"),
                ),
                "build.congruent-cover",
                "build-congruence-coverage.v1",
                BuildV2PayloadKind::Portfolio,
                None,
            ),
            (
                BuildV2AppCommand::build_setup_cover(
                    BuildSetupCoverV1Request::new(
                        one_piece_query(),
                        colored_target("kat:setup-cover"),
                        BuildObjective::MaxProbabilityMinimum,
                    )
                    .expect("setup-cover request"),
                ),
                "build.setup-cover",
                "build-setup-cover.v1",
                BuildV2PayloadKind::Portfolio,
                None,
            ),
            (
                BuildV2AppCommand::build_setup_cover_percent(
                    BuildSetupCoverPercentV1Request::new(
                        one_piece_query(),
                        colored_target("kat:setup-cover-percent"),
                        BuildObjective::Unique,
                    )
                    .expect("setup-cover-percent request"),
                ),
                "build.setup-cover-percent",
                "build-setup-cover-probability.v1",
                BuildV2PayloadKind::Probability,
                None,
            ),
            (
                BuildV2AppCommand::build_setup_cover_score(
                    BuildSetupCoverScoreV1Request::new(
                        one_piece_query(),
                        colored_target("kat:setup-cover-score"),
                        BuildScoreProfile::Guideline,
                        u16::MAX,
                    )
                    .expect("setup-cover-score request"),
                ),
                "build.setup-cover-score",
                "build-setup-cover-score.v1",
                BuildV2PayloadKind::ScorePortfolio,
                None,
            ),
            (
                BuildV2AppCommand::build_evaluate_cover(
                    BuildEvaluateCoverV1Request::new(
                        one_piece_query(),
                        supplied("kat:evaluate-cover"),
                    )
                    .expect("evaluate-cover request"),
                ),
                "build.evaluate.cover",
                "build-supplied-coverage.v1",
                BuildV2PayloadKind::CandidateFamily,
                Some(false),
            ),
            (
                BuildV2AppCommand::build_evaluate_minimals(
                    BuildEvaluateMinimalsV1Request::new(
                        one_piece_query(),
                        supplied("kat:evaluate-minimals"),
                    )
                    .expect("evaluate-minimals request"),
                ),
                "build.evaluate.minimals",
                "build-supplied-minimum-cover.v1",
                BuildV2PayloadKind::Portfolio,
                None,
            ),
            (
                BuildV2AppCommand::build_evaluate_score(
                    BuildEvaluateScoreV1Request::new(
                        one_piece_query(),
                        supplied("kat:evaluate-score"),
                        BuildScoreProfile::Tetrio,
                        0,
                    )
                    .expect("evaluate-score request"),
                ),
                "build.evaluate.score",
                "build-supplied-score.v1",
                BuildV2PayloadKind::ScorePortfolio,
                None,
            ),
            (
                BuildV2AppCommand::build_evaluate_b2b_cover(
                    BuildEvaluateB2bCoverV1Request::new(
                        one_piece_query(),
                        supplied("kat:evaluate-b2b-cover"),
                    )
                    .expect("evaluate-b2b-cover request"),
                ),
                "build.evaluate.b2b-cover",
                "build-supplied-b2b-coverage.v1",
                BuildV2PayloadKind::CandidateFamily,
                Some(true),
            ),
            (
                BuildV2AppCommand::build_evaluate_cover_percent(
                    BuildEvaluateCoverPercentV1Request::new(
                        one_piece_query(),
                        supplied("kat:evaluate-cover-percent"),
                    )
                    .expect("evaluate-cover-percent request"),
                ),
                "build.evaluate.cover-percent",
                "build-supplied-probability.v1",
                BuildV2PayloadKind::Probability,
                None,
            ),
        ];

        for (command, capability_id, result_contract, kind, expected_b2b) in cases {
            let response = app.run(AppRequest::new(AppCommand::BuildV2(command)));
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "{capability_id}: {:?}",
                response.error()
            );
            let public_payload = response
                .public_result_payload()
                .expect("validated public product payload");
            assert_eq!(public_payload.contract(), capability_id);
            assert_eq!(public_payload.result_kind(), result_contract);
            assert_eq!(
                response.public_page_source_owner().is_some(),
                matches!(
                    kind,
                    BuildV2PayloadKind::Portfolio | BuildV2PayloadKind::ScorePortfolio
                ),
                "{capability_id} page ownership",
            );
            if let Some(ProductPageSourceOwner::CoveragePortfolio(owner)) =
                response.public_page_source_owner()
            {
                assert_eq!(owner.canonical_page().portfolio().candidate_ids(), &[1]);
            }

            let host = response.to_host_response();
            let ProductResultPayloadContent::BuildV2(payload) = host
                .product_result_payload()
                .expect("closed Build v2 Host payload")
                .content()
            else {
                panic!("{capability_id} must use BuildV2 payload");
            };
            assert_eq!(payload.kind(), kind, "{capability_id}");
            assert_eq!(payload.capability_id(), capability_id);
            assert_eq!(payload.result_contract(), result_contract);
            assert_eq!(payload.b2b_preservation_required(), expected_b2b);
            assert!(payload.completeness().replay_complete());
            if kind == BuildV2PayloadKind::ScorePortfolio {
                assert_eq!(payload.score_equality_basis(), Some("score-only"));
                assert_eq!(
                    payload.informational_attack_basis(),
                    Some("canonical-equal-score-trace")
                );
                assert!(payload.completeness().score_portfolio_complete());
                assert!(!payload.winners().is_empty());
            }

            let artifact_host = response.to_host_response_with_solution_set_artifact(Some(
                HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
            ));
            let artifact = artifact_host.solution_set_artifact();
            let portfolio = matches!(
                kind,
                BuildV2PayloadKind::Portfolio | BuildV2PayloadKind::ScorePortfolio
            );
            assert_eq!(artifact.is_some(), portfolio, "{capability_id} artifact");
            if let Some(artifact) = artifact {
                assert_eq!(artifact.source_result_kind(), result_contract);
                assert_eq!(artifact.selection_kind(), "portfolio-alternative");
                assert_eq!(artifact.selection_id(), "1");
                assert_eq!(
                    artifact.normalized_key_algorithm(),
                    "clearra-colored-field-key-v1"
                );
                assert_eq!(artifact.solution_count(), 1);
                assert!(artifact.page_source_identity_sha256().is_some());
                assert!(artifact.formats().iter().all(|format| format.available()));
                let ctk3 = artifact
                    .formats()
                    .iter()
                    .find(|format| format.format() == "ctk3")
                    .and_then(|format| format.document())
                    .expect("Build portfolio CTK3 sidecar");
                assert_eq!(
                    clearra_output::decode_ctk3_exact(ctk3)
                        .expect("decode Build sidecar")
                        .pages
                        .len(),
                    1
                );
            }
        }
    }

    #[test]
    fn build_cover_and_setup_attach_complete_native_solution_sidecars() {
        let _resource_guard = build_probability_resource_test_guard();
        let app = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let cases = [
            (
                BuildV2AppCommand::build_cover(
                    BuildCoverV2Request::new(one_piece_query(), BuildObjective::MinCover)
                        .expect("cover request"),
                ),
                "build-coverage-portfolio.v2",
                "portfolio-alternative",
                "portfolio-member-normalized-tiling-key.v1",
            ),
            (
                BuildV2AppCommand::build_setup(
                    BuildSetupV1Request::new(
                        one_piece_query(),
                        colored_target("kat:setup"),
                        BuildObjective::Unique,
                    )
                    .expect("setup request"),
                ),
                "build-target-family.v2",
                "solution-family",
                "clearra-colored-field-key-v1",
            ),
        ];

        for (command, result_contract, selection_kind, key_algorithm) in cases {
            let response = app.run(AppRequest::new(AppCommand::BuildV2(command)));
            assert_eq!(response.status(), AppStatus::Success, "{result_contract}");
            assert!(response.resource_report().solver_executed());
            assert_eq!(
                response.resource_report().result_completeness().as_str(),
                "complete"
            );
            let host = response.to_host_response_with_solution_set_artifact(Some(
                HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
            ));
            let artifact = host
                .solution_set_artifact()
                .expect("complete Build solution sidecar");
            assert_eq!(artifact.source_result_kind(), result_contract);
            assert_eq!(artifact.selection_kind(), selection_kind);
            assert_eq!(artifact.normalized_key_algorithm(), key_algorithm);
            assert!(artifact.solution_count() >= 1);
            assert!(artifact.formats().iter().all(|format| format.available()));
        }
    }
}
