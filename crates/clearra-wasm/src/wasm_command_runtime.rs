use std::fmt;

use clearra_app::{
    AppContext, AppCoreExecutorService, AppRequest, AppServices, CooperativeAppAdvance,
    CooperativeAppExecution, ExecutionControl,
};
use clearra_host_contract::{AppResponse as HostAppResponse, Diagnostic, DiagnosticReport};
use clearra_web_command::{WebCommandError, WebCommandParser, WebCommandRequest};

use crate::WasmHostCapabilities;
use crate::WebGpuBackendReport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmExecutionResult {
    app_response: HostAppResponse,
    webgpu_backend: WebGpuBackendReport,
    search_report: Option<WasmSearchReport>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WasmSearchReport {
    pub backend_selected: String,
    pub workers_used: usize,
    pub cpu_parallel_execution: bool,
    pub cpu_parallel_decision_reason: String,
    pub cpu_warmup_requested: bool,
    pub cpu_warmup_performed: bool,
    pub supply_window_resolution: String,
    pub projects_unplaced_lookahead: bool,
    pub source_sequence_length: usize,
    pub total_possible_pattern_count: String,
    pub solution_found: bool,
    pub packing_candidate_count: usize,
    pub geometry_candidate_family_count: String,
    pub packing_candidate_set_digest: String,
    pub packing_candidate_keys: Vec<String>,
    pub unique_solution_count: usize,
    pub normalized_solution_set_hash: String,
    pub normalized_solution_keys: Vec<String>,
    pub solution_probabilities: Vec<WasmSolutionProbability>,
    pub build_variant_count: u64,
    pub build_variant_count_exact: String,
    pub materialized_pattern_count: usize,
    pub covered_pattern_count: usize,
    pub coverage_probability: String,
    pub probability_complete: bool,
    pub count_complete: bool,
    pub searched_nodes: usize,
    pub geometry_domain_pruned_states: usize,
    pub geometry_hall_pruned_states: usize,
    pub geometry_column_pruned_states: usize,
    pub geometry_component_compositions: usize,
    pub peak_frontier_states: usize,
    pub peak_cpu_bytes: usize,
    pub peak_build_order_nodes: usize,
    pub total_build_order_nodes: usize,
    pub coverage_product_words: usize,
    pub coverage_product_states: usize,
    pub coverage_product_edge_checks: usize,
    pub piece_language_coverage_cache_hits: usize,
    pub piece_language_coverage_cache_misses: usize,
    pub standard_bag_symbolic_cache_hits: usize,
    pub standard_bag_symbolic_cache_misses: usize,
    pub peak_reachability_states: usize,
    pub total_reachability_states: usize,
    pub reachability_lock_queries: usize,
    pub reachability_harddrop_queries: usize,
    pub reachability_harddrop_hits: usize,
    pub reachability_cache_reachable_hits: usize,
    pub reachability_cache_unreachable_hits: usize,
    pub reachability_cache_key_misses: usize,
    pub reachability_partial_searches: usize,
    pub reachability_exhaustive_searches: usize,
    pub realization_feasibility_states: usize,
    pub realization_feasibility_rejected_candidates: usize,
    pub resource_truncated: bool,
    pub resource_truncation_reason: String,
    pub representative_candidate_id: Option<String>,
    pub representative_pattern_id: Option<u32>,
    pub representative_path: Vec<WasmSearchPathStep>,
    pub summary_fields: Vec<(String, String)>,
    pub forward_search_kind: Option<String>,
    pub forward_initial_board_mask: Option<String>,
    pub maximum_damage: Option<u32>,
    pub forward_outcomes: Vec<WasmForwardSearchOutcome>,
    pub setup_report: Option<WasmSetupFinderReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSetupFinderReport {
    pub search_mode: String,
    pub cycle: u8,
    pub remaining_pieces: String,
    pub queue_based_pieces: String,
    pub next_cycle_remaining_pieces: String,
    pub post_cycle_borrow_enabled: bool,
    pub coverage_semantics: String,
    pub geometry_family_count: String,
    pub partial_build_node_count: usize,
    pub complete: bool,
    pub hold_conditions: Vec<WasmSetupHoldCondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSetupHoldCondition {
    pub condition_id: String,
    pub initial_hold: Option<String>,
    pub pattern_expression: String,
    pub pattern_count: usize,
    pub candidate_count: usize,
    pub result_truncated: bool,
    pub complete: bool,
    pub candidates: Vec<WasmSetupCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSetupCandidate {
    pub setup_id: String,
    pub board_mask: String,
    pub min_locks: u8,
    pub max_locks: u8,
    pub build_covered_patterns: usize,
    pub joint_covered_patterns: usize,
    pub build_probability: String,
    pub joint_probability: String,
    pub conditional_pc_probability: String,
    pub representative_path: Vec<WasmSearchPathStep>,
    pub solution_path_count: usize,
    pub solution_paths_complete: bool,
    pub solution_paths: Vec<Vec<WasmSearchPathStep>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmForwardSearchOutcome {
    pub id: String,
    pub source_pattern_index: u32,
    pub source_queue: String,
    pub group: Option<String>,
    pub final_board_mask: String,
    pub spin_piece: Option<String>,
    pub spin_mini: bool,
    pub spin_lines: u8,
    pub total_damage: u32,
    pub path: Vec<WasmForwardPathStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmForwardPathStep {
    pub piece: String,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
    pub hold: String,
    pub cleared_lines: u8,
    pub spin_piece: Option<String>,
    pub spin_mini: bool,
    pub damage: u32,
    pub total_damage: u32,
    pub placement_mask: String,
    pub cleared_row_mask: u32,
    pub board_after_mask: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSolutionProbability {
    pub solution_key: String,
    pub probability: String,
    pub covered_pattern_count: usize,
    pub pattern_count: usize,
    pub probability_complete: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WasmSearchPathStep {
    pub piece: String,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
    pub hold: String,
    pub cleared_lines: u8,
}

impl WasmSearchReport {
    pub(crate) fn from_response(response: &clearra_app::AppResponse) -> Option<Self> {
        let render_model = response.render_model()?;
        if let Some(result) = render_model.forward_search_result() {
            let outcomes = result
                .outcomes()
                .iter()
                .map(|outcome| WasmForwardSearchOutcome {
                    id: outcome.id().to_string(),
                    source_pattern_index: outcome.source_pattern_index(),
                    source_queue: outcome
                        .source_queue()
                        .iter()
                        .map(|piece| piece.as_ascii())
                        .collect(),
                    group: outcome.group().map(|group| group.as_str().to_owned()),
                    final_board_mask: board_words_hex(outcome.final_board()),
                    spin_piece: outcome
                        .spin_piece()
                        .map(|piece| piece.as_ascii().to_string()),
                    spin_mini: outcome.spin_mini(),
                    spin_lines: outcome.spin_lines(),
                    total_damage: outcome.total_damage(),
                    path: outcome
                        .path()
                        .iter()
                        .map(|step| {
                            let spin = step.spin();
                            WasmForwardPathStep {
                                piece: step.piece().as_ascii().to_string(),
                                rotation: step.rotation().quarter_turns(),
                                x: i32::from(step.x()),
                                y: i32::from(step.y()),
                                hold: step.hold_decision().to_owned(),
                                cleared_lines: step.cleared_lines(),
                                spin_piece: spin.map(|(piece, _)| piece.to_string()),
                                spin_mini: spin.is_some_and(|(_, mini)| mini),
                                damage: step.damage(),
                                total_damage: step.total_damage(),
                                placement_mask: board_words_hex(step.placement_mask()),
                                cleared_row_mask: step.cleared_row_mask(),
                                board_after_mask: board_words_hex(step.board_after()),
                            }
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            return Some(Self {
                backend_selected: "wasm-cpu-forward-search".to_owned(),
                workers_used: result.workers_used(),
                cpu_parallel_execution: result.workers_used() > 1,
                cpu_parallel_decision_reason: if result.workers_used() > 1 {
                    "forward_search_exact_layer_map_reduce"
                } else {
                    "forward_search_below_parallel_threshold"
                }
                .to_owned(),
                solution_found: !outcomes.is_empty(),
                unique_solution_count: outcomes.len(),
                count_complete: result.complete(),
                probability_complete: result.complete(),
                searched_nodes: usize::try_from(result.visited_states()).unwrap_or(usize::MAX),
                peak_frontier_states: result.peak_frontier(),
                forward_search_kind: Some(render_model.kind().as_str().to_owned()),
                forward_initial_board_mask: Some(board_words_hex(result.initial_board())),
                maximum_damage: result.maximum_damage(),
                forward_outcomes: outcomes,
                summary_fields: vec![
                    (
                        "forward_search_complete".to_owned(),
                        result.complete().to_string(),
                    ),
                    (
                        "visited_states".to_owned(),
                        result.visited_states().to_string(),
                    ),
                    (
                        "generated_locks".to_owned(),
                        result.generated_locks().to_string(),
                    ),
                ],
                ..Self::default()
            });
        }
        let result = render_model.core_result()?;
        Some(Self {
            backend_selected: result.field("backend_selected")?.to_owned(),
            workers_used: result.usize_field("workers_used").unwrap_or(1),
            cpu_parallel_execution: result.bool_field("cpu_parallel_execution").unwrap_or(false),
            cpu_parallel_decision_reason: result
                .field("cpu_parallel_decision_reason")
                .unwrap_or("unknown")
                .to_owned(),
            cpu_warmup_requested: result.bool_field("cpu_warmup_requested").unwrap_or(false),
            cpu_warmup_performed: result.bool_field("cpu_warmup_performed").unwrap_or(false),
            supply_window_resolution: result
                .field("supply_window_resolution")
                .unwrap_or("unknown")
                .to_owned(),
            projects_unplaced_lookahead: result
                .bool_field("projects_unplaced_lookahead")
                .unwrap_or(false),
            source_sequence_length: result.usize_field("source_sequence_length").unwrap_or(0),
            total_possible_pattern_count: result
                .field("total_possible_pattern_count")
                .unwrap_or("unknown")
                .to_owned(),
            solution_found: result.bool_field("solution_found").unwrap_or(false),
            packing_candidate_count: result.usize_field("packing_candidate_count").unwrap_or(0),
            geometry_candidate_family_count: result
                .field("geometry_candidate_family_count")
                .unwrap_or("overflow-or-incomplete")
                .to_owned(),
            packing_candidate_set_digest: result
                .field("packing_candidate_set_digest")
                .unwrap_or("0000000000000000")
                .to_owned(),
            packing_candidate_keys: result.packing_candidate_keys().to_vec(),
            unique_solution_count: result.usize_field("unique_solution_count").unwrap_or(0),
            normalized_solution_set_hash: result
                .field("normalized_solution_set_hash")
                .unwrap_or("cts1:cbf29ce484222325")
                .to_owned(),
            normalized_solution_keys: result.normalized_solution_keys().to_vec(),
            solution_probabilities: result
                .solution_probabilities()
                .iter()
                .map(|entry| WasmSolutionProbability {
                    solution_key: entry.solution_key().to_owned(),
                    probability: canonical_probability_field(Some(entry.probability())),
                    covered_pattern_count: entry.covered_pattern_count(),
                    pattern_count: entry.pattern_count(),
                    probability_complete: entry.probability_complete(),
                })
                .collect(),
            build_variant_count: result
                .field("build_variant_count")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            build_variant_count_exact: result
                .field("build_variant_count_exact")
                .unwrap_or("false")
                .to_owned(),
            materialized_pattern_count: result
                .usize_field("materialized_pattern_count")
                .unwrap_or(0),
            covered_pattern_count: result.usize_field("covered_pattern_count").unwrap_or(0),
            coverage_probability: canonical_probability_field(result.field("coverage_probability")),
            probability_complete: result.bool_field("probability_complete").unwrap_or(false),
            count_complete: result.bool_field("count_complete").unwrap_or(false),
            searched_nodes: result.usize_field("searched_nodes").unwrap_or(0),
            geometry_domain_pruned_states: result
                .usize_field("geometry_domain_pruned_states")
                .unwrap_or(0),
            geometry_hall_pruned_states: result
                .usize_field("geometry_hall_pruned_states")
                .unwrap_or(0),
            geometry_column_pruned_states: result
                .usize_field("geometry_column_pruned_states")
                .unwrap_or(0),
            geometry_component_compositions: result
                .usize_field("geometry_component_compositions")
                .unwrap_or(0),
            peak_frontier_states: result
                .usize_field("resource_peak_frontier_states")
                .unwrap_or(0),
            peak_cpu_bytes: result.usize_field("resource_peak_cpu_bytes").unwrap_or(0),
            peak_build_order_nodes: result.usize_field("peak_build_order_nodes").unwrap_or(0),
            total_build_order_nodes: result.usize_field("total_build_order_nodes").unwrap_or(0),
            coverage_product_words: result.usize_field("coverage_product_words").unwrap_or(0),
            coverage_product_states: result.usize_field("coverage_product_states").unwrap_or(0),
            coverage_product_edge_checks: result
                .usize_field("coverage_product_edge_checks")
                .unwrap_or(0),
            piece_language_coverage_cache_hits: result
                .usize_field("piece_language_coverage_cache_hits")
                .unwrap_or(0),
            piece_language_coverage_cache_misses: result
                .usize_field("piece_language_coverage_cache_misses")
                .unwrap_or(0),
            standard_bag_symbolic_cache_hits: result
                .usize_field("standard_bag_symbolic_cache_hits")
                .unwrap_or(0),
            standard_bag_symbolic_cache_misses: result
                .usize_field("standard_bag_symbolic_cache_misses")
                .unwrap_or(0),
            peak_reachability_states: result.usize_field("peak_reachability_states").unwrap_or(0),
            total_reachability_states: result.usize_field("total_reachability_states").unwrap_or(0),
            reachability_lock_queries: result.usize_field("reachability_lock_queries").unwrap_or(0),
            reachability_harddrop_queries: result
                .usize_field("reachability_harddrop_queries")
                .unwrap_or(0),
            reachability_harddrop_hits: result
                .usize_field("reachability_harddrop_hits")
                .unwrap_or(0),
            reachability_cache_reachable_hits: result
                .usize_field("reachability_cache_reachable_hits")
                .unwrap_or(0),
            reachability_cache_unreachable_hits: result
                .usize_field("reachability_cache_unreachable_hits")
                .unwrap_or(0),
            reachability_cache_key_misses: result
                .usize_field("reachability_cache_key_misses")
                .unwrap_or(0),
            reachability_partial_searches: result
                .usize_field("reachability_partial_searches")
                .unwrap_or(0),
            reachability_exhaustive_searches: result
                .usize_field("reachability_exhaustive_searches")
                .unwrap_or(0),
            realization_feasibility_states: result
                .usize_field("realization_feasibility_states")
                .unwrap_or(0),
            realization_feasibility_rejected_candidates: result
                .usize_field("realization_feasibility_rejected_candidates")
                .unwrap_or(0),
            resource_truncated: result.bool_field("resource_truncated").unwrap_or(false),
            resource_truncation_reason: result
                .field("resource_truncation_reason")
                .unwrap_or("none")
                .to_owned(),
            representative_candidate_id: result
                .field("representative_candidate_id")
                .filter(|value| *value != "none")
                .map(ToOwned::to_owned),
            representative_pattern_id: result
                .field("representative_pattern_id")
                .and_then(|value| value.parse().ok()),
            representative_path: result
                .path_steps()
                .iter()
                .map(|step| WasmSearchPathStep {
                    piece: step.piece().as_ascii().to_string(),
                    rotation: step.rotation(),
                    x: step.x(),
                    y: step.y(),
                    hold: step.hold().to_owned(),
                    cleared_lines: step.cleared_lines(),
                })
                .collect(),
            summary_fields: result.summary_fields(),
            forward_search_kind: None,
            forward_initial_board_mask: None,
            maximum_damage: None,
            forward_outcomes: Vec::new(),
            setup_report: result
                .setup_finder_report()
                .map(|report| WasmSetupFinderReport {
                    search_mode: report.search_mode().keyword().to_owned(),
                    cycle: report.cycle(),
                    remaining_pieces: report.remaining_pieces().to_owned(),
                    queue_based_pieces: report.queue_based_pieces().to_owned(),
                    next_cycle_remaining_pieces: report.next_cycle_remaining_pieces().to_owned(),
                    post_cycle_borrow_enabled: report.post_cycle_borrow_enabled(),
                    coverage_semantics: report.coverage_semantics().to_owned(),
                    geometry_family_count: report.geometry_family_count().to_owned(),
                    partial_build_node_count: report.partial_build_node_count(),
                    complete: report.complete(),
                    hold_conditions: report
                        .hold_conditions()
                        .iter()
                        .map(|condition| WasmSetupHoldCondition {
                            condition_id: condition.condition_id().to_owned(),
                            initial_hold: condition
                                .initial_hold()
                                .map(|piece| piece.as_ascii().to_string()),
                            pattern_expression: condition.pattern_expression().to_owned(),
                            pattern_count: condition.pattern_count(),
                            candidate_count: condition.candidate_count(),
                            result_truncated: condition.result_truncated(),
                            complete: condition.complete(),
                            candidates: condition
                                .candidates()
                                .iter()
                                .map(|candidate| WasmSetupCandidate {
                                    setup_id: candidate.setup_id().to_owned(),
                                    board_mask: format!("0x{:x}", candidate.board_mask()),
                                    min_locks: candidate.min_locks(),
                                    max_locks: candidate.max_locks(),
                                    build_covered_patterns: candidate.build_covered_patterns(),
                                    joint_covered_patterns: candidate.joint_covered_patterns(),
                                    build_probability: candidate.build_probability().to_owned(),
                                    joint_probability: candidate.joint_probability().to_owned(),
                                    conditional_pc_probability: candidate
                                        .conditional_pc_probability()
                                        .to_owned(),
                                    representative_path: candidate
                                        .representative_path()
                                        .iter()
                                        .map(|step| WasmSearchPathStep {
                                            piece: step.piece().as_ascii().to_string(),
                                            rotation: step.rotation(),
                                            x: step.x(),
                                            y: step.y(),
                                            hold: step.hold().to_owned(),
                                            cleared_lines: step.cleared_lines(),
                                        })
                                        .collect(),
                                    solution_path_count: candidate.solution_path_count(),
                                    solution_paths_complete: candidate.solution_paths_complete(),
                                    solution_paths: candidate
                                        .solution_paths()
                                        .iter()
                                        .map(|path| {
                                            path.iter()
                                                .map(|step| WasmSearchPathStep {
                                                    piece: step.piece().as_ascii().to_string(),
                                                    rotation: step.rotation(),
                                                    x: step.x(),
                                                    y: step.y(),
                                                    hold: step.hold().to_owned(),
                                                    cleared_lines: step.cleared_lines(),
                                                })
                                                .collect()
                                        })
                                        .collect(),
                                })
                                .collect(),
                        })
                        .collect(),
                }),
        })
    }
}

fn board_words_hex(words: [u64; 4]) -> String {
    let highest = words.iter().rposition(|word| *word != 0).unwrap_or(0);
    let mut output = format!("0x{:x}", words[highest]);
    for word in words[..highest].iter().rev() {
        output.push_str(&format!("{word:016x}"));
    }
    output
}

impl WasmExecutionResult {
    pub(crate) fn from_app_response(
        response: clearra_app::AppResponse,
        webgpu_requested: bool,
    ) -> Self {
        let search_report = WasmSearchReport::from_response(&response);
        let webgpu_backend = WebGpuBackendReport::from_app_response(&response, webgpu_requested);
        Self {
            app_response: response.to_host_response(),
            webgpu_backend,
            search_report,
        }
    }

    pub fn app_response(&self) -> &HostAppResponse {
        &self.app_response
    }

    pub fn webgpu_backend(&self) -> &WebGpuBackendReport {
        &self.webgpu_backend
    }

    pub fn search_report(&self) -> Option<&WasmSearchReport> {
        self.search_report.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct WasmCommandRuntime {
    app_context: AppContext,
    host_capabilities: WasmHostCapabilities,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedWasmCommand {
    request: AppRequest,
    webgpu_requested: bool,
}

pub(crate) struct PreparedWasmExecution {
    execution: CooperativeAppExecution,
    webgpu_requested: bool,
}

pub(crate) enum PreparedWasmAdvance {
    Pending,
    Completed(WasmExecutionResult),
    Cancelled,
}

impl WasmCommandRuntime {
    pub fn new(app_context: AppContext) -> Self {
        Self {
            app_context,
            host_capabilities: WasmHostCapabilities::default(),
        }
    }

    pub fn with_host_capabilities(mut self, capabilities: WasmHostCapabilities) -> Self {
        self.host_capabilities = capabilities;
        self
    }

    pub fn set_host_capabilities(&mut self, capabilities: WasmHostCapabilities) {
        self.host_capabilities = capabilities;
    }

    pub(crate) fn app_context(&self) -> &AppContext {
        &self.app_context
    }

    pub fn compile_command_text(
        &self,
        command_text: &str,
    ) -> Result<AppRequest, WasmCommandRuntimeError> {
        self.parse_command(command_text)?
            .with_runtime_webgpu_available(self.host_capabilities.webgpu_available())
            .to_app_request()
            .map_err(WasmCommandRuntimeError::from_web_command)
    }

    pub fn run_command_text(
        &self,
        command_text: &str,
    ) -> Result<WasmExecutionResult, WasmCommandRuntimeError> {
        let prepared = self.prepare_command_text(command_text)?;
        self.execute_prepared(prepared)
    }

    pub(crate) fn prepare_command_text(
        &self,
        command_text: &str,
    ) -> Result<PreparedWasmCommand, WasmCommandRuntimeError> {
        let parsed = self.parse_command(command_text)?;
        let webgpu_requested = parsed.requests_webgpu();
        let request = parsed
            .with_runtime_webgpu_available(self.host_capabilities.webgpu_available())
            .to_app_request()
            .map_err(WasmCommandRuntimeError::from_web_command)?;
        Ok(PreparedWasmCommand {
            request,
            webgpu_requested,
        })
    }

    fn parse_command(
        &self,
        command_text: &str,
    ) -> Result<WebCommandRequest, WasmCommandRuntimeError> {
        WebCommandParser::parse_with_worker_limit(
            command_text,
            self.host_capabilities.logical_processor_count(),
        )
        .map_err(WasmCommandRuntimeError::from_web_command)
    }

    pub(crate) fn execute_prepared(
        &self,
        prepared: PreparedWasmCommand,
    ) -> Result<WasmExecutionResult, WasmCommandRuntimeError> {
        let control = ExecutionControl::default();
        let mut execution = self.start_prepared_execution(prepared);
        loop {
            match execution.advance(4096, &control) {
                PreparedWasmAdvance::Pending => {}
                PreparedWasmAdvance::Completed(result) => return Ok(result),
                PreparedWasmAdvance::Cancelled => {
                    return Err(WasmCommandRuntimeError::new(
                        "E_WASM_EXECUTION_STATE",
                        "default execution control was cancelled unexpectedly",
                    ));
                }
            }
        }
    }

    pub(crate) fn start_prepared_execution(
        &self,
        prepared: PreparedWasmCommand,
    ) -> PreparedWasmExecution {
        PreparedWasmExecution {
            execution: self
                .app_context
                .start_cooperative_execution(prepared.request),
            webgpu_requested: prepared.webgpu_requested,
        }
    }
}

fn canonical_probability_field(value: Option<&str>) -> String {
    let value = value.unwrap_or("0");
    match value.parse::<f64>() {
        Ok(number) if number == 0.0 => "0".to_owned(),
        _ => value.to_owned(),
    }
}

impl PreparedWasmCommand {
    pub(crate) fn into_parts(self) -> (AppRequest, bool) {
        (self.request, self.webgpu_requested)
    }
}

impl PreparedWasmExecution {
    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> PreparedWasmAdvance {
        match self.execution.advance(work_budget, control) {
            CooperativeAppAdvance::Pending => PreparedWasmAdvance::Pending,
            CooperativeAppAdvance::Cancelled => PreparedWasmAdvance::Cancelled,
            CooperativeAppAdvance::Completed(response) => PreparedWasmAdvance::Completed(
                WasmExecutionResult::from_app_response(response, self.webgpu_requested),
            ),
        }
    }
}

impl Default for WasmCommandRuntime {
    fn default() -> Self {
        Self::new(AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmCommandRuntimeError {
    code: &'static str,
    message: String,
}

impl WasmCommandRuntimeError {
    fn from_web_command(error: WebCommandError) -> Self {
        Self::new(error.code().as_diagnostic_code(), error.message())
    }

    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn diagnostic_report(&self) -> DiagnosticReport {
        DiagnosticReport::single(Diagnostic::new(self.code, "error", self.message.as_str()))
    }
}

impl fmt::Display for WasmCommandRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WasmCommandRuntimeError {}
