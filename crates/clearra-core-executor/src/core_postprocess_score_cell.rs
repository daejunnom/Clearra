use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CorePostProcessScoreCell {
    candidate_identity: StandardBoard64TilingIdentity,
    pattern_id: usize,
    trace_identity: String,
    score: u64,
    attack: u32,
}

impl CorePostProcessScoreCell {
    pub fn new(
        candidate_identity: StandardBoard64TilingIdentity,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
    ) -> Self {
        Self {
            candidate_identity,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
        }
    }

    pub const fn candidate_identity(&self) -> StandardBoard64TilingIdentity {
        self.candidate_identity
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn attack(&self) -> u32 {
        self.attack
    }

    /// Heap storage retained by this score cell, excluding its inline value.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        Some(self.trace_identity.capacity() as u128)
    }

    /// Heap storage requested by cloning this score cell's nested owner.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        Some(self.trace_identity.len() as u128)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.checked_nested_retained_bytes()?)?
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

#[cfg(test)]
mod retained_memory_projection_tests {
    use super::*;

    #[test]
    fn score_cell_projection_distinguishes_capacity_from_clone_length() {
        let mut trace_identity = String::with_capacity(29);
        trace_identity.push_str("trace");
        let retained_identity_bytes = trace_identity.capacity() as u128;
        let cell = CorePostProcessScoreCell::new(
            StandardBoard64TilingIdentity::from_placements(0, []).expect("empty tiling identity"),
            0,
            trace_identity,
            0,
            0,
        );
        assert_eq!(
            cell.checked_nested_retained_bytes(),
            Some(retained_identity_bytes)
        );
        assert_eq!(cell.checked_clone_nested_bytes(), Some(5));
        assert_eq!(
            cell.checked_clone_peak_bytes(),
            Some(
                2_u128 * core::mem::size_of::<CorePostProcessScoreCell>() as u128
                    + retained_identity_bytes
                    + 5
            )
        );
    }
}
