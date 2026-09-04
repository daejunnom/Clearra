// SRP rationale: this module has one behavior-level change reason: binding validated PC score execution evidence into typed summary and witness result contracts.

use std::{fmt, mem::size_of, sync::Arc};

use clearra_core_domain::resource::ResourceReport;
use clearra_core_executor::{
    CoreExecutionResult, PcScoreDistributedMergeEvidence, WasmCpuTerminalResourceAuthority,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectiveMode, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery, PcSolutionProbabilityPolicy,
};
use clearra_problem::{
    PcChanceEvidencePolicy, ProblemCompileError, ProblemCompiler, SearchOutputPolicy,
    SearchProblem, SearchProblemPreset,
};
use clearra_supply::QueueObservationPolicy;

use crate::{
    pc_result_projection::{
        validate_pc_score_minimals_opening_request_contract,
        validate_pc_score_minimals_scenario_request_contract,
        validate_pc_score_opening_request_contract, validate_pc_score_scenario_request_contract,
        PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES,
    },
    pc_score_field_result::{
        PcScoreSolutionFieldAverageV1, PC_SCORE_OVERALL_SCORE_BASIS,
        PC_SCORE_SOLUTION_FIELD_AVERAGE_BASIS, PC_SCORE_SOLUTION_FIELD_ORDERING,
    },
    pc_score_minimum_cover_result::PcScoreMinimalsIngressOrigin,
    pc_score_postprocess::{score_profile_for_policy, PcScoreDerivation, PcScoreExecutionSource},
    pc_score_winner_result::{
        canonical_score_winner, PcScorePatternWinnerV1, PC_SCORE_CANONICAL_SELECTION,
        PC_SCORE_INFORMATIONAL_ATTACK_BASIS,
    },
};

pub(crate) const PC_SCORE_RESULT_CONTRACT: &str = "pc-score-summary.v2";
pub(crate) const PC_FIXED_SCORE_WITNESS_RESULT_CONTRACT: &str = "pc-fixed-score-witness.v2";
pub(crate) const PC_SCORE_ACCURACY_LEVEL: &str = "basic-approximation";
pub(crate) const PC_SCORE_ACCURACY_REASON: &str =
    "profile-specific basic score/attack tables with configurable spin detection";

// Rust's finite-f64 Display form can expand to more than 300 decimal bytes.
// Keep the representation inline while covering that complete domain.
const SCORE_NUMBER_TEXT_CAPACITY: usize = 384;

#[derive(Clone, Eq, PartialEq)]
struct InlineScoreNumberText {
    bytes: [u8; SCORE_NUMBER_TEXT_CAPACITY],
    len: u16,
}

impl InlineScoreNumberText {
    fn try_from_display(value: impl fmt::Display) -> Result<Self, PcScoreExecutionError> {
        use fmt::Write;

        let mut text = Self {
            bytes: [0; SCORE_NUMBER_TEXT_CAPACITY],
            len: 0,
        };
        write!(&mut text, "{value}")
            .map_err(|_| rejected("pc_score_summary_number_text_overflow"))?;
        Ok(text)
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("fmt::Write only accepts valid UTF-8 strings")
    }
}

impl fmt::Write for InlineScoreNumberText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let start = usize::from(self.len);
        let end = start.checked_add(value.len()).ok_or(fmt::Error)?;
        let target = self.bytes.get_mut(start..end).ok_or(fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.len = u16::try_from(end).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

impl fmt::Debug for InlineScoreNumberText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("InlineScoreNumberText")
            .field(&self.as_str())
            .finish()
    }
}

/// Closed identity for the Web spelling that selected `pc.score`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreIngressOrigin {
    CanonicalPcScore,
    CompatibilityScore,
    CanonicalPcScoreFinder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcScoreCompiledProduct {
    Summary(PcScoreIngressOrigin),
    Portfolio(PcScoreMinimalsIngressOrigin),
}

impl PcScoreCompiledProduct {
    const fn score_execution_origin(self) -> PcScoreIngressOrigin {
        match self {
            Self::Summary(origin) => origin,
            Self::Portfolio(PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals) => {
                PcScoreIngressOrigin::CanonicalPcScore
            }
        }
    }

    fn expected_objective(
        self,
        score_policy: clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy,
    ) -> ObjectivePolicy {
        match self {
            Self::Summary(_) => ObjectivePolicy::all().with_score_policy(score_policy),
            Self::Portfolio(_) => ObjectivePolicy::minimum_cover().with_score_policy(score_policy),
        }
    }
}

