// SRP rationale: bound inverse Build row projections by the rows its exact target can complete.

#[derive(Clone, Copy)]
pub(super) struct ClearRowDomain(u32);

impl ClearRowDomain {
    /// PC and graph materialization retain their original catalog contract.
    pub const fn unrestricted() -> Self {
        Self(u32::MAX)
    }

    /// Every Build lock belongs to the declared target. A logical row outside
    /// this union cannot ever clear, regardless of placement order or hold.
    pub fn for_completed_target(
        width: u8,
        height: u8,
        mut occupied_row: impl FnMut(u8) -> u64,
    ) -> Self {
        let full_row = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        let mut rows = 0_u32;
        for row in 0..height {
            if occupied_row(row) == full_row {
                rows |= 1_u32 << row;
            }
        }
        Self(rows)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn allows_required_rows(self, required: u32) -> bool {
        required & !self.0 == 0
    }

    /// The enumerator has fixed the preceding row. Only the newly introduced
    /// interval needs checking; earlier intervals have already passed.
    pub fn allows_gap(self, first_deleted: u8, next_target_row: u8) -> bool {
        let below = |row: u8| u32::MAX.checked_shr(32 - u32::from(row)).unwrap_or(0);
        self.allows_required_rows(below(next_target_row) & !below(first_deleted))
    }
}
