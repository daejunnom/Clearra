use super::*;
use crate::backend::HybridThrottleReason;

#[test]
fn larger_batch_planner_selects_large_batch_under_low_pressure() {
    let plan = GpuLargerBatchPlanner::plan(
        GpuWorkerBudget::default_local(),
        GpuWorkerMetrics::default(),
        512,
    );

    assert!(plan.larger_batch_planner());
    assert_eq!(plan.planned_batch_size(), 256);
    assert!(plan.dominance_prefilter_enabled());
    assert!(plan.readback_compression_enabled());
    assert!(plan.cpu_exact_confirm_optimization_enabled());
}

#[test]
fn readback_compression_preserves_candidates() {
    let candidates = sample_candidates();
    let compressed = GpuReadbackCompression::compress(&candidates);
    let decompressed = GpuReadbackCompression::decompress(&compressed).expect("decompressed");

    assert_eq!(decompressed, candidates);
}

#[test]
fn dominance_prefilter_does_not_drop_required_candidate() {
    let mut candidates = sample_candidates();
    candidates.push(candidates[0].clone());
    let report = GpuDominancePrefilter::retain_required_and_deduplicate_optional(&candidates);

    assert!(!report.required_candidate_dropped());
    assert!(report
        .retained_candidates()
        .iter()
        .any(GpuPackingCandidate::required_candidate));
}

#[test]
fn dominance_prefilter_preserves_distinct_optional_candidates() {
    let candidates = vec![
        GpuPackingCandidate::new(1, 10, 20, 30, 40, 0b0001, false),
        GpuPackingCandidate::new(2, 10, 20, 30, 41, 0b0001, false),
    ];
    let report = GpuDominancePrefilter::retain_required_and_deduplicate_optional(&candidates);

    assert_eq!(report.retained_candidates().len(), 2);
    assert_eq!(report.dropped_optional_count(), 0);
}

#[test]
fn gpu_result_deterministic_cpu_confirmed_and_reference_matched() {
    let candidates = sample_candidates();
    let report =
        GpuCpuExactConfirmOptimizer::confirm_against_cpu_reference(&candidates, &candidates);

    assert!(report.gpu_result_deterministic());
    assert!(report.gpu_result_cpu_confirmed());
    assert!(report.cpu_reference_and_gpu_result_match());
    assert!(report.hash_exact_confirmed());
    assert!(!report.hash_only_success());
}

#[test]
fn gpu_candidate_hash_only_is_not_success() {
    let gpu = sample_candidates();
    let cpu = vec![GpuPackingCandidate::new(9, 1, 2, 999, 4, 0b0001, true)];
    let report = GpuCpuExactConfirmOptimizer::confirm_against_cpu_reference(&gpu, &cpu);

    assert!(!report.gpu_result_cpu_confirmed());
    assert!(!report.cpu_reference_and_gpu_result_match());
    assert!(!report.hash_only_success());
}

#[test]
fn unconfirmed_gpu_coverage_cannot_source_probability() {
    let mismatched =
        GpuCpuExactConfirmOptimizer::confirm_against_cpu_reference(&sample_candidates(), &[]);

    assert_eq!(
        GpuCoverageBitsetOrHelper::union_confirmed(&mismatched),
        Err(super::super::gpu_coverage_bitset_or_helper::GpuCoverageBitsetOrError::CpuConfirmRequired)
    );
}

#[test]
fn coverage_bitset_or_helper_uses_confirmed_candidates() {
    let candidates = sample_candidates();
    let confirm =
        GpuCpuExactConfirmOptimizer::confirm_against_cpu_reference(&candidates, &candidates);

    assert_eq!(
        GpuCoverageBitsetOrHelper::union_confirmed(&confirm).expect("coverage union"),
        0b0111
    );
}

#[test]
fn fallback_reason_visible() {
    let report = GpuPackingStrengthening::evaluate(
        GpuWorkerBudget::default_local(),
        GpuWorkerMetrics {
            gpu_readback_pending: 3,
            ..GpuWorkerMetrics::default()
        },
        512,
        &sample_candidates(),
        &sample_candidates(),
        Some(SearchBackendFallbackReason::GpuFeatureDisabled),
    );

    assert!(report.gpu_result_deterministic());
    assert!(report.gpu_result_cpu_confirmed());
    assert!(report.cpu_reference_and_gpu_result_match());
    assert!(report.readback_compression_preserves_candidates());
    assert!(report.dominance_prefilter_does_not_drop_required_candidate());
    assert_eq!(
        report
            .fallback_reason()
            .map(SearchBackendFallbackReason::as_str),
        Some("gpu_feature_disabled")
    );
    assert_eq!(
        report.autotune().throttle_reason(),
        HybridThrottleReason::ReadbackPending
    );
}

fn sample_candidates() -> Vec<GpuPackingCandidate> {
    vec![
        GpuPackingCandidate::new(1, 10, 20, 30, 40, 0b0001, true),
        GpuPackingCandidate::new(2, 11, 21, 31, 41, 0b0110, false),
    ]
}