impl PcScoreIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcScore => "canonical-pc-score",
            Self::CompatibilityScore => "compatibility-score",
            Self::CanonicalPcScoreFinder => "canonical-pc-score-finder",
        }
    }

    pub const fn is_score_finder(self) -> bool {
        matches!(self, Self::CanonicalPcScoreFinder)
    }

    pub(crate) const fn result_contract(self) -> &'static str {
        if self.is_score_finder() {
            PC_FIXED_SCORE_WITNESS_RESULT_CONTRACT
        } else {
            PC_SCORE_RESULT_CONTRACT
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcScoreQuerySnapshot {
    Opening(Arc<OpeningPcSearchQuery>),
    Scenario(Arc<PcScenarioQuery>),
}

impl PcScoreQuerySnapshot {
    pub const fn problem_preset(&self) -> PcScoreProblemPreset {
        match self {
            Self::Opening(_) => PcScoreProblemPreset::OpeningPc,
            Self::Scenario(_) => PcScoreProblemPreset::ScenarioPc,
        }
    }

    /// Complete retained bytes of the snapshot pointee and its query pointee.
    ///
    /// The outer `Arc<PcScoreQuerySnapshot>` handle/control block is excluded.
    /// The nested query `Arc` handle is included by `size_of::<Self>()`, while
    /// the pointer-identical query pointee and its queue allocation are counted
    /// exactly once here.
    pub(crate) fn checked_pointee_retained_bytes(&self) -> Option<u128> {
        let query_pointee_bytes = match self {
            Self::Opening(query) => (size_of::<OpeningPcSearchQuery>() as u128)
                .checked_add(query.queue().checked_pc_score_retained_capacity_bytes()?)?,
            Self::Scenario(query) => (size_of::<PcScenarioQuery>() as u128).checked_add(
                query
                    .remaining_queue()
                    .checked_pc_score_retained_capacity_bytes()?,
            )?,
        };
        (size_of::<Self>() as u128).checked_add(query_pointee_bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcScoreProblemPreset {
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
pub struct PcScoreCompletenessEvidence {
    source_universe_complete: bool,
    execution_source_complete: bool,
    objective_complete: bool,
    count_complete: bool,
    probability_complete: bool,
    resource_probability_complete: bool,
    matrix_complete: bool,
    summary_complete: bool,
}

impl PcScoreCompletenessEvidence {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        source_universe_complete: bool,
        execution_source_complete: bool,
        objective_complete: bool,
        count_complete: bool,
        probability_complete: bool,
        resource_probability_complete: bool,
        matrix_complete: bool,
        summary_complete: bool,
    ) -> Self {
        Self {
            source_universe_complete,
            execution_source_complete,
            objective_complete,
            count_complete,
            probability_complete,
            resource_probability_complete,
            matrix_complete,
            summary_complete,
        }
    }

    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn execution_source_complete(self) -> bool {
        self.execution_source_complete
    }

    pub const fn objective_complete(self) -> bool {
        self.objective_complete
    }

    pub const fn count_complete(self) -> bool {
        self.count_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn resource_probability_complete(self) -> bool {
        self.resource_probability_complete
    }

    pub const fn matrix_complete(self) -> bool {
        self.matrix_complete
    }

    pub const fn summary_complete(self) -> bool {
        self.summary_complete
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.execution_source_complete
            && self.objective_complete
            && self.count_complete
            && self.probability_complete
            && self.resource_probability_complete
            && self.matrix_complete
            && self.summary_complete
    }
}

/// Closed, authority-checked projection for the internal `pc.score` candidate.
///
/// Profile names identify selected Clearra table bundles only. The accuracy
/// fields deliberately cannot express provider-specific exactness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScoreSummaryV2Result {
    contract_id: &'static str,
    origin: PcScoreIngressOrigin,
    query: Arc<PcScoreQuerySnapshot>,
    problem_preset: PcScoreProblemPreset,
    problem_id: Arc<str>,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    score_profile_selection: ScoreProfileSelection,
    spin_profile_selection: SpinProfileSelection,
    initial_b2b: u32,
    score_profile_id: Arc<str>,
    accuracy_level: &'static str,
    accuracy_reason: &'static str,
    profile_specific_exact: bool,
    materialized_pattern_count: usize,
    total_pattern_count: u128,
    matrix_cell_count: usize,
    all_universe_patterns_covered: bool,
    pattern_optimal_count: usize,
    failed_pc_pattern_count: usize,
    best_score: Option<u64>,
    best_attack: Option<u32>,
    pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    canonical_winner: Option<PcScorePatternWinnerV1>,
    solution_field_averages: Arc<Vec<PcScoreSolutionFieldAverageV1>>,
    score_evaluation_basis: &'static str,
    score_evaluation_scope: &'static str,
    solution_field_average_basis: &'static str,
    overall_score_basis: &'static str,
    overall_score_bits: u64,
    overall_score: InlineScoreNumberText,
    covered_probability_bits: u64,
    covered_probability: InlineScoreNumberText,
    unconditional_expected_score_bits: u64,
    unconditional_expected_score: InlineScoreNumberText,
    unconditional_expected_attack_bits: u64,
    unconditional_expected_attack: InlineScoreNumberText,
    covered_pattern_conditional_average_score_bits: Option<u64>,
    covered_pattern_conditional_average_score: Option<InlineScoreNumberText>,
    completeness: PcScoreCompletenessEvidence,
}

impl PcScoreSummaryV2Result {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        origin: PcScoreIngressOrigin,
        query: Arc<PcScoreQuerySnapshot>,
        problem_id: Arc<str>,
        piece_source_id: u64,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        score_profile_selection: ScoreProfileSelection,
        spin_profile_selection: SpinProfileSelection,
        initial_b2b: u32,
        score_profile_id: Arc<str>,
        materialized_pattern_count: usize,
        total_pattern_count: u128,
        matrix_cell_count: usize,
        all_universe_patterns_covered: bool,
        pattern_optimal_count: usize,
        failed_pc_pattern_count: usize,
        best_score: Option<u64>,
        best_attack: Option<u32>,
        pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
        solution_field_averages: Arc<Vec<PcScoreSolutionFieldAverageV1>>,
        overall_score: f64,
        covered_probability: f64,
        unconditional_expected_score: f64,
        unconditional_expected_attack: f64,
        covered_pattern_conditional_average_score: Option<f64>,
        completeness: PcScoreCompletenessEvidence,
    ) -> Result<Self, PcScoreExecutionError> {
        let problem_preset = query.problem_preset();
        // The typed App result owns the score-tie witness. Candidate ID is the
        // only selector here; informational attack is deliberately not read by
        // this projection or by downstream adapters.
        let canonical_winner = canonical_score_winner(pattern_winners.as_slice());
        Ok(Self {
            contract_id: origin.result_contract(),
            origin,
            query,
            problem_preset,
            problem_id,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            score_profile_selection,
            spin_profile_selection,
            initial_b2b,
            score_profile_id,
            accuracy_level: PC_SCORE_ACCURACY_LEVEL,
            accuracy_reason: PC_SCORE_ACCURACY_REASON,
            profile_specific_exact: false,
            materialized_pattern_count,
            total_pattern_count,
            matrix_cell_count,
            all_universe_patterns_covered,
            pattern_optimal_count,
            failed_pc_pattern_count,
            best_score,
            best_attack,
            pattern_winners,
            canonical_winner,
            solution_field_averages,
            score_evaluation_basis: "all-traces",
            score_evaluation_scope: "full",
            solution_field_average_basis: PC_SCORE_SOLUTION_FIELD_AVERAGE_BASIS,
            overall_score_basis: PC_SCORE_OVERALL_SCORE_BASIS,
            overall_score_bits: overall_score.to_bits(),
            overall_score: InlineScoreNumberText::try_from_display(overall_score)?,
            covered_probability_bits: covered_probability.to_bits(),
            covered_probability: InlineScoreNumberText::try_from_display(covered_probability)?,
            unconditional_expected_score_bits: unconditional_expected_score.to_bits(),
            unconditional_expected_score: InlineScoreNumberText::try_from_display(
                unconditional_expected_score,
            )?,
            unconditional_expected_attack_bits: unconditional_expected_attack.to_bits(),
            unconditional_expected_attack: InlineScoreNumberText::try_from_display(
                unconditional_expected_attack,
            )?,
            covered_pattern_conditional_average_score_bits:
                covered_pattern_conditional_average_score.map(f64::to_bits),
            covered_pattern_conditional_average_score: covered_pattern_conditional_average_score
                .map(InlineScoreNumberText::try_from_display)
                .transpose()?,
            completeness,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcScoreIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcScoreQuerySnapshot {
        self.query.as_ref()
    }

    pub const fn problem_preset(&self) -> PcScoreProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        self.problem_id.as_ref()
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

    pub const fn score_profile_selection(&self) -> ScoreProfileSelection {
        self.score_profile_selection
    }

    pub const fn spin_profile_selection(&self) -> SpinProfileSelection {
        self.spin_profile_selection
    }

    pub const fn initial_b2b(&self) -> u32 {
        self.initial_b2b
    }

    pub fn score_profile_id(&self) -> &str {
        self.score_profile_id.as_ref()
    }

    pub const fn accuracy_level(&self) -> &'static str {
        self.accuracy_level
    }

    pub const fn accuracy_reason(&self) -> &'static str {
        self.accuracy_reason
    }

    pub const fn profile_specific_exact(&self) -> bool {
        self.profile_specific_exact
    }

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub const fn total_pattern_count(&self) -> u128 {
        self.total_pattern_count
    }

    pub const fn matrix_cell_count(&self) -> usize {
        self.matrix_cell_count
    }

    pub const fn all_universe_patterns_covered(&self) -> bool {
        self.all_universe_patterns_covered
    }

    pub const fn pattern_optimal_count(&self) -> usize {
        self.pattern_optimal_count
    }

    pub const fn failed_pc_pattern_count(&self) -> usize {
        self.failed_pc_pattern_count
    }

    pub const fn best_score(&self) -> Option<u64> {
        self.best_score
    }

    pub const fn best_attack(&self) -> Option<u32> {
        self.best_attack
    }

    /// Complete score-only maximum family, ordered by `(pattern_id,
    /// candidate_id)`. Attack values are informational projections of each
    /// candidate's canonical equal-score trace.
    pub fn pattern_winners(&self) -> &[PcScorePatternWinnerV1] {
        self.pattern_winners.as_slice()
    }

    pub fn pattern_winner_count(&self) -> usize {
        self.pattern_winners.len()
    }

    /// App-owned representative for a score-only winner family. Downstream
    /// adapters must serialize this witness and must not choose another
    /// representative themselves.
    pub const fn canonical_winner(&self) -> Option<PcScorePatternWinnerV1> {
        self.canonical_winner
    }

    pub const fn canonical_selection(&self) -> &'static str {
        PC_SCORE_CANONICAL_SELECTION
    }

    /// Exactly one row per normalized solution field. Every row is averaged
    /// over the whole materialized pattern universe; patterns that this field
    /// cannot solve contribute zero.
    pub fn solution_field_averages(&self) -> &[PcScoreSolutionFieldAverageV1] {
        self.solution_field_averages.as_slice()
    }

    pub fn solution_field_count(&self) -> usize {
        self.solution_field_averages.len()
    }

    pub const fn solution_field_ordering(&self) -> &'static str {
        PC_SCORE_SOLUTION_FIELD_ORDERING
    }

    pub const fn score_evaluation_basis(&self) -> &'static str {
        self.score_evaluation_basis
    }

    pub const fn score_evaluation_scope(&self) -> &'static str {
        self.score_evaluation_scope
    }

    pub const fn solution_field_average_basis(&self) -> &'static str {
        self.solution_field_average_basis
    }

    pub const fn overall_score_basis(&self) -> &'static str {
        self.overall_score_basis
    }

    pub const fn overall_score_bits(&self) -> u64 {
        self.overall_score_bits
    }

    pub fn overall_score(&self) -> &str {
        self.overall_score.as_str()
    }

    pub const fn informational_attack_basis(&self) -> &'static str {
        PC_SCORE_INFORMATIONAL_ATTACK_BASIS
    }

    pub const fn covered_probability_bits(&self) -> u64 {
        self.covered_probability_bits
    }

    pub fn covered_probability(&self) -> &str {
        self.covered_probability.as_str()
    }

    pub const fn unconditional_expected_score_bits(&self) -> u64 {
        self.unconditional_expected_score_bits
    }

    pub fn unconditional_expected_score(&self) -> &str {
        self.unconditional_expected_score.as_str()
    }

    pub const fn unconditional_expected_attack_bits(&self) -> u64 {
        self.unconditional_expected_attack_bits
    }

    pub fn unconditional_expected_attack(&self) -> &str {
        self.unconditional_expected_attack.as_str()
    }

    pub const fn covered_pattern_conditional_average_score_bits(&self) -> Option<u64> {
        self.covered_pattern_conditional_average_score_bits
    }

    pub fn covered_pattern_conditional_average_score(&self) -> Option<&str> {
        self.covered_pattern_conditional_average_score
            .as_ref()
            .map(InlineScoreNumberText::as_str)
    }

    pub const fn completeness(&self) -> PcScoreCompletenessEvidence {
        self.completeness
    }
}

