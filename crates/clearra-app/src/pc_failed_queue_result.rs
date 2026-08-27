use std::{fmt, sync::Arc};

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_core_executor::{
    canonical_probability_v2, CoreExecutionResult, PcFailedQueueEvidence, PcFailedQueueMemoryReport,
};
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcScenarioQuery};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem, SearchProblemPreset};
use clearra_supply::MaterializedPatternUniverse;

pub(crate) const PC_FAILED_QUEUE_V2_CONTRACT: &str = "pc-failed-queue.v2";

/// Closed identity for the ingress spelling that selected typed failed-queue
/// semantics. Public legacy spellings never construct either variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcFailedQueueIngressOrigin {
    CanonicalFailedQueue,
    CompatibilityFailedQueueUnderscore,
}

impl PcFailedQueueIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalFailedQueue => "canonical-pc-failed-queue",
            Self::CompatibilityFailedQueueUnderscore => "compatibility-failed-queue-underscore",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcFailedQueueQuerySnapshot {
    Opening(OpeningPcSearchQuery),
    Scenario(PcScenarioQuery),
}

impl PcFailedQueueQuerySnapshot {
    pub const fn problem_preset(&self) -> PcFailedQueueProblemPreset {
        match self {
            Self::Opening(_) => PcFailedQueueProblemPreset::OpeningPc,
            Self::Scenario(_) => PcFailedQueueProblemPreset::ScenarioPc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcFailedQueueProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcFailedQueueProblemPreset {
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
pub struct PcFailedQueueV2Example {
    pattern_index: usize,
    pieces: Vec<PieceKind>,
}

impl PcFailedQueueV2Example {
    pub const fn pattern_index(&self) -> usize {
        self.pattern_index
    }

    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }

    pub fn sequence(&self) -> String {
        self.pieces.iter().map(|piece| piece.as_ascii()).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcFailedQueueV2MemoryEvidence {
    admission_cap_bytes: u128,
    observed_execution_bytes: u128,
    admitted_producer_peak_bytes: u128,
    retained_producer_bytes: u128,
}

impl PcFailedQueueV2MemoryEvidence {
    fn from_core(report: PcFailedQueueMemoryReport) -> Self {
        Self {
            admission_cap_bytes: report.admission_cap_bytes(),
            observed_execution_bytes: report.observed_execution_bytes(),
            admitted_producer_peak_bytes: report.admitted_producer_peak_bytes(),
            retained_producer_bytes: report.retained_producer_bytes(),
        }
    }

    pub const fn admission_cap_bytes(self) -> u128 {
        self.admission_cap_bytes
    }

    pub const fn observed_execution_bytes(self) -> u128 {
        self.observed_execution_bytes
    }

    pub const fn admitted_producer_peak_bytes(self) -> u128 {
        self.admitted_producer_peak_bytes
    }

    pub const fn retained_producer_bytes(self) -> u128 {
        self.retained_producer_bytes
    }
}

/// Value-only `pc-failed-queue.v2` result. No producer owner, raw coverage row,
/// or execution authority can escape through this report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcFailedQueueV2Result {
    contract_id: &'static str,
    origin: PcFailedQueueIngressOrigin,
    query: PcFailedQueueQuerySnapshot,
    problem_preset: PcFailedQueueProblemPreset,
    problem_id: String,
    failed_pattern_limit: usize,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    pattern_count: usize,
    source_row_count: usize,
    success_pattern_count: usize,
    failed_pattern_count: usize,
    success_probability_bits: u64,
    success_probability: String,
    failed_probability_bits: u64,
    failed_probability: String,
    materialized_probability_mass_bits: u64,
    materialized_probability_mass: String,
    examples: Vec<PcFailedQueueV2Example>,
    examples_truncated: bool,
    memory_evidence: PcFailedQueueV2MemoryEvidence,
}

impl PcFailedQueueV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcFailedQueueIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcFailedQueueQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcFailedQueueProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }

    pub const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
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

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn source_row_count(&self) -> usize {
        self.source_row_count
    }

    pub const fn success_pattern_count(&self) -> usize {
        self.success_pattern_count
    }

    pub const fn failed_pattern_count(&self) -> usize {
        self.failed_pattern_count
    }

    pub const fn success_probability_bits(&self) -> u64 {
        self.success_probability_bits
    }

    pub fn success_probability(&self) -> &str {
        &self.success_probability
    }

    pub const fn failed_probability_bits(&self) -> u64 {
        self.failed_probability_bits
    }

    pub fn failed_probability(&self) -> &str {
        &self.failed_probability
    }

    pub const fn materialized_probability_mass_bits(&self) -> u64 {
        self.materialized_probability_mass_bits
    }

    pub fn materialized_probability_mass(&self) -> &str {
        &self.materialized_probability_mass
    }

    pub fn examples(&self) -> &[PcFailedQueueV2Example] {
        &self.examples
    }

    pub const fn examples_truncated(&self) -> bool {
        self.examples_truncated
    }

    pub const fn memory_evidence(&self) -> PcFailedQueueV2MemoryEvidence {
        self.memory_evidence
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PcFailedQueueCompiledAuthority {
    origin: PcFailedQueueIngressOrigin,
    query: PcFailedQueueQuerySnapshot,
    failed_pattern_limit: usize,
    problem: Arc<SearchProblem>,
}

impl PcFailedQueueCompiledAuthority {
    pub(crate) fn opening(
        query: &OpeningPcSearchQuery,
        origin: PcFailedQueueIngressOrigin,
        failed_pattern_limit: usize,
    ) -> Result<Self, PcFailedQueueExecutionValidationError> {
        let problem = ProblemCompiler::compile_opening_percent(query)
            .map(Arc::new)
            .map_err(|_| rejected("pc failed-queue opening query did not compile"))?;
        Self::new(
            PcFailedQueueQuerySnapshot::Opening(query.clone()),
            origin,
            failed_pattern_limit,
            problem,
        )
    }

    pub(crate) fn scenario(
        query: &PcScenarioQuery,
        origin: PcFailedQueueIngressOrigin,
        failed_pattern_limit: usize,
    ) -> Result<Self, PcFailedQueueExecutionValidationError> {
        let problem = ProblemCompiler::compile_scenario_percent(query)
            .map(Arc::new)
            .map_err(|_| rejected("pc failed-queue scenario query did not compile"))?;
        Self::new(
            PcFailedQueueQuerySnapshot::Scenario(query.clone()),
            origin,
            failed_pattern_limit,
            problem,
        )
    }

    fn new(
        query: PcFailedQueueQuerySnapshot,
        origin: PcFailedQueueIngressOrigin,
        failed_pattern_limit: usize,
        problem: Arc<SearchProblem>,
    ) -> Result<Self, PcFailedQueueExecutionValidationError> {
        if problem.preset() != query.problem_preset().search_problem_preset() {
            return Err(rejected(
                "compiled problem preset does not match the failed-queue query",
            ));
        }
        if problem.output_policy() != SearchOutputPolicy::CoverageSummary {
            return Err(rejected("pc failed-queue requires coverage-summary output"));
        }
        if problem.goal().as_str() != "clear-to-empty" {
            return Err(rejected(
                "pc failed-queue requires the clear-to-empty compiled goal",
            ));
        }
        Ok(Self {
            origin,
            query,
            failed_pattern_limit,
            problem,
        })
    }

    pub(crate) fn problem_arc(&self) -> Arc<SearchProblem> {
        Arc::clone(&self.problem)
    }

    pub(crate) const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
    }

    pub(crate) fn validate_and_decorate(
        &self,
        result: CoreExecutionResult,
        evidence: PcFailedQueueEvidence,
    ) -> Result<
        (CoreExecutionResult, ValidatedPcFailedQueueExecutionEvidence),
        PcFailedQueueExecutionValidationError,
    > {
        validate_raw_evidence(self, &result, &evidence)?;
        let report = report_from_evidence(self, &evidence)?;
        let result = decorate_typed_result(result, &report);
        let critical_fields = critical_field_snapshot(&result)?;
        Ok((
            result,
            ValidatedPcFailedQueueExecutionEvidence {
                report,
                critical_fields,
                coverage_pattern_words: evidence.success_coverage().words().to_vec(),
            },
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcFailedQueueExecutionEvidence {
    report: PcFailedQueueV2Result,
    critical_fields: Vec<(String, String)>,
    coverage_pattern_words: Vec<u64>,
}

impl ValidatedPcFailedQueueExecutionEvidence {
    pub(crate) fn report(&self) -> &PcFailedQueueV2Result {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        critical_field_snapshot(result).is_ok_and(|fields| fields == self.critical_fields)
            && result.coverage_pattern_words() == self.coverage_pattern_words
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcFailedQueueExecutionValidationError {
    reason: &'static str,
}

impl PcFailedQueueExecutionValidationError {
    pub(crate) const fn reason(self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for PcFailedQueueExecutionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl std::error::Error for PcFailedQueueExecutionValidationError {}

fn rejected(reason: &'static str) -> PcFailedQueueExecutionValidationError {
    PcFailedQueueExecutionValidationError { reason }
}

fn validate_raw_evidence(
    authority: &PcFailedQueueCompiledAuthority,
    result: &CoreExecutionResult,
    evidence: &PcFailedQueueEvidence,
) -> Result<(), PcFailedQueueExecutionValidationError> {
    if !evidence.matches_problem_owner(&authority.problem) {
        return Err(rejected(
            "failed-queue evidence does not belong to the executed problem owner",
        ));
    }
    let problem = authority.problem.as_ref();
    if evidence.problem().preset() != authority.query.problem_preset().search_problem_preset()
        || evidence.problem().problem_id() != problem.problem_id()
    {
        return Err(rejected(
            "failed-queue evidence problem does not match the compiled authority",
        ));
    }
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or_else(|| rejected("compiled failed-queue problem has no materialized universe"))?;
    if !source.complete()
        || !universe.complete()
        || universe.total_possible_pattern_count() != universe.pattern_count() as u128
    {
        return Err(rejected(
            "compiled failed-queue pattern universe is incomplete",
        ));
    }
    if evidence.piece_source_id() != source.id().get()
        || evidence.pattern_universe_id() != universe.pattern_universe_id()
        || evidence.pattern_weight_model_id() != universe.pattern_weight_model_id()
        || evidence.pattern_count() != universe.pattern_count()
        || evidence.success_coverage().pattern_count() != universe.pattern_count()
    {
        return Err(rejected(
            "failed-queue evidence identity or pattern dimensions do not match",
        ));
    }
    if evidence.success_coverage().words() != result.coverage_pattern_words() {
        return Err(rejected(
            "failed-queue evidence union does not match the execution result",
        ));
    }
    if evidence
        .success_pattern_count()
        .checked_add(evidence.failed_pattern_count())
        != Some(evidence.pattern_count())
    {
        return Err(rejected(
            "failed-queue success and failure counts do not partition the universe",
        ));
    }
    let expected_examples = authority
        .failed_pattern_limit
        .min(evidence.failed_pattern_count());
    validate_failed_examples(
        universe,
        evidence.success_coverage().words(),
        evidence.pattern_count(),
        evidence
            .examples()
            .iter()
            .map(|example| (example.pattern_index(), example.pieces())),
        expected_examples,
    )?;

    require_field(result, "status", "percent-executed")?;
    require_field(result, "search_output_policy", "coverage-summary")?;
    require_field(
        result,
        "problem_preset",
        authority.query.problem_preset().as_str(),
    )?;
    require_usize(result, "pattern_count", evidence.pattern_count())?;
    require_usize(result, "coverage_pattern_count", evidence.pattern_count())?;
    require_usize(
        result,
        "materialized_pattern_count",
        evidence.pattern_count(),
    )?;
    require_usize(
        result,
        "covered_pattern_count",
        evidence.success_pattern_count(),
    )?;
    require_usize(
        result,
        "c_buildup_coverage_row_count",
        evidence.source_row_count(),
    )?;
    require_u64(result, "piece_source_id", evidence.piece_source_id())?;
    require_u64(
        result,
        "pattern_universe_id",
        evidence.pattern_universe_id().get(),
    )?;
    require_u64(
        result,
        "pattern_weight_model_id",
        evidence.pattern_weight_model_id().get(),
    )?;
    let success_probability = legacy_probability(evidence.success_probability());
    require_field(result, "coverage_probability", &success_probability)?;
    require_field(result, "probability", &success_probability)?;
    require_field(result, "weighted_probability", &success_probability)?;
    require_field(
        result,
        "materialized_probability_mass",
        &legacy_probability(evidence.materialized_probability_mass()),
    )?;
    for key in [
        "probability_complete",
        "count_complete",
        "resource_probability_complete",
    ] {
        require_field(result, key, "true")?;
    }
    for key in ["truncated", "resource_truncated"] {
        require_field(result, key, "false")?;
    }
    Ok(())
}

pub(crate) fn validate_failed_examples<'a>(
    universe: &MaterializedPatternUniverse,
    success_coverage_words: &[u64],
    pattern_count: usize,
    mut examples: impl ExactSizeIterator<Item = (usize, &'a [PieceKind])>,
    expected_example_count: usize,
) -> Result<(), PcFailedQueueExecutionValidationError> {
    if examples.len() != expected_example_count {
        return Err(rejected(
            "failed-queue example count does not match the requested limit",
        ));
    }

    let mut verified_example_count = 0_usize;
    let mut expected_pieces = Vec::new();
    for pattern_index in 0..pattern_count {
        if coverage_contains(success_coverage_words, pattern_index) {
            continue;
        }
        if verified_example_count == expected_example_count {
            break;
        }
        let Some((example_index, example_pieces)) = examples.next() else {
            return Err(rejected(
                "failed-queue example count does not match the requested limit",
            ));
        };
        if example_index != pattern_index {
            return Err(rejected(
                "failed-queue examples are not the exact first uncovered patterns",
            ));
        }
        expected_pieces.clear();
        let sequence_len = universe.sequence_len_at(pattern_index);
        expected_pieces
            .try_reserve_exact(sequence_len)
            .map_err(|_| rejected("failed-queue example validation scratch allocation failed"))?;
        if !universe.try_write_sequence_at(pattern_index, &mut expected_pieces) {
            return Err(rejected(
                "failed-queue example index is outside the compiled pattern universe",
            ));
        }
        if expected_pieces.len() != sequence_len {
            return Err(rejected(
                "failed-queue example sequence length does not match the compiled pattern universe",
            ));
        }
        if example_pieces != expected_pieces.as_slice() {
            return Err(rejected(
                "failed-queue example pieces do not match the compiled pattern universe",
            ));
        }
        verified_example_count += 1;
    }

    if verified_example_count != expected_example_count || examples.next().is_some() {
        return Err(rejected(
            "failed-queue example count does not match the requested limit",
        ));
    }
    Ok(())
}

fn coverage_contains(words: &[u64], pattern_index: usize) -> bool {
    words
        .get(pattern_index / u64::BITS as usize)
        .is_some_and(|word| word & (1_u64 << (pattern_index % u64::BITS as usize)) != 0)
}

fn report_from_evidence(
    authority: &PcFailedQueueCompiledAuthority,
    evidence: &PcFailedQueueEvidence,
) -> Result<PcFailedQueueV2Result, PcFailedQueueExecutionValidationError> {
    let examples = evidence
        .examples()
        .iter()
        .map(|example| PcFailedQueueV2Example {
            pattern_index: example.pattern_index(),
            pieces: example.pieces().to_vec(),
        })
        .collect::<Vec<_>>();
    let memory_evidence = PcFailedQueueV2MemoryEvidence::from_core(evidence.memory_report());
    let admitted_peak_bytes = memory_evidence
        .observed_execution_bytes()
        .checked_add(memory_evidence.admitted_producer_peak_bytes())
        .ok_or_else(|| rejected("failed-queue memory evidence total overflows"))?;
    if admitted_peak_bytes > memory_evidence.admission_cap_bytes()
        || memory_evidence.retained_producer_bytes()
            > memory_evidence.admitted_producer_peak_bytes()
    {
        return Err(rejected(
            "failed-queue memory evidence exceeds its admitted authority",
        ));
    }
    Ok(PcFailedQueueV2Result {
        contract_id: PC_FAILED_QUEUE_V2_CONTRACT,
        origin: authority.origin,
        query: authority.query.clone(),
        problem_preset: authority.query.problem_preset(),
        problem_id: authority.problem.problem_id().as_str().to_owned(),
        failed_pattern_limit: authority.failed_pattern_limit,
        piece_source_id: evidence.piece_source_id(),
        pattern_universe_id: evidence.pattern_universe_id().get(),
        pattern_weight_model_id: evidence.pattern_weight_model_id().get(),
        pattern_count: evidence.pattern_count(),
        source_row_count: evidence.source_row_count(),
        success_pattern_count: evidence.success_pattern_count(),
        failed_pattern_count: evidence.failed_pattern_count(),
        success_probability_bits: evidence.success_probability().get().to_bits(),
        success_probability: canonical_probability_v2(evidence.success_probability()),
        failed_probability_bits: evidence.failed_probability().get().to_bits(),
        failed_probability: canonical_probability_v2(evidence.failed_probability()),
        materialized_probability_mass_bits: evidence
            .materialized_probability_mass()
            .get()
            .to_bits(),
        materialized_probability_mass: canonical_probability_v2(
            evidence.materialized_probability_mass(),
        ),
        examples_truncated: examples.len() < evidence.failed_pattern_count(),
        examples,
        memory_evidence,
    })
}

fn decorate_typed_result(
    result: CoreExecutionResult,
    report: &PcFailedQueueV2Result,
) -> CoreExecutionResult {
    let mut fields = Vec::with_capacity(report.examples().len().saturating_add(12));
    fields.extend([
        ("objective_search_complete".to_owned(), "true".to_owned()),
        ("objective_complete".to_owned(), "true".to_owned()),
        ("result_mode".to_owned(), "failed-queue".to_owned()),
        (
            "failed_queue_contract".to_owned(),
            "exact-coverage-complement".to_owned(),
        ),
        (
            "failed_queue_probability".to_owned(),
            legacy_probability_bits(report.failed_probability_bits()),
        ),
        (
            "percent_evidence_contract".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "total_pattern_count".to_owned(),
            report.pattern_count().to_string(),
        ),
        (
            "probability".to_owned(),
            legacy_probability_bits(report.success_probability_bits()),
        ),
        (
            "failed_pattern_count".to_owned(),
            report.failed_pattern_count().to_string(),
        ),
        (
            "failed_pattern_scope".to_owned(),
            "materialized-universe".to_owned(),
        ),
        (
            "failed_pattern_count_complete".to_owned(),
            "true".to_owned(),
        ),
        (
            "failed_pattern_limit".to_owned(),
            report.failed_pattern_limit().to_string(),
        ),
    ]);
    for (index, example) in report.examples().iter().enumerate() {
        fields.push((format!("failed_pattern_{index}"), example.sequence()));
    }
    fields.extend([
        (
            "failed_pattern_examples_materialized".to_owned(),
            report.examples().len().to_string(),
        ),
        (
            "failed_pattern_examples_truncated".to_owned(),
            report.examples_truncated().to_string(),
        ),
    ]);
    result.with_replaced_fields(fields)
}

const CRITICAL_FIELDS: &[&str] = &[
    "status",
    "search_output_policy",
    "problem_preset",
    "piece_source_id",
    "pattern_universe_id",
    "pattern_weight_model_id",
    "pattern_count",
    "coverage_pattern_count",
    "verified_pattern_count",
    "materialized_pattern_count",
    "total_pattern_count",
    "covered_pattern_count",
    "weighted_pattern_count",
    "c_buildup_coverage_row_count",
    "probability",
    "weighted_probability",
    "coverage_probability",
    "materialized_probability_mass",
    "probability_complete",
    "count_complete",
    "truncated",
    "truncation_reason",
    "resource_probability_complete",
    "resource_truncated",
    "resource_truncation_reason",
    "objective_search_complete",
    "objective_complete",
    "result_mode",
    "failed_queue_contract",
    "failed_queue_probability",
    "percent_evidence_contract",
    "failed_pattern_count",
    "failed_pattern_scope",
    "failed_pattern_count_complete",
    "failed_pattern_limit",
    "failed_pattern_examples_materialized",
    "failed_pattern_examples_truncated",
];

fn critical_field_snapshot(
    result: &CoreExecutionResult,
) -> Result<Vec<(String, String)>, PcFailedQueueExecutionValidationError> {
    let fields = result.summary_fields();
    let mut snapshot = Vec::new();
    for key in CRITICAL_FIELDS {
        let mut values = fields
            .iter()
            .filter_map(|(field_key, value)| (field_key == key).then_some(value));
        if let Some(value) = values.next() {
            if values.next().is_some() {
                return Err(rejected(
                    "failed-queue result contains a duplicate authoritative field",
                ));
            }
            snapshot.push(((*key).to_owned(), value.clone()));
        }
    }
    for index in 0..result
        .usize_field("failed_pattern_examples_materialized")
        .unwrap_or(0)
    {
        let key = format!("failed_pattern_{index}");
        let mut values = fields
            .iter()
            .filter_map(|(field_key, value)| (field_key == &key).then_some(value));
        let value = values
            .next()
            .ok_or_else(|| rejected("failed-queue result example field is missing"))?;
        if values.next().is_some() {
            return Err(rejected(
                "failed-queue result contains a duplicate example field",
            ));
        }
        snapshot.push((key, value.clone()));
    }
    Ok(snapshot)
}

fn require_field(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: &str,
) -> Result<(), PcFailedQueueExecutionValidationError> {
    let fields = result.summary_fields();
    let values = fields
        .iter()
        .filter_map(|(field, value)| (field == key).then_some(value.as_str()))
        .collect::<Vec<_>>();
    if values.as_slice() != [expected] {
        return Err(rejected(
            "failed-queue result field is missing, duplicated, or mismatched",
        ));
    }
    Ok(())
}

fn require_usize(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: usize,
) -> Result<(), PcFailedQueueExecutionValidationError> {
    require_field(result, key, &expected.to_string())
}

fn require_u64(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: u64,
) -> Result<(), PcFailedQueueExecutionValidationError> {
    require_field(result, key, &expected.to_string())
}

fn legacy_probability(value: ProbabilityValue) -> String {
    legacy_probability_bits(value.get().to_bits())
}

fn legacy_probability_bits(bits: u64) -> String {
    let value = f64::from_bits(bits);
    if value == 0.0 || value == 1.0 {
        return format!("{value:.0}");
    }
    format!("{value:.12}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
