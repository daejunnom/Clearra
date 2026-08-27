use clearra_core_domain::probability::probability_value::ProbabilityValue;
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

    /// Heap storage owned by this value. Cloning a `PatternBitSet` only clones
    /// its backing `Arc`, so the shared storage is retained once and creates no
    /// additional nested allocation in the clone.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        self.covered_patterns.checked_storage_retained_bytes()
    }

    pub(crate) const fn checked_non_pattern_storage_retained_bytes(&self) -> Option<u128> {
        Some(0)
    }

    pub const fn checked_clone_nested_bytes(&self) -> Option<u128> {
        Some(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedSolutionCoverage {
    solution_key: String,
    covered_patterns: PatternBitSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedSolutionProbabilityError {
    SolutionKeysNotCanonical,
    CoverageKeysNotCanonical,
    CoverageKeysMismatch,
    ReportStorageUnavailable,
    PatternCountMismatch,
    PatternWeightMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionProbabilityPatternWeightsError {
    RequestPolicyMissing,
    RequestPolicyDuplicate,
    RequestPolicyInvalid,
    NotRequested,
    PatternCountMissing,
    PatternCountDuplicate,
    PatternCountInvalid,
    PatternWeightCountMismatch,
    PatternWeightInvalid { index: usize },
    PatternWeightNotCanonical { index: usize },
    PatternWeightSetInvalid,
}

impl SolutionProbabilityPatternWeightsError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::RequestPolicyMissing => "solution_probability_weight_policy_missing",
            Self::RequestPolicyDuplicate => "solution_probability_weight_policy_duplicate",
            Self::RequestPolicyInvalid => "solution_probability_weight_policy_invalid",
            Self::NotRequested => "solution_probability_weights_not_requested",
            Self::PatternCountMissing => "solution_probability_weight_pattern_count_missing",
            Self::PatternCountDuplicate => "solution_probability_weight_pattern_count_duplicate",
            Self::PatternCountInvalid => "solution_probability_weight_pattern_count_invalid",
            Self::PatternWeightCountMismatch => {
                "solution_probability_pattern_weight_count_mismatch"
            }
            Self::PatternWeightInvalid { .. } => "solution_probability_pattern_weight_invalid",
            Self::PatternWeightNotCanonical { .. } => {
                "solution_probability_pattern_weight_not_canonical"
            }
            Self::PatternWeightSetInvalid => "solution_probability_pattern_weight_set_invalid",
        }
    }
}

impl core::fmt::Display for SolutionProbabilityPatternWeightsError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for SolutionProbabilityPatternWeightsError {}

impl NormalizedSolutionProbabilityError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::SolutionKeysNotCanonical => "solution_probability_keys_not_canonical",
            Self::CoverageKeysNotCanonical => "solution_probability_coverages_not_canonical",
            Self::CoverageKeysMismatch => "solution_probability_coverage_keys_mismatch",
            Self::ReportStorageUnavailable => "solution_probability_report_storage_unavailable",
            Self::PatternCountMismatch => "solution_probability_pattern_count_mismatch",
            Self::PatternWeightMismatch => "solution_probability_pattern_weight_mismatch",
        }
    }
}

impl core::fmt::Display for NormalizedSolutionProbabilityError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for NormalizedSolutionProbabilityError {}

impl NormalizedSolutionCoverage {
    pub fn new(solution_key: impl Into<String>, covered_patterns: PatternBitSet) -> Self {
        let solution_key = solution_key.into();
        assert!(
            !solution_key.is_empty(),
            "normalized solution coverage key must be nonempty"
        );
        Self {
            solution_key,
            covered_patterns,
        }
    }

    pub fn solution_key(&self) -> &str {
        &self.solution_key
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128)
            .checked_add(self.covered_patterns.checked_storage_retained_bytes()?)
    }

    pub(crate) fn checked_non_pattern_storage_retained_bytes(&self) -> Option<u128> {
        Some(self.solution_key.capacity() as u128)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        Some(self.solution_key.len() as u128)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionAverageScoreReport {
    solution_key: String,
    average_score: String,
    covered_pattern_count: usize,
    pattern_count: usize,
    score_complete: bool,
}

impl SolutionAverageScoreReport {
    pub fn new(
        solution_key: impl Into<String>,
        average_score: impl Into<String>,
        covered_pattern_count: usize,
        pattern_count: usize,
        score_complete: bool,
    ) -> Self {
        Self {
            solution_key: solution_key.into(),
            average_score: average_score.into(),
            covered_pattern_count,
            pattern_count,
            score_complete,
        }
    }

    pub fn solution_key(&self) -> &str {
        &self.solution_key
    }

    pub fn average_score(&self) -> &str {
        &self.average_score
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn score_complete(&self) -> bool {
        self.score_complete
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.average_score.capacity() as u128)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.solution_key.len() as u128).checked_add(self.average_score.len() as u128)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
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

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.probability.capacity() as u128)
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.solution_key.len() as u128).checked_add(self.probability.len() as u128)
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
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