/// Closed authority over the exact compiled problem used by the typed score
/// execution seam. Canonical construction consumes the immutable query,
/// acquires the complete shared execution surface before compilation, compiles
/// exactly once, and retains every owner through terminal post-processing.
pub(crate) struct PcScoreCompiledAuthority {
    product: PcScoreCompiledProduct,
    query: Arc<PcScoreQuerySnapshot>,
    problem: Arc<SearchProblem>,
    terminal_resource_authority: WasmCpuTerminalResourceAuthority,
    problem_id: Arc<str>,
    score_profile_id: Arc<str>,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    total_pattern_count: u128,
    external_retained_base_bytes: u128,
}

impl fmt::Debug for PcScoreCompiledAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PcScoreCompiledAuthority")
            .field("product", &self.product)
            .field("query", &self.query)
            .field("problem_id", &self.problem_id)
            .field("score_profile_id", &self.score_profile_id)
            .field("piece_source_id", &self.piece_source_id)
            .field("pattern_universe_id", &self.pattern_universe_id)
            .field("pattern_weight_model_id", &self.pattern_weight_model_id)
            .field(
                "materialized_pattern_count",
                &self.materialized_pattern_count,
            )
            .field("total_pattern_count", &self.total_pattern_count)
            .field(
                "external_retained_base_bytes",
                &self.external_retained_base_bytes,
            )
            .field(
                "terminal_memory_capacity_bytes",
                &self.terminal_resource_authority.memory_capacity_bytes(),
            )
            .finish_non_exhaustive()
    }
}

impl PcScoreCompiledAuthority {
    pub(crate) fn compile_opening(
        query: impl Into<Arc<OpeningPcSearchQuery>>,
        origin: PcScoreIngressOrigin,
    ) -> Result<Self, PcScoreCompiledAuthorityError> {
        Self::compile(
            Arc::new(PcScoreQuerySnapshot::Opening(query.into())),
            PcScoreCompiledProduct::Summary(origin),
        )
    }

    pub(crate) fn compile_scenario(
        query: impl Into<Arc<PcScenarioQuery>>,
        origin: PcScoreIngressOrigin,
    ) -> Result<Self, PcScoreCompiledAuthorityError> {
        Self::compile(
            Arc::new(PcScoreQuerySnapshot::Scenario(query.into())),
            PcScoreCompiledProduct::Summary(origin),
        )
    }

