#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CKickEvidenceView {
    pub has_kick_evidence: u8,
    pub from_rotation: u8,
    pub to_rotation: u8,
    pub rotation_request: u8,
    pub kick_index: u8,
    pub kick_dx: i8,
    pub kick_dy: i8,
    pub reserved0: u8,
    pub kick_table_id: u64,
    pub kick_profile_id: u64,
    pub first_success_confirmed: u8,
    pub reserved1: [u8; 7],
    pub predecessor_x: i16,
    pub predecessor_y: i16,
    pub result_x: i16,
    pub result_y: i16,
}

impl CKickEvidenceView {
    pub fn first_success(
        from_rotation: u8,
        to_rotation: u8,
        rotation_request: u8,
        kick_index: u8,
        kick_dx: i8,
        kick_dy: i8,
    ) -> Self {
        Self {
            has_kick_evidence: 1,
            from_rotation,
            to_rotation,
            rotation_request,
            kick_index,
            kick_dx,
            kick_dy,
            first_success_confirmed: 1,
            ..Default::default()
        }
    }
}