/// Builds exact reports for an already-normalized solution surface.
///
/// Both inputs must use the same strict canonical key order. Requiring exact
/// key equality and an exact pattern denominator prevents a missing, duplicate,
/// or foreign coverage row from being reported as a zero-probability solution.
pub fn normalized_solution_probability_reports(
    solution_keys: &[String],
    coverage: &[NormalizedSolutionCoverage],
    weights: &WeightedPatternSet,
    inputs_complete: bool,
) -> Result<Vec<SolutionProbabilityReport>, NormalizedSolutionProbabilityError> {
    if solution_keys.iter().any(String::is_empty)
        || !solution_keys.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(NormalizedSolutionProbabilityError::SolutionKeysNotCanonical);
    }
    if coverage.iter().any(|entry| entry.solution_key().is_empty())
        || !coverage
            .windows(2)
            .all(|pair| pair[0].solution_key() < pair[1].solution_key())
    {
        return Err(NormalizedSolutionProbabilityError::CoverageKeysNotCanonical);
    }
    if solution_keys.len() != coverage.len()
        || solution_keys
            .iter()
            .zip(coverage)
            .any(|(key, entry)| key != entry.solution_key())
    {
        return Err(NormalizedSolutionProbabilityError::CoverageKeysMismatch);
    }

    let mut reports = Vec::new();
    reports
        .try_reserve_exact(solution_keys.len())
        .map_err(|_| NormalizedSolutionProbabilityError::ReportStorageUnavailable)?;
    for (solution_key, entry) in solution_keys.iter().zip(coverage) {
        let covered_patterns = entry.covered_patterns();
        if covered_patterns.pattern_count() != weights.len() {
            return Err(NormalizedSolutionProbabilityError::PatternCountMismatch);
        }
        let probability = weights
            .covered_weight(covered_patterns)
            .ok_or(NormalizedSolutionProbabilityError::PatternWeightMismatch)?;
        reports.push(SolutionProbabilityReport {
            solution_key: solution_key.clone(),
            probability: canonical_probability(probability.get()),
            covered_pattern_count: covered_patterns.count_ones() as usize,
            pattern_count: weights.len(),
            probability_complete: inputs_complete,
        });
    }
    Ok(reports)
}

