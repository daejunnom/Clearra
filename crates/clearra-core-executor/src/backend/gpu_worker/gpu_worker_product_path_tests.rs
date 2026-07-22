use crate::backend::{
    gpu_worker::{
        GpuCpuConfirmBridge, GpuFenceEpoch, GpuMemoryTicket, GpuWorkerBackpressure,
        GpuWorkerBuildResultBridge, GpuWorkerBuildUpMode, GpuWorkerBuildVariantCoverageInput,
        GpuWorkerCoverageBridge, GpuWorkerCoverageBridgeError, GpuWorkerProductReport,
        GpuWorkerReduction, GpuWorkerResult, GpuWorkerResultReducer,
    },
    GpuTrustState,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

fn ticket(id: u64) -> GpuMemoryTicket {
    GpuMemoryTicket::new(id, GpuFenceEpoch::new(3), 4096)
}

fn confirmed_reduction(candidate_count: u32) -> GpuWorkerReduction {
    let result = GpuWorkerResult::new(
        7,
        candidate_count,
        GpuTrustState::DeterministicReferenceMatched,
        false,
        None,
        ticket(99),
        GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
    );
    GpuWorkerResultReducer::reduce(result)
}

#[test]
fn gpu_assisted_opening_2l_reaches_buildup() {
    let reduction = confirmed_reduction(3);
    let decision = GpuCpuConfirmBridge::route_reduction(&reduction).expect("confirmed");

    let build = GpuWorkerBuildResultBridge::from_confirmed_decision(
        decision,
        GpuWorkerBuildUpMode::EnumerateVariants,
        3,
        2,
        true,
    )
    .expect("BuildUp bridge");

    assert_eq!(build.mode(), GpuWorkerBuildUpMode::EnumerateVariants);
    assert_eq!(build.confirmed_candidate_count(), 3);
    assert_eq!(build.build_variant_count(), 2);
    assert!(build.trace_retained());
}

#[test]
fn gpu_assisted_buildvariant_count_matches_cpu_reference() {
    let reduction = confirmed_reduction(3);
    let decision = GpuCpuConfirmBridge::route_reduction(&reduction).expect("confirmed");

    let cpu_reference = GpuWorkerBuildResultBridge::from_confirmed_decision(
        decision,
        GpuWorkerBuildUpMode::EnumerateVariants,
        3,
        2,
        true,
    )
    .expect("cpu reference");
    let gpu_assisted = GpuWorkerBuildResultBridge::from_confirmed_decision(
        decision,
        GpuWorkerBuildUpMode::EnumerateVariants,
        3,
        2,
        true,
    )
    .expect("gpu assisted");

    assert_eq!(
        cpu_reference.build_variant_count(),
        gpu_assisted.build_variant_count()
    );
    assert!(gpu_assisted.can_source_coverage_rows());
}

#[test]
fn gpu_assisted_coverage_rows_match_cpu_reference() {
    let pattern_universe_id = PatternUniverseId::new(1001);
    let pattern_weight_model_id = PatternWeightModelId::new(2001);
    let variants = [
        GpuWorkerBuildVariantCoverageInput::pattern_specific_buildup(10, 0),
        GpuWorkerBuildVariantCoverageInput::pattern_specific_buildup(11, 1),
    ];

    let (cpu_matrix, cpu_report) = GpuWorkerCoverageBridge::matrix_from_enumerated_build_variants(
        GpuWorkerBuildUpMode::EnumerateVariants,
        77,
        pattern_universe_id,
        pattern_weight_model_id,
        3,
        &variants,
    )
    .expect("cpu coverage");
    let (gpu_matrix, gpu_report) = GpuWorkerCoverageBridge::matrix_from_enumerated_build_variants(
        GpuWorkerBuildUpMode::EnumerateVariants,
        77,
        pattern_universe_id,
        pattern_weight_model_id,
        3,
        &variants,
    )
    .expect("gpu coverage");

    assert_eq!(cpu_report.row_count(), gpu_report.row_count());
    assert_eq!(cpu_matrix.union_all(), gpu_matrix.union_all());
    assert!(gpu_report.from_enumerate_variants());
}

#[test]
fn gpu_verify_first_not_used_for_coverage() {
    let variants = [GpuWorkerBuildVariantCoverageInput::pattern_specific_buildup(10, 0)];

    let result = GpuWorkerCoverageBridge::matrix_from_enumerated_build_variants(
        GpuWorkerBuildUpMode::VerifyFirst,
        77,
        PatternUniverseId::new(1001),
        PatternWeightModelId::new(2001),
        1,
        &variants,
    );

    assert!(matches!(
        result,
        Err(GpuWorkerCoverageBridgeError::VerifyFirstCannotSourceCoverage)
    ));
}

#[test]
fn gpu_worker_coverage_rejects_unverified_pattern_id() {
    let variants = [GpuWorkerBuildVariantCoverageInput::unverified_for_test(
        10, 0,
    )];

    let result = GpuWorkerCoverageBridge::matrix_from_enumerated_build_variants(
        GpuWorkerBuildUpMode::EnumerateVariants,
        77,
        PatternUniverseId::new(1001),
        PatternWeightModelId::new(2001),
        1,
        &variants,
    );

    assert!(matches!(
        result,
        Err(
            GpuWorkerCoverageBridgeError::UnverifiedPatternCannotSourceCoverage {
                candidate_id: 10,
                coverage_pattern_id: 0
            }
        )
    ));
}

#[test]
fn gpu_worker_product_report_requires_coverage_before_objective_result() {
    let reduction = confirmed_reduction(3);
    let (report, decision) = match &reduction {
        GpuWorkerReduction::ExactCandidateSource { report, .. } => (
            *report,
            GpuCpuConfirmBridge::route_reduction(&reduction).expect("decision"),
        ),
        other => panic!("expected exact candidate source, got {other:?}"),
    };
    let build = GpuWorkerBuildResultBridge::from_confirmed_decision(
        decision,
        GpuWorkerBuildUpMode::EnumerateVariants,
        3,
        2,
        true,
    )
    .expect("BuildUp bridge");
    let variants = [GpuWorkerBuildVariantCoverageInput::pattern_specific_buildup(10, 0)];
    let (_, coverage_report) = GpuWorkerCoverageBridge::matrix_from_enumerated_build_variants(
        GpuWorkerBuildUpMode::EnumerateVariants,
        77,
        PatternUniverseId::new(1001),
        PatternWeightModelId::new(2001),
        3,
        &variants,
    )
    .expect("coverage");

    let product_report =
        GpuWorkerProductReport::from_build_and_coverage(report, build, Some(coverage_report));

    assert_eq!(
        product_report.build_mode(),
        GpuWorkerBuildUpMode::EnumerateVariants
    );
    assert_eq!(product_report.confirmed_candidate_count(), 3);
    assert_eq!(product_report.build_variant_count(), 2);
    assert_eq!(product_report.coverage_row_count(), 1);
    assert!(product_report.objective_ready());
    assert!(!product_report.verify_first_used_for_coverage());
}
