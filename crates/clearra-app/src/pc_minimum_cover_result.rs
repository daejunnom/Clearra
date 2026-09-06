// SRP rationale: one change reason is the pc.minimals result contract from typed
// query binding through exact-portfolio completion. Source validation, the
// resumable preparation adapter, completeness evidence, selected identities,
// probabilities and retained-memory checks must agree on the same source and
// canonical portfolio before a result can be published. The coverage/portfolio
// engines own solving and paging; hosts and presenters do not define this proof.
use std::sync::Arc;

use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
};
use clearra_core_executor::{
    normalized_solution_probability_reports, CoreExecutionResult, SolutionProbabilityReport,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery, PcSolutionProbabilityPolicy,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblemPreset};
use clearra_supply::QueueObservationPolicy;

use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, CoveragePortfolioAlternativeSetPreparation,
    CoveragePortfolioAlternativeSetPreparationAdvance, PortfolioAlternativeSetIdentity,
};

pub const PC_MINIMUM_COVER_PROBLEM_CONTRACT: &str = "pc-clear-to-empty.v2";
pub const PC_MINIMUM_COVER_INPUT_CONTRACT: &str = "pc-pattern.v2";
pub const PC_MINIMUM_COVER_RESULT_CONTRACT: &str = "pc-minimum-cover.v2";
pub const PC_MINIMUM_COVER_CANONICAL_SELECTION: &str = "smallest-canonical-candidate-id";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcMinimalsIngressOrigin {
    CanonicalPcMinimals,
}

impl PcMinimalsIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcMinimals => "canonical-pc-minimals",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcMinimumCoverQuerySnapshot {
    Opening(Arc<OpeningPcSearchQuery>),
    Scenario(Arc<PcScenarioQuery>),
}

pub(crate) fn checked_minimum_opening_query_retained_bytes(
    query: &OpeningPcSearchQuery,
) -> Option<u128> {
    if query.verified_kick_profile().is_some() {
        return None;
    }
    (core::mem::size_of::<OpeningPcSearchQuery>() as u128)
        .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
        .checked_add(query.queue().checked_pc_score_retained_capacity_bytes()?)
}

pub(crate) fn checked_minimum_scenario_query_retained_bytes(
    query: &PcScenarioQuery,
) -> Option<u128> {
    (core::mem::size_of::<PcScenarioQuery>() as u128)
        .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
        .checked_add(query.checked_retained_capacity_bytes()?)
}

