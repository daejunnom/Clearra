//! Exact Build coverage-portfolio projection.
//!
//! This module consumes only a query-bound, fully authorized Build result. It
//! derives the portfolio universe by OR-ing candidate `PatternBitSet` rows;
//! per-candidate probabilities are never summed. The immutable shared
//! portfolio set is retained so GUI/WASM/CLI can enumerate every exact tie
//! without rerunning search or deep-cloning the source coverage table.

use std::sync::Arc;

use clearra_core_executor::{solution_probability_pattern_weights, CoreExecutionResult};
use clearra_coverage::{
    cover::ExactMinimumCoverError,
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::union_probability::union_probability,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildSolutionProbabilityPolicy, ProblemCompiler,
};
use sha2::{Digest, Sha256};

use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, PortfolioAlternativeError, PortfolioAlternativeSetIdentity,
};

use super::{
    build_v2_contract::{BuildTargetSearchContract, ValidatedBuildTargetSearchResultAuthority},
    build_v2_options::BuildObjective,
    validate_build_probability_response, validate_build_solution_probability_reducer_input,
    BuildSolutionProbabilityResultError,
};

pub(crate) const BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT: &str = "build-coverage-portfolio.v2";
pub(crate) const BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS: &str =
    "normalized-solution-pattern-bitset-or-union";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildCoveragePortfolioCompletenessEvidence {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
    query_bound: bool,
}

impl BuildCoveragePortfolioCompletenessEvidence {
    pub(crate) const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub(crate) const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub(crate) const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub(crate) const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub(crate) const fn query_bound(self) -> bool {
        self.query_bound
    }