    pub(crate) fn compile_score_minimals_opening(
        query: impl Into<Arc<OpeningPcSearchQuery>>,
        origin: PcScoreMinimalsIngressOrigin,
    ) -> Result<Self, PcScoreCompiledAuthorityError> {
        Self::compile(
            Arc::new(PcScoreQuerySnapshot::Opening(query.into())),
            PcScoreCompiledProduct::Portfolio(origin),
        )
    }

    pub(crate) fn compile_score_minimals_scenario(
        query: impl Into<Arc<PcScenarioQuery>>,
        origin: PcScoreMinimalsIngressOrigin,
    ) -> Result<Self, PcScoreCompiledAuthorityError> {
        Self::compile(
            Arc::new(PcScoreQuerySnapshot::Scenario(query.into())),
            PcScoreCompiledProduct::Portfolio(origin),
        )
    }

    fn compile(
        query: Arc<PcScoreQuerySnapshot>,
        product: PcScoreCompiledProduct,
    ) -> Result<Self, PcScoreCompiledAuthorityError> {
        match (query.as_ref(), product) {
            (PcScoreQuerySnapshot::Opening(query), PcScoreCompiledProduct::Summary(origin)) => {
                validate_pc_score_opening_request_contract(query.as_ref(), origin)
            }
            (PcScoreQuerySnapshot::Scenario(query), PcScoreCompiledProduct::Summary(origin)) => {
                validate_pc_score_scenario_request_contract(query.as_ref(), origin)
            }
            (PcScoreQuerySnapshot::Opening(query), PcScoreCompiledProduct::Portfolio(_)) => {
                validate_pc_score_minimals_opening_request_contract(query.as_ref())
            }
            (PcScoreQuerySnapshot::Scenario(query), PcScoreCompiledProduct::Portfolio(_)) => {
                validate_pc_score_minimals_scenario_request_contract(query.as_ref())
            }
        }
        .map_err(|_| {
            PcScoreCompiledAuthorityError::Contract(rejected("pc_score_request_contract_rejected"))
        })?;

        let query_retained_bytes = (size_of::<Arc<PcScoreQuerySnapshot>>() as u128)
            .checked_add(query.checked_pointee_retained_bytes().ok_or_else(|| {
                PcScoreCompiledAuthorityError::Contract(rejected(
                    "pc_score_query_retained_projection_unavailable",
                ))
            })?)
            .ok_or_else(|| {
                PcScoreCompiledAuthorityError::Contract(rejected(
                    "pc_score_query_retained_projection_unavailable",
                ))
            })?;
        if query_retained_bytes > PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(PcScoreCompiledAuthorityError::Contract(rejected(
                "pc_score_query_retained_envelope_exceeded",
            )));
        }

