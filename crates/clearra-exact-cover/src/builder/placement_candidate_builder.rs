use crate::model::ExactCoverCandidate;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlacementCandidateBuilder;

impl PlacementCandidateBuilder {
    pub fn from_masks(masks: impl IntoIterator<Item = u64>) -> Vec<ExactCoverCandidate> {
        masks
            .into_iter()
            .enumerate()
            .map(|(id, mask)| {
                let columns = (0..64)
                    .filter(|index| (mask & (1_u64 << index)) != 0)
                    .map(|index| index as usize)
                    .collect();
                ExactCoverCandidate::new(id, columns)
            })
            .collect()
    }
}
