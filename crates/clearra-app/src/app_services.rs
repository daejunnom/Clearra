use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};

use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;
use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_core_executor::{
    CoreExecutionError, CoreExecutionResult, CoreExecutor, CorePostProcessScoreCell,
    CorePostProcessSpinCoverage, PercentService, PercentServiceError, WasmBuildProbabilityBackend,
    WasmCpuSearchBackend, WasmCpuSearchError,
};
use clearra_i18n::{LanguageId, LanguagePreference, LanguageResolver};
use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectiveMode, ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_postprocess::{
    CandidateExecution, CandidateExecutionAggregate, ExactScoringExecutionMaterializer,
    PcScoringPostProcessInput, PcScoringPostProcessor, ScoreCell, ScoreMatrix, SpinCoverageTarget,
    TSpinCoverageOnlyMaterializer,
};
use clearra_problem::SearchProblem;
use clearra_scoring::{builtin::tetrio_pc_score_with_spin_profile, profile::SpinProfileId};

use crate::{
    diagnostics::AppDiagnosticReport,
    io::{AppFilePolicy, AppFileResolver},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppServices {
    core_executor: AppCoreExecutorService,
    file_resolver: AppFileResolver,
    language_resolver: AppLanguageResolverService,
    clock: AppClock,
    diagnostic_sink: AppDiagnosticSink,
}

impl AppServices {
    pub fn new() -> Self {
        Self::default()
    }
}
impl AppServices {
    pub fn with_core_executor(mut self, core_executor: AppCoreExecutorService) -> Self {
        self.core_executor = core_executor;
        self
    }
}
impl AppServices {
    pub fn with_file_resolver(mut self, file_resolver: AppFileResolver) -> Self {
        self.file_resolver = file_resolver;
        self
    }
}
impl AppServices {
    pub fn with_language_resolver(mut self, language_resolver: AppLanguageResolverService) -> Self {
        self.language_resolver = language_resolver;
        self
    }
}
impl AppServices {
    pub fn with_clock(mut self, clock: AppClock) -> Self {
        self.clock = clock;
        self
    }
}
impl AppServices {
    pub fn with_diagnostic_sink(mut self, diagnostic_sink: AppDiagnosticSink) -> Self {
        self.diagnostic_sink = diagnostic_sink;
        self
    }
}
impl AppServices {
    pub fn core_executor(&self) -> &AppCoreExecutorService {
        &self.core_executor
    }
}
impl AppServices {
    pub fn file_resolver(&self) -> &AppFileResolver {
        &self.file_resolver
    }
}
impl AppServices {
    pub fn file_resolver_for(&self, policy: &AppFilePolicy) -> AppFileResolver {
        self.file_resolver.with_policy(policy.clone())
    }
}
impl AppServices {
    pub fn language_resolver(&self) -> &AppLanguageResolverService {
        &self.language_resolver
    }
}
impl AppServices {
    pub fn clock(&self) -> &AppClock {
        &self.clock
    }
}
impl AppServices {
    pub fn diagnostic_sink(&self) -> &AppDiagnosticSink {
        &self.diagnostic_sink
    }
}

impl Default for AppServices {
    fn default() -> Self {
        Self {
            core_executor: AppCoreExecutorService::default(),
            file_resolver: AppFileResolver::default(),
            language_resolver: AppLanguageResolverService::default(),
            clock: AppClock::default(),
            diagnostic_sink: AppDiagnosticSink::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppCoreExecutorBackend {
    NativeCore,
    WasmCpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppCoreExecutorService {
    backend: AppCoreExecutorBackend,
}

impl AppCoreExecutorService {
    pub const fn wasm_cpu() -> Self {
        Self {
            backend: AppCoreExecutorBackend::WasmCpu,
        }
    }
}

impl AppCoreExecutorService {
    pub fn service_name(&self) -> &'static str {
        match self.backend {
            AppCoreExecutorBackend::NativeCore => "clearra-core-executor",
            AppCoreExecutorBackend::WasmCpu => "clearra-wasm-cpu-search-backend",
        }
    }

    pub(crate) const fn supports_cooperative_wasm_search(&self) -> bool {
        matches!(self.backend, AppCoreExecutorBackend::WasmCpu)
    }

    pub(crate) fn postprocess_search_result(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if result.field("search_kind") == Some("build-probability") {
            apply_build_spin_postprocess(result, control)
        } else {
            apply_pc_postprocess(result, control)
        }
    }

    pub fn materialize_pc_scoring_partition(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if result.field("search_kind") == Some("build-probability")
            || result.field("postprocess_scoring_requested") != Some("true")
        {
            return Ok(result);
        }
        let score_policy = score_policy_from_result(&result);
        let profile_id = score_profile_for_policy(score_policy).id().to_owned();
        let mut cells = Vec::new();
        let mut complete = !result.exact_scoring_execution_batches().is_empty();
        for batch in result.exact_scoring_execution_batches() {
            let materialized = ExactScoringExecutionMaterializer::materialize_score_cells(
                batch,
                score_policy,
                control,
            )
            .map_err(|_| CoreExecutionError::Cancelled)?;
            complete &= materialized.complete();
            cells.extend(materialized.scored_executions().iter().map(|execution| {
                CorePostProcessScoreCell::new(
                    execution.candidate_identity(),
                    execution.pattern_id(),
                    execution.trace_identity(),
                    execution.score(),
                    execution.attack(),
                )
            }));
        }
        cells.sort_unstable();
        cells.dedup();
        Ok(result.with_postprocess_score_cells(cells, complete, profile_id))
    }

    pub fn materialize_distributed_postprocess_partition(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if result.field("search_kind") != Some("build-probability") {
            return self.materialize_pc_scoring_partition(result, control);
        }
        if result.field("postprocess_build_spin_requested") != Some("true") {
            return Ok(result);
        }
        let Some((target_id, target)) = build_spin_coverage_target(&result) else {
            return Ok(result);
        };
        let pass_index = result
            .usize_field("build_distributed_pass_index")
            .unwrap_or(0);
        let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
        let mut coverage_words = vec![0_u64; pattern_count.div_ceil(u64::BITS as usize)];
        let mut candidate_identities = Vec::new();
        let mut execution_count = 0_u128;
        let mut complete = !result.exact_scoring_execution_batches().is_empty();
        for batch in result.exact_scoring_execution_batches() {
            let materialized = TSpinCoverageOnlyMaterializer::materialize_target(
                batch,
                target,
                0..batch.patterns().len(),
                control,
            )
            .map_err(|_| CoreExecutionError::Cancelled)?;
            for (target_word, source_word) in coverage_words
                .iter_mut()
                .zip(materialized.covered_patterns().words())
            {
                *target_word |= source_word;
            }
            candidate_identities.extend(materialized.candidate_identities());
            execution_count = execution_count.saturating_add(materialized.execution_count());
            complete &= materialized.complete();
        }
        let shard = CorePostProcessSpinCoverage::new(
            target_id,
            pass_index,
            pattern_count,
            coverage_words,
            candidate_identities,
            execution_count,
            complete,
        );
        Ok(result.with_postprocess_spin_coverages(vec![shard]))
    }
}
impl AppCoreExecutorService {
    pub fn execute(
        &self,
        problem: &SearchProblem,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_with_cancellation(problem, &ExecutionCancellationToken::new())
    }
}
impl AppCoreExecutorService {
    pub fn execute_with_cancellation(
        &self,
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
    }
}
impl AppCoreExecutorService {
    pub fn execute_with_control(
        &self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        let result = match self.backend {
            AppCoreExecutorBackend::NativeCore => {
                CoreExecutor::execute_with_control(problem, control)
            }
            AppCoreExecutorBackend::WasmCpu => {
                WasmCpuSearchBackend::execute_with_control(problem, control)
                    .map_err(core_error_from_wasm)
            }
        }?;
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        control.report_progress("postprocess", 0, Some(1));
        let result = apply_pc_postprocess(result, control)?;
        control.report_progress("postprocess", 1, Some(1));
        Ok(result)
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_build_coverage_with_cancellation(
            problem,
            query,
            &ExecutionCancellationToken::new(),
        )
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage_with_cancellation(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_build_coverage_with_control(
            problem,
            query,
            &ExecutionControl::new(cancellation.clone()),
        )
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage_with_control(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        match self.backend {
            AppCoreExecutorBackend::NativeCore => {
                CoreExecutor::execute_build_coverage_with_control(problem, query, control)
            }
            AppCoreExecutorBackend::WasmCpu => Err(CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_build_coverage_not_connected",
            }),
        }
    }
}

impl Default for AppCoreExecutorService {
    fn default() -> Self {
        Self {
            backend: AppCoreExecutorBackend::NativeCore,
        }
    }
}

fn core_error_from_wasm(error: WasmCpuSearchError) -> CoreExecutionError {
    match error {
        WasmCpuSearchError::Unsupported { reason } => {
            CoreExecutionError::RuntimeUnavailable { component: reason }
        }
        WasmCpuSearchError::WorkerPoolUnavailable => CoreExecutionError::RuntimeUnavailable {
            component: "wasm_cpu_worker_pool_unavailable",
        },
        WasmCpuSearchError::InvalidProblem { reason } => CoreExecutionError::Pc(reason.to_owned()),
        WasmCpuSearchError::Cancelled => CoreExecutionError::Cancelled,
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_probability_with_control(
        &self,
        problem: &SearchProblem,
        field: clearra_problem::BuildProbabilityField,
        aggregation: clearra_problem::BuildProbabilityAggregation,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        let result = match self.backend {
            AppCoreExecutorBackend::WasmCpu => WasmBuildProbabilityBackend::execute_with_control(
                problem,
                field,
                aggregation,
                control,
            )
            .map_err(core_error_from_wasm),
            AppCoreExecutorBackend::NativeCore => Err(CoreExecutionError::RuntimeUnavailable {
                component: "native_build_probability_backend_not_connected",
            }),
        }?;
        apply_build_spin_postprocess(result, control)
    }
}
impl AppCoreExecutorService {
    pub fn execute_percent_with_control(
        &self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, PercentServiceError> {
        match self.backend {
            AppCoreExecutorBackend::NativeCore => {
                PercentService::execute_with_cancellation(problem, &control.cancellation)
            }
            AppCoreExecutorBackend::WasmCpu => Err(PercentServiceError::UnsupportedPreset),
        }
    }
}

fn apply_pc_postprocess(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if result.field("postprocess_scoring_requested") != Some("true") {
        return Ok(result);
    }

    let probability = result
        .field("coverage_probability")
        .unwrap_or("0")
        .to_owned();
    let score_policy = score_policy_from_result(&result);
    let pattern_weights = result
        .postprocess_pattern_weights()
        .iter()
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let weights_complete = pattern_weights.len() == pattern_count && pattern_count > 0;
    let search_objective_complete = result
        .bool_field("objective_search_complete")
        .unwrap_or(false);
    let retained_trace_count = result.usize_field("retained_trace_count").unwrap_or(0);
    let distributed_score_available = result.postprocess_score_profile_id().is_some();
    let postprocess = if distributed_score_available {
        let profile = score_profile_for_policy(score_policy);
        let mut identities = result.normalized_solution_identities().to_vec();
        identities.sort_unstable();
        identities.dedup();
        let profile_matches = result.postprocess_score_profile_id() == Some(profile.id());
        let mut identities_complete = true;
        let cells = result
            .postprocess_score_cells()
            .iter()
            .filter_map(
                |cell| match identities.binary_search(&cell.candidate_identity()) {
                    Ok(index) => Some(ScoreCell::new(
                        (index + 1) as u64,
                        cell.pattern_id(),
                        cell.trace_identity(),
                        cell.score(),
                        cell.attack(),
                        profile.accuracy_level().as_str(),
                    )),
                    Err(_) => {
                        identities_complete = false;
                        None
                    }
                },
            )
            .collect::<Vec<_>>();
        let execution_source_complete = result.postprocess_score_cells_complete()
            && profile_matches
            && identities_complete
            && search_objective_complete;
        let matrix = ScoreMatrix::from_materialized_cells(
            cells,
            &profile,
            pattern_count,
            execution_source_complete && weights_complete,
        );
        PcScoringPostProcessor::process_materialized_with_control(
            PcScoringPostProcessInput::new(
                result.postprocess_replay_trace(),
                &[],
                &pattern_weights,
                pattern_count,
                execution_source_complete && weights_complete,
                score_policy,
                search_objective_complete,
                &probability,
                retained_trace_count,
            ),
            matrix,
            control,
        )
    } else {
        let legacy_candidate_executions = candidate_execution_aggregates(&result);
        let exact_materialization = result
            .exact_scoring_execution_batch()
            .map(|batch| {
                ExactScoringExecutionMaterializer::materialize(batch, score_policy, control)
            })
            .transpose()
            .map_err(|_| CoreExecutionError::Cancelled)?;
        let candidate_executions = exact_materialization
            .as_ref()
            .map_or(legacy_candidate_executions.as_slice(), |materialized| {
                materialized.aggregates()
            });
        let execution_source_complete = exact_materialization
            .as_ref()
            .map_or(result.postprocess_execution_complete(), |materialized| {
                materialized.complete()
            })
            && search_objective_complete;
        PcScoringPostProcessor::process_with_control(
            PcScoringPostProcessInput::new(
                result.postprocess_replay_trace(),
                candidate_executions,
                &pattern_weights,
                pattern_count,
                execution_source_complete && weights_complete,
                score_policy,
                search_objective_complete,
                &probability,
                retained_trace_count,
            ),
            control,
        )
    }
    .map_err(|_| CoreExecutionError::Cancelled)?;

    let mut fields = postprocess.fields();
    fields.push((
        "score_execution_distribution".to_owned(),
        if distributed_score_available {
            "worker-partitions"
        } else {
            "coordinator"
        }
        .to_owned(),
    ));
    fields.push((
        "score_distributed_cell_count".to_owned(),
        result.postprocess_score_cells().len().to_string(),
    ));

    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    Ok(result.with_replaced_fields(fields))
}

fn apply_build_spin_postprocess(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if result.field("postprocess_build_spin_requested") != Some("true") {
        return Ok(result);
    }
    let Some((target_id, target)) = build_spin_coverage_target(&result) else {
        return Ok(result);
    };
    let field_prefix = "spin_search";
    let rule_id = format!("{}-first-success", target.spin_profile().id().as_str());
    let batches = result.exact_scoring_execution_batches();
    let shards = result.postprocess_spin_coverages();
    if batches.is_empty() && shards.is_empty() {
        return Ok(result.with_replaced_fields(vec![
            (
                format!("{field_prefix}_probability_complete"),
                "false".to_owned(),
            ),
            (format!("{field_prefix}_accuracy"), "unavailable".to_owned()),
            (
                format!("{field_prefix}_incomplete_reason"),
                "exact_execution_graph_not_materialized".to_owned(),
            ),
        ]));
    }
    let mut pattern_ids = BTreeSet::new();
    let mut candidates_by_pass = BTreeMap::new();
    let mut execution_count = 0_u128;
    let mut materialization_complete = true;
    if shards.is_empty() {
        for (batch_index, batch) in batches.iter().enumerate() {
            let materialized = TSpinCoverageOnlyMaterializer::materialize_target(
                batch,
                target,
                0..batch.patterns().len(),
                control,
            )
            .map_err(|_| CoreExecutionError::Cancelled)?;
            pattern_ids.extend(
                materialized
                    .covered_patterns()
                    .covered_patterns()
                    .into_iter()
                    .map(|pattern| pattern.index()),
            );
            candidates_by_pass
                .entry(batch_index)
                .or_insert_with(BTreeSet::new)
                .extend(materialized.candidate_identities());
            execution_count = execution_count.saturating_add(materialized.execution_count());
            materialization_complete &= materialized.complete();
        }
    } else {
        for shard in shards {
            materialization_complete &= shard.complete() && shard.target_id() == target_id;
            if shard.pattern_count() != result.usize_field("coverage_pattern_count").unwrap_or(0)
                || shard.covered_pattern_words().len()
                    != shard.pattern_count().div_ceil(u64::BITS as usize)
            {
                materialization_complete = false;
                continue;
            }
            pattern_ids.extend(spin_pattern_ids_from_words(
                shard.covered_pattern_words(),
                shard.pattern_count(),
            ));
            candidates_by_pass
                .entry(shard.pass_index())
                .or_insert_with(BTreeSet::new)
                .extend(shard.candidate_identities().iter().copied());
            execution_count = execution_count.saturating_add(shard.execution_count());
        }
    }
    let original_candidate_count = candidates_by_pass.get(&0).map_or(0, BTreeSet::len);
    let mirror_candidate_count = candidates_by_pass.get(&1).map_or(0, BTreeSet::len);
    let weights = result
        .postprocess_pattern_weights()
        .iter()
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let weights_complete = pattern_count > 0
        && weights.len() == pattern_count
        && pattern_ids.iter().all(|pattern| *pattern < pattern_count);
    let probability = if weights_complete {
        pattern_ids
            .iter()
            .map(|pattern| weights[*pattern])
            .sum::<f64>()
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let search_complete = result.bool_field("probability_complete").unwrap_or(false);
    let complete = materialization_complete && search_complete && weights_complete;
    let fields = vec![
        (
            format!("{field_prefix}_candidate_count"),
            original_candidate_count.to_string(),
        ),
        (
            format!("{field_prefix}_mirror_candidate_count"),
            mirror_candidate_count.to_string(),
        ),
        (
            format!("{field_prefix}_symmetry_batch_count"),
            candidates_by_pass.len().max(batches.len()).to_string(),
        ),
        (
            format!("{field_prefix}_covered_pattern_count"),
            pattern_ids.len().to_string(),
        ),
        (
            format!("{field_prefix}_execution_count"),
            execution_count.to_string(),
        ),
        (
            format!("{field_prefix}_probability"),
            if probability == 0.0 {
                "0".to_owned()
            } else {
                probability.to_string()
            },
        ),
        (
            format!("{field_prefix}_probability_complete"),
            complete.to_string(),
        ),
        (
            format!("{field_prefix}_accuracy"),
            if complete { "exact" } else { "incomplete" }.to_owned(),
        ),
        (format!("{field_prefix}_rule"), rule_id),
        (
            format!("{field_prefix}_coverage_basis"),
            result
                .field("coverage_basis")
                .unwrap_or("original-field-patterns")
                .to_owned(),
        ),
        (
            format!("{field_prefix}_incomplete_reason"),
            if complete {
                "none"
            } else if !weights_complete {
                "pattern_weights_not_materialized"
            } else if !search_complete {
                "build_probability_incomplete"
            } else {
                "execution_graph_incomplete"
            }
            .to_owned(),
        ),
        ("spin_coverage_target".to_owned(), target_id),
        (
            "spin_coverage_execution_distribution".to_owned(),
            if shards.is_empty() {
                "coordinator"
            } else {
                "worker-partitions"
            }
            .to_owned(),
        ),
    ];
    Ok(result.with_replaced_fields(fields))
}

fn spin_pattern_ids_from_words(words: &[u64], pattern_count: usize) -> Vec<usize> {
    (0..pattern_count)
        .filter(|pattern| words[pattern / u64::BITS as usize] & (1_u64 << (pattern % 64)) != 0)
        .collect()
}

fn build_spin_coverage_target(
    result: &CoreExecutionResult,
) -> Option<(String, SpinCoverageTarget)> {
    if result.field("build_probability_aggregation") != Some("spin") {
        return None;
    }
    let selection = result
        .field("spin_profile_requested")
        .and_then(SpinProfileSelection::parse)
        .unwrap_or(SpinProfileSelection::TSpins);
    let profile_id = spin_profile_id(selection);
    Some((
        format!("spin:{}", selection.as_str()),
        SpinCoverageTarget::any_line_clear(profile_id),
    ))
}

fn spin_profile_id(selection: SpinProfileSelection) -> SpinProfileId {
    match selection {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

fn score_policy_from_result(result: &CoreExecutionResult) -> ScoreObjectivePolicy {
    let mode = match result.field("score_objective_mode") {
        Some("summary") => ScoreObjectiveMode::Summary,
        _ => ScoreObjectiveMode::Disabled,
    };
    let profile = result
        .field("score_profile_requested")
        .and_then(ScoreProfileSelection::parse)
        .unwrap_or_default();
    let spin_profile = result
        .field("spin_profile_requested")
        .and_then(SpinProfileSelection::parse)
        .unwrap_or_default();
    ScoreObjectivePolicy::new(mode)
        .with_profile(profile)
        .with_spin_profile(spin_profile)
        .with_initial_b2b(
            result
                .field("score_initial_b2b")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        )
}

fn score_profile_for_policy(
    policy: ScoreObjectivePolicy,
) -> clearra_scoring::profile::ScoreProfile {
    tetrio_pc_score_with_spin_profile(spin_profile_id(policy.spin_profile()))
}

fn candidate_execution_aggregates(
    result: &CoreExecutionResult,
) -> Vec<CandidateExecutionAggregate> {
    let mut by_candidate = BTreeMap::<u64, Vec<CandidateExecution>>::new();
    for execution in result.postprocess_executions() {
        by_candidate
            .entry(execution.candidate_id())
            .or_default()
            .push(CandidateExecution::new(
                execution.pattern_id(),
                execution.trace_identity(),
                execution.replay_trace().clone(),
            ));
    }
    by_candidate
        .into_iter()
        .map(|(candidate_id, mut executions)| {
            executions.sort_by(|left, right| {
                (left.pattern_id(), left.trace_identity())
                    .cmp(&(right.pattern_id(), right.trace_identity()))
            });
            CandidateExecutionAggregate::new(candidate_id, executions)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppLanguageResolverService;

impl AppLanguageResolverService {
    pub fn service_name(&self) -> &'static str {
        "clearra-i18n-language-resolver"
    }
}
impl AppLanguageResolverService {
    pub fn resolve(&self, preference: &LanguagePreference) -> LanguageId {
        LanguageResolver::resolve(preference)
    }
}
impl AppLanguageResolverService {
    pub fn resolve_from_selected(&self, selected: Option<LanguageId>) -> LanguageId {
        LanguageResolver::resolve_from_selected(selected)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppClock;

impl AppClock {
    pub fn service_name(&self) -> &'static str {
        "system-clock"
    }
}
impl AppClock {
    pub fn unix_timestamp_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppDiagnosticSink;

impl AppDiagnosticSink {
    pub fn service_name(&self) -> &'static str {
        "app-diagnostic-sink"
    }
}
impl AppDiagnosticSink {
    pub fn observe(&self, _report: &AppDiagnosticReport) {}
}
