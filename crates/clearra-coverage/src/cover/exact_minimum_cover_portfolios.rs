use std::sync::Arc;

use crate::pattern::pattern_bitset::PatternBitSet;

use super::exact_minimum_cover::{
    exact_minimum_cover, ExactMinimumCoverError, ExactMinimumCoverResult,
};

/// One exact minimum-cardinality cover expressed in original input-row
/// identities. The row indices are strictly increasing, which also defines the
/// canonical order between portfolios.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverPortfolio {
    row_indices: Vec<usize>,
}

impl ExactMinimumCoverPortfolio {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }

    pub fn into_row_indices(self) -> Vec<usize> {
        self.row_indices
    }
}

/// Why a lazy enumeration call returned before sealing the semantic result
/// set. `PageFull` is a presentation boundary; the other two reasons are
/// explicit incomplete outcomes that callers must not describe as "all".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverEnumerationStop {
    PageFull,
    WorkBudgetExhausted,
    Cancelled,
    Sealed,
}

/// Opaque restart checkpoint for the immutable `(required, rows)` input that
/// created it. A persistence layer must bind this value to its own query,
/// candidate-map, build, and integrity identities before accepting it from an
/// untrusted source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverRestart {
    input: Arc<ExactMinimumCoverPortfolioInput>,
    optimal_cardinality: usize,
    next_combination: Option<Vec<usize>>,
    known_alternative_count: DecimalCounter,
    enumeration_complete: bool,
}

