// SRP rationale: this module has one behavior-level change reason: validating PC chance evidence and materializing exact probability products.

use std::{collections::BTreeSet, fmt};

use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_core_executor::{
    canonical_probability_v2, strict_coverage_pattern_bitset_from_words, CoreExecutionResult,
    PcChanceProblemEvidence,
};
use clearra_coverage::{
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::union_probability::union_probability,
    row::coverage_row_kind::CoverageRowKind,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery, PcSolutionProbabilityPolicy,
};
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};
use clearra_supply::QueueObservationPolicy;

const CHANCE_RESULT_CONTRACT: &str = "pc-probability.v2";

pub(crate) fn supports_pc_chance_objective(objective: ObjectivePolicy) -> bool {
    let constraint = objective.execution_constraints();
    let expected = if constraint.preserves_back_to_back() {
        ObjectivePolicy::unique().with_back_to_back_preservation(constraint.spin_profile())
    } else {
        ObjectivePolicy::unique()
    };
    objective == expected
}

/// Closed identity for the Web ingress spelling that requested `pc.chance`.
///
/// This value is part of the request proof. Compatibility aliases are not
/// allowed to silently inherit the canonical spelling after translation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcChanceIngressOrigin {
    CanonicalPcChance,
    CompatibilityChance,
    CompatibilityPercent,
}

impl PcChanceIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcChance => "canonical-pc-chance",
            Self::CompatibilityChance => "compatibility-chance",
            Self::CompatibilityPercent => "compatibility-percent",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcChanceQuerySnapshot {
    Opening(OpeningPcSearchQuery),
    Scenario(PcScenarioQuery),
}

impl PcChanceQuerySnapshot {
    pub const fn problem_preset(&self) -> PcChanceProblemPreset {
        match self {
            Self::Opening(_) => PcChanceProblemPreset::OpeningPc,
            Self::Scenario(_) => PcChanceProblemPreset::ScenarioPc,
        }
    }

    const fn queue_observation_policy(&self) -> QueueObservationPolicy {
        match self {
            Self::Opening(query) => query.queue_observation_policy(),
            Self::Scenario(query) => query.queue_observation_policy(),
        }
    }