/// Reconstructs the exact typed pattern-weight authority bound to an included
/// per-solution probability result.
///
/// Core materializes the canonical round-trippable weight strings in the
/// private execution batch surface. This function validates the request and
/// denominator fields exactly once, rejects non-canonical or malformed weight
/// rows, and returns the same typed weight set used by the report reducer.
pub fn solution_probability_pattern_weights(
    result: &crate::core_execution_result::CoreExecutionResult,
) -> Result<WeightedPatternSet, SolutionProbabilityPatternWeightsError> {
    let requested = match result.field_occurrence_count("solution_probabilities_requested") {
        0 => return Err(SolutionProbabilityPatternWeightsError::RequestPolicyMissing),
        1 => match result.unique_field("solution_probabilities_requested") {
            Some("true") => true,
            Some("false") => false,
            _ => return Err(SolutionProbabilityPatternWeightsError::RequestPolicyInvalid),
        },
        _ => return Err(SolutionProbabilityPatternWeightsError::RequestPolicyDuplicate),
    };
    if !requested {
        return Err(SolutionProbabilityPatternWeightsError::NotRequested);
    }
    let pattern_count = match result.field_occurrence_count("coverage_pattern_count") {
        0 => return Err(SolutionProbabilityPatternWeightsError::PatternCountMissing),
        1 => {
            let value = result
                .unique_field("coverage_pattern_count")
                .ok_or(SolutionProbabilityPatternWeightsError::PatternCountInvalid)?;
            let parsed = value
                .parse::<usize>()
                .map_err(|_| SolutionProbabilityPatternWeightsError::PatternCountInvalid)?;
            if parsed.to_string() != value || parsed == 0 {
                return Err(SolutionProbabilityPatternWeightsError::PatternCountInvalid);
            }
            parsed
        }
        _ => return Err(SolutionProbabilityPatternWeightsError::PatternCountDuplicate),
    };
    let serialized = result.postprocess_pattern_weights();
    if serialized.len() != pattern_count {
        return Err(SolutionProbabilityPatternWeightsError::PatternWeightCountMismatch);
    }
    let mut weights = Vec::new();
    weights
        .try_reserve_exact(pattern_count)
        .map_err(|_| SolutionProbabilityPatternWeightsError::PatternWeightSetInvalid)?;
    for (index, serialized_weight) in serialized.iter().enumerate() {
        let parsed = serialized_weight
            .parse::<f64>()
            .ok()
            .and_then(|value| ProbabilityValue::new(value).ok())
            .ok_or(SolutionProbabilityPatternWeightsError::PatternWeightInvalid { index })?;
        if parsed.get().to_string() != serialized_weight.as_str() {
            return Err(
                SolutionProbabilityPatternWeightsError::PatternWeightNotCanonical { index },
            );
        }
        weights.push(parsed);
    }
    WeightedPatternSet::new(weights)
        .map_err(|_| SolutionProbabilityPatternWeightsError::PatternWeightSetInvalid)
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

#[cfg(test)]
mod normalized_probability_tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{NormalizedTilingSolutionKey, PiecePlacementMask},
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };

    use super::{
        normalized_solution_probability_reports, solution_probability_pattern_weights,
        NormalizedSolutionCoverage, NormalizedSolutionProbabilityError,
        SolutionProbabilityPatternWeightsError,
    };
    use crate::CoreExecutionResult;

    fn canonical_key(piece: PieceKind, cells_mask: u64) -> String {
        NormalizedTilingSolutionKey::from_placements(
            0,
            [PiecePlacementMask::new(piece, cells_mask)],
        )
        .expect("canonical solution")
        .as_str()
        .to_owned()
    }

    fn bits(pattern_count: usize, words: Vec<u64>) -> PatternBitSet {
        PatternBitSet::from_words(pattern_count, words).expect("matching pattern bitset")
    }

    #[test]
    fn normalized_reports_preserve_canonical_order_and_exact_denominator() {
        let mut keys = vec![
            canonical_key(PieceKind::O, 0x0c03),
            canonical_key(PieceKind::I, 0x000f),
        ];
        keys.sort_unstable();
        let coverage = vec![
            NormalizedSolutionCoverage::new(keys[0].clone(), bits(4, vec![0b0101])),
            NormalizedSolutionCoverage::new(keys[1].clone(), bits(4, vec![0b1000])),
        ];
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");

        let reports = normalized_solution_probability_reports(&keys, &coverage, &weights, true)
            .expect("validated normalized reports");

        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].solution_key(), keys[0]);
        assert_eq!(reports[0].probability(), "0.5");
        assert_eq!(reports[0].covered_pattern_count(), 2);
        assert_eq!(reports[0].pattern_count(), 4);
        assert!(reports[0].probability_complete());
        assert_eq!(reports[1].solution_key(), keys[1]);
        assert_eq!(reports[1].probability(), "0.25");
        assert_eq!(reports[1].covered_pattern_count(), 1);
    }

    #[test]
    fn normalized_reports_reject_missing_foreign_and_wrong_shape_coverage() {
        let mut keys = vec![
            canonical_key(PieceKind::O, 0x0c03),
            canonical_key(PieceKind::I, 0x000f),
        ];
        keys.sort_unstable();
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");

        let missing = vec![NormalizedSolutionCoverage::new(
            keys[0].clone(),
            bits(4, vec![1]),
        )];
        assert_eq!(
            normalized_solution_probability_reports(&keys, &missing, &weights, false),
            Err(NormalizedSolutionProbabilityError::CoverageKeysMismatch)
        );

        let foreign = vec![
            NormalizedSolutionCoverage::new(keys[0].clone(), bits(4, vec![1])),
            NormalizedSolutionCoverage::new("foreign", bits(4, vec![1])),
        ];
        assert!(normalized_solution_probability_reports(&keys, &foreign, &weights, false).is_err());

        let wrong_shape = vec![
            NormalizedSolutionCoverage::new(keys[0].clone(), bits(4, vec![1])),
            NormalizedSolutionCoverage::new(keys[1].clone(), bits(3, vec![1])),
        ];
        assert_eq!(
            normalized_solution_probability_reports(&keys, &wrong_shape, &weights, false),
            Err(NormalizedSolutionProbabilityError::PatternCountMismatch)
        );
    }

    fn weighted_result(weights: Vec<&str>) -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![
                (
                    "solution_probabilities_requested".to_owned(),
                    "true".to_owned(),
                ),
                ("coverage_pattern_count".to_owned(), "2".to_owned()),
            ],
            Vec::new(),
        )
        .with_postprocess_execution_batch(
            Vec::new(),
            true,
            weights.into_iter().map(str::to_owned).collect(),
        )
    }

    #[test]
    fn result_weight_authority_reconstructs_the_exact_typed_pattern_set() {
        let weights = solution_probability_pattern_weights(&weighted_result(vec!["0.25", "0.75"]))
            .expect("canonical result weights");

        assert_eq!(weights.len(), 2);
        assert_eq!(
            weights
                .weight(clearra_coverage::pattern::pattern_id::PatternId::new(0))
                .unwrap()
                .get(),
            0.25
        );
        assert_eq!(
            weights
                .weight(clearra_coverage::pattern::pattern_id::PatternId::new(1))
                .unwrap()
                .get(),
            0.75
        );
    }

    #[test]
    fn result_weight_authority_rejects_missing_noncanonical_and_invalid_rows() {
        assert_eq!(
            solution_probability_pattern_weights(&weighted_result(vec!["1"])),
            Err(SolutionProbabilityPatternWeightsError::PatternWeightCountMismatch)
        );
        assert_eq!(
            solution_probability_pattern_weights(&weighted_result(vec!["0.50", "0.5"])),
            Err(SolutionProbabilityPatternWeightsError::PatternWeightNotCanonical { index: 0 })
        );
        assert_eq!(
            solution_probability_pattern_weights(&weighted_result(vec!["NaN", "0.5"])),
            Err(SolutionProbabilityPatternWeightsError::PatternWeightInvalid { index: 0 })
        );
        let duplicate = weighted_result(vec!["0.5", "0.5"])
            .with_additional_fields(vec![("coverage_pattern_count".to_owned(), "2".to_owned())]);
        assert_eq!(
            solution_probability_pattern_weights(&duplicate),
            Err(SolutionProbabilityPatternWeightsError::PatternCountDuplicate)
        );
    }
}

