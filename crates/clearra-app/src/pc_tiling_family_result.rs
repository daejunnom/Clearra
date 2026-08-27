use std::{fmt, mem::size_of, sync::Arc};

use clearra_core_domain::{
    resource::ResourceReport,
    solution::{
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_core_executor::{
    CoreExecutionResult, PcTilingMemoryAdmissionEvidence, TilingSolutionPageStore,
    WasmCpuTerminalResourceAuthority,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery};
use clearra_problem::{
    ProblemCompileError, ProblemCompiler, SearchOutputPolicy, SearchProblem, SearchProblemPreset,
};

use crate::pc_result_projection::{
    validate_pc_tiling_opening_request_contract, validate_pc_tiling_scenario_request_contract,
    PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES,
};

pub const PC_TILING_FAMILY_RESULT_CONTRACT: &str = "pc-tiling-family.v1";
pub const PC_TILING_INPUT_CONTRACT: &str = "pc-pattern.v2";
pub const PC_TILING_INITIAL_PAGE_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcTilingIngressOrigin {
    CanonicalPcTiling,
}

impl PcTilingIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcTiling => "canonical-pc-tiling",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcTilingProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcTilingProblemPreset {
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
pub enum PcTilingQuerySnapshot {
    Opening(Arc<OpeningPcSearchQuery>),
    Scenario(Arc<PcScenarioQuery>),
}

impl PcTilingQuerySnapshot {
    pub const fn problem_preset(&self) -> PcTilingProblemPreset {
        match self {
            Self::Opening(_) => PcTilingProblemPreset::OpeningPc,
            Self::Scenario(_) => PcTilingProblemPreset::ScenarioPc,
        }
    }

    pub fn opening(&self) -> Option<&OpeningPcSearchQuery> {
        match self {
            Self::Opening(query) => Some(query.as_ref()),
            Self::Scenario(_) => None,
        }
    }

    pub fn scenario(&self) -> Option<&PcScenarioQuery> {
        match self {
            Self::Scenario(query) => Some(query.as_ref()),
            Self::Opening(_) => None,
        }
    }

    /// Complete retained snapshot/query pointee bytes. The outer authority's
    /// `Arc` handle and control block are accounted separately.
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcTilingCompletenessEvidence {
    family_complete: bool,
    incomplete_reason: String,
    initial_page_complete: bool,
    initial_page_covers_family: bool,
}

impl PcTilingCompletenessEvidence {
    pub const fn family_complete(&self) -> bool {
        self.family_complete
    }

    pub fn incomplete_reason(&self) -> &str {
        &self.incomplete_reason
    }

    pub const fn initial_page_complete(&self) -> bool {
        self.initial_page_complete
    }

    pub const fn initial_page_covers_family(&self) -> bool {
        self.initial_page_covers_family
    }
}

/// Closed, immutable result for the canonical `pc tiling` product route.
///
/// The first page is cheap to serialize. The complete normalized family stays
/// in the compact Core page store and can be paged without rerunning search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcTilingFamilyV1Result {
    origin: PcTilingIngressOrigin,
    query: Arc<PcTilingQuerySnapshot>,
    problem_preset: PcTilingProblemPreset,
    normalized_solution_count: usize,
    normalized_solution_set_hash: String,
    initial_page_keys: Vec<String>,
    family_store: Arc<TilingSolutionPageStore>,
    completeness: PcTilingCompletenessEvidence,
}

impl PcTilingFamilyV1Result {
    pub const fn contract_id(&self) -> &'static str {
        PC_TILING_FAMILY_RESULT_CONTRACT
    }

    pub const fn input_contract_id(&self) -> &'static str {
        PC_TILING_INPUT_CONTRACT
    }

    pub const fn origin(&self) -> PcTilingIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcTilingQuerySnapshot {
        self.query.as_ref()
    }

    pub const fn problem_preset(&self) -> PcTilingProblemPreset {
        self.problem_preset
    }

    pub const fn normalized_solution_count(&self) -> usize {
        self.normalized_solution_count
    }

    pub fn normalized_solution_set_hash(&self) -> &str {
        &self.normalized_solution_set_hash
    }

    pub const fn normalized_solution_key_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM
    }

    pub const fn normalized_solution_set_hash_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM
    }

    pub const fn initial_page_limit(&self) -> usize {
        PC_TILING_INITIAL_PAGE_LIMIT
    }

    pub fn initial_page_keys(&self) -> &[String] {
        &self.initial_page_keys
    }

    pub const fn completeness(&self) -> &PcTilingCompletenessEvidence {
        &self.completeness
    }

    pub fn page_keys(&self, offset: usize, limit: usize) -> Result<Vec<String>, &'static str> {
        self.family_store.page_keys(offset, limit)
    }
}

