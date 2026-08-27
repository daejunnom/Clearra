use std::sync::Arc;

use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    NormalizedTilingSolutionKey, NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
};
use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::{cover::exact_minimum_cover, pattern::pattern_bitset::PatternBitSet};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery, PcSolutionProbabilityPolicy,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblemPreset};
use clearra_supply::QueueObservationPolicy;

use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, PortfolioAlternativeSetIdentity,
};

pub const PC_MINIMUM_COVER_PROBLEM_CONTRACT: &str = "pc-clear-to-empty.v2";
pub const PC_MINIMUM_COVER_INPUT_CONTRACT: &str = "pc-pattern.v2";
pub const PC_MINIMUM_COVER_RESULT_CONTRACT: &str = "pc-minimum-cover.v2";

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
    portfolio_alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: PcMinimumCoverCompletenessEvidence,
}

impl PcMinimumCoverV2Result {
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

    pub fn portfolio_alternatives(&self) -> &CoveragePortfolioAlternativeSet {
        self.portfolio_alternatives.as_ref()
    }

    pub fn portfolio_alternative_owner(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.portfolio_alternatives
    }

    pub const fn completeness(&self) -> PcMinimumCoverCompletenessEvidence {
        self.completeness
    }
}

pub(crate) enum PcMinimumCoverQueryBinding<'a> {
    Opening(&'a Arc<OpeningPcSearchQuery>),
    Scenario(&'a Arc<PcScenarioQuery>),
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

pub(crate) fn validate_pc_minimum_cover_v2_result(
    query: PcMinimumCoverQueryBinding<'_>,
    origin: PcMinimalsIngressOrigin,
    result: &CoreExecutionResult,
) -> Result<PcMinimumCoverV2Result, &'static str> {
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
        ("minimum_cover_incomplete_reason", "none"),
        ("objective_incomplete_reason", "none"),
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
        "minimum_cover_complete",
        "minimum_cover_proven_minimum",
        "count_complete",
        "objective_search_complete",
        "objective_complete",
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
    require_unique_bool(result, "resource_truncated", false)?;

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

    let selection = exact_minimum_cover(&required_patterns, &rows)
        .map_err(|_| "pc minimals exact minimum-cover replay failed")?;
    if !selection.complete() || selection.covered_patterns() != &required_patterns {
        return Err("pc minimals exact minimum-cover replay is incomplete");
    }
    let selected_solution_keys = selection
        .row_indices()
        .iter()
        .map(|index| source[*index].solution_key().to_owned())
        .collect::<Vec<_>>();
    if result.normalized_solution_keys() != selected_solution_keys.as_slice() {
        return Err("pc minimals selected solution keys do not match exact replay");
    }
    let selected_solution_count = selected_solution_keys.len();
    require_unique_usize(
        result,
        "minimum_cover_selected_solution_count",
        selected_solution_count,
    )?;
    require_unique_usize(
        result,
        "normalized_unique_solution_count",
        selected_solution_count,
    )?;
    require_unique_usize(
        result,
        "solution_keys_materialized_count",
        selected_solution_count,
    )?;

    if result.normalized_solution_identities().len() != selected_solution_count
        || result.solution_coverages().len() != selected_solution_count
    {
        return Err("pc minimals selected identity evidence count mismatch");
    }
    for ((expected_key, identity), coverage) in selected_solution_keys
        .iter()
        .zip(result.normalized_solution_identities())
        .zip(result.solution_coverages())
    {
        let identity_key = NormalizedTilingSolutionKey::from_standard_board64_identity(*identity);
        let coverage_key =
            NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity());
        let source_coverage = source
            .binary_search_by(|candidate| candidate.solution_key().cmp(expected_key))
            .ok()
            .map(|index| source[index].covered_patterns());
        if identity_key.as_str() != expected_key
            || coverage_key.as_str() != expected_key
            || source_coverage != Some(coverage.covered_patterns())
        {
            return Err("pc minimals selected identity and coverage evidence mismatch");
        }
    }

    let normalized_solution_set_hash =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
            result.normalized_solution_identities(),
        );
    require_unique_field(
        result,
        "normalized_solution_set_hash",
        &normalized_solution_set_hash,
    )?;
    require_unique_field(
        result,
        "actual_normalized_solution_set_hash",
        &normalized_solution_set_hash,
    )?;

    let expected_probability_rows =
        if expected_problem.solution_probability_policy() == PcSolutionProbabilityPolicy::Include {
            selected_solution_count
        } else {
            0
        };
    if result.solution_probabilities().len() != expected_probability_rows
        || result
            .solution_probabilities()
            .iter()
            .zip(&selected_solution_keys)
            .any(|(probability, key)| probability.solution_key() != key)
    {
        return Err("pc minimals per-solution probability evidence mismatch");
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
    let portfolio_alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new(
            portfolio_identity,
            candidate_keys,
            required_patterns,
            rows,
            &selected_solution_keys,
        )
        .map_err(|_| "pc minimals portfolio alternative set validation failed")?,
    );

    Ok(PcMinimumCoverV2Result {
        contract_id: PC_MINIMUM_COVER_RESULT_CONTRACT,
        problem_contract_id: PC_MINIMUM_COVER_PROBLEM_CONTRACT,
        input_contract_id: PC_MINIMUM_COVER_INPUT_CONTRACT,
        origin,
        query: query.snapshot(),
        problem_preset: preset,
        source_solution_count: source.len(),
        selected_solution_count,
        required_pattern_count,
        normalized_solution_set_hash,
        selected_solution_keys,
        portfolio_alternatives,
        completeness: PcMinimumCoverCompletenessEvidence {
            source_universe_complete: true,
            coverage_rows_complete: true,
            search_complete: true,
            probability_complete: true,
            exact_minimum_proven: true,
            query_bound: true,
        },
    })
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