    // Retained as the single aggregate completeness predicate for product adapters.
    #[allow(dead_code)]
    pub(crate) const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
            && self.exact_minimum_proven
            && self.query_bound
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildCoveragePortfolioV2Result {
    contract_id: &'static str,
    probability_basis: &'static str,
    authority: ValidatedBuildTargetSearchResultAuthority,
    objective: BuildObjective,
    source_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: String,
    normalized_solution_set_hash: String,
    canonical_candidate_keys: Vec<String>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildCoveragePortfolioCompletenessEvidence,
}

impl BuildCoveragePortfolioV2Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub(crate) const fn probability_basis(&self) -> &'static str {
        self.probability_basis
    }

    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
    }

    pub(crate) const fn objective(&self) -> BuildObjective {
        self.objective
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn selected_candidate_count(&self) -> usize {
        self.selected_candidate_count
    }

    pub(crate) const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub(crate) const fn required_pattern_count(&self) -> usize {
        self.required_pattern_count
    }

    pub(crate) fn union_probability(&self) -> &str {
        &self.union_probability
    }

    pub(crate) fn normalized_solution_set_hash(&self) -> &str {
        &self.normalized_solution_set_hash
    }

    pub(crate) fn canonical_candidate_keys(&self) -> &[String] {
        &self.canonical_candidate_keys
    }

    // Retained as the borrowed counterpart to the shared-owner paging seam.
    #[allow(dead_code)]
    pub(crate) fn alternatives(&self) -> &CoveragePortfolioAlternativeSet {
        self.alternatives.as_ref()
    }

    // Retained for callers that need the exact shared alternative-store owner.
    #[allow(dead_code)]
    pub(crate) fn alternative_owner(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.alternatives
    }

    /// Borrowed product-neutral page source for portfolio objectives. The
    /// validated result can only exist for these two objectives, but the
    /// `Option` keeps the seam compatible with finite Build result families
    /// that do not allocate a live alternative store.
    pub(crate) fn portfolio_alternative_owner(
        &self,
    ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        matches!(
            self.objective,
            BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
        )
        .then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildCoveragePortfolioCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildCoveragePortfolioResultError {
    UnsupportedCapability,
    UnsupportedObjective,
    MissingBaseTargetQuery,
    QueryNotPortfolioCapable,
    QueryCompileFailed,
    Producer(BuildSolutionProbabilityResultError),
    IncompleteEvidence,
    PatternUniverseInvalid,
    NormalizedSolutionSetHashInvalid,
    ProbabilityUnionInvalid,
    // Preserves the exact-cover failure category at this product boundary.
    #[allow(dead_code)]
    MinimumCover(ExactMinimumCoverError),
    Portfolio(PortfolioAlternativeError),
}

/// Projects a complete `build.cover` result into the shared exact portfolio
/// store. `min-cover` and `max-probability-minimum` share this reducer: the
/// maximum attainable probability is the OR-union of all reachable rows, and
/// the second stage minimizes member count while retaining every tie.
pub(crate) fn validate_build_coverage_portfolio_v2_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    result: &CoreExecutionResult,
) -> Result<BuildCoveragePortfolioV2Result, BuildCoveragePortfolioResultError> {
    if authority.contract() != BuildTargetSearchContract::Cover {
        return Err(BuildCoveragePortfolioResultError::UnsupportedCapability);
    }
    let objective = authority.query().options().objective();
    if !matches!(
        objective,
        BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
    ) {
        return Err(BuildCoveragePortfolioResultError::UnsupportedObjective);
    }
    let query = authority
        .query()
        .base_target_query()
        .ok_or(BuildCoveragePortfolioResultError::MissingBaseTargetQuery)?;
    if query.aggregation() != BuildProbabilityAggregation::Buildability
        || query.finesse_metric().requested()
        || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
    {
        return Err(BuildCoveragePortfolioResultError::QueryNotPortfolioCapable);
    }

    validate_build_probability_response(
        query.finesse_request(),
        query.field(),
        query.aggregation(),
        query.solution_probability_policy(),
        result,
    )
    .map_err(BuildCoveragePortfolioResultError::Producer)?;
    let state = validate_build_solution_probability_reducer_input(
        Some(BuildSolutionProbabilityPolicy::Include),
        result,
    )
    .map_err(BuildCoveragePortfolioResultError::Producer)?;
    if !state.requested
        || !state.complete
        || !state.count_complete
        || !state.probability_complete
        || !state.solution_keys_complete
        || state.resource_truncated
    {
        return Err(BuildCoveragePortfolioResultError::IncompleteEvidence);
    }

    let pattern_count = unique_usize(result, "coverage_pattern_count")
        .ok_or(BuildCoveragePortfolioResultError::PatternUniverseInvalid)?;
    let required =
        PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
            .map_err(|_| BuildCoveragePortfolioResultError::PatternUniverseInvalid)?;
    // A fully validated zero-success source has one exact empty portfolio,
    // not an execution failure. Completeness is checked before this point.

    let candidate_keys = result.normalized_solution_keys().to_vec();
    let rows = result
        .normalized_solution_coverages()
        .iter()
        .map(|coverage| coverage.covered_patterns().clone())
        .collect::<Vec<_>>();
    if candidate_keys.len() != rows.len()
        || rows.iter().any(|row| row.pattern_count() != pattern_count)
    {
        return Err(BuildCoveragePortfolioResultError::PatternUniverseInvalid);
    }

    let normalized_solution_set_hash = result
        .unique_field("normalized_solution_set_hash")
        .filter(|value| !value.is_empty() && *value != "not-calculated")
        .ok_or(BuildCoveragePortfolioResultError::NormalizedSolutionSetHashInvalid)?;
    if result.unique_field("actual_normalized_solution_set_hash")
        != Some(normalized_solution_set_hash)
    {
        return Err(BuildCoveragePortfolioResultError::NormalizedSolutionSetHashInvalid);
    }

    let weights = solution_probability_pattern_weights(result)
        .map_err(|_| BuildCoveragePortfolioResultError::ProbabilityUnionInvalid)?;
    let union_probability = union_probability_from_pattern_weights(&required, &weights)?;
    let expected_problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|_| BuildCoveragePortfolioResultError::QueryCompileFailed)?;
    let identity = PortfolioAlternativeSetIdentity::new(
        build_query_identity(&authority, query, expected_problem.problem_id().as_str()),
        format!("build-normalized-solution-source.v1:{normalized_solution_set_hash}"),
        format!(
            "rule:{}:kick:{}:queue-knowledge:{}",
            expected_problem.rule_profile_value().id().as_str(),
            expected_problem.kick_profile().profile_id().as_str(),
            authority.query().options().queue_knowledge().as_str(),
        ),
        pattern_universe_identity(pattern_count, result),
        product_build_identity_component(),
    )
    .map_err(BuildCoveragePortfolioResultError::Portfolio)?;
    let alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new_canonical(
            identity,
            candidate_keys,
            required.clone(),
            rows,
        )
        .map_err(BuildCoveragePortfolioResultError::Portfolio)?,
    );
    let canonical_candidate_keys = alternatives
        .canonical_candidate_keys_owned()
        .map_err(BuildCoveragePortfolioResultError::Portfolio)?;

    Ok(BuildCoveragePortfolioV2Result {
        contract_id: BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT,
        probability_basis: BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS,
        authority,
        objective,
        source_candidate_count: alternatives.candidates().len(),
        selected_candidate_count: canonical_candidate_keys.len(),
        pattern_count,
        required_pattern_count: required.count_ones() as usize,
        union_probability,
        normalized_solution_set_hash: normalized_solution_set_hash.to_owned(),
        canonical_candidate_keys,
        alternatives,
        completeness: BuildCoveragePortfolioCompletenessEvidence {
            source_universe_complete: true,
            coverage_rows_complete: true,
            probability_weights_complete: true,
            exact_minimum_proven: true,
            query_bound: true,
        },
    })
}