        let terminal_resource_authority =
            WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
                .map_err(PcScoreCompiledAuthorityError::ResourceAdmission)?;
        let problem = match query.as_ref() {
            PcScoreQuerySnapshot::Opening(query) => {
                ProblemCompiler::compile_opening_pc(query.as_ref())
            }
            PcScoreQuerySnapshot::Scenario(query) => {
                ProblemCompiler::compile_scenario_pc(query.as_ref())
            }
        }
        .map(|problem| match product {
            PcScoreCompiledProduct::Summary(_) => problem,
            PcScoreCompiledProduct::Portfolio(_) => problem.with_pc_score_portfolio_v2_evidence(),
        })
        .map(Arc::new)
        .map_err(PcScoreCompiledAuthorityError::ProblemCompile)?;
        Self::new(query, product, problem, terminal_resource_authority)
            .map_err(PcScoreCompiledAuthorityError::Contract)
    }

    fn new(
        query: Arc<PcScoreQuerySnapshot>,
        product: PcScoreCompiledProduct,
        problem: Arc<SearchProblem>,
        terminal_resource_authority: WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, PcScoreExecutionError> {
        let problem_preset = query.problem_preset();
        if problem.preset() != problem_preset.search_problem_preset()
            || problem.output_policy() != SearchOutputPolicy::Trace
            || problem.goal().as_str() != "clear-to-empty"
            || problem.count_policy() != PcCountPolicy::CountAll
            || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
            || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
            || problem.allowed_colored_solution_identities().is_some()
        {
            return Err(rejected("pc_score_compiled_contract_mismatch"));
        }
        let score_policy = problem.objective().score();
        if score_policy.mode() != ScoreObjectiveMode::Summary
            || problem.objective() != product.expected_objective(score_policy)
        {
            return Err(rejected("pc_score_compiled_objective_mismatch"));
        }
        let evidence_policy_matches_product = match product {
            PcScoreCompiledProduct::Summary(_) => {
                problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::Disabled
            }
            PcScoreCompiledProduct::Portfolio(_) => problem
                .pc_chance_evidence_policy()
                .retains_pc_score_portfolio_v2_evidence(),
        };
        if !evidence_policy_matches_product {
            return Err(rejected("pc_score_compiled_evidence_policy_mismatch"));
        }
        if product == PcScoreCompiledProduct::Summary(PcScoreIngressOrigin::CompatibilityScore)
            && score_policy.profile() != ScoreProfileSelection::JstrisUltra
        {
            return Err(rejected("pc_score_compatibility_profile_mismatch"));
        }

        let query_execution = match query.as_ref() {
            PcScoreQuerySnapshot::Opening(query) => query.execution_policy(),
            PcScoreQuerySnapshot::Scenario(query) => query.execution_policy(),
        };
        if problem.backend_request() != query_execution {
            return Err(rejected("pc_score_compiled_execution_policy_mismatch"));
        }

        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .ok_or_else(|| rejected("pc_score_pattern_universe_missing"))?;
        let materialized_pattern_count = universe.pattern_count();
        let total_pattern_count = universe.total_possible_pattern_count();
        if source.id().get() == 0
            || universe.pattern_universe_id().get() == 0
            || universe.pattern_weight_model_id().get() == 0
            || !source.complete()
            || source.truncation_reason().is_some()
            || !universe.complete()
            || universe.truncation_reason().is_some()
            || materialized_pattern_count == 0
            || total_pattern_count != materialized_pattern_count as u128
            || universe.weights().len() != materialized_pattern_count
            || universe.materialized_probability_mass().get().to_bits() != 1.0_f64.to_bits()
        {
            return Err(rejected("pc_score_pattern_universe_incomplete"));
        }

        let problem_id: Arc<str> = Arc::from(problem.problem_id().as_str());
        let score_profile_id: Arc<str> = Arc::from(score_profile_for_policy(score_policy).id());
        let external_retained_base_bytes = (size_of::<Self>() as u128)
            .checked_add(
                query
                    .checked_pointee_retained_bytes()
                    .ok_or_else(|| rejected("pc_score_query_retained_projection_unavailable"))?,
            )
            .and_then(|bytes| {
                problem
                    .checked_pc_score_pointee_retained_bytes()
                    .and_then(|problem_bytes| bytes.checked_add(problem_bytes))
            })
            .and_then(|bytes| bytes.checked_add(problem_id.len() as u128))
            .and_then(|bytes| bytes.checked_add(score_profile_id.len() as u128))
            .ok_or_else(|| rejected("pc_score_external_retained_projection_unavailable"))?;
        if external_retained_base_bytes > PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(rejected("pc_score_external_retained_envelope_exceeded"));
        }

        Ok(Self {
            product,
            query,
            problem_id,
            score_profile_id,
            piece_source_id: source.id().get(),
            pattern_universe_id: universe.pattern_universe_id().get(),
            pattern_weight_model_id: universe.pattern_weight_model_id().get(),
            materialized_pattern_count,
            total_pattern_count,
            problem,
            terminal_resource_authority,
            external_retained_base_bytes,
        })
    }

    pub(crate) fn problem_arc(&self) -> Arc<SearchProblem> {
        Arc::clone(&self.problem)
    }

    #[cfg(test)]
    pub(crate) fn query_snapshot_for_test(&self) -> &PcScoreQuerySnapshot {
        self.query.as_ref()
    }

    pub(crate) fn terminal_resource_authority(&self) -> &WasmCpuTerminalResourceAuthority {
        &self.terminal_resource_authority
    }

    pub(crate) const fn external_retained_base_bytes(&self) -> u128 {
        self.external_retained_base_bytes
    }

    /// Proves that the authority base and every caller-supplied concurrent
    /// owner fit inside the closed product envelope. A successful proof returns
    /// the full conservative reservation passed to Core; unused envelope bytes
    /// are never exposed as reusable allocation credit.
    pub(crate) fn checked_external_retained_upper_bound_bytes(
        &self,
        concurrent_additional_bytes: u128,
    ) -> Result<u128, PcScoreExecutionError> {
        let retained_bytes = self
            .external_retained_base_bytes
            .checked_add(concurrent_additional_bytes)
            .ok_or_else(|| rejected("pc_score_external_retained_projection_unavailable"))?;
        if retained_bytes > PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(rejected("pc_score_external_retained_envelope_exceeded"));
        }
        Ok(PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES)
    }

    pub(crate) fn validate_raw_wasm_execution(
        &self,
        executed_problem: &Arc<SearchProblem>,
        result: &CoreExecutionResult,
    ) -> Result<(), PcScoreExecutionError> {
        self.validate_wasm_execution_problem_and_batch(executed_problem, result)?;
        if result.postprocess_score_profile_id().is_some()
            || !result.postprocess_score_cells().is_empty()
            || result.postprocess_score_cells_complete()
            || result.pc_score_distributed_merge_evidence().is_some()
        {
            return Err(rejected("pc_score_distributed_cells_not_authoritative"));
        }
        Ok(())
    }

    pub(crate) fn validate_distributed_wasm_execution(
        &self,
        executed_problem: &Arc<SearchProblem>,
        result: &CoreExecutionResult,
    ) -> Result<(), PcScoreExecutionError> {
        self.validate_wasm_execution_problem_and_batch(executed_problem, result)?;
        if result.pc_score_distributed_merge_evidence()
            != Some(PcScoreDistributedMergeEvidence::WasmVerifiedMerger)
            || !result.postprocess_score_cells_complete()
            || result.postprocess_score_profile_id() != Some(self.score_profile_id.as_ref())
        {
            return Err(rejected("pc_score_distributed_merge_evidence_mismatch"));
        }
        let identities = result.normalized_solution_identities();
        let cells = result.postprocess_score_cells();
        if !cells.windows(2).all(|pair| pair[0] < pair[1])
            || cells.iter().any(|cell| {
                cell.pattern_id() >= self.materialized_pattern_count
                    || identities
                        .binary_search(&cell.candidate_identity())
                        .is_err()
                    || cell.trace_identity().is_empty()
                    || cell.trace_identity().chars().any(char::is_control)
            })
        {
            return Err(rejected("pc_score_distributed_cell_family_mismatch"));
        }
        Ok(())
    }

    fn validate_wasm_execution_problem_and_batch(
        &self,
        executed_problem: &Arc<SearchProblem>,
        result: &CoreExecutionResult,
    ) -> Result<(), PcScoreExecutionError> {
        if !Arc::ptr_eq(&self.problem, executed_problem) {
            return Err(rejected("pc_score_executed_problem_owner_mismatch"));
        }
        let problem_evidence = result
            .pc_score_problem_evidence()
            .ok_or_else(|| rejected("pc_score_executed_problem_evidence_missing"))?;
        if !problem_evidence.matches_search_problem(self.problem.as_ref()) {
            return Err(rejected("pc_score_executed_problem_evidence_mismatch"));
        }
        let [batch] = result.exact_scoring_execution_batches() else {
            return Err(rejected("pc_score_exact_wasm_batch_missing_or_ambiguous"));
        };
        if !batch.complete() || !result.postprocess_execution_complete() {
            return Err(rejected("pc_score_exact_wasm_batch_incomplete"));
        }

        let board = self.problem.initial_board();
        let supply = self.problem.supply();
        if batch.layout().width() != board.width()
            || batch.layout().height() != board.visible_height()
            || batch.initial_occupied() != board.occupied_mask()
            || batch.initial_cursor() != self.problem.initial_hold().cursor()
            || batch.initial_hold() != self.problem.initial_hold().hold_piece()
            || batch.hold_enabled() != supply.hold_enabled()
            || batch.projects_unplaced_lookahead() != supply.projects_unplaced_lookahead()
            || batch.projects_standard_bag_lookahead() != supply.projects_standard_bag_lookahead()
            || batch.kick_table_id() != problem_evidence.kick_table_id()
            || batch.rule_profile_id() != problem_evidence.rule_profile_id()
        {
            return Err(rejected("pc_score_exact_wasm_batch_problem_mismatch"));
        }

        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or_else(|| rejected("pc_score_pattern_universe_missing"))?;
        if batch.patterns().len() != self.materialized_pattern_count
            || batch
                .patterns()
                .iter()
                .enumerate()
                .any(|(index, pattern)| pattern.as_slice() != universe.sequence_at(index).as_ref())
            || result.postprocess_pattern_weights().len() != self.materialized_pattern_count
            || result
                .postprocess_pattern_weights()
                .iter()
                .enumerate()
                .any(|(index, weight)| {
                    weight.parse::<f64>().ok().map(f64::to_bits)
                        != Some(universe.weight_at(index).get().to_bits())
                })
        {
            return Err(rejected("pc_score_exact_wasm_pattern_mismatch"));
        }

        let identities = result.normalized_solution_identities();
        if batch.graphs().len() != identities.len()
            || !identities.windows(2).all(|pair| pair[0] < pair[1])
            || batch.graphs().iter().enumerate().any(|(index, graph)| {
                graph.candidate_id() != (index + 1) as u64 || graph.identity() != identities[index]
            })
        {
            return Err(rejected("pc_score_exact_wasm_solution_identity_mismatch"));
        }
        Ok(())
    }

    pub(crate) fn validate_postprocessed_result(
        &self,
        executed_problem: &Arc<SearchProblem>,
        result: &CoreExecutionResult,
        derivation: &PcScoreDerivation,
    ) -> Result<ValidatedPcScoreExecutionEvidence, PcScoreExecutionError> {
        let execution_source_matches = match derivation.source() {
            PcScoreExecutionSource::WasmExactBatch => {
                result.pc_score_distributed_merge_evidence().is_none()
                    && result.unique_field("score_execution_distribution") == Some("coordinator")
                    && result.unique_field("score_distributed_cell_count") == Some("0")
            }
            PcScoreExecutionSource::DistributedPrecomputedCells => {
                result.pc_score_distributed_merge_evidence()
                    == Some(PcScoreDistributedMergeEvidence::WasmVerifiedMerger)
                    && result.unique_field("score_execution_distribution")
                        == Some("worker-partitions")
                    && result.usize_field("score_distributed_cell_count")
                        == Some(result.postprocess_score_cells().len())
            }
            PcScoreExecutionSource::NativeLegacyReplay => false,
        };
        if !Arc::ptr_eq(&self.problem, executed_problem)
            || !result
                .pc_score_problem_evidence()
                .is_some_and(|evidence| evidence.matches_search_problem(self.problem.as_ref()))
            || !execution_source_matches
            || !derivation.execution_source_complete()
        {
            return Err(rejected("pc_score_execution_evidence_source_mismatch"));
        }

        let score_policy = self.problem.objective().score();
        require_result_field(result, "score_profile", self.score_profile_id.as_ref())?;
        require_result_field(result, "score_accuracy_level", PC_SCORE_ACCURACY_LEVEL)?;
        require_result_field(result, "score_accuracy_reason", PC_SCORE_ACCURACY_REASON)?;
        require_result_field(result, "score_profile_specific_exact", "false")?;
        require_result_field(result, "score_evaluation_basis", "all-traces")?;
        require_result_field(result, "score_evaluation_scope", "full")?;
        require_result_field(
            result,
            "score_field_average_basis",
            PC_SCORE_OVERALL_SCORE_BASIS,
        )?;
        for key in [
            "score_evaluation_complete",
            "score_matrix_materialized",
            "score_matrix_complete",
            "score_summary_complete",
        ] {
            require_result_field(result, key, "true")?;
        }
        require_result_field(result, "score_summary_incomplete_reason", "none")?;

        require_result_field(result, "search_output_policy", "trace")?;
        require_result_field(result, "postprocess_scoring_requested", "true")?;
        require_result_field(result, "score_objective_mode", "summary")?;
        require_result_field(
            result,
            "score_profile_requested",
            score_policy.profile().as_str(),
        )?;
        require_result_field(
            result,
            "spin_profile_requested",
            score_policy.spin_profile().as_str(),
        )?;
        require_parsed_result_field(result, "score_initial_b2b", score_policy.initial_b2b())?;
        require_result_field(result, "objective_search_complete", "true")?;
        require_result_field(result, "objective_complete", "true")?;
        require_result_field(result, "objective_incomplete_reason", "none")?;
        require_result_field(result, "count_complete", "true")?;
        require_result_field(result, "count_truncated_reason", "none")?;
        require_result_field(result, "probability_complete", "true")?;
        require_result_field(result, "resource_probability_complete", "true")?;
        require_result_field(result, "resource_truncated", "false")?;
        require_result_field(result, "resource_truncation_reason", "none")?;
        require_parsed_result_field(
            result,
            "coverage_pattern_count",
            self.materialized_pattern_count,
        )?;
        require_parsed_result_field(
            result,
            "materialized_pattern_count",
            self.materialized_pattern_count,
        )?;
        require_parsed_result_field(
            result,
            "total_possible_pattern_count",
            self.total_pattern_count,
        )?;

        let matrix_pattern_count = strict_usize_result(result, "score_matrix_pattern_count")?;
        let matrix_cell_count = strict_usize_result(result, "score_matrix_cell_count")?;
        let pattern_optimal_count = strict_usize_result(result, "score_pattern_optimal_count")?;
        let failed_pc_pattern_count = strict_usize_result(result, "score_failed_pc_pattern_count")?;
        if matrix_pattern_count != self.materialized_pattern_count
            || pattern_optimal_count.checked_add(failed_pc_pattern_count)
                != Some(self.materialized_pattern_count)
        {
            return Err(rejected("pc_score_summary_pattern_count_mismatch"));
        }

        let all_universe_patterns_covered =
            strict_bool_result(result, "score_all_universe_patterns_covered")?;
        if all_universe_patterns_covered
            != (pattern_optimal_count == self.materialized_pattern_count)
        {
            return Err(rejected("pc_score_summary_coverage_flag_mismatch"));
        }
        let covered_probability = strict_f64_result(result, "score_covered_probability")?;
        let overall_score = strict_f64_result(result, "score_field_average_score")?;
        let unconditional_expected_score =
            strict_f64_result(result, "score_unconditional_expected_score")?;
        let unconditional_expected_attack =
            strict_f64_result(result, "score_unconditional_expected_attack")?;
        if !(0.0..=1.0).contains(&covered_probability)
            || unconditional_expected_score < 0.0
            || unconditional_expected_attack < 0.0
            || overall_score.to_bits() != unconditional_expected_score.to_bits()
        {
            return Err(rejected("pc_score_summary_numeric_domain_mismatch"));
        }
        let conditional_average =
            optional_f64_result(result, "score_covered_pattern_conditional_average_score")?;
        if conditional_average.is_some() != (covered_probability > 0.0) {
            return Err(rejected("pc_score_summary_conditional_average_mismatch"));
        }

        let best_score = optional_u64_result(result, "score_best_score")?;
        let best_attack = optional_u32_result(result, "score_best_attack")?;
        if best_score.is_some() != best_attack.is_some()
            || best_score.is_some() != (matrix_cell_count > 0)
        {
            return Err(rejected("pc_score_summary_best_cell_mismatch"));
        }
        let pattern_winners = Arc::clone(derivation.pattern_winner_owner());
        if !pattern_winner_family_is_valid(
            pattern_winners.as_slice(),
            result.normalized_solution_identities(),
            self.materialized_pattern_count,
            pattern_optimal_count,
            best_score,
        ) {
            return Err(rejected("pc_score_pattern_winner_family_mismatch"));
        }
        let solution_field_averages = Arc::clone(derivation.solution_field_average_owner());
        if !solution_field_average_family_is_valid(
            solution_field_averages.as_slice(),
            result.normalized_solution_identities(),
            self.materialized_pattern_count,
        ) {
            return Err(rejected("pc_score_solution_field_average_family_mismatch"));
        }

        let report = PcScoreSummaryV2Result::new(
            self.product.score_execution_origin(),
            Arc::clone(&self.query),
            Arc::clone(&self.problem_id),
            self.piece_source_id,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
            score_policy.profile(),
            score_policy.spin_profile(),
            score_policy.initial_b2b(),
            Arc::clone(&self.score_profile_id),
            self.materialized_pattern_count,
            self.total_pattern_count,
            matrix_cell_count,
            all_universe_patterns_covered,
            pattern_optimal_count,
            failed_pc_pattern_count,
            best_score,
            best_attack,
            pattern_winners,
            solution_field_averages,
            overall_score,
            covered_probability,
            unconditional_expected_score,
            unconditional_expected_attack,
            conditional_average,
            PcScoreCompletenessEvidence::new(true, true, true, true, true, true, true, true),
        )?;
        if !score_result_matches_report(result, &report) {
            return Err(rejected("pc_score_critical_result_field_mismatch"));
        }
        Ok(ValidatedPcScoreExecutionEvidence { report })
    }
}

