use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_core_executor::{
    canonical_probability_v2, CoreExecutionResult, CorePostProcessExecution,
};
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    probability::union_probability::union_probability,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcQueueInput, PcScenarioQuery, PcSolutionProbabilityPolicy,
};
use clearra_problem::{
    ProblemCompileError, ProblemCompiler, SearchOutputPolicy, SearchProblem, SearchProblemPreset,
};
use clearra_supply::{mixed::supply_provenance::BagBoundaryEvidence, QueueObservationPolicy};

pub const PC_SAVE_GROUPS_RESULT_CONTRACT: &str = "pc-save-groups.v2";
pub const PC_BEST_SAVE_RESULT_CONTRACT: &str = "pc-best-save.v2";
pub const PC_BEST_SAVE_SCHEMA: &str = "clearra-save-v1";
const SAVE_GROUP_IDENTITY_CONTRACT: &str = "terminal-hold-plus-active-bag-remainder-multiset.v1";
const BEST_SAVE_PROBABILITY_BASIS: &str = "whole-universe-unconditional";

/// Closed identity for the command spelling that requested a save result.
/// Saves and best-save intentionally have separate canonical and compatibility
/// origins even though they share the same terminal supply evidence producer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcSaveIngressOrigin {
    CanonicalPcSaves,
    CompatibilitySaves,
    CanonicalPcBestSave,
    CompatibilityBestSave,
}

