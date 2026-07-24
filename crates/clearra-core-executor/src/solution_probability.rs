use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionCoverage {
    identity: StandardBoard64TilingIdentity,
    covered_patterns: PatternBitSet,
}

impl SolutionCoverage {
    pub fn new(identity: StandardBoard64TilingIdentity, covered_patterns: PatternBitSet) -> Self {
        Self {
            identity,
            covered_patterns,
        }
    }

    pub const fn identity(&self) -> StandardBoard64TilingIdentity {
        self.identity
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionProbabilityReport {
    solution_key: String,
    probability: String,
    covered_pattern_count: usize,
    pattern_count: usize,
    probability_complete: bool,
}

impl SolutionProbabilityReport {
    pub fn solution_key(&self) -> &str {
        &self.solution_key
    }

    pub fn probability(&self) -> &str {
        &self.probability
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}

pub fn probability_reports(
    identities: &[StandardBoard64TilingIdentity],
    coverage: &[SolutionCoverage],
    weights: &WeightedPatternSet,
    inputs_complete: bool,
) -> Vec<SolutionProbabilityReport> {
    debug_assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
    debug_assert!(coverage
        .windows(2)
        .all(|pair| pair[0].identity < pair[1].identity));

    let mut coverage_index = 0_usize;
    identities
        .iter()
        .copied()
        .map(|identity| {
            while coverage_index < coverage.len() && coverage[coverage_index].identity < identity {
                coverage_index += 1;
            }
            let matching = coverage
                .get(coverage_index)
                .filter(|entry| entry.identity == identity)
                .map(SolutionCoverage::covered_patterns);
            let probability = matching
                .and_then(|bits| weights.covered_weight(bits))
                .map(|value| canonical_probability(value.get()))
                .unwrap_or_else(|| "0".to_owned());
            let coverage_shape_matches =
                matching.is_some_and(|bits| bits.pattern_count() == weights.len());
            SolutionProbabilityReport {
                solution_key: NormalizedTilingSolutionKey::from_standard_board64_identity(identity)
                    .to_string(),
                probability,
                covered_pattern_count: matching.map(|bits| bits.count_ones() as usize).unwrap_or(0),
                pattern_count: weights.len(),
                probability_complete: inputs_complete && coverage_shape_matches,
            }
        })
        .collect()
}

pub(crate) fn covers_all_identities(
    identities: &[StandardBoard64TilingIdentity],
    coverage: &[SolutionCoverage],
) -> bool {
    debug_assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
    debug_assert!(coverage
        .windows(2)
        .all(|pair| pair[0].identity < pair[1].identity));

    let mut coverage_index = 0_usize;
    for identity in identities {
        while coverage_index < coverage.len() && coverage[coverage_index].identity < *identity {
            coverage_index += 1;
        }
        if coverage
            .get(coverage_index)
            .is_none_or(|entry| entry.identity != *identity)
        {
            return false;
        }
    }
    true
}

fn canonical_probability(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else if value == 1.0 {
        "1".to_owned()
    } else {
        value.to_string()
    }
}