fn pattern_winner_family_is_valid(
    winners: &[PcScorePatternWinnerV1],
    solution_identities: &[clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity],
    materialized_pattern_count: usize,
    pattern_optimal_count: usize,
    best_score: Option<u64>,
) -> bool {
    if winners.is_empty() {
        return pattern_optimal_count == 0 && best_score.is_none();
    }
    if pattern_optimal_count == 0 || best_score.is_none() {
        return false;
    }
    let mut distinct_pattern_count = 0_usize;
    let mut previous_pattern = None;
    let mut active_score = None;
    let mut previous_identity = None;
    let mut observed_best_score = None;

    for winner in winners {
        let identity = (winner.pattern_id(), winner.candidate_id());
        if winner.pattern_id() >= materialized_pattern_count
            || winner.candidate_id() == 0
            || previous_identity.is_some_and(|previous| previous >= identity)
        {
            return false;
        }
        let Some(candidate_index) = winner
            .candidate_id()
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
        else {
            return false;
        };
        if solution_identities.get(candidate_index).copied() != Some(winner.solution_identity()) {
            return false;
        }
        if previous_pattern != Some(winner.pattern_id()) {
            distinct_pattern_count = match distinct_pattern_count.checked_add(1) {
                Some(count) => count,
                None => return false,
            };
            previous_pattern = Some(winner.pattern_id());
            active_score = Some(winner.score());
        } else if active_score != Some(winner.score()) {
            return false;
        }
        observed_best_score = Some(
            observed_best_score.map_or(winner.score(), |score: u64| score.max(winner.score())),
        );
        previous_identity = Some(identity);
    }
    distinct_pattern_count == pattern_optimal_count && observed_best_score == best_score
}

