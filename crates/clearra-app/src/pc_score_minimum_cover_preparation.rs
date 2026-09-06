// SRP rationale: one change reason is the resumable, memory-admitted sealing
// of score-only PC portfolio evidence. Source eligibility remains validated
// by the score contract; the common portfolio owner alone proves and selects.

use std::{fmt, mem::size_of};

use clearra_coverage::cover::ExactMinimumCoverError;

use super::*;
use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSetPreparation, CoveragePortfolioAlternativeSetPreparationAdvance,
};

type Error = PcScorePortfolioValidationError;

/// Source construction charges actual Vec/String capacities. Temporary
/// buffers remain charged until construction ends, a conservative peak that
/// avoids relying on private BTreeMap node layouts or allocator growth rules.
pub(super) struct ScorePortfolioSourceMemory<'a> {
    live: u128,
    guard: &'a mut dyn FnMut(u128) -> Result<(), ExactMinimumCoverError>,
}

impl ScorePortfolioSourceMemory<'_> {
    pub(super) fn release_vec<T>(&mut self, values: Vec<T>) -> Result<(), Error> {
        let bytes = (values.capacity() as u128)
            .checked_mul(size_of::<T>() as u128)
            .ok_or(Error::MemoryProjectionOverflow)?;
        drop(values);
        self.live = self
            .live
            .checked_sub(bytes)
            .ok_or(Error::MemoryProjectionOverflow)?;
        Ok(())
    }
    pub(super) fn charge(&mut self, extra: u128) -> Result<(), Error> {
        self.live = self
            .live
            .checked_add(extra)
            .ok_or(Error::MemoryProjectionOverflow)?;
        (self.guard)(self.live).map_err(|_| Error::MemoryLimitExceeded)
    }

    pub(super) fn vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Error> {
        let requested = (capacity as u128)
            .checked_mul(size_of::<T>() as u128)
            .ok_or(Error::MemoryProjectionOverflow)?;
        let before = self.live;
        self.charge(requested)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| Error::MemoryLimitExceeded)?;
        self.live = before;
        self.charge(
            (values.capacity() as u128)
                .checked_mul(size_of::<T>() as u128)
                .ok_or(Error::MemoryProjectionOverflow)?,
        )?;
        Ok(values)
    }

    pub(super) fn string(&mut self, value: &str) -> Result<String, Error> {
        self.format(|output| output.write_str(value))
    }

    pub(super) fn canonical_key(
        &mut self,
        identity: StandardBoard64TilingIdentity,
    ) -> Result<String, Error> {
        self.format(|mut output| identity.write_canonical(&mut output))
    }

    pub(super) fn format(
        &mut self,
        write: impl Fn(&mut dyn fmt::Write) -> fmt::Result,
    ) -> Result<String, Error> {
        struct Counter(usize);
        impl fmt::Write for Counter {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.0 = self.0.checked_add(value.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut counter = Counter(0);
        write(&mut counter).map_err(|_| Error::MemoryProjectionOverflow)?;
        let before = self.live;
        self.charge(counter.0 as u128)?;
        let mut value = String::new();
        value
            .try_reserve_exact(counter.0)
            .map_err(|_| Error::MemoryLimitExceeded)?;
        self.live = before;
        self.charge(value.capacity() as u128)?;
        write(&mut value).map_err(|_| Error::MemoryProjectionOverflow)?;
        if value.len() != counter.0 {
            return Err(Error::MemoryProjectionOverflow);
        }
        Ok(value)
    }
}

pub(super) struct PcScorePortfolioProjection {
    pub summary: PcScoreSummaryV2Result,
    pub pattern_best_scores: Vec<u64>,
    pub pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    pub eligible_candidates: Vec<PcScoreEligibleCandidateV2>,
    pub eligible_candidate_map_sha256: String,
    pub score_eligibility_sha256: String,
    pub candidate_identities: Vec<(u64, StandardBoard64TilingIdentity)>,
}

impl PcScorePortfolioProjection {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .summary
            .checked_retained_capacity_bytes()?
            .checked_add(
                (self.pattern_best_scores.capacity() as u128)
                    .checked_mul(size_of::<u64>() as u128)?,
            )?
            .checked_add(
                (size_of::<Vec<PcScorePatternWinnerV1>>() + 2 * size_of::<usize>()) as u128,
            )?
            .checked_add(
                (self.pattern_winners.capacity() as u128)
                    .checked_mul(size_of::<PcScorePatternWinnerV1>() as u128)?,
            )?
            .checked_add(
                (self.eligible_candidates.capacity() as u128)
                    .checked_mul(size_of::<PcScoreEligibleCandidateV2>() as u128)?,
            )?
            .checked_add(self.eligible_candidate_map_sha256.capacity() as u128)?
            .checked_add(self.score_eligibility_sha256.capacity() as u128)?
            .checked_add(
                (self.candidate_identities.capacity() as u128).checked_mul(size_of::<(
                    u64,
                    StandardBoard64TilingIdentity,
                )>()
                    as u128)?,
            )?;
        for candidate in &self.eligible_candidates {
            bytes = bytes
                .checked_add(candidate.normalized_solution_key.capacity() as u128)?
                .checked_add(
                    (candidate.eligible_patterns.capacity() as u128).checked_mul(size_of::<
                        PcScoreEligiblePatternV2,
                    >(
                    )
                        as u128)?,
                )?;
        }
        Some(bytes)
    }

    fn checked_completion_extra_bytes(&self) -> Option<u128> {
        // Public ID validation holds its input, sorted clone and Arc copy;
        // canonical keys hold borrowed keys plus owned String entries. The
        // final eligible Vec->Arc and report/portfolio Arc conversions may
        // coexist with their source values. Every bound uses the full source,
        // never assumes that a known optimum or short first page is supplied.
        let count = self.eligible_candidates.len() as u128;
        let mut bytes = count.checked_mul(
            (size_of::<PcScoreEligibleCandidateV2>()
                + 4 * size_of::<String>()
                + 6 * size_of::<u64>()
                + size_of::<&str>()) as u128,
        )?;
        for candidate in &self.eligible_candidates {
            bytes = bytes
                .checked_add((candidate.normalized_solution_key.len() as u128).checked_mul(3)?)?;
        }
        bytes
            .checked_add(self.summary.checked_retained_capacity_bytes()?)?
            .checked_add(
                (2 * size_of::<PcScorePortfolioV2Result>()
                    + 2 * size_of::<CoveragePortfolioAlternativeSet>()
                    + 16 * size_of::<usize>()) as u128,
            )
    }
}

