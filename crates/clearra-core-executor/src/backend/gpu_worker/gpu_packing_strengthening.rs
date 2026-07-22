use crate::backend::SearchBackendFallbackReason;

use super::{
    GpuCandidateHash, GpuCoverageBitsetOrHelper, GpuCpuExactConfirmOptimizer,
    GpuDominancePrefilter, GpuLargerBatchPlan, GpuLargerBatchPlanner, GpuPackingCandidate,
    GpuReadbackCompression, GpuWorkerAutotune, GpuWorkerAutotuneDecision, GpuWorkerBudget,
    GpuWorkerMetrics,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPackingStrengtheningReport {
    larger_batch_plan: GpuLargerBatchPlan,
    gpu_candidate_hash: u64,
    compressed_readback_bytes: usize,
    readback_compression_preserves_candidates: bool,
    dominance_prefilter_does_not_drop_required_candidate: bool,
    gpu_result_deterministic: bool,
    gpu_result_cpu_confirmed: bool,
    cpu_reference_and_gpu_result_match: bool,
    coverage_bitset_or: u128,
    fallback_reason: Option<SearchBackendFallbackReason>,
    autotune: GpuWorkerAutotuneDecision,
}

impl GpuPackingStrengtheningReport {
    pub const fn larger_batch_plan(&self) -> GpuLargerBatchPlan {
        self.larger_batch_plan
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn gpu_candidate_hash(&self) -> u64 {
        self.gpu_candidate_hash
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn compressed_readback_bytes(&self) -> usize {
        self.compressed_readback_bytes
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn readback_compression_preserves_candidates(&self) -> bool {
        self.readback_compression_preserves_candidates
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn dominance_prefilter_does_not_drop_required_candidate(&self) -> bool {
        self.dominance_prefilter_does_not_drop_required_candidate
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn gpu_result_deterministic(&self) -> bool {
        self.gpu_result_deterministic
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn gpu_result_cpu_confirmed(&self) -> bool {
        self.gpu_result_cpu_confirmed
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn cpu_reference_and_gpu_result_match(&self) -> bool {
        self.cpu_reference_and_gpu_result_match
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn coverage_bitset_or(&self) -> u128 {
        self.coverage_bitset_or
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn fallback_reason(&self) -> Option<SearchBackendFallbackReason> {
        self.fallback_reason
    }
}
impl GpuPackingStrengtheningReport {
    pub const fn autotune(&self) -> GpuWorkerAutotuneDecision {
        self.autotune
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuPackingStrengthening;

impl GpuPackingStrengthening {
    pub fn evaluate(
        budget: GpuWorkerBudget,
        metrics: GpuWorkerMetrics,
        candidate_count_hint: u32,
        gpu_candidates: &[GpuPackingCandidate],
        cpu_reference: &[GpuPackingCandidate],
        fallback_reason: Option<SearchBackendFallbackReason>,
    ) -> GpuPackingStrengtheningReport {
        let larger_batch_plan = GpuLargerBatchPlanner::plan(budget, metrics, candidate_count_hint);
        let prefilter_report =
            GpuDominancePrefilter::retain_required_and_deduplicate_optional(gpu_candidates);
        let compressed = GpuReadbackCompression::compress(prefilter_report.retained_candidates());
        let decompressed = GpuReadbackCompression::decompress(&compressed).unwrap_or_default();
        let readback_compression_preserves_candidates =
            decompressed == prefilter_report.retained_candidates();
        let confirm_report = GpuCpuExactConfirmOptimizer::confirm_against_cpu_reference(
            &decompressed,
            cpu_reference,
        );
        let coverage_bitset_or =
            GpuCoverageBitsetOrHelper::union_confirmed(&confirm_report).unwrap_or(0);
        let autotune = GpuWorkerAutotune::evaluate(budget, metrics);

        GpuPackingStrengtheningReport {
            larger_batch_plan,
            gpu_candidate_hash: GpuCandidateHash::hash_candidates(&decompressed),
            compressed_readback_bytes: compressed.len(),
            readback_compression_preserves_candidates,
            dominance_prefilter_does_not_drop_required_candidate: !prefilter_report
                .required_candidate_dropped(),
            gpu_result_deterministic: confirm_report.gpu_result_deterministic(),
            gpu_result_cpu_confirmed: confirm_report.gpu_result_cpu_confirmed(),
            cpu_reference_and_gpu_result_match: confirm_report.cpu_reference_and_gpu_result_match(),
            coverage_bitset_or,
            fallback_reason,
            autotune,
        }
    }
}

#[cfg(test)]
#[path = "gpu_packing_strengthening_tests.rs"]
mod tests;
