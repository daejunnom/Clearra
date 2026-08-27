#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePostProcessSpinCoverage {
    target_id: String,
    pass_index: usize,
    pattern_count: usize,
    covered_pattern_words: Vec<u64>,
    candidate_keys: Vec<String>,
    witnessed_pattern_count: u128,
    complete: bool,
}

impl CorePostProcessSpinCoverage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_id: impl Into<String>,
        pass_index: usize,
        pattern_count: usize,
        covered_pattern_words: Vec<u64>,
        mut candidate_keys: Vec<String>,
        witnessed_pattern_count: u128,
        complete: bool,
    ) -> Self {
        candidate_keys.sort_unstable();
        candidate_keys.dedup();
        Self {
            target_id: target_id.into(),
            pass_index,
            pattern_count,
            covered_pattern_words,
            candidate_keys,
            witnessed_pattern_count,
            complete,
        }
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub const fn pass_index(&self) -> usize {
        self.pass_index
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn covered_pattern_words(&self) -> &[u64] {
        &self.covered_pattern_words
    }

    pub fn candidate_keys(&self) -> &[String] {
        &self.candidate_keys
    }

    pub const fn witnessed_pattern_count(&self) -> u128 {
        self.witnessed_pattern_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Bytes retained by heap owners nested inside an already-accounted
    /// `CorePostProcessSpinCoverage` slot.
    ///
    /// Callers that own a `Vec<CorePostProcessSpinCoverage>` account its
    /// backing slots once, then add this value for every live element. This
    /// keeps the `String`/`Vec` headers from being counted twice while still
    /// including their full allocator-visible capacities and UTF-8 payloads.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.target_id.capacity() as u128;
        bytes = bytes.checked_add(
            (self.covered_pattern_words.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.candidate_keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
        for key in &self.candidate_keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        Some(bytes)
    }

    /// Full retained bytes when this value is itself a standalone heap-owned
    /// object rather than an element whose outer slot is already accounted.
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128).checked_add(self.checked_nested_retained_bytes()?)
    }

    /// Additional bytes requested by `Clone` before the clone is installed in
    /// an already-reserved outer slot. Existing storage remains live at this
    /// point and must be accounted separately by the caller.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        let mut bytes = self.target_id.len() as u128;
        bytes = bytes.checked_add(
            (self.covered_pattern_words.len() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.candidate_keys.len() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
        for key in &self.candidate_keys {
            bytes = bytes.checked_add(key.len() as u128)?;
        }
        Some(bytes)
    }

    /// Peak bytes while cloning this value as a standalone owner: the full
    /// original remains live alongside the new inline slot and nested clone.
    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_retained_bytes()?
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

#[cfg(test)]
mod tests {
    use super::CorePostProcessSpinCoverage;

    #[test]
    fn retained_projection_counts_outer_slots_capacities_and_utf8_payloads_once() {
        let mut target = String::with_capacity(23);
        target.push_str("곳");
        let mut words = Vec::with_capacity(9);
        words.extend([1_u64, 2]);
        let mut candidate = String::with_capacity(31);
        candidate.push_str("후보");
        let mut candidates = Vec::with_capacity(7);
        candidates.push(candidate);
        let coverage = CorePostProcessSpinCoverage::new(target, 0, 64, words, candidates, 1, true);

        let expected_nested = (coverage.target_id.capacity() as u128)
            + (coverage.covered_pattern_words.capacity() * core::mem::size_of::<u64>()) as u128
            + (coverage.candidate_keys.capacity() * core::mem::size_of::<String>()) as u128
            + coverage
                .candidate_keys
                .iter()
                .map(|key| key.capacity() as u128)
                .sum::<u128>();
        assert_eq!(
            coverage.checked_nested_retained_bytes(),
            Some(expected_nested)
        );
        assert_eq!(
            coverage.checked_retained_bytes(),
            Some(core::mem::size_of::<CorePostProcessSpinCoverage>() as u128 + expected_nested)
        );

        let clone_nested = coverage.target_id.len() as u128
            + (coverage.covered_pattern_words.len() * core::mem::size_of::<u64>()) as u128
            + (coverage.candidate_keys.len() * core::mem::size_of::<String>()) as u128
            + coverage
                .candidate_keys
                .iter()
                .map(|key| key.len() as u128)
                .sum::<u128>();
        assert_eq!(coverage.checked_clone_nested_bytes(), Some(clone_nested));
        assert_eq!(
            coverage.checked_clone_peak_bytes(),
            Some(
                core::mem::size_of::<CorePostProcessSpinCoverage>() as u128
                    + expected_nested
                    + core::mem::size_of::<CorePostProcessSpinCoverage>() as u128
                    + clone_nested
            )
        );
    }
}
