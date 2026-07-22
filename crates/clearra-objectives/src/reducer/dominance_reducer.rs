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
    if left.score < right.score || left.attack < right.attack {
        return Some(false);
    }

    let strictly_better = left.patterns != right.patterns
        || left.score > right.score
        || left.attack > right.attack
        || left.candidate_id < right.candidate_id;
    Some(strictly_better)
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
}
