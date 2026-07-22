use super::ProofLevel;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ComponentKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClearStateKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoardProfileId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PieceSetId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacementId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceFamilyMask(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceFamily(pub u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementDomainKey {
    pub component_key: ComponentKey,
    pub clear_state_key: ClearStateKey,
    pub board_profile_id: BoardProfileId,
    pub piece_set_id: PieceSetId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementDomain {
    pub key: PlacementDomainKey,
    pub candidate_placement_ids: Vec<PlacementId>,
    pub allowed_piece_mask: PieceFamilyMask,
    pub forced_piece_family: Option<PieceFamily>,
    proof_level: ProofLevel,
}

impl PlacementDomain {
    pub fn new(
        key: PlacementDomainKey,
        candidate_placement_ids: Vec<PlacementId>,
        allowed_piece_mask: PieceFamilyMask,
    ) -> Self {
        Self {
            key,
            candidate_placement_ids,
            allowed_piece_mask,
            forced_piece_family: None,
            proof_level: ProofLevel::ClearStateConditional,
        }
    }
}
impl PlacementDomain {
    pub fn with_forced_piece_family(mut self, family: PieceFamily) -> Self {
        self.forced_piece_family = Some(family);
        self
    }
}
impl PlacementDomain {
    pub fn is_empty_under_clear_state(&self) -> bool {
        self.candidate_placement_ids.is_empty()
            && self.proof_level == ProofLevel::ClearStateConditional
    }
}
impl PlacementDomain {
    pub const fn proof_level(&self) -> ProofLevel {
        self.proof_level
    }
}

#[cfg(test)]
#[path = "placement_domain_tests.rs"]
mod tests;