impl ExactMinimumCoverRestart {
    pub fn row_count(&self) -> usize {
        self.input.row_words.len()
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn next_combination(&self) -> Option<&[usize]> {
        self.next_combination.as_deref()
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.known_alternative_count.to_decimal_string()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    /// Exact heap payload retained by this restart owner. This includes the
    /// immutable restart input shared by clones, the current combination, and
    /// the arbitrary-precision decimal counter. Callers that share a cloned
    /// enumerator must charge the immutable input graph only once.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.input.row_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<Vec<u64>>() as u128)?;
        for row in &self.input.row_words {
            bytes = bytes.checked_add(
                (row.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(
            (self.input.target_words.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        if let Some(combination) = &self.next_combination {
            bytes = bytes.checked_add(
                (combination.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(self.known_alternative_count.digits.capacity() as u128)?;
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverPortfolioPage {
    portfolios: Vec<ExactMinimumCoverPortfolio>,
    optimal_cardinality: usize,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
    stop: ExactMinimumCoverEnumerationStop,
    work_steps: u64,
    restart: Option<ExactMinimumCoverRestart>,
}

impl ExactMinimumCoverPortfolioPage {
    pub fn portfolios(&self) -> &[ExactMinimumCoverPortfolio] {
        &self.portfolios
    }

    pub fn into_portfolios(self) -> Vec<ExactMinimumCoverPortfolio> {
        self.portfolios
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn known_alternative_count_decimal(&self) -> &str {
        &self.known_alternative_count_decimal
    }

    pub fn total_alternative_count_decimal(&self) -> Option<&str> {
        self.total_alternative_count_decimal.as_deref()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub const fn stop(&self) -> ExactMinimumCoverEnumerationStop {
        self.stop
    }

    pub const fn work_steps(&self) -> u64 {
        self.work_steps
    }

    pub fn restart(&self) -> Option<&ExactMinimumCoverRestart> {
        self.restart.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverPortfolioError {
    MinimumCover(ExactMinimumCoverError),
    RequiredPatternsNotCoverable {
        covered_pattern_count: u32,
        required_pattern_count: u32,
    },
    PageSizeMustBePositive,
    InvalidRestart,
    AllocationFailed {
        component: &'static str,
    },
}

/// A two-pass exact minimum-cover authority.
///
/// Construction first delegates to the existing branch-and-bound solver to
/// prove the minimum cardinality `k*`. Enumeration then scans the original row
/// identities, not the solver's dominated-row reduction, through every
/// `k*`-combination in numeric lexicographic order. Consequently equal and
/// dominated rows remain observable when they participate in an equally small
/// cover, and there is no semantic top-K cutoff.
#[derive(Clone, Debug)]
pub struct ExactMinimumCoverPortfolioEnumerator {
    input: Arc<ExactMinimumCoverPortfolioInput>,
    optimal_cardinality: usize,
    next_combination: Option<Vec<usize>>,
    known_alternative_count: DecimalCounter,
    enumeration_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactMinimumCoverPortfolioInput {
    row_words: Vec<Vec<u64>>,
    target_words: Vec<u64>,
}

impl ExactMinimumCoverPortfolioEnumerator {
    pub fn new(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let proof = exact_minimum_cover(required, rows)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Self::from_proof(required, rows, &proof)
    }

    fn from_proof(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        proof: &ExactMinimumCoverResult,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        if !proof.complete() {
            return Err(
                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                    covered_pattern_count: proof.covered_patterns().count_ones(),
                    required_pattern_count: required.count_ones(),
                },
            );
        }

        let mut target_words = Vec::new();
        target_words
            .try_reserve_exact(required.word_count())
            .map_err(|_| ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_portfolio_target",
            })?;
        for word_index in 0..required.word_count() {
            target_words.push(required.word_at(word_index));
        }

        let mut row_words = Vec::new();
        row_words.try_reserve_exact(rows.len()).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_portfolio_rows",
            }
        })?;
        for row in rows {
            let mut words = Vec::new();
            words
                .try_reserve_exact(required.word_count())
                .map_err(|_| ExactMinimumCoverPortfolioError::AllocationFailed {
                    component: "exact_minimum_cover_portfolio_row_words",
                })?;
            for word_index in 0..required.word_count() {
                words.push(row.word_at(word_index) & required.word_at(word_index));
            }
            row_words.push(words);
        }

        let optimal_cardinality = proof.row_indices().len();
        let next_combination = first_combination(rows.len(), optimal_cardinality);
        Ok(Self {
            input: Arc::new(ExactMinimumCoverPortfolioInput {
                row_words,
                target_words,
            }),
            optimal_cardinality,
            next_combination,
            known_alternative_count: DecimalCounter::zero(),
            enumeration_complete: false,
        })
    }

    /// Resumes a checkpoint against the same immutable semantic input. The
    /// minimum cardinality is reproved, so a checkpoint cannot silently change
    /// the optimization target. External snapshot integrity remains the
    /// responsibility of the layer that serializes this opaque value.
    pub fn resume(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        restart: ExactMinimumCoverRestart,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut enumerator = Self::new(required, rows)?;
        if restart.input.as_ref() != enumerator.input.as_ref()
            || restart.optimal_cardinality != enumerator.optimal_cardinality
            || !valid_restart_combination(
                restart.next_combination.as_deref(),
                restart.enumeration_complete,
                rows.len(),
                enumerator.optimal_cardinality,
            )
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        enumerator.next_combination = restart.next_combination;
        enumerator.known_alternative_count = restart.known_alternative_count;
        enumerator.enumeration_complete = restart.enumeration_complete;
        Ok(enumerator)
    }

    /// Reconstructs a restart from a persistence-safe field projection.
    ///
    /// The immutable semantic input and optimum are independently rebuilt and
    /// compared before any frontier state is accepted. Persistence layers must
    /// additionally bind these fields to their query/build/profile/candidate
    /// identities and authenticate the containing snapshot.
    pub fn resume_from_fields(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        optimal_cardinality: usize,
        next_combination: Option<Vec<usize>>,
        known_alternative_count_decimal: &str,
        enumeration_complete: bool,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut enumerator = Self::new(required, rows)?;
        if optimal_cardinality != enumerator.optimal_cardinality
            || !valid_restart_combination(
                next_combination.as_deref(),
                enumeration_complete,
                rows.len(),
                optimal_cardinality,
            )
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        let known_alternative_count = DecimalCounter::parse_canonical_bounded(
            known_alternative_count_decimal,
            maximum_subset_count_decimal_digits(rows.len()),
        )
        .ok_or(ExactMinimumCoverPortfolioError::InvalidRestart)?;
        enumerator.next_combination = next_combination;
        enumerator.known_alternative_count = known_alternative_count;
        enumerator.enumeration_complete = enumeration_complete;
        Ok(enumerator)
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.known_alternative_count.to_decimal_string()
    }

    /// Exact heap payload retained by this enumerator. This includes the
    /// immutable restart input shared by clones, the current combination, and
    /// the arbitrary-precision decimal counter. Callers that share a cloned
    /// enumerator must charge the immutable input graph only once.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.input.row_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<Vec<u64>>() as u128)?;
        for row in &self.input.row_words {
            bytes = bytes.checked_add(
                (row.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(
            (self.input.target_words.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        if let Some(combination) = &self.next_combination {
            bytes = bytes.checked_add(
                (combination.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(self.known_alternative_count.digits.capacity() as u128)?;
        Some(bytes)
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub fn restart_state(&self) -> Option<ExactMinimumCoverRestart> {
        (!self.enumeration_complete).then(|| ExactMinimumCoverRestart {
            input: Arc::clone(&self.input),
            optimal_cardinality: self.optimal_cardinality,
            next_combination: self.next_combination.clone(),
            known_alternative_count: self.known_alternative_count.clone(),
            enumeration_complete: self.enumeration_complete,
        })
    }

    /// Returns the next canonical portfolio without imposing a work cap. This
    /// is suitable for selecting the canonical first result after `k*` has
    /// already been proven. Interactive and distributed callers should use
    /// [`Self::next_page_with_control`] instead.
    pub fn next_portfolio(
        &mut self,
    ) -> Result<Option<ExactMinimumCoverPortfolio>, ExactMinimumCoverPortfolioError> {
        while let Some(combination) = self.next_combination.clone() {
            self.advance_frontier(&combination);
            if self.combination_covers(&combination) {
                self.known_alternative_count.increment()?;
                if self.next_combination.is_none() {
                    self.enumeration_complete = true;
                }
                return Ok(Some(ExactMinimumCoverPortfolio {
                    row_indices: combination,
                }));
            }
        }
        self.enumeration_complete = true;
        Ok(None)
    }

    pub fn next_page(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        self.next_page_with_control(page_size, max_work_steps, &mut || false)
    }

    /// Advances at most `max_work_steps` candidate combinations. The work
    /// budget bounds one call, never the semantic result set. A caller can
    /// resume indefinitely from the returned checkpoint until `Sealed`.
    pub fn next_page_with_control(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        if page_size == 0 {
            return Err(ExactMinimumCoverPortfolioError::PageSizeMustBePositive);
        }
        let mut portfolios = Vec::new();
        portfolios.try_reserve_exact(page_size).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_portfolio_page",
            }
        })?;
        let mut work_steps = 0_u64;
        let stop = loop {
            if self.enumeration_complete {
                break ExactMinimumCoverEnumerationStop::Sealed;
            }
            if cancelled() {
                break ExactMinimumCoverEnumerationStop::Cancelled;
            }
            if work_steps >= max_work_steps {
                break ExactMinimumCoverEnumerationStop::WorkBudgetExhausted;
            }
            let Some(combination) = self.next_combination.clone() else {
                self.enumeration_complete = true;
                break ExactMinimumCoverEnumerationStop::Sealed;
            };
            work_steps += 1;
            self.advance_frontier(&combination);
            if self.combination_covers(&combination) {
                self.known_alternative_count.increment()?;
                portfolios.push(ExactMinimumCoverPortfolio {
                    row_indices: combination,
                });
                if portfolios.len() == page_size {
                    if self.next_combination.is_none() {
                        self.enumeration_complete = true;
                        break ExactMinimumCoverEnumerationStop::Sealed;
                    }
                    break ExactMinimumCoverEnumerationStop::PageFull;
                }
            }
        };

        let known = self.known_alternative_count.to_decimal_string();
        let total = self.enumeration_complete.then(|| known.clone());
        let restart = self.restart_state();
        Ok(ExactMinimumCoverPortfolioPage {
            portfolios,
            optimal_cardinality: self.optimal_cardinality,
            known_alternative_count_decimal: known,
            total_alternative_count_decimal: total,
            enumeration_complete: self.enumeration_complete,
            stop,
            work_steps,
            restart,
        })
    }

    fn advance_frontier(&mut self, current: &[usize]) {
        let mut next = current.to_vec();
        self.next_combination =
            advance_combination(&mut next, self.input.row_words.len()).then_some(next);
    }

    fn combination_covers(&self, combination: &[usize]) -> bool {
        (0..self.input.target_words.len()).all(|word_index| {
            let covered = combination.iter().fold(0_u64, |covered, row_index| {
                covered | self.input.row_words[*row_index][word_index]
            });
            covered & self.input.target_words[word_index] == self.input.target_words[word_index]
        })
    }
}

fn first_combination(row_count: usize, cardinality: usize) -> Option<Vec<usize>> {
    (cardinality <= row_count).then(|| (0..cardinality).collect())
}

fn advance_combination(combination: &mut [usize], row_count: usize) -> bool {
    let cardinality = combination.len();
    for index in (0..cardinality).rev() {
        let maximum = row_count - cardinality + index;
        if combination[index] < maximum {
            combination[index] += 1;
            for suffix in index + 1..cardinality {
                combination[suffix] = combination[suffix - 1] + 1;
            }
            return true;
        }
    }
    false
}

fn valid_restart_combination(
    combination: Option<&[usize]>,
    complete: bool,
    row_count: usize,
    cardinality: usize,
) -> bool {
    if complete {
        return combination.is_none();
    }
    let Some(combination) = combination else {
        return false;
    };
    combination.len() == cardinality
        && combination.iter().all(|index| *index < row_count)
        && combination.windows(2).all(|pair| pair[0] < pair[1])
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DecimalCounter {
    // Little-endian base-10 digits make increment exact without a fixed-width
    // integer ceiling. The public representation is canonical decimal text.
    digits: Vec<u8>,
}

impl DecimalCounter {
    fn zero() -> Self {
        Self { digits: vec![0] }
    }

    fn parse_canonical_bounded(value: &str, maximum_digits: usize) -> Option<Self> {
        if value.is_empty()
            || value.len() > maximum_digits.max(1)
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let mut digits = Vec::new();
        digits.try_reserve_exact(value.len()).ok()?;
        digits.extend(value.bytes().rev().map(|byte| byte - b'0'));
        Some(Self { digits })
    }

    fn increment(&mut self) -> Result<(), ExactMinimumCoverPortfolioError> {
        for digit in &mut self.digits {
            if *digit < 9 {
                *digit += 1;
                return Ok(());
            }
            *digit = 0;
        }
        self.digits.try_reserve_exact(1).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_alternative_count",
            }
        })?;
        self.digits.push(1);
        Ok(())
    }

    fn to_decimal_string(&self) -> String {
        self.digits
            .iter()
            .rev()
            .map(|digit| char::from(b'0' + *digit))
            .collect()
    }
}

fn maximum_subset_count_decimal_digits(row_count: usize) -> usize {
    // Every portfolio is one subset, so the exact count cannot exceed 2^n.
    // ceil(n * log10(2)) + one guard digit, using an integer upper bound for
    // log10(2). This also bounds hostile persisted decimal allocation.
    row_count
        .saturating_mul(30_104)
        .div_ceil(100_000)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use crate::pattern::pattern_id::PatternId;

    use super::*;

    fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
        PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
            .expect("valid bitset")
    }

    fn row_vectors(page: &ExactMinimumCoverPortfolioPage) -> Vec<Vec<usize>> {
        page.portfolios()
            .iter()
            .map(|portfolio| portfolio.row_indices().to_vec())
            .collect()
    }

    #[test]
    fn equal_rows_remain_distinct_exact_alternatives() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let first = enumerator.next_page(1, 1).expect("first page");
        assert_eq!(row_vectors(&first), vec![vec![0]]);
        assert_eq!(first.known_alternative_count_decimal(), "1");
        assert_eq!(first.total_alternative_count_decimal(), None);
        assert_eq!(first.stop(), ExactMinimumCoverEnumerationStop::PageFull);
        assert!(!first.enumeration_complete());

        let second = enumerator.next_page(1, 1).expect("second page");
        assert_eq!(row_vectors(&second), vec![vec![1]]);
        assert_eq!(second.known_alternative_count_decimal(), "2");
        assert_eq!(second.total_alternative_count_decimal(), Some("2"));
        assert_eq!(second.stop(), ExactMinimumCoverEnumerationStop::Sealed);
        assert!(second.enumeration_complete());
    }

    #[test]
    fn dominated_original_row_identity_can_participate_in_an_optimal_cover() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(10, 10).expect("all alternatives");

        assert_eq!(page.optimal_cardinality(), 2);
        assert_eq!(row_vectors(&page), vec![vec![0, 2], vec![1, 2]]);
        assert_eq!(page.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn portfolios_are_numeric_lexicographic_and_not_search_ordered() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![
            bitset(2, &[0]),
            bitset(2, &[1]),
            bitset(2, &[0]),
            bitset(2, &[1]),
        ];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(10, 20).expect("all alternatives");

        assert_eq!(
            row_vectors(&page),
            vec![vec![0, 1], vec![0, 3], vec![1, 2], vec![2, 3]]
        );
        assert!(page.enumeration_complete());
    }

    #[test]
    fn work_budget_and_cancellation_return_exact_restart_state() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[0]), bitset(2, &[1])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let budgeted = enumerator.next_page(2, 1).expect("budgeted page");
        assert!(budgeted.portfolios().is_empty());
        assert_eq!(
            budgeted.stop(),
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
        );
        let restart = budgeted.restart().expect("restart").clone();

        let mut resumed = ExactMinimumCoverPortfolioEnumerator::resume(&required, &rows, restart)
            .expect("valid restart");
        let cancelled = resumed
            .next_page_with_control(2, 10, &mut || true)
            .expect("cancelled page");
        assert_eq!(
            cancelled.stop(),
            ExactMinimumCoverEnumerationStop::Cancelled
        );
        assert!(cancelled.portfolios().is_empty());

        let completed = resumed.next_page(10, 10).expect("resumed page");
        assert_eq!(row_vectors(&completed), vec![vec![0, 2], vec![1, 2]]);
        assert_eq!(completed.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn empty_required_set_has_one_empty_exact_portfolio() {
        let required = bitset(2, &[]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[1])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(1, 1).expect("empty cover page");

        assert_eq!(row_vectors(&page), vec![Vec::<usize>::new()]);
        assert_eq!(page.total_alternative_count_decimal(), Some("1"));
    }

    #[test]
    fn incomplete_cover_is_rejected_instead_of_claiming_all_alternatives() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0])];

        assert!(matches!(
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows),
            Err(
                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                    covered_pattern_count: 1,
                    required_pattern_count: 2,
                }
            )
        ));
    }

    #[test]
    fn restart_is_fieldwise_bound_to_the_original_required_and_rows() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[1]), bitset(2, &[0, 1])];
        let mut original =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let checkpoint = original
            .next_page(1, 0)
            .expect("checkpoint page")
            .restart()
            .expect("restart")
            .clone();
        let changed_rows = vec![bitset(2, &[1]), bitset(2, &[0]), bitset(2, &[0, 1])];

        assert!(matches!(
            ExactMinimumCoverPortfolioEnumerator::resume(&required, &changed_rows, checkpoint),
            Err(ExactMinimumCoverPortfolioError::InvalidRestart)
        ));
    }

    #[test]
    fn persistence_fields_resume_without_serializing_private_input_owners() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];
        let mut original =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let first = original.next_page(1, 1).expect("first page");
        let restart = first.restart().expect("restart");

        let mut resumed = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
            &required,
            &rows,
            restart.optimal_cardinality(),
            restart.next_combination().map(ToOwned::to_owned),
            &restart.known_alternative_count_decimal(),
            restart.enumeration_complete(),
        )
        .expect("fieldwise resume");
        let second = resumed.next_page(1, 1).expect("second page");

        assert_eq!(row_vectors(&second), vec![vec![1]]);
        assert_eq!(second.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn persistence_fields_reject_noncanonical_or_unbound_state() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];

        for result in [
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                2,
                Some(vec![1]),
                "1",
                false,
            ),
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                1,
                Some(vec![1]),
                "01",
                false,
            ),
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                1,
                Some(vec![1]),
                "999",
                false,
            ),
        ] {
            assert!(matches!(
                result,
                Err(ExactMinimumCoverPortfolioError::InvalidRestart)
            ));
        }
    }

    #[test]
    fn progressive_count_has_no_fixed_width_integer_ceiling() {
        let mut count = DecimalCounter {
            digits: vec![9; 40],
        };

        count.increment().expect("grow decimal counter");

        assert_eq!(count.to_decimal_string(), format!("1{}", "0".repeat(40)));
    }
}