impl PcMinimumCoverQuerySnapshot {
    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Opening(query) => checked_minimum_opening_query_retained_bytes(query),
            Self::Scenario(query) => checked_minimum_scenario_query_retained_bytes(query),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcMinimumCoverProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcMinimumCoverProblemPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPc => "opening-pc",
            Self::ScenarioPc => "scenario-pc",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcMinimumCoverCompletenessEvidence {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    search_complete: bool,
    probability_complete: bool,
    exact_minimum_proven: bool,
    query_bound: bool,
}

impl PcMinimumCoverCompletenessEvidence {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn search_complete(self) -> bool {
        self.search_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub const fn query_bound(self) -> bool {
        self.query_bound
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.search_complete
            && self.probability_complete
            && self.exact_minimum_proven
            && self.query_bound
    }
}

/// Compact public result produced only after the full source coverage table is
/// replayed through the exact minimum-cover primitive and bound to the exact
/// typed query that compiled the executed problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcMinimumCoverV2Result {
    contract_id: &'static str,
    problem_contract_id: &'static str,
    input_contract_id: &'static str,
    origin: PcMinimalsIngressOrigin,
    query: PcMinimumCoverQuerySnapshot,
    problem_preset: PcMinimumCoverProblemPreset,
    source_solution_count: usize,
    selected_solution_count: usize,
    required_pattern_count: usize,
    normalized_solution_set_hash: String,
    selected_solution_keys: Vec<String>,
    selected_solution_probabilities: Vec<SolutionProbabilityReport>,
    portfolio_alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: PcMinimumCoverCompletenessEvidence,
}

impl PcMinimumCoverV2Result {
    /// Heap owner of a completed compact result, including its lazy all-optima
    /// source. Shared query/row owners are counted conservatively, not omitted.
    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .query
            .checked_retained_capacity_bytes()?
            .checked_add(self.normalized_solution_set_hash.capacity() as u128)?
            .checked_add(
                (self.selected_solution_keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )?
            .checked_add(
                (self.selected_solution_probabilities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
            )?
            .checked_add(core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128)?
            .checked_add(2 * core::mem::size_of::<usize>() as u128)?
            .checked_add(
                self.portfolio_alternatives
                    .checked_retained_capacity_bytes()?,
            )?;
        for key in &self.selected_solution_keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        for probability in &self.selected_solution_probabilities {
            bytes = bytes.checked_add(probability.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn problem_contract_id(&self) -> &'static str {
        self.problem_contract_id
    }

    pub const fn input_contract_id(&self) -> &'static str {
        self.input_contract_id
    }

    pub const fn origin(&self) -> PcMinimalsIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcMinimumCoverQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcMinimumCoverProblemPreset {
        self.problem_preset
    }

    pub const fn source_solution_count(&self) -> usize {
        self.source_solution_count
    }

    pub const fn selected_solution_count(&self) -> usize {
        self.selected_solution_count
    }

    pub const fn required_pattern_count(&self) -> usize {
        self.required_pattern_count
    }

    pub fn normalized_solution_set_hash(&self) -> &str {
        &self.normalized_solution_set_hash
    }

    pub fn selected_solution_keys(&self) -> &[String] {
        &self.selected_solution_keys
    }

    pub fn selected_solution_probabilities(&self) -> &[SolutionProbabilityReport] {
        &self.selected_solution_probabilities
    }

    pub fn portfolio_alternatives(&self) -> &CoveragePortfolioAlternativeSet {
        self.portfolio_alternatives.as_ref()
    }

    pub fn portfolio_alternative_owner(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.portfolio_alternatives
    }

    /// Canonical candidate selected by the App-owned exact set's stable dense
    /// candidate-id order. Presenters consume this identity; they do not
    /// re-rank the page locally.
    pub fn canonical_candidate(&self) -> Option<(u64, &str)> {
        let set = self.portfolio_alternatives();
        let candidate_id = *set.canonical_page().portfolio().candidate_ids().first()?;
        let index: usize = candidate_id.checked_sub(1)?.try_into().ok()?;
        let candidate = set.candidates().get(index)?;
        (candidate.candidate_id() == candidate_id)
            .then_some((candidate_id, candidate.normalized_key()))
    }

    pub const fn canonical_selection(&self) -> &'static str {
        PC_MINIMUM_COVER_CANONICAL_SELECTION
    }

    pub const fn completeness(&self) -> PcMinimumCoverCompletenessEvidence {
        self.completeness
    }
}

pub(crate) enum PcMinimumCoverQueryBinding<'a> {
    Opening(&'a Arc<OpeningPcSearchQuery>),
    Scenario(&'a Arc<PcScenarioQuery>),
}

/// Query-bound source evidence retained while the product coordinator proves
/// the optimum and selects the first original-row canonical portfolio.
///
/// Construction performs every producer/query/identity/probability check once.
/// Exact-cover code cannot construct this value and presenters cannot bypass
/// it, so a cooperative continuation never has to replay the trust boundary.
pub(crate) struct ValidatedPcMinimumCoverSource {
    query: PcMinimumCoverQuerySnapshot,
    origin: PcMinimalsIngressOrigin,
    preset: PcMinimumCoverProblemPreset,
    source_solution_count: usize,
    required_pattern_count: usize,
    candidate_keys: Vec<String>,
    required_patterns: PatternBitSet,
    rows: Vec<PatternBitSet>,
    source_solution_identities: Vec<StandardBoard64TilingIdentity>,
    expected_source_probabilities: Vec<SolutionProbabilityReport>,
    portfolio_identity: PortfolioAlternativeSetIdentity,
}

impl ValidatedPcMinimumCoverSource {
    pub(crate) fn into_portfolio_input(
        self,
    ) -> (
        PcMinimumCoverResultProjection,
        PortfolioAlternativeSetIdentity,
        Vec<String>,
        PatternBitSet,
        Vec<PatternBitSet>,
    ) {
        let projection = PcMinimumCoverResultProjection {
            query: self.query,
            origin: self.origin,
            preset: self.preset,
            source_solution_count: self.source_solution_count,
            required_pattern_count: self.required_pattern_count,
            source_solution_identities: self.source_solution_identities,
            expected_source_probabilities: self.expected_source_probabilities,
        };
        (
            projection,
            self.portfolio_identity,
            self.candidate_keys,
            self.required_patterns,
            self.rows,
        )
    }
}

pub(crate) struct PcMinimumCoverResultProjection {
    query: PcMinimumCoverQuerySnapshot,
    origin: PcMinimalsIngressOrigin,
    preset: PcMinimumCoverProblemPreset,
    source_solution_count: usize,
    required_pattern_count: usize,
    source_solution_identities: Vec<StandardBoard64TilingIdentity>,
    expected_source_probabilities: Vec<SolutionProbabilityReport>,
}

impl PcMinimumCoverResultProjection {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self
            .query
            .checked_retained_capacity_bytes()?
            .checked_add(
                (self.source_solution_identities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?,
            )?
            .checked_add(
                (self.expected_source_probabilities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
            )?;
        for probability in &self.expected_source_probabilities {
            bytes = bytes.checked_add(probability.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Debug)]
pub(crate) enum PcMinimumCoverV2PreparationAdvance {
    Pending { work_steps: u64 },
    Completed(PcMinimumCoverV2Result),
    Cancelled { work_steps: u64 },
}

pub(crate) struct PcMinimumCoverV2Preparation {
    projection: Option<PcMinimumCoverResultProjection>,
    portfolio: CoveragePortfolioAlternativeSetPreparation,
}

impl PcMinimumCoverV2Preparation {
    pub(crate) fn parallel_source_dimensions(&self) -> Option<(usize, usize)> {
        self.portfolio.parallel_source_dimensions()
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.portfolio
            .checked_retained_capacity_bytes()?
            .checked_add(self.checked_projection_retained_capacity_bytes()?)
    }

    fn checked_projection_retained_capacity_bytes(&self) -> Option<u128> {
        self.projection.as_ref().map_or(
            Some(0),
            PcMinimumCoverResultProjection::checked_retained_capacity_bytes,
        )
    }

    pub(crate) fn enable_parallel(&mut self, partitions: usize) -> Result<(), &'static str> {
        self.portfolio
            .enable_parallel(partitions)
            .map_err(|_| "pc minimals parallel preparation failed")
    }

    pub(crate) fn parallel_query_satisfied(&self) -> bool {
        self.portfolio.parallel_query_satisfied()
    }

    pub(crate) fn parallel_query(&self) -> Option<&clearra_coverage::cover::ExactAtMostQuery> {
        self.portfolio.parallel_query()
    }

    pub(crate) fn take_parallel_task(
        &mut self,
    ) -> Option<clearra_coverage::cover::ExactAtMostTask> {
        self.portfolio.take_parallel_task()
    }

    pub(crate) fn prepare_parallel_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), clearra_coverage::cover::ExactMinimumCoverError>,
    ) -> Result<bool, &'static str> {
        self.portfolio
            .prepare_parallel_idle_assist(maximum_children, guard)
            .map_err(|_| "pc minimals idle assistance rejected")
    }

    pub(crate) fn parallel_task_is_redundant(
        &self,
        identity: clearra_coverage::cover::ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, &'static str> {
        self.portfolio
            .parallel_task_is_redundant(identity, partition_id)
            .map_err(|_| "pc minimals task identity rejected")
    }

    pub(crate) fn accept_parallel_receipt(
        &mut self,
        receipt: clearra_coverage::cover::ExactAtMostReceipt,
    ) -> Result<(), &'static str> {
        self.portfolio
            .accept_parallel_receipt(receipt)
            .map_err(|_| "pc minimals parallel receipt validation failed")
    }