impl PcSaveIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcSaves => "canonical-pc-saves",
            Self::CompatibilitySaves => "compatibility-saves",
            Self::CanonicalPcBestSave => "canonical-pc-best-save",
            Self::CompatibilityBestSave => "compatibility-best-save",
        }
    }

    pub const fn mode(self) -> PcSaveResultMode {
        match self {
            Self::CanonicalPcSaves | Self::CompatibilitySaves => PcSaveResultMode::SaveGroups,
            Self::CanonicalPcBestSave | Self::CompatibilityBestSave => PcSaveResultMode::BestSave,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcSaveResultMode {
    SaveGroups,
    BestSave,
}

impl PcSaveResultMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SaveGroups => "save-groups",
            Self::BestSave => "best-save",
        }
    }

    pub const fn contract_id(self) -> &'static str {
        match self {
            Self::SaveGroups => PC_SAVE_GROUPS_RESULT_CONTRACT,
            Self::BestSave => PC_BEST_SAVE_RESULT_CONTRACT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcSaveProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcSaveProblemPreset {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcSaveQuerySnapshot {
    Opening(Arc<OpeningPcSearchQuery>),
    Scenario(Arc<PcScenarioQuery>),
}

impl PcSaveQuerySnapshot {
    pub const fn problem_preset(&self) -> PcSaveProblemPreset {
        match self {
            Self::Opening(_) => PcSaveProblemPreset::OpeningPc,
            Self::Scenario(_) => PcSaveProblemPreset::ScenarioPc,
        }
    }
}

/// Canonical tetromino multiset used as the public save-group identity.
/// Counts are stored in the Core `I,O,T,S,Z,J,L` order; the textual identity
/// deliberately uses the `clearra-save-v1` ranking order `T,I,O,J,L,S,Z`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PcSavePieceMultiset {
    counts: [u8; 7],
    total_count: u8,
    canonical_id: String,
}

impl PcSavePieceMultiset {
    fn from_counts(counts: [u8; 7]) -> Result<Self, PcSaveExecutionError> {
        let total_count = counts.iter().try_fold(0_u8, |total, count| {
            total
                .checked_add(*count)
                .ok_or_else(|| rejected("pc_save_multiset_count_overflow"))
        })?;
        let canonical_id = format!(
            "T{}I{}O{}J{}L{}S{}Z{}",
            counts[piece_index(PieceKind::T)],
            counts[piece_index(PieceKind::I)],
            counts[piece_index(PieceKind::O)],
            counts[piece_index(PieceKind::J)],
            counts[piece_index(PieceKind::L)],
            counts[piece_index(PieceKind::S)],
            counts[piece_index(PieceKind::Z)],
        );
        Ok(Self {
            counts,
            total_count,
            canonical_id,
        })
    }

    pub const fn counts(&self) -> [u8; 7] {
        self.counts
    }

    pub const fn total_count(&self) -> u8 {
        self.total_count
    }

    pub const fn count(&self, piece: PieceKind) -> u8 {
        self.counts[piece_index(piece)]
    }

    pub fn canonical_id(&self) -> &str {
        &self.canonical_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcSaveExactProbability {
    bits: u64,
    decimal: String,
}

impl PcSaveExactProbability {
    fn from_value(value: ProbabilityValue) -> Self {
        Self {
            bits: value.get().to_bits(),
            decimal: canonical_probability_v2(value),
        }
    }

    pub const fn bits(&self) -> u64 {
        self.bits
    }

    pub fn decimal(&self) -> &str {
        &self.decimal
    }

    pub fn value(&self) -> f64 {
        f64::from_bits(self.bits)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcSaveWitness {
    pattern_index: usize,
    candidate_id: u64,
    trace_identity: String,
    source_cursor: usize,
    terminal_hold: Option<PieceKind>,
    active_bag_remainder: PcSavePieceMultiset,
}

impl PcSaveWitness {
    pub const fn pattern_index(&self) -> usize {
        self.pattern_index
    }

    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub const fn source_cursor(&self) -> usize {
        self.source_cursor
    }

    pub const fn terminal_hold(&self) -> Option<PieceKind> {
        self.terminal_hold
    }

    pub fn active_bag_remainder(&self) -> &PcSavePieceMultiset {
        &self.active_bag_remainder
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcSaveGroupV2 {
    identity_contract: &'static str,
    identity: PcSavePieceMultiset,
    successful_pattern_count: usize,
    unconditional_probability: PcSaveExactProbability,
    conditional_probability_given_pc: PcSaveExactProbability,
    canonical_candidate_id: u64,
    witnesses: Vec<PcSaveWitness>,
}

impl PcSaveGroupV2 {
    pub const fn identity_contract(&self) -> &'static str {
        self.identity_contract
    }

    pub fn identity(&self) -> &PcSavePieceMultiset {
        &self.identity
    }

    pub const fn successful_pattern_count(&self) -> usize {
        self.successful_pattern_count
    }

    pub fn unconditional_probability(&self) -> &PcSaveExactProbability {
        &self.unconditional_probability
    }

    pub fn conditional_probability_given_pc(&self) -> &PcSaveExactProbability {
        &self.conditional_probability_given_pc
    }

    /// Smallest producer candidate id represented by this group. Best-save
    /// winners are sorted by this value so Discord can deterministically emit
    /// the first normal-list witness without inventing a portfolio tie.
    pub const fn canonical_candidate_id(&self) -> u64 {
        self.canonical_candidate_id
    }

    pub fn witnesses(&self) -> &[PcSaveWitness] {
        &self.witnesses
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcSaveCompletenessEvidence {
    source_universe_complete: bool,
    fixed_bag_boundary_proven: bool,
    execution_batch_complete: bool,
    pattern_weights_complete: bool,
    count_complete: bool,
    probability_complete: bool,
}

impl PcSaveCompletenessEvidence {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn fixed_bag_boundary_proven(self) -> bool {
        self.fixed_bag_boundary_proven
    }

    pub const fn execution_batch_complete(self) -> bool {
        self.execution_batch_complete
    }

    pub const fn pattern_weights_complete(self) -> bool {
        self.pattern_weights_complete
    }

    pub const fn count_complete(self) -> bool {
        self.count_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.fixed_bag_boundary_proven
            && self.execution_batch_complete
            && self.pattern_weights_complete
            && self.count_complete
            && self.probability_complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcSaveGroupsV2Result {
    contract_id: &'static str,
    origin: PcSaveIngressOrigin,
    query: PcSaveQuerySnapshot,
    problem_preset: PcSaveProblemPreset,
    problem_id: String,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    pc_success_pattern_count: usize,
    pc_probability: PcSaveExactProbability,
    groups: Vec<PcSaveGroupV2>,
    completeness: PcSaveCompletenessEvidence,
}

impl PcSaveGroupsV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcSaveIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcSaveQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcSaveProblemPreset {
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

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub const fn pc_success_pattern_count(&self) -> usize {
        self.pc_success_pattern_count
    }

    pub fn pc_probability(&self) -> &PcSaveExactProbability {
        &self.pc_probability
    }

    pub fn groups(&self) -> &[PcSaveGroupV2] {
        &self.groups
    }

    pub const fn completeness(&self) -> PcSaveCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcBestSaveWinnerV2 {
    weighted_total: u16,
    balanced_jl_count: u8,
    group: PcSaveGroupV2,
}

impl PcBestSaveWinnerV2 {
    pub const fn weighted_total(&self) -> u16 {
        self.weighted_total
    }

    pub const fn balanced_jl_count(&self) -> u8 {
        self.balanced_jl_count
    }

    pub fn exact_group_probability(&self) -> &PcSaveExactProbability {
        self.group.unconditional_probability()
    }

    pub fn group(&self) -> &PcSaveGroupV2 {
        &self.group
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcBestSaveV2Result {
    contract_id: &'static str,
    schema_id: &'static str,
    probability_basis: &'static str,
    origin: PcSaveIngressOrigin,
    query: PcSaveQuerySnapshot,
    problem_preset: PcSaveProblemPreset,
    problem_id: String,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    pc_success_pattern_count: usize,
    pc_probability: PcSaveExactProbability,
    winners: Vec<PcBestSaveWinnerV2>,
    completeness: PcSaveCompletenessEvidence,
}

impl PcBestSaveV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn schema_id(&self) -> &'static str {
        self.schema_id
    }

    pub const fn probability_basis(&self) -> &'static str {
        self.probability_basis
    }

    pub const fn origin(&self) -> PcSaveIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcSaveQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcSaveProblemPreset {
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

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub const fn pc_success_pattern_count(&self) -> usize {
        self.pc_success_pattern_count
    }

    pub fn pc_probability(&self) -> &PcSaveExactProbability {
        &self.pc_probability
    }

    /// Every exact lexicographic winner is a member of this ordinary list.
    /// No portfolio identifier or tie metadata is part of this contract.
    pub fn winners(&self) -> &[PcBestSaveWinnerV2] {
        &self.winners
    }

    pub const fn completeness(&self) -> PcSaveCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PcSaveExecutionReport {
    SaveGroups(PcSaveGroupsV2Result),
    BestSave(PcBestSaveV2Result),
}

impl PcSaveExecutionReport {
    pub(crate) const fn mode(&self) -> PcSaveResultMode {
        match self {
            Self::SaveGroups(_) => PcSaveResultMode::SaveGroups,
            Self::BestSave(_) => PcSaveResultMode::BestSave,
        }
    }

    pub(crate) fn save_groups(&self) -> Option<&PcSaveGroupsV2Result> {
        match self {
            Self::SaveGroups(report) => Some(report),
            Self::BestSave(_) => None,
        }
    }

    pub(crate) fn best_save(&self) -> Option<&PcBestSaveV2Result> {
        match self {
            Self::BestSave(report) => Some(report),
            Self::SaveGroups(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PcSaveCompiledAuthority {
    origin: PcSaveIngressOrigin,
    query: PcSaveQuerySnapshot,
    problem: Arc<SearchProblem>,
}

impl PcSaveCompiledAuthority {
    pub(crate) fn compile_opening(
        query: Arc<OpeningPcSearchQuery>,
        origin: PcSaveIngressOrigin,
    ) -> Result<Self, PcSaveCompiledAuthorityError> {
        let problem = ProblemCompiler::compile_opening_pc_save(query.as_ref())
            .map(Arc::new)
            .map_err(PcSaveCompiledAuthorityError::ProblemCompile)?;
        Self::new(PcSaveQuerySnapshot::Opening(query), origin, problem)
            .map_err(PcSaveCompiledAuthorityError::Contract)
    }

    pub(crate) fn compile_scenario(
        query: Arc<PcScenarioQuery>,
        origin: PcSaveIngressOrigin,
    ) -> Result<Self, PcSaveCompiledAuthorityError> {
        let problem = ProblemCompiler::compile_scenario_pc_save(query.as_ref())
            .map(Arc::new)
            .map_err(PcSaveCompiledAuthorityError::ProblemCompile)?;
        Self::new(PcSaveQuerySnapshot::Scenario(query), origin, problem)
            .map_err(PcSaveCompiledAuthorityError::Contract)
    }

    fn new(
        query: PcSaveQuerySnapshot,
        origin: PcSaveIngressOrigin,
        problem: Arc<SearchProblem>,
    ) -> Result<Self, PcSaveExecutionError> {
        validate_compiled_save_problem(&query, origin, problem.as_ref())?;
        Ok(Self {
            origin,
            query,
            problem,
        })
    }

    pub(crate) fn problem(&self) -> &SearchProblem {
        self.problem.as_ref()
    }

    pub(crate) fn problem_arc(&self) -> Arc<SearchProblem> {
        Arc::clone(&self.problem)
    }

    pub(crate) fn validate_execution_result(
        &self,
        executed_problem: &Arc<SearchProblem>,
        result: &CoreExecutionResult,
    ) -> Result<ValidatedPcSaveExecutionEvidence, PcSaveExecutionError> {
        if !Arc::ptr_eq(&self.problem, executed_problem) {
            return Err(rejected("pc_save_executed_problem_owner_mismatch"));
        }
        let report = project_save_report(
            self.query.clone(),
            self.origin,
            self.problem.as_ref(),
            result,
        )?;
        Ok(ValidatedPcSaveExecutionEvidence {
            report,
            fingerprint: PcSaveCoreFingerprint::from_result(result),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcSaveCompiledAuthorityError {
    ProblemCompile(ProblemCompileError),
    Contract(PcSaveExecutionError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcSaveExecutionEvidence {
    report: PcSaveExecutionReport,
    fingerprint: PcSaveCoreFingerprint,
}

impl ValidatedPcSaveExecutionEvidence {
    pub(crate) fn report(&self) -> &PcSaveExecutionReport {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        self.fingerprint == PcSaveCoreFingerprint::from_result(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PcSaveCoreFingerprint {
    execution_count: usize,
    execution_hash: u64,
    weight_count: usize,
    weight_hash: u64,
    complete: bool,
}

impl PcSaveCoreFingerprint {
    fn from_result(result: &CoreExecutionResult) -> Self {
        let mut execution_hash = FNV_OFFSET;
        for execution in result.postprocess_executions() {
            hash_u64(&mut execution_hash, execution.candidate_id());
            hash_usize(&mut execution_hash, execution.pattern_id());
            hash_bytes(&mut execution_hash, execution.trace_identity().as_bytes());
            for step in execution.replay_trace().solution_trace().steps() {
                let decision = step.piece_decision();
                hash_usize(&mut execution_hash, decision.input_cursor());
                hash_usize(&mut execution_hash, decision.output_cursor());
                hash_optional_piece(&mut execution_hash, decision.input_hold_piece());
                hash_optional_piece(&mut execution_hash, decision.output_hold_piece());
                hash_piece(&mut execution_hash, decision.active_piece());
            }
        }
        let mut weight_hash = FNV_OFFSET;
        for weight in result.postprocess_pattern_weights() {
            hash_bytes(&mut weight_hash, weight.as_bytes());
        }
        Self {
            execution_count: result.postprocess_executions().len(),
            execution_hash,
            weight_count: result.postprocess_pattern_weights().len(),
            weight_hash,
            complete: result.postprocess_execution_complete(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcSaveExecutionError {
    component: &'static str,
}

impl PcSaveExecutionError {
    pub(crate) const fn component(self) -> &'static str {
        self.component
    }
}

impl fmt::Display for PcSaveExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.component)
    }
}

impl std::error::Error for PcSaveExecutionError {}

const fn rejected(component: &'static str) -> PcSaveExecutionError {
    PcSaveExecutionError { component }
}

fn validate_compiled_save_problem(
    query: &PcSaveQuerySnapshot,
    origin: PcSaveIngressOrigin,
    problem: &SearchProblem,
) -> Result<(), PcSaveExecutionError> {
    if origin.mode().contract_id().is_empty()
        || problem.preset() != query.problem_preset().search_problem_preset()
        || problem.output_policy() != SearchOutputPolicy::Trace
        || problem.goal().as_str() != "clear-to-empty"
        || problem.count_policy() != PcCountPolicy::CountAll
        || problem.objective() != ObjectivePolicy::all()
        || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
        || !problem
            .pc_chance_evidence_policy()
            .retains_pc_save_groups_v2_evidence()
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || problem.allowed_colored_solution_identities().is_some()
    {
        return Err(rejected("pc_save_compiled_contract_mismatch"));
    }

    match problem.supply().queue() {
        PcQueueInput::Standard7Bag
        | PcQueueInput::BagAlignedPattern(_)
        | PcQueueInput::PatternExpression(_) => {}
        PcQueueInput::FixedSequence(_) => {
            return Err(rejected("pc_save_fixed_source_bag_provenance_missing"))
        }
        PcQueueInput::Observed(_) => {
            return Err(rejected("pc_save_observed_bag_boundary_not_fixed"))
        }
    }

    let source = problem.piece_source();
    let provenance = source.provenance();
    let universe = source
        .materialized_universe()
        .ok_or_else(|| rejected("pc_save_pattern_universe_missing"))?;
    if provenance.bag_boundary_evidence() != BagBoundaryEvidence::FixedBoundary
        || provenance.ambiguity_report()
        || provenance.bag_profile_id() != problem.supply().bag().id().as_str()
        || source.id().get() == 0
        || !source.complete()
        || source.truncation_reason().is_some()
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || universe.pattern_count() == 0
        || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        || universe.weights().len() != universe.pattern_count()
        || universe.materialized_probability_mass().get().to_bits() != 1.0_f64.to_bits()
        || problem.supply().bag().bag_size() == 0
    {
        return Err(rejected("pc_save_fixed_bag_provenance_incomplete"));
    }
    Ok(())
}

fn project_save_report(
    query: PcSaveQuerySnapshot,
    origin: PcSaveIngressOrigin,
    problem: &SearchProblem,
    result: &CoreExecutionResult,
) -> Result<PcSaveExecutionReport, PcSaveExecutionError> {
    validate_compiled_save_problem(&query, origin, problem)?;
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or_else(|| rejected("pc_save_pattern_universe_missing"))?;
    let pattern_count = universe.pattern_count();
    if !result.postprocess_execution_complete() {
        if result.postprocess_executions().is_empty() {
            return Err(rejected("pc_save_execution_batch_missing"));
        }
        if result.postprocess_pattern_weights().is_empty() {
            return Err(rejected("pc_save_pattern_weight_batch_missing"));
        }
        if result.bool_field("build_variant_count_exact") != Some(true) {
            return Err(rejected("pc_save_build_variant_count_inexact"));
        }
        if result.usize_field("build_variant_count") != Some(result.postprocess_executions().len())
        {
            return Err(rejected("pc_save_execution_batch_count_mismatch"));
        }
        return Err(rejected("pc_save_postprocess_execution_incomplete"));
    }
    if result.bool_field("count_complete") != Some(true) {
        return Err(rejected("pc_save_count_incomplete"));
    }
    if result.bool_field("objective_complete") != Some(true) {
        return Err(rejected("pc_save_objective_incomplete"));
    }
    if result.bool_field("probability_complete") != Some(true)
        || result.bool_field("resource_probability_complete") != Some(true)
    {
        return Err(rejected("pc_save_probability_incomplete"));
    }
    if result.usize_field("materialized_pattern_count") != Some(pattern_count)
        || result.usize_field("coverage_pattern_count") != Some(pattern_count)
    {
        return Err(rejected("pc_save_pattern_count_evidence_mismatch"));
    }
    if u64_field(result, "piece_source_id") != Some(source.id().get())
        || u64_field(result, "pattern_universe_id") != Some(universe.pattern_universe_id().get())
        || u64_field(result, "pattern_weight_model_id")
            != Some(universe.pattern_weight_model_id().get())
    {
        return Err(rejected("pc_save_source_identity_evidence_mismatch"));
    }
    if result.field("problem_preset") != Some(query.problem_preset().as_str()) {
        return Err(rejected("pc_save_problem_preset_evidence_mismatch"));
    }
    if result.postprocess_pattern_weights().len() != pattern_count
        || result
            .postprocess_pattern_weights()
            .iter()
            .enumerate()
            .any(|(index, weight)| {
                weight.parse::<f64>().ok().map(f64::to_bits)
                    != Some(universe.weight_at(index).get().to_bits())
            })
    {
        return Err(rejected("pc_save_pattern_weight_evidence_mismatch"));
    }

    let canonical_candidate_ids = result
        .postprocess_executions()
        .iter()
        .map(CorePostProcessExecution::candidate_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, producer_id)| {
            let canonical_id = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| rejected("pc_save_canonical_candidate_id_overflow"))?;
            Ok((producer_id, canonical_id))
        })
        .collect::<Result<BTreeMap<_, _>, PcSaveExecutionError>>()?;
    let mut groups = BTreeMap::<PcSavePieceMultiset, BTreeMap<usize, PcSaveWitness>>::new();
    let mut pc_patterns = BTreeSet::new();
    for execution in result.postprocess_executions() {
        if execution.pattern_id() >= pattern_count {
            return Err(rejected("pc_save_execution_pattern_out_of_range"));
        }
        let (group, mut witness) = save_group_for_execution(problem, universe, execution)?;
        witness.candidate_id = *canonical_candidate_ids
            .get(&execution.candidate_id())
            .ok_or_else(|| rejected("pc_save_canonical_candidate_id_missing"))?;
        pc_patterns.insert(execution.pattern_id());
        let pattern_witnesses = groups.entry(group).or_default();
        match pattern_witnesses.entry(execution.pattern_id()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(witness);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if witness_order_key(&witness) < witness_order_key(entry.get()) {
                    entry.insert(witness);
                }
            }
        }
    }

    let pc_probability_value = probability_for_patterns(&pc_patterns, universe.weights())?;
    let pc_probability = PcSaveExactProbability::from_value(pc_probability_value);
    let mut group_rows = Vec::with_capacity(groups.len());
    for (identity, pattern_witnesses) in groups {
        let pattern_ids = pattern_witnesses.keys().copied().collect::<BTreeSet<_>>();
        let unconditional = probability_for_patterns(&pattern_ids, universe.weights())?;
        let conditional = conditional_probability(unconditional, pc_probability_value)?;
        let witnesses = pattern_witnesses.into_values().collect::<Vec<_>>();
        let canonical_candidate_id = witnesses
            .iter()
            .map(PcSaveWitness::candidate_id)
            .min()
            .ok_or_else(|| rejected("pc_save_group_has_no_witness"))?;
        group_rows.push(PcSaveGroupV2 {
            identity_contract: SAVE_GROUP_IDENTITY_CONTRACT,
            identity,
            successful_pattern_count: witnesses.len(),
            unconditional_probability: PcSaveExactProbability::from_value(unconditional),
            conditional_probability_given_pc: PcSaveExactProbability::from_value(conditional),
            canonical_candidate_id,
            witnesses,
        });
    }
    group_rows.sort_by(|left, right| left.identity.cmp(&right.identity));

    let completeness = PcSaveCompletenessEvidence {
        source_universe_complete: true,
        fixed_bag_boundary_proven: true,
        execution_batch_complete: true,
        pattern_weights_complete: true,
        count_complete: true,
        probability_complete: true,
    };
    let problem_id = problem.problem_id().as_str().to_owned();
    let common = SaveReportCommon {
        origin,
        query,
        problem_preset: problem.preset().into(),
        problem_id,
        piece_source_id: source.id().get(),
        pattern_universe_id: universe.pattern_universe_id().get(),
        pattern_weight_model_id: universe.pattern_weight_model_id().get(),
        materialized_pattern_count: pattern_count,
        pc_success_pattern_count: pc_patterns.len(),
        pc_probability,
        completeness,
    };

    match origin.mode() {
        PcSaveResultMode::SaveGroups => {
            Ok(PcSaveExecutionReport::SaveGroups(PcSaveGroupsV2Result {
                contract_id: PC_SAVE_GROUPS_RESULT_CONTRACT,
                origin: common.origin,
                query: common.query,
                problem_preset: common.problem_preset,
                problem_id: common.problem_id,
                piece_source_id: common.piece_source_id,
                pattern_universe_id: common.pattern_universe_id,
                pattern_weight_model_id: common.pattern_weight_model_id,
                materialized_pattern_count: common.materialized_pattern_count,
                pc_success_pattern_count: common.pc_success_pattern_count,
                pc_probability: common.pc_probability,
                groups: group_rows,
                completeness: common.completeness,
            }))
        }
        PcSaveResultMode::BestSave => {
            let winners = best_save_winners(group_rows);
            Ok(PcSaveExecutionReport::BestSave(PcBestSaveV2Result {
                contract_id: PC_BEST_SAVE_RESULT_CONTRACT,
                schema_id: PC_BEST_SAVE_SCHEMA,
                probability_basis: BEST_SAVE_PROBABILITY_BASIS,
                origin: common.origin,
                query: common.query,
                problem_preset: common.problem_preset,
                problem_id: common.problem_id,
                piece_source_id: common.piece_source_id,
                pattern_universe_id: common.pattern_universe_id,
                pattern_weight_model_id: common.pattern_weight_model_id,
                materialized_pattern_count: common.materialized_pattern_count,
                pc_success_pattern_count: common.pc_success_pattern_count,
                pc_probability: common.pc_probability,
                winners,
                completeness: common.completeness,
            }))
        }
    }
}

struct SaveReportCommon {
    origin: PcSaveIngressOrigin,
    query: PcSaveQuerySnapshot,
    problem_preset: PcSaveProblemPreset,
    problem_id: String,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    pc_success_pattern_count: usize,
    pc_probability: PcSaveExactProbability,
    completeness: PcSaveCompletenessEvidence,
}

impl From<SearchProblemPreset> for PcSaveProblemPreset {
    fn from(value: SearchProblemPreset) -> Self {
        match value {
            SearchProblemPreset::OpeningPc => Self::OpeningPc,
            SearchProblemPreset::ScenarioPc => Self::ScenarioPc,
            SearchProblemPreset::Setup | SearchProblemPreset::Build => {
                unreachable!("validated pc.save problem has a PC preset")
            }
        }
    }
}

fn save_group_for_execution(
    problem: &SearchProblem,
    universe: &clearra_supply::MaterializedPatternUniverse,
    execution: &CorePostProcessExecution,
) -> Result<(PcSavePieceMultiset, PcSaveWitness), PcSaveExecutionError> {
    let mut cursor = usize::from(problem.initial_hold().cursor());
    let mut hold = problem.initial_hold().hold_piece();
    for (index, step) in execution
        .replay_trace()
        .solution_trace()
        .steps()
        .iter()
        .enumerate()
    {
        let decision = step.piece_decision();
        if decision.input_cursor() != cursor || decision.input_hold_piece() != hold {
            return Err(rejected("pc_save_replay_supply_chain_mismatch"));
        }
        if step.step_index() != index {
            return Err(rejected("pc_save_replay_step_index_mismatch"));
        }
        cursor = decision.output_cursor();
        hold = decision.output_hold_piece();
    }

    let sequence = universe
        .try_sequence_at(execution.pattern_id())
        .ok_or_else(|| rejected("pc_save_pattern_sequence_missing"))?;
    if cursor > sequence.len() {
        return Err(rejected("pc_save_terminal_cursor_out_of_range"));
    }
    let bag = problem.supply().bag();
    let bag_size = bag.bag_size();
    if bag_size == 0 {
        return Err(rejected("pc_save_bag_profile_empty"));
    }
    let mut bag_counts = [0_u8; 7];
    for entry in bag.entries() {
        bag_counts[piece_index(entry.piece())] = u8::try_from(entry.multiplicity())
            .map_err(|_| rejected("pc_save_bag_multiplicity_out_of_range"))?;
    }
    validate_consumed_bag_prefixes(sequence.as_ref(), cursor, bag_counts, bag_size)?;
    let active_start = cursor - cursor % bag_size;
    let mut remainder_counts = bag_counts;
    for piece in &sequence[active_start..cursor] {
        let count = &mut remainder_counts[piece_index(*piece)];
        *count = count
            .checked_sub(1)
            .ok_or_else(|| rejected("pc_save_active_bag_multiplicity_mismatch"))?;
    }
    let active_bag_remainder = PcSavePieceMultiset::from_counts(remainder_counts)?;
    let mut save_counts = remainder_counts;
    if let Some(piece) = hold {
        save_counts[piece_index(piece)] = save_counts[piece_index(piece)]
            .checked_add(1)
            .ok_or_else(|| rejected("pc_save_group_count_overflow"))?;
    }
    let group = PcSavePieceMultiset::from_counts(save_counts)?;
    let witness = PcSaveWitness {
        pattern_index: execution.pattern_id(),
        candidate_id: execution.candidate_id(),
        trace_identity: execution.trace_identity().to_owned(),
        source_cursor: cursor,
        terminal_hold: hold,
        active_bag_remainder,
    };
    Ok((group, witness))
}

fn validate_consumed_bag_prefixes(
    sequence: &[PieceKind],
    cursor: usize,
    bag_counts: [u8; 7],
    bag_size: usize,
) -> Result<(), PcSaveExecutionError> {
    for chunk in sequence[..cursor].chunks(bag_size) {
        let mut remaining = bag_counts;
        for piece in chunk {
            let count = &mut remaining[piece_index(*piece)];
            *count = count
                .checked_sub(1)
                .ok_or_else(|| rejected("pc_save_bag_prefix_multiplicity_mismatch"))?;
        }
        if chunk.len() == bag_size && remaining.iter().any(|count| *count != 0) {
            return Err(rejected("pc_save_complete_bag_multiplicity_mismatch"));
        }
    }
    Ok(())
}

fn u64_field(result: &CoreExecutionResult, key: &str) -> Option<u64> {
    result.field(key)?.parse().ok()
}

fn probability_for_patterns(
    patterns: &BTreeSet<usize>,
    weights: &clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet,
) -> Result<ProbabilityValue, PcSaveExecutionError> {
    let bits =
        PatternBitSet::from_patterns(weights.len(), patterns.iter().copied().map(PatternId::new))
            .map_err(|_| rejected("pc_save_pattern_probability_bitset_invalid"))?;
    union_probability(&bits, weights).map_err(|_| rejected("pc_save_pattern_probability_invalid"))
}

fn conditional_probability(
    group: ProbabilityValue,
    pc: ProbabilityValue,
) -> Result<ProbabilityValue, PcSaveExecutionError> {
    if group == ProbabilityValue::ZERO {
        return Ok(ProbabilityValue::ZERO);
    }
    if pc == ProbabilityValue::ZERO {
        return Err(rejected("pc_save_conditional_probability_denominator_zero"));
    }
    let value = if group.get().to_bits() == pc.get().to_bits() {
        1.0
    } else {
        group.get() / pc.get()
    };
    ProbabilityValue::new(value).map_err(|_| rejected("pc_save_conditional_probability_invalid"))
}

fn best_save_winners(groups: Vec<PcSaveGroupV2>) -> Vec<PcBestSaveWinnerV2> {
    let Some(best_key) = groups
        .iter()
        .map(best_save_key)
        .max_by(compare_best_save_keys)
    else {
        return Vec::new();
    };
    let mut winners = groups
        .into_iter()
        .filter(|group| exact_best_save_key_matches(best_save_key(group), best_key))
        .map(|group| {
            let (weighted_total, balanced_jl_count, _) = best_save_key(&group);
            PcBestSaveWinnerV2 {
                weighted_total,
                balanced_jl_count,
                group,
            }
        })
        .collect::<Vec<_>>();
    winners.sort_by(|left, right| {
        left.group
            .canonical_candidate_id()
            .cmp(&right.group.canonical_candidate_id())
            .then_with(|| left.group.identity().cmp(right.group.identity()))
    });
    winners
}

fn best_save_key(group: &PcSaveGroupV2) -> (u16, u8, u64) {
    let counts = group.identity();
    let weighted_total = u16::from(counts.count(PieceKind::T)) * 6
        + u16::from(counts.count(PieceKind::I)) * 4
        + u16::from(counts.count(PieceKind::O)) * 3
        + u16::from(counts.count(PieceKind::J))
        + u16::from(counts.count(PieceKind::L));
    let balanced_jl_count = counts.count(PieceKind::J).min(counts.count(PieceKind::L));
    (
        weighted_total,
        balanced_jl_count,
        group.unconditional_probability().bits(),
    )
}

fn compare_best_save_keys(left: &(u16, u8, u64), right: &(u16, u8, u64)) -> std::cmp::Ordering {
    left.0
        .cmp(&right.0)
        .then_with(|| left.1.cmp(&right.1))
        .then_with(|| f64::from_bits(left.2).total_cmp(&f64::from_bits(right.2)))
}

fn exact_best_save_key_matches(left: (u16, u8, u64), right: (u16, u8, u64)) -> bool {
    left.0 == right.0 && left.1 == right.1 && left.2 == right.2
}

fn witness_order_key(witness: &PcSaveWitness) -> (u64, &str) {
    (witness.candidate_id(), witness.trace_identity())
}

const fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    hash_usize(hash, bytes.len());
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn hash_u64(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn hash_usize(hash: &mut u64, value: usize) {
    hash_u64(hash, value as u64);
}

fn hash_piece(hash: &mut u64, piece: PieceKind) {
    hash_u64(hash, piece_index(piece) as u64 + 1);
}

fn hash_optional_piece(hash: &mut u64, piece: Option<PieceKind>) {
    hash_u64(hash, piece.map_or(0, |piece| piece_index(piece) as u64 + 1));
}
