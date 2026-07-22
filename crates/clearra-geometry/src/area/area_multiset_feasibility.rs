use std::collections::BTreeMap;

use clearra_core_domain::ids::piece_id::PieceDefinitionId;
use clearra_piece_registry::registry::{
    piece_area_multiset_fingerprint, MixedBagProfile, MixedPieceSet, MixedPieceSetEntry,
};

use super::standard_tetromino_area_rule::StandardTetrominoAreaRule;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaMultisetFeasibility {
    active_piece_area_multiset: Vec<usize>,
    fingerprint: u64,
}

impl AreaMultisetFeasibility {
    pub fn new(
        active_piece_area_multiset: impl IntoIterator<Item = usize>,
    ) -> Result<Self, AreaMultisetFeasibilityError> {
        let active_piece_area_multiset = active_piece_area_multiset.into_iter().collect::<Vec<_>>();
        if active_piece_area_multiset.is_empty() {
            return Err(AreaMultisetFeasibilityError::EmptyPieceAreas);
        }
        if active_piece_area_multiset.contains(&0) {
            return Err(AreaMultisetFeasibilityError::ZeroPieceArea);
        }

        Ok(Self {
            fingerprint: piece_area_multiset_fingerprint(&active_piece_area_multiset),
            active_piece_area_multiset,
        })
    }
}
impl AreaMultisetFeasibility {
    pub fn standard_tetrominoes(piece_count: usize) -> Result<Self, AreaMultisetFeasibilityError> {
        Self::new(StandardTetrominoAreaRule::piece_areas(piece_count))
    }
}
impl AreaMultisetFeasibility {
    pub fn from_mixed_piece_set(
        piece_set: &MixedPieceSet,
    ) -> Result<Self, AreaMultisetFeasibilityError> {
        Self::new(piece_set.entries().iter().map(MixedPieceSetEntry::area))
    }
}
impl AreaMultisetFeasibility {
    pub fn from_mixed_bag_profile(
        piece_set: &MixedPieceSet,
        bag_profile: &MixedBagProfile,
    ) -> Result<Self, AreaMultisetFeasibilityError> {
        let area_by_id = piece_set
            .entries()
            .iter()
            .map(|entry| (entry.stable_id(), entry.area()))
            .collect::<BTreeMap<_, _>>();
        let mut areas = Vec::new();
        for entry in bag_profile.entries() {
            let area = area_by_id.get(entry.piece_id()).copied().ok_or_else(|| {
                AreaMultisetFeasibilityError::UnknownPieceId {
                    piece_id: entry.piece_id().clone(),
                }
            })?;
            areas.extend(std::iter::repeat(area).take(entry.multiplicity()));
        }
        Self::new(areas)
    }
}
impl AreaMultisetFeasibility {
    pub fn active_piece_area_multiset(&self) -> &[usize] {
        &self.active_piece_area_multiset
    }
}
impl AreaMultisetFeasibility {
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}
impl AreaMultisetFeasibility {
    pub fn can_fill_exactly(&self, component_area: usize) -> bool {
        bounded_area_subset_sum(component_area, &self.active_piece_area_multiset)
    }
}
impl AreaMultisetFeasibility {
    pub fn check_component_area(&self, component_area: usize) -> AreaFeasibilityDecision {
        if self.can_fill_exactly(component_area) {
            AreaFeasibilityDecision::SearchMayContinue
        } else {
            AreaFeasibilityDecision::RejectAreaInfeasible { component_area }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AreaMultisetFeasibilityError {
    EmptyPieceAreas,
    ZeroPieceArea,
    UnknownPieceId { piece_id: PieceDefinitionId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaFeasibilityDecision {
    RejectAreaInfeasible { component_area: usize },
    SearchMayContinue,
}

impl AreaFeasibilityDecision {
    pub fn is_reject(self) -> bool {
        matches!(self, Self::RejectAreaInfeasible { .. })
    }
}
impl AreaFeasibilityDecision {
    pub fn is_solution_found(self) -> bool {
        false
    }
}

fn bounded_area_subset_sum(target: usize, piece_areas: &[usize]) -> bool {
    let mut reachable = vec![false; target + 1];
    reachable[0] = true;

    for area in piece_areas {
        if *area > target {
            continue;
        }
        for candidate in (*area..=target).rev() {
            reachable[candidate] = reachable[candidate] || reachable[candidate - *area];
        }
    }

    reachable[target]
}

pub fn area_multiset_feasibility_uses_piece_area_multiset() -> bool {
    let feasibility = AreaMultisetFeasibility::new([4, 3]).expect("area multiset");
    feasibility.can_fill_exactly(7)
        && !feasibility.can_fill_exactly(5)
        && feasibility.active_piece_area_multiset() == [4, 3]
}

#[cfg(test)]
#[path = "area_multiset_feasibility_tests.rs"]
mod tests;
