//! Explicit global-probability projection. The row-complete v2 product keeps
//! its original authority; this projection never manufactures solution counts.

use clearra_core_executor::{PcGlobalProbabilityCompletion, PcGlobalProbabilityEvidence};
use clearra_problem::ProblemCompiler;

use super::*;

/// Exact global probability with a completion basis independent of enumeration.
/// Construction recompiles the original query and verifies Core-owned evidence;
/// a public bitmap or a summary claiming 100% is not sufficient authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcGlobalProbabilityV1Result {
    origin: PcChanceIngressOrigin,
    query: PcChanceQuerySnapshot,
    evidence: PcGlobalProbabilityEvidence,
    weighted_probability_bits: u64,
    weighted_probability: String,
}

impl PcGlobalProbabilityV1Result {
    /// Validates an explicitly opted-in Core execution. This does not change
    /// the ordinary pc.chance/v2 compiler or any CountUnique request.
    pub fn from_scenario_execution(
        query: &PcScenarioQuery,
        origin: PcChanceIngressOrigin,
        result: &CoreExecutionResult,
    ) -> Result<Self, PcGlobalProbabilityValidationError> {
        let problem = ProblemCompiler::compile_scenario_percent(query)
            .map_err(|_| rejected("global probability scenario compilation failed"))?
            .with_pc_global_probability_v1_evidence();
        let authority = PcChanceCompiledAuthority::global_scenario(query, origin, &problem)?;
        Self::from_authorized_execution(&authority, &problem, result)
    }

    pub fn from_opening_execution(
        query: &OpeningPcSearchQuery,
        origin: PcChanceIngressOrigin,
        result: &CoreExecutionResult,
    ) -> Result<Self, PcGlobalProbabilityValidationError> {
        let problem = ProblemCompiler::compile_opening_percent(query)
            .map_err(|_| rejected("global probability opening compilation failed"))?
            .with_pc_global_probability_v1_evidence();
        let authority = PcChanceCompiledAuthority::global_opening(query, origin, &problem)?;
        Self::from_authorized_execution(&authority, &problem, result)
    }

    fn from_authorized_execution(
        authority: &PcChanceCompiledAuthority,
        problem: &SearchProblem,
        result: &CoreExecutionResult,
    ) -> Result<Self, PcGlobalProbabilityValidationError> {
        let validated = authority.validate_global_execution_result(problem, result)?;
        if !validated.matches_core_result(result) {
            return Err(rejected("global probability result changed after validation").into());
        }
        Ok(validated.report)
    }

    pub const fn contract_id(&self) -> &'static str {
        "pc-global-probability.v1"
    }

    pub const fn origin(&self) -> PcChanceIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcChanceQuerySnapshot {
        &self.query
    }

    pub fn problem_id(&self) -> &str {
        self.evidence.problem().problem_id()
    }

    pub fn piece_source_id(&self) -> u64 {
        self.evidence.problem().piece_source_id()
    }

    pub fn pattern_universe_id(&self) -> u64 {
        self.evidence.problem().pattern_universe_id().get()
    }

    pub fn pattern_weight_model_id(&self) -> u64 {
        self.evidence.problem().pattern_weight_model_id().get()
    }

    pub fn pattern_count(&self) -> usize {
        self.evidence.problem().pattern_count()
    }

    pub fn total_pattern_count(&self) -> u128 {
        self.evidence.problem().total_possible_pattern_count()
    }

    pub fn covered_pattern_count(&self) -> usize {
        self.evidence.covered_pattern_count()
    }

    pub fn coverage_pattern_words(&self) -> &[u64] {
        self.evidence.coverage().words()
    }

    pub fn completion_basis(&self) -> PcGlobalProbabilityCompletion {
        self.evidence.completion()
    }

    pub fn enumeration_complete(&self) -> bool {
        self.evidence.enumeration_complete()
    }

    pub const fn probability_complete(&self) -> bool {
        true
    }

    pub const fn count_complete(&self) -> bool {
        false
    }

    pub const fn solution_count(&self) -> Option<usize> {
        None
    }

    pub const fn normalized_solution_set_hash(&self) -> Option<&str> {
        None
    }

    pub const fn weighted_probability_bits(&self) -> u64 {
        self.weighted_probability_bits
    }

    pub fn weighted_probability(&self) -> &str {
        &self.weighted_probability
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcGlobalProbabilityValidationError {
    reason: &'static str,
}

impl PcGlobalProbabilityValidationError {
    pub const fn reason(self) -> &'static str {
        self.reason
    }
}