    fn objective(&self) -> ObjectivePolicy {
        match self {
            Self::Opening(query) => query.objective(),
            Self::Scenario(query) => query.objective(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcChanceProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcChanceProblemPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPc => "opening-pc",
            Self::ScenarioPc => "scenario-pc",
        }
    }

    const fn search_problem_preset(self) -> SearchProblemPreset {
        match self {
            Self::OpeningPc => SearchProblemPreset::OpeningPc,
            Self::ScenarioPc => SearchProblemPreset::ScenarioPc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcProbabilityCompletenessEvidence {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    count_complete: bool,
    objective_complete: bool,
    probability_complete: bool,
    resource_probability_complete: bool,
}

impl PcProbabilityCompletenessEvidence {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn count_complete(self) -> bool {
        self.count_complete
    }

    pub const fn objective_complete(self) -> bool {
        self.objective_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn resource_probability_complete(self) -> bool {
        self.resource_probability_complete
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.count_complete
            && self.objective_complete
            && self.probability_complete
            && self.resource_probability_complete
    }
}

/// Fieldwise, recomputed `pc-probability.v2` payload.
///
/// Floating-point values are exposed as both exact IEEE-754 bits and their
/// shortest round-trip decimal spelling. Legacy twelve-place aliases remain
/// Core projection fields and are never used as authority for this payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcProbabilityV2Result {
    contract_id: &'static str,
    origin: PcChanceIngressOrigin,
    query: PcChanceQuerySnapshot,
    problem_preset: PcChanceProblemPreset,
    problem_id: String,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    compiled_board_width: u16,
    compiled_board_visible_height: u16,
    compiled_board_occupied_words: [u64; 4],
    compiled_search_height: u16,
    queue_mode: String,
    queue_len: usize,
    compiled_piece_window: usize,
    exact_pieces: Option<usize>,
    source_sequence_length: usize,
    projects_unplaced_lookahead: bool,
    materialized_pattern_count: usize,
    total_pattern_count: u128,
    coverage_row_count: usize,
    covered_pattern_count: usize,
    coverage_pattern_words: Vec<u64>,
    weighted_probability_bits: u64,
    weighted_probability: String,
    materialized_probability_mass_bits: u64,
    materialized_probability_mass: String,
    completeness: PcProbabilityCompletenessEvidence,
}

impl PcProbabilityV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcChanceIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcChanceQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcChanceProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }

    pub const fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }

    pub const fn pattern_universe_id(&self) -> u64 {
        self.pattern_universe_id
    }

    pub const fn pattern_weight_model_id(&self) -> u64 {
        self.pattern_weight_model_id
    }

    pub const fn compiled_board_width(&self) -> u16 {
        self.compiled_board_width
    }

    pub const fn compiled_board_visible_height(&self) -> u16 {
        self.compiled_board_visible_height
    }

    /// A compact view is available only when it represents the whole field.
    pub const fn compiled_board_occupied_mask(&self) -> Option<u64> {
        if self.compiled_board_occupied_words[1] == 0
            && self.compiled_board_occupied_words[2] == 0
            && self.compiled_board_occupied_words[3] == 0
        {
            Some(self.compiled_board_occupied_words[0])
        } else {
            None
        }
    }

    pub const fn compiled_board_occupied_words(&self) -> [u64; 4] {
        self.compiled_board_occupied_words
    }

    pub const fn compiled_search_height(&self) -> u16 {
        self.compiled_search_height
    }

    pub fn queue_mode(&self) -> &str {
        &self.queue_mode
    }

    pub const fn queue_len(&self) -> usize {
        self.queue_len
    }

    pub const fn compiled_piece_window(&self) -> usize {
        self.compiled_piece_window
    }

    pub const fn exact_pieces(&self) -> Option<usize> {
        self.exact_pieces
    }

    pub const fn source_sequence_length(&self) -> usize {
        self.source_sequence_length
    }

    pub const fn projects_unplaced_lookahead(&self) -> bool {
        self.projects_unplaced_lookahead
    }

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub const fn total_pattern_count(&self) -> u128 {
        self.total_pattern_count
    }

    pub const fn coverage_row_count(&self) -> usize {
        self.coverage_row_count
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub fn coverage_pattern_words(&self) -> &[u64] {
        &self.coverage_pattern_words
    }

    pub const fn weighted_probability_bits(&self) -> u64 {
        self.weighted_probability_bits
    }

    pub fn weighted_probability(&self) -> &str {
        &self.weighted_probability
    }

    pub const fn materialized_probability_mass_bits(&self) -> u64 {
        self.materialized_probability_mass_bits
    }

    pub fn materialized_probability_mass(&self) -> &str {
        &self.materialized_probability_mass
    }

    pub const fn completeness(&self) -> PcProbabilityCompletenessEvidence {
        self.completeness
    }
}

/// Owner-bound authority created from the exact query and the exact compiled
/// `SearchProblem` that will be executed. The weighted universe remains
/// private and transient; only the recomputed compact report can escape.
#[derive(Clone, Debug)]
pub(crate) struct PcChanceCompiledAuthority {
    origin: PcChanceIngressOrigin,
    query: PcChanceQuerySnapshot,
    problem_preset: PcChanceProblemPreset,
    problem_id: String,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    weights: WeightedPatternSet,
    compiled_board_width: u16,
    compiled_board_visible_height: u16,
    compiled_board_occupied_words: [u64; 4],
    compiled_search_height: u16,
    queue_mode: String,
    queue_len: usize,
    compiled_piece_window: usize,
    exact_pieces: Option<usize>,
    source_sequence_length: usize,
    projects_unplaced_lookahead: bool,
    materialized_pattern_count: usize,
    total_pattern_count: u128,
}

impl PcChanceCompiledAuthority {
    pub(crate) fn opening(
        query: &OpeningPcSearchQuery,
        origin: PcChanceIngressOrigin,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceExecutionError> {
        Self::new(
            PcChanceQuerySnapshot::Opening(query.clone()),
            origin,
            problem,
        )
    }

    pub(crate) fn scenario(
        query: &PcScenarioQuery,
        origin: PcChanceIngressOrigin,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceExecutionError> {
        Self::new(
            PcChanceQuerySnapshot::Scenario(query.clone()),
            origin,
            problem,
        )
    }

    fn new(
        query: PcChanceQuerySnapshot,
        origin: PcChanceIngressOrigin,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceExecutionError> {
        let problem_preset = query.problem_preset();
        if problem.preset() != problem_preset.search_problem_preset() {
            return Err(rejected(
                "compiled problem preset does not match the chance query",
            ));
        }
        if problem.output_policy() != SearchOutputPolicy::CoverageSummary {
            return Err(rejected("pc chance requires coverage-summary output"));
        }
        if !problem
            .pc_chance_evidence_policy()
            .retains_pc_probability_v2_evidence()
        {
            return Err(rejected(
                "compiled pc chance problem is missing the pc-probability.v2 evidence marker",
            ));
        }
        if problem.backend_request().max_memory_mib().is_some() {
            return Err(rejected(
                "pc chance does not support an explicit memory cap until transient proof memory is accounted",
            ));
        }
        if query.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
            || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        {
            return Err(rejected(
                "pc chance requires full-queue oracle knowledge in both query and compiled problem",
            ));
        }
        if problem.goal().as_str() != "clear-to-empty"
            || problem.objective() != query.objective()
            || !supports_pc_chance_objective(problem.objective())
            || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
            || problem.count_policy() != PcCountPolicy::CountUnique
            || problem.allowed_colored_solution_identities().is_some()
        {
            return Err(rejected(
                "compiled chance problem does not preserve the validated objective contract",
            ));
        }

        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .ok_or_else(|| rejected("compiled chance problem has no materialized universe"))?;
        if source.id().get() == 0
            || universe.pattern_universe_id().get() == 0
            || universe.pattern_weight_model_id().get() == 0
        {
            return Err(rejected("compiled chance universe identity is missing"));
        }
        if !source.complete() || source.truncation_reason().is_some() || !universe.complete() {
            return Err(rejected("compiled chance universe is incomplete"));
        }
        let materialized_pattern_count = universe.pattern_count();
        let total_pattern_count = universe.total_possible_pattern_count();
        if materialized_pattern_count == 0
            || total_pattern_count != materialized_pattern_count as u128
            || universe.weights().len() != materialized_pattern_count
            || universe.materialized_probability_mass().get().to_bits() != 1.0_f64.to_bits()
        {
            return Err(rejected(
                "compiled chance universe counts or complete probability mass do not agree",
            ));
        }

        let board = problem.initial_board();
        let supply = problem.supply();
        Ok(Self {
            origin,
            query,
            problem_preset,
            problem_id: problem.problem_id().as_str().to_owned(),
            piece_source_id: source.id().get(),
            pattern_universe_id: universe.pattern_universe_id(),
            pattern_weight_model_id: universe.pattern_weight_model_id(),
            weights: universe.weights().clone(),
            compiled_board_width: board.width(),
            compiled_board_visible_height: board.visible_height(),
            compiled_board_occupied_words: board.occupied_words(),
            compiled_search_height: problem.search_height(),
            queue_mode: supply.queue_mode().to_owned(),
            queue_len: supply.queue().len(),
            compiled_piece_window: problem.piece_window().max_pieces(),
            exact_pieces: problem.exact_pieces(),
            source_sequence_length: supply.source_sequence_length(),
            projects_unplaced_lookahead: supply.projects_unplaced_lookahead(),
            materialized_pattern_count,
            total_pattern_count,
        })
    }

    pub(crate) fn validate_execution_result(
        &self,
        expected_problem: &SearchProblem,
        result: &CoreExecutionResult,
    ) -> Result<ValidatedPcChanceExecutionEvidence, PcChanceExecutionError> {
        if expected_problem.objective() != self.query.objective() {
            return Err(rejected(
                "executed pc chance objective does not match the query authority",
            ));
        }
        let critical_fields = critical_field_snapshot(result)?;
        validate_pc_chance_constraint_materialization(expected_problem, result)?;
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
        require_optional_usize(result, "pattern_count", self.materialized_pattern_count)?;
        require_optional_usize(
            result,
            "weighted_pattern_count",
            self.materialized_pattern_count,
        )?;
        require_optional_usize(
            result,
            "verified_pattern_count",
            self.materialized_pattern_count,
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
        require_optional_field(result, "problem_preset", self.problem_preset.as_str())?;
        require_optional_field(result, "compiled_goal", "clear-to-empty")?;
        require_optional_usize(result, "compiled_piece_window", self.compiled_piece_window)?;
        require_optional_field(result, "queue_mode", &self.queue_mode)?;
        require_optional_usize(result, "queue_len", self.queue_len)?;
        require_optional_usize(result, "minimum_len", self.compiled_piece_window)?;
        require_optional_usize(
            result,
            "source_sequence_length",
            self.source_sequence_length,
        )?;

        let total_pattern_count = total_pattern_count(result)?;
        if total_pattern_count != self.total_pattern_count {
            return Err(rejected(
                "reported total pattern count does not match authority",
            ));
        }

        require_true(result, "probability_complete")?;
        require_true(result, "count_complete")?;
        require_true(result, "resource_probability_complete")?;
        require_false(result, "renormalized")?;
        require_false(result, "resource_truncated")?;
        require_field(result, "resource_truncation_reason", "none")?;
        require_field(result, "count_truncated_reason", "none")?;

        let producer = match result.field("status") {
            Some("percent-executed") => {
                require_false(result, "truncated")?;
                require_field(result, "truncation_reason", "none")?;
                require_field(result, "weighted_probability_reducer", "union_probability")?;
                PcChanceResultProducer::DirectPercentService
            }
            None => {
                require_true(result, "objective_complete")?;
                require_true(result, "objective_search_complete")?;
                PcChanceResultProducer::CooperativeWasm
            }
            Some(_) => return Err(rejected("unsupported chance result producer status")),
        };

        let row_evidence = result
            .pc_chance_coverage_evidence()
            .ok_or_else(|| rejected("pc chance coverage-row evidence is missing"))?;
        if !row_evidence
            .problem()
            .matches_search_problem(expected_problem)
        {
            return Err(rejected(
                "executed pc chance problem does not match the expected compiled problem",
            ));
        }
        if !row_evidence.complete() {
            return Err(rejected("pc chance coverage-row evidence is incomplete"));
        }
        if row_evidence.row_kind() != &CoverageRowKind::Build
            || row_evidence.piece_source_id() != self.piece_source_id
            || row_evidence.pattern_universe_id() != self.pattern_universe_id
            || row_evidence.pattern_weight_model_id() != self.pattern_weight_model_id
            || row_evidence.pattern_count() != self.materialized_pattern_count
        {
            return Err(rejected(
                "pc chance coverage-row evidence identity does not match authority",
            ));
        }
        let expected_row_count = match producer {
            PcChanceResultProducer::DirectPercentService => {
                strict_usize_field(result, "c_buildup_coverage_row_count")?
            }
            PcChanceResultProducer::CooperativeWasm => {
                require_field(result, "coverage_row_count", "not-calculated")?;
                row_evidence.row_count()
            }
        };
        if expected_row_count != row_evidence.rows().len() {
            return Err(rejected(
                "coverage-row evidence count does not match the result",
            ));
        }

        let mut candidate_ids = BTreeSet::new();
        let mut matrix = TypedCoverageMatrix::new_with_piece_source(
            CoverageRowKind::Build,
            self.piece_source_id,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
            self.materialized_pattern_count,
        );
        for row in row_evidence.rows() {
            if !candidate_ids.insert(row.candidate_id()) {
                return Err(rejected(
                    "coverage-row evidence contains a duplicate candidate",
                ));
            }
            matrix
                .push(row.clone())
                .map_err(|_| rejected("coverage-row evidence identity or dimensions mismatch"))?;
        }
        if self
            .query
            .objective()
            .execution_constraints()
            .preserves_back_to_back()
            && result.unique_field("b2b_preserving_candidate_pattern_count")
                != Some("not-calculated")
        {
            return Err(rejected("pc chance B2B witness count must remain private"));
        }
        let union = matrix.union_all();

        let aggregate = strict_coverage_pattern_bitset_from_words(
            self.materialized_pattern_count,
            result.coverage_pattern_words(),
        )
        .map_err(|_| rejected("public coverage aggregate is not a strict universe bitset"))?;
        if aggregate != union {
            return Err(rejected(
                "public coverage aggregate is not the coverage-row union",
            ));
        }
        let covered_pattern_count = union.count_ones() as usize;
        require_usize(result, "covered_pattern_count", covered_pattern_count)?;

        let weighted_probability = self
            .weights
            .covered_weight(&union)
            .ok_or_else(|| rejected("coverage union and weight universe do not agree"))?;
        let materialized_probability_mass = self.weights.total_weight();
        if materialized_probability_mass.get().to_bits() != 1.0_f64.to_bits() {
            return Err(rejected("complete chance probability mass is not one"));
        }
        validate_probability_projection(
            result,
            producer,
            &union,
            &self.weights,
            weighted_probability,
            materialized_probability_mass,
        )?;

        let report = PcProbabilityV2Result {
            contract_id: CHANCE_RESULT_CONTRACT,
            origin: self.origin,
            query: self.query.clone(),
            problem_preset: self.problem_preset,
            problem_id: self.problem_id.clone(),
            piece_source_id: self.piece_source_id,
            pattern_universe_id: self.pattern_universe_id.get(),
            pattern_weight_model_id: self.pattern_weight_model_id.get(),
            compiled_board_width: self.compiled_board_width,
            compiled_board_visible_height: self.compiled_board_visible_height,
            compiled_board_occupied_words: self.compiled_board_occupied_words,
            compiled_search_height: self.compiled_search_height,
            queue_mode: self.queue_mode.clone(),
            queue_len: self.queue_len,
            compiled_piece_window: self.compiled_piece_window,
            exact_pieces: self.exact_pieces,
            source_sequence_length: self.source_sequence_length,
            projects_unplaced_lookahead: self.projects_unplaced_lookahead,
            materialized_pattern_count: self.materialized_pattern_count,
            total_pattern_count: self.total_pattern_count,
            coverage_row_count: row_evidence.rows().len(),
            covered_pattern_count,
            coverage_pattern_words: union.words().to_vec(),
            weighted_probability_bits: weighted_probability.get().to_bits(),
            weighted_probability: canonical_probability_v2(weighted_probability),
            materialized_probability_mass_bits: materialized_probability_mass.get().to_bits(),
            materialized_probability_mass: canonical_probability_v2(materialized_probability_mass),
            completeness: PcProbabilityCompletenessEvidence {
                source_universe_complete: true,
                coverage_rows_complete: true,
                count_complete: true,
                objective_complete: true,
                probability_complete: true,
                resource_probability_complete: true,
            },
        };

        Ok(ValidatedPcChanceExecutionEvidence {
            report,
            critical_fields,
            problem_evidence: row_evidence.problem().clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcChanceResultProducer {
    DirectPercentService,
    CooperativeWasm,
}

pub(crate) fn validate_pc_chance_before_public_surface(
    problem: &SearchProblem,
    result: &CoreExecutionResult,
) -> Result<(), PcChanceExecutionError> {
    // Public summary projection canonicalizes some fields. Detect conflicting
    // producer occurrences before that operation can erase their ambiguity.
    for key in CRITICAL_FIELDS {
        if result.field_occurrence_count(key) > 1 {
            return Err(rejected(
                "chance result contains a duplicate authoritative field",
            ));
        }
    }
    validate_pc_chance_constraint_materialization(problem, result)?;
    if problem
        .objective()
        .execution_constraints()
        .preserves_back_to_back()
    {
        let evidence = result
            .pc_chance_coverage_evidence()
            .expect("constraint validation requires the private coverage rows");
        let witnessed = evidence
            .rows()
            .iter()
            .try_fold(0_u128, |count, row| {
                count.checked_add(u128::from(row.coverage_bits().count_ones()))
            })
            .ok_or_else(|| rejected("pc chance B2B witness count overflow"))?;
        if optional_strict_u128_field(result, "b2b_preserving_candidate_pattern_count")?
            != Some(witnessed)
        {
            return Err(rejected(
                "pc chance B2B witness count does not match its rows",
            ));
        }
    }
    Ok(())
}

/// The probability product owns coverage of the constrained physical language.
/// Validate that producer binding before the generic public-surface pass can
/// detach its rows or trust the already-materialized execution constraint.
pub(crate) fn validate_pc_chance_constraint_materialization(
    problem: &SearchProblem,
    result: &CoreExecutionResult,
) -> Result<(), PcChanceExecutionError> {
    let evidence = result
        .pc_chance_coverage_evidence()
        .ok_or_else(|| rejected("pc chance coverage-row evidence is missing"))?;
    if !evidence.problem().matches_search_problem(problem) {
        return Err(rejected(
            "executed pc chance problem does not match the expected compiled problem",
        ));
    }
    let constraint = problem.objective().execution_constraints();
    if !constraint.preserves_back_to_back() {
        require_optional_field(result, "execution_constraint_preserve_b2b", "false")?;
        require_optional_field(result, "execution_constraint_materialized", "false")?;
        if result.field_occurrence_count("b2b_preservation_evaluation_basis") != 0 {
            return Err(rejected(
                "unconstrained pc chance carries a B2B filter authority",
            ));
        }
        return Ok(());
    }
    if !problem
        .pc_chance_evidence_policy()
        .retains_pc_probability_v2_evidence()
        || problem.output_policy() != SearchOutputPolicy::CoverageSummary
        || !supports_pc_chance_objective(problem.objective())
        || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
        || problem.count_policy() != PcCountPolicy::CountUnique
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
    {
        return Err(rejected(
            "pc chance B2B authority has an incompatible problem",
        ));
    }
    for (name, expected) in [
        ("execution_constraint_preserve_b2b", "true"),
        ("execution_constraint_materialized", "true"),
        (
            "execution_constraint_spin_profile",
            constraint.spin_profile().as_str(),
        ),
        (
            "b2b_preservation_evaluation_basis",
            "filtered-pc-chance-order-language",
        ),
        ("b2b_preservation_selection", "existential"),
        (
            "b2b_preservation_denominator_semantics",
            "original-materialized-queue",
        ),
        ("b2b_preservation_path_multiplicity_counted", "false"),
        ("objective", "unique"),
        ("postprocess_scoring_requested", "false"),
        ("postprocess_pc_save_requested", "false"),
        ("postprocess_pc_path_requested", "false"),
    ] {
        if result.unique_field(name) != Some(expected) {
            return Err(rejected(
                "pc chance B2B filter authority does not match the request",
            ));
        }
    }
    for (filtered, aggregate) in [
        ("b2b_preserving_pattern_count", "covered_pattern_count"),
        (
            "b2b_preservation_pattern_universe_count",
            "coverage_pattern_count",
        ),
        ("b2b_preservation_probability", "coverage_probability"),
        ("b2b_preservation_count_complete", "count_complete"),
        (
            "b2b_preservation_probability_complete",
            "probability_complete",
        ),
    ] {
        let expected = result
            .unique_field(aggregate)
            .ok_or_else(|| rejected("pc chance B2B filter is missing its aggregate authority"))?;
        if result.unique_field(filtered) != Some(expected) {
            return Err(rejected(
                "pc chance B2B filter and coverage aggregate disagree",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcChanceExecutionEvidence {
    report: PcProbabilityV2Result,
    critical_fields: Vec<(String, String)>,
    problem_evidence: PcChanceProblemEvidence,
}

impl ValidatedPcChanceExecutionEvidence {
    pub(crate) fn report(&self) -> &PcProbabilityV2Result {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        critical_field_snapshot(result).is_ok_and(|fields| fields == self.critical_fields)
            && result.coverage_pattern_words() == self.report.coverage_pattern_words()
            && result
                .pc_chance_coverage_evidence()
                .is_some_and(|evidence| {
                    evidence.complete()
                        && evidence.problem() == &self.problem_evidence
                        && evidence.row_kind() == &CoverageRowKind::Build
                        && evidence.piece_source_id() == self.report.piece_source_id()
                        && evidence.pattern_universe_id().get() == self.report.pattern_universe_id()
                        && evidence.pattern_weight_model_id().get()
                            == self.report.pattern_weight_model_id()
                        && evidence.pattern_count() == self.report.materialized_pattern_count()
                        && evidence.row_count() == self.report.coverage_row_count()
                        && evidence.coverage_union().words() == self.report.coverage_pattern_words()
                })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcChanceExecutionError {
    reason: &'static str,
}

impl PcChanceExecutionError {
    #[cfg(test)]
    pub(crate) const fn reason(self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for PcChanceExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl std::error::Error for PcChanceExecutionError {}

const fn rejected(reason: &'static str) -> PcChanceExecutionError {
    PcChanceExecutionError { reason }
}

const CRITICAL_FIELDS: &[&str] = &[
    "status",
    "search_output_policy",
    "problem_preset",
    "compiled_goal",
    "compiled_piece_window",
    "queue_mode",
    "queue_len",
    "minimum_len",
    "source_sequence_length",
    "piece_source_id",
    "pattern_universe_id",
    "pattern_weight_model_id",
    "pattern_count",
    "coverage_pattern_count",
    "verified_pattern_count",
    "materialized_pattern_count",
    "total_pattern_count",
    "total_possible_pattern_count",
    "covered_pattern_count",
    "weighted_pattern_count",
    "c_buildup_coverage_row_count",
    "coverage_row_count",
    "probability",
    "weighted_probability",
    "coverage_probability",
    "materialized_probability_mass",
    "weighted_probability_reducer",
    "renormalized",
    "probability_complete",
    "count_complete",
    "count_truncated_reason",
    "truncated",
    "truncation_reason",
    "resource_probability_complete",
    "resource_truncated",
    "resource_truncation_reason",
    "objective_search_complete",
    "objective_complete",
    "objective",
    "postprocess_scoring_requested",
    "postprocess_pc_save_requested",
    "postprocess_pc_path_requested",
    "execution_constraint_preserve_b2b",
    "execution_constraint_materialized",
    "execution_constraint_spin_profile",
    "b2b_preservation_evaluation_basis",
    "b2b_preservation_selection",
    "b2b_preservation_denominator_semantics",
    "b2b_preserving_pattern_count",
    "b2b_preservation_pattern_universe_count",
    "b2b_preservation_probability",
    "b2b_preservation_count_complete",
    "b2b_preservation_probability_complete",
    "b2b_preserving_candidate_pattern_count",
    "b2b_preservation_path_multiplicity_counted",
];

fn critical_field_snapshot(
    result: &CoreExecutionResult,
) -> Result<Vec<(String, String)>, PcChanceExecutionError> {
    let fields = result.summary_fields();
    let mut snapshot = Vec::new();
    for key in CRITICAL_FIELDS {
        let mut values = fields
            .iter()
            .filter_map(|(field_key, value)| (field_key == key).then_some(value));
        if let Some(value) = values.next() {
            if values.next().is_some() {
                return Err(rejected(
                    "chance result contains a duplicate authoritative field",
                ));
            }
            snapshot.push(((*key).to_owned(), value.clone()));
        }
    }
    Ok(snapshot)
}

fn total_pattern_count(result: &CoreExecutionResult) -> Result<u128, PcChanceExecutionError> {
    let direct = optional_strict_u128_field(result, "total_pattern_count")?;
    let cooperative = optional_strict_u128_field(result, "total_possible_pattern_count")?;
    match (direct, cooperative) {
        (Some(left), Some(right)) if left == right => Ok(left),
        (Some(_), Some(_)) => Err(rejected("reported total pattern count aliases disagree")),
        (Some(value), None) | (None, Some(value)) => Ok(value),
        (None, None) => Err(rejected("reported total pattern count is missing")),
    }
}

fn validate_probability_projection(
    result: &CoreExecutionResult,
    producer: PcChanceResultProducer,
    coverage: &PatternBitSet,
    weights: &WeightedPatternSet,
    weighted_probability: ProbabilityValue,
    materialized_probability_mass: ProbabilityValue,
) -> Result<(), PcChanceExecutionError> {
    match producer {
        PcChanceResultProducer::DirectPercentService => {
            let expected = direct_legacy_probability(coverage, weights)?;
            require_field(result, "coverage_probability", &expected)?;
            require_field(result, "probability", &expected)?;
            require_field(result, "weighted_probability", &expected)?;
            require_field(
                result,
                "materialized_probability_mass",
                &legacy_probability(materialized_probability_mass.get()),
            )
        }
        PcChanceResultProducer::CooperativeWasm => {
            require_field(
                result,
                "coverage_probability",
                &canonical_probability_v2(weighted_probability),
            )?;
            require_field(
                result,
                "materialized_probability_mass",
                &canonical_probability_v2(materialized_probability_mass),
            )?;
            if result.field("probability").is_some()
                || result.field("weighted_probability").is_some()
            {
                return Err(rejected(
                    "cooperative chance result carries an unsupported probability alias",
                ));
            }
            Ok(())
        }
    }
}

fn direct_legacy_probability(
    coverage: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<String, PcChanceExecutionError> {
    union_probability(coverage, weights)
        .map(|probability| legacy_probability(probability.get()))
        .map_err(|_| rejected("coverage union and raw probability weights do not agree"))
}

fn legacy_probability(value: f64) -> String {
    if value == 0.0 || value == 1.0 {
        return format!("{value:.0}");
    }
    format!("{value:.12}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn require_field(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: &str,
) -> Result<(), PcChanceExecutionError> {
    if result.field(key) == Some(expected) {
        Ok(())
    } else {
        Err(rejected(
            "chance result field is missing or does not match authority",
        ))
    }
}

fn require_optional_field(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: &str,
) -> Result<(), PcChanceExecutionError> {
    match result.field(key) {
        None => Ok(()),
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(rejected(
            "optional chance result field does not match authority",
        )),
    }
}

fn require_true(
    result: &CoreExecutionResult,
    key: &'static str,
) -> Result<(), PcChanceExecutionError> {
    require_field(result, key, "true")
}

fn require_false(
    result: &CoreExecutionResult,
    key: &'static str,
) -> Result<(), PcChanceExecutionError> {
    require_field(result, key, "false")
}

fn require_usize(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: usize,
) -> Result<(), PcChanceExecutionError> {
    if strict_usize_field(result, key)? == expected {
        Ok(())
    } else {
        Err(rejected("chance result count does not match authority"))
    }
}

fn require_optional_usize(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: usize,
) -> Result<(), PcChanceExecutionError> {
    match result.field(key) {
        None => Ok(()),
        Some(_) if strict_usize_field(result, key)? == expected => Ok(()),
        Some(_) => Err(rejected(
            "optional chance result count does not match authority",
        )),
    }
}

fn require_optional_u64(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: u64,
) -> Result<(), PcChanceExecutionError> {
    match result.field(key) {
        None => Ok(()),
        Some(value) => {
            let parsed = value
                .parse::<u64>()
                .map_err(|_| rejected("chance result identity is not an unsigned integer"))?;
            if parsed == expected && value == parsed.to_string() {
                Ok(())
            } else {
                Err(rejected("chance result identity does not match authority"))
            }
        }
    }
}

fn strict_usize_field(
    result: &CoreExecutionResult,
    key: &'static str,
) -> Result<usize, PcChanceExecutionError> {
    let value = result
        .field(key)
        .ok_or_else(|| rejected("required chance result count is missing"))?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| rejected("chance result count is not an unsigned integer"))?;
    if value != parsed.to_string() {
        return Err(rejected("chance result count is not canonical"));
    }
    Ok(parsed)
}

fn optional_strict_u128_field(
    result: &CoreExecutionResult,
    key: &'static str,
) -> Result<Option<u128>, PcChanceExecutionError> {
    let Some(value) = result.field(key) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<u128>()
        .map_err(|_| rejected("chance total pattern count is not an unsigned integer"))?;
    if value != parsed.to_string() {
        return Err(rejected("chance total pattern count is not canonical"));
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod probability_alias_tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    };

    use super::{direct_legacy_probability, legacy_probability};

    #[test]
    fn direct_alias_uses_union_order_while_v2_authority_uses_canonical_covered_weight() {
        let explicit = WeightedPatternSet::new(
            [0.1, 0.2, 0.3, 0.4]
                .into_iter()
                .map(|value| ProbabilityValue::new(value).expect("probability"))
                .collect(),
        )
        .expect("explicit weights");
        let explicit_coverage = PatternBitSet::from_patterns(
            4,
            [PatternId::new(0), PatternId::new(2), PatternId::new(3)],
        )
        .expect("explicit coverage");
        let explicit_union = clearra_coverage::probability::union_probability::union_probability(
            &explicit_coverage,
            &explicit,
        )
        .expect("explicit union probability");
        assert_eq!(
            direct_legacy_probability(&explicit_coverage, &explicit).expect("legacy alias"),
            legacy_probability(explicit_union.get())
        );

        let weight = ProbabilityValue::new(1.0 / 7.0).expect("uniform weight");
        let terminal = WeightedPatternSet::uniform_with_terminal_remainder(7, weight)
            .expect("terminal-remainder weights");
        let full_coverage = PatternBitSet::from_patterns(7, (0..7).map(PatternId::new))
            .expect("full terminal coverage");
        let wrapper_authority = terminal
            .covered_weight(&full_coverage)
            .expect("canonical covered weight");
        let producer_alias = clearra_coverage::probability::union_probability::union_probability(
            &full_coverage,
            &terminal,
        )
        .expect("producer union probability");
        assert_eq!(wrapper_authority.get().to_bits(), 1.0_f64.to_bits());
        assert_ne!(
            wrapper_authority.get().to_bits(),
            producer_alias.get().to_bits(),
            "the test must exercise the summation-order distinction"
        );
        assert_eq!(
            direct_legacy_probability(&full_coverage, &terminal).expect("legacy alias"),
            legacy_probability(producer_alias.get())
        );
    }
}

#[cfg(test)]
mod constraint_tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{PcChanceCompiledAuthority, PcChanceIngressOrigin};
    use crate::{AppCoreExecutorService, ExecutionControl};

    fn query(height: u16, profile: SpinProfileSelection) -> PcScenarioQuery {
        let mask = if height == 1 {
            0x3f0
        } else {
            (0..4).fold(0_u64, |mask, row| mask | (0x3fe_u64 << (row * 10)))
        };
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::unique().with_back_to_back_preservation(profile))
    }

    #[test]
    fn compiled_chance_authority_binds_the_complete_constraint_objective() {
        let constrained = query(4, SpinProfileSelection::AllSpinPlus);
        let problem = ProblemCompiler::compile_scenario_percent(&constrained)
            .expect("tiny constrained chance problem")
            .with_pc_chance_probability_v2_evidence();
        PcChanceCompiledAuthority::scenario(
            &constrained,
            PcChanceIngressOrigin::CanonicalPcChance,
            &problem,
        )
        .expect("matching B2B objective");
        for objective in [
            ObjectivePolicy::unique(),
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        ] {
            let altered = constrained.clone().with_objective(objective);
            let error = PcChanceCompiledAuthority::scenario(
                &altered,
                PcChanceIngressOrigin::CanonicalPcChance,
                &problem,
            )
            .expect_err("compiled objective must exactly match the originating query");
            assert_eq!(
                error.reason(),
                "compiled chance problem does not preserve the validated objective contract"
            );
        }
        let noncanonical = constrained.with_objective(
            ObjectivePolicy::new(
                clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique,
                clearra_core_domain::objective::tie_policy::TiePolicy::StableInputOrder,
                clearra_core_domain::objective::trace_policy::TracePolicy::Discard,
            )
            .with_back_to_back_preservation(SpinProfileSelection::AllSpinPlus),
        );
        let noncanonical_problem = ProblemCompiler::compile_scenario_percent(&noncanonical)
            .expect("syntactically valid noncanonical objective")
            .with_pc_chance_probability_v2_evidence();
        assert!(
            PcChanceCompiledAuthority::scenario(
                &noncanonical,
                PcChanceIngressOrigin::CanonicalPcChance,
                &noncanonical_problem,
            )
            .is_err(),
            "allowing B2B must not loosen tie or trace semantics"
        );
    }

    #[test]
    fn filtered_chance_rows_survive_app_postprocessing_without_replay_materialization() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let service = AppCoreExecutorService::wasm_cpu();
        let control = ExecutionControl::default();
        for profile in [
            SpinProfileSelection::TSpins,
            SpinProfileSelection::TSpinsPlus,
            SpinProfileSelection::AllSpin,
            SpinProfileSelection::AllSpinPlus,
            SpinProfileSelection::AllMini,
            SpinProfileSelection::AllMiniPlus,
        ] {
            for height in [1, 4] {
                let query = query(height, profile);
                let problem = ProblemCompiler::compile_scenario_percent(&query)
                    .expect("tiny constrained chance problem")
                    .with_pc_chance_probability_v2_evidence();
                let authority = PcChanceCompiledAuthority::scenario(
                    &query,
                    PcChanceIngressOrigin::CanonicalPcChance,
                    &problem,
                )
                .expect("matching chance authority");
                let result = service
                    .execute_pc_chance_with_control(&problem, &control)
                    .expect("bounded constrained chance finishes through the App seam");
                let validated = authority
                    .validate_execution_result(&problem, &result)
                    .expect("filtered rows remain the exact probability authority");
                // Both one-I fixtures finish in a perfect clear. The PC
                // exception preserves B2B even for the one-line clear, for
                // every spin recognition profile.
                assert_eq!(validated.report().covered_pattern_count(), 1);
                assert_eq!(validated.report().weighted_probability(), "1");
                assert!(result.exact_scoring_execution_batches().is_empty());
                assert!(result.postprocess_pattern_weights().is_empty());
                assert_eq!(
                    result.field("unique_solution_count"),
                    Some("not-calculated")
                );
                assert!(validated.report().completeness().complete());
            }
        }
    }

    #[test]
    fn chance_filter_authority_is_checked_before_detaching_rows_and_after_postprocessing() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let query = query(4, SpinProfileSelection::TSpins);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("tiny constrained chance problem")
            .with_pc_chance_probability_v2_evidence();
        let service = AppCoreExecutorService::wasm_cpu();
        let control = ExecutionControl::default();
        let raw_result =
            clearra_core_executor::CoreExecutor::execute_with_control(&problem, &control)
                .expect("bounded constrained chance producer");
        let result = service
            .postprocess_pc_chance_result_before_public_surface(
                &problem,
                raw_result.clone(),
                &control,
            )
            .expect("untampered producer passes the public-surface seam");
        let authority = PcChanceCompiledAuthority::scenario(
            &query,
            PcChanceIngressOrigin::CanonicalPcChance,
            &problem,
        )
        .expect("matching chance authority");
        let foreign_query = query.clone().with_objective(
            ObjectivePolicy::unique()
                .with_back_to_back_preservation(SpinProfileSelection::AllSpinPlus),
        );
        let foreign_problem = ProblemCompiler::compile_scenario_percent(&foreign_query)
            .expect("other valid chance constraint")
            .with_pc_chance_probability_v2_evidence();
        let foreign_result = service
            .execute_pc_chance_with_control(&foreign_problem, &control)
            .expect("other bounded chance result");
        assert_eq!(
            authority
                .validate_execution_result(&foreign_problem, &foreign_result)
                .expect_err("a matching foreign problem/result cannot replace this query")
                .reason(),
            "executed pc chance objective does not match the query authority"
        );
        for (name, value) in [
            ("execution_constraint_preserve_b2b", "false"),
            ("execution_constraint_materialized", "false"),
            ("execution_constraint_spin_profile", "all-spin-plus"),
            (
                "b2b_preservation_evaluation_basis",
                "filtered-build-order-language",
            ),
            ("b2b_preservation_selection", "all"),
            ("b2b_preservation_denominator_semantics", "filtered-queues"),
            ("b2b_preservation_path_multiplicity_counted", "true"),
            ("b2b_preserving_pattern_count", "0"),
            ("b2b_preservation_pattern_universe_count", "2"),
            ("b2b_preservation_probability", "0"),
            ("b2b_preservation_count_complete", "false"),
            ("b2b_preservation_probability_complete", "false"),
        ] {
            let changed_raw = raw_result
                .clone()
                .with_replaced_fields(vec![(name.to_owned(), value.to_owned())]);
            assert!(
                service
                    .postprocess_pc_chance_result_before_public_surface(
                        &problem,
                        changed_raw,
                        &control
                    )
                    .is_err(),
                "{name}"
            );
            let changed = result
                .clone()
                .with_replaced_fields(vec![(name.to_owned(), value.to_owned())]);
            assert!(
                authority
                    .validate_execution_result(&problem, &changed)
                    .is_err(),
                "{name}"
            );
        }
        let changed_raw = raw_result.clone().with_replaced_fields(vec![(
            "b2b_preserving_candidate_pattern_count".to_owned(),
            "0".to_owned(),
        )]);
        assert!(service
            .postprocess_pc_chance_result_before_public_surface(&problem, changed_raw, &control)
            .is_err());
        for (key, value) in [
            ("search_output_policy", "trace"),
            ("b2b_preserving_candidate_pattern_count", "0"),
        ] {
            let duplicated = raw_result
                .clone()
                .with_additional_fields(vec![(key.to_owned(), value.to_owned())]);
            let error = service
                .postprocess_pc_chance_result_before_public_surface(&problem, duplicated, &control)
                .expect_err("summary canonicalization must not hide conflicting producer fields");
            assert!(
                matches!(&error, clearra_core_executor::CoreExecutionError::Pc(message)
                    if message.contains("duplicate authoritative field")),
                "{error:?}"
            );
        }
        let changed = result.with_replaced_fields(vec![(
            "b2b_preserving_candidate_pattern_count".to_owned(),
            "0".to_owned(),
        )]);
        assert_eq!(
            authority
                .validate_execution_result(&problem, &changed)
                .expect_err("private witness count must not leak through public summary")
                .reason(),
            "pc chance B2B witness count must remain private"
        );
    }
}

#[cfg(all(test, feature = "native-c-core"))]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_executor::{CoreExecutionResult, PercentService};
    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{ProblemCompiler, SearchProblem};
    use clearra_rules::profile::{
        builtin_rules::{srs, srs_plus},
        rule_profile::RuleProfile,
    };
    use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

    use super::{PcChanceCompiledAuthority, PcChanceIngressOrigin};

    struct RawChanceExecution {
        problem: SearchProblem,
        authority: PcChanceCompiledAuthority,
        result: CoreExecutionResult,
    }

    fn raw_chance_execution(
        board_height: u16,
        board_mask: u64,
        piece: PieceKind,
        rule: RuleProfile,
    ) -> RawChanceExecution {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(board_height, board_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_rule(rule);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("tiny chance problem compiles")
            .with_pc_chance_probability_v2_evidence();
        let authority = PcChanceCompiledAuthority::scenario(
            &query,
            PcChanceIngressOrigin::CanonicalPcChance,
            &problem,
        )
        .expect("tiny chance authority");
        let result = PercentService::execute(&problem).expect("tiny native percent result");
        RawChanceExecution {
            problem,
            authority,
            result,
        }
    }

    fn solvable() -> RawChanceExecution {
        raw_chance_execution(1, 0x3f0, PieceKind::I, srs_plus())
    }

    fn unsolved() -> RawChanceExecution {
        raw_chance_execution(1, 0x3f0, PieceKind::O, srs_plus())
    }

    fn rejected_reason(
        execution: &RawChanceExecution,
        result: &CoreExecutionResult,
    ) -> &'static str {
        execution
            .authority
            .validate_execution_result(&execution.problem, result)
            .expect_err("tampered chance result must fail closed")
            .reason()
    }

    #[test]
    fn compiled_authority_rejects_visible_seven_before_native_execution() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        let problem = ProblemCompiler::compile_scenario_percent(&query)
            .expect("visible-seven percent problem compiles")
            .with_pc_chance_probability_v2_evidence();
        let error = PcChanceCompiledAuthority::scenario(
            &query,
            PcChanceIngressOrigin::CanonicalPcChance,
            &problem,
        )
        .expect_err("typed pc chance authority must reject visible-seven semantics");
        assert_eq!(
            error.reason(),
            "pc chance requires full-queue oracle knowledge in both query and compiled problem"
        );
    }

    fn field(key: &str, value: impl ToString) -> (String, String) {
        (key.to_owned(), value.to_string())
    }

    #[test]
    fn tiny_native_percent_is_accepted_only_with_its_full_typed_evidence() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let execution = solvable();
        let evidence = execution
            .authority
            .validate_execution_result(&execution.problem, &execution.result)
            .expect("untampered tiny chance result");
        assert_eq!(evidence.report().covered_pattern_count(), 1);
        assert_eq!(evidence.report().coverage_pattern_words(), &[1]);
        assert_eq!(evidence.report().weighted_probability(), "1");
    }

    #[test]
    fn strict_aggregate_rejects_padding_and_both_union_directions() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let execution = solvable();
        let padding = execution
            .result
            .clone()
            .with_coverage_pattern_words(vec![1 | (1_u64 << 63)]);
        assert_eq!(
            rejected_reason(&execution, &padding),
            "public coverage aggregate is not a strict universe bitset"
        );

        let omitted = execution
            .result
            .clone()
            .with_coverage_pattern_words(vec![0]);
        assert_eq!(
            rejected_reason(&execution, &omitted),
            "public coverage aggregate is not the coverage-row union"
        );

        let no_solution = unsolved();
        assert_eq!(no_solution.result.coverage_pattern_words(), &[0]);
        let extra = no_solution
            .result
            .clone()
            .with_coverage_pattern_words(vec![1]);
        assert_eq!(
            rejected_reason(&no_solution, &extra),
            "public coverage aggregate is not the coverage-row union"
        );
    }

    #[test]
    fn duplicate_authoritative_fields_and_legacy_alias_tampering_fail_closed() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let execution = solvable();
        let duplicate = execution
            .result
            .clone()
            .with_additional_fields(vec![field("coverage_pattern_count", 1)]);
        assert_eq!(
            rejected_reason(&execution, &duplicate),
            "chance result contains a duplicate authoritative field"
        );

        let alias = execution.result.clone().with_replaced_fields(vec![
            field("probability", "0.123456789012"),
            field("weighted_probability", "0.123456789012"),
            field("coverage_probability", "0.123456789012"),
        ]);
        assert_eq!(
            rejected_reason(&execution, &alias),
            "chance result field is missing or does not match authority"
        );
    }

    #[test]
    fn reported_counts_and_identity_aliases_must_match_compiled_authority() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let execution = solvable();
        let wrong_count = execution
            .result
            .clone()
            .with_replaced_fields(vec![field("coverage_pattern_count", 2)]);
        assert_eq!(
            rejected_reason(&execution, &wrong_count),
            "chance result count does not match authority"
        );

        let wrong_row_count = execution
            .result
            .clone()
            .with_replaced_fields(vec![field("c_buildup_coverage_row_count", 2)]);
        assert_eq!(
            rejected_reason(&execution, &wrong_row_count),
            "coverage-row evidence count does not match the result"
        );

        let wrong_source_id = execution
            .result
            .clone()
            .with_replaced_fields(vec![field("piece_source_id", u64::MAX)]);
        assert_eq!(
            rejected_reason(&execution, &wrong_source_id),
            "chance result identity does not match authority"
        );
    }

    #[test]
    fn incomplete_resource_or_missing_transient_evidence_cannot_succeed() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let execution = solvable();
        let incomplete = execution
            .result
            .clone()
            .with_replaced_fields(vec![field("resource_probability_complete", false)]);
        assert_eq!(
            rejected_reason(&execution, &incomplete),
            "chance result field is missing or does not match authority"
        );

        let missing = execution
            .result
            .clone()
            .without_pc_chance_transient_evidence();
        assert_eq!(
            rejected_reason(&execution, &missing),
            "pc chance coverage-row evidence is missing"
        );
    }

    #[test]
    fn same_problem_id_rule_swap_and_foreign_batch_identity_are_rejected() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let expected = solvable();
        let executed_under_other_rule = raw_chance_execution(1, 0x3f0, PieceKind::I, srs());
        assert_eq!(
            expected.problem.problem_id(),
            executed_under_other_rule.problem.problem_id(),
            "the corroborative problem id intentionally omits the rule distinction"
        );
        assert_eq!(
            rejected_reason(&expected, &executed_under_other_rule.result),
            "executed pc chance problem does not match the expected compiled problem"
        );

        let validated = expected
            .authority
            .validate_execution_result(&expected.problem, &expected.result)
            .expect("expected result validates before finalization");
        let swapped_after_validation = expected.result.clone().with_pc_chance_transient_evidence(
            executed_under_other_rule
                .result
                .pc_chance_coverage_evidence()
                .expect("other-rule evidence")
                .clone(),
        );
        assert!(
            !validated.matches_core_result(&swapped_after_validation),
            "finalization must bind the exact fieldwise problem evidence validated earlier"
        );

        let foreign = unsolved();
        let foreign_evidence = foreign
            .result
            .pc_chance_coverage_evidence()
            .expect("foreign batch")
            .clone();
        let wrong_batch = expected
            .result
            .clone()
            .with_pc_chance_transient_evidence(foreign_evidence);
        assert_eq!(
            rejected_reason(&expected, &wrong_batch),
            "executed pc chance problem does not match the expected compiled problem"
        );
    }
}
