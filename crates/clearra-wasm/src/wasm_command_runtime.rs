// SRP rationale: this module has one behavior-level change reason: adapting the typed in-process
// App command lifecycle, from preparation through cooperative execution and report projection, to
// the public WASM host contract.
use std::{
    fmt,
    sync::{atomic::AtomicU32, Arc},
};

use clearra_app::{
    app_response::GovernedAppResponse, AppCommand, AppContext, AppCoreExecutorService,
    AppErrorCode, AppRequest, AppServices, CooperativeAppAdvance, CooperativeAppExecution,
    ExecutionControl, FiniteCooperativeCallerMemory, FiniteCooperativeCallerMemoryRejection,
    PcBestSaveWinnerV2, PcPathFamilyV2Result, PcPathWitnessV2, PcSaveCompletenessEvidence,
    PcSaveGroupV2, PcSavePieceMultiset, PcSaveWitness, ProductCapabilityContract,
    ProductCapabilityResultKind, ProductPageSourceOwner, PC_BEST_SAVE_SCHEMA,
    PC_PATH_CANONICAL_SELECTION, PC_SCORE_INFORMATIONAL_ATTACK_BASIS,
    PC_SCORE_PATTERN_WINNER_CONTRACT, PC_SCORE_SOLUTION_FIELD_CONTRACT,
    PORTFOLIO_MEMBER_PAGE_CONTRACT, PORTFOLIO_MEMBER_PAGE_SIZE,
};
use clearra_cli_command::{CliCommandError, CliCommandParser, CliCommandRequest};
use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_core_executor::{
    CoreExecutionResult, PcTilingMemoryAdmissionEvidence, TilingSolutionPageStore,
};
use clearra_host_contract::{
    AppResponse as HostAppResponse, AppResult as HostAppResult, AppStatus as HostAppStatus,
    BackendReport, BuildV2CandidateCoveragePayload, BuildV2ProductPayload,
    BuildV2ScoreWinnerPayload, CapabilityReport, ContinuationReport, CoveragePortfolioPagePayload,
    Diagnostic, DiagnosticReport, ExecutionAvailabilityReport, PcBestSavePayload,
    PcBestSaveWinnerPayload, PcPathFamilyPayload, PcPathStepPayload, PcPathWitnessPayload,
    PcSaveCompletenessPayload, PcSaveGroupPayload, PcSaveGroupsPayload, PcSavePieceMultisetPayload,
    PcSaveRunMetadataPayload, PcSaveWitnessPayload, PcScoreFieldPayload,
    PcScoreFieldSummaryPayload, ProductBuildIdentity, ProductCandidateMemberPayload,
    ProductResultPayload, ProductResultPayloadContent, RenderCapabilityReport, ResourceReport,
    ScorePatternWinnerFamilyPayload, ScorePatternWinnerPayload, SetupRankedCandidatePayload,
    SetupRankedFamilyPayload, SetupScoreCandidatePayload, SetupScoreRankingPayload,
    SpinStructureCandidatePayload, SpinStructureFamilyPayload, ARTIFACT_SCHEMA_VERSION,
    COMPILED_ENGINE_BUILD_ID, COMPILED_SOURCE_COMMIT, CONTRACT_SCHEMA_VERSION,
    HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES, SUPPLY_SEMANTICS_ID,
};

use crate::{WasmHostCapabilities, WebGpuBackendOutcomeState, WebGpuBackendReport};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmExecutionResult {
    app_response: HostAppResponse,
    webgpu_backend: WebGpuBackendReport,
    search_report: Option<WasmSearchReport>,
    tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
    product_page_source_owner: Option<ProductPageSourceOwner>,
}

/// Non-cloneable authority paired with a finite WASM transport result.
/// `actual_retained_bytes` includes the inline `WasmExecutionResult`, every
/// target-owned allocation, and any explicitly transferred shared page-store
/// backing.
#[derive(Debug, Eq, PartialEq)]
pub struct WasmExecutionMemoryAuthority {
    memory_limit_bytes: u128,
    actual_retained_bytes: u128,
}

impl WasmExecutionMemoryAuthority {
    pub const fn memory_limit_bytes(&self) -> u128 {
        self.memory_limit_bytes
    }

    pub const fn actual_retained_bytes(&self) -> u128 {
        self.actual_retained_bytes
    }
}

/// Consuming finite counterpart to the legacy cloneable execution result.
#[derive(Debug, Eq, PartialEq)]
pub struct GovernedWasmExecutionResult {
    result: WasmExecutionResult,
    authority: WasmExecutionMemoryAuthority,
}

impl GovernedWasmExecutionResult {
    pub fn result(&self) -> &WasmExecutionResult {
        &self.result
    }

    pub fn authority(&self) -> &WasmExecutionMemoryAuthority {
        &self.authority
    }

    pub fn into_parts(self) -> (WasmExecutionResult, WasmExecutionMemoryAuthority) {
        (self.result, self.authority)
    }

    #[cfg(test)]
    pub(crate) fn with_memory_limit_for_test(mut self, memory_limit_bytes: u128) -> Self {
        self.authority.memory_limit_bytes = memory_limit_bytes;
        self
    }
}

const WASM_FINITE_MEMORY_LIMIT: &str = "E_WASM_FINITE_MEMORY_LIMIT";
const WASM_FINITE_ALLOCATION: &str = "E_WASM_FINITE_ALLOCATION";
const WASM_FINITE_PROJECTION: &str = "E_WASM_FINITE_PROJECTION";
const WASM_FINITE_APP_ENTRY: &str = "E_WASM_FINITE_APP_ENTRY";
const WASM_FINITE_AUTHORITY_UNAVAILABLE: &str = "E_WASM_FINITE_AUTHORITY_UNAVAILABLE";
const WASM_MIB_BYTES: u128 = 1024 * 1024;

#[derive(Clone, Copy)]
enum WasmFiniteConversionRoute {
    PublicDirect,
    CooperativeAdvance,
    DistributedFinish,
}

impl WasmFiniteConversionRoute {
    fn checked_caller_retained_bytes(self) -> Option<u128> {
        match self {
            Self::CooperativeAdvance => (core::mem::size_of::<PreparedWasmExecution>() as u128)
                .checked_add(checked_finite_direct_default_control_retained_owner_bytes()?),
            Self::PublicDirect | Self::DistributedFinish => Some(0),
        }
    }

    fn returned_carrier_inline_bytes(self) -> u128 {
        let wrapper = core::mem::size_of::<GovernedWasmExecutionResult>();
        let parts = core::mem::size_of::<(WasmExecutionResult, WasmExecutionMemoryAuthority)>();
        let result =
            core::mem::size_of::<Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>>();
        (match self {
            Self::CooperativeAdvance => wrapper
                .max(parts)
                .max(result)
                .max(core::mem::size_of::<PreparedWasmAdvance>()),
            Self::PublicDirect | Self::DistributedFinish => wrapper.max(parts).max(result),
        }) as u128
    }
}

struct WasmFiniteMemoryLedger {
    source_live_bytes: u128,
    caller_retained_bytes: u128,
    returned_carrier_inline_bytes: u128,
    target_inline_bytes: u128,
    target_heap_bytes: u128,
    memory_limit_bytes: u128,
}

impl WasmFiniteMemoryLedger {
    fn new(
        source_live_bytes: u128,
        memory_limit_bytes: u128,
        route: WasmFiniteConversionRoute,
    ) -> Result<Self, WasmCommandRuntimeError> {
        let ledger = Self {
            source_live_bytes,
            caller_retained_bytes: route
                .checked_caller_retained_bytes()
                .ok_or_else(finite_projection_error)?,
            returned_carrier_inline_bytes: route.returned_carrier_inline_bytes(),
            target_inline_bytes: core::mem::size_of::<WasmExecutionResult>() as u128,
            target_heap_bytes: 0,
            memory_limit_bytes,
        };
        ledger.authorize_requested(0)?;
        Ok(ledger)
    }

    fn checked_live_with_requested(&self, requested_bytes: u128) -> Option<u128> {
        self.source_live_bytes
            .checked_add(self.caller_retained_bytes)?
            .checked_add(self.target_inline_bytes)?
            .checked_add(self.target_heap_bytes)?
            .checked_add(requested_bytes)
    }

    fn authorize_requested(&self, requested_bytes: u128) -> Result<(), WasmCommandRuntimeError> {
        let required = self
            .checked_live_with_requested(requested_bytes)
            .ok_or_else(finite_projection_error)?;
        if required > self.memory_limit_bytes {
            return Err(finite_limit_error());
        }
        Ok(())
    }

    fn retain_actual(&mut self, actual_bytes: u128) -> Result<(), WasmCommandRuntimeError> {
        self.target_heap_bytes = self
            .target_heap_bytes
            .checked_add(actual_bytes)
            .ok_or_else(finite_projection_error)?;
        self.authorize_requested(0)
    }

    fn target_heap_bytes(&self) -> u128 {
        self.target_heap_bytes
    }

    fn finish_source(
        mut self,
        shared_page_store_bytes: u128,
    ) -> Result<WasmExecutionMemoryAuthority, WasmCommandRuntimeError> {
        self.source_live_bytes = 0;
        self.target_heap_bytes = self
            .target_heap_bytes
            .checked_add(shared_page_store_bytes)
            .ok_or_else(finite_projection_error)?;
        self.authorize_requested(0)?;
        let actual_retained_bytes = self
            .target_inline_bytes
            .checked_add(self.target_heap_bytes)
            .ok_or_else(finite_projection_error)?;
        let final_required = self
            .caller_retained_bytes
            .checked_add(self.returned_carrier_inline_bytes)
            .and_then(|bytes| bytes.checked_add(self.target_heap_bytes))
            .ok_or_else(finite_projection_error)?;
        if final_required > self.memory_limit_bytes {
            return Err(finite_limit_error());
        }
        Ok(WasmExecutionMemoryAuthority {
            memory_limit_bytes: self.memory_limit_bytes,
            actual_retained_bytes,
        })
    }
}

fn finite_limit_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(WASM_FINITE_MEMORY_LIMIT, String::new())
}

fn finite_allocation_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(WASM_FINITE_ALLOCATION, String::new())
}

fn finite_projection_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(WASM_FINITE_PROJECTION, String::new())
}

fn governed_app_transition_metadata_bytes() -> Option<u128> {
    let app_inline = core::mem::size_of::<clearra_app::AppResponse>() as u128;
    let wrapper_inline = core::mem::size_of::<GovernedAppResponse>() as u128;
    let parts_inline =
        core::mem::size_of::<(clearra_app::AppResponse, Option<u128>, u128)>() as u128;
    wrapper_inline.max(parts_inline).checked_sub(app_inline)
}

fn try_owned_string(
    value: &str,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    ledger.authorize_requested(value.len() as u128)?;
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = output.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    if actual_capacity < value.len() {
        return Err(finite_projection_error());
    }
    output.push_str(value);
    if output.capacity() != actual_capacity {
        return Err(finite_projection_error());
    }
    Ok(output)
}