impl From<PcChanceExecutionError> for PcGlobalProbabilityValidationError {
    fn from(error: PcChanceExecutionError) -> Self {
        Self {
            reason: error.reason,
        }
    }
}

impl fmt::Display for PcGlobalProbabilityValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl std::error::Error for PcGlobalProbabilityValidationError {}

impl PcChanceCompiledAuthority {
    fn global_scenario(
        query: &PcScenarioQuery,
        origin: PcChanceIngressOrigin,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceExecutionError> {
        Self::new_for_policy(
            PcChanceQuerySnapshot::Scenario(query.clone()),
            origin,
            problem,
            PcChanceEvidencePolicy::PcGlobalProbabilityV1,
        )
    }

    fn global_opening(
        query: &OpeningPcSearchQuery,
        origin: PcChanceIngressOrigin,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceExecutionError> {
        Self::new_for_policy(
            PcChanceQuerySnapshot::Opening(query.clone()),
            origin,
            problem,
            PcChanceEvidencePolicy::PcGlobalProbabilityV1,
        )
    }

    fn validate_global_execution_result(
        &self,
        expected_problem: &SearchProblem,
        result: &CoreExecutionResult,
    ) -> Result<ValidatedPcGlobalProbabilityExecutionEvidence, PcChanceExecutionError> {
        if self.evidence_policy != PcChanceEvidencePolicy::PcGlobalProbabilityV1
            || expected_problem.pc_chance_evidence_policy()
                != PcChanceEvidencePolicy::PcGlobalProbabilityV1
        {
            return Err(rejected(
                "global probability requires its explicit evidence policy",
            ));
        }
        let critical_fields = global_critical_field_snapshot(result)?;
        let evidence = result
            .pc_global_probability_evidence()
            .ok_or_else(|| rejected("global probability certificate is missing"))?;
        if !evidence.problem().matches_search_problem(expected_problem)
            || evidence.problem().piece_source_id() != self.piece_source_id
            || evidence.problem().pattern_universe_id() != self.pattern_universe_id
            || evidence.problem().pattern_weight_model_id() != self.pattern_weight_model_id
            || evidence.problem().pattern_count() != self.materialized_pattern_count
            || evidence.problem().total_possible_pattern_count() != self.total_pattern_count
            || !evidence.problem().piece_source_complete()
            || evidence.problem().materialized_probability_mass_bits() != 1_f64.to_bits()
        {
            return Err(rejected(
                "global probability certificate does not match the original problem",
            ));
        }
        require_field(result, "search_output_policy", "coverage-summary")?;
        require_usize(
            result,
            "coverage_pattern_count",
            self.materialized_pattern_count,
        )?;
        require_usize(
            result,
            "materialized_pattern_count",
            self.materialized_pattern_count,
        )?;
        require_usize(
            result,
            "source_sequence_length",
            self.source_sequence_length,
        )?;
        require_optional_u64(result, "piece_source_id", self.piece_source_id)?;
        require_optional_u64(
            result,
            "pattern_universe_id",
            self.pattern_universe_id.get(),
        )?;
        require_optional_u64(
            result,
            "pattern_weight_model_id",
            self.pattern_weight_model_id.get(),
        )?;
        if total_pattern_count(result)? != self.total_pattern_count {
            return Err(rejected(
                "global probability total pattern count differs from the original source",
            ));
        }
        require_true(result, "probability_complete")?;
        require_true(result, "resource_probability_complete")?;
        require_false(result, "count_complete")?;
        require_false(result, "resource_truncated")?;
        require_false(result, "renormalized")?;
        require_field(result, "resource_truncation_reason", "none")?;
        require_field(
            result,
            "probability_completion_basis",
            evidence.completion().as_str(),
        )?;
        require_field(
            result,
            "enumeration_complete",
            if evidence.enumeration_complete() {
                "true"
            } else {
                "false"
            },
        )?;
        for field in GLOBAL_SOLUTION_AUTHORITY_FIELDS {
            require_optional_field(result, field, "not-calculated")?;
        }
        if !result.normalized_solution_keys().is_empty()
            || !result.normalized_solution_identities().is_empty()
            || !result.solution_coverages().is_empty()
            || !result.normalized_solution_coverages().is_empty()
            || !result.solution_probabilities().is_empty()
            || !result.path_steps().is_empty()
            || result.representative_solution_identity().is_some()
            || result.pc_chance_coverage_evidence().is_some()
            || result.distributed_pc_chance_coverage_rows().is_some()
        {
            return Err(rejected(
                "global probability cannot publish a solution-family or row certificate",
            ));
        }
        let aggregate = strict_coverage_pattern_bitset_from_words(
            self.materialized_pattern_count,
            result.coverage_pattern_words(),
        )
        .map_err(|_| {
            rejected("global probability aggregate is not a strict original-universe bitset")
        })?;
        if &aggregate != evidence.coverage() {
            return Err(rejected(
                "public aggregate differs from the global probability certificate",
            ));
        }
        require_usize(
            result,
            "covered_pattern_count",
            evidence.covered_pattern_count(),
        )?;
        if evidence.completion() == PcGlobalProbabilityCompletion::AllOriginalQueuesCovered
            && evidence.covered_pattern_count() != self.materialized_pattern_count
        {
            return Err(rejected(
                "global probability all-queues certificate leaves original queues uncovered",
            ));
        }
        let weighted_probability = self
            .weights
            .covered_weight(&aggregate)
            .ok_or_else(|| rejected("global probability coverage and original weights disagree"))?;
        if self.weights.total_weight().get().to_bits() != 1_f64.to_bits() {
            return Err(rejected(
                "global probability original weight mass is not one",
            ));
        }
        require_field(
            result,
            "coverage_probability",
            &canonical_probability_v2(weighted_probability),
        )?;
        require_optional_field(
            result,
            "weighted_probability",
            &canonical_probability_v2(weighted_probability),
        )?;
        require_optional_field(
            result,
            "probability",
            &canonical_probability_v2(weighted_probability),
        )?;
        require_optional_field(result, "materialized_probability_mass", "1")?;
        Ok(ValidatedPcGlobalProbabilityExecutionEvidence {
            report: PcGlobalProbabilityV1Result {
                origin: self.origin,
                query: self.query.clone(),
                evidence: evidence.clone(),
                weighted_probability_bits: weighted_probability.get().to_bits(),
                weighted_probability: canonical_probability_v2(weighted_probability),
            },
            critical_fields,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedPcGlobalProbabilityExecutionEvidence {
    report: PcGlobalProbabilityV1Result,
    critical_fields: Vec<(String, String)>,
}

impl ValidatedPcGlobalProbabilityExecutionEvidence {
    fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        global_critical_field_snapshot(result).is_ok_and(|fields| fields == self.critical_fields)
            && result.coverage_pattern_words() == self.report.coverage_pattern_words()
            && result.pc_global_probability_evidence() == Some(&self.report.evidence)
    }
}

const GLOBAL_SOLUTION_AUTHORITY_FIELDS: &[&str] = &[
    "solution_count",
    "unique_solution_count",
    "total_solution_count",
    "normalized_unique_solution_count",
    "actual_normalized_unique_solution_count",
    "normalized_solution_set_hash",
    "actual_normalized_solution_set_hash",
    "mirror_normalized_solution_set_hash",
];

fn global_critical_field_snapshot(
    result: &CoreExecutionResult,
) -> Result<Vec<(String, String)>, PcChanceExecutionError> {
    let mut fields = critical_field_snapshot(result)?;
    let summary_fields = result.summary_fields();
    for key in ["probability_completion_basis", "enumeration_complete"]
        .iter()
        .chain(GLOBAL_SOLUTION_AUTHORITY_FIELDS)
    {
        let mut values = summary_fields
            .iter()
            .filter_map(|(name, value)| (name == key).then_some(value));
        if let Some(value) = values.next() {
            if values.next().is_some() {
                return Err(rejected(
                    "global probability result contains a duplicate authoritative field",
                ));
            }
            fields.push(((*key).to_owned(), value.clone()));
        }
    }
    Ok(fields)
}
