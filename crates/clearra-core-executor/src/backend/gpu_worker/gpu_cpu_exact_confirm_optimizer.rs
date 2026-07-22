use std::collections::BTreeMap;

use crate::backend::GpuTrustState;

use super::{GpuCandidateHash, GpuPackingCandidate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuCpuExactConfirmReport {
    gpu_result_deterministic: bool,
    gpu_result_cpu_confirmed: bool,
    cpu_reference_and_gpu_result_match: bool,
    hash_exact_confirmed: bool,
    hash_only_success: bool,
    confirmed_candidates: Vec<GpuPackingCandidate>,
    trust_state: GpuTrustState,
}

impl GpuCpuExactConfirmReport {
    pub const fn gpu_result_deterministic(&self) -> bool {
        self.gpu_result_deterministic
    }
}
impl GpuCpuExactConfirmReport {
    pub const fn gpu_result_cpu_confirmed(&self) -> bool {
        self.gpu_result_cpu_confirmed
    }
}
impl GpuCpuExactConfirmReport {
    pub const fn cpu_reference_and_gpu_result_match(&self) -> bool {
        self.cpu_reference_and_gpu_result_match
    }
}
impl GpuCpuExactConfirmReport {
    pub const fn hash_exact_confirmed(&self) -> bool {
        self.hash_exact_confirmed
    }
}
impl GpuCpuExactConfirmReport {
    pub const fn hash_only_success(&self) -> bool {
        self.hash_only_success
    }
}
impl GpuCpuExactConfirmReport {
    pub fn confirmed_candidates(&self) -> &[GpuPackingCandidate] {
        &self.confirmed_candidates
    }
}
impl GpuCpuExactConfirmReport {
    pub const fn trust_state(&self) -> GpuTrustState {
        self.trust_state
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuCpuExactConfirmOptimizer;

impl GpuCpuExactConfirmOptimizer {
    pub fn confirm_against_cpu_reference(
        gpu_candidates: &[GpuPackingCandidate],
        cpu_reference: &[GpuPackingCandidate],
    ) -> GpuCpuExactConfirmReport {
        let mut cpu_by_hash = BTreeMap::<u64, Vec<&GpuPackingCandidate>>::new();
        for candidate in cpu_reference {
            cpu_by_hash
                .entry(GpuCandidateHash::hash_candidate(candidate))
                .or_default()
                .push(candidate);
        }

        let mut confirmed = Vec::new();
        for gpu_candidate in gpu_candidates {
            let hash = GpuCandidateHash::hash_candidate(gpu_candidate);
            let Some(bucket) = cpu_by_hash.get(&hash) else {
                return mismatch_report();
            };
            if bucket
                .iter()
                .any(|cpu_candidate| gpu_candidate.exact_identity_matches(cpu_candidate))
            {
                confirmed.push(gpu_candidate.clone());
            } else {
                return mismatch_report();
            }
        }

        let all_cpu_matched = cpu_reference.iter().all(|cpu_candidate| {
            confirmed
                .iter()
                .any(|gpu_candidate| gpu_candidate.exact_identity_matches(cpu_candidate))
        });

        GpuCpuExactConfirmReport {
            gpu_result_deterministic: all_cpu_matched,
            gpu_result_cpu_confirmed: all_cpu_matched,
            cpu_reference_and_gpu_result_match: all_cpu_matched,
            hash_exact_confirmed: all_cpu_matched,
            hash_only_success: false,
            confirmed_candidates: if all_cpu_matched {
                confirmed
            } else {
                Vec::new()
            },
            trust_state: if all_cpu_matched {
                GpuTrustState::GpuComputedCpuConfirmed
            } else {
                GpuTrustState::GpuComputedMismatch
            },
        }
    }
}

fn mismatch_report() -> GpuCpuExactConfirmReport {
    GpuCpuExactConfirmReport {
        gpu_result_deterministic: false,
        gpu_result_cpu_confirmed: false,
        cpu_reference_and_gpu_result_match: false,
        hash_exact_confirmed: false,
        hash_only_success: false,
        confirmed_candidates: Vec::new(),
        trust_state: GpuTrustState::GpuComputedMismatch,
    }
}