struct PcScorePortfolioPreparation {
    projection: Option<PcScorePortfolioProjection>,
    portfolio: CoveragePortfolioAlternativeSetPreparation,
}

// Completed evidence moves inline; the completion guard includes this carrier
// and must not acquire another unaccounted box merely to shrink the enum.
#[allow(clippy::large_enum_variant)]
enum ScoreReportAdvance {
    Pending { work_steps: u64 },
    Completed(PcScorePortfolioV2Result),
    Cancelled { work_steps: u64 },
}

impl PcScorePortfolioPreparation {
    fn new(
        summary: &PcScoreSummaryV2Result,
        derivation: &PcScoreDerivation,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, Error> {
        let shared = summary
            .checked_retained_capacity_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    (size_of::<Vec<PcScorePatternWinnerV1>>() + 2 * size_of::<usize>()) as u128,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (derivation.pattern_winner_owner().capacity() as u128)
                        .checked_mul(size_of::<PcScorePatternWinnerV1>() as u128)?,
                )
            })
            .and_then(|bytes| bytes.checked_add(size_of::<Self>() as u128))
            .ok_or(Error::MemoryProjectionOverflow)?;
        guard(shared).map_err(|_| Error::MemoryLimitExceeded)?;
        let (projection, identity, keys, required, rows) = prepare_pc_score_portfolio_input(
            summary,
            derivation,
            &mut ScorePortfolioSourceMemory {
                live: shared,
                guard,
            },
        )?;
        let outer = projection
            .checked_retained_capacity_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    (size_of::<Self>() - size_of::<CoveragePortfolioAlternativeSetPreparation>())
                        as u128,
                )
            })
            .ok_or(Error::MemoryProjectionOverflow)?;
        let portfolio = CoveragePortfolioAlternativeSetPreparation::new_with_memory_guard(
            identity,
            keys,
            required,
            rows,
            &mut |peak| {
                guard(
                    outer
                        .checked_add(peak)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )
            },
        )
        .map_err(|_| Error::PortfolioAlternativeSetInvalid)?;
        let preparation = Self {
            projection: Some(projection),
            portfolio,
        };
        guard(
            (size_of::<Self>() as u128)
                .checked_add(
                    preparation
                        .checked_retained_capacity_bytes()
                        .ok_or(Error::MemoryProjectionOverflow)?,
                )
                .ok_or(Error::MemoryProjectionOverflow)?,
        )
        .map_err(|_| Error::MemoryLimitExceeded)?;
        Ok(preparation)
    }

    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.portfolio
            .checked_retained_capacity_bytes()?
            .checked_add(self.projection.as_ref().map_or(
                Some(0),
                PcScorePortfolioProjection::checked_retained_capacity_bytes,
            )?)
    }

    fn advance(
        &mut self,
        work: u64,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ScoreReportAdvance, Error> {
        let outer = self
            .projection
            .as_ref()
            .map_or(
                Some(0),
                PcScorePortfolioProjection::checked_retained_capacity_bytes,
            )
            .and_then(|bytes| {
                bytes.checked_add(
                    (size_of::<Self>() - size_of::<CoveragePortfolioAlternativeSetPreparation>())
                        as u128,
                )
            })
            .ok_or(Error::MemoryProjectionOverflow)?;
        match self
            .portfolio
            .advance_with_memory_guard(
                work,
                &mut |peak| {
                    guard(
                        outer
                            .checked_add(peak)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(|_| Error::PortfolioAlternativeSetInvalid)?
        {
            CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                Ok(ScoreReportAdvance::Pending { work_steps })
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps } => {
                self.projection = None;
                Ok(ScoreReportAdvance::Cancelled { work_steps })
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Completed(portfolio) => {
                let projection = self
                    .projection
                    .take()
                    .ok_or(Error::PortfolioAlternativeSetInvalid)?;
                let baseline = (size_of::<Self>() as u128)
                    .checked_add(
                        self.portfolio
                            .checked_retained_capacity_bytes()
                            .ok_or(Error::MemoryProjectionOverflow)?,
                    )
                    .ok_or(Error::MemoryProjectionOverflow)?;
                let peak = baseline
                    .checked_add(
                        projection
                            .checked_retained_capacity_bytes()
                            .ok_or(Error::MemoryProjectionOverflow)?,
                    )
                    .and_then(|bytes| {
                        bytes.checked_add(portfolio.checked_retained_capacity_bytes()?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(projection.checked_completion_extra_bytes()?)
                    })
                    .ok_or(Error::MemoryProjectionOverflow)?;
                guard(peak).map_err(|_| Error::MemoryLimitExceeded)?;
                let report = finish_pc_score_portfolio_result(projection, portfolio)?;
                guard(
                    baseline
                        .checked_add(
                            checked_report_bytes(&report).ok_or(Error::MemoryProjectionOverflow)?,
                        )
                        .ok_or(Error::MemoryProjectionOverflow)?,
                )
                .map_err(|_| Error::MemoryLimitExceeded)?;
                Ok(ScoreReportAdvance::Completed(report))
            }
        }
    }
}

fn checked_report_bytes(report: &PcScorePortfolioV2Result) -> Option<u128> {
    report.checked_retained_capacity_bytes()?.checked_add(
        (size_of::<PcScorePortfolioV2Result>()
            + size_of::<CoveragePortfolioAlternativeSet>()
            + size_of::<Vec<PcScorePatternWinnerV1>>()
            + 12 * size_of::<usize>()) as u128,
    )
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum PcScorePortfolioPreparationAdvance {
    Pending { work_steps: u64 },
    Completed(ValidatedPcScorePortfolioExecutionEvidence),
    Cancelled { work_steps: u64 },
}

pub(crate) struct PcScorePortfolioExecutionPreparation {
    score_execution: Option<ValidatedPcScoreExecutionEvidence>,
    preparation: PcScorePortfolioPreparation,
}

impl PcScorePortfolioExecutionPreparation {
    pub(crate) fn new(
        score_execution: ValidatedPcScoreExecutionEvidence,
        derivation: &PcScoreDerivation,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, Error> {
        let outer = score_execution
            .report()
            .checked_retained_capacity_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    (size_of::<Self>() - size_of::<PcScorePortfolioPreparation>()) as u128,
                )
            })
            .ok_or(Error::MemoryProjectionOverflow)?;
        let preparation =
            PcScorePortfolioPreparation::new(score_execution.report(), derivation, &mut |peak| {
                guard(
                    outer
                        .checked_add(peak)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )
            })?;
        Ok(Self {
            score_execution: Some(score_execution),
            preparation,
        })
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        self.score_execution
            .as_ref()
            .is_some_and(|evidence| evidence.matches_core_result(result))
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.preparation
            .checked_retained_capacity_bytes()?
            .checked_add(self.score_execution.as_ref().map_or(Some(0), |evidence| {
                evidence.report().checked_retained_capacity_bytes()
            })?)
    }

    pub(crate) fn parallel_work(&self) -> &CoveragePortfolioAlternativeSetPreparation {
        &self.preparation.portfolio
    }
    pub(crate) fn parallel_work_mut(&mut self) -> &mut CoveragePortfolioAlternativeSetPreparation {
        &mut self.preparation.portfolio
    }

    pub(crate) fn advance_with_memory_guard(
        &mut self,
        work: u64,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PcScorePortfolioPreparationAdvance, Error> {
        let outer = self
            .score_execution
            .as_ref()
            .map_or(Some(0), |evidence| {
                evidence.report().checked_retained_capacity_bytes()
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (size_of::<Self>() - size_of::<PcScorePortfolioPreparation>()) as u128,
                )
            })
            .ok_or(Error::MemoryProjectionOverflow)?;
        match self.preparation.advance(
            work,
            &mut |peak| {
                guard(
                    outer
                        .checked_add(peak)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )
            },
            cancelled,
        )? {
            ScoreReportAdvance::Pending { work_steps } => {
                Ok(PcScorePortfolioPreparationAdvance::Pending { work_steps })
            }
            ScoreReportAdvance::Cancelled { work_steps } => {
                self.score_execution = None;
                Ok(PcScorePortfolioPreparationAdvance::Cancelled { work_steps })
            }
            ScoreReportAdvance::Completed(report) => {
                let peak = self
                    .checked_retained_capacity_bytes()
                    .and_then(|bytes| bytes.checked_add(size_of::<Self>() as u128))
                    .and_then(|bytes| bytes.checked_add(checked_report_bytes(&report)?))
                    .and_then(|bytes| {
                        bytes.checked_add(
                            (size_of::<PcScorePortfolioV2Result>() + 2 * size_of::<usize>())
                                as u128,
                        )
                    })
                    .ok_or(Error::MemoryProjectionOverflow)?;
                guard(peak).map_err(|_| Error::MemoryLimitExceeded)?;
                let score_execution = self
                    .score_execution
                    .take()
                    .ok_or(Error::PortfolioAlternativeSetInvalid)?;
                Ok(PcScorePortfolioPreparationAdvance::Completed(
                    ValidatedPcScorePortfolioExecutionEvidence {
                        score_execution,
                        report: Arc::new(report),
                    },
                ))
            }
        }
    }
}

#[cfg(test)]
pub(super) fn complete_score_portfolio(
    summary: &PcScoreSummaryV2Result,
    derivation: &PcScoreDerivation,
) -> Result<PcScorePortfolioV2Result, Error> {
    let mut preparation = PcScorePortfolioPreparation::new(summary, derivation, &mut |_| Ok(()))?;
    loop {
        match preparation.advance(u64::MAX, &mut |_| Ok(()), &mut || false)? {
            ScoreReportAdvance::Pending { work_steps: 0 } => {
                return Err(Error::ExactMinimumCoverFailed)
            }
            ScoreReportAdvance::Pending { .. } => {}
            ScoreReportAdvance::Completed(report) => return Ok(report),
            ScoreReportAdvance::Cancelled { .. } => return Err(Error::Cancelled),
        }
    }
}