    pub(crate) fn new(source: ValidatedPcMinimumCoverSource) -> Result<Self, &'static str> {
        let (projection, identity, candidate_keys, required_patterns, rows) =
            source.into_portfolio_input();
        let portfolio = CoveragePortfolioAlternativeSetPreparation::new(
            identity,
            candidate_keys,
            required_patterns,
            rows,
        )
        .map_err(|_| "pc minimals portfolio alternative set validation failed")?;
        Ok(Self {
            projection: Some(projection),
            portfolio,
        })
    }

    #[cfg(test)]
    pub(crate) fn advance(
        &mut self,
        maximum_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PcMinimumCoverV2PreparationAdvance, &'static str> {
        self.advance_with_memory_guard(maximum_work_steps, &mut |_| Ok(()), cancelled)
    }

    /// Includes this wrapper's inline/projection owner around the portfolio
    /// callback; shared source/query Arcs are conservatively charged per owner.
    pub(crate) fn advance_with_memory_guard(
        &mut self,
        maximum_work_steps: u64,
        memory_guard: &mut impl FnMut(
            u128,
        )
            -> Result<(), clearra_coverage::cover::ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PcMinimumCoverV2PreparationAdvance, &'static str> {
        let outer_live = self
            .checked_projection_retained_capacity_bytes()
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| {
                bytes.checked_sub(
                    core::mem::size_of::<CoveragePortfolioAlternativeSetPreparation>() as u128,
                )
            })
            .ok_or("pc_minimum_cover_memory_projection_overflow")?;
        match self
            .portfolio
            .advance_with_memory_guard(
                maximum_work_steps,
                &mut |portfolio_peak| {
                    memory_guard(outer_live.checked_add(portfolio_peak).ok_or(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    )?)
                },
                cancelled,
            )
            .map_err(|error| match error {
                crate::portfolio_alternative_store::PortfolioAlternativeError::Enumeration(
                    clearra_coverage::cover::ExactMinimumCoverPortfolioError::MinimumCover(
                        clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected
                        | clearra_coverage::cover::ExactMinimumCoverError::MemoryCapacityExceeded {
                            ..
                        }
                        | clearra_coverage::cover::ExactMinimumCoverError::AllocationFailed {
                            ..
                        },
                    ),
                ) => "pc_minimum_cover_memory_limit_exceeded",
                crate::portfolio_alternative_store::PortfolioAlternativeError::Enumeration(
                    clearra_coverage::cover::ExactMinimumCoverPortfolioError::MinimumCover(
                        clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                    ),
                ) => "pc_minimum_cover_memory_projection_overflow",
                _ => "pc minimals portfolio alternative set validation failed",
            })? {
            CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                Ok(PcMinimumCoverV2PreparationAdvance::Pending { work_steps })
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Completed(portfolio) => {
                let residual_owner = (core::mem::size_of::<Self>() as u128)
                    .checked_add(
                        self.portfolio
                            .checked_retained_capacity_bytes()
                            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
                    )
                    .ok_or("pc_minimum_cover_memory_projection_overflow")?;
                let projection = self
                    .projection
                    .take()
                    .ok_or("pc minimals product preparation completed more than once")?;
                let arc_peak = projection
                    .checked_retained_capacity_bytes()
                    .and_then(|bytes| {
                        bytes.checked_add(portfolio.checked_retained_capacity_bytes()?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(
                            (core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128)
                                .checked_mul(2)?,
                        )
                    })
                    .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>() as u128))
                    .and_then(|bytes| bytes.checked_add(residual_owner))
                    .ok_or("pc_minimum_cover_memory_projection_overflow")?;
                memory_guard(arc_peak).map_err(|_| "pc_minimum_cover_memory_limit_exceeded")?;
                finish_pc_minimum_cover_v2_result_with_memory_guard(
                    projection,
                    Arc::new(portfolio),
                    &mut |peak| {
                        memory_guard(residual_owner.checked_add(peak).ok_or(
                            clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
                        )?)
                    },
                )
                .map(PcMinimumCoverV2PreparationAdvance::Completed)
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps } => {
                self.projection = None;
                Ok(PcMinimumCoverV2PreparationAdvance::Cancelled { work_steps })
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn complete(mut self) -> Result<PcMinimumCoverV2Result, &'static str> {
        loop {
            match self.advance(u64::MAX, &mut || false)? {
                PcMinimumCoverV2PreparationAdvance::Pending { work_steps } => {
                    if work_steps == 0 {
                        return Err("pc minimals product preparation made no progress");
                    }
                }
                PcMinimumCoverV2PreparationAdvance::Completed(result) => return Ok(result),
                PcMinimumCoverV2PreparationAdvance::Cancelled { .. } => {
                    return Err("pc minimals product preparation was cancelled")
                }
            }
        }
    }
}

