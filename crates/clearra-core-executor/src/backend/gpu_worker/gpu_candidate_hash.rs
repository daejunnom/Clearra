use super::GpuPackingCandidate;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuCandidateHash;

impl GpuCandidateHash {
    pub fn hash_candidate(candidate: &GpuPackingCandidate) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for value in [
            candidate.shape_key(),
            candidate.tiling_key(),
            candidate.operation_set_key(),
            candidate.final_board_mask(),
            candidate.coverage_bits() as u64,
            (candidate.coverage_bits() >> 64) as u64,
        ] {
            hash = stable_hash_u64(hash, value);
        }
        hash
    }
}
impl GpuCandidateHash {
    pub fn hash_candidates(candidates: &[GpuPackingCandidate]) -> u64 {
        candidates
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, candidate| {
                stable_hash_u64(hash, Self::hash_candidate(candidate))
            })
    }
}

fn stable_hash_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
