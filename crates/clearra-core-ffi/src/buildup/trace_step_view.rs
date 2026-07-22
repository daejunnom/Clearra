use super::kick_evidence_view::CKickEvidenceView;

pub const C_BUILDUP_HOLD_BRANCH_CURRENT: u8 = 1;
pub const C_BUILDUP_HOLD_BRANCH_SWAP_HELD: u8 = 2;
pub const C_BUILDUP_HOLD_BRANCH_STORE_CURRENT: u8 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CReachabilityEvidenceView {
    pub reachable: u8,
    pub exhaustive: u8,
    pub used_kick: u8,
    pub used_180: u8,
    pub visited_states: u16,
    pub last_action_was_rotation: u8,
    pub rotation_evidence_complete: u8,
    pub path_digest: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBuildUpTraceStep {
    pub operation_id: u16,
    pub operation_index: u16,
    pub piece: u8,
    pub rotation: u8,
    pub hold_branch_kind: u8,
    pub used_hold: u8,
    pub incoming_piece: u8,
    pub held_piece_before: u8,
    pub hold_empty_before: u8,
    pub kick_evidence_index: u8,
    pub adjusted_x: i8,
    pub adjusted_y: i8,
    pub cleared_row_mask: u16,
    pub target_frame_mask: u64,
    pub reachability: CReachabilityEvidenceView,
}

impl CBuildUpTraceStep {
    pub fn kick_evidence<'a>(
        &self,
        evidence: &'a [CKickEvidenceView],
    ) -> Option<&'a CKickEvidenceView> {
        (self.kick_evidence_index != u8::MAX)
            .then(|| evidence.get(usize::from(self.kick_evidence_index)))
            .flatten()
    }
}