pub(crate) enum PcTilingQueryBinding<'a> {
    Opening(&'a OpeningPcSearchQuery),
    Scenario(&'a PcScenarioQuery),
}

impl PcTilingQueryBinding<'_> {
    pub(crate) fn matches_snapshot(&self, snapshot: &PcTilingQuerySnapshot) -> bool {
        match (self, snapshot) {
            (Self::Opening(expected), PcTilingQuerySnapshot::Opening(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Scenario(expected), PcTilingQuerySnapshot::Scenario(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Opening(_), PcTilingQuerySnapshot::Scenario(_))
            | (Self::Scenario(_), PcTilingQuerySnapshot::Opening(_)) => false,
        }
    }
}

pub(crate) struct PcTilingCompiledAuthority {
    origin: PcTilingIngressOrigin,
    query: Arc<PcTilingQuerySnapshot>,
    problem: Arc<SearchProblem>,
    memory_authority: PcTilingExecutionMemoryAuthority,
}

enum PcTilingExecutionMemoryAuthority {
    NativeInternal,
    WasmTerminal {
        terminal_resource_authority: WasmCpuTerminalResourceAuthority,
        external_retained_base_bytes: u128,
    },
}

impl fmt::Debug for PcTilingCompiledAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PcTilingCompiledAuthority")
            .field("origin", &self.origin)
            .field("query", &self.query)
            .field(
                "memory_authority",
                &match &self.memory_authority {
                    PcTilingExecutionMemoryAuthority::NativeInternal => "native-internal",
                    PcTilingExecutionMemoryAuthority::WasmTerminal { .. } => "wasm-terminal",
                },
            )
            .finish_non_exhaustive()
    }
}

impl PcTilingCompiledAuthority {
    pub(crate) const fn execution_evidence_retained_upper_bound_bytes() -> u128 {
        (PC_TILING_INITIAL_PAGE_LIMIT as u128) * ((size_of::<String>() as u128) + 512) + 128
    }

    pub(crate) fn compile_opening(
        query: Arc<OpeningPcSearchQuery>,
        origin: PcTilingIngressOrigin,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        validate_pc_tiling_opening_request_contract(query.as_ref(), origin)
            .map_err(PcTilingCompiledAuthorityError::Contract)?;
        let problem = ProblemCompiler::compile_opening_pc_tiling(query.as_ref())
            .map_err(PcTilingCompiledAuthorityError::ProblemCompile)?;
        Self::new(
            Arc::new(PcTilingQuerySnapshot::Opening(query)),
            origin,
            Arc::new(problem),
            PcTilingExecutionMemoryAuthority::NativeInternal,
        )
    }

    pub(crate) fn compile_opening_under_terminal_authority(
        query: Arc<OpeningPcSearchQuery>,
        origin: PcTilingIngressOrigin,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        validate_pc_tiling_opening_request_contract(query.as_ref(), origin)
            .map_err(PcTilingCompiledAuthorityError::Contract)?;
        Self::compile_under_terminal_authority(
            Arc::new(PcTilingQuerySnapshot::Opening(query)),
            origin,
        )
    }

    pub(crate) fn compile_scenario(
        query: Arc<PcScenarioQuery>,
        origin: PcTilingIngressOrigin,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        validate_pc_tiling_scenario_request_contract(query.as_ref(), origin)
            .map_err(PcTilingCompiledAuthorityError::Contract)?;
        let problem = ProblemCompiler::compile_scenario_pc_tiling(query.as_ref())
            .map_err(PcTilingCompiledAuthorityError::ProblemCompile)?;
        Self::new(
            Arc::new(PcTilingQuerySnapshot::Scenario(query)),
            origin,
            Arc::new(problem),
            PcTilingExecutionMemoryAuthority::NativeInternal,
        )
    }

