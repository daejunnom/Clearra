//! Recovery's adapter to the existing exact minimum/pinned portfolio engine.
//! The input is a COMPLETE solution coverage catalog, never representative
//! replay samples. Render pagination and GIF/PNG choices are not solver inputs.
use clearra_coverage::{
    cover::pinned_minimum_cover::{PinnedMinimumCoverError, PinnedMinimumCoverInput},
    pattern::pattern_bitset::PatternBitSet,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct RecoverySolutionCoverageRow {
    pub solution_id: String,
    pub covered_pairs: PatternBitSet,
}
#[derive(Clone, Debug)]
pub struct RecoverySolutionCoverageCatalog {
    pub input_identity: [u8; 32],
    pub universe_identity: [u8; 32],
    pub weight_model_identity: [u8; 32],
    pub pattern_count: usize,
    pub expected_solution_count: usize,
    pub enumeration_complete: bool,
    pub coverage_complete: bool,
    pub rows: Vec<RecoverySolutionCoverageRow>,
}
#[derive(Debug)]
pub enum RecoveryMinimumError {
    IncompleteCatalog,
    StaleInput,
    InvalidSolutionIdentity,
    PatternCountMismatch,
    UnknownPinnedSolution(String),
    DuplicatePinnedSolution(String),
    AllocationFailed,
    Exact(PinnedMinimumCoverError),
}

/// Keeps exact-search rows in stable original-solution order. Private pin bits
/// belong solely to the existing minimum engine, not to probability outputs.
pub struct RecoveryMinimumPreparation {
    pub solution_ids: Vec<String>,
    pub input: PinnedMinimumCoverInput,
}
impl RecoverySolutionCoverageCatalog {
    pub fn prepare_minimum(
        &self,
        expected_input: [u8; 32],
        pinned: &[String],
    ) -> Result<RecoveryMinimumPreparation, RecoveryMinimumError> {
        if self.input_identity != expected_input {
            return Err(RecoveryMinimumError::StaleInput);
        }
        if !self.enumeration_complete
            || !self.coverage_complete
            || self.expected_solution_count != self.rows.len()
        {
            return Err(RecoveryMinimumError::IncompleteCatalog);
        }
        let mut order = BTreeMap::new();
        for row in &self.rows {
            if row.solution_id.is_empty()
                || order
                    .insert(row.solution_id.as_str(), &row.covered_pairs)
                    .is_some()
            {
                return Err(RecoveryMinimumError::InvalidSolutionIdentity);
            }
            if row.covered_pairs.pattern_count() != self.pattern_count {
                return Err(RecoveryMinimumError::PatternCountMismatch);
            }
        }
        let mut ids = Vec::new();
        let mut rows = Vec::new();
        ids.try_reserve_exact(order.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        rows.try_reserve_exact(order.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        let mut required_words = vec![0_u64; self.pattern_count.div_ceil(64)];
        for (id, row) in order {
            for (union, word) in required_words.iter_mut().zip(row.to_owned_words()) {
                *union |= word;
            }
            ids.push(id.to_owned());
            rows.push(row.clone());
        }
        let mut pins = Vec::new();
        pins.try_reserve_exact(pinned.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        for id in pinned {
            let index = ids
                .binary_search(id)
                .map_err(|_| RecoveryMinimumError::UnknownPinnedSolution(id.clone()))?;
            if pins.contains(&index) {
                return Err(RecoveryMinimumError::DuplicatePinnedSolution(id.clone()));
            }
            pins.push(index);
        }
        let required = PatternBitSet::from_words(self.pattern_count, required_words)
            .map_err(|_| RecoveryMinimumError::PatternCountMismatch)?;
        let input = PinnedMinimumCoverInput::new(required, rows, pins)
            .map_err(RecoveryMinimumError::Exact)?;
        Ok(RecoveryMinimumPreparation {
            solution_ids: ids,
            input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_coverage::pattern::pattern_id::PatternId;
    fn catalog(masks: &[u8]) -> RecoverySolutionCoverageCatalog {
        RecoverySolutionCoverageCatalog {
            input_identity: [1; 32],
            universe_identity: [2; 32],
            weight_model_identity: [3; 32],
            pattern_count: 4,
            expected_solution_count: masks.len(),
            enumeration_complete: true,
            coverage_complete: true,
            rows: masks
                .iter()
                .enumerate()
                .map(|(id, &mask)| RecoverySolutionCoverageRow {
                    solution_id: format!("solution-{id}"),
                    covered_pairs: PatternBitSet::from_patterns(
                        4,
                        (0..4)
                            .filter(|bit| mask & (1 << bit) != 0)
                            .map(PatternId::new),
                    )
                    .unwrap(),
                })
                .collect(),
        }
    }
    #[test]
    fn recovery_minimum_and_pins_match_independent_exhaustive_subsets() {
        for seed in 0..32_u32 {
            let masks = (0..5)
                .map(|i| ((seed.wrapping_mul(13) + i * 7) % 16) as u8)
                .collect::<Vec<_>>();
            let catalog = catalog(&masks);
            for pins in [0_u32, 1, 3, 17] {
                let pinned = (0..5)
                    .filter(|i| pins & (1 << i) != 0)
                    .map(|i| format!("solution-{i}"))
                    .collect::<Vec<_>>();
                let prepared = catalog.prepare_minimum([1; 32], &pinned).unwrap();
                let actual = prepared.input.canonical_portfolio().unwrap();
                let required = masks.iter().fold(0, |a, b| a | b);
                let expected = (0..32_u32)
                    .filter(|set| set & pins == pins)
                    .filter(|set| {
                        (0..5)
                            .filter(|i| set & (1 << i) != 0)
                            .fold(0, |a, i| a | masks[i])
                            == required
                    })
                    .map(|set| set.count_ones() as usize)
                    .min()
                    .unwrap();
                assert_eq!(actual.all_row_indices().len(), expected);
                assert!(actual
                    .pinned_row_indices()
                    .iter()
                    .all(|&i| actual.all_row_indices().contains(&i)));
            }
        }
    }
    #[test]
    fn recovery_minimum_does_not_optimize_a_representative_page() {
        let mut c = catalog(&[15, 1, 2, 0]);
        let result = c
            .prepare_minimum([1; 32], &["solution-3".into()])
            .unwrap()
            .input
            .canonical_portfolio()
            .unwrap();
        assert_eq!(result.all_row_indices(), [0, 3]); // Even a redundant pin remains.
        assert!(matches!(
            c.prepare_minimum([9; 32], &[]),
            Err(RecoveryMinimumError::StaleInput)
        ));
        c.enumeration_complete = false;
        assert!(matches!(
            c.prepare_minimum([1; 32], &[]),
            Err(RecoveryMinimumError::IncompleteCatalog)
        ));
        c.enumeration_complete = true;
        c.expected_solution_count = 1000;
        assert!(matches!(
            c.prepare_minimum([1; 32], &[]),
            Err(RecoveryMinimumError::IncompleteCatalog)
        ));
    }
}