impl PcMinimumCoverQueryBinding<'_> {
    fn snapshot(&self) -> PcMinimumCoverQuerySnapshot {
        match self {
            Self::Opening(query) => PcMinimumCoverQuerySnapshot::Opening(Arc::clone(query)),
            Self::Scenario(query) => PcMinimumCoverQuerySnapshot::Scenario(Arc::clone(query)),
        }
    }

    fn preset(&self) -> PcMinimumCoverProblemPreset {
        match self {
            Self::Opening(_) => PcMinimumCoverProblemPreset::OpeningPc,
            Self::Scenario(_) => PcMinimumCoverProblemPreset::ScenarioPc,
        }
    }

    fn compile_expected(&self) -> Result<clearra_problem::SearchProblem, &'static str> {
        let problem = match self {
            Self::Opening(query) => ProblemCompiler::compile_opening_pc(query.as_ref()),
            Self::Scenario(query) => ProblemCompiler::compile_scenario_pc(query.as_ref()),
        }
        .map_err(|_| "pc minimals expected problem did not compile")?;
        Ok(problem.with_pc_minimum_cover_v2_evidence())
    }
}

#[cfg(test)]
pub(crate) fn validate_pc_minimum_cover_v2_result(
    query: PcMinimumCoverQueryBinding<'_>,
    origin: PcMinimalsIngressOrigin,
    result: &CoreExecutionResult,
) -> Result<PcMinimumCoverV2Result, &'static str> {
    let source = validate_pc_minimum_cover_v2_source(query, origin, result)?;
    PcMinimumCoverV2Preparation::new(source)?.complete()
}