#[cfg(test)]
mod memory_projection_tests {
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    use super::{
        NormalizedSolutionCoverage, SolutionAverageScoreReport, SolutionProbabilityReport,
    };

    fn reserved(value: &str, capacity: usize) -> String {
        let mut result = String::with_capacity(capacity);
        result.push_str(value);
        result
    }

    #[test]
    fn solution_reports_project_actual_string_capacities_and_clone_lengths() {
        let probability = SolutionProbabilityReport {
            solution_key: reserved("key", 41),
            probability: reserved("0.5", 47),
            covered_pattern_count: 1,
            pattern_count: 2,
            probability_complete: true,
        };
        let probability_retained =
            probability.solution_key.capacity() + probability.probability.capacity();
        assert_eq!(
            probability.checked_nested_retained_bytes(),
            Some(probability_retained as u128)
        );
        assert_eq!(probability.checked_clone_nested_bytes(), Some(6));
        assert_eq!(
            probability.checked_clone_peak_bytes(),
            Some((probability_retained + 6) as u128)
        );

        let average =
            SolutionAverageScoreReport::new(reserved("key", 43), reserved("12.25", 53), 1, 2, true);
        let average_retained = average.solution_key.capacity() + average.average_score.capacity();
        assert_eq!(
            average.checked_nested_retained_bytes(),
            Some(average_retained as u128)
        );
        assert_eq!(average.checked_clone_nested_bytes(), Some(8));
        assert_eq!(
            average.checked_clone_peak_bytes(),
            Some((average_retained + 8) as u128)
        );

        let normalized =
            NormalizedSolutionCoverage::new(reserved("key", 59), PatternBitSet::new(65));
        assert_eq!(
            normalized.checked_nested_retained_bytes(),
            Some(
                normalized.solution_key.capacity() as u128
                    + 2 * core::mem::size_of::<u64>() as u128
            )
        );
        assert_eq!(normalized.checked_clone_nested_bytes(), Some(3));
    }
}
