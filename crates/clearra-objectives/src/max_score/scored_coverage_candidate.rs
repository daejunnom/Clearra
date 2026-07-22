use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredCoverageCandidate {
    candidate_id: usize,
    patterns: PatternBitSet,
    score: u64,
    attack: u32,
}

impl ScoredCoverageCandidate {
    pub fn new(candidate_id: usize, patterns: PatternBitSet, score: u64, attack: u32) -> Self {
        Self {
            candidate_id,
            patterns,
            score,
            attack,
        }
    }
}
impl ScoredCoverageCandidate {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl ScoredCoverageCandidate {
    pub fn patterns(&self) -> &PatternBitSet {
        &self.patterns
    }
}
impl ScoredCoverageCandidate {
    pub fn score(&self) -> u64 {
        self.score
    }
}
impl ScoredCoverageCandidate {
    pub fn attack(&self) -> u32 {
        self.attack
    }
}