pub(crate) fn validate_pc_minimum_cover_v2_source(
    query: PcMinimumCoverQueryBinding<'_>,
    origin: PcMinimalsIngressOrigin,
    result: &CoreExecutionResult,
) -> Result<ValidatedPcMinimumCoverSource, &'static str> {
    let expected_problem = query.compile_expected()?;
    let preset = query.preset();
    if expected_problem.preset()
        != match preset {
            PcMinimumCoverProblemPreset::OpeningPc => SearchProblemPreset::OpeningPc,
            PcMinimumCoverProblemPreset::ScenarioPc => SearchProblemPreset::ScenarioPc,
        }
        || expected_problem.goal().as_str() != "clear-to-empty"
        || expected_problem.objective().kind() != ObjectivePolicy::minimum_cover().kind()
        || expected_problem.objective().score().requested()
        || expected_problem.output_policy() != SearchOutputPolicy::Trace
        || expected_problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || !expected_problem
            .pc_chance_evidence_policy()
            .retains_pc_minimum_cover_v2_evidence()
        || expected_problem
            .allowed_colored_solution_identities()
            .is_some()
    {
        return Err("pc minimals compiled problem contract mismatch");
    }

    let producer = result
        .pc_chance_coverage_evidence()
        .ok_or("pc minimals producer coverage evidence is missing")?;
    if !producer.complete()
        || !producer.problem().matches_search_problem(&expected_problem)
        || producer.pattern_count() == 0
    {
        return Err("pc minimals producer evidence does not match the typed query");
    }

    require_optional_field(result, "problem_preset", preset.as_str())?;
    require_optional_field(result, "compiled_goal", "clear-to-empty")?;
    for (key, expected) in [
        ("search_output_policy", "trace"),
        ("objective", "minimum-cover"),
        ("minimum_cover_incomplete_reason", "deferred-to-coordinator"),
        ("objective_incomplete_reason", "deferred-to-coordinator"),
        (
            "normalized_solution_key_algorithm",
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        ),
        ("resource_truncation_reason", "none"),
        ("count_truncated_reason", "none"),
    ] {
        require_unique_field(result, key, expected)?;
    }
    for key in [
        "minimum_cover_requested",
        "count_complete",
        "objective_search_complete",
        "probability_complete",
        "resource_probability_complete",
        "solution_count_calculated",
        "solution_set_materialized",
        "solution_keys_complete",
        "coverage_calculated",
        "probability_calculated",
        "buildability_verified",
    ] {
        require_unique_bool(result, key, true)?;
    }
    for key in [
        "minimum_cover_complete",
        "minimum_cover_proven_minimum",
        "objective_complete",
        "resource_truncated",
    ] {
        require_unique_bool(result, key, false)?;
    }

    let required_patterns = producer.coverage_union();
    if result.coverage_pattern_words() != required_patterns.words() {
        return Err("pc minimals required coverage union does not match producer evidence");
    }
    let required_pattern_count = usize::try_from(required_patterns.count_ones())
        .map_err(|_| "pc minimals required pattern count does not fit usize")?;
    require_unique_usize(
        result,
        "minimum_cover_required_pattern_count",
        required_pattern_count,
    )?;

    let source = result.normalized_solution_coverages();
    require_unique_usize(result, "minimum_cover_source_solution_count", source.len())?;
    let mut source_union = PatternBitSet::new(producer.pattern_count());
    let mut previous_key: Option<&str> = None;
    let mut rows = Vec::with_capacity(source.len());
    let mut candidate_keys = Vec::with_capacity(source.len());
    for coverage in source {
        let key = coverage.solution_key();
        if key.is_empty()
            || previous_key.is_some_and(|previous| previous >= key)
            || coverage.covered_patterns().pattern_count() != producer.pattern_count()
        {
            return Err("pc minimals source coverage rows are not canonical");
        }
        source_union
            .union_with(coverage.covered_patterns())
            .map_err(|_| "pc minimals source coverage universe mismatch")?;
        rows.push(coverage.covered_patterns().clone());
        candidate_keys.push(key.to_owned());
        previous_key = Some(key);
    }
    if source_union != required_patterns {
        return Err("pc minimals source rows do not cover the producer-required union");
    }

    // The deferred producer must expose one complete, canonical source
    // dictionary. Its provisional result fields describe that full source;
    // none of them are accepted as a minimum-cover selection authority.
    let source_solution_count = source.len();
    if result.normalized_solution_keys() != candidate_keys.as_slice()
        || result.normalized_solution_identities().len() != source_solution_count
        || result.solution_coverages().len() != source_solution_count
    {
        return Err("pc minimals deferred source identity evidence count mismatch");
    }
    for (((key, normalized), identity), coverage) in candidate_keys
        .iter()
        .zip(source)
        .zip(result.normalized_solution_identities())
        .zip(result.solution_coverages())
    {
        let parsed = NormalizedTilingSolutionKey::parse_canonical(key)
            .map_err(|_| "pc minimals deferred source key is not canonical")?;
        if parsed.standard_board64_identity().ok() != Some(*identity)
            || coverage.identity() != *identity
            || normalized.covered_patterns() != coverage.covered_patterns()
        {
            return Err("pc minimals deferred source identity and coverage evidence mismatch");
        }
    }
    for key in [
        "minimum_cover_selected_solution_count",
        "unique_solution_count",
        "normalized_unique_solution_count",
        "solution_keys_materialized_count",
    ] {
        require_unique_usize(result, key, source_solution_count)?;
    }

    let source_hash = normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
        result.normalized_solution_identities(),
    );
    require_unique_field(result, "normalized_solution_set_hash", &source_hash)?;
    require_unique_field(result, "actual_normalized_solution_set_hash", &source_hash)?;

    let expected_source_probabilities =
        if expected_problem.solution_probability_policy() == PcSolutionProbabilityPolicy::Include {
            let weights = expected_problem
                .piece_source()
                .materialized_universe()
                .ok_or("pc minimals expected probability universe is missing")?
                .weights();
            normalized_solution_probability_reports(&candidate_keys, source, weights, true)
                .map_err(|_| "pc minimals deferred source probability replay failed")?
        } else {
            Vec::new()
        };
    if result.solution_probabilities() != expected_source_probabilities.as_slice() {
        return Err("pc minimals deferred source probability evidence mismatch");
    }

    let product_build = clearra_host_contract::ProductBuildIdentity::current();
    let portfolio_identity = PortfolioAlternativeSetIdentity::new(
        expected_problem.problem_id().as_str(),
        format!(
            "pc-chance-source.v1:{}:{}",
            producer.piece_source_id(),
            producer.problem().problem_id(),
        ),
        format!(
            "rule:{}:kick:{}",
            expected_problem.rule_profile_value().id().as_str(),
            expected_problem.kick_profile().profile_id().as_str(),
        ),
        format!(
            "pc-pattern-universe.v1:{}:{}:{}:{}:{:016x}",
            producer.pattern_universe_id().get(),
            producer.pattern_weight_model_id().get(),
            producer.pattern_count(),
            producer.problem().total_possible_pattern_count(),
            producer.problem().materialized_probability_mass_bits(),
        ),
        product_build_identity_component(&product_build),
    )
    .map_err(|_| "pc minimals portfolio identity is invalid")?;

    Ok(ValidatedPcMinimumCoverSource {
        query: query.snapshot(),
        origin,
        preset,
        source_solution_count,
        required_pattern_count,
        candidate_keys,
        required_patterns,
        rows,
        source_solution_identities: result.normalized_solution_identities().to_vec(),
        expected_source_probabilities,
        portfolio_identity,
    })
}