fn build_query_identity(
    authority: &ValidatedBuildTargetSearchResultAuthority,
    query: &clearra_problem::BuildProbabilityQuery,
    core_problem_id: &str,
) -> String {
    let field = query.field();
    format!(
        "{}:{}:{}:{}:{}:{:?}:{:?}:{}:{}:{}",
        authority.contract().capability_id(),
        authority.contract().problem_contract_id(),
        core_problem_id,
        field.height(),
        field.includes_horizontal_mirror(),
        field.base_words(),
        field.target_words(),
        query.aggregation().as_str(),
        authority.query().options().queue_knowledge().as_str(),
        authority.query().options().objective().as_str(),
    )
}

fn pattern_universe_identity(pattern_count: usize, result: &CoreExecutionResult) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-pattern-universe.v1\0");
    hasher.update((pattern_count as u64).to_be_bytes());
    for word in result.coverage_pattern_words() {
        hasher.update(word.to_be_bytes());
    }
    for weight in result.postprocess_pattern_weights() {
        hasher.update((weight.len() as u64).to_be_bytes());
        hasher.update(weight.as_bytes());
    }
    format!(
        "build-pattern-universe.v1:{}",
        hex_sha256(hasher.finalize())
    )
}

fn product_build_identity_component() -> String {
    let identity = clearra_host_contract::ProductBuildIdentity::current();
    format!(
        "product-build.v1:{}:{}:{}:{}:{}",
        identity.engine_build_id(),
        identity.source_commit(),
        identity.contract_schema_version(),
        identity.supply_semantics_id(),
        identity.artifact_schema_version(),
    )
}

fn union_probability_from_pattern_weights(
    union: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let probability = union_probability(union, weights)
        .map_err(|_| BuildCoveragePortfolioResultError::ProbabilityUnionInvalid)?;
    Ok(probability.get().to_string())
}

fn unique_usize(result: &CoreExecutionResult, key: &str) -> Option<usize> {
    result.unique_field(key)?.parse::<usize>().ok()
}

