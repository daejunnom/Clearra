use clearra_coverage::{
    cover::{
        ExactMinimumCoverEnumerationStop, ExactMinimumCoverPortfolioEnumerator,
        ExactMinimumCoverPortfolioError, ExactMinimumCoverRestart,
    },
    pattern::pattern_bitset::PatternBitSet,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaxScoreCoverPortfolio {
    candidate_ids: Vec<usize>,
}

impl MaxScoreCoverPortfolio {
    pub fn candidate_ids(&self) -> &[usize] {
        &self.candidate_ids
    }

    pub fn into_candidate_ids(self) -> Vec<usize> {
        self.candidate_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaxScoreCoverPortfolioRestart {
    candidate_ids: Vec<usize>,
    coverage_restart: ExactMinimumCoverRestart,
}

impl MaxScoreCoverPortfolioRestart {
    pub fn candidate_ids(&self) -> &[usize] {
        &self.candidate_ids
    }

    pub fn coverage_restart(&self) -> &ExactMinimumCoverRestart {
        &self.coverage_restart
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaxScoreCoverPortfolioPage {
    portfolios: Vec<MaxScoreCoverPortfolio>,
    optimal_cardinality: usize,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
    stop: ExactMinimumCoverEnumerationStop,
    work_steps: u64,
    restart: Option<MaxScoreCoverPortfolioRestart>,
}

impl MaxScoreCoverPortfolioPage {
    pub fn portfolios(&self) -> &[MaxScoreCoverPortfolio] {
        &self.portfolios
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

    pub fn restart(&self) -> Option<&MaxScoreCoverPortfolioRestart> {
        self.restart.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct MaxScoreCoverPortfolioEnumerator {
    candidate_ids: Vec<usize>,
    coverage: ExactMinimumCoverPortfolioEnumerator,
}

impl MaxScoreCoverPortfolioEnumerator {
    pub(crate) fn new(
        candidate_ids: Vec<usize>,
        rows: &[PatternBitSet],
        required: &PatternBitSet,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        debug_assert_eq!(candidate_ids.len(), rows.len());
        Ok(Self {
            candidate_ids,
            coverage: ExactMinimumCoverPortfolioEnumerator::new(required, rows)?,
        })
    }

    pub(crate) fn resume(
        candidate_ids: Vec<usize>,
        rows: &[PatternBitSet],
        required: &PatternBitSet,
        restart: MaxScoreCoverPortfolioRestart,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        if candidate_ids != restart.candidate_ids {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        Ok(Self {
            candidate_ids,
            coverage: ExactMinimumCoverPortfolioEnumerator::resume(
                required,
                rows,
                restart.coverage_restart,
            )?,
        })
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.coverage.optimal_cardinality()
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.coverage.known_alternative_count_decimal()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.coverage.enumeration_complete()
    }

    pub fn restart_state(&self) -> Option<MaxScoreCoverPortfolioRestart> {
        self.coverage
            .restart_state()
            .map(|coverage_restart| MaxScoreCoverPortfolioRestart {
                candidate_ids: self.candidate_ids.clone(),
                coverage_restart,
            })
    }

    pub fn next_portfolio(
        &mut self,
    ) -> Result<Option<MaxScoreCoverPortfolio>, ExactMinimumCoverPortfolioError> {
        self.coverage
            .next_portfolio()
            .map(|portfolio| portfolio.map(|portfolio| self.map_portfolio(portfolio.row_indices())))
    }

    pub fn next_page(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
    ) -> Result<MaxScoreCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        self.next_page_with_control(page_size, max_work_steps, &mut || false)
    }

    pub fn next_page_with_control(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<MaxScoreCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        let page = self
            .coverage
            .next_page_with_control(page_size, max_work_steps, cancelled)?;
        let portfolios = page
            .portfolios()
            .iter()
            .map(|portfolio| self.map_portfolio(portfolio.row_indices()))
            .collect();
        let restart = page
            .restart()
            .map(|coverage_restart| MaxScoreCoverPortfolioRestart {
                candidate_ids: self.candidate_ids.clone(),
                coverage_restart: coverage_restart.clone(),
            });
        Ok(MaxScoreCoverPortfolioPage {
            portfolios,
            optimal_cardinality: page.optimal_cardinality(),
            known_alternative_count_decimal: page.known_alternative_count_decimal().to_owned(),
            total_alternative_count_decimal: page
                .total_alternative_count_decimal()
                .map(str::to_owned),
            enumeration_complete: page.enumeration_complete(),
            stop: page.stop(),
            work_steps: page.work_steps(),
            restart,
        })
    }

    fn map_portfolio(&self, row_indices: &[usize]) -> MaxScoreCoverPortfolio {
        MaxScoreCoverPortfolio {
            candidate_ids: row_indices
                .iter()
                .map(|row_index| self.candidate_ids[*row_index])
                .collect(),
        }
    }
}