fn finish_pc_minimum_cover_v2_result_with_memory_guard(
    projection: PcMinimumCoverResultProjection,
    portfolio_alternatives: Arc<CoveragePortfolioAlternativeSet>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), clearra_coverage::cover::ExactMinimumCoverError>,
) -> Result<PcMinimumCoverV2Result, &'static str> {
    // Only the App-owned exact set can select the public portfolio. Candidate
    // IDs index the already validated source dictionary, so the selected hash
    // and optional probability reports are projections rather than producer
    // claims or a second proof.
    let canonical_ids = portfolio_alternatives
        .canonical_page()
        .portfolio()
        .candidate_ids();
    let selected_solution_count = canonical_ids.len();
    let input_owner = projection
        .checked_retained_capacity_bytes()
        .and_then(|bytes| {
            bytes.checked_add(portfolio_alternatives.checked_retained_capacity_bytes()?)
        })
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<PcMinimumCoverResultProjection>() as u128)
        })
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<PcMinimumCoverV2Result>() as u128))
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128)
        })
        .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>() as u128))
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    let mut guard = |additional: u128| {
        memory_guard(
            input_owner
                .checked_add(additional)
                .ok_or("pc_minimum_cover_memory_projection_overflow")?,
        )
        .map_err(|_| "pc_minimum_cover_memory_limit_exceeded")
    };
    let probability_count = projection
        .expected_source_probabilities
        .len()
        .min(selected_solution_count);
    let requested = (selected_solution_count as u128)
        .checked_mul(
            (core::mem::size_of::<String>() + core::mem::size_of::<StandardBoard64TilingIdentity>())
                as u128,
        )
        .and_then(|bytes| {
            bytes.checked_add(
                (probability_count as u128)
                    .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
            )
        })
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    guard(requested)?;
    let mut selected_solution_keys = Vec::<String>::new();
    selected_solution_keys
        .try_reserve_exact(selected_solution_count)
        .map_err(|_| "pc_minimum_cover_memory_allocation_failed")?;
    let key_inline = (selected_solution_keys.capacity() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    guard(
        requested
            .checked_add(key_inline.saturating_sub(
                selected_solution_count as u128 * core::mem::size_of::<String>() as u128,
            ))
            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
    )?;
    let mut selected_solution_identities = Vec::<StandardBoard64TilingIdentity>::new();
    selected_solution_identities
        .try_reserve_exact(selected_solution_count)
        .map_err(|_| "pc_minimum_cover_memory_allocation_failed")?;
    let identity_inline = (selected_solution_identities.capacity() as u128)
        .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    let probability_requested = (probability_count as u128)
        .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    guard(
        key_inline
            .checked_add(identity_inline)
            .and_then(|bytes| bytes.checked_add(probability_requested))
            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
    )?;
    let mut selected_solution_probabilities = Vec::<SolutionProbabilityReport>::new();
    selected_solution_probabilities
        .try_reserve_exact(probability_count)
        .map_err(|_| "pc_minimum_cover_memory_allocation_failed")?;
    let mut selected_live = key_inline
        .checked_add(identity_inline)
        .and_then(|bytes| {
            bytes.checked_add(
                (selected_solution_probabilities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
            )
        })
        .ok_or("pc_minimum_cover_memory_projection_overflow")?;
    guard(selected_live)?;
    for candidate_id in canonical_ids {
        let index = candidate_id
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("pc minimals canonical candidate id is invalid")?;
        let candidate = portfolio_alternatives
            .candidates()
            .get(index)
            .ok_or("pc minimals canonical candidate is outside the source")?;
        if candidate.candidate_id() != *candidate_id {
            return Err("pc minimals canonical candidate map is inconsistent");
        }
        guard(
            selected_live
                .checked_add(candidate.normalized_key().len() as u128)
                .ok_or("pc_minimum_cover_memory_projection_overflow")?,
        )?;
        let key = candidate.normalized_key().to_owned();
        selected_live = selected_live
            .checked_add(key.capacity() as u128)
            .ok_or("pc_minimum_cover_memory_projection_overflow")?;
        guard(selected_live)?;
        selected_solution_keys.push(key);
        selected_solution_identities.push(
            *projection
                .source_solution_identities
                .get(index)
                .ok_or("pc minimals canonical identity is outside the source")?,
        );
        if let Some(probability) = projection.expected_source_probabilities.get(index) {
            guard(
                selected_live
                    .checked_add(
                        probability
                            .checked_clone_nested_bytes()
                            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
                    )
                    .ok_or("pc_minimum_cover_memory_projection_overflow")?,
            )?;
            let probability = probability.clone();
            selected_live = selected_live
                .checked_add(
                    probability
                        .checked_nested_retained_bytes()
                        .ok_or("pc_minimum_cover_memory_projection_overflow")?,
                )
                .ok_or("pc_minimum_cover_memory_projection_overflow")?;
            guard(selected_live)?;
            selected_solution_probabilities.push(probability);
        }
    }
    if !projection.expected_source_probabilities.is_empty()
        && selected_solution_probabilities.len() != selected_solution_count
    {
        return Err("pc minimals selected probability projection is incomplete");
    }
    // The existing formatter writes only `cts1:` and sixteen hex digits.
    // Reserve its bounded small formatting growth, then check actual capacity.
    guard(
        selected_live
            .checked_add(64)
            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
    )?;
    let normalized_solution_set_hash =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
            &selected_solution_identities,
        );
    guard(
        selected_live
            .checked_add(normalized_solution_set_hash.capacity() as u128)
            .ok_or("pc_minimum_cover_memory_projection_overflow")?,
    )?;

    let result = PcMinimumCoverV2Result {
        contract_id: PC_MINIMUM_COVER_RESULT_CONTRACT,
        problem_contract_id: PC_MINIMUM_COVER_PROBLEM_CONTRACT,
        input_contract_id: PC_MINIMUM_COVER_INPUT_CONTRACT,
        origin: projection.origin,
        query: projection.query,
        problem_preset: projection.preset,
        source_solution_count: projection.source_solution_count,
        selected_solution_count,
        required_pattern_count: projection.required_pattern_count,
        normalized_solution_set_hash,
        selected_solution_keys,
        selected_solution_probabilities,
        portfolio_alternatives,
        completeness: PcMinimumCoverCompletenessEvidence {
            source_universe_complete: true,
            coverage_rows_complete: true,
            search_complete: true,
            probability_complete: true,
            exact_minimum_proven: true,
            query_bound: true,
        },
    };
    // Input vectors not moved into the public result are still alive here.
    // The pre-return bound above deliberately retains them until this frame
    // exits; the wrapper separately accounts for its residual preparation.
    Ok(result)
}

