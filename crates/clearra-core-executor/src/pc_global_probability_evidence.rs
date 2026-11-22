//! Producer-owned completion authority for global PC probability.
//!
//! A successful original queue is a monotone positive fact. Covering the whole
//! finite source proves probability one without proving a complete solution
//! family. Candidate-domain exhaustion is the separate authority needed for an
//! exact probability below one. Neither authority is a CountUnique certificate.

use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{PcCountPolicy, PcSolutionProbabilityPolicy};
use clearra_problem::{SearchOutputPolicy, SearchProblem};
use clearra_supply::QueueObservationPolicy;

use crate::pc_chance_coverage_evidence::{
    PcChanceCoverageEvidence, PcChanceProblemEvidence, PcChanceProblemEvidenceError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcGlobalProbabilityCompletion {
    /// Every ordinal in the complete original source has a verified legal PC.
    AllOriginalQueuesCovered,
    /// The producer exhausted the candidate domain and joined all its workers.
    CandidateDomainExhausted,
}

impl PcGlobalProbabilityCompletion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllOriginalQueuesCovered => "all-original-queues-covered",
            Self::CandidateDomainExhausted => "candidate-domain-exhausted",
        }
    }

    pub const fn enumeration_complete(self) -> bool {
        matches!(self, Self::CandidateDomainExhausted)
    }
}

/// An immutable aggregate bound to every normalized executed-problem field.
/// Only Core producers can construct or attach this authority. Public scalar
/// fields, a probability rounded to one, or a foreign coverage bitmap cannot
/// create it. Cloning retains the immutable PatternBitSet backing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcGlobalProbabilityEvidence {
    problem: PcChanceProblemEvidence,
    coverage: PatternBitSet,
    covered_pattern_count: usize,
    completion: PcGlobalProbabilityCompletion,
}

impl PcGlobalProbabilityEvidence {
    /// The caller must publish only verified positive facts and must join its
    /// workers without error/cancellation before sealing. Exhaustion requires
    /// complete candidate-domain authority, not a timeout or an aborted queue.
    pub(crate) fn from_executed_problem(
        problem: &SearchProblem,
        coverage: PatternBitSet,
        completion: PcGlobalProbabilityCompletion,
    ) -> Result<Self, PcGlobalProbabilityEvidenceError> {
        if !problem
            .pc_chance_evidence_policy()
            .retains_pc_global_probability_evidence()
            || problem.output_policy() != SearchOutputPolicy::CoverageSummary
            || problem.goal().as_str() != "clear-to-empty"
            || problem.objective() != ObjectivePolicy::unique()
            || problem.count_policy() != PcCountPolicy::CountUnique
            || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
            || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
            || problem.allowed_colored_solution_identities().is_some()
        {
            return Err(PcGlobalProbabilityEvidenceError::UnsupportedPurpose);
        }
        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .ok_or(PcGlobalProbabilityEvidenceError::IncompleteSource)?;
        let pattern_count = universe.pattern_count();
        if !source.complete()
            || source.truncation_reason().is_some()
            || !universe.complete()
            || universe.truncation_reason().is_some()
            || pattern_count == 0
            || universe.total_possible_pattern_count() != pattern_count as u128
            || universe.weights().len() != pattern_count
            || universe.materialized_probability_mass().get().to_bits() != 1_f64.to_bits()
            || universe.weights().total_weight().get().to_bits() != 1_f64.to_bits()
        {
            return Err(PcGlobalProbabilityEvidenceError::IncompleteSource);
        }
        if coverage.pattern_count() != pattern_count {
            return Err(PcGlobalProbabilityEvidenceError::CoverageUniverseMismatch);
        }
        let covered_pattern_count = (0..coverage.word_count())
            .try_fold(0_usize, |count, index| {
                count.checked_add(coverage.word_at(index).count_ones() as usize)
            })
            .ok_or(PcGlobalProbabilityEvidenceError::CoverageUniverseMismatch)?;
        if covered_pattern_count > pattern_count {
            return Err(PcGlobalProbabilityEvidenceError::CoverageUniverseMismatch);
        }
        if completion == PcGlobalProbabilityCompletion::AllOriginalQueuesCovered
            && covered_pattern_count != pattern_count
        {
            return Err(PcGlobalProbabilityEvidenceError::OriginalQueuesRemainUncovered);
        }
        let problem = PcChanceProblemEvidence::from_global_probability_search_problem(problem)
            .map_err(PcGlobalProbabilityEvidenceError::Problem)?;
        Ok(Self {
            problem,
            coverage,
            covered_pattern_count,
            completion,
        })
    }

    /// Conservative credit for creating the fieldwise source snapshot. The
    /// already-owned coverage bitset is moved in and accounted by its storage
    /// component; this projection never charges one dense row per candidate.
    pub(crate) fn checked_creation_future_bytes(problem: &SearchProblem) -> Option<u128> {
        PcChanceCoverageEvidence::checked_pc_family_creation_future_bytes(problem, 0)?
            .checked_add(core::mem::size_of::<Self>() as u128)
    }