fn hex_sha256(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        probability::probability_value::ProbabilityValue,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
        ProblemCompiler,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        union_probability_from_pattern_weights, validate_build_coverage_portfolio_v2_result,
        BuildCoveragePortfolioResultError, BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS,
        BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT,
    };
    use crate::{
        build_solution_probability_result::{
            build_probability_resource_test_guard,
            build_v2_contract::{
                BuildTargetSearchQuerySnapshot, ReportedBuildTargetSearchResultIdentity,
                ValidatedBuildTargetSearchResultAuthority,
            },
            build_v2_options::{BuildObjective, BuildV2OptionRequest},
        },
        AppCoreExecutorService,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };

    fn build_cover_query(
        solution_probability_policy: BuildSolutionProbabilityPolicy,
    ) -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("canonical one-piece target");
        BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(solution_probability_policy)
    }

    fn authority_for(query: &BuildProbabilityQuery) -> ValidatedBuildTargetSearchResultAuthority {
        let snapshot = BuildTargetSearchQuerySnapshot::cover(query.clone())
            .expect("valid Build coverage query")
            .with_options(BuildV2OptionRequest::default().with_objective(BuildObjective::MinCover))
            .expect("coverage objective is supported");
        let contract = snapshot.contract();
        ValidatedBuildTargetSearchResultAuthority::validate(
            snapshot,
            ReportedBuildTargetSearchResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .expect("fieldwise identity matches the query-owned contract")
    }

    fn execute(query: &BuildProbabilityQuery) -> clearra_core_executor::CoreExecutionResult {
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("one-piece Build problem compiles");
        AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("one-piece Build producer completes")
    }

    #[test]
    fn complete_build_cover_result_mints_exact_shared_portfolio() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let portfolio = validate_build_coverage_portfolio_v2_result(authority_for(&query), &result)
            .expect("complete query-bound result is portfolio-authorized");

        assert_eq!(
            portfolio.contract_id(),
            BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT
        );
        assert_eq!(
            portfolio.probability_basis(),
            BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS
        );
        assert_eq!(portfolio.objective(), BuildObjective::MinCover);
        assert_eq!(portfolio.source_candidate_count(), 1);
        assert_eq!(portfolio.selected_candidate_count(), 1);
        assert_eq!(portfolio.pattern_count(), 1);
        assert_eq!(portfolio.required_pattern_count(), 1);
        assert_eq!(portfolio.union_probability(), "1");
        assert!(portfolio.completeness().complete());
        assert_eq!(portfolio.alternatives().candidates().len(), 1);
        assert_eq!(
            portfolio.canonical_candidate_keys(),
            &[portfolio.alternatives().candidates()[0]
                .normalized_key()
                .to_owned()]
        );
        assert_eq!(
            portfolio
                .alternatives()
                .canonical_page()
                .portfolio()
                .candidate_ids(),
            &[1]
        );
        assert_eq!(
            portfolio.alternative_owner().identity(),
            portfolio.alternatives().identity()
        );
    }

    #[test]
    fn complete_unreachable_build_cover_has_one_exact_empty_portfolio() {
        let _resource_guard = build_probability_resource_test_guard();
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0]).unwrap();
        let query = BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let portfolio = validate_build_coverage_portfolio_v2_result(authority_for(&query), &result)
            .expect("complete unreachable source is an exact empty result");
        assert_eq!(portfolio.source_candidate_count(), 0);
        assert_eq!(portfolio.selected_candidate_count(), 0);
        assert_eq!(portfolio.pattern_count(), 1);
        assert_eq!(portfolio.required_pattern_count(), 0);
        assert_eq!(portfolio.union_probability(), "0");
        assert!(portfolio.completeness().complete());
        assert!(portfolio.canonical_candidate_keys().is_empty());
        assert!(portfolio
            .alternatives()
            .canonical_page()
            .portfolio()
            .candidate_ids()
            .is_empty());
    }

    #[test]
    fn omitted_probability_evidence_never_becomes_a_portfolio_winner() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Omit);
        let result = execute(&query);
        assert_eq!(
            validate_build_coverage_portfolio_v2_result(authority_for(&query), &result),
            Err(BuildCoveragePortfolioResultError::QueryNotPortfolioCapable)
        );
    }

    #[test]
    fn union_probability_is_an_or_union_and_never_a_candidate_sum() {
        let both = PatternBitSet::from_words(2, vec![0b11]).expect("two-pattern union");
        let first = PatternBitSet::from_words(2, vec![0b01]).expect("first pattern");
        let weights = WeightedPatternSet::new(vec![
            ProbabilityValue::new(0.4).expect("first weight"),
            ProbabilityValue::new(0.6).expect("second weight"),
        ])
        .expect("complete weight set");

        assert_eq!(
            union_probability_from_pattern_weights(&both, &weights)
                .expect("complete union probability"),
            "1"
        );
        assert_eq!(
            union_probability_from_pattern_weights(&first, &weights)
                .expect("partial union probability"),
            "0.4"
        );
    }
}