fn try_optional_owned_string(
    value: Option<&str>,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Option<String>, WasmCommandRuntimeError> {
    value
        .map(|value| try_owned_string(value, ledger))
        .transpose()
}

fn try_owned_vec<T, U>(
    values: &[T],
    ledger: &mut WasmFiniteMemoryLedger,
    mut convert: impl FnMut(&T, &mut WasmFiniteMemoryLedger) -> Result<U, WasmCommandRuntimeError>,
) -> Result<Vec<U>, WasmCommandRuntimeError> {
    let requested = (values.len() as u128)
        .checked_mul(core::mem::size_of::<U>() as u128)
        .ok_or_else(finite_projection_error)?;
    ledger.authorize_requested(requested)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(values.len())
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = output.capacity();
    let actual_bytes = (actual_capacity as u128)
        .checked_mul(core::mem::size_of::<U>() as u128)
        .ok_or_else(finite_projection_error)?;
    ledger.retain_actual(actual_bytes)?;
    if actual_capacity < values.len() {
        return Err(finite_projection_error());
    }
    for value in values {
        output.push(convert(value, ledger)?);
        if output.capacity() != actual_capacity {
            return Err(finite_projection_error());
        }
    }
    Ok(output)
}

fn try_owned_string_vec(
    values: &[String],
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Vec<String>, WasmCommandRuntimeError> {
    try_owned_vec(values, ledger, |value, ledger| {
        try_owned_string(value, ledger)
    })
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
    pub projects_standard_bag_lookahead: bool,
    pub source_sequence_length: usize,
    pub total_possible_pattern_count: String,
    pub solution_found: bool,
    pub packing_candidate_count: usize,
    pub geometry_candidate_family_count: String,
    pub packing_candidate_set_digest: String,
    pub packing_candidate_keys: Vec<String>,
    pub unique_solution_count: usize,
    pub solution_count_calculated: bool,
    pub solution_set_materialized: bool,
    pub solution_keys_materialized_count: usize,
    pub solution_keys_complete: bool,
    pub solution_page_available: bool,
    pub normalized_solution_set_hash: String,
    pub normalized_solution_keys: Vec<String>,
    pub solution_probabilities: Vec<WasmSolutionProbability>,
    pub solution_average_scores: Vec<WasmSolutionAverageScore>,
    pub finesse_report: Option<WasmFinesseReport>,
    pub build_variant_count: u64,
    pub build_variant_count_exact: String,
    pub buildability_verified: bool,
    pub coverage_calculated: bool,
    pub probability_calculated: bool,
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
    pub forward_canonical_selection: Option<String>,
    pub canonical_forward_outcome: Option<WasmForwardSearchOutcome>,
    pub maximum_damage: Option<u32>,
    pub maximum_ren: Option<u8>,
    pub forward_outcomes: Vec<WasmForwardSearchOutcome>,
    pub setup_report: Option<WasmSetupFinderReport>,
    pub spin_structure_report: Option<WasmSpinStructureReport>,
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
    pub continuation_supply_semantics: String,
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
    pub candidate_id: String,
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
pub struct WasmSpinStructureReport {
    pub initial_board_mask: String,
    pub height: u8,
    pub inventory: String,
    pub spin_profile: String,
    pub line_requirement: String,
    pub fill_bottom: u8,
    pub fill_top: u8,
    pub rule_profile: String,
    pub minimality: String,
    pub minimum_placements: Option<u8>,
    pub workers_used: u16,
    pub complete: bool,
    pub regular: Vec<WasmSpinStructureOutcome>,
    pub mini: Vec<WasmSpinStructureOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSpinStructureOutcome {
    pub candidate_id: String,
    pub partition: String,
    pub placement_count: usize,
    pub board_before_spin: String,
    pub final_board: String,
    pub cleared_lines: u8,
    pub logical_spin_cleared_rows: u32,
    pub logical_spin: WasmStructureOperation,
    pub logical_operations: Vec<WasmStructureOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmStructureOperation {
    pub piece: String,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub logical_mask: String,
    pub need_deleted_rows: u32,
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
    pub ren_count: Option<u8>,
    pub total_damage: u32,
    pub evidence_path_count: String,
    pub evidence_complete: bool,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmSolutionAverageScore {
    pub solution_key: String,
    pub average_score: String,
    pub covered_pattern_count: usize,
    pub pattern_count: usize,
    pub score_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmFinesseSolutionAverage {
    pub solution_key: String,
    pub average_inputs: String,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmFinessePolicyResult {
    pub policy: String,
    pub overall_average_inputs: String,
    pub complete: bool,
    pub oracle_on_covered_average_inputs: Option<String>,
    pub information_penalty_inputs: Option<String>,
    pub success_probability_gap: Option<String>,
    pub successful_probability_mass: Option<String>,
    pub successful_unique_queue_count: Option<usize>,
    pub total_unique_queue_count: Option<usize>,
    pub solution_averages: Vec<WasmFinesseSolutionAverage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmFinesseRepresentativeWitness {
    pub policy: String,
    pub solution_key: Option<String>,
    pub pattern_ids: Vec<usize>,
    pub queue: Vec<String>,
    pub total_inputs: u32,
    pub input_sequence: Vec<String>,
    pub placements: Vec<WasmFinessePlacement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmFinessePlacement {
    pub piece: String,
    pub rotation: u8,
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmFinesseReport {
    pub mode: String,
    pub metric: String,
    pub pattern_knowledge: String,
    pub complete: bool,
    pub exact_total_inputs: Option<String>,
    pub representative_witness: Option<WasmFinesseRepresentativeWitness>,
    pub policy_results: Vec<WasmFinessePolicyResult>,
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

fn checked_vec_capacity_bytes<T>(values: &Vec<T>) -> Option<u128> {
    (values.capacity() as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn checked_optional_string_capacity(bytes: u128, value: &Option<String>) -> Option<u128> {
    bytes.checked_add(value.as_ref().map_or(0, |value| value.capacity() as u128))
}

fn checked_string_vec_capacity(values: &Vec<String>) -> Option<u128> {
    let mut bytes = checked_vec_capacity_bytes(values)?;
    for value in values {
        bytes = bytes.checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

impl WasmSearchPathStep {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.piece.capacity() as u128).checked_add(self.hold.capacity() as u128)
    }
}

impl WasmForwardPathStep {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = (self.piece.capacity() as u128).checked_add(self.hold.capacity() as u128)?;
        let bytes = checked_optional_string_capacity(bytes, &self.spin_piece)?;
        bytes
            .checked_add(self.placement_mask.capacity() as u128)?
            .checked_add(self.board_after_mask.capacity() as u128)
    }
}

impl WasmForwardSearchOutcome {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.id.capacity() as u128)
            .checked_add(self.source_queue.capacity() as u128)?
            .checked_add(self.final_board_mask.capacity() as u128)?
            .checked_add(self.evidence_path_count.capacity() as u128)?;
        bytes = checked_optional_string_capacity(bytes, &self.group)?;
        bytes = checked_optional_string_capacity(bytes, &self.spin_piece)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.path)?)?;
        for step in &self.path {
            bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmStructureOperation {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.piece.capacity() as u128).checked_add(self.logical_mask.capacity() as u128)
    }
}

impl WasmSpinStructureOutcome {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.candidate_id.capacity() as u128)
            .checked_add(self.partition.capacity() as u128)?
            .checked_add(self.board_before_spin.capacity() as u128)?
            .checked_add(self.final_board.capacity() as u128)?
            .checked_add(self.logical_spin.checked_retained_capacity_bytes()?)?
            .checked_add(checked_vec_capacity_bytes(&self.logical_operations)?)?;
        for operation in &self.logical_operations {
            bytes = bytes.checked_add(operation.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSpinStructureReport {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.initial_board_mask.capacity() as u128)
            .checked_add(self.inventory.capacity() as u128)?
            .checked_add(self.spin_profile.capacity() as u128)?
            .checked_add(self.line_requirement.capacity() as u128)?
            .checked_add(self.rule_profile.capacity() as u128)?
            .checked_add(self.minimality.capacity() as u128)?
            .checked_add(checked_vec_capacity_bytes(&self.regular)?)?
            .checked_add(checked_vec_capacity_bytes(&self.mini)?)?;
        for outcome in self.regular.iter().chain(&self.mini) {
            bytes = bytes.checked_add(outcome.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSolutionProbability {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.probability.capacity() as u128)
    }
}

impl WasmSolutionAverageScore {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.average_score.capacity() as u128)
    }
}

impl WasmFinesseSolutionAverage {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.solution_key.capacity() as u128).checked_add(self.average_inputs.capacity() as u128)
    }
}

impl WasmFinessePolicyResult {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.policy.capacity() as u128)
            .checked_add(self.overall_average_inputs.capacity() as u128)?;
        for value in [
            &self.oracle_on_covered_average_inputs,
            &self.information_penalty_inputs,
            &self.success_probability_gap,
            &self.successful_probability_mass,
        ] {
            bytes = checked_optional_string_capacity(bytes, value)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.solution_averages)?)?;
        for solution in &self.solution_averages {
            bytes = bytes.checked_add(solution.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmFinessePlacement {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.piece.capacity() as u128)
    }
}

impl WasmFinesseRepresentativeWitness {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.policy.capacity() as u128;
        bytes = checked_optional_string_capacity(bytes, &self.solution_key)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.pattern_ids)?)?;
        bytes = bytes.checked_add(checked_string_vec_capacity(&self.queue)?)?;
        bytes = bytes.checked_add(checked_string_vec_capacity(&self.input_sequence)?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.placements)?)?;
        for placement in &self.placements {
            bytes = bytes.checked_add(placement.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmFinesseReport {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.mode.capacity() as u128)
            .checked_add(self.metric.capacity() as u128)?
            .checked_add(self.pattern_knowledge.capacity() as u128)?;
        bytes = checked_optional_string_capacity(bytes, &self.exact_total_inputs)?;
        if let Some(witness) = &self.representative_witness {
            bytes = bytes.checked_add(witness.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.policy_results)?)?;
        for policy in &self.policy_results {
            bytes = bytes.checked_add(policy.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSetupCandidate {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.setup_id.capacity() as u128)
            .checked_add(self.board_mask.capacity() as u128)?
            .checked_add(self.build_probability.capacity() as u128)?
            .checked_add(self.joint_probability.capacity() as u128)?
            .checked_add(self.conditional_pc_probability.capacity() as u128)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.representative_path)?)?;
        for step in &self.representative_path {
            bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.solution_paths)?)?;
        for path in &self.solution_paths {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(path)?)?;
            for step in path {
                bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
            }
        }
        Some(bytes)
    }
}

impl WasmSetupHoldCondition {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.condition_id.capacity() as u128)
            .checked_add(self.pattern_expression.capacity() as u128)?;
        bytes = checked_optional_string_capacity(bytes, &self.initial_hold)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.candidates)?)?;
        for candidate in &self.candidates {
            bytes = bytes.checked_add(candidate.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSetupFinderReport {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.search_mode.capacity() as u128)
            .checked_add(self.remaining_pieces.capacity() as u128)?
            .checked_add(self.queue_based_pieces.capacity() as u128)?
            .checked_add(self.next_cycle_remaining_pieces.capacity() as u128)?
            .checked_add(self.coverage_semantics.capacity() as u128)?
            .checked_add(self.continuation_supply_semantics.capacity() as u128)?
            .checked_add(self.geometry_family_count.capacity() as u128)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.hold_conditions)?)?;
        for condition in &self.hold_conditions {
            bytes = bytes.checked_add(condition.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSearchReport {
    /// Returns the complete report-owned heap graph, including outer vector
    /// buffers and all nested string/vector slack, using actual capacities.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        for value in [
            &self.backend_selected,
            &self.cpu_parallel_decision_reason,
            &self.supply_window_resolution,
            &self.total_possible_pattern_count,
            &self.geometry_candidate_family_count,
            &self.packing_candidate_set_digest,
            &self.normalized_solution_set_hash,
            &self.build_variant_count_exact,
            &self.coverage_probability,
            &self.resource_truncation_reason,
        ] {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        for value in [
            &self.representative_candidate_id,
            &self.forward_search_kind,
            &self.forward_initial_board_mask,
            &self.forward_canonical_selection,
        ] {
            bytes = checked_optional_string_capacity(bytes, value)?;
        }
        bytes = bytes.checked_add(checked_string_vec_capacity(&self.packing_candidate_keys)?)?;
        bytes = bytes.checked_add(checked_string_vec_capacity(&self.normalized_solution_keys)?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.solution_probabilities)?)?;
        for probability in &self.solution_probabilities {
            bytes = bytes.checked_add(probability.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.solution_average_scores)?)?;
        for score in &self.solution_average_scores {
            bytes = bytes.checked_add(score.checked_retained_capacity_bytes()?)?;
        }
        if let Some(finesse) = &self.finesse_report {
            bytes = bytes.checked_add(finesse.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.representative_path)?)?;
        for step in &self.representative_path {
            bytes = bytes.checked_add(step.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.summary_fields)?)?;
        for (key, value) in &self.summary_fields {
            bytes = bytes
                .checked_add(key.capacity() as u128)?
                .checked_add(value.capacity() as u128)?;
        }
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.forward_outcomes)?)?;
        for outcome in &self.forward_outcomes {
            bytes = bytes.checked_add(outcome.checked_retained_capacity_bytes()?)?;
        }
        if let Some(outcome) = &self.canonical_forward_outcome {
            bytes = bytes.checked_add(outcome.checked_retained_capacity_bytes()?)?;
        }
        if let Some(setup) = &self.setup_report {
            bytes = bytes.checked_add(setup.checked_retained_capacity_bytes()?)?;
        }
        if let Some(spin_structure) = &self.spin_structure_report {
            bytes = bytes.checked_add(spin_structure.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl WasmSearchReport {
    pub(crate) fn from_response(response: &clearra_app::AppResponse) -> Option<Self> {
        let render_model = response.render_model()?;
        if render_model.kind() == clearra_app::AppResultKind::Sequence {
            let message = render_model.message()?;
            let summary_fields: Vec<_> = message
                .fields()
                .iter()
                .map(|field| (field.key().to_owned(), field.value().as_text()))
                .collect();
            let field = |key: &str| {
                summary_fields
                    .iter()
                    .find(|(candidate, _)| candidate == key)
                    .map(|(_, value)| value.as_str())
            };
            return Some(Self {
                backend_selected: "wasm-cpu-operation-sequence".to_owned(),
                workers_used: 1,
                cpu_parallel_decision_reason: "operation_sequence_serial_replay".to_owned(),
                solution_found: field("complete") == Some("true"),
                solution_count_calculated: true,
                count_complete: field("complete") == Some("true"),
                searched_nodes: field("operation_count")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                representative_candidate_id: field("trace_key").map(str::to_owned),
                summary_fields,
                ..Self::default()
            });
        }
        if render_model.kind() == clearra_app::AppResultKind::SequenceDependencies {
            let message = render_model.message()?;
            let summary_fields: Vec<_> = message
                .fields()
                .iter()
                .map(|field| (field.key().to_owned(), field.value().as_text()))
                .collect();
            let field = |key: &str| {
                summary_fields
                    .iter()
                    .find(|(candidate, _)| candidate == key)
                    .map(|(_, value)| value.as_str())
            };
            let exact_order_count = field("exact_order_count").unwrap_or("0");
            return Some(Self {
                backend_selected: "wasm-cpu-sequence-dependencies".to_owned(),
                workers_used: 1,
                cpu_parallel_decision_reason: "sequence_dependencies_serial_exact_dag".to_owned(),
                solution_found: exact_order_count != "0",
                solution_count_calculated: true,
                count_complete: field("complete") == Some("true"),
                searched_nodes: field("explored_state_count")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                representative_candidate_id: field("candidate_id").map(str::to_owned),
                summary_fields,
                ..Self::default()
            });
        }
        if let Some(result) = render_model.spin_structure_result() {
            let query = result
                .query
                .as_ref()
                .expect("validated spin-structure results retain the normalized query");
            let operation = |operation: clearra_spin_structure_search::StructureOperation| {
                WasmStructureOperation {
                    piece: operation.piece().as_ascii().to_string(),
                    rotation: operation.rotation().quarter_turns(),
                    x: operation.x(),
                    y: operation.y(),
                    logical_mask: board_words_hex(operation.mask().words()),
                    need_deleted_rows: operation.need_deleted_rows(),
                }
            };
            let outcome = |outcome: &clearra_spin_structure_search::SpinStructureOutcome,
                           partition: &str| {
                WasmSpinStructureOutcome {
                    candidate_id: clearra_app::spin_structure_search_candidate_id(outcome),
                    partition: partition.to_owned(),
                    placement_count: outcome.placement_count(),
                    board_before_spin: board_words_hex(outcome.board_before_spin.words()),
                    final_board: board_words_hex(outcome.final_board.words()),
                    cleared_lines: outcome.spin.cleared_lines,
                    logical_spin_cleared_rows: outcome.logical_spin_cleared_rows(),
                    logical_spin: operation(outcome.logical_spin()),
                    logical_operations: outcome
                        .logical_operations()
                        .iter()
                        .copied()
                        .map(operation)
                        .collect(),
                }
            };
            let regular = result
                .regular
                .iter()
                .map(|candidate| outcome(candidate, "regular"))
                .collect::<Vec<_>>();
            let mini = result
                .mini
                .iter()
                .map(|candidate| outcome(candidate, "mini"))
                .collect::<Vec<_>>();
            let inventory = PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .flat_map(|piece| {
                    core::iter::repeat(piece.as_ascii())
                        .take(usize::from(query.inventory.count(piece)))
                })
                .collect();
            return Some(Self {
                backend_selected: "wasm-cpu-spin-structure".to_owned(),
                workers_used: usize::from(result.workers_used()),
                cpu_parallel_execution: result.workers_used() > 1,
                cpu_parallel_decision_reason: if result.workers_used() > 1 {
                    "spin_structure_exact_target_partition"
                } else {
                    "spin_structure_serial_target_partition"
                }
                .to_owned(),
                solution_found: !regular.is_empty() || !mini.is_empty(),
                unique_solution_count: regular.len() + mini.len(),
                solution_count_calculated: true,
                solution_set_materialized: true,
                solution_keys_materialized_count: regular.len() + mini.len(),
                solution_keys_complete: result.complete,
                solution_page_available: false,
                count_complete: result.complete,
                probability_complete: result.complete,
                searched_nodes: result
                    .layers
                    .iter()
                    .map(|layer| usize::try_from(layer.input_states).unwrap_or(usize::MAX))
                    .fold(0_usize, usize::saturating_add),
                peak_frontier_states: result
                    .layers
                    .iter()
                    .map(|layer| usize::try_from(layer.input_states).unwrap_or(usize::MAX))
                    .max()
                    .unwrap_or(0),
                summary_fields: vec![
                    (
                        "result_contract".to_owned(),
                        "spin-structure-family.v2".to_owned(),
                    ),
                    ("regular_count".to_owned(), regular.len().to_string()),
                    ("mini_count".to_owned(), mini.len().to_string()),
                    ("complete".to_owned(), result.complete.to_string()),
                ],
                spin_structure_report: Some(WasmSpinStructureReport {
                    initial_board_mask: board_words_hex(query.initial_board.words()),
                    height: query.height,
                    inventory,
                    spin_profile: query.mode.as_str().to_owned(),
                    line_requirement: query.line_requirement.as_str(),
                    fill_bottom: query.fill_bottom,
                    fill_top: query.fill_top,
                    rule_profile: query.rule_profile.as_str().to_owned(),
                    minimality: query.minimality.as_str().to_owned(),
                    minimum_placements: result.minimum_placements,
                    workers_used: result.workers_used(),
                    complete: result.complete,
                    regular,
                    mini,
                }),
                ..Self::default()
            });
        }
        if let Some(result) = render_model.forward_search_result() {
            let forward_kind = render_model.kind();
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
                    ren_count: outcome.ren_count(),
                    total_damage: outcome.total_damage(),
                    evidence_path_count: outcome.evidence_path_count().to_owned(),
                    evidence_complete: outcome.evidence_complete(),
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
            let canonical_forward_outcome = if forward_kind == clearra_app::AppResultKind::Ren {
                match result.canonical_outcome() {
                    Some(_) => outcomes.first().cloned(),
                    None => None,
                }
            } else {
                None
            };
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
                solution_count_calculated: true,
                solution_set_materialized: true,
                solution_keys_materialized_count: 0,
                solution_keys_complete: false,
                solution_page_available: false,
                count_complete: result.complete(),
                probability_complete: result.complete(),
                searched_nodes: usize::try_from(result.visited_states()).unwrap_or(usize::MAX),
                peak_frontier_states: result.peak_frontier(),
                forward_search_kind: Some(forward_kind.as_str().to_owned()),
                forward_initial_board_mask: Some(board_words_hex(result.initial_board())),
                forward_canonical_selection: (forward_kind == clearra_app::AppResultKind::Ren)
                    .then(|| "smallest-canonical-candidate-id".to_owned()),
                canonical_forward_outcome,
                maximum_damage: result.maximum_damage(),
                maximum_ren: result.maximum_ren(),
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
        // Finesse score deliberately keeps backend and worker implementation
        // details out of the user-facing CoreExecutionResult summary. It is a
        // serial WASM command, however, and still needs the same typed browser
        // search-report envelope as finesse search. Keep the adapter fallback
        // narrowly scoped to that tagged report so a missing backend field on
        // every other search result remains a contract error.
        let backend_selected = match result.field("backend_selected") {
            Some(backend) => backend.to_owned(),
            None if result
                .finesse_report()
                .is_some_and(|report| report.mode() == "score") =>
            {
                "wasm-cpu-finesse-score".to_owned()
            }
            None => return None,
        };
        let solution_availability = result.execution_report().solution_set_availability();
        let finesse_score_exception = result
            .finesse_report()
            .is_some_and(|report| report.mode() == "score");
        let solution_contract_valid = solution_availability.contract_valid()
            && solution_availability
                .materialized_key_count_matches(result.normalized_solution_keys().len())
            && (result.field("search_output_policy") != Some("tiling-only")
                || wasm_pc_tiling_publication_contract_is_valid(result));
        let solution_count_calculated =
            solution_contract_valid && solution_availability.solution_count_calculated();
        let solution_set_materialized =
            solution_contract_valid && solution_availability.solution_set_materialized();
        let solution_keys_materialized_count = if solution_contract_valid {
            solution_availability.solution_keys_materialized_count()
        } else {
            0
        };
        let solution_keys_complete =
            solution_contract_valid && solution_availability.solution_keys_complete();
        let solution_page_available =
            solution_contract_valid && solution_availability.solution_page_available();
        let unique_solution_count = if solution_count_calculated {
            result.usize_field("unique_solution_count").unwrap_or(0)
        } else {
            0
        };
        Some(Self {
            backend_selected,
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
            projects_standard_bag_lookahead: result
                .bool_field("projects_standard_bag_lookahead")
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
            packing_candidate_keys: if solution_set_materialized {
                result.packing_candidate_keys().to_vec()
            } else {
                Vec::new()
            },
            unique_solution_count,
            solution_count_calculated,
            solution_set_materialized,
            solution_keys_materialized_count,
            solution_keys_complete,
            solution_page_available,
            normalized_solution_set_hash: result
                .field("normalized_solution_set_hash")
                .filter(|_| solution_set_materialized)
                .unwrap_or("not-calculated")
                .to_owned(),
            normalized_solution_keys: if solution_set_materialized {
                result.normalized_solution_keys().to_vec()
            } else {
                Vec::new()
            },
            solution_probabilities: if solution_set_materialized {
                result
                    .solution_probabilities()
                    .iter()
                    .map(|entry| WasmSolutionProbability {
                        solution_key: entry.solution_key().to_owned(),
                        probability: canonical_probability_field(Some(entry.probability())),
                        covered_pattern_count: entry.covered_pattern_count(),
                        pattern_count: entry.pattern_count(),
                        probability_complete: entry.probability_complete(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
            solution_average_scores: if solution_set_materialized {
                result
                    .solution_average_scores()
                    .iter()
                    .map(|entry| WasmSolutionAverageScore {
                        solution_key: entry.solution_key().to_owned(),
                        average_score: entry.average_score().to_owned(),
                        covered_pattern_count: entry.covered_pattern_count(),
                        pattern_count: entry.pattern_count(),
                        score_complete: entry.score_complete(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
            finesse_report: result
                .finesse_report()
                .filter(|report| {
                    solution_set_materialized || finesse_score_exception && report.mode() == "score"
                })
                .map(|report| WasmFinesseReport {
                    mode: report.mode().to_owned(),
                    metric: report.metric().to_owned(),
                    pattern_knowledge: report.pattern_knowledge().to_owned(),
                    complete: report.complete(),
                    exact_total_inputs: report.exact_total_inputs().map(ToOwned::to_owned),
                    representative_witness: report.representative_witness().map(|witness| {
                        WasmFinesseRepresentativeWitness {
                            policy: witness.policy().to_owned(),
                            solution_key: witness.solution_key().map(ToOwned::to_owned),
                            pattern_ids: witness.pattern_ids().to_vec(),
                            queue: witness
                                .queue()
                                .iter()
                                .map(|piece| piece.as_ascii().to_string())
                                .collect(),
                            total_inputs: witness.total_inputs(),
                            input_sequence: witness
                                .input_sequence()
                                .iter()
                                .map(|input| input.as_str().to_owned())
                                .collect(),
                            placements: witness
                                .placements()
                                .iter()
                                .map(|placement| WasmFinessePlacement {
                                    piece: placement.piece().as_ascii().to_string(),
                                    rotation: placement.rotation().quarter_turns(),
                                    x: placement.x(),
                                    y: placement.y(),
                                })
                                .collect(),
                        }
                    }),
                    policy_results: report
                        .policy_results()
                        .iter()
                        .map(|policy| WasmFinessePolicyResult {
                            policy: policy.policy().to_owned(),
                            overall_average_inputs: policy.overall_average_inputs().to_owned(),
                            complete: policy.complete(),
                            oracle_on_covered_average_inputs: policy
                                .oracle_on_covered_average_inputs()
                                .map(ToOwned::to_owned),
                            information_penalty_inputs: policy
                                .information_penalty_inputs()
                                .map(ToOwned::to_owned),
                            success_probability_gap: policy
                                .success_probability_gap()
                                .map(ToOwned::to_owned),
                            successful_probability_mass: policy
                                .successful_probability_mass()
                                .map(ToOwned::to_owned),
                            successful_unique_queue_count: policy.successful_unique_queue_count(),
                            total_unique_queue_count: policy.total_unique_queue_count(),
                            solution_averages: policy
                                .solution_averages()
                                .iter()
                                .map(|solution| WasmFinesseSolutionAverage {
                                    solution_key: solution.solution_key().to_owned(),
                                    average_inputs: solution.average_inputs().to_owned(),
                                    complete: solution.complete(),
                                })
                                .collect(),
                        })
                        .collect(),
                }),
            build_variant_count: result
                .field("build_variant_count")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            build_variant_count_exact: result
                .field("build_variant_count_exact")
                .unwrap_or("false")
                .to_owned(),
            buildability_verified: result.bool_field("buildability_verified").unwrap_or(true),
            coverage_calculated: result.bool_field("coverage_calculated").unwrap_or(true),
            probability_calculated: result.bool_field("probability_calculated").unwrap_or(true),
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
            representative_candidate_id: solution_set_materialized
                .then(|| {
                    result
                        .field("representative_candidate_id")
                        .filter(|value| *value != "none")
                        .map(ToOwned::to_owned)
                })
                .flatten(),
            representative_pattern_id: solution_set_materialized
                .then(|| {
                    result
                        .field("representative_pattern_id")
                        .and_then(|value| value.parse().ok())
                })
                .flatten(),
            representative_path: if solution_set_materialized {
                result
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
                    .collect()
            } else {
                Vec::new()
            },
            summary_fields: result.fail_closed_solution_summary_fields(),
            forward_search_kind: None,
            forward_initial_board_mask: None,
            forward_canonical_selection: None,
            canonical_forward_outcome: None,
            maximum_damage: None,
            maximum_ren: None,
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
                    continuation_supply_semantics: report
                        .continuation_supply_semantics()
                        .to_owned(),
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
                                    candidate_id: clearra_app::setup_ranked_candidate_id(
                                        condition.condition_id(),
                                        candidate,
                                    ),
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
            spin_structure_report: None,
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

fn try_host_status(status: clearra_app::AppStatus) -> HostAppStatus {
    match status {
        clearra_app::AppStatus::Success => HostAppStatus::Success,
        clearra_app::AppStatus::ValidationFailed => HostAppStatus::ValidationFailed,
        clearra_app::AppStatus::Unsupported => HostAppStatus::Unsupported,
        clearra_app::AppStatus::ExecutionFailed => HostAppStatus::ExecutionFailed,
    }
}

fn host_error_code(code: AppErrorCode) -> &'static str {
    match code {
        AppErrorCode::MissingInput => "E_APP_INPUT_REQUIRED",
        AppErrorCode::InvalidInput => "E_APP_INPUT_INVALID",
        AppErrorCode::ProblemCompileFailed => "E_PROBLEM_COMPILE_FAILED",
        AppErrorCode::ExecutionFailed => "E_APP_EXECUTION_FAILED",
        AppErrorCode::TraceUnavailable => "E_PATH_TRACE_UNAVAILABLE",
        AppErrorCode::NoSolution => "E_PATH_NO_SOLUTION",
        AppErrorCode::Unsupported => "E_PRODUCT_RUNTIME_UNSUPPORTED",
        AppErrorCode::NativeCoreUnavailable => "E_NATIVE_CORE_UNAVAILABLE",
        AppErrorCode::BackendGpuUnavailable => "E_BACKEND_GPU_UNAVAILABLE",
        AppErrorCode::CliCommandUnsupported => "E_CLI_COMMAND_UNSUPPORTED",
        AppErrorCode::PcScenarioExpectedMismatch => "E_PC_SCENARIO_EXPECTED_MISMATCH",
        AppErrorCode::RulesProfileUnknown => "E_RULES_PROFILE_UNKNOWN",
        AppErrorCode::RulesInputRequired => "E_RULES_INPUT_REQUIRED",
        AppErrorCode::RulesInputInvalid => "E_RULES_INPUT_INVALID",
        AppErrorCode::RulesExportUnsupported => "E_RULES_EXPORT_UNSUPPORTED",
        AppErrorCode::ScoringProfileUnknown => "E_SCORING_PROFILE_UNKNOWN",
        AppErrorCode::ScoringInputRequired => "E_SCORING_INPUT_REQUIRED",
        AppErrorCode::ScoringInputInvalid => "E_SCORING_INPUT_INVALID",
        AppErrorCode::ConvertInputRequired => "E_CONVERT_INPUT_REQUIRED",
        AppErrorCode::ConvertDirectionUnsupported => "E_CONVERT_DIRECTION_UNSUPPORTED",
        AppErrorCode::ConvertInputInvalid => "E_CONVERT_INPUT_INVALID",
        AppErrorCode::ContinueTokenRequired => "E_CONTINUE_TOKEN_REQUIRED",
        AppErrorCode::ContinueTokenInvalid => "E_CONTINUE_TOKEN_INVALID",
        AppErrorCode::VerifyTargetUnknown => "E_VERIFY_TARGET_UNKNOWN",
        AppErrorCode::VerifyKicksFailed => "E_VERIFY_KICKS_FAILED",
        AppErrorCode::OperationSequenceInvalid => "E_OPERATION_SEQUENCE_INVALID",
        AppErrorCode::OperationSequenceCancelled => "E_OPERATION_SEQUENCE_CANCELLED",
        AppErrorCode::OperationSequenceTimedOut => "E_OPERATION_SEQUENCE_TIMED_OUT",
        AppErrorCode::OperationSequenceIncomplete => "E_OPERATION_SEQUENCE_INCOMPLETE",
        AppErrorCode::SequenceDependenciesInvalid => "E_SEQUENCE_DEPENDENCIES_INVALID",
        AppErrorCode::SequenceDependenciesCancelled => "E_SEQUENCE_DEPENDENCIES_CANCELLED",
        AppErrorCode::SequenceDependenciesTimedOut => "E_SEQUENCE_DEPENDENCIES_TIMED_OUT",
        AppErrorCode::SequenceDependenciesIncomplete => "E_SEQUENCE_DEPENDENCIES_INCOMPLETE",
        AppErrorCode::UtilityParityInvalid => "E_UTILITY_PARITY_INVALID",
        AppErrorCode::UtilityFumenInvalid => "E_UTILITY_FUMEN_INVALID",
        AppErrorCode::UtilityRenderInvalid => "E_UTILITY_RENDER_INVALID",
        AppErrorCode::UtilityRenderLimitExceeded => "E_UTILITY_RENDER_LIMIT_EXCEEDED",
        AppErrorCode::UtilityToGrayInvalid => "E_UTILITY_TO_GRAY_INVALID",
        AppErrorCode::UtilityMirrorInvalid => "E_UTILITY_MIRROR_INVALID",
    }
}

#[derive(Default)]
struct FmtLength {
    len: usize,
    overflowed: bool,
}

impl fmt::Write for FmtLength {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        match self.len.checked_add(value.len()) {
            Some(len) => self.len = len,
            None => self.overflowed = true,
        }
        Ok(())
    }
}

fn try_debug_lowercase(
    value: impl fmt::Debug,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    let mut length = FmtLength::default();
    fmt::write(&mut length, format_args!("{value:?}")).map_err(|_| finite_projection_error())?;
    if length.overflowed {
        return Err(finite_projection_error());
    }
    ledger.authorize_requested(length.len as u128)?;
    let mut output = String::new();
    output
        .try_reserve_exact(length.len)
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = output.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    fmt::write(&mut output, format_args!("{value:?}")).map_err(|_| finite_projection_error())?;
    if output.len() != length.len || output.capacity() != actual_capacity {
        return Err(finite_projection_error());
    }
    output.make_ascii_lowercase();
    Ok(output)
}

fn try_clone_backend_report(
    source: &BackendReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<BackendReport, WasmCommandRuntimeError> {
    Ok(BackendReport::from_owned_memory_authorized_parts_strict(
        try_owned_string(source.backend_requested(), ledger)?,
        try_owned_string(source.backend_selected(), ledger)?,
        source.fallback_used(),
        try_optional_owned_string(source.fallback_reason(), ledger)?,
        try_optional_owned_string(source.backend_fallback_reason(), ledger)?,
        try_optional_owned_string(source.fallback_backend(), ledger)?,
        try_optional_owned_string(source.gpu_failure_class(), ledger)?,
        try_optional_owned_string(source.gpu_failure_stage(), ledger)?,
        source.discarded_partial_gpu_result(),
        try_optional_owned_string(source.gpu_device_requested(), ledger)?,
        source.gpu_device_selected_index(),
        try_optional_owned_string(source.gpu_device_selected_name(), ledger)?,
        try_optional_owned_string(source.gpu_device_selected_type(), ledger)?,
        try_optional_owned_string(source.gpu_device_selected_backend(), ledger)?,
    ))
}

fn try_clone_availability_report(
    source: &ExecutionAvailabilityReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ExecutionAvailabilityReport, WasmCommandRuntimeError> {
    Ok(
        ExecutionAvailabilityReport::from_owned_memory_authorized_parts(
            source.state(),
            source.reason(),
            source.surface(),
            try_optional_owned_string(source.descriptor_pattern_count(), ledger)?,
            try_optional_owned_string(source.dense_pattern_count(), ledger)?,
            try_optional_owned_string(source.required_dense_bytes(), ledger)?,
            try_optional_owned_string(source.required_memory_bytes(), ledger)?,
        ),
    )
}

fn try_clone_resource_report(
    source: &ResourceReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ResourceReport, WasmCommandRuntimeError> {
    Ok(ResourceReport::from_owned_memory_authorized_parts(
        source.solver_executed(),
        try_owned_string(source.memory_status(), ledger)?,
        source.truncated(),
        try_optional_owned_string(source.truncation_reason(), ledger)?,
        source.peak_frontier_states,
        source.peak_candidate_rows,
        source.peak_hash_buckets,
        source.peak_gpu_bytes,
        source.peak_cpu_bytes,
        source.build_worker_backlog_peak,
        source.coverage_rows_emitted,
        source.probability_complete(),
        try_clone_availability_report(source.execution_availability(), ledger)?,
        source.result_completeness(),
    ))
}

fn try_clone_render_capability(
    source: &RenderCapabilityReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<RenderCapabilityReport, WasmCommandRuntimeError> {
    Ok(RenderCapabilityReport::new(
        source.png_supported(),
        source.gif_supported(),
        source.render_exact(),
        try_optional_owned_string(source.unsupported_reason(), ledger)?,
    ))
}

fn try_clone_capability_report(
    source: &CapabilityReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<CapabilityReport, WasmCommandRuntimeError> {
    Ok(CapabilityReport::from_owned_memory_authorized_parts(
        try_owned_string(source.app_request_boundary(), ledger)?,
        try_owned_string(source.executor_boundary(), ledger)?,
        source
            .render_capability()
            .map(|report| try_clone_render_capability(report, ledger))
            .transpose()?,
    ))
}

fn try_clone_continuation_report(
    source: &ContinuationReport,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ContinuationReport, WasmCommandRuntimeError> {
    Ok(ContinuationReport::new(
        source.available(),
        try_optional_owned_string(source.token(), ledger)?,
    ))
}

fn try_product_identity(
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ProductBuildIdentity, WasmCommandRuntimeError> {
    Ok(ProductBuildIdentity::from_owned_memory_authorized_parts(
        try_owned_string(COMPILED_ENGINE_BUILD_ID, ledger)?,
        try_owned_string(COMPILED_SOURCE_COMMIT, ledger)?,
        try_owned_string(CONTRACT_SCHEMA_VERSION, ledger)?,
        try_owned_string(SUPPLY_SEMANTICS_ID, ledger)?,
        try_owned_string(ARTIFACT_SCHEMA_VERSION, ledger)?,
    ))
}

fn try_host_diagnostics(
    source: &clearra_app::AppResponse,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Vec<Diagnostic>, WasmCommandRuntimeError> {
    let validation = source.diagnostics().validation().diagnostics();
    let extra_error = source.error().and_then(|error| {
        let code = host_error_code(error.code());
        (!validation
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code))
        .then_some((code, error.message()))
    });
    let len = validation
        .len()
        .checked_add(usize::from(extra_error.is_some()))
        .ok_or_else(finite_projection_error)?;
    let requested = (len as u128)
        .checked_mul(core::mem::size_of::<Diagnostic>() as u128)
        .ok_or_else(finite_projection_error)?;
    ledger.authorize_requested(requested)?;
    let mut diagnostics = Vec::new();
    diagnostics
        .try_reserve_exact(len)
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = diagnostics.capacity();
    ledger.retain_actual(
        (actual_capacity as u128)
            .checked_mul(core::mem::size_of::<Diagnostic>() as u128)
            .ok_or_else(finite_projection_error)?,
    )?;
    for diagnostic in validation {
        diagnostics.push(Diagnostic::new(
            try_owned_string(diagnostic.code().as_str(), ledger)?,
            try_debug_lowercase(diagnostic.severity(), ledger)?,
            try_owned_string(diagnostic.message(), ledger)?,
        ));
        if diagnostics.capacity() != actual_capacity {
            return Err(finite_projection_error());
        }
    }
    if let Some((code, message)) = extra_error {
        diagnostics.push(Diagnostic::new(
            try_owned_string(code, ledger)?,
            try_owned_string("error", ledger)?,
            try_owned_string(message, ledger)?,
        ));
    }
    if diagnostics.capacity() != actual_capacity {
        return Err(finite_projection_error());
    }
    Ok(diagnostics)
}

fn try_host_app_response(
    source: &clearra_app::AppResponse,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<HostAppResponse, WasmCommandRuntimeError> {
    let result_kind = preferred_result_kind(
        source.public_result_payload(),
        source
            .product_capability_result()
            .map(|result| result.result_kind().as_str()),
        source.result(),
    );
    let result = result_kind
        .map(|kind| try_owned_string(kind, ledger).map(HostAppResult::new))
        .transpose()?;
    let product_result_payload = try_host_product_result_payload(source, ledger)?;
    Ok(HostAppResponse::from_owned_memory_authorized_parts(
        try_product_identity(ledger)?,
        source.command(),
        try_host_status(source.status()),
        result,
        try_host_diagnostics(source, ledger)?,
        try_clone_backend_report(source.backend_report(), ledger)?,
        try_clone_resource_report(source.resource_report(), ledger)?,
        try_clone_capability_report(source.capability_report(), ledger)?,
        source
            .continuation()
            .map(|report| try_clone_continuation_report(report, ledger))
            .transpose()?,
    )
    .with_product_result_payload(product_result_payload))
}

fn preferred_result_kind<'a>(
    public_result_payload: Option<&'a ProductResultPayload>,
    product_capability_result_kind: Option<&'a str>,
    fallback: Option<&'a HostAppResult>,
) -> Option<&'a str> {
    public_result_payload
        .map(ProductResultPayload::result_kind)
        .or(product_capability_result_kind)
        .or_else(|| fallback.map(HostAppResult::kind))
}

fn try_host_product_result_payload(
    source: &clearra_app::AppResponse,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Option<ProductResultPayload>, WasmCommandRuntimeError> {
    if let Some(payload) = source.public_result_payload() {
        return try_clone_public_product_result_payload(payload, ledger).map(Some);
    }
    let Some(product) = source.product_capability_result() else {
        return Ok(None);
    };
    match (product.contract(), product.result_kind()) {
        (ProductCapabilityContract::PcMinimals, ProductCapabilityResultKind::PcMinimumCoverV2) => {
            let report = product
                .pc_minimum_cover_v2()
                .ok_or_else(finite_projection_error)?;
            let set = report.portfolio_alternatives();
            let page = set.canonical_page();
            let member_count = page.portfolio().candidate_ids().len();
            let member_end = member_count.min(PORTFOLIO_MEMBER_PAGE_SIZE);
            let (canonical_candidate_id, canonical_solution_key) = report
                .canonical_candidate()
                .ok_or_else(finite_projection_error)?;
            let canonical_witness = ProductCandidateMemberPayload::new(
                try_decimal_u128(canonical_candidate_id as u128, ledger)?,
                try_owned_string(canonical_solution_key, ledger)?,
            );
            let members = try_owned_vec(
                &page.portfolio().candidate_ids()[..member_end],
                ledger,
                |candidate_id, ledger| {
                    let index = candidate_id
                        .checked_sub(1)
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or_else(finite_projection_error)?;
                    let candidate = set
                        .candidates()
                        .get(index)
                        .filter(|candidate| candidate.candidate_id() == *candidate_id)
                        .ok_or_else(finite_projection_error)?;
                    Ok(ProductCandidateMemberPayload::new(
                        try_decimal_u128(*candidate_id as u128, ledger)?,
                        try_owned_string(candidate.normalized_key(), ledger)?,
                    ))
                },
            )?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::CoveragePortfolio(
                    CoveragePortfolioPagePayload::new(
                        try_owned_string(set.contract_id(), ledger)?,
                        try_owned_string(page.contract_id(), ledger)?,
                        try_owned_string(PORTFOLIO_MEMBER_PAGE_CONTRACT, ledger)?,
                        try_owned_string(set.set_identity_sha256(), ledger)?,
                        try_owned_string(set.candidate_map_sha256(), ledger)?,
                        try_owned_string(page.alternative_index_decimal(), ledger)?,
                        try_decimal_u128(page.optimal_cardinality() as u128, ledger)?,
                        try_owned_string(page.known_alternative_count_decimal(), ledger)?,
                        try_optional_owned_string(page.total_alternative_count_decimal(), ledger)?,
                        page.enumeration_complete(),
                        try_owned_string("1", ledger)?,
                        try_decimal_u128(
                            member_count.div_ceil(PORTFOLIO_MEMBER_PAGE_SIZE).max(1) as u128,
                            ledger,
                        )?,
                        members,
                        true,
                    )
                    .with_canonical_witness(
                        try_owned_string(report.canonical_selection(), ledger)?,
                        canonical_witness,
                    ),
                ),
            )))
        }
        (
            ProductCapabilityContract::PcScoreMinimals,
            ProductCapabilityResultKind::PcScorePortfolioV2,
        ) => {
            let report = product
                .pc_score_portfolio_v2()
                .ok_or_else(finite_projection_error)?;
            let set = report.portfolio_alternatives();
            let page = set.canonical_page();
            let member_count = page.portfolio().candidate_ids().len();
            let member_end = member_count.min(PORTFOLIO_MEMBER_PAGE_SIZE);
            let members = try_owned_vec(
                &page.portfolio().candidate_ids()[..member_end],
                ledger,
                |dense_candidate_id, ledger| {
                    let index = dense_candidate_id
                        .checked_sub(1)
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or_else(finite_projection_error)?;
                    let candidate = set
                        .candidates()
                        .get(index)
                        .filter(|candidate| candidate.candidate_id() == *dense_candidate_id)
                        .ok_or_else(finite_projection_error)?;
                    let public_candidate_id = set
                        .public_candidate_id(*dense_candidate_id)
                        .ok_or_else(finite_projection_error)?;
                    Ok(ProductCandidateMemberPayload::new(
                        try_decimal_u128(public_candidate_id as u128, ledger)?,
                        try_owned_string(candidate.normalized_key(), ledger)?,
                    ))
                },
            )?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::CoveragePortfolio(CoveragePortfolioPagePayload::new(
                    try_owned_string(set.contract_id(), ledger)?,
                    try_owned_string(page.contract_id(), ledger)?,
                    try_owned_string(PORTFOLIO_MEMBER_PAGE_CONTRACT, ledger)?,
                    try_owned_string(set.set_identity_sha256(), ledger)?,
                    try_owned_string(set.candidate_map_sha256(), ledger)?,
                    try_owned_string(page.alternative_index_decimal(), ledger)?,
                    try_decimal_u128(page.optimal_cardinality() as u128, ledger)?,
                    try_owned_string(page.known_alternative_count_decimal(), ledger)?,
                    try_optional_owned_string(page.total_alternative_count_decimal(), ledger)?,
                    page.enumeration_complete(),
                    try_owned_string("1", ledger)?,
                    try_decimal_u128(
                        member_count.div_ceil(PORTFOLIO_MEMBER_PAGE_SIZE).max(1) as u128,
                        ledger,
                    )?,
                    members,
                    true,
                )),
            )))
        }
        (ProductCapabilityContract::PcPath, ProductCapabilityResultKind::PcPathFamilyV2) => {
            let report = product
                .pc_path_family_v2()
                .ok_or_else(finite_projection_error)?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::PcPathFamily(try_pc_path_family_payload(
                    report, ledger,
                )?),
            )))
        }
        (ProductCapabilityContract::PcScore, ProductCapabilityResultKind::PcScoreSummaryV2) => {
            let report = product
                .pc_score_summary_v2()
                .ok_or_else(finite_projection_error)?;
            let fields =
                try_owned_vec(report.solution_field_averages(), ledger, |field, ledger| {
                    Ok(PcScoreFieldPayload::new(
                        try_pc_score_field_key(field.field_identity(), ledger)?,
                        try_display_string(field.average_score(), ledger)?,
                        try_decimal_u128(field.covered_pattern_count() as u128, ledger)?,
                        try_decimal_u128(field.pattern_count() as u128, ledger)?,
                        field.score_complete(),
                    ))
                })?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::PcScoreFieldSummary(PcScoreFieldSummaryPayload::new(
                    try_owned_string(PC_SCORE_SOLUTION_FIELD_CONTRACT, ledger)?,
                    try_owned_string(report.solution_field_ordering(), ledger)?,
                    try_owned_string(report.solution_field_average_basis(), ledger)?,
                    try_owned_string(report.score_evaluation_basis(), ledger)?,
                    try_owned_string(report.score_evaluation_scope(), ledger)?,
                    try_owned_string(report.overall_score_basis(), ledger)?,
                    try_decimal_u128(report.piece_source_id() as u128, ledger)?,
                    try_decimal_u128(report.pattern_universe_id() as u128, ledger)?,
                    try_decimal_u128(report.pattern_weight_model_id() as u128, ledger)?,
                    try_decimal_u128(report.materialized_pattern_count() as u128, ledger)?,
                    try_decimal_u128(report.solution_field_count() as u128, ledger)?,
                    try_decimal_u128(report.pattern_optimal_count() as u128, ledger)?,
                    try_decimal_u128(report.failed_pc_pattern_count() as u128, ledger)?,
                    try_owned_string(report.covered_probability(), ledger)?,
                    try_owned_string(report.overall_score(), ledger)?,
                    try_optional_owned_string(
                        report.covered_pattern_conditional_average_score(),
                        ledger,
                    )?,
                    report.completeness().complete(),
                    fields,
                )),
            )))
        }
        (
            ProductCapabilityContract::PcScoreFinder,
            ProductCapabilityResultKind::PcFixedScoreWitnessV2,
        ) => {
            let report = product
                .pc_score_summary_v2()
                .ok_or_else(finite_projection_error)?;
            let canonical_winner = report
                .canonical_winner()
                .ok_or_else(finite_projection_error)?;
            let canonical_solution_key = canonical_winner.normalized_solution_key();
            let canonical_winner_payload = ScorePatternWinnerPayload::new(
                try_decimal_u128(canonical_winner.pattern_id() as u128, ledger)?,
                try_decimal_u128(canonical_winner.candidate_id() as u128, ledger)?,
                try_owned_string(canonical_solution_key.as_str(), ledger)?,
                try_decimal_u128(canonical_winner.score() as u128, ledger)?,
                try_decimal_u128(canonical_winner.informational_attack() as u128, ledger)?,
            );
            let winners = try_owned_vec(report.pattern_winners(), ledger, |winner, ledger| {
                let normalized_key = winner.normalized_solution_key();
                Ok(ScorePatternWinnerPayload::new(
                    try_decimal_u128(winner.pattern_id() as u128, ledger)?,
                    try_decimal_u128(winner.candidate_id() as u128, ledger)?,
                    try_owned_string(normalized_key.as_str(), ledger)?,
                    try_decimal_u128(winner.score() as u128, ledger)?,
                    try_decimal_u128(winner.informational_attack() as u128, ledger)?,
                ))
            })?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::ScorePatternWinnerFamily(
                    ScorePatternWinnerFamilyPayload::new(
                        try_owned_string(PC_SCORE_PATTERN_WINNER_CONTRACT, ledger)?,
                        try_owned_string(
                            "pattern-id-ascending-then-candidate-id-ascending",
                            ledger,
                        )?,
                        try_owned_string("score-only-attack-informational", ledger)?,
                        try_owned_string(PC_SCORE_INFORMATIONAL_ATTACK_BASIS, ledger)?,
                        try_decimal_u128(PORTFOLIO_MEMBER_PAGE_SIZE as u128, ledger)?,
                        try_decimal_u128(report.pattern_winners().len() as u128, ledger)?,
                        try_owned_string(report.canonical_selection(), ledger)?,
                        canonical_winner_payload,
                        winners,
                    ),
                ),
            )))
        }
        (ProductCapabilityContract::PcSaves, ProductCapabilityResultKind::PcSaveGroupsV2) => {
            let report = product
                .pc_save_groups_v2()
                .ok_or_else(finite_projection_error)?;
            let groups = try_owned_vec(report.groups(), ledger, try_pc_save_group_payload)?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::PcSaveGroups(PcSaveGroupsPayload::new(
                    try_owned_string(PC_BEST_SAVE_SCHEMA, ledger)?,
                    try_decimal_u128(PORTFOLIO_MEMBER_PAGE_SIZE as u128, ledger)?,
                    try_decimal_u128(report.groups().len() as u128, ledger)?,
                    try_pc_save_run_metadata(
                        report.origin().as_str(),
                        report.problem_preset().as_str(),
                        report.problem_id(),
                        report.piece_source_id(),
                        report.pattern_universe_id(),
                        report.pattern_weight_model_id(),
                        report.materialized_pattern_count(),
                        report.pc_success_pattern_count(),
                        report.pc_probability().decimal(),
                        report.completeness(),
                        ledger,
                    )?,
                    groups,
                )),
            )))
        }
        (ProductCapabilityContract::PcBestSave, ProductCapabilityResultKind::PcBestSaveV2) => {
            let report = product
                .pc_best_save_v2()
                .ok_or_else(finite_projection_error)?;
            let winners = try_owned_vec(report.winners(), ledger, |winner, ledger| {
                try_pc_best_save_winner_payload(winner, ledger)
            })?;
            let canonical_winner = report
                .canonical_winner()
                .map(|winner| try_pc_best_save_winner_payload(winner, ledger))
                .transpose()?;
            Ok(Some(ProductResultPayload::new(
                try_owned_string(product.contract().as_str(), ledger)?,
                try_owned_string(product.result_kind().as_str(), ledger)?,
                ProductResultPayloadContent::PcBestSave(PcBestSavePayload::new(
                    try_owned_string(report.schema_id(), ledger)?,
                    try_owned_string(report.probability_basis(), ledger)?,
                    try_owned_string(
                        "weighted-total-descending-then-balanced-jl-descending-then-unconditional-probability-descending-then-canonical-candidate-id-ascending",
                        ledger,
                    )?,
                    try_owned_string(
                        "weighted-total-balanced-jl-and-exact-unconditional-probability",
                        ledger,
                    )?,
                    try_decimal_u128(PORTFOLIO_MEMBER_PAGE_SIZE as u128, ledger)?,
                    try_decimal_u128(report.winners().len() as u128, ledger)?,
                    try_owned_string(report.canonical_selection(), ledger)?,
                    canonical_winner,
                    try_pc_save_run_metadata(
                        report.origin().as_str(),
                        report.problem_preset().as_str(),
                        report.problem_id(),
                        report.piece_source_id(),
                        report.pattern_universe_id(),
                        report.pattern_weight_model_id(),
                        report.materialized_pattern_count(),
                        report.pc_success_pattern_count(),
                        report.pc_probability().decimal(),
                        report.completeness(),
                        ledger,
                    )?,
                    winners,
                )),
            )))
        }
        _ => Ok(None),
    }
}

fn try_clone_public_product_result_payload(
    source: &ProductResultPayload,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ProductResultPayload, WasmCommandRuntimeError> {
    let contract = try_owned_string(source.contract(), ledger)?;
    let result_kind = try_owned_string(source.result_kind(), ledger)?;
    let content = match source.content() {
        ProductResultPayloadContent::CoveragePortfolio(payload) => {
            let members = try_owned_vec(payload.members(), ledger, |member, ledger| {
                Ok(ProductCandidateMemberPayload::new(
                    try_owned_string(member.candidate_id(), ledger)?,
                    try_owned_string(member.normalized_solution_key(), ledger)?,
                ))
            })?;
            let cloned = CoveragePortfolioPagePayload::new(
                try_owned_string(payload.set_contract(), ledger)?,
                try_owned_string(payload.page_contract(), ledger)?,
                try_owned_string(payload.member_page_contract(), ledger)?,
                try_owned_string(payload.set_identity_sha256(), ledger)?,
                try_owned_string(payload.candidate_map_sha256(), ledger)?,
                try_owned_string(payload.alternative_index(), ledger)?,
                try_owned_string(payload.optimal_cardinality(), ledger)?,
                try_owned_string(payload.known_alternative_count(), ledger)?,
                try_optional_owned_string(payload.total_alternative_count(), ledger)?,
                payload.enumeration_complete(),
                try_owned_string(payload.member_page_number(), ledger)?,
                try_owned_string(payload.total_member_pages(), ledger)?,
                members,
                payload.page_handle_available(),
            );
            let cloned = match (payload.canonical_selection(), payload.canonical_witness()) {
                (Some(selection), Some(witness)) => cloned.with_canonical_witness(
                    try_owned_string(selection, ledger)?,
                    ProductCandidateMemberPayload::new(
                        try_owned_string(witness.candidate_id(), ledger)?,
                        try_owned_string(witness.normalized_solution_key(), ledger)?,
                    ),
                ),
                (None, None) => cloned,
                _ => return Err(finite_projection_error()),
            };
            ProductResultPayloadContent::CoveragePortfolio(cloned)
        }
        ProductResultPayloadContent::BuildV2(payload) => ProductResultPayloadContent::BuildV2(
            try_clone_build_v2_product_payload(payload, ledger)?,
        ),
        ProductResultPayloadContent::SetupRankedFamily(payload) => {
            let candidates = try_owned_vec(payload.candidates(), ledger, |candidate, ledger| {
                SetupRankedCandidatePayload::try_new(
                    try_owned_string(candidate.candidate_id(), ledger)?,
                    try_owned_string(candidate.condition_id(), ledger)?,
                    try_owned_string(candidate.setup_id(), ledger)?,
                )
                .map_err(|_| finite_projection_error())
            })?;
            ProductResultPayloadContent::SetupRankedFamily(
                SetupRankedFamilyPayload::try_new(
                    try_owned_string(payload.schema_id(), ledger)?,
                    try_owned_string(payload.query_identity_sha256(), ledger)?,
                    try_owned_string(payload.rule_profile(), ledger)?,
                    try_owned_string(payload.supply_identity_sha256(), ledger)?,
                    try_owned_string(payload.universe_identity_sha256(), ledger)?,
                    try_owned_string(payload.product_build(), ledger)?,
                    try_owned_string(payload.ordering(), ledger)?,
                    try_owned_string(payload.resolved_length_preference(), ledger)?,
                    try_owned_string(payload.candidate_count(), ledger)?,
                    candidates,
                )
                .map_err(|_| finite_projection_error())?,
            )
        }
        ProductResultPayloadContent::SetupScoreRanking(payload) => {
            let candidates = try_owned_vec(payload.candidates(), ledger, |candidate, ledger| {
                SetupScoreCandidatePayload::try_new(
                    try_owned_string(candidate.rank(), ledger)?,
                    try_owned_string(candidate.candidate_id(), ledger)?,
                    try_owned_string(candidate.completed_board_mask(), ledger)?,
                    try_owned_string(candidate.setup_covered_pattern_count(), ledger)?,
                    try_owned_string(candidate.setup_covered_probability(), ledger)?,
                    try_owned_string(candidate.continuation_probability(), ledger)?,
                    try_owned_string(candidate.unconditional_expected_score(), ledger)?,
                )
                .map_err(|_| finite_projection_error())
            })?;
            ProductResultPayloadContent::SetupScoreRanking(
                SetupScoreRankingPayload::try_new(
                    try_owned_string(payload.schema_id(), ledger)?,
                    try_owned_string(payload.input_identity_sha256(), ledger)?,
                    try_owned_string(payload.evaluation_identity_sha256(), ledger)?,
                    try_owned_string(payload.document_format(), ledger)?,
                    try_owned_string(payload.rule_profile(), ledger)?,
                    try_owned_string(payload.score_profile(), ledger)?,
                    try_owned_string(payload.initial_b2b(), ledger)?,
                    try_owned_string(payload.ordering(), ledger)?,
                    try_owned_string(payload.source_page_count(), ledger)?,
                    try_owned_string(payload.candidate_count(), ledger)?,
                    try_owned_string(payload.setup_pattern_count(), ledger)?,
                    try_owned_string(payload.average_priority_score(), ledger)?,
                    payload.complete(),
                    candidates,
                )
                .map_err(|_| finite_projection_error())?,
            )
        }
        ProductResultPayloadContent::SpinStructureFamily(payload) => {
            let candidates = try_owned_vec(payload.candidates(), ledger, |candidate, ledger| {
                SpinStructureCandidatePayload::try_new(
                    try_owned_string(candidate.candidate_id(), ledger)?,
                    try_owned_string(candidate.partition(), ledger)?,
                    try_owned_string(candidate.placement_count(), ledger)?,
                )
                .map_err(|_| finite_projection_error())
            })?;
            ProductResultPayloadContent::SpinStructureFamily(
                SpinStructureFamilyPayload::try_new(
                    try_owned_string(payload.schema_id(), ledger)?,
                    try_owned_string(payload.query_identity_sha256(), ledger)?,
                    try_owned_string(payload.rule_profile(), ledger)?,
                    try_owned_string(payload.spin_profile(), ledger)?,
                    try_owned_string(payload.supply_identity_sha256(), ledger)?,
                    try_owned_string(payload.universe_identity_sha256(), ledger)?,
                    try_owned_string(payload.product_build(), ledger)?,
                    try_owned_string(payload.ordering(), ledger)?,
                    try_optional_owned_string(payload.minimum_placements(), ledger)?,
                    try_optional_owned_string(payload.guaranteed_final_piece(), ledger)?,
                    try_optional_owned_string(payload.guarantee_basis(), ledger)?,
                    payload.dependency_report_included(),
                    try_optional_owned_string(payload.dependency_relation(), ledger)?,
                    try_optional_owned_string(payload.dependency_edge_count(), ledger)?,
                    try_owned_string(payload.regular_count(), ledger)?,
                    try_owned_string(payload.mini_count(), ledger)?,
                    try_owned_string(payload.candidate_count(), ledger)?,
                    payload.complete(),
                    candidates,
                )
                .map_err(|_| finite_projection_error())?,
            )
        }
        ProductResultPayloadContent::PcScoreFieldSummary(payload) => {
            let fields = try_owned_vec(payload.fields(), ledger, |field, ledger| {
                Ok(PcScoreFieldPayload::new(
                    try_owned_string(field.normalized_field_key(), ledger)?,
                    try_owned_string(field.average_score(), ledger)?,
                    try_owned_string(field.covered_pattern_count(), ledger)?,
                    try_owned_string(field.pattern_count(), ledger)?,
                    field.score_complete(),
                ))
            })?;
            ProductResultPayloadContent::PcScoreFieldSummary(PcScoreFieldSummaryPayload::new(
                try_owned_string(payload.field_contract(), ledger)?,
                try_owned_string(payload.ordering(), ledger)?,
                try_owned_string(payload.solution_field_average_basis(), ledger)?,
                try_owned_string(payload.score_evaluation_basis(), ledger)?,
                try_owned_string(payload.score_evaluation_scope(), ledger)?,
                try_owned_string(payload.overall_score_basis(), ledger)?,
                try_owned_string(payload.piece_source_id(), ledger)?,
                try_owned_string(payload.pattern_universe_id(), ledger)?,
                try_owned_string(payload.pattern_weight_model_id(), ledger)?,
                try_owned_string(payload.materialized_pattern_count(), ledger)?,
                try_owned_string(payload.solution_field_count(), ledger)?,
                try_owned_string(payload.scored_pattern_count(), ledger)?,
                try_owned_string(payload.failed_pc_pattern_count(), ledger)?,
                try_owned_string(payload.covered_probability(), ledger)?,
                try_owned_string(payload.overall_score(), ledger)?,
                try_optional_owned_string(
                    payload.score_covered_pattern_conditional_average_score(),
                    ledger,
                )?,
                payload.complete(),
                fields,
            ))
        }
        ProductResultPayloadContent::ScorePatternWinnerFamily(payload) => {
            let canonical_winner = payload.canonical_winner();
            let canonical_winner = ScorePatternWinnerPayload::new(
                try_owned_string(canonical_winner.pattern_id(), ledger)?,
                try_owned_string(canonical_winner.candidate_id(), ledger)?,
                try_owned_string(canonical_winner.normalized_solution_key(), ledger)?,
                try_owned_string(canonical_winner.score(), ledger)?,
                try_owned_string(canonical_winner.informational_attack(), ledger)?,
            );
            let winners = try_owned_vec(payload.winners(), ledger, |winner, ledger| {
                Ok(ScorePatternWinnerPayload::new(
                    try_owned_string(winner.pattern_id(), ledger)?,
                    try_owned_string(winner.candidate_id(), ledger)?,
                    try_owned_string(winner.normalized_solution_key(), ledger)?,
                    try_owned_string(winner.score(), ledger)?,
                    try_owned_string(winner.informational_attack(), ledger)?,
                ))
            })?;
            ProductResultPayloadContent::ScorePatternWinnerFamily(
                ScorePatternWinnerFamilyPayload::new(
                    try_owned_string(payload.winner_contract(), ledger)?,
                    try_owned_string(payload.ordering(), ledger)?,
                    try_owned_string(payload.equality(), ledger)?,
                    try_owned_string(payload.informational_attack_basis(), ledger)?,
                    try_owned_string(payload.page_size(), ledger)?,
                    try_owned_string(payload.winner_count(), ledger)?,
                    try_owned_string(payload.canonical_selection(), ledger)?,
                    canonical_winner,
                    winners,
                ),
            )
        }
        ProductResultPayloadContent::PcPathFamily(payload) => {
            ProductResultPayloadContent::PcPathFamily(try_clone_pc_path_family_payload(
                payload, ledger,
            )?)
        }
        _ => return Err(finite_projection_error()),
    };
    Ok(ProductResultPayload::from_owned_memory_authorized_parts(
        contract,
        result_kind,
        content,
    ))
}

fn try_clone_build_v2_product_payload(
    source: &BuildV2ProductPayload,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<BuildV2ProductPayload, WasmCommandRuntimeError> {
    let capability_id = try_owned_string(source.capability_id(), ledger)?;
    let result_contract = try_owned_string(source.result_contract(), ledger)?;
    let input_identity_sha256 = try_owned_string(source.input_identity_sha256(), ledger)?;
    let evaluation_identity_sha256 =
        try_optional_owned_string(source.evaluation_identity_sha256(), ledger)?;
    let replay_basis = try_optional_owned_string(source.replay_basis(), ledger)?;
    let objective = try_owned_string(source.objective(), ledger)?;
    let score_profile = try_optional_owned_string(source.score_profile(), ledger)?;
    let initial_b2b = try_optional_owned_string(source.initial_b2b(), ledger)?;
    let score_accuracy = try_optional_owned_string(source.score_accuracy(), ledger)?;
    let score_equality_basis = try_optional_owned_string(source.score_equality_basis(), ledger)?;
    let informational_attack_basis =
        try_optional_owned_string(source.informational_attack_basis(), ledger)?;
    let source_candidate_count = try_owned_string(source.source_candidate_count(), ledger)?;
    let reachable_candidate_count = try_owned_string(source.reachable_candidate_count(), ledger)?;
    let selected_candidate_count =
        try_optional_owned_string(source.selected_candidate_count(), ledger)?;
    let pattern_count = try_owned_string(source.pattern_count(), ledger)?;
    let covered_pattern_count = try_optional_owned_string(source.covered_pattern_count(), ledger)?;
    let required_pattern_count =
        try_optional_owned_string(source.required_pattern_count(), ledger)?;
    let union_probability = try_optional_owned_string(source.union_probability(), ledger)?;
    let candidates = try_owned_vec(source.candidates(), ledger, |row, ledger| {
        BuildV2CandidateCoveragePayload::try_from_owned_memory_authorized_parts(
            try_owned_string(row.candidate_key(), ledger)?,
            try_owned_string(row.covered_pattern_count(), ledger)?,
        )
        .map_err(|_| finite_projection_error())
    })?;
    let canonical_candidate_keys = try_owned_string_vec(source.canonical_candidate_keys(), ledger)?;
    let winners = try_owned_vec(source.winners(), ledger, |winner, ledger| {
        BuildV2ScoreWinnerPayload::try_from_owned_memory_authorized_parts(
            try_owned_string(winner.pattern_id(), ledger)?,
            try_owned_string(winner.candidate_key(), ledger)?,
            try_owned_string(winner.score(), ledger)?,
            try_owned_string(winner.informational_attack(), ledger)?,
        )
        .map_err(|_| finite_projection_error())
    })?;
    let page_source_identity_sha256 =
        try_optional_owned_string(source.page_source_identity_sha256(), ledger)?;

    BuildV2ProductPayload::try_from_owned_memory_authorized_parts(
        source.kind(),
        capability_id,
        result_contract,
        input_identity_sha256,
        evaluation_identity_sha256,
        replay_basis,
        objective,
        score_profile,
        initial_b2b,
        score_accuracy,
        source.profile_specific_exact(),
        score_equality_basis,
        informational_attack_basis,
        source_candidate_count,
        reachable_candidate_count,
        selected_candidate_count,
        pattern_count,
        covered_pattern_count,
        required_pattern_count,
        union_probability,
        source.b2b_preservation_required(),
        candidates,
        canonical_candidate_keys,
        winners,
        source.completeness(),
        source.page_source_available(),
        page_source_identity_sha256,
    )
    .map_err(|_| finite_projection_error())
}

fn try_pc_path_family_payload(
    report: &PcPathFamilyV2Result,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcPathFamilyPayload, WasmCommandRuntimeError> {
    let witnesses = try_owned_vec(report.witnesses(), ledger, try_pc_path_witness_payload)?;
    let canonical_witness = report
        .canonical_witness()
        .map(|witness| try_pc_path_witness_payload(witness, ledger))
        .transpose()?;
    Ok(PcPathFamilyPayload::new(
        try_owned_string(report.witness_contract(), ledger)?,
        try_owned_string(report.ordering(), ledger)?,
        try_owned_string(report.problem_id(), ledger)?,
        try_decimal_u128(report.materialized_pattern_count() as u128, ledger)?,
        try_decimal_u128(report.witnesses().len() as u128, ledger)?,
        report.completeness().complete(),
        try_owned_string(report.canonical_selection(), ledger)?,
        canonical_witness,
        witnesses,
    ))
}

fn try_pc_path_witness_payload(
    witness: &PcPathWitnessV2,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcPathWitnessPayload, WasmCommandRuntimeError> {
    let steps = try_owned_vec(witness.steps(), ledger, |step, ledger| {
        Ok(PcPathStepPayload::new(
            try_decimal_u128(step.step_index() as u128, ledger)?,
            try_decimal_u128(step.operation_id() as u128, ledger)?,
            try_owned_string(piece_name(step.active_piece()), ledger)?,
            try_decimal_u128(step.input_cursor() as u128, ledger)?,
            try_decimal_u128(step.output_cursor() as u128, ledger)?,
            step.input_hold_piece()
                .map(|piece| try_owned_string(piece_name(piece), ledger))
                .transpose()?,
            step.output_hold_piece()
                .map(|piece| try_owned_string(piece_name(piece), ledger))
                .transpose()?,
            try_owned_string(step.hold_decision(), ledger)?,
            try_decimal_u128(step.rotation() as u128, ledger)?,
            try_decimal_u128(step.x() as u128, ledger)?,
            try_decimal_u128(step.y() as u128, ledger)?,
            try_hex_mask(step.placement_mask(), ledger)?,
            try_hex_mask(step.board_before_mask(), ledger)?,
            try_hex_mask(step.board_after_placement_mask(), ledger)?,
            try_hex_mask(step.board_after_line_clear_mask(), ledger)?,
            try_hex_mask(step.cleared_row_mask(), ledger)?,
            try_decimal_u128(step.cleared_lines() as u128, ledger)?,
            try_owned_string(step.line_clear_identity(), ledger)?,
        ))
    })?;
    Ok(PcPathWitnessPayload::new(
        try_decimal_u128(witness.candidate_id() as u128, ledger)?,
        try_decimal_u128(witness.producer_candidate_id() as u128, ledger)?,
        try_decimal_u128(witness.pattern_id() as u128, ledger)?,
        try_owned_string(witness.trace_identity(), ledger)?,
        try_owned_string(witness.normalized_trace_key(), ledger)?,
        try_decimal_u128(witness.consumed_piece_count() as u128, ledger)?,
        witness
            .terminal_hold_piece()
            .map(|piece| try_owned_string(piece_name(piece), ledger))
            .transpose()?,
        steps,
    ))
}

fn try_clone_pc_path_family_payload(
    source: &PcPathFamilyPayload,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcPathFamilyPayload, WasmCommandRuntimeError> {
    if source.canonical_selection() != PC_PATH_CANONICAL_SELECTION {
        return Err(finite_projection_error());
    }
    let canonical_witness = match (source.canonical_witness(), source.witnesses().first()) {
        (Some(canonical), Some(first)) if canonical == first => {
            Some(try_clone_pc_path_witness_payload(canonical, ledger)?)
        }
        (None, None) => None,
        _ => return Err(finite_projection_error()),
    };
    let witnesses = try_owned_vec(
        source.witnesses(),
        ledger,
        try_clone_pc_path_witness_payload,
    )?;
    Ok(PcPathFamilyPayload::new(
        try_owned_string(source.witness_contract(), ledger)?,
        try_owned_string(source.ordering(), ledger)?,
        try_owned_string(source.problem_id(), ledger)?,
        try_owned_string(source.materialized_pattern_count(), ledger)?,
        try_owned_string(source.witness_count(), ledger)?,
        source.complete(),
        try_owned_string(source.canonical_selection(), ledger)?,
        canonical_witness,
        witnesses,
    ))
}

fn try_clone_pc_path_witness_payload(
    witness: &PcPathWitnessPayload,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcPathWitnessPayload, WasmCommandRuntimeError> {
    let steps = try_owned_vec(witness.steps(), ledger, |step, ledger| {
        Ok(PcPathStepPayload::new(
            try_owned_string(step.step_index(), ledger)?,
            try_owned_string(step.operation_id(), ledger)?,
            try_owned_string(step.active_piece(), ledger)?,
            try_owned_string(step.input_cursor(), ledger)?,
            try_owned_string(step.output_cursor(), ledger)?,
            try_optional_owned_string(step.input_hold_piece(), ledger)?,
            try_optional_owned_string(step.output_hold_piece(), ledger)?,
            try_owned_string(step.hold_decision(), ledger)?,
            try_owned_string(step.rotation(), ledger)?,
            try_owned_string(step.x(), ledger)?,
            try_owned_string(step.y(), ledger)?,
            try_owned_string(step.placement_mask(), ledger)?,
            try_owned_string(step.board_before_mask(), ledger)?,
            try_owned_string(step.board_after_placement_mask(), ledger)?,
            try_owned_string(step.board_after_line_clear_mask(), ledger)?,
            try_owned_string(step.cleared_row_mask(), ledger)?,
            try_owned_string(step.cleared_lines(), ledger)?,
            try_owned_string(step.line_clear_identity(), ledger)?,
        ))
    })?;
    Ok(PcPathWitnessPayload::new(
        try_owned_string(witness.candidate_id(), ledger)?,
        try_owned_string(witness.producer_candidate_id(), ledger)?,
        try_owned_string(witness.pattern_id(), ledger)?,
        try_owned_string(witness.trace_identity(), ledger)?,
        try_owned_string(witness.normalized_trace_key(), ledger)?,
        try_owned_string(witness.consumed_piece_count(), ledger)?,
        try_optional_owned_string(witness.terminal_hold_piece(), ledger)?,
        steps,
    ))
}

#[allow(clippy::too_many_arguments)]
fn try_pc_save_run_metadata(
    origin: &str,
    problem_preset: &str,
    problem_id: &str,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    materialized_pattern_count: usize,
    pc_success_pattern_count: usize,
    pc_probability: &str,
    completeness: PcSaveCompletenessEvidence,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcSaveRunMetadataPayload, WasmCommandRuntimeError> {
    Ok(PcSaveRunMetadataPayload::new(
        try_owned_string(origin, ledger)?,
        try_owned_string(problem_preset, ledger)?,
        try_owned_string(problem_id, ledger)?,
        try_decimal_u128(piece_source_id as u128, ledger)?,
        try_decimal_u128(pattern_universe_id as u128, ledger)?,
        try_decimal_u128(pattern_weight_model_id as u128, ledger)?,
        try_decimal_u128(materialized_pattern_count as u128, ledger)?,
        try_decimal_u128(pc_success_pattern_count as u128, ledger)?,
        try_owned_string(pc_probability, ledger)?,
        PcSaveCompletenessPayload::new(
            completeness.source_universe_complete(),
            completeness.fixed_bag_boundary_proven(),
            completeness.execution_batch_complete(),
            completeness.pattern_weights_complete(),
            completeness.count_complete(),
            completeness.probability_complete(),
            completeness.complete(),
        ),
    ))
}

fn try_pc_save_group_payload(
    group: &PcSaveGroupV2,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcSaveGroupPayload, WasmCommandRuntimeError> {
    Ok(PcSaveGroupPayload::new(
        try_owned_string(group.identity_contract(), ledger)?,
        try_pc_save_piece_multiset_payload(group.identity(), ledger)?,
        try_decimal_u128(group.successful_pattern_count() as u128, ledger)?,
        try_owned_string(group.unconditional_probability().decimal(), ledger)?,
        try_owned_string(group.conditional_probability_given_pc().decimal(), ledger)?,
        try_decimal_u128(group.canonical_candidate_id() as u128, ledger)?,
        try_owned_vec(group.witnesses(), ledger, try_pc_save_witness_payload)?,
    ))
}

fn try_pc_best_save_winner_payload(
    winner: &PcBestSaveWinnerV2,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcBestSaveWinnerPayload, WasmCommandRuntimeError> {
    Ok(PcBestSaveWinnerPayload::new(
        try_decimal_u128(winner.weighted_total() as u128, ledger)?,
        try_decimal_u128(winner.balanced_jl_count() as u128, ledger)?,
        try_owned_string(winner.exact_group_probability().decimal(), ledger)?,
        try_pc_save_group_payload(winner.group(), ledger)?,
    ))
}

fn try_pc_save_witness_payload(
    witness: &PcSaveWitness,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcSaveWitnessPayload, WasmCommandRuntimeError> {
    Ok(PcSaveWitnessPayload::new(
        try_decimal_u128(witness.pattern_index() as u128, ledger)?,
        try_decimal_u128(witness.candidate_id() as u128, ledger)?,
        try_owned_string(witness.trace_identity(), ledger)?,
        try_decimal_u128(witness.source_cursor() as u128, ledger)?,
        witness
            .terminal_hold()
            .map(|piece| try_owned_string(piece_name(piece), ledger))
            .transpose()?,
        try_pc_save_piece_multiset_payload(witness.active_bag_remainder(), ledger)?,
    ))
}

fn try_pc_save_piece_multiset_payload(
    multiset: &PcSavePieceMultiset,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<PcSavePieceMultisetPayload, WasmCommandRuntimeError> {
    Ok(PcSavePieceMultisetPayload::new(
        try_owned_string(multiset.canonical_id(), ledger)?,
        multiset.count(PieceKind::T),
        multiset.count(PieceKind::I),
        multiset.count(PieceKind::O),
        multiset.count(PieceKind::J),
        multiset.count(PieceKind::L),
        multiset.count(PieceKind::S),
        multiset.count(PieceKind::Z),
        multiset.total_count(),
    ))
}

const fn piece_name(piece: PieceKind) -> &'static str {
    match piece {
        PieceKind::I => "I",
        PieceKind::O => "O",
        PieceKind::T => "T",
        PieceKind::S => "S",
        PieceKind::Z => "Z",
        PieceKind::J => "J",
        PieceKind::L => "L",
    }
}

// A normalized Board64 key has a documented maximum of 362 bytes and a
// finite-f64 Display form can require more than 300 decimal bytes. Format both
// on the stack so the WASM ledger authorizes the only retained allocation.
const INLINE_PROJECTION_TEXT_CAPACITY: usize = 384;

struct InlineProjectionText {
    bytes: [u8; INLINE_PROJECTION_TEXT_CAPACITY],
    len: usize,
}

impl InlineProjectionText {
    const fn new() -> Self {
        Self {
            bytes: [0; INLINE_PROJECTION_TEXT_CAPACITY],
            len: 0,
        }
    }

    fn as_str(&self) -> Result<&str, WasmCommandRuntimeError> {
        core::str::from_utf8(&self.bytes[..self.len]).map_err(|_| finite_projection_error())
    }
}

impl fmt::Write for InlineProjectionText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let target = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn try_display_string(
    value: impl fmt::Display,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    use fmt::Write;

    let mut text = InlineProjectionText::new();
    write!(&mut text, "{value}").map_err(|_| finite_projection_error())?;
    try_owned_string(text.as_str()?, ledger)
}

fn try_pc_score_field_key(
    identity: StandardBoard64TilingIdentity,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    let mut text = InlineProjectionText::new();
    identity
        .write_canonical(&mut text)
        .map_err(|_| finite_projection_error())?;
    try_owned_string(text.as_str()?, ledger)
}

fn try_decimal_u128(
    mut value: u128,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    let mut digits = [0_u8; 39];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let decimal = core::str::from_utf8(&digits[start..]).map_err(|_| finite_projection_error())?;
    try_owned_string(decimal, ledger)
}

fn try_hex_mask(
    value: u64,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [b'0'; 18];
    bytes[1] = b'x';
    for index in 0..16 {
        let shift = (15 - index) * 4;
        bytes[index + 2] = HEX[((value >> shift) & 0x0f) as usize];
    }
    let value = core::str::from_utf8(&bytes).map_err(|_| finite_projection_error())?;
    try_owned_string(value, ledger)
}

fn try_join_three(
    first: &str,
    middle: &str,
    last: &str,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    let len = first
        .len()
        .checked_add(middle.len())
        .and_then(|len| len.checked_add(last.len()))
        .ok_or_else(finite_projection_error)?;
    ledger.authorize_requested(len as u128)?;
    let mut output = String::new();
    output
        .try_reserve_exact(len)
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = output.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    if actual_capacity < len {
        return Err(finite_projection_error());
    }
    output.push_str(first);
    output.push_str(middle);
    output.push_str(last);
    if output.len() != len || output.capacity() != actual_capacity {
        return Err(finite_projection_error());
    }
    Ok(output)
}

fn try_unavailable_webgpu_report(
    reason: Option<String>,
    fallback_to_wasm_cpu: bool,
    shader_status: &str,
    outcome_state: WebGpuBackendOutcomeState,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<WebGpuBackendReport, WasmCommandRuntimeError> {
    Ok(WebGpuBackendReport {
        outcome_state,
        webgpu_available: false,
        webgpu_adapter_label_or_redacted: try_owned_string("redacted", ledger)?,
        webgpu_limits: crate::WebGpuLimitsReport::default(),
        webgpu_required_limits: crate::WebGpuLimitsReport::default(),
        webgpu_unavailable_reason: reason,
        expected_digest: None,
        actual_digest: None,
        shader: crate::WebGpuShaderReport {
            shader_compile_status: try_owned_string(shader_status, ledger)?,
            shader_hash: None,
            shader_version: None,
            embedded_reviewed: false,
            user_shader_allowed: false,
            runtime_shader_injection_allowed: false,
        },
        memory: crate::WebGpuMemoryReport {
            wasm_memory_usage: try_owned_string("not-reported-backend-unavailable", ledger)?,
            wasm_memory_pressure: try_owned_string("not-reported-backend-unavailable", ledger)?,
        },
        fallback_used: fallback_to_wasm_cpu,
        fallback_backend: fallback_to_wasm_cpu
            .then(|| try_owned_string("wasm-cpu", ledger))
            .transpose()?,
        gpu_warmup_requested: false,
        gpu_warmup_performed: false,
        gpu_session_reused: false,
        gpu_trust_state: if outcome_state == WebGpuBackendOutcomeState::Unavailable {
            crate::WebGpuReportTrustState::Unavailable
        } else {
            crate::WebGpuReportTrustState::NotUsed
        },
        cpu_confirmed: false,
        can_source_exact_probability: false,
    })
}

fn try_webgpu_report(
    response: &clearra_app::AppResponse,
    requested: bool,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<WebGpuBackendReport, WasmCommandRuntimeError> {
    if !requested {
        return try_unavailable_webgpu_report(
            None,
            false,
            "not-requested",
            WebGpuBackendOutcomeState::NotRequested,
            ledger,
        );
    }
    let Some(result) = response
        .render_model()
        .and_then(|model| model.core_result())
    else {
        let backend = response.backend_report();
        let reason = match (
            backend.backend_fallback_reason(),
            backend.gpu_failure_class(),
            backend.gpu_failure_stage(),
        ) {
            (Some(reason), _, _) => try_owned_string(reason, ledger)?,
            (None, Some(class), Some(stage)) => try_join_three(class, ":", stage, ledger)?,
            (None, Some(class), None) => try_owned_string(class, ledger)?,
            (None, None, Some(stage)) => try_owned_string(stage, ledger)?,
            (None, None, None) => try_owned_string("webgpu_result_not_materialized", ledger)?,
        };
        return try_unavailable_webgpu_report(
            Some(reason),
            backend.fallback_used(),
            "search-backend-unavailable",
            WebGpuBackendOutcomeState::Unavailable,
            ledger,
        );
    };
    if result.field("backend_selected") == Some("webgpu") {
        return Ok(WebGpuBackendReport {
            outcome_state: WebGpuBackendOutcomeState::Connected,
            webgpu_available: true,
            webgpu_adapter_label_or_redacted: try_owned_string(
                result.field("gpu_adapter").unwrap_or("redacted"),
                ledger,
            )?,
            webgpu_limits: crate::WebGpuLimitsReport::default(),
            webgpu_required_limits: crate::WebGpuLimitsReport::default(),
            webgpu_unavailable_reason: None,
            expected_digest: try_optional_owned_string(
                result.field("packing_candidate_set_digest"),
                ledger,
            )?,
            actual_digest: try_optional_owned_string(
                result.field("packing_candidate_set_digest"),
                ledger,
            )?,
            shader: crate::WebGpuShaderReport {
                shader_compile_status: try_owned_string("connected", ledger)?,
                shader_hash: try_optional_owned_string(result.field("gpu_shader_hash"), ledger)?,
                shader_version: try_optional_owned_string(
                    result.field("gpu_shader_version"),
                    ledger,
                )?,
                embedded_reviewed: true,
                user_shader_allowed: false,
                runtime_shader_injection_allowed: false,
            },
            memory: crate::WebGpuMemoryReport {
                wasm_memory_usage: try_owned_string(
                    result.field("gpu_peak_bytes").unwrap_or("0"),
                    ledger,
                )?,
                wasm_memory_pressure: try_owned_string("within-reported-budget", ledger)?,
            },
            fallback_used: false,
            fallback_backend: None,
            gpu_warmup_requested: result.bool_field("gpu_warmup_requested").unwrap_or(false),
            gpu_warmup_performed: result.bool_field("gpu_warmup_performed").unwrap_or(false),
            gpu_session_reused: result.bool_field("gpu_session_reused").unwrap_or(false),
            gpu_trust_state: crate::WebGpuReportTrustState::TrustedCpuSampleConfirmed,
            cpu_confirmed: true,
            can_source_exact_probability: result
                .bool_field("probability_complete")
                .unwrap_or(false),
        });
    }
    let fallback_used = result.bool_field("backend_fallback_used").unwrap_or(false);
    let reason = result
        .field("gpu_disabled_reason")
        .filter(|reason| !matches!(*reason, "none" | "not_requested"))
        .or_else(|| {
            result
                .field("backend_fallback_reason")
                .filter(|reason| *reason != "none")
        })
        .unwrap_or("webgpu_not_selected");
    let reason = try_owned_string(reason, ledger)?;
    let mut report = try_unavailable_webgpu_report(
        Some(reason),
        fallback_used,
        "search-backend-unavailable",
        WebGpuBackendOutcomeState::Unavailable,
        ledger,
    )?;
    report.gpu_warmup_requested = result.bool_field("gpu_warmup_requested").unwrap_or(false);
    report.gpu_warmup_performed = result.bool_field("gpu_warmup_performed").unwrap_or(false);
    report.gpu_session_reused = result.bool_field("gpu_session_reused").unwrap_or(false);
    Ok(report)
}

fn try_canonical_probability(
    value: Option<&str>,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<String, WasmCommandRuntimeError> {
    let value = value.unwrap_or("0");
    let canonical = match value.parse::<f64>() {
        Ok(number) if number == 0.0 => "0",
        _ => value,
    };
    try_owned_string(canonical, ledger)
}

fn try_summary_fields(
    result: &CoreExecutionResult,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Vec<(String, String)>, WasmCommandRuntimeError> {
    let availability = result.execution_report().solution_set_availability();
    let has_declared_policy = result
        .summary_field_entries()
        .any(|(key, _)| key == "search_output_policy");
    let coverage_summary = result
        .summary_field_entries()
        .any(|(key, value)| key == "search_output_policy" && value == "coverage-summary");
    let can_copy_verbatim = !coverage_summary
        && ((!availability.uses_explicit_contract() && !has_declared_policy)
            || (availability.contract_valid()
                && availability
                    .materialized_key_count_matches(result.normalized_solution_keys().len())));
    if !can_copy_verbatim {
        // The compatibility builder can synthesize a redacted summary, but
        // that helper allocates internally. Until Core exposes its borrowed
        // redaction plan, the finite boundary fails closed before allocation.
        return Err(finite_projection_error());
    }

    let len = result.summary_field_count();
    let requested = (len as u128)
        .checked_mul(core::mem::size_of::<(String, String)>() as u128)
        .ok_or_else(finite_projection_error)?;
    ledger.authorize_requested(requested)?;
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(len)
        .map_err(|_| finite_allocation_error())?;
    let actual_capacity = fields.capacity();
    ledger.retain_actual(
        (actual_capacity as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)
            .ok_or_else(finite_projection_error)?,
    )?;
    for (key, value) in result.summary_field_entries() {
        fields.push((
            try_owned_string(key, ledger)?,
            try_owned_string(value, ledger)?,
        ));
        if fields.capacity() != actual_capacity {
            return Err(finite_projection_error());
        }
    }
    Ok(fields)
}

fn try_search_path_step(
    step: &clearra_core_executor::CorePathStep,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<WasmSearchPathStep, WasmCommandRuntimeError> {
    let piece = match step.piece().as_ascii() {
        'I' => "I",
        'O' => "O",
        'T' => "T",
        'S' => "S",
        'Z' => "Z",
        'J' => "J",
        'L' => "L",
        _ => return Err(finite_projection_error()),
    };
    Ok(WasmSearchPathStep {
        piece: try_owned_string(piece, ledger)?,
        rotation: step.rotation(),
        x: step.x(),
        y: step.y(),
        hold: try_owned_string(step.hold(), ledger)?,
        cleared_lines: step.cleared_lines(),
    })
}

fn try_build_search_report(
    response: &clearra_app::AppResponse,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Option<WasmSearchReport>, WasmCommandRuntimeError> {
    let Some(render_model) = response.render_model() else {
        return Ok(None);
    };
    if render_model.forward_search_result().is_some() {
        return Err(finite_projection_error());
    }
    let Some(result) = render_model.core_result() else {
        return Ok(None);
    };
    if result.finesse_report().is_some() || result.setup_finder_report().is_some() {
        return Err(finite_projection_error());
    }
    let backend_selected = result
        .field("backend_selected")
        .ok_or_else(finite_projection_error)?;
    let solution_availability = result.execution_report().solution_set_availability();
    let solution_contract_valid = solution_availability.contract_valid()
        && solution_availability
            .materialized_key_count_matches(result.normalized_solution_keys().len())
        && (result.field("search_output_policy") != Some("tiling-only")
            || wasm_pc_tiling_publication_contract_is_valid(result));
    let solution_count_calculated =
        solution_contract_valid && solution_availability.solution_count_calculated();
    let solution_set_materialized =
        solution_contract_valid && solution_availability.solution_set_materialized();
    let solution_keys_materialized_count = if solution_contract_valid {
        solution_availability.solution_keys_materialized_count()
    } else {
        0
    };
    let solution_keys_complete =
        solution_contract_valid && solution_availability.solution_keys_complete();
    let solution_page_available =
        solution_contract_valid && solution_availability.solution_page_available();
    let unique_solution_count = if solution_count_calculated {
        result.usize_field("unique_solution_count").unwrap_or(0)
    } else {
        0
    };

    let packing_candidate_keys = if solution_set_materialized {
        try_owned_string_vec(result.packing_candidate_keys(), ledger)?
    } else {
        Vec::new()
    };
    let normalized_solution_keys = if solution_set_materialized {
        try_owned_string_vec(result.normalized_solution_keys(), ledger)?
    } else {
        Vec::new()
    };
    let solution_probabilities = if solution_set_materialized {
        try_owned_vec(result.solution_probabilities(), ledger, |entry, ledger| {
            Ok(WasmSolutionProbability {
                solution_key: try_owned_string(entry.solution_key(), ledger)?,
                probability: try_canonical_probability(Some(entry.probability()), ledger)?,
                covered_pattern_count: entry.covered_pattern_count(),
                pattern_count: entry.pattern_count(),
                probability_complete: entry.probability_complete(),
            })
        })?
    } else {
        Vec::new()
    };
    let solution_average_scores = if solution_set_materialized {
        try_owned_vec(result.solution_average_scores(), ledger, |entry, ledger| {
            Ok(WasmSolutionAverageScore {
                solution_key: try_owned_string(entry.solution_key(), ledger)?,
                average_score: try_owned_string(entry.average_score(), ledger)?,
                covered_pattern_count: entry.covered_pattern_count(),
                pattern_count: entry.pattern_count(),
                score_complete: entry.score_complete(),
            })
        })?
    } else {
        Vec::new()
    };
    let representative_path = if solution_set_materialized {
        try_owned_vec(result.path_steps(), ledger, try_search_path_step)?
    } else {
        Vec::new()
    };
    let summary_fields = try_summary_fields(result, ledger)?;

    Ok(Some(WasmSearchReport {
        backend_selected: try_owned_string(backend_selected, ledger)?,
        workers_used: result.usize_field("workers_used").unwrap_or(1),
        cpu_parallel_execution: result.bool_field("cpu_parallel_execution").unwrap_or(false),
        cpu_parallel_decision_reason: try_owned_string(
            result
                .field("cpu_parallel_decision_reason")
                .unwrap_or("unknown"),
            ledger,
        )?,
        cpu_warmup_requested: result.bool_field("cpu_warmup_requested").unwrap_or(false),
        cpu_warmup_performed: result.bool_field("cpu_warmup_performed").unwrap_or(false),
        supply_window_resolution: try_owned_string(
            result
                .field("supply_window_resolution")
                .unwrap_or("unknown"),
            ledger,
        )?,
        projects_unplaced_lookahead: result
            .bool_field("projects_unplaced_lookahead")
            .unwrap_or(false),
        projects_standard_bag_lookahead: result
            .bool_field("projects_standard_bag_lookahead")
            .unwrap_or(false),
        source_sequence_length: result.usize_field("source_sequence_length").unwrap_or(0),
        total_possible_pattern_count: try_owned_string(
            result
                .field("total_possible_pattern_count")
                .unwrap_or("unknown"),
            ledger,
        )?,
        solution_found: result.bool_field("solution_found").unwrap_or(false),
        packing_candidate_count: result.usize_field("packing_candidate_count").unwrap_or(0),
        geometry_candidate_family_count: try_owned_string(
            result
                .field("geometry_candidate_family_count")
                .unwrap_or("overflow-or-incomplete"),
            ledger,
        )?,
        packing_candidate_set_digest: try_owned_string(
            result
                .field("packing_candidate_set_digest")
                .unwrap_or("0000000000000000"),
            ledger,
        )?,
        packing_candidate_keys,
        unique_solution_count,
        solution_count_calculated,
        solution_set_materialized,
        solution_keys_materialized_count,
        solution_keys_complete,
        solution_page_available,
        normalized_solution_set_hash: try_owned_string(
            result
                .field("normalized_solution_set_hash")
                .filter(|_| solution_set_materialized)
                .unwrap_or("not-calculated"),
            ledger,
        )?,
        normalized_solution_keys,
        solution_probabilities,
        solution_average_scores,
        finesse_report: None,
        build_variant_count: result
            .field("build_variant_count")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        build_variant_count_exact: try_owned_string(
            result.field("build_variant_count_exact").unwrap_or("false"),
            ledger,
        )?,
        buildability_verified: result.bool_field("buildability_verified").unwrap_or(true),
        coverage_calculated: result.bool_field("coverage_calculated").unwrap_or(true),
        probability_calculated: result.bool_field("probability_calculated").unwrap_or(true),
        materialized_pattern_count: result
            .usize_field("materialized_pattern_count")
            .unwrap_or(0),
        covered_pattern_count: result.usize_field("covered_pattern_count").unwrap_or(0),
        coverage_probability: try_canonical_probability(
            result.field("coverage_probability"),
            ledger,
        )?,
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
        resource_truncation_reason: try_owned_string(
            result.field("resource_truncation_reason").unwrap_or("none"),
            ledger,
        )?,
        representative_candidate_id: if solution_set_materialized {
            try_optional_owned_string(
                result
                    .field("representative_candidate_id")
                    .filter(|value| *value != "none"),
                ledger,
            )?
        } else {
            None
        },
        representative_pattern_id: if solution_set_materialized {
            result
                .field("representative_pattern_id")
                .and_then(|value| value.parse().ok())
        } else {
            None
        },
        representative_path,
        summary_fields,
        forward_search_kind: None,
        forward_initial_board_mask: None,
        forward_canonical_selection: None,
        canonical_forward_outcome: None,
        maximum_damage: None,
        maximum_ren: None,
        forward_outcomes: Vec::new(),
        setup_report: None,
        spin_structure_report: None,
    }))
}

impl WasmExecutionResult {
    /// Consumes a memory-authorized App response and constructs the WASM
    /// transport field by field. Every target allocation is checked before
    /// `try_reserve_exact`, rechecked from actual capacity afterwards, and is
    /// charged while the complete source response is still live.
    ///
    /// A public immutable tiling store is measured through the Core producer's
    /// allocation-free graph projection. The backing is shared during
    /// construction, so it remains part of the source charge until the App
    /// response drops and is transferred to the returned authority exactly
    /// once. Callers cannot provide or forge this retained-byte fact.
    pub fn try_from_governed_app_response(
        governed: GovernedAppResponse,
        webgpu_requested: bool,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        Self::try_from_governed_app_response_for_route(
            governed,
            webgpu_requested,
            WasmFiniteConversionRoute::PublicDirect,
        )
    }

    pub(crate) fn try_from_governed_app_response_for_distributed_finish(
        governed: GovernedAppResponse,
        webgpu_requested: bool,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        Self::try_from_governed_app_response_for_route(
            governed,
            webgpu_requested,
            WasmFiniteConversionRoute::DistributedFinish,
        )
    }

    fn try_from_governed_app_response_for_prepared_advance(
        governed: GovernedAppResponse,
        webgpu_requested: bool,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        Self::try_from_governed_app_response_for_route(
            governed,
            webgpu_requested,
            WasmFiniteConversionRoute::CooperativeAdvance,
        )
    }

    fn try_from_governed_app_response_for_route(
        governed: GovernedAppResponse,
        webgpu_requested: bool,
        route: WasmFiniteConversionRoute,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        let memory_limit_bytes = governed
            .memory_limit_bytes()
            .ok_or_else(finite_limit_error)?;
        let governed_metadata_bytes =
            governed_app_transition_metadata_bytes().ok_or_else(finite_projection_error)?;
        let source_live_bytes = governed
            .actual_retained_bytes()
            .checked_add(governed_metadata_bytes)
            .ok_or_else(finite_projection_error)?;
        if source_live_bytes > memory_limit_bytes {
            return Err(finite_limit_error());
        }
        authorize_public_page_store_projection(
            governed.response(),
            source_live_bytes,
            memory_limit_bytes,
            route,
        )?;
        let shared_page_store_retained_bytes =
            checked_public_page_store_retained_bytes(governed.response())?;
        let (response, confirmed_limit, confirmed_actual) = governed.into_parts();
        if confirmed_limit != Some(memory_limit_bytes)
            || confirmed_actual
                .checked_add(governed_metadata_bytes)
                .ok_or_else(finite_projection_error)?
                != source_live_bytes
        {
            return Err(finite_projection_error());
        }
        try_from_app_response_under_authority(
            response,
            source_live_bytes,
            memory_limit_bytes,
            webgpu_requested,
            shared_page_store_retained_bytes,
            route,
        )
    }

    pub(crate) fn from_app_response(
        response: clearra_app::AppResponse,
        webgpu_requested: bool,
    ) -> Self {
        let search_report = WasmSearchReport::from_response(&response);
        let webgpu_backend = WebGpuBackendReport::from_app_response(&response, webgpu_requested);
        let tiling_solution_page_store = response
            .render_model()
            .and_then(|model| model.core_result())
            .filter(|result| solution_page_store_is_public(result))
            .and_then(|result| result.tiling_solution_page_store())
            .cloned();
        let product_page_source_owner = response.public_page_source_owner();
        Self {
            app_response: response.to_host_response_with_solution_set_artifact(Some(
                HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
            )),
            webgpu_backend,
            search_report,
            tiling_solution_page_store,
            product_page_source_owner,
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

    pub fn tiling_solution_page_store(&self) -> Option<&Arc<TilingSolutionPageStore>> {
        self.tiling_solution_page_store.as_ref()
    }

    pub fn product_page_source_owner(&self) -> Option<&ProductPageSourceOwner> {
        self.product_page_source_owner.as_ref()
    }

    /// Heap payload retained by the JSON-visible transport fields. The shared
    /// tiling page store is intentionally excluded because its graph cannot be
    /// reconstructed from public page-store accessors.
    pub fn checked_transport_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.app_response.checked_retained_capacity_bytes()?;
        bytes = bytes.checked_add(self.webgpu_backend.checked_retained_capacity_bytes()?)?;
        if let Some(search_report) = &self.search_report {
            bytes = bytes.checked_add(search_report.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }

    /// Complete heap payload when the result has no separately shared page
    /// store. Results with a store fail closed and must use the non-cloneable
    /// memory authority returned by `try_from_governed_app_response`.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.tiling_solution_page_store.is_none() && self.product_page_source_owner.is_none())
            .then(|| self.checked_transport_retained_capacity_bytes())
            .flatten()
    }

    /// Moves every result component without cloning terminal payloads.
    pub fn into_parts(
        self,
    ) -> (
        HostAppResponse,
        WebGpuBackendReport,
        Option<WasmSearchReport>,
        Option<Arc<TilingSolutionPageStore>>,
        Option<ProductPageSourceOwner>,
    ) {
        (
            self.app_response,
            self.webgpu_backend,
            self.search_report,
            self.tiling_solution_page_store,
            self.product_page_source_owner,
        )
    }
}

fn try_from_app_response_under_authority(
    response: clearra_app::AppResponse,
    source_live_bytes: u128,
    memory_limit_bytes: u128,
    webgpu_requested: bool,
    shared_page_store_retained_bytes: u128,
    route: WasmFiniteConversionRoute,
) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
    let mut ledger = WasmFiniteMemoryLedger::new(source_live_bytes, memory_limit_bytes, route)?;
    let search_report = try_build_search_report(&response, &mut ledger)?;
    let webgpu_backend = try_webgpu_report(&response, webgpu_requested, &mut ledger)?;
    let tiling_solution_page_store = response
        .render_model()
        .and_then(|model| model.core_result())
        .filter(|result| solution_page_store_is_public(result))
        .and_then(|result| result.tiling_solution_page_store())
        .cloned();
    let product_page_source_owner = response.public_page_source_owner();
    let transferred_page_store_bytes = tiling_solution_page_store
        .as_ref()
        .map_or(Some(0), |store| store.checked_retained_capacity_bytes())
        .and_then(|bytes| {
            bytes.checked_add(product_page_source_owner.as_ref().map_or(
                Some(0),
                ProductPageSourceOwner::checked_retained_capacity_bytes,
            )?)
        })
        .ok_or_else(finite_projection_error)?;
    if transferred_page_store_bytes != shared_page_store_retained_bytes {
        return Err(finite_projection_error());
    }
    let app_response = try_host_app_response(&response, &mut ledger)?;
    let result = WasmExecutionResult {
        app_response,
        webgpu_backend,
        search_report,
        tiling_solution_page_store,
        product_page_source_owner,
    };
    let target_heap = result
        .checked_transport_retained_capacity_bytes()
        .ok_or_else(finite_projection_error)?;
    if target_heap != ledger.target_heap_bytes() {
        return Err(finite_projection_error());
    }

    drop(response);
    let authority = ledger.finish_source(shared_page_store_retained_bytes)?;
    let expected_actual = (core::mem::size_of::<WasmExecutionResult>() as u128)
        .checked_add(target_heap)
        .and_then(|bytes| bytes.checked_add(shared_page_store_retained_bytes))
        .ok_or_else(finite_projection_error)?;
    if authority.actual_retained_bytes() != expected_actual {
        return Err(finite_projection_error());
    }
    Ok(GovernedWasmExecutionResult { result, authority })
}

fn checked_public_page_store_retained_bytes(
    response: &clearra_app::AppResponse,
) -> Result<u128, WasmCommandRuntimeError> {
    let tiling_bytes = public_page_store(response)
        .map_or(Some(0), |store| store.checked_retained_capacity_bytes())
        .ok_or_else(finite_projection_error)?;
    let product_bytes = response
        .public_page_source_owner()
        .as_ref()
        .map_or(
            Some(0),
            ProductPageSourceOwner::checked_retained_capacity_bytes,
        )
        .ok_or_else(finite_projection_error)?;
    tiling_bytes
        .checked_add(product_bytes)
        .ok_or_else(finite_projection_error)
}

fn public_page_store(
    response: &clearra_app::AppResponse,
) -> Option<&std::sync::Arc<TilingSolutionPageStore>> {
    response
        .render_model()
        .and_then(|model| model.core_result())
        .filter(|result| solution_page_store_is_public(result))
        .and_then(|result| result.tiling_solution_page_store())
}

fn authorize_public_page_store_projection(
    response: &clearra_app::AppResponse,
    source_live_bytes: u128,
    memory_limit_bytes: u128,
    route: WasmFiniteConversionRoute,
) -> Result<(), WasmCommandRuntimeError> {
    if public_page_store(response).is_none() {
        return Ok(());
    }
    authorize_page_store_projection_workspace(source_live_bytes, memory_limit_bytes, route)
}

fn authorize_page_store_projection_workspace(
    source_live_bytes: u128,
    memory_limit_bytes: u128,
    route: WasmFiniteConversionRoute,
) -> Result<(), WasmCommandRuntimeError> {
    let workspace =
        TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes()
            .ok_or_else(finite_projection_error)?;
    let required = source_live_bytes
        .checked_add(
            route
                .checked_caller_retained_bytes()
                .ok_or_else(finite_projection_error)?,
        )
        .and_then(|bytes| bytes.checked_add(workspace))
        .ok_or_else(finite_projection_error)?;
    if required > memory_limit_bytes {
        return Err(finite_limit_error());
    }
    Ok(())
}

pub(crate) fn solution_page_store_is_public(result: &CoreExecutionResult) -> bool {
    let availability = result.execution_report().solution_set_availability();
    let Some(store) = result.tiling_solution_page_store() else {
        return false;
    };
    wasm_pc_tiling_publication_contract_is_valid(result)
        && availability.solution_page_available()
        && store.len() > availability.solution_keys_materialized_count()
}

fn wasm_pc_tiling_publication_contract_is_valid(result: &CoreExecutionResult) -> bool {
    result.pc_tiling_memory_admission_evidence()
        == Some(PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority)
        && result.pc_tiling_family_publication_contract_is_valid()
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
    execution: Option<CooperativeAppExecution>,
    webgpu_requested: bool,
}

pub(crate) enum PreparedWasmAdvance {
    Pending,
    Progress,
    Completed(WasmExecutionResult),
    CompletedGoverned(GovernedWasmExecutionResult),
    Failed(WasmCommandRuntimeError),
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
        let request = self
            .parse_command(command_text)?
            .with_runtime_webgpu_available(self.host_capabilities.webgpu_available())
            .to_app_request()
            .map_err(WasmCommandRuntimeError::from_cli_command)?;
        validate_build_memory_authorities(&request)?;
        Ok(request)
    }

    pub fn run_command_text(
        &self,
        command_text: &str,
    ) -> Result<WasmExecutionResult, WasmCommandRuntimeError> {
        let prepared = self.prepare_command_text(command_text)?;
        self.execute_prepared(prepared)
    }

    /// Raw command text cannot enter the finite route until the parser exposes
    /// an allocation-free discriminator or accepts a finite allocator. Fail
    /// closed before parsing rather than presenting a post-parse authority as
    /// an end-to-end hard cap.
    pub fn run_command_text_governed(
        &self,
        _command_text: &str,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        Err(WasmCommandRuntimeError::new(
            WASM_FINITE_AUTHORITY_UNAVAILABLE,
            String::new(),
        ))
    }

    /// Internal bridge for a command whose parsing and preparation ownership
    /// has already been resolved by its caller. This does not grant authority
    /// to the raw-text parser or to the worker transport.
    pub(crate) fn execute_prepared_governed(
        &self,
        prepared: PreparedWasmCommand,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        if prepared.has_finite_build_memory_authority() {
            let control_retained_owner_bytes =
                checked_finite_direct_default_control_retained_owner_bytes()
                    .ok_or_else(finite_projection_error)?;
            let caller_retained_owner_bytes =
                checked_finite_direct_start_retained_owner_bytes(control_retained_owner_bytes)
                    .ok_or_else(finite_projection_error)?;
            // Admit the inline control and its default cancellation Arc
            // pointee before constructing either owner; raw parser allocations
            // are outside this prepared boundary.
            authorize_finite_direct_request_entry(&prepared, caller_retained_owner_bytes)?;
        }
        let control = ExecutionControl::default();
        let mut execution = self.start_prepared_direct_execution(prepared, &control)?;
        loop {
            match execution.advance(4096, &control) {
                PreparedWasmAdvance::Pending | PreparedWasmAdvance::Progress => {}
                PreparedWasmAdvance::CompletedGoverned(result) => return Ok(result),
                PreparedWasmAdvance::Completed(result) => {
                    drop(result);
                    return Err(WasmCommandRuntimeError::new(
                        WASM_FINITE_AUTHORITY_UNAVAILABLE,
                        String::new(),
                    ));
                }
                PreparedWasmAdvance::Failed(error) => return Err(error),
                PreparedWasmAdvance::Cancelled => {
                    return Err(WasmCommandRuntimeError::new(
                        "E_WASM_EXECUTION_STATE",
                        String::new(),
                    ));
                }
            }
        }
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
            .map_err(WasmCommandRuntimeError::from_cli_command)?;
        validate_build_memory_authorities(&request)?;
        Ok(PreparedWasmCommand {
            request,
            webgpu_requested,
        })
    }

    fn parse_command(
        &self,
        command_text: &str,
    ) -> Result<CliCommandRequest, WasmCommandRuntimeError> {
        let request = CliCommandParser::parse_with_worker_limit(
            command_text,
            self.host_capabilities.logical_processor_count(),
        )
        .map_err(WasmCommandRuntimeError::from_cli_command)?;
        Ok(request)
    }

    pub(crate) fn execute_prepared(
        &self,
        prepared: PreparedWasmCommand,
    ) -> Result<WasmExecutionResult, WasmCommandRuntimeError> {
        let control = ExecutionControl::default();
        let mut execution = self.start_prepared_execution(prepared);
        loop {
            match execution.advance(4096, &control) {
                PreparedWasmAdvance::Pending | PreparedWasmAdvance::Progress => {}
                PreparedWasmAdvance::Completed(result) => return Ok(result),
                PreparedWasmAdvance::CompletedGoverned(result) => {
                    drop(result);
                    return Err(WasmCommandRuntimeError::new(
                        "E_WASM_FINITE_AUTHORITY_REQUIRED",
                        String::new(),
                    ));
                }
                PreparedWasmAdvance::Failed(error) => return Err(error),
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
            execution: Some(
                self.app_context
                    .start_cooperative_execution(prepared.request),
            ),
            webgpu_requested: prepared.webgpu_requested,
        }
    }

    /// Direct finite entry point. The worker intentionally continues to call
    /// `start_prepared_execution`, whose compatibility App entry rejects a
    /// finite request without an explicit caller-memory owner.
    fn start_prepared_direct_execution(
        &self,
        prepared: PreparedWasmCommand,
        control: &ExecutionControl,
    ) -> Result<PreparedWasmExecution, WasmCommandRuntimeError> {
        if !prepared.has_finite_build_memory_authority() {
            return Ok(self.start_prepared_execution(prepared));
        }

        let control_retained_owner_bytes =
            checked_finite_direct_control_retained_owner_bytes(control)
                .ok_or_else(finite_projection_error)?;
        let caller_retained_owner_bytes =
            checked_finite_direct_start_retained_owner_bytes(control_retained_owner_bytes)
                .ok_or_else(finite_projection_error)?;
        authorize_finite_direct_request_entry(&prepared, caller_retained_owner_bytes)?;
        let caller_memory = FiniteCooperativeCallerMemory::start(
            caller_retained_owner_bytes,
            finite_direct_returned_carrier_inline_bytes(),
        )
        .map_err(finite_core_execution_error)?;
        let (request, webgpu_requested) = prepared.into_parts();
        let execution = self
            .app_context
            .start_finite_cooperative_execution(request, caller_memory)
            .map_err(finite_app_entry_error)?;
        Ok(PreparedWasmExecution {
            execution: Some(execution),
            webgpu_requested,
        })
    }
}

fn checked_finite_direct_start_retained_owner_bytes(
    control_retained_owner_bytes: u128,
) -> Option<u128> {
    (core::mem::size_of::<PreparedWasmCommand>() as u128)
        .checked_sub(core::mem::size_of::<AppRequest>() as u128)
        .and_then(|bytes| bytes.checked_add(control_retained_owner_bytes))
}

fn checked_finite_direct_advance_retained_owner_bytes(control: &ExecutionControl) -> Option<u128> {
    (core::mem::size_of::<PreparedWasmExecution>() as u128)
        .checked_sub(core::mem::size_of::<CooperativeAppExecution>() as u128)
        .and_then(|bytes| {
            bytes.checked_add(checked_finite_direct_control_retained_owner_bytes(control)?)
        })
}

fn checked_finite_direct_control_retained_owner_bytes(control: &ExecutionControl) -> Option<u128> {
    // The direct finite route installs no progress sink. A trait-object sink
    // has no complete retained-byte projection here, so any such shape fails
    // closed before the App owner or generation can move.
    if control.progress_sink.is_some() {
        return None;
    }
    checked_finite_direct_default_control_retained_owner_bytes()
}

fn checked_finite_direct_default_control_retained_owner_bytes() -> Option<u128> {
    (core::mem::size_of::<ExecutionControl>() as u128)
        .checked_add(core::mem::size_of::<AtomicU32>() as u128)
}

fn finite_direct_returned_carrier_inline_bytes() -> u128 {
    WasmFiniteConversionRoute::CooperativeAdvance.returned_carrier_inline_bytes()
}

fn checked_finite_direct_request_entry_bytes(
    prepared: &PreparedWasmCommand,
    caller_retained_owner_bytes: u128,
) -> Option<u128> {
    (core::mem::size_of::<AppRequest>() as u128)
        .checked_add(
            prepared
                .request
                .checked_build_probability_retained_capacity_bytes()?,
        )?
        .checked_add(caller_retained_owner_bytes)
}

fn authorize_finite_direct_bytes(
    required_bytes: u128,
    memory_limit_bytes: u128,
) -> Result<(), WasmCommandRuntimeError> {
    if required_bytes > memory_limit_bytes {
        return Err(finite_limit_error());
    }
    Ok(())
}

fn authorize_finite_direct_request_entry(
    prepared: &PreparedWasmCommand,
    caller_retained_owner_bytes: u128,
) -> Result<(), WasmCommandRuntimeError> {
    let memory_limit_bytes = prepared
        .request
        .resource_budget()
        .max_memory_mib()
        .and_then(|mib| u128::from(mib).checked_mul(WASM_MIB_BYTES))
        .ok_or_else(finite_projection_error)?;
    let required_bytes =
        checked_finite_direct_request_entry_bytes(prepared, caller_retained_owner_bytes)
            .ok_or_else(finite_projection_error)?;
    authorize_finite_direct_bytes(required_bytes, memory_limit_bytes)
}

fn validate_build_memory_authorities(request: &AppRequest) -> Result<(), WasmCommandRuntimeError> {
    let AppCommand::BuildProbability(command) = request.command() else {
        return Ok(());
    };
    let query_memory_mib = command
        .query()
        .core_query()
        .execution_policy()
        .max_memory_mib();
    let request_memory_mib = request.resource_budget().max_memory_mib().map(u64::from);
    if query_memory_mib != request_memory_mib {
        return Err(WasmCommandRuntimeError::new(
            "E_WASM_BUILD_MEMORY_AUTHORITY_MISMATCH",
            String::new(),
        ));
    }
    Ok(())
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

    pub(crate) fn has_finite_build_memory_authority(&self) -> bool {
        matches!(self.request.command(), AppCommand::BuildProbability(_))
            && self.request.resource_budget().max_memory_mib().is_some()
    }
}

impl PreparedWasmExecution {
    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> PreparedWasmAdvance {
        let execution = self
            .execution
            .as_mut()
            .expect("a finished WASM execution cannot advance again");
        let advance = if execution.finite_caller_memory().is_some() {
            let caller_retained_owner_bytes =
                match checked_finite_direct_advance_retained_owner_bytes(control) {
                    Some(bytes) => bytes,
                    None => return PreparedWasmAdvance::Failed(finite_projection_error()),
                };
            match advance_finite_prepared_app_execution(
                execution,
                caller_retained_owner_bytes,
                finite_direct_returned_carrier_inline_bytes(),
                work_budget,
                control,
            ) {
                Ok(advance) => advance,
                Err(error) => return PreparedWasmAdvance::Failed(error),
            }
        } else {
            execution.advance(work_budget, control)
        };
        match advance {
            CooperativeAppAdvance::Pending => PreparedWasmAdvance::Pending,
            CooperativeAppAdvance::Progress => PreparedWasmAdvance::Progress,
            CooperativeAppAdvance::Cancelled => {
                drop(self.execution.take());
                PreparedWasmAdvance::Cancelled
            }
            CooperativeAppAdvance::Completed(response) => {
                drop(self.execution.take());
                PreparedWasmAdvance::Completed(WasmExecutionResult::from_app_response(
                    response,
                    self.webgpu_requested,
                ))
            }
            CooperativeAppAdvance::CompletedGoverned(response) => {
                drop(self.execution.take());
                match WasmExecutionResult::try_from_governed_app_response_for_prepared_advance(
                    response,
                    self.webgpu_requested,
                ) {
                    Ok(result) => PreparedWasmAdvance::CompletedGoverned(result),
                    Err(error) => PreparedWasmAdvance::Failed(error),
                }
            }
            CooperativeAppAdvance::FailedFinite(error) => {
                drop(self.execution.take());
                PreparedWasmAdvance::Failed(finite_core_execution_error(error))
            }
        }
    }
}

fn advance_finite_prepared_app_execution(
    execution: &mut CooperativeAppExecution,
    caller_retained_owner_bytes: u128,
    returned_carrier_inline_bytes: u128,
    work_budget: usize,
    control: &ExecutionControl,
) -> Result<CooperativeAppAdvance, WasmCommandRuntimeError> {
    execution
        .advance_finite_with_next_caller_memory(
            caller_retained_owner_bytes,
            returned_carrier_inline_bytes,
            work_budget,
            control,
        )
        .map_err(finite_core_execution_error)
}

fn finite_app_entry_error(
    rejection: FiniteCooperativeCallerMemoryRejection,
) -> WasmCommandRuntimeError {
    // No finite App session exists on this terminal start rejection. Destroy
    // the returned unique caller-memory owner together with the rejected
    // detail, then expose only an allocation-free static classification.
    drop(rejection);
    WasmCommandRuntimeError::new(WASM_FINITE_APP_ENTRY, String::new())
}

fn finite_core_execution_error(
    error: clearra_core_executor::CoreExecutionError,
) -> WasmCommandRuntimeError {
    // The finite producer failed before it could transfer an admitted response
    // authority. Do not let an owned Core message or resource report escape
    // through the ordinary rich error surface after that failure. Destroy the
    // complete input graph and return one allocation-free static classification.
    drop(error);
    WasmCommandRuntimeError::new("E_WASM_FINITE_CORE_EXECUTION", String::new())
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
    resource_report: Option<clearra_core_domain::resource::ResourceReport>,
}

impl WasmCommandRuntimeError {
    fn from_cli_command(error: CliCommandError) -> Self {
        Self::new(error.code().as_diagnostic_code(), error.message())
    }

    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            resource_report: None,
        }
    }

    pub fn with_resource_report(
        mut self,
        resource_report: clearra_core_domain::resource::ResourceReport,
    ) -> Self {
        self.resource_report = Some(resource_report);
        self
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn resource_report(&self) -> Option<&clearra_core_domain::resource::ResourceReport> {
        self.resource_report.as_ref()
    }

    pub fn diagnostic_report(&self) -> DiagnosticReport {
        DiagnosticReport::single(Diagnostic::new(self.code, "error", self.message.as_str()))
    }

    #[cfg(test)]
    pub(crate) fn message_capacity_for_test(&self) -> usize {
        self.message.capacity()
    }
}

impl fmt::Display for WasmCommandRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WasmCommandRuntimeError {}

#[cfg(test)]
mod finite_memory_tests {
    use super::*;
    use clearra_app::render::AppRenderModel;
    use clearra_host_contract::{AppCommandKind, ResourceBudget};
    use std::{
        collections::{hash_map::DefaultHasher, VecDeque},
        hash::{Hash, Hasher},
    };

    const FINITE_BUILD_COMMAND: &str =
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
         --queue I --no-hold --no-mirror --workers 1 --max-memory-mib 64";
    const UNBOUNDED_BUILD_COMMAND: &str =
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
         --queue I --no-hold --no-mirror --workers 1";

    fn finite_prepared_command(runtime: &WasmCommandRuntime) -> PreparedWasmCommand {
        runtime
            .prepare_command_text(FINITE_BUILD_COMMAND)
            .expect("finite Build command prepares")
    }

    struct UnmeasuredProgressSink;

    impl clearra_core_domain::execution_cancellation::ProgressSink for UnmeasuredProgressSink {
        fn report(
            &self,
            _progress: clearra_core_domain::execution_cancellation::ExecutionProgress,
        ) {
        }
    }

    #[test]
    fn build_memory_authority_comparison_is_fieldwise_and_fails_closed() {
        let runtime = WasmCommandRuntime::default();
        let request = runtime
            .compile_command_text(FINITE_BUILD_COMMAND)
            .expect("matching finite Build authorities");
        assert_eq!(request.resource_budget().max_memory_mib(), Some(64));

        let mismatched = request.with_resource_budget(ResourceBudget::new(1, None, Some(63)));
        let error = validate_build_memory_authorities(&mismatched)
            .expect_err("two distinct Build memory authorities must be rejected");
        assert_eq!(error.code(), "E_WASM_BUILD_MEMORY_AUTHORITY_MISMATCH");
        assert!(error.message().is_empty());
    }

    #[test]
    fn direct_finite_request_entry_has_an_exact_cap_and_rejects_one_byte_short() {
        let runtime = WasmCommandRuntime::default();
        let prepared = finite_prepared_command(&runtime);
        let request_heap = prepared
            .request
            .checked_build_probability_retained_capacity_bytes()
            .expect("finite request retained capacity fits");
        let control_retained_owner = checked_finite_direct_default_control_retained_owner_bytes()
            .expect("default control retained-owner projection fits");
        let start_transport =
            checked_finite_direct_start_retained_owner_bytes(control_retained_owner)
                .expect("PreparedWasmCommand contains AppRequest inline");
        let required = checked_finite_direct_request_entry_bytes(&prepared, start_transport)
            .expect("direct finite request entry projection fits");
        let expected_transport = (core::mem::size_of::<PreparedWasmCommand>() as u128)
            .checked_sub(core::mem::size_of::<AppRequest>() as u128)
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<ExecutionControl>() as u128))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<AtomicU32>() as u128))
            .expect("transport wrapper is at least as large as AppRequest");
        let expected_required = (core::mem::size_of::<AppRequest>() as u128)
            .checked_add(request_heap)
            .and_then(|bytes| bytes.checked_add(expected_transport))
            .expect("direct finite request entry fixture fits");

        assert_eq!(start_transport, expected_transport);
        assert_eq!(required, expected_required);
        authorize_finite_direct_bytes(required, required)
            .expect("the exact direct finite request-entry cap is admitted");

        let error = authorize_finite_direct_bytes(
            required,
            required
                .checked_sub(1)
                .expect("request entry fixture is nonzero"),
        )
        .expect_err("one byte below the exact request-entry cap is rejected");
        assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);
    }

    #[test]
    fn direct_finite_transport_formulas_cover_generations_and_full_public_route_carrier() {
        let control_retained_owner = checked_finite_direct_default_control_retained_owner_bytes()
            .expect("default control retained-owner projection fits");
        let expected_control_retained_owner = (core::mem::size_of::<ExecutionControl>() as u128)
            .checked_add(core::mem::size_of::<AtomicU32>() as u128)
            .expect("default control retained-owner fixture fits");
        assert_eq!(control_retained_owner, expected_control_retained_owner);
        let start_transport =
            checked_finite_direct_start_retained_owner_bytes(control_retained_owner)
                .expect("finite direct start transport projection fits");
        let expected_returned_carrier = core::mem::size_of::<GovernedWasmExecutionResult>()
            .max(core::mem::size_of::<(
                WasmExecutionResult,
                WasmExecutionMemoryAuthority,
            )>())
            .max(core::mem::size_of::<
                Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>,
            >())
            .max(core::mem::size_of::<PreparedWasmAdvance>())
            as u128;
        let caller_memory = FiniteCooperativeCallerMemory::start(
            start_transport,
            finite_direct_returned_carrier_inline_bytes(),
        )
        .expect("generation-zero transport fixture is representable");

        assert_eq!(caller_memory.generation(), 0);
        assert_eq!(caller_memory.retained_owner_bytes(), start_transport);
        assert_eq!(
            caller_memory.returned_carrier_bytes(),
            expected_returned_carrier
        );
        assert_eq!(
            caller_memory.returned_carrier_bytes(),
            finite_direct_returned_carrier_inline_bytes()
        );
        assert!(
            caller_memory.returned_carrier_bytes()
                >= core::mem::size_of::<Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>>(
                ) as u128
        );

        let control = ExecutionControl::default();
        let advance_transport = checked_finite_direct_advance_retained_owner_bytes(&control)
            .expect("finite direct advance transport projection fits");
        let generation_one = caller_memory
            .next(advance_transport, expected_returned_carrier)
            .expect("generation-one transport fixture is representable");
        assert_eq!(generation_one.generation(), 1);
        assert_eq!(generation_one.retained_owner_bytes(), advance_transport);
        let expected_advance_transport = (core::mem::size_of::<PreparedWasmExecution>() as u128)
            .checked_sub(core::mem::size_of::<CooperativeAppExecution>() as u128)
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<ExecutionControl>() as u128))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<AtomicU32>() as u128))
            .expect("finite direct advance transport fixture fits");
        assert_eq!(
            generation_one.retained_owner_bytes(),
            expected_advance_transport
        );
        assert_eq!(
            generation_one.returned_carrier_bytes(),
            expected_returned_carrier
        );
        let expected_conversion_caller_retained = (core::mem::size_of::<PreparedWasmExecution>()
            as u128)
            .checked_add(expected_control_retained_owner)
            .expect("finite conversion caller-retained fixture fits");
        assert_eq!(
            WasmFiniteConversionRoute::CooperativeAdvance
                .checked_caller_retained_bytes()
                .expect("finite conversion caller-retained projection fits"),
            expected_conversion_caller_retained
        );
    }

    #[test]
    fn direct_finite_generation_failure_returns_the_exact_previous_owner() {
        let control_retained_owner = checked_finite_direct_default_control_retained_owner_bytes()
            .expect("default control retained-owner projection fits");
        let start_transport =
            checked_finite_direct_start_retained_owner_bytes(control_retained_owner)
                .expect("finite direct start transport projection fits");
        let returned_carrier = finite_direct_returned_carrier_inline_bytes();
        let caller_memory = FiniteCooperativeCallerMemory::start(start_transport, returned_carrier)
            .expect("generation-zero transport fixture is representable");
        let (core_error, restored) = caller_memory
            .next(u128::MAX, 1)
            .expect_err("overflowing next transport bytes return the exact old owner");
        let error = finite_core_execution_error(core_error);
        assert_eq!(error.code(), "E_WASM_FINITE_CORE_EXECUTION");
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);
        assert_eq!(restored.generation(), 0);
        assert_eq!(restored.retained_owner_bytes(), start_transport);
        assert_eq!(restored.returned_carrier_bytes(), returned_carrier);

        let control = ExecutionControl::default();
        let advance_transport = checked_finite_direct_advance_retained_owner_bytes(&control)
            .expect("finite direct advance transport projection fits");
        let advanced = restored
            .next(advance_transport, returned_carrier)
            .expect("the returned owner accepts the exact next generation");
        assert_eq!(advanced.generation(), 1);
        assert_eq!(advanced.retained_owner_bytes(), advance_transport);
        assert_eq!(advanced.returned_carrier_bytes(), returned_carrier);
    }

    #[test]
    fn direct_finite_unmeasured_progress_owner_fails_before_owner_or_generation_moves() {
        let control_retained_owner = checked_finite_direct_default_control_retained_owner_bytes()
            .expect("default control retained-owner projection fits");
        let start_transport =
            checked_finite_direct_start_retained_owner_bytes(control_retained_owner)
                .expect("finite direct start transport projection fits");
        let returned_carrier = finite_direct_returned_carrier_inline_bytes();
        let caller_memory = FiniteCooperativeCallerMemory::start(start_transport, returned_carrier)
            .expect("generation-zero transport fixture is representable");
        let control =
            ExecutionControl::default().with_progress_sink(Arc::new(UnmeasuredProgressSink));

        assert!(checked_finite_direct_advance_retained_owner_bytes(&control).is_none());
        let error = finite_projection_error();
        assert_eq!(error.code(), WASM_FINITE_PROJECTION);
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);
        assert_eq!(caller_memory.generation(), 0);
        assert_eq!(caller_memory.retained_owner_bytes(), start_transport);
        assert_eq!(caller_memory.returned_carrier_bytes(), returned_carrier);
    }

    #[test]
    fn raw_text_governed_entry_fails_closed_before_parser_authority() {
        let runtime = WasmCommandRuntime::default();
        for command_text in [FINITE_BUILD_COMMAND, "not even a Clearra command"] {
            let error = runtime
                .run_command_text_governed(command_text)
                .expect_err("raw text has no finite parser authority");
            assert_eq!(error.code(), WASM_FINITE_AUTHORITY_UNAVAILABLE);
            assert!(error.message().is_empty());
            assert_eq!(error.message_capacity_for_test(), 0);
            assert!(error.resource_report().is_none());
        }
    }

    #[test]
    fn compatibility_start_remains_fail_closed_and_unbounded_direct_start_is_unchanged() {
        let runtime = WasmCommandRuntime::default();
        let mut compatibility = runtime.start_prepared_execution(finite_prepared_command(&runtime));
        assert!(
            compatibility
                .execution
                .as_ref()
                .expect("compatibility App placeholder exists")
                .finite_caller_memory()
                .is_none(),
            "the worker-compatible starter must not mint finite caller authority"
        );
        let error = match compatibility.advance(1, &ExecutionControl::default()) {
            PreparedWasmAdvance::Failed(error) => error,
            _ => panic!("finite compatibility execution must fail closed"),
        };
        assert_eq!(error.code(), "E_WASM_FINITE_CORE_EXECUTION");
        assert!(error.message().is_empty());
        assert_eq!(error.message_capacity_for_test(), 0);

        let unbounded = runtime
            .prepare_command_text(UNBOUNDED_BUILD_COMMAND)
            .expect("unbounded compatibility Build prepares");
        assert!(!unbounded.has_finite_build_memory_authority());
        let control = ExecutionControl::default();
        let unbounded = runtime
            .start_prepared_direct_execution(unbounded, &control)
            .expect("unbounded direct execution keeps compatibility behavior");
        assert!(unbounded
            .execution
            .as_ref()
            .expect("unbounded App execution exists")
            .finite_caller_memory()
            .is_none());
    }

    fn allocated(capacity: usize, value: &str) -> String {
        let mut output = String::with_capacity(capacity);
        output.push_str(value);
        output
    }

    fn build_v2_public_payload_fixtures() -> Vec<ProductResultPayload> {
        let replay_basis = Some("normalized-colored-solution-replay.v1".to_owned());
        let replay_complete = clearra_host_contract::BuildV2CompletenessPayload::new(
            true, true, true, true, true, false, false,
        );
        let portfolio_complete = clearra_host_contract::BuildV2CompletenessPayload::new(
            true, true, true, true, true, true, false,
        );
        let score_complete = clearra_host_contract::BuildV2CompletenessPayload::new(
            true, true, true, true, true, true, true,
        );

        let candidate_family = BuildV2ProductPayload::try_candidate_family(
            "build.evaluate.cover",
            "build-supplied-coverage.v1",
            "a".repeat(64),
            "b".repeat(64),
            "all",
            "2",
            "1",
            "2",
            "1",
            "50%",
            Some(false),
            vec![
                BuildV2CandidateCoveragePayload::try_new("candidate-a", "1").expect("candidate a"),
                BuildV2CandidateCoveragePayload::try_new("candidate-b", "0").expect("candidate b"),
            ],
            replay_complete,
        )
        .expect("candidate-family fixture");
        let probability = BuildV2ProductPayload::try_probability(
            "build.evaluate.cover-percent",
            "build-supplied-probability.v1",
            "c".repeat(64),
            "d".repeat(64),
            replay_basis.clone(),
            "unique",
            "2",
            "1",
            "2",
            "1",
            "50%",
            replay_complete,
        )
        .expect("probability fixture");
        let portfolio = BuildV2ProductPayload::try_portfolio(
            "build.evaluate.minimals",
            "build-supplied-minimum-cover.v1",
            "e".repeat(64),
            replay_basis,
            "min-cover",
            "2",
            "2",
            "1",
            "2",
            "2",
            "100%",
            vec!["candidate-a".to_owned()],
            portfolio_complete,
            "f".repeat(64),
        )
        .expect("portfolio fixture");
        let score = BuildV2ProductPayload::try_score_portfolio(
            "build.evaluate.score",
            "build-supplied-score.v1",
            "1".repeat(64),
            "tetrio",
            "0",
            "basic-approximation",
            false,
            "score-only",
            "canonical-equal-score-trace",
            "2",
            "2",
            "2",
            "2",
            "2",
            vec!["candidate-a".to_owned(), "candidate-b".to_owned()],
            vec![
                BuildV2ScoreWinnerPayload::try_new("0", "candidate-a", "1200", "4")
                    .expect("score winner 0"),
                BuildV2ScoreWinnerPayload::try_new("1", "candidate-b", "1200", "9")
                    .expect("score winner 1"),
            ],
            score_complete,
            "2".repeat(64),
        )
        .expect("score fixture");

        [
            (
                "build.evaluate.cover",
                "build-supplied-coverage.v1",
                candidate_family,
            ),
            (
                "build.evaluate.cover-percent",
                "build-supplied-probability.v1",
                probability,
            ),
            (
                "build.evaluate.minimals",
                "build-supplied-minimum-cover.v1",
                portfolio,
            ),
            ("build.evaluate.score", "build-supplied-score.v1", score),
        ]
        .into_iter()
        .map(|(contract, result_kind, payload)| {
            ProductResultPayload::new(
                contract,
                result_kind,
                ProductResultPayloadContent::BuildV2(payload),
            )
        })
        .collect()
    }

    #[test]
    fn public_payload_result_kind_precedes_product_and_raw_fallbacks() {
        let payloads = build_v2_public_payload_fixtures();
        let score = payloads.last().expect("score public payload");
        let fallback = HostAppResult::new("verify");

        assert_eq!(
            preferred_result_kind(Some(score), Some("legacy-product-result"), Some(&fallback),),
            Some("build-supplied-score.v1")
        );
        assert_eq!(
            preferred_result_kind(None, Some("legacy-product-result"), Some(&fallback)),
            Some("legacy-product-result")
        );
        assert_eq!(
            preferred_result_kind(None, None, Some(&fallback)),
            Some("verify")
        );
    }

    #[test]
    fn build_v2_public_payload_fieldwise_copy_has_exact_peak_boundaries() {
        for source in build_v2_public_payload_fixtures() {
            let source_heap = source
                .checked_retained_capacity_bytes()
                .expect("source payload heap fits");
            let source_live = (core::mem::size_of::<ProductResultPayload>() as u128)
                .checked_add(source_heap)
                .expect("source payload live bytes fit");
            let mut measured_ledger = WasmFiniteMemoryLedger::new(
                source_live,
                u128::MAX,
                WasmFiniteConversionRoute::PublicDirect,
            )
            .expect("measurement ledger");
            let measured = try_clone_public_product_result_payload(&source, &mut measured_ledger)
                .expect("fieldwise public payload copy");
            let target_heap = measured
                .checked_retained_capacity_bytes()
                .expect("target payload heap fits");
            assert_eq!(measured, source);
            assert_eq!(measured_ledger.target_heap_bytes(), target_heap);
            if let ProductResultPayloadContent::BuildV2(payload) = measured.content() {
                if payload.kind() == clearra_host_contract::BuildV2PayloadKind::ScorePortfolio {
                    assert_eq!(payload.objective(), "max-score-cover");
                    assert_eq!(payload.winners()[1].informational_attack(), "9");
                }
            } else {
                panic!("Build v2 fixture changed payload kind");
            }
            let exact_peak = source_live
                .checked_add(core::mem::size_of::<WasmExecutionResult>() as u128)
                .and_then(|bytes| bytes.checked_add(target_heap))
                .expect("fieldwise copy peak fits");
            drop(measured);

            let mut exact_ledger = WasmFiniteMemoryLedger::new(
                source_live,
                exact_peak,
                WasmFiniteConversionRoute::PublicDirect,
            )
            .expect("exact baseline");
            let exact = try_clone_public_product_result_payload(&source, &mut exact_ledger)
                .expect("exact construction peak is admitted");
            assert_eq!(exact, source);
            assert_eq!(exact_ledger.target_heap_bytes(), target_heap);
            drop(exact);

            let mut below_ledger = WasmFiniteMemoryLedger::new(
                source_live,
                exact_peak - 1,
                WasmFiniteConversionRoute::PublicDirect,
            )
            .expect("peak-minus-one baseline");
            let error = try_clone_public_product_result_payload(&source, &mut below_ledger)
                .expect_err("construction peak minus one is rejected");
            assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
        }
    }

    fn build_app_response_fixture() -> clearra_app::AppResponse {
        let result = CoreExecutionResult::new(
            vec![
                ("search_kind".to_owned(), "build-probability".to_owned()),
                ("backend_requested".to_owned(), "cpu".to_owned()),
                ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
                ("memory_status".to_owned(), "reported".to_owned()),
                ("solution_found".to_owned(), "false".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                ("coverage_probability".to_owned(), "0".to_owned()),
            ],
            Vec::new(),
        );
        clearra_app::AppResponse::success(AppRenderModel::BuildProbability(result))
            .with_contract_context(AppCommandKind::BuildProbability)
    }

    fn checked_fixture_source_live_bytes(response: &clearra_app::AppResponse) -> u128 {
        assert!(response.effects().is_empty());
        // `AppResponse::success` installs an empty `Vec::new()` effects owner.
        // Preserve that owner's actual capacity as a pointer-free oracle and
        // destroy the mirror owner before any exact-boundary execution.
        let fixture_effects_owner = Vec::<clearra_app::AppEffect>::new();
        let effects_capacity = fixture_effects_owner.capacity();
        drop(fixture_effects_owner);
        let mut heap = response
            .result()
            .and_then(HostAppResult::checked_retained_capacity_bytes)
            .expect("fixture has a result");
        heap = heap
            .checked_add(
                response
                    .diagnostics()
                    .validation()
                    .checked_retained_capacity_bytes()
                    .expect("diagnostic capacity fits"),
            )
            .and_then(|bytes| {
                bytes.checked_add(
                    response
                        .backend_report()
                        .checked_retained_capacity_bytes()?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    response
                        .resource_report()
                        .checked_retained_capacity_bytes()?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    response
                        .capability_report()
                        .checked_retained_capacity_bytes()?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (effects_capacity as u128)
                        .checked_mul(core::mem::size_of::<clearra_app::AppEffect>() as u128)?,
                )
            })
            .expect("fixture host heap fits");
        let core_result = response
            .render_model()
            .and_then(|model| model.core_result())
            .expect("fixture core result");
        let core_heap = core_result
            .checked_resource_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
            })
            .expect("fixture core heap fits");
        let governed_metadata =
            governed_app_transition_metadata_bytes().expect("governed metadata fits");
        (core::mem::size_of::<clearra_app::AppResponse>() as u128)
            .checked_add(heap)
            .and_then(|bytes| bytes.checked_add(core_heap))
            .and_then(|bytes| bytes.checked_add(governed_metadata))
            .expect("fixture source live bytes fit")
    }

    fn governed_result_fixture() -> GovernedWasmExecutionResult {
        let response = build_app_response_fixture();
        let source_live = checked_fixture_source_live_bytes(&response);
        try_from_app_response_under_authority(
            response,
            source_live,
            u128::MAX,
            false,
            0,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect("governed result fixture")
    }

    fn event_prefix_fixture() -> VecDeque<crate::WasmWorkerJobEvent> {
        let job_id = crate::WasmWorkerJobId::new(7);
        let mut events = VecDeque::with_capacity(6);
        events.push_back(crate::WasmWorkerJobEvent::Started { job_id });
        events.push_back(crate::WasmWorkerJobEvent::Progress {
            job_id,
            progress: crate::JobProgress::new(1, 2, "AppRequest parsed and validated"),
        });
        events.push_back(crate::WasmWorkerJobEvent::PartialResult {
            job_id,
            partial: true,
            label: allocated(192, "quote\" slash\\ newline\n control\u{1} 한글"),
            final_result: false,
        });
        events
    }

    fn checked_prefix_actual(prefix: &VecDeque<crate::WasmWorkerJobEvent>) -> u128 {
        let outer = (prefix.capacity() as u128)
            .checked_mul(core::mem::size_of::<crate::WasmWorkerJobEvent>() as u128)
            .expect("prefix outer fits");
        let nested = prefix
            .iter()
            .try_fold(0_u128, |bytes, event| {
                bytes.checked_add(event.checked_retained_capacity_bytes()?)
            })
            .expect("prefix nested fits");
        outer.checked_add(nested).expect("prefix heap fits")
    }

    #[test]
    fn full_finite_conversion_matches_generic_and_has_exact_peak_boundary() {
        let expected = WasmExecutionResult::from_app_response(build_app_response_fixture(), false);
        let measured_source = build_app_response_fixture();
        let source_live = checked_fixture_source_live_bytes(&measured_source);
        let measured = try_from_app_response_under_authority(
            measured_source,
            source_live,
            u128::MAX,
            false,
            0,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect("unbounded measurement succeeds");
        assert_eq!(measured.result(), &expected);
        let target_heap = measured
            .result()
            .checked_transport_retained_capacity_bytes()
            .expect("target heap capacity fits");
        let construction_peak = source_live
            .checked_add(core::mem::size_of::<WasmExecutionResult>() as u128)
            .and_then(|bytes| bytes.checked_add(target_heap))
            .expect("conversion peak fits");
        let returned_peak = WasmFiniteConversionRoute::PublicDirect
            .returned_carrier_inline_bytes()
            .checked_add(target_heap)
            .expect("returned conversion carrier fits");
        let exact_peak = construction_peak.max(returned_peak);
        drop(measured);
        drop(expected);

        let exact_source = build_app_response_fixture();
        assert_eq!(
            checked_fixture_source_live_bytes(&exact_source),
            source_live
        );
        let exact = try_from_app_response_under_authority(
            exact_source,
            source_live,
            exact_peak,
            false,
            0,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect("exact measured conversion peak is admitted");
        assert_eq!(
            exact
                .result()
                .app_response()
                .result()
                .map(|result| result.kind()),
            Some("build-probability")
        );
        assert_eq!(
            exact.result().checked_transport_retained_capacity_bytes(),
            Some(target_heap)
        );
        drop(exact);

        let below_source = build_app_response_fixture();
        assert_eq!(
            checked_fixture_source_live_bytes(&below_source),
            source_live
        );
        let error = try_from_app_response_under_authority(
            below_source,
            source_live,
            exact_peak - 1,
            false,
            0,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect_err("conversion peak minus one is rejected");
        assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
    }

    fn assert_conversion_route_exact_boundary(route: WasmFiniteConversionRoute) {
        let measured_source = build_app_response_fixture();
        let source_live = checked_fixture_source_live_bytes(&measured_source);
        let measured = try_from_app_response_under_authority(
            measured_source,
            source_live,
            u128::MAX,
            false,
            0,
            route,
        )
        .expect("route measurement succeeds");
        let target_heap = measured
            .result()
            .checked_transport_retained_capacity_bytes()
            .expect("route target heap fits");
        let caller_retained = route
            .checked_caller_retained_bytes()
            .expect("route caller retained bytes fit");
        let construction_peak = source_live
            .checked_add(caller_retained)
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<WasmExecutionResult>() as u128)
            })
            .and_then(|bytes| bytes.checked_add(target_heap))
            .expect("route construction peak fits");
        let returned_peak = caller_retained
            .checked_add(route.returned_carrier_inline_bytes())
            .and_then(|bytes| bytes.checked_add(target_heap))
            .expect("route return peak fits");
        let exact_peak = construction_peak.max(returned_peak);
        drop(measured);

        let exact_source = build_app_response_fixture();
        let exact = try_from_app_response_under_authority(
            exact_source,
            source_live,
            exact_peak,
            false,
            0,
            route,
        )
        .expect("exact typed route peak is admitted");
        assert_eq!(
            exact.result().checked_transport_retained_capacity_bytes(),
            Some(target_heap)
        );
        drop(exact);

        let below_source = build_app_response_fixture();
        let error = try_from_app_response_under_authority(
            below_source,
            source_live,
            exact_peak - 1,
            false,
            0,
            route,
        )
        .expect_err("typed route peak minus one is rejected");
        assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
    }

    #[test]
    fn cooperative_and_distributed_conversion_carriers_have_exact_boundaries() {
        assert_conversion_route_exact_boundary(WasmFiniteConversionRoute::CooperativeAdvance);
        assert_conversion_route_exact_boundary(WasmFiniteConversionRoute::DistributedFinish);
    }

    #[test]
    fn governed_event_transition_preserves_prefix_order_and_exact_peak() {
        let governed = governed_result_fixture();
        let old_actual = governed.authority().actual_retained_bytes();
        let transport_heap = governed
            .result()
            .checked_transport_retained_capacity_bytes()
            .expect("transport heap fits");
        let payload_pointer = governed
            .result()
            .app_response()
            .result()
            .expect("result")
            .kind()
            .as_ptr();
        let prefix = event_prefix_fixture();
        let prefix_actual = checked_prefix_actual(&prefix);
        let prefix_outer = (prefix.capacity() as u128)
            .checked_mul(core::mem::size_of::<crate::WasmWorkerJobEvent>() as u128)
            .expect("prefix outer fits");
        let prefix_nested = prefix_actual
            .checked_sub(prefix_outer)
            .expect("prefix nested fits");
        let measured = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed,
            prefix,
        )
        .expect("event measurement succeeds");
        assert!(matches!(
            measured.events().first(),
            Some(crate::WasmWorkerJobEvent::Started { .. })
        ));
        assert!(matches!(
            measured.events().get(measured.events().len() - 2),
            Some(crate::WasmWorkerJobEvent::Progress { progress, .. }) if progress.done == 2
        ));
        let final_response = measured
            .events()
            .last()
            .and_then(|event| match event {
                crate::WasmWorkerJobEvent::FinalResponse { response, .. } => Some(response),
                _ => None,
            })
            .expect("final response follows progress");
        assert_eq!(
            final_response
                .result()
                .expect("moved result")
                .kind()
                .as_ptr(),
            payload_pointer
        );
        let progress_heap = measured
            .events()
            .get(measured.events().len() - 2)
            .and_then(|event| match event {
                crate::WasmWorkerJobEvent::Progress { progress, .. } => {
                    progress.checked_retained_capacity_bytes()
                }
                _ => None,
            })
            .expect("terminal progress heap fits");
        let final_event_heap = measured
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<crate::GovernedWasmWorkerEvents>() as u128)
            .expect("event heap fits");
        let target_outer = final_event_heap
            .checked_sub(transport_heap)
            .and_then(|bytes| bytes.checked_sub(prefix_nested))
            .and_then(|bytes| bytes.checked_sub(progress_heap))
            .expect("target outer fits");
        let governed_result_metadata = (core::mem::size_of::<GovernedWasmExecutionResult>()
            .max(core::mem::size_of::<(
                WasmExecutionResult,
                WasmExecutionMemoryAuthority,
            )>())
            .max(core::mem::size_of::<
                Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>,
            >()) as u128)
            .checked_sub(core::mem::size_of::<WasmExecutionResult>() as u128)
            .expect("governed result metadata fits");
        let transition_peak = old_actual
            .checked_add(governed_result_metadata)
            .and_then(|bytes| {
                bytes.checked_add(
                    core::mem::size_of::<VecDeque<crate::WasmWorkerJobEvent>>().max(
                        core::mem::size_of::<Option<VecDeque<crate::WasmWorkerJobEvent>>>(),
                    ) as u128,
                )
            })
            .and_then(|bytes| bytes.checked_add(prefix_actual))
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<Vec<crate::WasmWorkerJobEvent>>() as u128)
            })
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<crate::JobProgress>() as u128))
            .and_then(|bytes| bytes.checked_add(target_outer))
            .and_then(|bytes| bytes.checked_add(progress_heap))
            .expect("event transition peak fits");
        let event_payload_heap = measured
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<crate::GovernedWasmWorkerEvents>() as u128)
            .expect("governed event payload fits");
        let returned_event_carrier = core::mem::size_of::<crate::GovernedWasmWorkerEvents>()
            .max(core::mem::size_of::<
                Result<crate::GovernedWasmWorkerEvents, WasmCommandRuntimeError>,
            >())
            .max(core::mem::size_of::<(
                Vec<crate::WasmWorkerJobEvent>,
                Option<Arc<TilingSolutionPageStore>>,
                u128,
                u128,
            )>()) as u128;
        let returned_peak = event_payload_heap
            .checked_add(returned_event_carrier)
            .expect("returned event carrier fits");
        let exact_limit = transition_peak.max(returned_peak);
        drop(measured);

        let exact = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed_result_fixture().with_memory_limit_for_test(exact_limit),
            event_prefix_fixture(),
        )
        .expect("exact event transition peak is admitted");
        drop(exact);
        let error = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed_result_fixture().with_memory_limit_for_test(exact_limit - 1),
            event_prefix_fixture(),
        )
        .expect_err("event transition peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_EVENT_MEMORY_LIMIT");
    }

    #[test]
    fn worker_event_transition_accounts_prepared_and_runtime_storage_carriers() {
        let measured_source = governed_result_fixture();
        let source_actual = measured_source.authority().actual_retained_bytes();
        assert!(measured_source
            .result()
            .tiling_solution_page_store()
            .is_none());
        let source_transport_heap = measured_source
            .result()
            .checked_transport_retained_capacity_bytes()
            .expect("worker source transport heap fits");
        let measured = crate::GovernedWasmWorkerEvents::try_from_final_result_for_worker_storage(
            crate::WasmWorkerJobId::new(17),
            measured_source,
            VecDeque::new(),
        )
        .expect("worker event measurement succeeds");
        let target_heap = measured
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<crate::GovernedWasmWorkerEvents>() as u128)
            .and_then(|bytes| bytes.checked_sub(source_transport_heap))
            .expect("worker event target heap fits");
        let source_carrier = core::mem::size_of::<GovernedWasmExecutionResult>()
            .max(core::mem::size_of::<(
                WasmExecutionResult,
                WasmExecutionMemoryAuthority,
            )>())
            .max(core::mem::size_of::<
                Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>,
            >())
            .max(core::mem::size_of::<PreparedWasmAdvance>()) as u128;
        let source_metadata = source_carrier
            .checked_sub(core::mem::size_of::<WasmExecutionResult>() as u128)
            .expect("worker source carrier metadata fits");
        let prefix_carrier =
            core::mem::size_of::<VecDeque<crate::WasmWorkerJobEvent>>().max(core::mem::size_of::<
                Option<VecDeque<crate::WasmWorkerJobEvent>>,
            >()) as u128;
        let transition_peak = source_actual
            .checked_add(source_metadata)
            .and_then(|bytes| bytes.checked_add(prefix_carrier))
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<Vec<crate::WasmWorkerJobEvent>>() as u128)
            })
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<crate::JobProgress>() as u128))
            .and_then(|bytes| bytes.checked_add(target_heap))
            .expect("worker event transition peak fits");
        let stored_peak = measured
            .checked_worker_storage_peak_bytes()
            .expect("worker storage peak fits");
        let exact_peak = transition_peak.max(stored_peak);
        drop(measured);

        let exact = crate::GovernedWasmWorkerEvents::try_from_final_result_for_worker_storage(
            crate::WasmWorkerJobId::new(17),
            governed_result_fixture().with_memory_limit_for_test(exact_peak),
            VecDeque::new(),
        )
        .expect("exact worker event transition peak is admitted");
        drop(exact);
        let error = crate::GovernedWasmWorkerEvents::try_from_final_result_for_worker_storage(
            crate::WasmWorkerJobId::new(17),
            governed_result_fixture().with_memory_limit_for_test(exact_peak - 1),
            VecDeque::new(),
        )
        .expect_err("worker event transition peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_EVENT_MEMORY_LIMIT");
    }

    #[test]
    fn governed_json_preserves_escaping_and_has_exact_peak_boundary() {
        let measured_events = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed_result_fixture(),
            event_prefix_fixture(),
        )
        .expect("event fixture");
        let source_actual = measured_events.actual_retained_bytes();
        let compatibility =
            crate::json_event_envelope::serialize_worker_events(measured_events.events())
                .expect("compatibility serializer");
        let measured_json = crate::serialize_governed_worker_events(measured_events)
            .expect("governed JSON measurement");
        assert!(measured_json
            .completed_tiling_solution_page_store()
            .is_none());
        assert_eq!(measured_json.json(), compatibility);
        assert!(measured_json
            .json()
            .contains("quote\\\" slash\\\\ newline\\n control\\u0001 한글"));
        let event_wrapper_inline = core::mem::size_of::<crate::GovernedWasmWorkerEvents>() as u128;
        let source_payload_heap = source_actual
            .checked_sub(event_wrapper_inline)
            .expect("event payload heap fits");
        let json_wrapper_inline = core::mem::size_of::<crate::GovernedWasmJson>() as u128;
        let json_payload_heap = measured_json
            .actual_retained_bytes()
            .checked_sub(json_wrapper_inline)
            .expect("JSON payload heap fits");
        let construction_peak = source_payload_heap
            .checked_add(crate::json_event_envelope::governed_event_source_carrier_inline_bytes())
            .and_then(|bytes| {
                bytes.checked_add(crate::json_event_envelope::json_build_carrier_inline_bytes())
            })
            .and_then(|bytes| bytes.checked_add(json_payload_heap))
            .expect("JSON construction peak fits");
        let returned_peak = json_payload_heap
            .checked_add(crate::json_event_envelope::governed_json_returned_carrier_inline_bytes())
            .expect("JSON return peak fits");
        let exact_peak = construction_peak.max(returned_peak);
        let compatibility_len = compatibility.len();
        let mut compatibility_hasher = DefaultHasher::new();
        compatibility.hash(&mut compatibility_hasher);
        let compatibility_hash = compatibility_hasher.finish();
        assert_eq!(measured_json.json().len(), compatibility_len);
        drop(measured_json);
        drop(compatibility);

        let exact_events = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed_result_fixture(),
            event_prefix_fixture(),
        )
        .expect("exact event fixture")
        .with_memory_limit_for_test(exact_peak);
        let exact = crate::serialize_governed_worker_events(exact_events)
            .expect("exact JSON coexistence peak is admitted");
        let mut exact_hasher = DefaultHasher::new();
        exact.json().hash(&mut exact_hasher);
        assert_eq!(exact.json().len(), compatibility_len);
        assert_eq!(exact_hasher.finish(), compatibility_hash);
        drop(exact);

        let below_events = crate::GovernedWasmWorkerEvents::try_from_final_result_with_prefix(
            crate::WasmWorkerJobId::new(7),
            governed_result_fixture(),
            event_prefix_fixture(),
        )
        .expect("below event fixture")
        .with_memory_limit_for_test(exact_peak - 1);
        let error = crate::serialize_governed_worker_events(below_events)
            .expect_err("JSON coexistence peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_JSON_MEMORY_LIMIT");
    }

    #[test]
    fn finite_string_allocation_accepts_actual_peak_and_rejects_peak_minus_one() {
        let requested = "finite-result";
        let mut probe = String::new();
        probe
            .try_reserve_exact(requested.len())
            .expect("probe allocation");
        let actual_capacity = probe.capacity() as u128;
        drop(probe);
        let source = 1_024_u128;
        let inline = core::mem::size_of::<WasmExecutionResult>() as u128;
        let exact_limit = source
            .checked_add(inline)
            .and_then(|bytes| bytes.checked_add(actual_capacity))
            .expect("test peak fits u128");

        let mut exact = WasmFiniteMemoryLedger::new(
            source,
            exact_limit,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect("exact baseline admitted");
        let value = try_owned_string(requested, &mut exact).expect("exact peak admitted");
        assert_eq!(value.capacity() as u128, actual_capacity);

        let mut below = WasmFiniteMemoryLedger::new(
            source,
            exact_limit - 1,
            WasmFiniteConversionRoute::PublicDirect,
        )
        .expect("peak-minus-one baseline admitted");
        let error = try_owned_string(requested, &mut below).expect_err("peak-minus-one rejected");
        assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
    }

    #[test]
    fn page_store_projection_workspace_accepts_exact_peak_and_rejects_peak_minus_one() {
        let source_live = 4_096_u128;
        let route = WasmFiniteConversionRoute::CooperativeAdvance;
        let workspace =
            TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes()
                .expect("projection workspace fits");
        let exact_peak = source_live
            .checked_add(
                route
                    .checked_caller_retained_bytes()
                    .expect("route caller retained bytes fit"),
            )
            .and_then(|bytes| bytes.checked_add(workspace))
            .expect("projection peak fits");

        authorize_page_store_projection_workspace(source_live, exact_peak, route)
            .expect("exact page-store projection carrier is admitted");
        let error = authorize_page_store_projection_workspace(source_live, exact_peak - 1, route)
            .expect_err("page-store projection carrier peak minus one is rejected");
        assert_eq!(error.code(), WASM_FINITE_MEMORY_LIMIT);
    }

    #[test]
    fn backend_clone_preserves_fallback_bit_independently_from_reason() {
        let source = BackendReport::from_owned_memory_authorized_parts_strict(
            "webgpu".to_owned(),
            "wasm-cpu".to_owned(),
            true,
            None,
            None,
            Some("wasm-cpu".to_owned()),
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
        );
        let mut ledger =
            WasmFiniteMemoryLedger::new(0, u128::MAX, WasmFiniteConversionRoute::PublicDirect)
                .expect("backend clone ledger");

        let cloned = try_clone_backend_report(&source, &mut ledger)
            .expect("fieldwise backend clone succeeds");

        assert!(cloned.fallback_used());
        assert_eq!(cloned.fallback_reason(), None);
        assert_eq!(cloned.fallback_backend(), Some("wasm-cpu"));
    }

    #[test]
    fn search_report_counts_nested_vector_and_string_capacities() {
        let mut path = Vec::with_capacity(3);
        path.push(WasmSearchPathStep {
            piece: allocated(32, "T"),
            rotation: 1,
            x: 2,
            y: 3,
            hold: allocated(48, "use"),
            cleared_lines: 1,
        });
        let mut solution_paths = Vec::with_capacity(2);
        solution_paths.push(path);
        let candidate = WasmSetupCandidate {
            candidate_id: allocated(48, "candidate"),
            setup_id: allocated(56, "setup"),
            board_mask: allocated(64, "0xff"),
            min_locks: 1,
            max_locks: 2,
            build_covered_patterns: 3,
            joint_covered_patterns: 4,
            build_probability: allocated(72, "1/2"),
            joint_probability: allocated(80, "1/3"),
            conditional_pc_probability: allocated(88, "2/3"),
            representative_path: Vec::new(),
            solution_path_count: 1,
            solution_paths_complete: true,
            solution_paths,
        };
        let mut candidates = Vec::with_capacity(4);
        candidates.push(candidate);
        let condition = WasmSetupHoldCondition {
            condition_id: allocated(96, "hold"),
            initial_hold: Some(allocated(104, "I")),
            pattern_expression: allocated(112, "*p7"),
            pattern_count: 7,
            candidate_count: 1,
            result_truncated: false,
            complete: true,
            candidates,
        };
        let mut hold_conditions = Vec::with_capacity(5);
        hold_conditions.push(condition);
        let setup = WasmSetupFinderReport {
            search_mode: allocated(120, "setup"),
            cycle: 1,
            remaining_pieces: allocated(128, "TIL"),
            queue_based_pieces: allocated(136, "TIL"),
            next_cycle_remaining_pieces: allocated(144, "JOSZ"),
            post_cycle_borrow_enabled: true,
            coverage_semantics: allocated(152, "exact"),
            continuation_supply_semantics: allocated(160, "projected"),
            geometry_family_count: allocated(168, "1"),
            partial_build_node_count: 4,
            complete: true,
            hold_conditions,
        };
        let independently_counted = {
            let condition = &setup.hold_conditions[0];
            let candidate = &condition.candidates[0];
            let path = &candidate.solution_paths[0];
            let step = &path[0];
            [
                setup.search_mode.capacity() as u128,
                setup.remaining_pieces.capacity() as u128,
                setup.queue_based_pieces.capacity() as u128,
                setup.next_cycle_remaining_pieces.capacity() as u128,
                setup.coverage_semantics.capacity() as u128,
                setup.continuation_supply_semantics.capacity() as u128,
                setup.geometry_family_count.capacity() as u128,
                (setup.hold_conditions.capacity() * core::mem::size_of::<WasmSetupHoldCondition>())
                    as u128,
                condition.condition_id.capacity() as u128,
                condition.initial_hold.as_ref().unwrap().capacity() as u128,
                condition.pattern_expression.capacity() as u128,
                (condition.candidates.capacity() * core::mem::size_of::<WasmSetupCandidate>())
                    as u128,
                candidate.candidate_id.capacity() as u128,
                candidate.setup_id.capacity() as u128,
                candidate.board_mask.capacity() as u128,
                candidate.build_probability.capacity() as u128,
                candidate.joint_probability.capacity() as u128,
                candidate.conditional_pc_probability.capacity() as u128,
                (candidate.solution_paths.capacity()
                    * core::mem::size_of::<Vec<WasmSearchPathStep>>()) as u128,
                (path.capacity() * core::mem::size_of::<WasmSearchPathStep>()) as u128,
                step.piece.capacity() as u128,
                step.hold.capacity() as u128,
            ]
            .into_iter()
            .try_fold(0_u128, u128::checked_add)
        };
        let mut report = WasmSearchReport::default();
        report.setup_report = Some(setup);

        assert_eq!(
            report.checked_retained_capacity_bytes(),
            independently_counted
        );
    }

    #[test]
    fn into_parts_moves_terminal_payload_without_changing_string_pointer() {
        let app_response = HostAppResponse::new(None, HostAppStatus::Success).with_result(Some(
            HostAppResult::new(allocated(160, "build-probability")),
        ));
        let pointer = app_response.result().expect("result").kind().as_ptr();
        let result = WasmExecutionResult {
            app_response,
            webgpu_backend: WebGpuBackendReport::not_requested(),
            search_report: None,
            tiling_solution_page_store: None,
            product_page_source_owner: None,
        };

        let (app_response, _, _, _, _) = result.into_parts();
        assert_eq!(
            app_response.result().expect("moved result").kind().as_ptr(),
            pointer
        );
    }

    #[test]
    fn finite_core_error_destroys_owned_message_and_returns_static_empty_error() {
        let error = finite_core_execution_error(clearra_core_executor::CoreExecutionError::Pc(
            allocated(512, "owned core detail"),
        ));

        assert_eq!(error.code(), "E_WASM_FINITE_CORE_EXECUTION");
        assert!(error.message().is_empty());
        assert!(error.resource_report().is_none());
    }
}
