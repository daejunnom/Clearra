use crate::build_solution_probability_result::build_v2_facade::BuildSetupV1;
use clearra_host_contract::{
    BuildSetupCandidateCoverageV1Payload, BuildSetupCompletenessPayload,
    BuildSetupFamilyPayloadError, BuildSetupFamilyV1Payload,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildSetupProductProjectionError {
    CandidateRejected(BuildSetupFamilyPayloadError),
    FamilyRejected(BuildSetupFamilyPayloadError),
}

pub fn project_build_setup_v1(
    report: &BuildSetupV1,
) -> Result<BuildSetupFamilyV1Payload, BuildSetupProductProjectionError> {
    let candidates = report
        .candidates()
        .iter()
        .map(|candidate| {
            BuildSetupCandidateCoverageV1Payload::try_new(
                candidate.candidate_key(),
                candidate.covered_pattern_count().to_string(),
            )
            .map_err(BuildSetupProductProjectionError::CandidateRejected)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = report.completeness();
    BuildSetupFamilyV1Payload::try_new(
        report.contract_id(),
        report.input_identity_sha256(),
        report.evaluation_identity_sha256(),
        report.objective().as_str(),
        report.source_candidate_count().to_string(),
        report.reachable_candidate_count().to_string(),
        report.pattern_count().to_string(),
        report.covered_pattern_count().to_string(),
        report.union_probability(),
        BuildSetupCompletenessPayload::new(
            evidence.input_identity_bound(),
            evidence.producer_filter_bound(),
            evidence.buildability_replay_complete(),
            evidence.coverage_rows_complete(),
            evidence.probability_weights_complete(),
        ),
        candidates,
    )
    .map_err(BuildSetupProductProjectionError::FamilyRejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_solution_probability_result::{
            build_probability_resource_test_guard,
            build_v2_facade::{BuildColoredTargetSetV1, BuildObjective, BuildSetupV1Request},
        },
        AppCoreExecutorService,
    };
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    fn query() -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        BuildProbabilityQuery::new(
            core,
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0]).unwrap(),
        )
        .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include)
    }
    fn identity(index: usize) -> StandardBoard64ColoredTilingIdentity {
        let mut masks = [0; 7];
        masks[index] = 0xf;
        StandardBoard64ColoredTilingIdentity::from_piece_masks(0, masks).unwrap()
    }
    #[test]
    fn projection_signature_is_closed_over_actual_setup_report() {
        let projection: fn(
            &BuildSetupV1,
        )
            -> Result<BuildSetupFamilyV1Payload, BuildSetupProductProjectionError> =
            project_build_setup_v1;
        let _ = projection;
    }

    #[test]
    fn actual_setup_report_projects_fieldwise_without_page_or_tie_shape() {
        let _guard = build_probability_resource_test_guard();
        let target =
            BuildColoredTargetSetV1::new(4, 2, "setup-projection", [identity(0), identity(1)])
                .unwrap();
        let report = BuildSetupV1Request::new(query(), target, BuildObjective::Unique)
            .unwrap()
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            )
            .unwrap();
        let payload = project_build_setup_v1(&report).unwrap();
        let expected_rows = report
            .candidates()
            .iter()
            .map(|row| {
                BuildSetupCandidateCoverageV1Payload::try_new(
                    row.candidate_key(),
                    row.covered_pattern_count().to_string(),
                )
                .unwrap()
            })
            .collect();
        let evidence = report.completeness();
        let expected = BuildSetupFamilyV1Payload::try_new(
            report.contract_id(),
            report.input_identity_sha256(),
            report.evaluation_identity_sha256(),
            report.objective().as_str(),
            report.source_candidate_count().to_string(),
            report.reachable_candidate_count().to_string(),
            report.pattern_count().to_string(),
            report.covered_pattern_count().to_string(),
            report.union_probability(),
            BuildSetupCompletenessPayload::new(
                evidence.input_identity_bound(),
                evidence.producer_filter_bound(),
                evidence.buildability_replay_complete(),
                evidence.coverage_rows_complete(),
                evidence.probability_weights_complete(),
            ),
            expected_rows,
        )
        .unwrap();
        assert_eq!(payload, expected);
        assert_eq!(payload.candidates().len(), report.candidates().len());
        for (actual, projected) in report.candidates().iter().zip(payload.candidates()) {
            assert_eq!(projected.candidate_key(), actual.candidate_key());
            assert_eq!(
                projected.covered_pattern_count(),
                actual.covered_pattern_count().to_string()
            );
        }
        assert!(payload.completeness().complete());
    }
}
