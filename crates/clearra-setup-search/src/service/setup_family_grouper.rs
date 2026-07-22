use std::collections::{BTreeMap, BTreeSet};

use clearra_core_domain::{ids::setup_id::SetupFamilyId, piece::piece_kind::PieceKind};

use super::setup_candidate_enumerator::SetupBuildCandidate;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TilingKey {
    pub(crate) pieces: Vec<PieceKind>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct BuildKey {
    pub(crate) final_hold: Option<PieceKind>,
    pub(crate) remaining_queue: Vec<PieceKind>,
}

pub(crate) fn family_map_for_candidates(
    candidates: &[SetupBuildCandidate],
    max_shape_families: usize,
) -> BTreeMap<u64, SetupFamilyId> {
    let shapes = candidates
        .iter()
        .map(|candidate| candidate.occupied_shape)
        .collect::<BTreeSet<_>>();
    shapes
        .iter()
        .take(max_shape_families)
        .enumerate()
        .map(|(index, shape)| (*shape, SetupFamilyId::new(index as u32)))
        .collect()
}

pub(crate) fn unique_shape_count(candidates: &[SetupBuildCandidate]) -> usize {
    candidates
        .iter()
        .map(|candidate| candidate.occupied_shape)
        .collect::<BTreeSet<_>>()
        .len()
}

pub(crate) fn tiling_groups_for_family(
    candidates: &[SetupBuildCandidate],
) -> BTreeMap<TilingKey, Vec<SetupBuildCandidate>> {
    let mut groups = BTreeMap::new();
    for candidate in candidates {
        groups
            .entry(TilingKey {
                pieces: candidate.placed_pieces.clone(),
            })
            .or_insert_with(Vec::new)
            .push(candidate.clone());
    }
    groups
}

pub(crate) fn build_groups_for_tiling(
    candidates: &[SetupBuildCandidate],
) -> BTreeMap<BuildKey, Vec<SetupBuildCandidate>> {
    let mut groups = BTreeMap::new();
    for candidate in candidates {
        groups
            .entry(BuildKey {
                final_hold: candidate.final_hold,
                remaining_queue: candidate.remaining_queue.clone(),
            })
            .or_insert_with(Vec::new)
            .push(candidate.clone());
    }
    groups
}