    pub fn problem(&self) -> &PcChanceProblemEvidence {
        &self.problem
    }

    pub fn coverage(&self) -> &PatternBitSet {
        &self.coverage
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub const fn completion(&self) -> PcGlobalProbabilityCompletion {
        self.completion
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.completion.enumeration_complete()
    }

    pub(crate) fn checked_non_pattern_storage_retained_bytes(&self) -> Option<u128> {
        self.problem.checked_storage_retained_bytes()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcGlobalProbabilityEvidenceError {
    UnsupportedPurpose,
    IncompleteSource,
    CoverageUniverseMismatch,
    OriginalQueuesRemainUncovered,
    Problem(PcChanceProblemEvidenceError),
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::builtin_rules::{srs, srs_plus};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    fn query() -> PcScenarioQuery {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_rule(srs_plus())
    }

    fn problem(query: &PcScenarioQuery) -> SearchProblem {
        ProblemCompiler::compile_scenario_percent(query)
            .unwrap()
            .with_pc_global_probability_v1_evidence()
    }

    #[test]
    fn full_source_proof_and_exhausted_domain_have_distinct_completeness() {
        let problem = problem(&query());
        let full = PcGlobalProbabilityEvidence::from_executed_problem(
            &problem,
            PatternBitSet::all(1),
            PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
        )
        .unwrap();
        assert_eq!(full.covered_pattern_count(), 1);
        assert!(!full.enumeration_complete());
        assert_eq!(
            PcGlobalProbabilityEvidence::from_executed_problem(
                &problem,
                PatternBitSet::new(1),
                PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
            ),
            Err(PcGlobalProbabilityEvidenceError::OriginalQueuesRemainUncovered)
        );
        let exhausted = PcGlobalProbabilityEvidence::from_executed_problem(
            &problem,
            PatternBitSet::new(1),
            PcGlobalProbabilityCompletion::CandidateDomainExhausted,
        )
        .unwrap();
        assert!(exhausted.enumeration_complete());
        assert_eq!(exhausted.covered_pattern_count(), 0);
    }

    #[test]
    fn summary_rows_and_visible_seven_cannot_borrow_global_authority() {
        let bare = ProblemCompiler::compile_scenario_percent(&query()).unwrap();
        for foreign in [bare.clone(), bare.with_pc_chance_probability_v2_evidence()] {
            assert_eq!(
                PcGlobalProbabilityEvidence::from_executed_problem(
                    &foreign,
                    PatternBitSet::all(1),
                    PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
                ),
                Err(PcGlobalProbabilityEvidenceError::UnsupportedPurpose)
            );
        }
        let visible =
            problem(&query().with_queue_observation_policy(QueueObservationPolicy::VisibleSeven));
        assert_eq!(
            PcGlobalProbabilityEvidence::from_executed_problem(
                &visible,
                PatternBitSet::all(1),
                PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
            ),
            Err(PcGlobalProbabilityEvidenceError::UnsupportedPurpose)
        );
    }

    #[test]
    fn global_proof_binds_the_executed_rule_and_original_universe() {
        let expected = problem(&query());
        let foreign_rule = problem(&query().with_rule(srs()));
        let evidence = PcGlobalProbabilityEvidence::from_executed_problem(
            &expected,
            PatternBitSet::all(1),
            PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
        )
        .unwrap();
        assert!(evidence.problem().matches_search_problem(&expected));
        assert!(!evidence.problem().matches_search_problem(&foreign_rule));
        assert_eq!(
            PcGlobalProbabilityEvidence::from_executed_problem(
                &expected,
                PatternBitSet::all(2),
                PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
            ),
            Err(PcGlobalProbabilityEvidenceError::CoverageUniverseMismatch)
        );
    }

    #[test]
    fn global_probability_owns_one_aggregate_and_drops_private_authority() {
        let problem = problem(&query());
        assert!(!problem
            .pc_chance_evidence_policy()
            .retains_pc_coverage_evidence());
        let coverage = PatternBitSet::all(1);
        let evidence = PcGlobalProbabilityEvidence::from_executed_problem(
            &problem,
            coverage.clone(),
            PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
        )
        .unwrap();
        let private_bytes = evidence
            .checked_non_pattern_storage_retained_bytes()
            .unwrap();
        let future = PcGlobalProbabilityEvidence::checked_creation_future_bytes(&problem).unwrap();
        assert!(future >= private_bytes);
        let empty = crate::CoreExecutionResult::default();
        let empty_bytes = empty.checked_resource_retained_bytes().unwrap();
        let result = empty.with_pc_global_probability_evidence(evidence);
        assert_eq!(
            result.checked_resource_retained_bytes().unwrap() - empty_bytes,
            private_bytes + coverage.checked_storage_retained_bytes().unwrap()
        );
        let public = result.without_pc_chance_transient_evidence();
        assert!(public.pc_global_probability_evidence().is_none());
        assert_eq!(public.checked_resource_retained_bytes(), Some(empty_bytes));
    }
}
