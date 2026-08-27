use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DominanceCandidate {
    candidate_id: usize,
    patterns: PatternBitSet,
    score: u64,
    attack: u32,
}

impl DominanceCandidate {
    pub fn new(candidate_id: usize, patterns: PatternBitSet, score: u64, attack: u32) -> Self {
        Self {
            candidate_id,
            patterns,
            score,
            attack,
        }
    }
}
impl DominanceCandidate {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl DominanceCandidate {
    pub fn patterns(&self) -> &PatternBitSet {
        &self.patterns
    }
}
impl DominanceCandidate {
    pub fn score(&self) -> u64 {
        self.score
    }
}
impl DominanceCandidate {
    pub fn attack(&self) -> u32 {
        self.attack
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DominanceReducer;

impl DominanceReducer {
    pub fn reduce(candidates: &[DominanceCandidate]) -> Vec<DominanceCandidate> {
        candidates
            .iter()
            .filter(|candidate| {
                !candidates
                    .iter()
                    .any(|other| dominates(other, candidate).unwrap_or(false))
            })
            .cloned()
            .collect()
    }
}

fn dominates(left: &DominanceCandidate, right: &DominanceCandidate) -> Option<bool> {
    if left.candidate_id == right.candidate_id {
        return Some(false);
    }
    if !left.patterns.is_superset(&right.patterns).ok()? {
        return Some(false);
    }
    // Attack is informational and must never participate in product
    // eligibility or dominance. Equal-score identities are retained even when
    // one coverage row is a superset: the narrower original row can still be
    // a member of a distinct equal-cardinality optimum with other rows.
    if left.score <= right.score {
        return Some(false);
    }

    Some(true)
}

#[cfg(test)]
mod tests {
    use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

    use super::*;

    fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
        PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
            .expect("patterns")
    }

    #[test]
    fn dominance_reducer_removes_covered_weaker_candidates() {
        let candidates = vec![
            DominanceCandidate::new(1, bitset(3, &[0, 1]), 100, 2),
            DominanceCandidate::new(2, bitset(3, &[0]), 80, 1),
            DominanceCandidate::new(3, bitset(3, &[2]), 20, 1),
        ];

        let reduced = DominanceReducer::reduce(&candidates);

        assert_eq!(
            reduced
                .iter()
                .map(DominanceCandidate::candidate_id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn attack_and_candidate_id_do_not_dominate_equal_score_rows() {
        let candidates = vec![
            DominanceCandidate::new(1, bitset(2, &[0, 1]), 100, 999),
            DominanceCandidate::new(2, bitset(2, &[0]), 100, 0),
            DominanceCandidate::new(3, bitset(2, &[0]), 100, 1_000),
        ];

        let reduced = DominanceReducer::reduce(&candidates);

        assert_eq!(reduced, candidates);
    }
}