fn product_build_identity_component(
    identity: &clearra_host_contract::ProductBuildIdentity,
) -> String {
    format!(
        "product-build.v1:{}:{}:{}:{}:{}",
        identity.engine_build_id(),
        identity.source_commit(),
        identity.contract_schema_version(),
        identity.supply_semantics_id(),
        identity.artifact_schema_version(),
    )
}

fn require_optional_field(
    result: &CoreExecutionResult,
    key: &str,
    expected: &str,
) -> Result<(), &'static str> {
    match result.field_occurrence_count(key) {
        0 => Ok(()),
        1 => require_unique_field(result, key, expected),
        _ => Err(match key {
            "problem_preset" => "pc minimals problem preset field mismatch",
            "compiled_goal" => "pc minimals compiled goal field mismatch",
            _ => "pc minimals optional result field mismatch",
        }),
    }
}

fn require_unique_field(
    result: &CoreExecutionResult,
    key: &str,
    expected: &str,
) -> Result<(), &'static str> {
    if result.unique_field(key) == Some(expected) {
        Ok(())
    } else {
        Err(match key {
            "problem_preset" => "pc minimals problem preset field mismatch",
            "compiled_goal" => "pc minimals compiled goal field mismatch",
            "search_output_policy" => "pc minimals search output policy field mismatch",
            "objective" => "pc minimals objective field mismatch",
            "minimum_cover_incomplete_reason" => {
                "pc minimals minimum-cover incomplete reason mismatch"
            }
            "objective_incomplete_reason" => "pc minimals objective incomplete reason mismatch",
            "normalized_solution_key_algorithm" => {
                "pc minimals normalized solution key algorithm mismatch"
            }
            "resource_truncation_reason" => "pc minimals resource truncation reason mismatch",
            "count_truncated_reason" => "pc minimals count truncation reason mismatch",
            "normalized_solution_set_hash" => "pc minimals normalized solution set hash mismatch",
            "actual_normalized_solution_set_hash" => {
                "pc minimals actual normalized solution set hash mismatch"
            }
            _ => "pc minimals result field mismatch",
        })
    }
}