    pub(crate) fn compile_scenario_under_terminal_authority(
        query: Arc<PcScenarioQuery>,
        origin: PcTilingIngressOrigin,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        validate_pc_tiling_scenario_request_contract(query.as_ref(), origin)
            .map_err(PcTilingCompiledAuthorityError::Contract)?;
        Self::compile_under_terminal_authority(
            Arc::new(PcTilingQuerySnapshot::Scenario(query)),
            origin,
        )
    }

    fn compile_under_terminal_authority(
        query: Arc<PcTilingQuerySnapshot>,
        origin: PcTilingIngressOrigin,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        let query_retained_bytes = (size_of::<Arc<PcTilingQuerySnapshot>>() as u128)
            .checked_add(query.checked_pointee_retained_bytes().ok_or(
                PcTilingCompiledAuthorityError::Contract(
                    "pc_tiling_query_retained_projection_unavailable",
                ),
            )?)
            .ok_or(PcTilingCompiledAuthorityError::Contract(
                "pc_tiling_query_retained_projection_unavailable",
            ))?;
        if query_retained_bytes > PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(PcTilingCompiledAuthorityError::Contract(
                "pc_tiling_query_retained_envelope_exceeded",
            ));
        }
        let terminal_resource_authority =
            WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
                .map_err(PcTilingCompiledAuthorityError::ResourceAdmission)?;
        let problem = match query.as_ref() {
            PcTilingQuerySnapshot::Opening(query) => {
                ProblemCompiler::compile_opening_pc_tiling(query.as_ref())
            }
            PcTilingQuerySnapshot::Scenario(query) => {
                ProblemCompiler::compile_scenario_pc_tiling(query.as_ref())
            }
        }
        .map(Arc::new)
        .map_err(PcTilingCompiledAuthorityError::ProblemCompile)?;
        let problem_retained_bytes = problem.checked_pc_tiling_pointee_retained_bytes().ok_or(
            PcTilingCompiledAuthorityError::Contract(
                "pc_tiling_problem_retained_projection_unavailable",
            ),
        )?;
        let external_retained_base_bytes = (size_of::<Self>() as u128)
            .checked_add(query.checked_pointee_retained_bytes().ok_or(
                PcTilingCompiledAuthorityError::Contract(
                    "pc_tiling_query_retained_projection_unavailable",
                ),
            )?)
            .and_then(|bytes| bytes.checked_add(problem_retained_bytes))
            .ok_or(PcTilingCompiledAuthorityError::Contract(
                "pc_tiling_external_retained_projection_unavailable",
            ))?;
        if external_retained_base_bytes > PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(PcTilingCompiledAuthorityError::Contract(
                "pc_tiling_external_retained_envelope_exceeded",
            ));
        }
        Self::new(
            query,
            origin,
            problem,
            PcTilingExecutionMemoryAuthority::WasmTerminal {
                terminal_resource_authority,
                external_retained_base_bytes,
            },
        )
    }

    fn new(
        query: Arc<PcTilingQuerySnapshot>,
        origin: PcTilingIngressOrigin,
        problem: Arc<SearchProblem>,
        memory_authority: PcTilingExecutionMemoryAuthority,
    ) -> Result<Self, PcTilingCompiledAuthorityError> {
        let expected_execution_policy = match query.as_ref() {
            PcTilingQuerySnapshot::Opening(query) => query.execution_policy(),
            PcTilingQuerySnapshot::Scenario(query) => query.execution_policy(),
        };
        if problem.preset() != query.problem_preset().search_problem_preset()
            || problem.output_policy() != SearchOutputPolicy::TilingOnly
            || problem.goal().as_str() != "clear-to-empty"
            || problem.objective() != ObjectivePolicy::tiling()
            || problem.solution_probability_policy().requested()
            || problem.allowed_colored_solution_identities().is_some()
            || problem.backend_request() != expected_execution_policy
        {
            return Err(PcTilingCompiledAuthorityError::Contract(
                "pc tiling compiled problem contract mismatch",
            ));
        }
        if matches!(query.as_ref(), PcTilingQuerySnapshot::Scenario(_))
            && problem.count_policy() != PcCountPolicy::CountUnique
        {
            return Err(PcTilingCompiledAuthorityError::Contract(
                "pc tiling scenario compiler did not preserve unique family counting",
            ));
        }
        Ok(Self {
            origin,
            query,
            problem,
            memory_authority,
        })
    }

    pub(crate) fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub(crate) fn problem_arc(&self) -> Arc<SearchProblem> {
        Arc::clone(&self.problem)
    }

    pub(crate) fn terminal_resource_authority(&self) -> Option<&WasmCpuTerminalResourceAuthority> {
        match &self.memory_authority {
            PcTilingExecutionMemoryAuthority::NativeInternal => None,
            PcTilingExecutionMemoryAuthority::WasmTerminal {
                terminal_resource_authority,
                ..
            } => Some(terminal_resource_authority),
        }
    }

    pub(crate) fn checked_external_retained_upper_bound_bytes(
        &self,
        concurrent_additional_bytes: u128,
    ) -> Result<u128, PcTilingExecutionError> {
        let PcTilingExecutionMemoryAuthority::WasmTerminal {
            external_retained_base_bytes,
            ..
        } = &self.memory_authority
        else {
            return Err(rejected("pc_tiling_terminal_authority_missing"));
        };
        let retained = (*external_retained_base_bytes)
            .checked_add(concurrent_additional_bytes)
            .ok_or_else(|| rejected("pc_tiling_external_retained_projection_unavailable"))?;
        if retained > PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES {
            return Err(rejected("pc_tiling_external_retained_envelope_exceeded"));
        }
        Ok(PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES)
    }

    pub(crate) fn validate_execution_result(
        &self,
        executed_problem: &SearchProblem,
        result: &CoreExecutionResult,
    ) -> Result<ValidatedPcTilingExecutionEvidence, PcTilingExecutionError> {
        if !core::ptr::eq(executed_problem, self.problem.as_ref()) {
            return Err(rejected("pc tiling executed problem authority mismatch"));
        }
        let expected_memory_evidence = match &self.memory_authority {
            PcTilingExecutionMemoryAuthority::NativeInternal => {
                PcTilingMemoryAdmissionEvidence::NativeInternal
            }
            PcTilingExecutionMemoryAuthority::WasmTerminal { .. } => {
                PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority
            }
        };
        if result.pc_tiling_memory_admission_evidence() != Some(expected_memory_evidence) {
            return Err(rejected(
                "pc tiling materialization memory authority evidence mismatch",
            ));
        }
        if !result.pc_tiling_family_publication_contract_is_valid() {
            return Err(rejected("pc tiling family publication contract mismatch"));
        }
        let report = validate_complete_result(
            result,
            self.origin,
            Arc::clone(&self.query),
            self.query.problem_preset(),
        )?;
        Ok(ValidatedPcTilingExecutionEvidence { report })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcTilingExecutionEvidence {
    report: PcTilingFamilyV1Result,
}

impl ValidatedPcTilingExecutionEvidence {
    pub(crate) fn report(&self) -> &PcTilingFamilyV1Result {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        let Some(store) = result.tiling_solution_page_store() else {
            return false;
        };
        Arc::ptr_eq(store, &self.report.family_store)
            && result.normalized_solution_keys() == self.report.initial_page_keys()
            && result.usize_field("normalized_unique_solution_count")
                == Some(self.report.normalized_solution_count())
            && result.field("normalized_solution_set_hash")
                == Some(self.report.normalized_solution_set_hash())
            && result.bool_field("tiling_family_complete") == Some(true)
            && result.field("tiling_family_incomplete_reason") == Some("none")
    }
}

fn validate_complete_result(
    result: &CoreExecutionResult,
    origin: PcTilingIngressOrigin,
    query: Arc<PcTilingQuerySnapshot>,
    problem_preset: PcTilingProblemPreset,
) -> Result<PcTilingFamilyV1Result, PcTilingExecutionError> {
    require_field(result, "problem_preset", problem_preset.as_str())?;
    require_field(result, "compiled_goal", "clear-to-empty")?;
    require_field(result, "search_output_policy", "tiling-only")?;
    require_field(
        result,
        "actual_solution_set_contract",
        "normalized-tiling-set",
    )?;
    require_field(
        result,
        "normalized_solution_key_algorithm",
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
    )?;
    require_field(
        result,
        "normalized_solution_set_hash_algorithm",
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    )?;
    for key in [
        "packing_source_raw_geometry",
        "tiling_objective_canonical",
        "tiling_materialization_memory_admission_accounted",
        "tiling_materialization_complete",
        "tiling_family_complete",
        "tiling_initial_page_complete",
        "count_complete",
        "solution_set_materialized",
    ] {
        require_bool(result, key, true)?;
    }
    for key in [
        "packing_source_buildability_preverified",
        "buildup_executed",
        "additional_buildup_executed",
        "buildability_verified",
        "coverage_calculated",
        "probability_calculated",
        "resource_truncated",
        "solution_probabilities_requested",
    ] {
        require_bool(result, key, false)?;
    }
    require_field(result, "tiling_materialization_incomplete_reason", "none")?;
    require_field(result, "tiling_family_incomplete_reason", "none")?;
    require_field(result, "resource_truncation_reason", "none")?;
    require_field(result, "count_truncated_reason", "none")?;

    let store = result
        .tiling_solution_page_store()
        .cloned()
        .ok_or_else(|| rejected("pc tiling family page store missing"))?;
    let normalized_solution_count = store.len();
    let expected_initial_page_count = normalized_solution_count.min(PC_TILING_INITIAL_PAGE_LIMIT);
    let expected_initial_page = store
        .page_keys(0, expected_initial_page_count)
        .map_err(rejected)?;
    if result.normalized_solution_keys() != expected_initial_page.as_slice()
        || result.usize_field("normalized_unique_solution_count") != Some(normalized_solution_count)
        || result.usize_field("actual_normalized_unique_solution_count")
            != Some(normalized_solution_count)
        || result.usize_field("total_solution_count") != Some(normalized_solution_count)
        || result.usize_field("unique_solution_count") != Some(normalized_solution_count)
        || result.usize_field("solution_keys_materialized_count")
            != Some(expected_initial_page_count)
        || result.usize_field("tiling_initial_page_count") != Some(expected_initial_page_count)
        || result.bool_field("tiling_initial_page_covers_family")
            != Some(expected_initial_page_count == normalized_solution_count)
        || result.bool_field("solution_page_available")
            != Some(expected_initial_page_count < normalized_solution_count)
        || result.bool_field("solution_keys_complete")
            != Some(expected_initial_page_count == normalized_solution_count)
        || result.field("normalized_solution_set_hash") != Some(store.normalized_hash())
        || result.field("actual_normalized_solution_set_hash") != Some(store.normalized_hash())
    {
        return Err(rejected(
            "pc tiling family count, hash, or initial page mismatch",
        ));
    }

    Ok(PcTilingFamilyV1Result {
        origin,
        query,
        problem_preset,
        normalized_solution_count,
        normalized_solution_set_hash: store.normalized_hash().to_owned(),
        initial_page_keys: expected_initial_page,
        family_store: store,
        completeness: PcTilingCompletenessEvidence {
            family_complete: true,
            incomplete_reason: "none".to_owned(),
            initial_page_complete: true,
            initial_page_covers_family: expected_initial_page_count == normalized_solution_count,
        },
    })
}

fn require_field(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: &str,
) -> Result<(), PcTilingExecutionError> {
    if result.field(key) == Some(expected) {
        Ok(())
    } else {
        Err(rejected(key))
    }
}

fn require_bool(
    result: &CoreExecutionResult,
    key: &'static str,
    expected: bool,
) -> Result<(), PcTilingExecutionError> {
    if result.bool_field(key) == Some(expected) {
        Ok(())
    } else {
        Err(rejected(key))
    }
}

fn rejected(reason: &'static str) -> PcTilingExecutionError {
    PcTilingExecutionError::ContractRejected(reason)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcTilingCompiledAuthorityError {
    ResourceAdmission(ResourceReport),
    ProblemCompile(ProblemCompileError),
    Contract(&'static str),
}

impl fmt::Display for PcTilingCompiledAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceAdmission(_) => {
                formatter.write_str("pc tiling resource admission failed")
            }
            Self::ProblemCompile(error) => write!(formatter, "{error:?}"),
            Self::Contract(reason) => formatter.write_str(reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcTilingExecutionError {
    ContractRejected(&'static str),
}

impl fmt::Display for PcTilingExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContractRejected(reason) => formatter.write_str(reason),
        }
    }
}

impl PcTilingExecutionError {
    pub(crate) const fn component(&self) -> &'static str {
        match self {
            Self::ContractRejected(reason) => reason,
        }
    }
}