fn solution_field_average_family_is_valid(
    fields: &[PcScoreSolutionFieldAverageV1],
    solution_identities: &[clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity],
    materialized_pattern_count: usize,
) -> bool {
    fields.len() == solution_identities.len()
        && fields
            .iter()
            .zip(solution_identities)
            .all(|(field, identity)| {
                field.field_identity() == *identity
                    && field.pattern_count() == materialized_pattern_count
                    && field.covered_pattern_count() <= materialized_pattern_count
                    && field.score_complete()
                    && field.average_score().is_finite()
                    && field.average_score() >= 0.0
            })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcScoreCompiledAuthorityError {
    ResourceAdmission(ResourceReport),
    ProblemCompile(ProblemCompileError),
    Contract(PcScoreExecutionError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcScoreExecutionEvidence {
    report: PcScoreSummaryV2Result,
}

impl ValidatedPcScoreExecutionEvidence {
    pub(crate) fn report(&self) -> &PcScoreSummaryV2Result {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        score_result_matches_report(result, &self.report)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcScoreExecutionError {
    component: &'static str,
}

impl PcScoreExecutionError {
    pub(crate) const fn component(self) -> &'static str {
        self.component
    }
}

impl fmt::Display for PcScoreExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.component)
    }
}

impl std::error::Error for PcScoreExecutionError {}

const fn rejected(component: &'static str) -> PcScoreExecutionError {
    PcScoreExecutionError { component }
}

fn score_result_matches_report(
    result: &CoreExecutionResult,
    report: &PcScoreSummaryV2Result,
) -> bool {
    result.unique_field("search_output_policy") == Some("trace")
        && parsed_result_field_eq(
            result,
            "coverage_pattern_count",
            report.materialized_pattern_count(),
        )
        && parsed_result_field_eq(
            result,
            "materialized_pattern_count",
            report.materialized_pattern_count(),
        )
        && parsed_result_field_eq(
            result,
            "total_possible_pattern_count",
            report.total_pattern_count(),
        )
        && result
            .unique_field("materialized_probability_mass")
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value.to_bits() == 1.0_f64.to_bits())
        && result.unique_field("probability_complete") == Some("true")
        && result.unique_field("resource_probability_complete") == Some("true")
        && result.unique_field("count_complete") == Some("true")
        && result.unique_field("count_truncated_reason") == Some("none")
        && result.unique_field("resource_truncated") == Some("false")
        && result.unique_field("resource_truncation_reason") == Some("none")
        && result.unique_field("objective_search_complete") == Some("true")
        && result.unique_field("objective_complete") == Some("true")
        && result.unique_field("objective_incomplete_reason") == Some("none")
        && result.unique_field("postprocess_scoring_requested") == Some("true")
        && result.unique_field("score_objective_mode") == Some("summary")
        && result.unique_field("score_profile_requested")
            == Some(report.score_profile_selection().as_str())
        && result.unique_field("spin_profile_requested")
            == Some(report.spin_profile_selection().as_str())
        && parsed_result_field_eq(result, "score_initial_b2b", report.initial_b2b())
        && result.unique_field("score_profile") == Some(report.score_profile_id())
        && result.unique_field("score_accuracy_level") == Some(report.accuracy_level())
        && result.unique_field("score_accuracy_reason") == Some(report.accuracy_reason())
        && result.unique_field("score_profile_specific_exact") == Some("false")
        && result.unique_field("score_evaluation_complete") == Some("true")
        && result.unique_field("score_evaluation_basis") == Some(report.score_evaluation_basis())
        && result.unique_field("score_evaluation_scope") == Some(report.score_evaluation_scope())
        && result.unique_field("score_matrix_materialized") == Some("true")
        && result.unique_field("score_matrix_complete") == Some("true")
        && parsed_result_field_eq(
            result,
            "score_matrix_cell_count",
            report.matrix_cell_count(),
        )
        && parsed_result_field_eq(
            result,
            "score_matrix_pattern_count",
            report.materialized_pattern_count(),
        )
        && result.unique_field("score_matrix_profile_id") == Some(report.score_profile_id())
        && result.unique_field("score_matrix_accuracy_level") == Some(report.accuracy_level())
        && result.unique_field("score_matrix_incomplete_reason") == Some("none")
        && result.unique_field("score_best_complete") == Some("true")
        && result.unique_field("score_summary_complete") == Some("true")
        && result.unique_field("score_summary_incomplete_reason") == Some("none")
        && parsed_result_field_eq(
            result,
            "score_all_universe_patterns_covered",
            report.all_universe_patterns_covered(),
        )
        && parsed_result_field_eq(
            result,
            "score_pattern_optimal_count",
            report.pattern_optimal_count(),
        )
        && parsed_result_field_eq(
            result,
            "score_failed_pc_pattern_count",
            report.failed_pc_pattern_count(),
        )
        && parsed_result_field_eq(result, "score_failed_pc_pattern_score", 0_u64)
        && result.unique_field("score_field_average_basis") == Some(report.overall_score_basis())
        && result.unique_field("score_field_average_score") == Some(report.overall_score())
        && result.unique_field("score_covered_probability") == Some(report.covered_probability())
        && result.unique_field("score_unconditional_expected_score")
            == Some(report.unconditional_expected_score())
        && result.unique_field("score_unconditional_expected_attack")
            == Some(report.unconditional_expected_attack())
        && optional_parsed_result_field_eq(result, "score_best_score", report.best_score())
        && optional_parsed_result_field_eq(result, "score_best_attack", report.best_attack())
        && optional_result_field_eq(
            result,
            "score_covered_pattern_conditional_average_score",
            report.covered_pattern_conditional_average_score(),
        )
}