fn require_unique_bool(
    result: &CoreExecutionResult,
    key: &str,
    expected: bool,
) -> Result<(), &'static str> {
    match result.unique_field(key) {
        Some("true") if expected => Ok(()),
        Some("false") if !expected => Ok(()),
        _ => Err("pc minimals result boolean field mismatch"),
    }
}

fn require_unique_usize(
    result: &CoreExecutionResult,
    key: &str,
    expected: usize,
) -> Result<(), &'static str> {
    if result
        .unique_field(key)
        .and_then(|value| value.parse::<usize>().ok())
        == Some(expected)
    {
        Ok(())
    } else {
        Err("pc minimals result count field mismatch")
    }
}

pub(crate) fn validate_pc_minimals_common_request_contract(
    objective: ObjectivePolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
) -> Result<(), &'static str> {
    if objective.kind() != ObjectivePolicy::minimum_cover().kind() || objective.score().requested()
    {
        return Err("pc minimals requires the non-scoring minimum-cover objective");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc minimals requires full-queue oracle knowledge");
    }
    let _ = probability_policy;
    Ok(())
}

pub(crate) fn validate_pc_minimals_scenario_shape(
    query: &PcScenarioQuery,
) -> Result<(), &'static str> {
    if query.completion_goal().as_str() != "clear-to-empty"
        || query.count_policy() != PcCountPolicy::CountUnique
        || query.allowed_colored_solution_identities().is_some()
    {
        return Err("pc minimals scenario does not preserve the typed pc-pattern.v2 contract");
    }
    Ok(())
}