fn parsed_result_field_eq<T>(result: &CoreExecutionResult, key: &str, expected: T) -> bool
where
    T: std::str::FromStr + PartialEq,
{
    result
        .unique_field(key)
        .and_then(|value| value.parse::<T>().ok())
        .is_some_and(|value| value == expected)
}

fn optional_parsed_result_field_eq<T>(
    result: &CoreExecutionResult,
    key: &str,
    expected: Option<T>,
) -> bool
where
    T: std::str::FromStr + PartialEq,
{
    match expected {
        Some(expected) => {
            result.field_occurrence_count(key) == 1 && parsed_result_field_eq(result, key, expected)
        }
        None => result.field_occurrence_count(key) == 0,
    }
}

fn optional_result_field_eq(
    result: &CoreExecutionResult,
    key: &str,
    expected: Option<&str>,
) -> bool {
    match expected {
        Some(expected) => {
            result.field_occurrence_count(key) == 1 && result.unique_field(key) == Some(expected)
        }
        None => result.field_occurrence_count(key) == 0,
    }
}

fn require_result_field(
    result: &CoreExecutionResult,
    key: &str,
    expected: &str,
) -> Result<(), PcScoreExecutionError> {
    if result.unique_field(key) != Some(expected) {
        return Err(rejected("pc_score_result_field_mismatch"));
    }
    Ok(())
}

fn require_parsed_result_field<T>(
    result: &CoreExecutionResult,
    key: &str,
    expected: T,
) -> Result<(), PcScoreExecutionError>
where
    T: std::str::FromStr + PartialEq,
{
    if !parsed_result_field_eq(result, key, expected) {
        return Err(rejected("pc_score_result_field_mismatch"));
    }
    Ok(())
}

fn strict_usize_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<usize, PcScoreExecutionError> {
    result
        .unique_field(key)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| rejected("pc_score_derivation_usize_invalid"))
}

fn strict_bool_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<bool, PcScoreExecutionError> {
    result
        .unique_field(key)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| rejected("pc_score_derivation_bool_invalid"))
}

fn strict_f64_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<f64, PcScoreExecutionError> {
    let value = result
        .unique_field(key)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .ok_or_else(|| rejected("pc_score_derivation_float_invalid"))?;
    Ok(value)
}

fn optional_f64_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<Option<f64>, PcScoreExecutionError> {
    match result.field_occurrence_count(key) {
        0 => Ok(None),
        1 => result
            .unique_field(key)
            .map(|value| {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| rejected("pc_score_derivation_optional_float_invalid"))
            })
            .transpose(),
        _ => Err(rejected("pc_score_optional_result_field_ambiguous")),
    }
}

fn optional_u64_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<Option<u64>, PcScoreExecutionError> {
    match result.field_occurrence_count(key) {
        0 => Ok(None),
        1 => result
            .unique_field(key)
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| rejected("pc_score_derivation_optional_u64_invalid"))
            })
            .transpose(),
        _ => Err(rejected("pc_score_optional_result_field_ambiguous")),
    }
}

fn optional_u32_result(
    result: &CoreExecutionResult,
    key: &str,
) -> Result<Option<u32>, PcScoreExecutionError> {
    match result.field_occurrence_count(key) {
        0 => Ok(None),
        1 => result
            .unique_field(key)
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| rejected("pc_score_derivation_optional_u32_invalid"))
            })
            .transpose(),
        _ => Err(rejected("pc_score_optional_result_field_ambiguous")),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::InlineScoreNumberText;

    struct TooLong;

    impl fmt::Display for TooLong {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            for _ in 0..=super::SCORE_NUMBER_TEXT_CAPACITY {
                formatter.write_str("9")?;
            }
            Ok(())
        }
    }

    #[test]
    fn score_number_text_is_inline_and_fails_closed_on_overflow() {
        let largest = InlineScoreNumberText::try_from_display(f64::MAX)
            .expect("finite f64 display fits the inline score contract");
        assert_eq!(largest.as_str(), f64::MAX.to_string());

        let error = InlineScoreNumberText::try_from_display(TooLong)
            .expect_err("oversized score text must fail closed");
        assert_eq!(error.component(), "pc_score_summary_number_text_overflow");
    }
}
