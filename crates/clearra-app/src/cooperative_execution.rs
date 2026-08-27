use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    CoreExecutionError, WasmBuildProbabilityAdvance, WasmBuildProbabilitySession,
    WasmCpuSearchAdvance, WasmCpuSearchError, WasmCpuSearchSession, WasmSetupSearchAdvance,
    WasmSetupSearchSession,
};
use clearra_forward_search::{
    ForwardSearchAdvance, ForwardSearchError, ForwardSearchReport, ForwardSearchSession,
};
use clearra_host_contract::{AppCommandKind, BackendReport, ResourceBudget};
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    BuildSolutionProbabilityPolicy, FiniteScenarioPcCompileBudget, FiniteScenarioPcCompileError,
    ProblemCompiler, SetupSearchQuery,
};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::{AppCommand, RunnableAppCommand},
    app_context::AppContext,
    app_error::{AppError, AppErrorCode},
    app_request::{AppExecutionParts, AppOutputPolicy, AppRequest},
    app_response::{
        try_finite_build_success_response, AppResponse, AppStatus, FiniteBuildMemoryPhase,
        GovernedAppResponse,
    },
    build_solution_probability_result::build_probability_response_is_authorized,
    commands::{
        core_execution_error_response, path_app_command::path_response,
        setup_app_command::setup_success_response,
    },
    pc_allspin_result::project_pc_allspin_result,
    pc_chance_probability_result::PcChanceCompiledAuthority,
    pc_result_projection::ValidatedPcResultProjection,
    pc_save_result::{PcSaveCompiledAuthority, PcSaveCompiledAuthorityError},
    pc_score_summary_result::{
        PcScoreCompiledAuthority, PcScoreCompiledAuthorityError, PcScoreExecutionError,
    },
    pc_tiling_family_result::{PcTilingCompiledAuthority, PcTilingCompiledAuthorityError},
    product_capability_contract::{ProductCapabilityContract, ValidatedProductCapabilityContract},
    render::AppRenderModel,
};

#[derive(Debug, Eq, PartialEq)]
pub enum CooperativeAppAdvance {
    Pending,
    Progress,
    Completed(AppResponse),
    CompletedGoverned(GovernedAppResponse),
    FailedFinite(CoreExecutionError),
    Cancelled,
}

/// The caller-owned memory tranche admitted to a finite cooperative session.
///
/// This value is deliberately non-`Clone`: once admitted, the execution owns
/// the exact tranche until the next explicit advance.  Construction keeps the
/// generation and byte fields private so callers cannot bypass the checked
/// entry points accidentally. `retained_owner_bytes` excludes the typed
/// `AppRequest` and every App execution owner: App measures those itself, so a
/// transport must report only its own still-live retained allocation here.
/// `returned_carrier_bytes` is the complete outer return carrier; App combines
/// it with its own advance carrier by `max`, because the carriers overlap.
#[derive(Debug, Eq, PartialEq)]
pub struct FiniteCooperativeCallerMemory {
    generation: u64,
    retained_owner_bytes: u128,
    returned_carrier_bytes: u128,
}

impl FiniteCooperativeCallerMemory {
    fn try_new(
        generation: u64,
        retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, CoreExecutionError> {
        retained_owner_bytes
            .checked_add(returned_carrier_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_caller_memory_bytes_overflow",
            })?;
        Ok(Self {
            generation,
            retained_owner_bytes,
            returned_carrier_bytes,
        })
    }

    pub fn start(
        retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, CoreExecutionError> {
        Self::try_new(0, retained_owner_bytes, returned_carrier_bytes)
    }

    pub fn next(
        self,
        retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, (CoreExecutionError, Self)> {
        let next_generation = match self.generation.checked_add(1) {
            Some(generation) => generation,
            None => {
                return Err((
                    CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_caller_memory_generation_overflow",
                    },
                    self,
                ))
            }
        };
        match Self::try_new(
            next_generation,
            retained_owner_bytes,
            returned_carrier_bytes,
        ) {
            Ok(next) => Ok(next),
            Err(error) => Err((error, self)),
        }
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn retained_owner_bytes(&self) -> u128 {
        self.retained_owner_bytes
    }

    pub const fn returned_carrier_bytes(&self) -> u128 {
        self.returned_carrier_bytes
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum FiniteCooperativeCallerMemoryRejection {
    Missing {
        expected_generation: u64,
    },
    GenerationOverflow {
        generation: u64,
        caller_memory: Option<FiniteCooperativeCallerMemory>,
    },
    Invalid {
        error: CoreExecutionError,
        caller_memory: FiniteCooperativeCallerMemory,
    },
}

impl FiniteCooperativeCallerMemoryRejection {
    pub const fn caller_memory(&self) -> Option<&FiniteCooperativeCallerMemory> {
        match self {
            Self::Missing { .. } => None,
            Self::GenerationOverflow { caller_memory, .. } => caller_memory.as_ref(),
            Self::Invalid { caller_memory, .. } => Some(caller_memory),
        }
    }
}

pub struct CooperativeAppExecution {
    context: Option<AppContext>,
    state: CooperativeExecutionState,
    finite_caller_memory: Option<FiniteCooperativeCallerMemory>,
    finite_caller_generation: Option<u64>,
}

enum CooperativeExecutionState {
    Immediate(Option<AppExecutionParts>),
    Ready(Option<AppResponse>),
    Search(CooperativeSearchExecution),
    Postprocess(CooperativePostprocessExecution),
    FiniteRequiresCallerMemory,
    Finished,
}

struct CooperativeSearchExecution {
    session: CooperativeSearchSession,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    backend_requested: String,
    gpu_device_requested: Option<String>,
    resource_budget: ResourceBudget,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

struct CooperativePostprocessExecution {
    result: Option<clearra_core_executor::CoreExecutionResult>,
    pc_score_session: Option<WasmCpuSearchSession>,
    build_probability_session: Option<WasmBuildProbabilitySession>,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    resource_budget: ResourceBudget,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

/// Exact App-owned graph that remains live while the consumed scenario query
/// is handed to the finite compiler. Keeping these owners in one concrete
/// value makes inline padding part of the measured contract and prevents the
/// moved query or already-dropped request strings from being counted twice.
struct FiniteBuildCompileRemainder {
    output_policy: AppOutputPolicy,
    resource_budget: ResourceBudget,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse: BuildProbabilityFinesseRequest,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    caller_memory: FiniteCooperativeCallerMemory,
}

impl FiniteBuildCompileRemainder {
    fn checked_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.output_policy.checked_retained_capacity_bytes()?)?
            .checked_add(self.finesse.checked_retained_capacity_bytes()?)?
            .checked_add(self.caller_memory.retained_owner_bytes())
    }

    fn returned_carrier_bytes(&self) -> u128 {
        checked_finite_build_compile_returned_carrier_bytes(
            self.caller_memory.returned_carrier_bytes(),
        )
    }

    fn into_caller_memory(self) -> FiniteCooperativeCallerMemory {
        self.caller_memory
    }

    fn into_parts(
        self,
    ) -> (
        AppOutputPolicy,
        ResourceBudget,
        BuildProbabilityField,
        BuildProbabilityAggregation,
        BuildProbabilityFinesseRequest,
        BuildSolutionProbabilityPolicy,
        FiniteCooperativeCallerMemory,
    ) {
        (
            self.output_policy,
            self.resource_budget,
            self.field,
            self.aggregation,
            self.finesse,
            self.solution_probability_policy,
            self.caller_memory,
        )
    }
}

enum CooperativeSearchSession {
    Pc(WasmCpuSearchSession),
    Setup(WasmSetupSearchSession),
    BuildProbability(WasmBuildProbabilitySession),
    Forward(ForwardSearchSession),
}

const FORWARD_SEARCH_COOPERATIVE_WORK_BUDGET: usize = 256;
const MIB_BYTES: u128 = 1024 * 1024;

fn checked_cooperative_request_memory_limit_bytes(
    resource_budget: ResourceBudget,
) -> Result<Option<u128>, CoreExecutionError> {
    resource_budget
        .max_memory_mib()
        .map(|mib| {
            u128::from(mib)
                .checked_mul(MIB_BYTES)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_request_memory_limit_overflow",
                })
        })
        .transpose()
}

fn checked_finite_build_request_entry_bytes(
    request_retained_capacity_bytes: u128,
    caller_retained_owner_bytes: u128,
) -> Option<u128> {
    (core::mem::size_of::<AppRequest>() as u128)
        .checked_add(request_retained_capacity_bytes)?
        .checked_add(caller_retained_owner_bytes)
}

fn checked_string_retained_capacity_bytes(value: &String) -> Option<u128> {
    (value.capacity() as u128).checked_mul(core::mem::size_of::<u8>() as u128)
}

fn checked_optional_string_retained_capacity_bytes(value: Option<&String>) -> Option<u128> {
    match value {
        Some(value) => checked_string_retained_capacity_bytes(value),
        None => Some(0),
    }
}

fn checked_finite_build_response_kind_retained_bytes(
    response_kind: &CooperativeSearchResponseKind,
) -> Option<u128> {
    match response_kind {
        CooperativeSearchResponseKind::BuildProbability { finesse, .. }
            if finesse.score().is_none() =>
        {
            finesse.checked_retained_capacity_bytes()
        }
        _ => None,
    }
}

fn checked_finite_build_search_app_heap_bytes(
    response_kind: &CooperativeSearchResponseKind,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
    backend_requested: &String,
    gpu_device_requested: Option<&String>,
) -> Option<u128> {
    output_policy
        .checked_retained_capacity_bytes()?
        .checked_add(validation_report.checked_retained_capacity_bytes()?)?
        .checked_add(checked_string_retained_capacity_bytes(backend_requested)?)?
        .checked_add(checked_optional_string_retained_capacity_bytes(
            gpu_device_requested,
        )?)?
        .checked_add(checked_finite_build_response_kind_retained_bytes(
            response_kind,
        )?)
}

fn checked_finite_build_constructor_external_retained_bytes(
    problem: &clearra_problem::SearchProblem,
    response_kind: &CooperativeSearchResponseKind,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
    backend_requested: &String,
    gpu_device_requested: Option<&String>,
    caller_retained_owner_bytes: u128,
) -> Option<u128> {
    caller_retained_owner_bytes
        .checked_add(core::mem::size_of::<CooperativeAppExecution>() as u128)?
        .checked_add(core::mem::size_of::<(
            Arc<clearra_problem::SearchProblem>,
            CooperativeSearchResponseKind,
        )>() as u128)?
        .checked_add(problem.checked_build_probability_pointee_retained_bytes()?)?
        .checked_add(checked_finite_build_search_app_heap_bytes(
            response_kind,
            output_policy,
            validation_report,
            backend_requested,
            gpu_device_requested,
        )?)
}

fn checked_finite_build_search_advance_external_retained_bytes(
    search: &CooperativeSearchExecution,
    caller_retained_owner_bytes: u128,
) -> Option<u128> {
    caller_retained_owner_bytes
        .checked_add(core::mem::size_of::<CooperativeAppExecution>() as u128)?
        .checked_add(core::mem::size_of::<CooperativeSearchExecution>() as u128)?
        .checked_add(checked_finite_build_search_app_heap_bytes(
            &search.response_kind,
            &search.output_policy,
            &search.validation_report,
            &search.backend_requested,
            search.gpu_device_requested.as_ref(),
        )?)
}

fn checked_finite_build_returned_carrier_bytes(caller_returned_carrier_bytes: u128) -> u128 {
    caller_returned_carrier_bytes.max(core::mem::size_of::<CooperativeAppAdvance>() as u128)
}

fn checked_finite_build_compile_returned_carrier_bytes(
    caller_returned_carrier_bytes: u128,
) -> u128 {
    caller_returned_carrier_bytes.max(core::mem::size_of::<
        Result<CooperativeAppExecution, FiniteCooperativeCallerMemoryRejection>,
    >() as u128)
}

fn finite_scenario_pc_compile_error(error: FiniteScenarioPcCompileError) -> CoreExecutionError {
    let component = match error {
        FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape => {
            "finite_cooperative_problem_compile_shape_unavailable"
        }
        FiniteScenarioPcCompileError::ProjectionOverflow => {
            "finite_cooperative_problem_compile_projection_overflow"
        }
        FiniteScenarioPcCompileError::FiniteSupplyAllocation(_) => {
            "finite_cooperative_problem_compile_allocation_failed"
        }
        FiniteScenarioPcCompileError::PieceSourceMaterialization(_) => {
            "finite_cooperative_problem_compile_piece_source_failed"
        }
        FiniteScenarioPcCompileError::RetainedMemoryMeasurementUnavailable => {
            "finite_cooperative_problem_compile_retained_measurement_unavailable"
        }
        FiniteScenarioPcCompileError::RetainedMemoryAccountingMismatch { .. } => {
            "finite_cooperative_problem_compile_retained_accounting_mismatch"
        }
        FiniteScenarioPcCompileError::ProblemIdLengthMismatch { .. }
        | FiniteScenarioPcCompileError::ProblemIdAuthorizedLengthExceeded { .. }
        | FiniteScenarioPcCompileError::ProblemIdAllocatedCapacityExceeded { .. } => {
            "finite_cooperative_problem_compile_problem_id_invariant_failed"
        }
        FiniteScenarioPcCompileError::MemoryCapacityExceeded { .. } => {
            "finite_cooperative_problem_compile_memory_budget_exceeded"
        }
        FiniteScenarioPcCompileError::ProblemCompile(_) => {
            "finite_cooperative_problem_compile_failed"
        }
    };
    CoreExecutionError::RuntimeUnavailable { component }
}

fn checked_finite_build_materialization_external_retained_bytes(
    postprocess: &CooperativePostprocessExecution,
    caller_retained_owner_bytes: u128,
) -> Option<u128> {
    // `advance` has moved the postprocess state out of the outer execution, so
    // both inline owners coexist while the session materializes the public
    // Core result. The session authority accounts for its own heap and the
    // stage result; this is the exact App-owned remainder still live beside it.
    caller_retained_owner_bytes
        .checked_add(core::mem::size_of::<CooperativeAppExecution>() as u128)?
        .checked_add(core::mem::size_of::<CooperativePostprocessExecution>() as u128)?
        .checked_add(
            postprocess
                .output_policy
                .checked_retained_capacity_bytes()?,
        )?
        .checked_add(
            postprocess
                .validation_report
                .checked_retained_capacity_bytes()?,
        )
}

fn validate_finite_cooperative_memory_requirement(
    required_bytes: u128,
    request_memory_limit_bytes: u128,
    component: &'static str,
) -> Result<(), CoreExecutionError> {
    if required_bytes > request_memory_limit_bytes {
        return Err(CoreExecutionError::RuntimeUnavailable { component });
    }
    Ok(())
}

fn validate_finite_build_advance_memory(
    state: &CooperativeExecutionState,
    caller_retained_owner_bytes: u128,
    caller_returned_carrier_bytes: u128,
) -> Result<(), CoreExecutionError> {
    let returned_carrier_bytes =
        checked_finite_build_returned_carrier_bytes(caller_returned_carrier_bytes);
    match state {
        CooperativeExecutionState::Search(search) => {
            let external_retained_owner_bytes =
                checked_finite_build_search_advance_external_retained_bytes(
                    search,
                    caller_retained_owner_bytes,
                )
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_search_memory_projection_overflow",
                })?;
            let request_memory_limit_bytes = checked_cooperative_request_memory_limit_bytes(
                search.resource_budget,
            )?
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_advance_requires_finite_session",
            })?;
            let returned_required_bytes = external_retained_owner_bytes
                .checked_add(returned_carrier_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_search_memory_projection_overflow",
                })?;
            validate_finite_cooperative_memory_requirement(
                returned_required_bytes,
                request_memory_limit_bytes,
                "finite_cooperative_search_return_memory_budget_exceeded",
            )?;
            let CooperativeSearchSession::BuildProbability(session) = &search.session else {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_search_session_mismatch",
                });
            };
            session
                .validate_finite_advance_memory(
                    external_retained_owner_bytes,
                    returned_carrier_bytes,
                )
                .map_err(WasmCpuSearchError::into_core_execution_error)
        }
        CooperativeExecutionState::Postprocess(postprocess) => {
            if postprocess.product_capability_contract.is_some() {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_finite_build_product_capability_authority_unavailable",
                });
            }
            if !matches!(
                &postprocess.response_kind,
                CooperativeSearchResponseKind::BuildProbability { finesse, .. }
                    if finesse.score().is_none()
            ) {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_finite_build_response_kind_mismatch",
                });
            }
            let request_memory_limit_bytes =
                checked_cooperative_request_memory_limit_bytes(postprocess.resource_budget)?
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_advance_requires_finite_session",
                    })?;
            let external_retained_bytes =
                checked_finite_build_materialization_external_retained_bytes(
                    postprocess,
                    caller_retained_owner_bytes,
                )
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_finite_build_external_memory_projection_overflow",
                })?;
            validate_finite_cooperative_memory_requirement(
                external_retained_bytes,
                request_memory_limit_bytes,
                "cooperative_finite_build_external_memory_budget_exceeded",
            )?;
            let session = postprocess.build_probability_session.as_ref().ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_finite_build_session_authority_missing",
                },
            )?;
            let result =
                postprocess
                    .result
                    .as_ref()
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "cooperative_finite_build_result_missing",
                    })?;
            session
                .validate_public_result_memory_with_finite_caller_memory(
                    result,
                    0,
                    external_retained_bytes,
                    returned_carrier_bytes,
                )
                .map_err(WasmCpuSearchError::into_core_execution_error)
        }
        CooperativeExecutionState::Immediate(_)
        | CooperativeExecutionState::Ready(_)
        | CooperativeExecutionState::FiniteRequiresCallerMemory
        | CooperativeExecutionState::Finished => Err(CoreExecutionError::RuntimeUnavailable {
            component: "finite_cooperative_advance_requires_active_build_state",
        }),
    }
}

fn checked_finite_build_stage_required_bytes(
    result: &clearra_core_executor::CoreExecutionResult,
    checked_future_bytes: u128,
    external_retained_bytes: u128,
    returned_carrier_bytes: u128,
) -> Option<u128> {
    let result_or_reported_peak = result
        .checked_resource_retained_bytes()?
        .max(result.usize_field("resource_peak_cpu_bytes").unwrap_or(0) as u128);
    result_or_reported_peak
        .checked_add(checked_future_bytes)?
        .checked_add(external_retained_bytes)?
        .checked_add(returned_carrier_bytes)
}

fn finite_cooperative_completion_error(error: CoreExecutionError) -> CoreExecutionError {
    drop(error);
    CoreExecutionError::RuntimeUnavailable {
        component: "cooperative_finite_build_result_unavailable",
    }
}

fn after_dropping_producer_owners<Owners, Output>(
    owners: Owners,
    next: impl FnOnce() -> Output,
) -> Output {
    drop(owners);
    next()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CooperativePcScoreProduct {
    Summary,
    ScoreFinder,
    Portfolio,
}

impl CooperativePcScoreProduct {
    const fn capability(self) -> ProductCapabilityContract {
        match self {
            Self::Summary => ProductCapabilityContract::PcScore,
            Self::ScoreFinder => ProductCapabilityContract::PcScoreFinder,
            Self::Portfolio => ProductCapabilityContract::PcScoreMinimals,
        }
    }
}

pub(crate) enum CooperativeSearchResponseKind {
    Pc(ValidatedPcResultProjection),
    PcChance {
        authority: PcChanceCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    PcScore {
        authority: PcScoreCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
        product: CooperativePcScoreProduct,
    },
    PcTiling {
        authority: PcTilingCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    PcSave {
        authority: PcSaveCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    Path(OpeningPcSearchQuery),
    Scenario {
        render_contract: Option<crate::commands::ScenarioAppRenderContract>,
        result_projection: ValidatedPcResultProjection,
    },
    ScenarioChance {
        authority: PcChanceCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    ScenarioScore {
        authority: PcScoreCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
        product: CooperativePcScoreProduct,
    },
    ScenarioTiling {
        authority: PcTilingCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    ScenarioSave {
        authority: PcSaveCompiledAuthority,
        expected_problem: Arc<clearra_problem::SearchProblem>,
    },
    Setup(SetupSearchQuery),
    BuildProbability {
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
    },
    Damage,
    SpinFinder,
    Ren,
}

impl AppContext {
    pub fn start_cooperative_execution(&self, request: AppRequest) -> CooperativeAppExecution {
        if request.resource_budget().max_memory_mib().is_some() {
            return CooperativeAppExecution {
                context: Some(self.clone()),
                state: CooperativeExecutionState::FiniteRequiresCallerMemory,
                finite_caller_memory: None,
                finite_caller_generation: None,
            };
        }
        self.start_cooperative_execution_inner(request)
    }

    pub fn start_finite_cooperative_execution(
        &self,
        request: AppRequest,
        caller_memory: FiniteCooperativeCallerMemory,
    ) -> Result<CooperativeAppExecution, FiniteCooperativeCallerMemoryRejection> {
        if request.resource_budget().max_memory_mib().is_none() {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_entry_requires_finite_request",
                },
                caller_memory,
            });
        }
        if caller_memory.generation() != 0 {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_start_generation_must_be_zero",
                },
                caller_memory,
            });
        }

        let request_memory_limit_bytes =
            match checked_cooperative_request_memory_limit_bytes(request.resource_budget()) {
                Ok(Some(limit)) => limit,
                Ok(None) => {
                    return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error: CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_entry_requires_finite_request",
                        },
                        caller_memory,
                    })
                }
                Err(error) => {
                    return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error,
                        caller_memory,
                    })
                }
            };
        let AppCommand::BuildProbability(build_command) = request.command() else {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_command_authority_unavailable",
                },
                caller_memory,
            });
        };
        if build_command.query().finesse_score().is_some() {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_finesse_score_authority_unavailable",
                },
                caller_memory,
            });
        }
        if !self
            .services()
            .core_executor()
            .supports_cooperative_wasm_search()
        {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_core_authority_unavailable",
                },
                caller_memory,
            });
        }
        if crate::commands::build_probability_app_command::invalid_query_reason(
            build_command.query(),
        )
        .is_some()
        {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_build_request_invalid",
                },
                caller_memory,
            });
        }
        let request_retained_capacity_bytes =
            match request.checked_build_probability_retained_capacity_bytes() {
                Some(bytes) => bytes,
                None => {
                    return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error: CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_request_retained_projection_unavailable",
                        },
                        caller_memory,
                    })
                }
            };
        let request_entry_bytes = match checked_finite_build_request_entry_bytes(
            request_retained_capacity_bytes,
            caller_memory.retained_owner_bytes(),
        ) {
            Some(bytes) => bytes,
            None => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_request_entry_projection_overflow",
                    },
                    caller_memory,
                })
            }
        };
        if let Err(error) = validate_finite_cooperative_memory_requirement(
            request_entry_bytes,
            request_memory_limit_bytes,
            "finite_cooperative_request_entry_memory_budget_exceeded",
        ) {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error,
                caller_memory,
            });
        }

        let execution_parts = match request.into_execution_parts() {
            Ok(parts) => parts,
            Err(_) => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_product_capability_binding_invalid",
                    },
                    caller_memory,
                })
            }
        };
        let (
            command,
            output_policy,
            resource_budget,
            language,
            file_policy,
            product_capability_contract,
        ) = execution_parts;
        debug_assert!(product_capability_contract.is_none());
        drop((language, file_policy, product_capability_contract));
        let AppCommand::BuildProbability(command) = command else {
            unreachable!("finite Build command was checked before consuming the request")
        };
        let command_kind = AppCommandKind::BuildProbability;
        let (core_query, field, aggregation, finesse, solution_probability_policy) =
            command.into_query().into_finite_compile_parts();
        let remainder = FiniteBuildCompileRemainder {
            output_policy,
            resource_budget,
            field,
            aggregation,
            finesse,
            solution_probability_policy,
            caller_memory,
        };
        let compile_external_retained_bytes = match remainder.checked_retained_bytes() {
            Some(bytes) => bytes,
            None => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_problem_compile_projection_overflow",
                    },
                    caller_memory: remainder.into_caller_memory(),
                })
            }
        };
        let compile_budget = match FiniteScenarioPcCompileBudget::try_new(
            request_memory_limit_bytes,
            compile_external_retained_bytes,
            remainder.returned_carrier_bytes(),
        ) {
            Ok(budget) => budget,
            Err(error) => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: finite_scenario_pc_compile_error(error),
                    caller_memory: remainder.into_caller_memory(),
                })
            }
        };
        let finite_compilation =
            match ProblemCompiler::compile_scenario_pc_finite_build(core_query, compile_budget) {
                Ok(compilation) => compilation,
                Err(error) => {
                    return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error: finite_scenario_pc_compile_error(error),
                        caller_memory: remainder.into_caller_memory(),
                    })
                }
            };
        let (problem, _compile_peak_bytes, admitted_problem_retained_bytes) =
            finite_compilation.into_parts();
        if problem.checked_build_probability_pointee_retained_bytes()
            != Some(admitted_problem_retained_bytes)
        {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_problem_compile_retained_mismatch",
                },
                caller_memory: remainder.into_caller_memory(),
            });
        }
        let problem = Arc::new(problem);
        let (
            output_policy,
            resource_budget,
            field,
            aggregation,
            finesse,
            solution_probability_policy,
            caller_memory,
        ) = remainder.into_parts();
        let validation_report = DiagnosticReport::new();
        let product_capability_contract = None;
        // Finite completion renders backend metadata from the governed Core
        // result, while finite failures carry only an allocation-free static
        // component. Keeping compatibility-only display strings empty here
        // removes two otherwise unguarded constructor allocations.
        let backend_requested = String::new();
        let gpu_device_requested = None;
        let response_kind = CooperativeSearchResponseKind::BuildProbability {
            field,
            aggregation,
            finesse,
            solution_probability_policy,
        };
        let constructor_external_retained_bytes =
            match checked_finite_build_constructor_external_retained_bytes(
                &problem,
                &response_kind,
                &output_policy,
                &validation_report,
                &backend_requested,
                gpu_device_requested.as_ref(),
                caller_memory.retained_owner_bytes(),
            ) {
                Some(bytes) => bytes,
                None => {
                    return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error: CoreExecutionError::RuntimeUnavailable {
                            component:
                                "finite_cooperative_constructor_memory_projection_unavailable",
                        },
                        caller_memory,
                    })
                }
            };
        if let Err(error) = validate_finite_cooperative_memory_requirement(
            constructor_external_retained_bytes,
            request_memory_limit_bytes,
            "finite_cooperative_constructor_external_memory_budget_exceeded",
        ) {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error,
                caller_memory,
            });
        }
        let returned_carrier_bytes =
            checked_finite_build_returned_carrier_bytes(caller_memory.returned_carrier_bytes());
        let session = match WasmBuildProbabilitySession::new_finite(
            &problem,
            field,
            aggregation,
            match &response_kind {
                CooperativeSearchResponseKind::BuildProbability { finesse, .. } => finesse.clone(),
                _ => unreachable!("finite response kind is BuildProbability"),
            },
            constructor_external_retained_bytes,
            returned_carrier_bytes,
        ) {
            Ok(session) => session,
            Err(error) => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: error.into_core_execution_error(),
                    caller_memory,
                })
            }
        };
        drop(problem);

        Ok(CooperativeAppExecution {
            context: Some(self.clone()),
            state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                session: CooperativeSearchSession::BuildProbability(session),
                response_kind,
                command_kind,
                output_policy,
                validation_report,
                backend_requested,
                gpu_device_requested,
                resource_budget,
                product_capability_contract,
            }),
            finite_caller_generation: Some(caller_memory.generation()),
            finite_caller_memory: Some(caller_memory),
        })
    }

    fn start_cooperative_execution_inner(&self, request: AppRequest) -> CooperativeAppExecution {
        let execution_parts = match request.into_execution_parts() {
            Ok(execution_parts) => execution_parts,
            Err(rejection) => {
                return CooperativeAppExecution {
                    context: Some(self.clone()),
                    state: CooperativeExecutionState::Ready(Some(
                        self.finalize_execution_parts_rejection(rejection),
                    )),
                    finite_caller_memory: None,
                    finite_caller_generation: None,
                }
            }
        };
        let forward = matches!(
            &execution_parts.0,
            AppCommand::Damage(_) | AppCommand::SpinFinder(_) | AppCommand::Ren(_)
        );
        let core_search = matches!(
            &execution_parts.0,
            AppCommand::Pc(_)
                | AppCommand::Path(_)
                | AppCommand::Scenario(_)
                | AppCommand::Setup(_)
                | AppCommand::BuildProbability(_)
        );
        if (!forward && !core_search)
            || (core_search
                && !self
                    .services()
                    .core_executor()
                    .supports_cooperative_wasm_search())
        {
            return CooperativeAppExecution {
                context: Some(self.clone()),
                state: CooperativeExecutionState::Immediate(Some(execution_parts)),
                finite_caller_memory: None,
                finite_caller_generation: None,
            };
        }

        let (command, output_policy, resource_budget, _, _, product_capability_contract) =
            execution_parts;
        let command_kind = command.kind();
        let backend_policy = command.backend_policy();
        let backend_requested = backend_policy.backend_requested().to_owned();
        let gpu_device_requested = command.gpu_device_requested();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return CooperativeAppExecution {
                context: Some(self.clone()),
                state: CooperativeExecutionState::Ready(Some(
                    self.finalize_response_with_product_capability(
                        response,
                        command_kind,
                        &output_policy,
                        product_capability_contract,
                    ),
                )),
                finite_caller_memory: None,
                finite_caller_generation: None,
            };
        }
        let command = match command {
            AppCommand::Damage(command) => {
                let session = ForwardSearchSession::new(command.into_query());
                let response_kind = CooperativeSearchResponseKind::Damage;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Forward(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                            resource_budget,
                            product_capability_contract,
                        }),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                    Err(error) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                forward_search_error_response(error),
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                };
            }
            AppCommand::SpinFinder(command) => {
                let session = ForwardSearchSession::new(command.into_query());
                let response_kind = CooperativeSearchResponseKind::SpinFinder;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Forward(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                            resource_budget,
                            product_capability_contract,
                        }),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                    Err(error) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                forward_search_error_response(error),
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                };
            }
            AppCommand::Ren(command) => {
                let session = ForwardSearchSession::new(command.into_query());
                let response_kind = CooperativeSearchResponseKind::Ren;
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Forward(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                            resource_budget,
                            product_capability_contract,
                        }),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                    Err(error) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                forward_search_error_response(error),
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                };
            }
            AppCommand::Setup(command) => {
                let query = command.query().clone();
                let session = WasmSetupSearchSession::new(&query);
                let response_kind = CooperativeSearchResponseKind::Setup(query);
                return match session {
                    Ok(session) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                            session: CooperativeSearchSession::Setup(session),
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report,
                            backend_requested,
                            gpu_device_requested,
                            resource_budget,
                            product_capability_contract,
                        }),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                    Err(error) => CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                wasm_search_error_response(
                                    error,
                                    &backend_requested,
                                    gpu_device_requested,
                                ),
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    },
                };
            }
            command => command,
        };

        let compiled = compile_search_command(command);
        let (problem, response_kind) = match compiled {
            Ok(compiled) => compiled,
            Err(response) => {
                return CooperativeAppExecution {
                    context: Some(self.clone()),
                    state: CooperativeExecutionState::Ready(Some(
                        self.finalize_response_with_product_capability(
                            response,
                            command_kind,
                            &output_policy,
                            product_capability_contract,
                        ),
                    )),
                    finite_caller_memory: None,
                    finite_caller_generation: None,
                }
            }
        };
        let pc_score_external_retained_upper_bound_bytes =
            match checked_cooperative_pc_score_external_retained_upper_bound_bytes(
                &response_kind,
                &output_policy,
                &validation_report,
                &backend_requested,
                gpu_device_requested.as_ref(),
                product_capability_contract.as_ref(),
            ) {
                Ok(retained) => retained,
                Err(error) => {
                    drop(problem);
                    drop(response_kind);
                    let response = error.into_response();
                    return CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                response,
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    };
                }
            };
        let pc_tiling_external_retained_upper_bound_bytes =
            match checked_cooperative_pc_tiling_external_retained_upper_bound_bytes(
                &response_kind,
                &output_policy,
                &validation_report,
                &backend_requested,
                gpu_device_requested.as_ref(),
                product_capability_contract.as_ref(),
            ) {
                Ok(retained) => retained,
                Err(error) => {
                    drop(problem);
                    drop(response_kind);
                    let response = error.into_response();
                    return CooperativeAppExecution {
                        context: Some(self.clone()),
                        state: CooperativeExecutionState::Ready(Some(
                            self.finalize_response_with_product_capability(
                                response,
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )),
                        finite_caller_memory: None,
                        finite_caller_generation: None,
                    };
                }
            };
        let session = match &response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                field,
                aggregation,
                finesse,
                ..
            } => WasmBuildProbabilitySession::new(&problem, *field, *aggregation, finesse.clone())
                .map(CooperativeSearchSession::BuildProbability),
            CooperativeSearchResponseKind::PcScore { authority, .. }
            | CooperativeSearchResponseKind::ScenarioScore { authority, .. } => {
                WasmCpuSearchSession::new_shared_under_authority(
                    Arc::clone(&problem),
                    pc_score_external_retained_upper_bound_bytes
                        .expect("score response kind has a checked external reservation"),
                    authority.terminal_resource_authority(),
                )
                .map(CooperativeSearchSession::Pc)
            }
            CooperativeSearchResponseKind::PcTiling { authority, .. }
            | CooperativeSearchResponseKind::ScenarioTiling { authority, .. } => {
                WasmCpuSearchSession::new_shared_under_authority(
                    Arc::clone(&problem),
                    pc_tiling_external_retained_upper_bound_bytes
                        .expect("tiling response kind has a checked external reservation"),
                    authority
                        .terminal_resource_authority()
                        .expect("cooperative tiling authority owns the terminal lease"),
                )
                .map(CooperativeSearchSession::Pc)
            }
            _ => WasmCpuSearchSession::new(&problem).map(CooperativeSearchSession::Pc),
        };
        let session = match session {
            Ok(session) => session,
            Err(error) => {
                drop(problem);
                drop(response_kind);
                let response = wasm_search_error_response(
                    error,
                    &backend_requested,
                    gpu_device_requested.clone(),
                );
                return CooperativeAppExecution {
                    context: Some(self.clone()),
                    state: CooperativeExecutionState::Ready(Some(
                        self.finalize_response_with_product_capability(
                            response,
                            command_kind,
                            &output_policy,
                            product_capability_contract,
                        ),
                    )),
                    finite_caller_memory: None,
                    finite_caller_generation: None,
                };
            }
        };
        CooperativeAppExecution {
            context: Some(self.clone()),
            state: CooperativeExecutionState::Search(CooperativeSearchExecution {
                session,
                response_kind,
                command_kind,
                output_policy,
                validation_report,
                backend_requested,
                gpu_device_requested,
                resource_budget,
                product_capability_contract,
            }),
            finite_caller_memory: None,
            finite_caller_generation: None,
        }
    }
}

/// Computes the largest App-owned cooperative envelope that can coexist with
/// the authority/Core session. The state and compile-result tuple are both
/// counted to cover the old+new construction peak; dynamic report/output and
/// backend strings are then added by actual allocation capacity.
fn checked_cooperative_pc_score_external_retained_upper_bound_bytes(
    response_kind: &CooperativeSearchResponseKind,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
    backend_requested: &String,
    gpu_device_requested: Option<&String>,
    product_capability_contract: Option<&ValidatedProductCapabilityContract>,
) -> Result<Option<u128>, CooperativePcScoreEnvelopeError> {
    let (authority, product) = match response_kind {
        CooperativeSearchResponseKind::PcScore {
            authority, product, ..
        }
        | CooperativeSearchResponseKind::ScenarioScore {
            authority, product, ..
        } => (authority, *product),
        _ => return Ok(None),
    };
    if !product_capability_contract
        .is_some_and(|contract| contract.contract() == product.capability())
    {
        return Err(CooperativePcScoreEnvelopeError::ProductProofMissing);
    }

    let execution_evidence_inline_bytes = match product {
        CooperativePcScoreProduct::Summary | CooperativePcScoreProduct::ScoreFinder => {
            core::mem::size_of::<crate::pc_score_summary_result::ValidatedPcScoreExecutionEvidence>(
            ) as u128
        }
        CooperativePcScoreProduct::Portfolio => core::mem::size_of::<
            crate::pc_score_minimum_cover_result::ValidatedPcScorePortfolioExecutionEvidence,
        >() as u128,
    };

    let additional_retained_bytes = (core::mem::size_of::<CooperativeAppExecution>() as u128)
        .checked_add(core::mem::size_of::<(
            Arc<clearra_problem::SearchProblem>,
            CooperativeSearchResponseKind,
        )>() as u128)
        .and_then(|bytes| bytes.checked_add(execution_evidence_inline_bytes))
        .and_then(|bytes| {
            bytes.checked_add(
                core::mem::size_of::<crate::pc_score_postprocess::PcScoreDerivation>() as u128,
            )
        })
        .and_then(|bytes| {
            validation_report
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .and_then(|bytes| {
            output_policy
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .and_then(|bytes| bytes.checked_add(backend_requested.capacity() as u128))
        .and_then(|bytes| {
            bytes.checked_add(
                gpu_device_requested
                    .map(|value| value.capacity())
                    .unwrap_or_default() as u128,
            )
        })
        .ok_or(CooperativePcScoreEnvelopeError::ProjectionUnavailable)?;
    authority
        .checked_external_retained_upper_bound_bytes(additional_retained_bytes)
        .map(Some)
        .map_err(CooperativePcScoreEnvelopeError::Authority)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CooperativePcScoreEnvelopeError {
    ProductProofMissing,
    ProjectionUnavailable,
    Authority(PcScoreExecutionError),
}

impl CooperativePcScoreEnvelopeError {
    fn into_response(self) -> AppResponse {
        match self {
            Self::ProductProofMissing => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "pc_score_cooperative_external_product_proof_missing",
                ),
            ),
            Self::ProjectionUnavailable => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "pc_score_cooperative_external_retained_projection_unavailable",
                ),
            ),
            Self::Authority(error) => {
                score_authority_failed_response(PcScoreCompiledAuthorityError::Contract(error))
            }
        }
    }
}

fn checked_cooperative_pc_tiling_external_retained_upper_bound_bytes(
    response_kind: &CooperativeSearchResponseKind,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
    backend_requested: &String,
    gpu_device_requested: Option<&String>,
    product_capability_contract: Option<&ValidatedProductCapabilityContract>,
) -> Result<Option<u128>, CooperativePcTilingEnvelopeError> {
    let authority = match response_kind {
        CooperativeSearchResponseKind::PcTiling { authority, .. }
        | CooperativeSearchResponseKind::ScenarioTiling { authority, .. } => authority,
        _ => return Ok(None),
    };
    if !product_capability_contract
        .is_some_and(|contract| contract.contract() == ProductCapabilityContract::PcTiling)
    {
        return Err(CooperativePcTilingEnvelopeError::ProductProofMissing);
    }
    let additional_retained_bytes = (core::mem::size_of::<CooperativeAppExecution>() as u128)
        .checked_add(core::mem::size_of::<(
            Arc<clearra_problem::SearchProblem>,
            CooperativeSearchResponseKind,
        )>() as u128)
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<
                crate::pc_tiling_family_result::ValidatedPcTilingExecutionEvidence,
            >() as u128)
        })
        .and_then(|bytes| {
            bytes.checked_add(
                PcTilingCompiledAuthority::execution_evidence_retained_upper_bound_bytes(),
            )
        })
        .and_then(|bytes| {
            validation_report
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .and_then(|bytes| {
            output_policy
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .and_then(|bytes| bytes.checked_add(backend_requested.capacity() as u128))
        .and_then(|bytes| {
            bytes.checked_add(
                gpu_device_requested
                    .map(|value| value.capacity())
                    .unwrap_or_default() as u128,
            )
        })
        .ok_or(CooperativePcTilingEnvelopeError::ProjectionUnavailable)?;
    authority
        .checked_external_retained_upper_bound_bytes(additional_retained_bytes)
        .map(Some)
        .map_err(CooperativePcTilingEnvelopeError::Authority)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CooperativePcTilingEnvelopeError {
    ProductProofMissing,
    ProjectionUnavailable,
    Authority(crate::pc_tiling_family_result::PcTilingExecutionError),
}

impl CooperativePcTilingEnvelopeError {
    fn into_response(self) -> AppResponse {
        let component = match self {
            Self::ProductProofMissing => "pc_tiling_cooperative_external_product_proof_missing",
            Self::ProjectionUnavailable => {
                "pc_tiling_cooperative_external_retained_projection_unavailable"
            }
            Self::Authority(error) => error.component(),
        };
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, component),
        )
    }
}

impl CooperativeAppExecution {
    fn context(&self) -> &AppContext {
        self.context
            .as_ref()
            .expect("cooperative execution context remains available")
    }

    fn take_context(&mut self) -> AppContext {
        self.context
            .take()
            .expect("cooperative execution context remains available")
    }

    pub fn finite_caller_memory(&self) -> Option<&FiniteCooperativeCallerMemory> {
        self.finite_caller_memory.as_ref()
    }

    /// Transfers the currently admitted caller-memory authority back to the
    /// caller. The execution retains only the last accepted generation and
    /// cannot advance until the unique authority is derived with `next` and
    /// returned through `advance_finite`.
    pub fn take_finite_caller_memory(&mut self) -> Option<FiniteCooperativeCallerMemory> {
        self.finite_caller_memory.take()
    }

    pub fn advance_finite(
        &mut self,
        incoming_caller_memory: Option<FiniteCooperativeCallerMemory>,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<CooperativeAppAdvance, FiniteCooperativeCallerMemoryRejection> {
        if matches!(self.state, CooperativeExecutionState::Finished) {
            return match incoming_caller_memory {
                Some(caller_memory) => Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "cooperative_finite_build_already_finished",
                    },
                    caller_memory,
                }),
                None => Err(FiniteCooperativeCallerMemoryRejection::Missing {
                    expected_generation: self
                        .finite_caller_generation
                        .and_then(|generation| generation.checked_add(1))
                        .unwrap_or(0),
                }),
            };
        }
        let expected_generation = match self.finite_caller_generation {
            Some(generation) => match generation.checked_add(1) {
                Some(generation) => generation,
                None => {
                    return Err(FiniteCooperativeCallerMemoryRejection::GenerationOverflow {
                        generation,
                        caller_memory: incoming_caller_memory,
                    })
                }
            },
            None if matches!(
                self.state,
                CooperativeExecutionState::FiniteRequiresCallerMemory
            ) =>
            {
                return match incoming_caller_memory {
                    Some(caller_memory) => Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                        error: CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_execution_requires_explicit_start",
                        },
                        caller_memory,
                    }),
                    None => Err(FiniteCooperativeCallerMemoryRejection::Missing {
                        expected_generation: 0,
                    }),
                }
            }
            None => {
                return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_advance_requires_finite_session",
                    },
                    caller_memory: incoming_caller_memory.ok_or_else(|| {
                        FiniteCooperativeCallerMemoryRejection::Missing {
                            expected_generation: 0,
                        }
                    })?,
                })
            }
        };
        if self.finite_caller_memory.is_some() {
            return match incoming_caller_memory {
                Some(caller_memory) => Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                    error: CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_caller_memory_owner_not_transferred",
                    },
                    caller_memory,
                }),
                None => Err(FiniteCooperativeCallerMemoryRejection::Missing {
                    expected_generation,
                }),
            };
        }
        let Some(caller_memory) = incoming_caller_memory else {
            return Err(FiniteCooperativeCallerMemoryRejection::Missing {
                expected_generation,
            });
        };
        if caller_memory.generation() != expected_generation {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_caller_memory_generation_mismatch",
                },
                caller_memory,
            });
        }
        if let Err(error) = validate_finite_build_advance_memory(
            &self.state,
            caller_memory.retained_owner_bytes(),
            caller_memory.returned_carrier_bytes(),
        ) {
            return Err(FiniteCooperativeCallerMemoryRejection::Invalid {
                error,
                caller_memory,
            });
        }

        self.finite_caller_generation = Some(caller_memory.generation());
        self.finite_caller_memory = Some(caller_memory);
        Ok(self.advance_inner(work_budget, control))
    }

    /// Advances a finite session while keeping the non-cloneable caller-memory
    /// authority inside this execution for the entire generation transition.
    ///
    /// Transport layers that can measure their next retained tranche should
    /// prefer this method over a detached `take`/`next`/`advance_finite`
    /// sequence. If deriving the next permit fails, the exact previous permit
    /// is restored before the error is returned.
    pub fn advance_finite_with_next_caller_memory(
        &mut self,
        retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<CooperativeAppAdvance, CoreExecutionError> {
        if matches!(self.state, CooperativeExecutionState::Finished) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "cooperative_finite_build_already_finished",
            });
        }
        let Some(accepted_generation) = self.finite_caller_generation else {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_advance_requires_finite_session",
            });
        };
        let Some(current) = self.finite_caller_memory.as_ref() else {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_caller_memory_owner_not_available",
            });
        };
        if current.generation() != accepted_generation {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_retained_caller_memory_generation_mismatch",
            });
        }
        let previous_generation = current.generation();
        let previous_retained_owner_bytes = current.retained_owner_bytes();
        let previous_returned_carrier_bytes = current.returned_carrier_bytes();
        let next_generation =
            current
                .generation()
                .checked_add(1)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_caller_memory_generation_overflow",
                })?;
        retained_owner_bytes
            .checked_add(returned_carrier_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_caller_memory_bytes_overflow",
            })?;
        validate_finite_build_advance_memory(
            &self.state,
            retained_owner_bytes,
            returned_carrier_bytes,
        )?;

        let current = self
            .finite_caller_memory
            .take()
            .expect("finite caller-memory owner was validated before transfer");
        let next = match current.next(retained_owner_bytes, returned_carrier_bytes) {
            Ok(next) => next,
            Err((error, current)) => {
                self.finite_caller_memory = Some(current);
                return Err(error);
            }
        };
        debug_assert_eq!(next.generation(), next_generation);
        self.finite_caller_generation = Some(next_generation);
        self.finite_caller_memory = Some(next);
        let advance = self.advance_inner(work_budget, control);
        if matches!(&advance, CooperativeAppAdvance::Cancelled) {
            self.finite_caller_generation = Some(previous_generation);
            self.finite_caller_memory = Some(FiniteCooperativeCallerMemory {
                generation: previous_generation,
                retained_owner_bytes: previous_retained_owner_bytes,
                returned_carrier_bytes: previous_returned_carrier_bytes,
            });
        }
        Ok(advance)
    }

    #[cfg(test)]
    pub(crate) fn finalize_search_response_for_product_capability_test(
        self,
        response: AppResponse,
    ) -> AppResponse {
        let CooperativeExecutionState::Search(search) = self.state else {
            panic!("expected cooperative search state");
        };
        self.context
            .expect("cooperative search context remains available")
            .finalize_response_with_product_capability(
                response,
                search.command_kind,
                &search.output_policy,
                search.product_capability_contract,
            )
    }

    fn advance_finite_build_postprocess(
        &mut self,
        postprocess: CooperativePostprocessExecution,
        result: clearra_core_executor::CoreExecutionResult,
        control: &ExecutionControl,
        request_memory_limit_bytes: u128,
    ) -> CooperativeAppAdvance {
        if postprocess.product_capability_contract.is_some() {
            return CooperativeAppAdvance::FailedFinite(CoreExecutionError::RuntimeUnavailable {
                component: "cooperative_finite_build_product_capability_authority_unavailable",
            });
        }
        let (caller_retained_owner_bytes, caller_returned_carrier_bytes) =
            match self.finite_caller_memory.as_ref() {
                Some(caller_memory) => (
                    caller_memory.retained_owner_bytes(),
                    caller_memory.returned_carrier_bytes(),
                ),
                None => {
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_caller_memory_owner_missing",
                        },
                    )
                }
            };
        let returned_carrier_bytes =
            checked_finite_build_returned_carrier_bytes(caller_returned_carrier_bytes);
        let external_retained_bytes =
            match checked_finite_build_materialization_external_retained_bytes(
                &postprocess,
                caller_retained_owner_bytes,
            ) {
                Some(bytes) => bytes,
                None => {
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component:
                                "cooperative_finite_build_external_memory_projection_overflow",
                        },
                    )
                }
            };
        if let Err(error) = validate_finite_cooperative_memory_requirement(
            external_retained_bytes,
            request_memory_limit_bytes,
            "cooperative_finite_build_external_memory_budget_exceeded",
        ) {
            return CooperativeAppAdvance::FailedFinite(error);
        }
        let Some(session) = postprocess.build_probability_session.as_ref() else {
            return CooperativeAppAdvance::FailedFinite(CoreExecutionError::RuntimeUnavailable {
                component: "cooperative_finite_build_session_authority_missing",
            });
        };
        if let Err(error) = session
            .validate_public_result_memory_with_finite_caller_memory(
                &result,
                0,
                external_retained_bytes,
                returned_carrier_bytes,
            )
            .map_err(WasmCpuSearchError::into_core_execution_error)
        {
            return CooperativeAppAdvance::FailedFinite(finite_cooperative_completion_error(error));
        }

        let result = {
            let (
                expected_field,
                expected_aggregation,
                expected_finesse,
                expected_solution_probability_policy,
            ) = match &postprocess.response_kind {
                CooperativeSearchResponseKind::BuildProbability {
                    field,
                    aggregation,
                    finesse,
                    solution_probability_policy,
                } if finesse.score().is_none() => {
                    (*field, *aggregation, finesse, *solution_probability_policy)
                }
                CooperativeSearchResponseKind::BuildProbability { .. } => {
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component:
                                "cooperative_finite_build_finesse_score_authority_unavailable",
                        },
                    )
                }
                _ => {
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "cooperative_finite_build_response_kind_mismatch",
                        },
                    )
                }
            };
            let core_executor = self.context().services().core_executor();
            let result = match core_executor
                .materialize_build_probability_public_result_with_memory_guard(
                    result,
                    expected_solution_probability_policy,
                    control,
                    |stage_result, checked_future_bytes| {
                        let request_required_bytes = checked_finite_build_stage_required_bytes(
                            stage_result,
                            checked_future_bytes,
                            external_retained_bytes,
                            0,
                        )
                        .ok_or(CoreExecutionError::RuntimeUnavailable {
                            component:
                                "cooperative_finite_build_materialization_memory_projection_overflow",
                        })?;
                        validate_finite_cooperative_memory_requirement(
                            request_required_bytes,
                            request_memory_limit_bytes,
                            "cooperative_finite_build_materialization_memory_budget_exceeded",
                        )?;
                        session
                            .validate_public_result_memory_with_finite_caller_memory(
                                stage_result,
                                checked_future_bytes,
                                external_retained_bytes,
                                0,
                            )
                            .map_err(WasmCpuSearchError::into_core_execution_error)
                    },
                ) {
                Ok(result) => result,
                Err(CoreExecutionError::Cancelled) => return CooperativeAppAdvance::Cancelled,
                Err(error) => {
                    return CooperativeAppAdvance::FailedFinite(
                        finite_cooperative_completion_error(error),
                    )
                }
            };
            control.report_progress("postprocess", 1, Some(1));
            let final_materialized_required_bytes = match checked_finite_build_stage_required_bytes(
                &result,
                0,
                external_retained_bytes,
                returned_carrier_bytes,
            ) {
                Some(bytes) => bytes,
                None => return CooperativeAppAdvance::FailedFinite(
                    CoreExecutionError::RuntimeUnavailable {
                        component:
                            "cooperative_finite_build_materialization_memory_projection_overflow",
                    },
                ),
            };
            if let Err(error) = validate_finite_cooperative_memory_requirement(
                final_materialized_required_bytes,
                request_memory_limit_bytes,
                "cooperative_finite_build_materialization_memory_budget_exceeded",
            ) {
                return CooperativeAppAdvance::FailedFinite(error);
            }
            if let Err(error) = session
                .validate_public_result_memory_with_finite_caller_memory(
                    &result,
                    0,
                    external_retained_bytes,
                    returned_carrier_bytes,
                )
                .map_err(WasmCpuSearchError::into_core_execution_error)
            {
                return CooperativeAppAdvance::FailedFinite(finite_cooperative_completion_error(
                    error,
                ));
            }
            if !build_probability_response_is_authorized(
                expected_finesse,
                expected_field,
                expected_aggregation,
                expected_solution_probability_policy,
                &result,
            ) {
                return CooperativeAppAdvance::FailedFinite(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "cooperative_finite_build_response_shape_unauthorized",
                    },
                );
            }
            result
        };

        let CooperativePostprocessExecution {
            result: exhausted_result,
            pc_score_session,
            build_probability_session,
            response_kind,
            command_kind,
            output_policy,
            validation_report,
            resource_budget: _,
            product_capability_contract,
        } = postprocess;
        debug_assert!(exhausted_result.is_none());
        let context = self.take_context();
        let mut memory_guard = |phase: FiniteBuildMemoryPhase, required_bytes: u128| {
            let component = if phase == FiniteBuildMemoryPhase::FinalizedResponse {
                "cooperative_finite_build_final_actual_memory_budget_exceeded"
            } else {
                "cooperative_finite_build_app_response_memory_budget_exceeded"
            };
            validate_finite_cooperative_memory_requirement(
                required_bytes,
                request_memory_limit_bytes,
                component,
            )
        };
        let completion = after_dropping_producer_owners(
            (
                exhausted_result,
                pc_score_session,
                build_probability_session,
                response_kind,
                product_capability_contract,
            ),
            || {
                try_finite_build_success_response(
                    result,
                    validation_report,
                    command_kind,
                    output_policy,
                    context,
                    Some(request_memory_limit_bytes),
                    (core::mem::size_of::<CooperativeAppExecution>() as u128)
                        .checked_add(caller_retained_owner_bytes)
                        .ok_or(CoreExecutionError::RuntimeUnavailable {
                            component:
                                "cooperative_finite_build_app_response_memory_projection_overflow",
                        })?,
                    returned_carrier_bytes,
                    &mut memory_guard,
                )
            },
        );
        match completion {
            Ok(response) => CooperativeAppAdvance::CompletedGoverned(response),
            Err(error) => CooperativeAppAdvance::FailedFinite(error),
        }
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> CooperativeAppAdvance {
        if self.finite_caller_generation.is_some()
            || matches!(
                self.state,
                CooperativeExecutionState::FiniteRequiresCallerMemory
            )
        {
            return CooperativeAppAdvance::FailedFinite(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_execution_requires_explicit_caller_memory",
            });
        }
        self.advance_inner(work_budget, control)
    }

    fn advance_inner(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> CooperativeAppAdvance {
        let finite_caller_memory = self.finite_caller_generation.map(|_| {
            self.finite_caller_memory.as_ref().map(|caller_memory| {
                (
                    caller_memory.retained_owner_bytes(),
                    caller_memory.returned_carrier_bytes(),
                )
            })
        });
        let state = std::mem::replace(&mut self.state, CooperativeExecutionState::Finished);
        match state {
            CooperativeExecutionState::Immediate(mut execution_parts) => {
                if control.is_cancelled() {
                    return CooperativeAppAdvance::Cancelled;
                }
                CooperativeAppAdvance::Completed(
                    self.context().run_execution_parts(
                        execution_parts
                            .take()
                            .expect("immediate execution parts exist"),
                        control,
                    ),
                )
            }
            CooperativeExecutionState::Ready(mut response) => {
                CooperativeAppAdvance::Completed(response.take().expect("ready response exists"))
            }
            CooperativeExecutionState::Search(mut search) => {
                if matches!(finite_caller_memory, Some(None)) {
                    self.state = CooperativeExecutionState::Search(search);
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_caller_memory_owner_missing",
                        },
                    );
                }
                if matches!(finite_caller_memory, Some(Some(_)))
                    && !matches!(
                        &search.session,
                        CooperativeSearchSession::BuildProbability(_)
                    )
                {
                    self.state = CooperativeExecutionState::Search(search);
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "finite_cooperative_search_session_mismatch",
                        },
                    );
                }
                let backend_advance = match finite_caller_memory {
                    Some(Some((caller_retained_owner_bytes, caller_returned_carrier_bytes))) => {
                        let external_retained_owner_bytes =
                            match checked_finite_build_search_advance_external_retained_bytes(
                                &search,
                                caller_retained_owner_bytes,
                            ) {
                                Some(bytes) => bytes,
                                None => {
                                    self.state = CooperativeExecutionState::Search(search);
                                    return CooperativeAppAdvance::FailedFinite(
                                        CoreExecutionError::RuntimeUnavailable {
                                            component:
                                                "finite_cooperative_search_memory_projection_overflow",
                                        },
                                    );
                                }
                            };
                        let returned_carrier_bytes = checked_finite_build_returned_carrier_bytes(
                            caller_returned_carrier_bytes,
                        );
                        match &mut search.session {
                            CooperativeSearchSession::BuildProbability(session) => session
                                .advance_finite(
                                    work_budget,
                                    control,
                                    external_retained_owner_bytes,
                                    returned_carrier_bytes,
                                )
                                .map(|advance| match advance {
                                    WasmBuildProbabilityAdvance::Pending => {
                                        CooperativeBackendAdvance::Pending
                                    }
                                    WasmBuildProbabilityAdvance::Completed(result) => {
                                        CooperativeBackendAdvance::CompletedCore(result)
                                    }
                                    WasmBuildProbabilityAdvance::Cancelled => {
                                        CooperativeBackendAdvance::Cancelled
                                    }
                                }),
                            _ => unreachable!("finite search session was checked before advance"),
                        }
                    }
                    Some(None) => unreachable!("finite caller-memory owner was checked above"),
                    None => advance_search_session(&mut search.session, work_budget, control),
                };
                match backend_advance {
                    Ok(CooperativeBackendAdvance::Pending) => {
                        self.state = CooperativeExecutionState::Search(search);
                        CooperativeAppAdvance::Pending
                    }
                    Ok(CooperativeBackendAdvance::Cancelled)
                    | Err(WasmCpuSearchError::Cancelled) => {
                        if self.finite_caller_generation.is_some() {
                            self.state = CooperativeExecutionState::Search(search);
                        }
                        CooperativeAppAdvance::Cancelled
                    }
                    Ok(CooperativeBackendAdvance::CompletedCore(result)) => {
                        if matches!(
                            &search.response_kind,
                            CooperativeSearchResponseKind::Setup(_)
                                | CooperativeSearchResponseKind::PcTiling { .. }
                                | CooperativeSearchResponseKind::ScenarioTiling { .. }
                        ) {
                            let response = response_from_search(search.response_kind, result);
                            let response = if search.validation_report.is_empty() {
                                response
                            } else {
                                response.with_validation_diagnostics(search.validation_report)
                            };
                            CooperativeAppAdvance::Completed(
                                self.context().finalize_response_with_product_capability(
                                    response,
                                    search.command_kind,
                                    &search.output_policy,
                                    search.product_capability_contract,
                                ),
                            )
                        } else {
                            control.report_progress("postprocess", 0, Some(1));
                            let retain_pc_score_session = matches!(
                                &search.response_kind,
                                CooperativeSearchResponseKind::PcScore { .. }
                                    | CooperativeSearchResponseKind::ScenarioScore { .. }
                            );
                            let retain_build_probability_session = matches!(
                                &search.response_kind,
                                CooperativeSearchResponseKind::BuildProbability { .. }
                            );
                            let (pc_score_session, build_probability_session) = match search.session
                            {
                                CooperativeSearchSession::Pc(session)
                                    if retain_pc_score_session =>
                                {
                                    (Some(session), None)
                                }
                                CooperativeSearchSession::BuildProbability(session)
                                    if retain_build_probability_session =>
                                {
                                    (None, Some(session))
                                }
                                _ => (None, None),
                            };
                            self.state = CooperativeExecutionState::Postprocess(
                                CooperativePostprocessExecution {
                                    result: Some(result),
                                    pc_score_session,
                                    build_probability_session,
                                    response_kind: search.response_kind,
                                    command_kind: search.command_kind,
                                    output_policy: search.output_policy,
                                    validation_report: search.validation_report,
                                    resource_budget: search.resource_budget,
                                    product_capability_contract: search.product_capability_contract,
                                },
                            );
                            CooperativeAppAdvance::Progress
                        }
                    }
                    Ok(CooperativeBackendAdvance::CompletedForward(report)) => {
                        let response = match search.response_kind {
                            CooperativeSearchResponseKind::Damage => {
                                AppResponse::success(AppRenderModel::Damage(report))
                            }
                            CooperativeSearchResponseKind::SpinFinder => {
                                AppResponse::success(AppRenderModel::SpinFinder(report))
                            }
                            CooperativeSearchResponseKind::Ren => {
                                AppResponse::success(AppRenderModel::Ren(report))
                            }
                            _ => AppResponse::failed(
                                AppStatus::ExecutionFailed,
                                AppError::new(
                                    AppErrorCode::ExecutionFailed,
                                    "forward search response kind mismatch",
                                ),
                            ),
                        };
                        CooperativeAppAdvance::Completed(
                            self.context().finalize_response_with_product_capability(
                                response,
                                search.command_kind,
                                &search.output_policy,
                                search.product_capability_contract,
                            ),
                        )
                    }
                    Err(error) => {
                        if self.finite_caller_generation.is_some() {
                            let error = error.into_core_execution_error();
                            self.state = CooperativeExecutionState::Search(search);
                            return CooperativeAppAdvance::FailedFinite(error);
                        }
                        let CooperativeSearchExecution {
                            session,
                            response_kind,
                            command_kind,
                            output_policy,
                            validation_report: _,
                            backend_requested,
                            gpu_device_requested,
                            resource_budget: _,
                            product_capability_contract,
                        } = search;
                        drop(session);
                        drop(response_kind);
                        let response = wasm_search_error_response(
                            error,
                            &backend_requested,
                            gpu_device_requested,
                        );
                        CooperativeAppAdvance::Completed(
                            self.context().finalize_response_with_product_capability(
                                response,
                                command_kind,
                                &output_policy,
                                product_capability_contract,
                            ),
                        )
                    }
                }
            }
            CooperativeExecutionState::Postprocess(mut postprocess) => {
                if matches!(
                    &postprocess.response_kind,
                    CooperativeSearchResponseKind::BuildProbability { .. }
                ) {
                    let request_memory_limit_bytes =
                        match checked_cooperative_request_memory_limit_bytes(
                            postprocess.resource_budget,
                        ) {
                            Ok(limit) => limit,
                            Err(error) => {
                                self.state = CooperativeExecutionState::Postprocess(postprocess);
                                return CooperativeAppAdvance::FailedFinite(error);
                            }
                        };
                    if let Some(request_memory_limit_bytes) = request_memory_limit_bytes {
                        if control.is_cancelled() {
                            self.state = CooperativeExecutionState::Postprocess(postprocess);
                            return CooperativeAppAdvance::Cancelled;
                        }
                        let result = postprocess
                            .result
                            .take()
                            .expect("postprocess result exists");
                        // Once public-result materialization starts it may
                        // consume `result`; a failure from that producer is an
                        // explicit terminal finite failure. All predictable
                        // owner, shape, projection, and carrier rejection was
                        // completed before this state was moved.
                        return self.advance_finite_build_postprocess(
                            postprocess,
                            result,
                            control,
                            request_memory_limit_bytes,
                        );
                    }
                }
                let result = postprocess
                    .result
                    .take()
                    .expect("postprocess result exists");
                if control.is_cancelled() {
                    return CooperativeAppAdvance::Cancelled;
                }
                let core_executor = self.context().services().core_executor();
                let mut score_evidence = None;
                let mut score_portfolio_evidence = None;
                let result = match &postprocess.response_kind {
                    CooperativeSearchResponseKind::PcChance { .. }
                    | CooperativeSearchResponseKind::ScenarioChance { .. } => core_executor
                        .postprocess_pc_chance_result_before_public_surface(result, control),
                    CooperativeSearchResponseKind::PcScore {
                        authority,
                        expected_problem,
                        product,
                    }
                    | CooperativeSearchResponseKind::ScenarioScore {
                        authority,
                        expected_problem,
                        product,
                    } => match postprocess.pc_score_session.as_ref() {
                        Some(session) => match product {
                            CooperativePcScoreProduct::Summary
                            | CooperativePcScoreProduct::ScoreFinder => core_executor
                                .postprocess_pc_score_wasm_result_with_memory_guard(
                                    authority,
                                    expected_problem,
                                    result,
                                    control,
                                    |stage_result, checked_future_bytes| {
                                        session
                                            .validate_public_result_memory_with_future(
                                                stage_result,
                                                checked_future_bytes,
                                            )
                                            .map_err(WasmCpuSearchError::into_core_execution_error)
                                    },
                                )
                                .map(|(result, evidence)| {
                                    score_evidence = Some(evidence);
                                    result
                                }),
                            CooperativePcScoreProduct::Portfolio => core_executor
                                .postprocess_pc_score_minimals_wasm_result_with_memory_guard(
                                    authority,
                                    expected_problem,
                                    result,
                                    control,
                                    |stage_result, checked_future_bytes| {
                                        session
                                            .validate_public_result_memory_with_future(
                                                stage_result,
                                                checked_future_bytes,
                                            )
                                            .map_err(WasmCpuSearchError::into_core_execution_error)
                                    },
                                )
                                .map(|(result, evidence)| {
                                    score_portfolio_evidence = Some(evidence);
                                    result
                                }),
                        },
                        None => Err(CoreExecutionError::RuntimeUnavailable {
                            component: "pc_score_cooperative_session_authority_missing",
                        }),
                    },
                    CooperativeSearchResponseKind::BuildProbability {
                        solution_probability_policy,
                        ..
                    } => match postprocess.build_probability_session.as_ref() {
                        Some(session) => core_executor
                            .materialize_build_probability_public_result_with_memory_guard(
                                result,
                                *solution_probability_policy,
                                control,
                                |stage_result, checked_future_bytes| {
                                    session
                                        .validate_public_result_memory_with_future(
                                            stage_result,
                                            checked_future_bytes,
                                        )
                                        .map_err(WasmCpuSearchError::into_core_execution_error)
                                },
                            ),
                        None => Err(CoreExecutionError::RuntimeUnavailable {
                            component: "build_probability_cooperative_session_authority_missing",
                        }),
                    },
                    _ => core_executor.postprocess_search_result(result, control),
                };
                control.report_progress("postprocess", 1, Some(1));
                let pc_score_response_is_scenario = matches!(
                    &postprocess.response_kind,
                    CooperativeSearchResponseKind::ScenarioScore { .. }
                );
                let pc_score_response = matches!(
                    &postprocess.response_kind,
                    CooperativeSearchResponseKind::PcScore { .. }
                        | CooperativeSearchResponseKind::ScenarioScore { .. }
                );
                if pc_score_response {
                    let CooperativePostprocessExecution {
                        result: _,
                        pc_score_session,
                        build_probability_session,
                        response_kind,
                        command_kind,
                        output_policy,
                        validation_report,
                        resource_budget: _,
                        product_capability_contract,
                    } = postprocess;
                    drop(pc_score_session);
                    drop(build_probability_session);
                    drop(response_kind);
                    let mut response = match result {
                        Ok(result) if pc_score_response_is_scenario => {
                            AppResponse::success(AppRenderModel::Scenario(result))
                        }
                        Ok(result) => AppResponse::success(AppRenderModel::Pc(result)),
                        Err(CoreExecutionError::Cancelled) => {
                            return CooperativeAppAdvance::Cancelled
                        }
                        Err(error) => core_execution_error_response(error),
                    };
                    if let Some(evidence) = score_evidence {
                        response = response.with_pc_score_execution_evidence(evidence);
                    }
                    if let Some(evidence) = score_portfolio_evidence {
                        response = response.with_pc_score_portfolio_execution_evidence(evidence);
                    }
                    let response = if validation_report.is_empty() {
                        response
                    } else {
                        response.with_validation_diagnostics(validation_report)
                    };
                    return CooperativeAppAdvance::Completed(
                        self.context().finalize_response_with_product_capability(
                            response,
                            command_kind,
                            &output_policy,
                            product_capability_contract,
                        ),
                    );
                }
                let mut response = match result {
                    Ok(result) => response_from_search(postprocess.response_kind, result),
                    Err(CoreExecutionError::Cancelled) => return CooperativeAppAdvance::Cancelled,
                    Err(error) => core_execution_error_response(error),
                };
                if let Some(evidence) = score_evidence {
                    response = response.with_pc_score_execution_evidence(evidence);
                }
                if let Some(evidence) = score_portfolio_evidence {
                    response = response.with_pc_score_portfolio_execution_evidence(evidence);
                }
                let response = if postprocess.validation_report.is_empty() {
                    response
                } else {
                    response.with_validation_diagnostics(postprocess.validation_report)
                };
                CooperativeAppAdvance::Completed(
                    self.context().finalize_response_with_product_capability(
                        response,
                        postprocess.command_kind,
                        &postprocess.output_policy,
                        postprocess.product_capability_contract,
                    ),
                )
            }
            CooperativeExecutionState::FiniteRequiresCallerMemory => {
                CooperativeAppAdvance::FailedFinite(CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_execution_requires_explicit_caller_memory",
                })
            }
            CooperativeExecutionState::Finished => {
                if self.context.is_none() {
                    return CooperativeAppAdvance::FailedFinite(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "cooperative_finite_build_already_finished",
                        },
                    );
                }
                CooperativeAppAdvance::Completed(AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::ExecutionFailed,
                        "cooperative execution already finished",
                    ),
                ))
            }
        }
    }
}

pub(crate) fn compile_search_command(
    command: AppCommand,
) -> Result<
    (
        Arc<clearra_problem::SearchProblem>,
        CooperativeSearchResponseKind,
    ),
    AppResponse,
> {
    let compiled = match command {
        AppCommand::Pc(command) => {
            let (query, result_projection) = match command.into_validated_search_parts() {
                Ok(parts) => parts,
                Err(reason) => return Err(allspin_validation_failed_response(reason)),
            };
            if let Some(origin) = result_projection.projection().chance_origin() {
                let problem = match ProblemCompiler::compile_opening_percent(&query) {
                    Ok(problem) => Arc::new(problem.with_pc_chance_probability_v2_evidence()),
                    Err(error) => return Err(problem_compile_failed_response(error)),
                };
                let authority = match PcChanceCompiledAuthority::opening(&query, origin, &problem) {
                    Ok(authority) => authority,
                    Err(error) => return Err(chance_authority_failed_response(error)),
                };
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::PcChance {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().score_origin() {
                let authority = match PcScoreCompiledAuthority::compile_opening(query, origin) {
                    Ok(authority) => authority,
                    Err(error) => return Err(score_authority_failed_response(error)),
                };
                let problem = authority.problem_arc();
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::PcScore {
                        authority,
                        expected_problem: problem,
                        product: if origin.is_score_finder() {
                            CooperativePcScoreProduct::ScoreFinder
                        } else {
                            CooperativePcScoreProduct::Summary
                        },
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().score_minimals_origin() {
                let authority =
                    match PcScoreCompiledAuthority::compile_score_minimals_opening(query, origin) {
                        Ok(authority) => authority,
                        Err(error) => return Err(score_authority_failed_response(error)),
                    };
                let problem = authority.problem_arc();
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::PcScore {
                        authority,
                        expected_problem: problem,
                        product: CooperativePcScoreProduct::Portfolio,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().tiling_origin() {
                let authority =
                    match PcTilingCompiledAuthority::compile_opening_under_terminal_authority(
                        query, origin,
                    ) {
                        Ok(authority) => authority,
                        Err(error) => return Err(tiling_authority_failed_response(error)),
                    };
                let problem = authority.problem_arc();
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::PcTiling {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().save_origin() {
                let authority = match PcSaveCompiledAuthority::compile_opening(query, origin) {
                    Ok(authority) => authority,
                    Err(error) => return Err(save_authority_failed_response(error)),
                };
                let problem = authority.problem_arc();
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::PcSave {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            ProblemCompiler::compile_opening_pc(&query).map(|problem| {
                let problem = if result_projection.projection().minimals_origin().is_some() {
                    problem.with_pc_minimum_cover_v2_evidence()
                } else if result_projection.projection().path_origin().is_some() {
                    problem.with_pc_path_v2_evidence()
                } else {
                    problem
                };
                (
                    Arc::new(problem),
                    CooperativeSearchResponseKind::Pc(result_projection),
                )
            })
        }
        AppCommand::Path(command) => {
            let query = command.query().clone();
            ProblemCompiler::compile_opening_pc(&query).map(|problem| {
                (
                    Arc::new(problem),
                    CooperativeSearchResponseKind::Path(query),
                )
            })
        }
        AppCommand::Scenario(command) => {
            let (query, render_contract, result_projection) =
                match command.into_validated_search_parts() {
                    Ok(parts) => parts,
                    Err(reason) => return Err(allspin_validation_failed_response(reason)),
                };
            if let Some(origin) = result_projection.projection().chance_origin() {
                let problem = match ProblemCompiler::compile_scenario_percent(&query) {
                    Ok(problem) => Arc::new(problem.with_pc_chance_probability_v2_evidence()),
                    Err(error) => return Err(problem_compile_failed_response(error)),
                };
                let authority = match PcChanceCompiledAuthority::scenario(&query, origin, &problem)
                {
                    Ok(authority) => authority,
                    Err(error) => return Err(chance_authority_failed_response(error)),
                };
                debug_assert!(render_contract.is_none());
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::ScenarioChance {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().score_origin() {
                let authority = match PcScoreCompiledAuthority::compile_scenario(query, origin) {
                    Ok(authority) => authority,
                    Err(error) => return Err(score_authority_failed_response(error)),
                };
                let problem = authority.problem_arc();
                debug_assert!(render_contract.is_none());
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::ScenarioScore {
                        authority,
                        expected_problem: problem,
                        product: if origin.is_score_finder() {
                            CooperativePcScoreProduct::ScoreFinder
                        } else {
                            CooperativePcScoreProduct::Summary
                        },
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().score_minimals_origin() {
                let authority = match PcScoreCompiledAuthority::compile_score_minimals_scenario(
                    query, origin,
                ) {
                    Ok(authority) => authority,
                    Err(error) => return Err(score_authority_failed_response(error)),
                };
                let problem = authority.problem_arc();
                debug_assert!(render_contract.is_none());
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::ScenarioScore {
                        authority,
                        expected_problem: problem,
                        product: CooperativePcScoreProduct::Portfolio,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().tiling_origin() {
                let authority =
                    match PcTilingCompiledAuthority::compile_scenario_under_terminal_authority(
                        query, origin,
                    ) {
                        Ok(authority) => authority,
                        Err(error) => return Err(tiling_authority_failed_response(error)),
                    };
                let problem = authority.problem_arc();
                debug_assert!(render_contract.is_none());
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::ScenarioTiling {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            if let Some(origin) = result_projection.projection().save_origin() {
                let authority = match PcSaveCompiledAuthority::compile_scenario(query, origin) {
                    Ok(authority) => authority,
                    Err(error) => return Err(save_authority_failed_response(error)),
                };
                let problem = authority.problem_arc();
                debug_assert!(render_contract.is_none());
                return Ok((
                    Arc::clone(&problem),
                    CooperativeSearchResponseKind::ScenarioSave {
                        authority,
                        expected_problem: problem,
                    },
                ));
            }
            ProblemCompiler::compile_scenario_pc(&query).map(|problem| {
                let problem = if result_projection.projection().minimals_origin().is_some() {
                    problem.with_pc_minimum_cover_v2_evidence()
                } else if result_projection.projection().path_origin().is_some() {
                    problem.with_pc_path_v2_evidence()
                } else {
                    problem
                };
                (
                    Arc::new(problem),
                    CooperativeSearchResponseKind::Scenario {
                        render_contract,
                        result_projection,
                    },
                )
            })
        }
        AppCommand::BuildProbability(command) => {
            if let Some(reason) =
                crate::commands::build_probability_app_command::invalid_query_reason(
                    command.query(),
                )
            {
                return Err(AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(AppErrorCode::InvalidInput, reason),
                ));
            }
            let field = command.query().field();
            let aggregation = command.query().aggregation();
            let finesse = command.query().finesse_request().clone();
            let solution_probability_policy = command.query().solution_probability_policy();
            ProblemCompiler::compile_scenario_pc(command.query().core_query()).map(|problem| {
                (
                    Arc::new(problem),
                    CooperativeSearchResponseKind::BuildProbability {
                        field,
                        aggregation,
                        finesse,
                        solution_probability_policy,
                    },
                )
            })
        }
        _ => {
            return Err(AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "partitioned cooperative result requires a PC or path command",
                ),
            ))
        }
    };
    compiled.map_err(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
        )
    })
}

fn allspin_validation_failed_response(reason: &'static str) -> AppResponse {
    AppResponse::failed(
        AppStatus::ValidationFailed,
        AppError::new(AppErrorCode::InvalidInput, reason),
    )
}

fn problem_compile_failed_response(error: impl core::fmt::Debug) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
    )
}

fn chance_authority_failed_response(
    error: crate::pc_chance_probability_result::PcChanceExecutionError,
) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::ExecutionFailed,
            format!("pc chance compiled authority rejected: {error}"),
        ),
    )
}

fn score_authority_failed_response(error: PcScoreCompiledAuthorityError) -> AppResponse {
    match error {
        PcScoreCompiledAuthorityError::ResourceAdmission(resource_report) => {
            core_execution_error_response(CoreExecutionError::resource_incomplete(
                "execution-admission",
                0,
                resource_report,
            ))
        }
        PcScoreCompiledAuthorityError::ProblemCompile(error) => {
            problem_compile_failed_response(error)
        }
        PcScoreCompiledAuthorityError::Contract(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("pc score compiled authority rejected: {error}"),
            ),
        ),
    }
}

fn tiling_authority_failed_response(error: PcTilingCompiledAuthorityError) -> AppResponse {
    match error {
        PcTilingCompiledAuthorityError::ResourceAdmission(resource_report) => {
            core_execution_error_response(CoreExecutionError::resource_incomplete(
                "execution-admission",
                0,
                resource_report,
            ))
        }
        PcTilingCompiledAuthorityError::ProblemCompile(error) => {
            problem_compile_failed_response(error)
        }
        PcTilingCompiledAuthorityError::Contract(reason) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, reason),
        ),
    }
}

fn save_authority_failed_response(error: PcSaveCompiledAuthorityError) -> AppResponse {
    match error {
        PcSaveCompiledAuthorityError::ProblemCompile(error) => {
            problem_compile_failed_response(error)
        }
        PcSaveCompiledAuthorityError::Contract(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("pc save compiled authority rejected: {error}"),
            ),
        ),
    }
}

pub(crate) fn response_from_search(
    response_kind: CooperativeSearchResponseKind,
    result: clearra_core_executor::CoreExecutionResult,
) -> AppResponse {
    match response_kind {
        CooperativeSearchResponseKind::Pc(result_projection) => AppResponse::success(
            AppRenderModel::Pc(project_pc_allspin_result(result, result_projection)),
        ),
        CooperativeSearchResponseKind::PcChance {
            authority,
            expected_problem,
        } => pc_chance_response(authority, &expected_problem, result, false),
        CooperativeSearchResponseKind::PcScore { .. } => {
            AppResponse::success(AppRenderModel::Pc(result))
        }
        CooperativeSearchResponseKind::PcTiling {
            authority,
            expected_problem,
        } => pc_tiling_response(authority, &expected_problem, result, false),
        CooperativeSearchResponseKind::PcSave {
            authority,
            expected_problem,
        } => pc_save_response(authority, &expected_problem, result, false),
        CooperativeSearchResponseKind::Path(query) => path_response(&query, result),
        CooperativeSearchResponseKind::Scenario {
            render_contract,
            result_projection,
        } => {
            let result = project_pc_allspin_result(result, result_projection);
            match render_contract {
                Some(contract) => contract.success_response(result),
                None => AppResponse::success(AppRenderModel::Scenario(result)),
            }
        }
        CooperativeSearchResponseKind::ScenarioChance {
            authority,
            expected_problem,
        } => pc_chance_response(authority, &expected_problem, result, true),
        CooperativeSearchResponseKind::ScenarioScore { .. } => {
            AppResponse::success(AppRenderModel::Scenario(result))
        }
        CooperativeSearchResponseKind::ScenarioTiling {
            authority,
            expected_problem,
        } => pc_tiling_response(authority, &expected_problem, result, true),
        CooperativeSearchResponseKind::ScenarioSave {
            authority,
            expected_problem,
        } => pc_save_response(authority, &expected_problem, result, true),
        CooperativeSearchResponseKind::Setup(query) => setup_success_response(&query, result),
        CooperativeSearchResponseKind::BuildProbability {
            field,
            aggregation,
            finesse,
            solution_probability_policy,
        } => crate::build_solution_probability_result::build_probability_response(
            &finesse,
            field,
            aggregation,
            solution_probability_policy,
            result,
        ),
        CooperativeSearchResponseKind::Damage
        | CooperativeSearchResponseKind::SpinFinder
        | CooperativeSearchResponseKind::Ren => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ExecutionFailed, "forward result expected"),
        ),
    }
}

fn pc_chance_response(
    authority: PcChanceCompiledAuthority,
    expected_problem: &clearra_problem::SearchProblem,
    result: clearra_core_executor::CoreExecutionResult,
    scenario: bool,
) -> AppResponse {
    let evidence = match authority.validate_execution_result(expected_problem, &result) {
        Ok(evidence) => evidence,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("pc chance result rejected: {error}"),
                ),
            )
        }
    };
    let response = if scenario {
        AppResponse::success(AppRenderModel::Scenario(result))
    } else {
        AppResponse::success(AppRenderModel::Pc(result))
    };
    response.with_pc_chance_execution_evidence(evidence)
}

fn pc_tiling_response(
    authority: PcTilingCompiledAuthority,
    expected_problem: &clearra_problem::SearchProblem,
    result: clearra_core_executor::CoreExecutionResult,
    scenario: bool,
) -> AppResponse {
    let evidence = match authority.validate_execution_result(expected_problem, &result) {
        Ok(evidence) => evidence,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("pc tiling result rejected: {error}"),
                ),
            )
        }
    };
    let response = if scenario {
        AppResponse::success(AppRenderModel::Scenario(result))
    } else {
        AppResponse::success(AppRenderModel::Pc(result))
    };
    response.with_pc_tiling_execution_evidence(evidence)
}

fn pc_save_response(
    authority: PcSaveCompiledAuthority,
    expected_problem: &Arc<clearra_problem::SearchProblem>,
    result: clearra_core_executor::CoreExecutionResult,
    scenario: bool,
) -> AppResponse {
    let evidence = match authority.validate_execution_result(expected_problem, &result) {
        Ok(evidence) => evidence,
        Err(error) => {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("pc save result rejected: {error}"),
                ),
            )
        }
    };
    let response = if scenario {
        AppResponse::success(AppRenderModel::Scenario(result))
    } else {
        AppResponse::success(AppRenderModel::Pc(result))
    };
    response.with_pc_save_execution_evidence(evidence)
}

enum CooperativeBackendAdvance {
    Pending,
    CompletedCore(clearra_core_executor::CoreExecutionResult),
    CompletedForward(ForwardSearchReport),
    Cancelled,
}

fn advance_search_session(
    session: &mut CooperativeSearchSession,
    work_budget: usize,
    control: &ExecutionControl,
) -> Result<CooperativeBackendAdvance, WasmCpuSearchError> {
    match session {
        CooperativeSearchSession::Pc(session) => match session.advance(work_budget, control)? {
            WasmCpuSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            WasmCpuSearchAdvance::Completed(result) => {
                Ok(CooperativeBackendAdvance::CompletedCore(result))
            }
            WasmCpuSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
        CooperativeSearchSession::Setup(session) => match session.advance(work_budget, control)? {
            WasmSetupSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            WasmSetupSearchAdvance::Completed(result) => {
                Ok(CooperativeBackendAdvance::CompletedCore(result))
            }
            WasmSetupSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
        CooperativeSearchSession::BuildProbability(session) => {
            match session.advance(work_budget, control)? {
                WasmBuildProbabilityAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
                WasmBuildProbabilityAdvance::Completed(result) => {
                    Ok(CooperativeBackendAdvance::CompletedCore(result))
                }
                WasmBuildProbabilityAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
            }
        }
        CooperativeSearchSession::Forward(session) => match session
            .advance(
                work_budget.min(FORWARD_SEARCH_COOPERATIVE_WORK_BUDGET),
                control,
            )
            .map_err(forward_error_as_wasm)?
        {
            ForwardSearchAdvance::Pending => Ok(CooperativeBackendAdvance::Pending),
            ForwardSearchAdvance::Completed(report) => {
                Ok(CooperativeBackendAdvance::CompletedForward(report))
            }
            ForwardSearchAdvance::Cancelled => Ok(CooperativeBackendAdvance::Cancelled),
        },
    }
}

fn forward_error_as_wasm(error: ForwardSearchError) -> WasmCpuSearchError {
    match error {
        ForwardSearchError::Cancelled => WasmCpuSearchError::Cancelled,
        ForwardSearchError::UnsupportedRuleProfile(reason) => {
            WasmCpuSearchError::Unsupported { reason }
        }
        ForwardSearchError::EmptyQueue => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_empty_queue",
        },
        ForwardSearchError::QueueTooLong => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_queue_too_long",
        },
        ForwardSearchError::InvalidHeight => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_invalid_height",
        },
        ForwardSearchError::BoardOutsideField => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_board_outside_field",
        },
        ForwardSearchError::PatternRequiresSpinFinder => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_pattern_requires_spin_finder",
        },
        ForwardSearchError::RenRequiresFixedQueue => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_requires_fixed_queue",
        },
        ForwardSearchError::RenQueueTooLong => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_queue_too_long",
        },
        ForwardSearchError::RenInitialComboUnsupported => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_initial_combo_unsupported",
        },
        ForwardSearchError::RenInitialBackToBackUnsupported => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_initial_back_to_back_unsupported",
        },
        ForwardSearchError::RenLineClearPolicyUnsupported => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_line_clear_policy_unsupported",
        },
        ForwardSearchError::RenSpinProfileMustBeDisabled => WasmCpuSearchError::InvalidProblem {
            reason: "forward_ren_spin_profile_must_be_disabled",
        },
        ForwardSearchError::SpinProfileDisabled => WasmCpuSearchError::InvalidProblem {
            reason: "forward_search_spin_profile_disabled",
        },
    }
}

fn forward_search_error_response(error: ForwardSearchError) -> AppResponse {
    let error = forward_error_as_wasm(error);
    wasm_search_error_response(error, "cpu", None)
}

fn core_error_from_wasm(error: WasmCpuSearchError) -> CoreExecutionError {
    error.into_core_execution_error()
}

fn wasm_search_error_response(
    error: WasmCpuSearchError,
    backend_requested: &str,
    gpu_device_requested: Option<String>,
) -> AppResponse {
    let reason = error.reason();
    let response = core_execution_error_response(core_error_from_wasm(error));
    let mut report = BackendReport::new(backend_requested, "none", None::<String>).with_gpu_device(
        gpu_device_requested,
        None,
        None,
        None,
        None,
    );
    if let Some((failure_class, failure_stage)) = gpu_failure_contract(reason) {
        report = report.with_gpu_execution_failure(
            Some(failure_class.to_owned()),
            Some(failure_stage.to_owned()),
            None,
            false,
        );
    }
    response.with_backend_report(report)
}

fn gpu_failure_contract(reason: &str) -> Option<(&'static str, &'static str)> {
    match reason {
        "webgpu_backend_unavailable" => Some(("unavailable", "capability-query")),
        "webgpu_transient_before_commit" => Some(("transient-before-commit", "readback")),
        "webgpu_resource_incomplete" => Some(("resource-incomplete", "host-reduction")),
        "webgpu_invalid_request" => Some(("invalid-request", "batch-planning")),
        "webgpu_trust_mismatch_invalid_result" => Some(("trust-mismatch", "exact-confirm")),
        "webgpu_trust_mismatch"
        | "webgpu_trust_mismatch_buffer_shape"
        | "webgpu_trust_mismatch_edge_count"
        | "webgpu_trust_mismatch_operation_index"
        | "webgpu_trust_mismatch_child_state"
        | "webgpu_trust_mismatch_no_confirmed_dispatch"
        | "webgpu_trust_mismatch_no_confirmed_parent"
        | "webgpu_trust_mismatch_unconfirmed_result" => {
            Some(("trust-mismatch", "cpu-reference-confirm"))
        }
        "webgpu_fatal_internal" => Some(("fatal-internal", "readback")),
        _ => None,
    }
}

#[cfg(test)]
mod setup_ranked_completion_tests {
    use clearra_problem::SetupCandidatePriority;

    use super::{response_from_search, CooperativeSearchResponseKind};
    use crate::{render::AppRenderModel, setup_ranked_fixture};

    #[test]
    fn cooperative_terminal_projection_preserves_the_ranked_family_snapshot() {
        let query = setup_ranked_fixture::query(SetupCandidatePriority::PcProbabilityFirst);
        let response = response_from_search(
            CooperativeSearchResponseKind::Setup(query.clone()),
            setup_ranked_fixture::core_result(&query),
        );
        let snapshot = response
            .render_model()
            .and_then(AppRenderModel::setup_ranked_family_snapshot)
            .expect("cooperative Setup ranked-family snapshot");
        assert_eq!(snapshot.capability_id(), "setup.pc");
        assert_eq!(snapshot.result_schema(), "setup-pc-ranking.v2");
        assert_eq!(snapshot.candidate_count(), 1);
    }
}

#[cfg(test)]
mod pc_allspin_projection_tests {
    use std::cell::Cell;

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        pc::pc_target::PcTarget,
        piece::{piece_kind::PieceKind, rotation::RotationState},
    };
    use clearra_core_executor::CoreExecutionError;
    use clearra_core_executor::CoreExecutionResult;
    use clearra_host_contract::ResourceBudget;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow,
    };
    use clearra_problem::SearchOutputPolicy;
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery,
        BuildSolutionProbabilityPolicy, FinesseMetric, FinessePatternKnowledge, FinessePlacement,
        FinesseScoreRequest,
    };
    use clearra_supply::queue::{
        fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
    };

    use super::{
        after_dropping_producer_owners, checked_finite_build_request_entry_bytes,
        compile_search_command, response_from_search,
        validate_finite_cooperative_memory_requirement, CooperativeAppAdvance,
        CooperativeAppExecution, CooperativeExecutionState, CooperativeSearchResponseKind,
        FiniteCooperativeCallerMemory, FiniteCooperativeCallerMemoryRejection, MIB_BYTES,
    };
    use crate::{
        AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppServices,
        AppStatus, BuildProbabilityAppCommand, DistributedSearchPreparation, PcAppCommand,
        PcResultProjection, PcTilingIngressOrigin, ScenarioAppCommand, ScenarioAppExpected,
        ScenarioAppRenderContract,
    };

    fn fixed_queue() -> PcQueueInput {
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ]))
    }

    #[test]
    fn finite_cooperative_drops_session_owner_before_first_app_allocation() {
        struct SessionDropProbe<'a>(&'a Cell<bool>);

        impl Drop for SessionDropProbe<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let session_dropped = Cell::new(false);
        let probe = SessionDropProbe(&session_dropped);
        let mut first_app_allocation_observed = false;
        after_dropping_producer_owners(probe, || {
            assert!(session_dropped.get());
            first_app_allocation_observed = true;
        });
        assert!(first_app_allocation_observed);
    }

    fn b2b_objective(profile: SpinProfileSelection) -> ObjectivePolicy {
        ObjectivePolicy::unique().with_back_to_back_preservation(profile)
    }

    fn pattern_queue() -> PcQueueInput {
        PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("[TI]!", 2).expect("two-pattern queue"),
        )
    }

    fn incomplete_result(problem_preset: &str) -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![("problem_preset".to_owned(), problem_preset.to_owned())],
            Vec::new(),
        )
    }

    fn one_piece_build_query() -> BuildProbabilityQuery {
        one_piece_build_query_with_objective(ObjectivePolicy::unique())
    }

    fn one_piece_build_query_with_objective(objective: ObjectivePolicy) -> BuildProbabilityQuery {
        one_piece_build_query_at_with_objective(0, objective)
    }

    fn one_piece_build_query_at(target_x: u8) -> BuildProbabilityQuery {
        one_piece_build_query_at_with_objective(target_x, ObjectivePolicy::unique())
    }

    fn one_piece_build_query_at_with_objective(
        target_x: u8,
        objective: ObjectivePolicy,
    ) -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_objective(objective);
        let field = BuildProbabilityField::from_words_preserving_height(
            4,
            [0; 4],
            [0xf_u64 << u32::from(target_x), 0, 0, 0],
        )
        .expect("one-piece Build field");
        BuildProbabilityQuery::new(core, field)
    }

    fn one_piece_score_query(x: i16) -> BuildProbabilityQuery {
        one_piece_build_query()
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle)
            .with_finesse_score(
                FinesseScoreRequest::new(vec![FinessePlacement::new(
                    PieceKind::I,
                    RotationState::Zero,
                    x,
                    0,
                )])
                .expect("one score placement"),
            )
    }

    fn omitted_build_probability_result() -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
                ("solution_probability_count".to_owned(), "0".to_owned()),
                (
                    "solution_probability_complete".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "solution_probability_basis".to_owned(),
                    "not-requested".to_owned(),
                ),
                (
                    "solution_probability_incomplete_reason".to_owned(),
                    "none".to_owned(),
                ),
            ],
            Vec::new(),
        )
    }

    fn core_result(response: &crate::AppResponse) -> &CoreExecutionResult {
        response
            .render_model()
            .and_then(AppRenderModel::core_result)
            .expect("core render result")
    }

    #[test]
    fn finite_caller_memory_has_checked_generation_and_preserves_owner_on_overflow() {
        let start = FiniteCooperativeCallerMemory::start(13, 29).expect("checked start tranche");
        assert_eq!(start.generation(), 0);
        assert_eq!(start.retained_owner_bytes(), 13);
        assert_eq!(start.returned_carrier_bytes(), 29);

        let next = start.next(31, 47).expect("checked next tranche generation");
        assert_eq!(next.generation(), 1);
        assert_eq!(next.retained_owner_bytes(), 31);
        assert_eq!(next.returned_carrier_bytes(), 47);

        let max = FiniteCooperativeCallerMemory::try_new(u64::MAX, 53, 71)
            .expect("maximum generation itself is representable");
        let (error, owner) = max
            .next(1, 1)
            .expect_err("advancing maximum generation must fail closed");
        assert!(matches!(
            error,
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_caller_memory_generation_overflow"
            }
        ));
        assert_eq!(owner.generation(), u64::MAX);
        assert_eq!(owner.retained_owner_bytes(), 53);
        assert_eq!(owner.returned_carrier_bytes(), 71);
    }

    #[test]
    fn finite_request_entry_admission_accepts_exact_cap_and_rejects_one_byte_short() {
        let required = checked_finite_build_request_entry_bytes(17, 23)
            .expect("small request-entry projection is representable");
        assert!(validate_finite_cooperative_memory_requirement(
            required,
            required,
            "finite_entry_test",
        )
        .is_ok());
        assert!(matches!(
            validate_finite_cooperative_memory_requirement(
                required,
                required - 1,
                "finite_entry_test",
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_entry_test"
            })
        ));
        assert_eq!(checked_finite_build_request_entry_bytes(u128::MAX, 1), None);
    }

    #[test]
    fn finite_advance_requires_transferring_the_unique_owner_before_replacement() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(5, 7).unwrap(),
            )
            .expect("finite request uses explicit start");

        let rejection = execution
            .advance_finite(
                Some(FiniteCooperativeCallerMemory::try_new(1, 11, 13).unwrap()),
                1,
                &ExecutionControl::default(),
            )
            .expect_err("a replacement cannot overwrite the retained unique owner");
        assert!(matches!(
            rejection,
            FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_caller_memory_owner_not_transferred"
                },
                caller_memory,
            } if caller_memory.generation() == 1
                && caller_memory.retained_owner_bytes() == 11
                && caller_memory.returned_carrier_bytes() == 13
        ));
        let retained = execution
            .finite_caller_memory()
            .expect("rejection preserves the originally admitted owner");
        assert_eq!(retained.generation(), 0);
        assert_eq!(retained.retained_owner_bytes(), 5);
        assert_eq!(retained.returned_carrier_bytes(), 7);
        assert_eq!(execution.finite_caller_generation, Some(0));
    }

    #[test]
    fn finite_atomic_advance_restores_owner_on_next_failure_and_preserves_finished_state() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(5, 7).unwrap(),
            )
            .expect("finite request uses explicit start");

        assert!(matches!(
            execution.advance_finite_with_next_caller_memory(
                u128::MAX,
                1,
                1,
                &ExecutionControl::default(),
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_caller_memory_bytes_overflow"
            })
        ));
        let restored = execution
            .finite_caller_memory()
            .expect("failed next restores the exact generation-zero owner");
        assert_eq!(restored.generation(), 0);
        assert_eq!(restored.retained_owner_bytes(), 5);
        assert_eq!(restored.returned_carrier_bytes(), 7);
        assert_eq!(execution.finite_caller_generation, Some(0));

        execution
            .advance_finite_with_next_caller_memory(11, 13, 1, &ExecutionControl::default())
            .expect("atomic finite advance derives and retains generation one");
        let advanced = execution
            .finite_caller_memory()
            .expect("successful atomic advance retains the next owner");
        assert_eq!(advanced.generation(), 1);
        assert_eq!(advanced.retained_owner_bytes(), 11);
        assert_eq!(advanced.returned_carrier_bytes(), 13);
        assert_eq!(execution.finite_caller_generation, Some(1));

        let mut finished = CooperativeAppExecution {
            context: None,
            state: CooperativeExecutionState::Finished,
            finite_caller_memory: Some(FiniteCooperativeCallerMemory::start(17, 19).unwrap()),
            finite_caller_generation: Some(0),
        };
        assert!(matches!(
            finished.advance_finite_with_next_caller_memory(
                23,
                29,
                1,
                &ExecutionControl::default(),
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: "cooperative_finite_build_already_finished"
            })
        ));
        let retained = finished
            .finite_caller_memory()
            .expect("finished rejection leaves the owner in place");
        assert_eq!(retained.generation(), 0);
        assert_eq!(retained.retained_owner_bytes(), 17);
        assert_eq!(retained.returned_carrier_bytes(), 19);
        assert_eq!(finished.finite_caller_generation, Some(0));
    }

    #[test]
    fn finite_carrier_preflight_preserves_previous_owner_generation_and_search_state() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(5, 7).unwrap(),
            )
            .expect("finite request uses explicit start");
        assert!(matches!(
            &execution.state,
            CooperativeExecutionState::Search(_)
        ));
        let one_byte_short_carrier = match &execution.state {
            CooperativeExecutionState::Search(search) => {
                let external =
                    super::checked_finite_build_search_advance_external_retained_bytes(search, 11)
                        .expect("finite search external bytes");
                (64 * MIB_BYTES)
                    .checked_sub(external)
                    .and_then(|bytes| bytes.checked_add(1))
                    .expect("one-byte-short returned-carrier boundary")
            }
            _ => unreachable!("finite Build starts in Search"),
        };

        assert!(matches!(
            execution.advance_finite_with_next_caller_memory(
                11,
                one_byte_short_carrier,
                usize::MAX,
                &ExecutionControl::default(),
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: "finite_cooperative_search_return_memory_budget_exceeded"
            })
        ));

        let restored = execution
            .finite_caller_memory()
            .expect("carrier preflight rejection preserves the previous owner");
        assert_eq!(restored.generation(), 0);
        assert_eq!(restored.retained_owner_bytes(), 5);
        assert_eq!(restored.returned_carrier_bytes(), 7);
        assert_eq!(execution.finite_caller_generation, Some(0));
        assert!(matches!(
            &execution.state,
            CooperativeExecutionState::Search(_)
        ));

        execution
            .advance_finite_with_next_caller_memory(11, 13, 1, &ExecutionControl::default())
            .expect("the preserved search owner remains retryable");
        assert_eq!(
            execution
                .finite_caller_memory()
                .expect("successful retry retains the next owner")
                .generation(),
            1
        );
    }

    #[test]
    fn finite_cancel_restores_previous_owner_generation_and_search_state() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(5, 7).unwrap(),
            )
            .expect("finite request uses explicit start");
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();

        assert!(matches!(
            execution
                .advance_finite_with_next_caller_memory(11, 13, usize::MAX, &control)
                .expect("cancel is a cooperative advance outcome"),
            CooperativeAppAdvance::Cancelled
        ));
        let restored = execution
            .finite_caller_memory()
            .expect("cancel restores the previous caller-memory owner");
        assert_eq!(restored.generation(), 0);
        assert_eq!(restored.retained_owner_bytes(), 5);
        assert_eq!(restored.returned_carrier_bytes(), 7);
        assert_eq!(execution.finite_caller_generation, Some(0));
        assert!(matches!(
            &execution.state,
            CooperativeExecutionState::Search(_)
        ));
    }

    #[test]
    fn finite_entry_rejects_unsupported_commands_and_finesse_score_with_owner() {
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let unsupported = AppRequest::new(AppCommand::Pc(PcAppCommand::new(
            OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(fixed_queue()),
        )))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let rejection = match context.start_finite_cooperative_execution(
            unsupported,
            FiniteCooperativeCallerMemory::start(31, 37).unwrap(),
        ) {
            Ok(_) => panic!("finite PC is outside the narrow measurable Build authority"),
            Err(rejection) => rejection,
        };
        assert!(matches!(
            rejection,
            FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_command_authority_unavailable"
                },
                caller_memory,
            } if caller_memory.retained_owner_bytes() == 31
                && caller_memory.returned_carrier_bytes() == 37
        ));

        let finesse_score = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_score_query(0)),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let rejection = match context.start_finite_cooperative_execution(
            finesse_score,
            FiniteCooperativeCallerMemory::start(41, 43).unwrap(),
        ) {
            Ok(_) => panic!("finite finesse score has no retained-memory authority"),
            Err(rejection) => rejection,
        };
        assert!(matches!(
            rejection,
            FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_finesse_score_authority_unavailable"
                },
                caller_memory,
            } if caller_memory.retained_owner_bytes() == 41
                && caller_memory.returned_carrier_bytes() == 43
        ));
    }

    #[test]
    fn finite_cooperative_advance_rejects_missing_replay_and_skip_without_mutating_state() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(5, 7).unwrap(),
            )
            .expect("finite request uses explicit start");

        let generation_zero = execution
            .take_finite_caller_memory()
            .expect("finite start returns the unique generation-zero owner");
        execution
            .advance_finite(
                Some(
                    generation_zero
                        .next(11, 13)
                        .expect("derive generation one from the admitted owner"),
                ),
                1,
                &ExecutionControl::default(),
            )
            .expect("generation one is the exact first advance");
        assert_eq!(
            execution
                .finite_caller_memory()
                .expect("finite state retains the tranche")
                .generation(),
            1
        );
        let generation_one = execution
            .take_finite_caller_memory()
            .expect("caller takes the exact accepted generation-one owner");

        let replay = FiniteCooperativeCallerMemory::try_new(1, 17, 19).unwrap();
        let rejection = execution
            .advance_finite(Some(replay), 1, &ExecutionControl::default())
            .expect_err("replayed generation must be rejected");
        match rejection {
            FiniteCooperativeCallerMemoryRejection::Invalid {
                error,
                caller_memory,
            } => {
                assert!(matches!(
                    error,
                    clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_caller_memory_generation_mismatch"
                    }
                ));
                assert_eq!(caller_memory.generation(), 1);
                assert_eq!(caller_memory.retained_owner_bytes(), 17);
                assert_eq!(caller_memory.returned_carrier_bytes(), 19);
            }
            other => panic!("unexpected replay rejection: {other:?}"),
        }
        assert!(execution.finite_caller_memory().is_none());
        assert_eq!(execution.finite_caller_generation, Some(1));

        let skip = FiniteCooperativeCallerMemory::try_new(3, 23, 29).unwrap();
        let rejection = execution
            .advance_finite(Some(skip), 1, &ExecutionControl::default())
            .expect_err("skipped generation must be rejected");
        assert!(matches!(
            rejection,
            FiniteCooperativeCallerMemoryRejection::Invalid { error, .. }
                if matches!(
                    error,
                    clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                        component: "finite_cooperative_caller_memory_generation_mismatch"
                    }
                )
        ));
        assert!(matches!(
            execution.advance_finite(None, 1, &ExecutionControl::default()),
            Err(FiniteCooperativeCallerMemoryRejection::Missing {
                expected_generation: 2
            })
        ));
        assert!(execution.finite_caller_memory().is_none());
        assert_eq!(execution.finite_caller_generation, Some(1));
        assert_eq!(generation_one.generation(), 1);
        assert_eq!(generation_one.retained_owner_bytes(), 11);
        assert_eq!(generation_one.returned_carrier_bytes(), 13);
    }

    #[test]
    fn finite_cooperative_compatibility_path_rejects_and_explicit_path_retains_bytes() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let finite_request = || {
            AppRequest::new(AppCommand::BuildProbability(
                BuildProbabilityAppCommand::new(one_piece_build_query()),
            ))
            .with_resource_budget(ResourceBudget::new(1, None, Some(64)))
        };
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut compatibility = context.start_cooperative_execution(finite_request());
        assert!(matches!(
            compatibility.advance(1, &ExecutionControl::default()),
            CooperativeAppAdvance::FailedFinite(
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "finite_cooperative_execution_requires_explicit_caller_memory"
                }
            )
        ));

        let explicit = context
            .start_finite_cooperative_execution(
                finite_request(),
                FiniteCooperativeCallerMemory::start(37, 41).unwrap(),
            )
            .expect("finite request accepts explicit generation-zero tranche");
        let retained = explicit
            .finite_caller_memory()
            .expect("finite execution retains caller memory");
        assert_eq!(retained.generation(), 0);
        assert_eq!(retained.retained_owner_bytes(), 37);
        assert_eq!(retained.returned_carrier_bytes(), 41);

        let nonfinite = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ));
        let owner = FiniteCooperativeCallerMemory::start(43, 47).unwrap();
        let rejection = match context.start_finite_cooperative_execution(nonfinite, owner) {
            Ok(_) => panic!("nonfinite request must not enter finite execution"),
            Err(rejection) => rejection,
        };
        let owner = rejection
            .caller_memory()
            .expect("rejected finite entry preserves caller owner");
        assert_eq!(owner.generation(), 0);
        assert_eq!(owner.retained_owner_bytes(), 43);
        assert_eq!(owner.returned_carrier_bytes(), 47);
    }

    #[test]
    fn shared_cooperative_response_keeps_standard_pc_results_unchanged() {
        let (_, response_kind) = compile_search_command(AppCommand::Pc(PcAppCommand::new(
            OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(fixed_queue()),
        )))
        .expect("compile standard PC search");
        let response = response_from_search(
            response_kind,
            CoreExecutionResult::new(
                vec![("sentinel".to_owned(), "preserved".to_owned())],
                Vec::new(),
            )
            .with_normalized_solution_keys(vec!["normalized-key".to_owned()]),
        );
        let result = core_result(&response);
        assert_eq!(result.field("sentinel"), Some("preserved"));
        assert_eq!(result.normalized_solution_keys(), ["normalized-key"]);
        assert_eq!(result.field("pc_allspin_result_contract"), None);
    }

    #[test]
    fn cooperative_build_snapshot_preserves_the_typed_solution_probability_policy() {
        let query = one_piece_build_query()
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
        let (_, response_kind) = compile_search_command(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(query),
        ))
        .expect("compile Build include request");
        assert!(matches!(
            &response_kind,
            CooperativeSearchResponseKind::BuildProbability {
                solution_probability_policy: BuildSolutionProbabilityPolicy::Include,
                ..
            }
        ));

        let response = response_from_search(response_kind, omitted_build_probability_result());
        assert_eq!(response.status(), AppStatus::ExecutionFailed);
        assert!(response.render_model().is_none());
    }

    #[test]
    fn cooperative_build_score_rejects_a_result_from_a_different_retained_request() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let producer_response = context.run(AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_score_query(1)),
        )));
        assert_eq!(
            producer_response.status(),
            AppStatus::Success,
            "{producer_response:?}"
        );
        let swapped_result = core_result(&producer_response).clone();

        let (_, response_kind) = compile_search_command(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_score_query(0)),
        ))
        .expect("compile retained cooperative score request");
        let response = response_from_search(response_kind, swapped_result);
        assert_eq!(response.status(), AppStatus::ExecutionFailed);
        assert!(response.render_model().is_none());
    }

    #[test]
    fn cooperative_build_rejects_swapped_field_and_aggregation_results() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let retained_query = one_piece_build_query();

        let foreign_field_response = context.run(AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query_at(1)),
        )));
        assert_eq!(
            foreign_field_response.status(),
            AppStatus::Success,
            "{foreign_field_response:?}"
        );
        let foreign_field_result = core_result(&foreign_field_response).clone();
        let (_, response_kind) = compile_search_command(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(retained_query.clone()),
        ))
        .expect("compile retained cooperative Build field request");
        let swapped_field = response_from_search(response_kind, foreign_field_result);
        assert_eq!(swapped_field.status(), AppStatus::ExecutionFailed);
        assert!(swapped_field.render_model().is_none());

        let foreign_aggregation_response = context.run(AppRequest::new(
            AppCommand::BuildProbability(BuildProbabilityAppCommand::new(
                one_piece_build_query().with_aggregation(BuildProbabilityAggregation::TilingOnly),
            )),
        ));
        assert_eq!(
            foreign_aggregation_response.status(),
            AppStatus::Success,
            "{foreign_aggregation_response:?}"
        );
        let foreign_aggregation_result = core_result(&foreign_aggregation_response).clone();
        let (_, response_kind) = compile_search_command(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(retained_query),
        ))
        .expect("compile retained cooperative Build aggregation request");
        let swapped_aggregation = response_from_search(response_kind, foreign_aggregation_result);
        assert_eq!(swapped_aggregation.status(), AppStatus::ExecutionFailed);
        assert!(swapped_aggregation.render_model().is_none());
    }

    #[test]
    fn direct_and_cooperative_build_solution_probability_results_share_one_contract() {
        let _resource_guard =
            crate::build_solution_probability_result::build_probability_resource_test_guard();
        let request = || {
            let query = one_piece_build_query()
                .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
            AppRequest::new(AppCommand::BuildProbability(
                BuildProbabilityAppCommand::new(query),
            ))
        };
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );

        let direct = context.run(request());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");

        let mut execution = context.start_cooperative_execution(request());
        let cooperative = (0..256)
            .find_map(
                |_| match execution.advance(4_096, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                    CooperativeAppAdvance::Completed(response) => Some(response),
                    CooperativeAppAdvance::CompletedGoverned(_) => {
                        panic!("unbounded cooperative Build must use the compatibility response")
                    }
                    CooperativeAppAdvance::FailedFinite(error) => {
                        panic!("unbounded cooperative Build failed as finite: {error:?}")
                    }
                    CooperativeAppAdvance::Cancelled => {
                        panic!("uncancelled Build execution must not cancel")
                    }
                },
            )
            .expect("one-piece Build execution completes within the bounded work budget");
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");

        let direct_result = core_result(&direct);
        let cooperative_result = core_result(&cooperative);
        assert_eq!(
            cooperative_result.normalized_solution_keys(),
            direct_result.normalized_solution_keys()
        );
        assert_eq!(
            cooperative_result.solution_probabilities(),
            direct_result.solution_probabilities()
        );
        assert_eq!(
            cooperative_result.field("solution_probability_complete"),
            Some("true")
        );
    }

    #[cfg(not(feature = "bitmap-render"))]
    #[test]
    fn finite_cooperative_build_returns_governed_owner_and_consumes_outer_context() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let memory_mib = 64_u64;
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(one_piece_build_query()),
        ))
        .with_resource_budget(ResourceBudget::new(1, None, Some(memory_mib)));
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context
            .start_finite_cooperative_execution(
                request,
                FiniteCooperativeCallerMemory::start(0, 0).unwrap(),
            )
            .expect("finite Build must use the explicit caller-memory entry point");
        let governed = (0..256)
            .find_map(|_| {
                let caller_memory = execution
                    .take_finite_caller_memory()
                    .expect("finite execution returns its unique caller-memory owner")
                    .next(0, 0)
                    .expect("test generation is bounded");
                match execution
                    .advance_finite(Some(caller_memory), 4_096, &ExecutionControl::default())
                    .expect("finite caller-memory generation must advance exactly")
                {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                    CooperativeAppAdvance::Completed(_) => {
                        panic!("finite cooperative Build escaped through Completed(AppResponse)")
                    }
                    CooperativeAppAdvance::CompletedGoverned(response) => Some(response),
                    CooperativeAppAdvance::FailedFinite(error) => {
                        panic!("finite cooperative Build failed: {error:?}")
                    }
                    CooperativeAppAdvance::Cancelled => {
                        panic!("uncancelled finite cooperative Build must not cancel")
                    }
                }
            })
            .expect("finite one-piece Build completes within the bounded work budget");
        assert!(execution.context.is_none());
        assert_eq!(
            governed.memory_limit_bytes(),
            Some(u128::from(memory_mib) * MIB_BYTES)
        );
        assert!(
            governed.actual_retained_bytes()
                <= governed
                    .memory_limit_bytes()
                    .expect("finite owner retains its limit")
        );
        assert_eq!(governed.response().status(), AppStatus::Success);
        let last_generation = execution.finite_caller_generation;
        let returned_owner = execution
            .take_finite_caller_memory()
            .expect("terminal finite execution returns its last accepted owner");
        let incoming = returned_owner
            .next(0, 0)
            .expect("terminal probe generation is bounded");
        let rejection = execution
            .advance_finite(Some(incoming), 1, &ExecutionControl::default())
            .expect_err("finished finite session rejects without consuming its incoming owner");
        assert!(matches!(
            rejection,
            FiniteCooperativeCallerMemoryRejection::Invalid {
                error: CoreExecutionError::RuntimeUnavailable {
                    component: "cooperative_finite_build_already_finished",
                },
                caller_memory,
            } if Some(caller_memory.generation().saturating_sub(1)) == last_generation
        ));
        assert_eq!(execution.finite_caller_generation, last_generation);
        assert!(execution.finite_caller_memory().is_none());
    }

    #[test]
    fn cooperative_compiler_reserves_tiling_only_output_for_the_typed_projection() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(fixed_queue())
            .with_objective(ObjectivePolicy::tiling());

        let (generic_problem, generic_kind) =
            compile_search_command(AppCommand::Pc(PcAppCommand::new(query.clone())))
                .expect("compile generic tiling objective");
        assert_eq!(generic_problem.output_policy(), SearchOutputPolicy::Trace);
        assert!(matches!(generic_kind, CooperativeSearchResponseKind::Pc(_)));

        let (typed_problem, typed_kind) = compile_search_command(AppCommand::Pc(
            PcAppCommand::new(query).with_result_projection(PcResultProjection::TilingFamilyV1(
                PcTilingIngressOrigin::CanonicalPcTiling,
            )),
        ))
        .expect("compile typed pc tiling");
        assert_eq!(
            typed_problem.output_policy(),
            SearchOutputPolicy::TilingOnly
        );
        assert!(matches!(
            typed_kind,
            CooperativeSearchResponseKind::PcTiling { .. }
        ));
    }

    #[test]
    fn opening_projection_survives_the_shared_cooperative_and_distributed_response_seam() {
        let profile = SpinProfileSelection::AllSpinPlus;
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(fixed_queue())
            .with_objective(b2b_objective(profile));
        let (_, response_kind) = compile_search_command(AppCommand::Pc(
            PcAppCommand::new(query)
                .with_result_projection(PcResultProjection::AllSpinSolution(profile)),
        ))
        .expect("compile opening All-Spin search");
        assert!(matches!(
            &response_kind,
            CooperativeSearchResponseKind::Pc(validated)
                if validated.projection() == PcResultProjection::AllSpinSolution(profile)
        ));

        let response = response_from_search(response_kind, incomplete_result("opening-pc"));
        let result = core_result(&response);
        assert_eq!(
            result.field("pc_allspin_result_contract"),
            Some("pc-b2b-preserving-witness.v1")
        );
        assert_eq!(result.bool_field("pc_allspin_complete"), Some(false));
        assert_eq!(
            result.bool_field("pc_allspin_witness_available"),
            Some(false)
        );
        assert_eq!(
            result.field("pc_allspin_witness_candidate_key"),
            Some("not-materialized")
        );
    }

    #[test]
    fn nonempty_initial_field_projection_survives_the_shared_response_seam() {
        let profile = SpinProfileSelection::AllMiniPlus;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            pattern_queue(),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(1)
        .with_objective(b2b_objective(profile));
        let (_, response_kind) = compile_search_command(AppCommand::Scenario(
            ScenarioAppCommand::new(query)
                .with_result_projection(PcResultProjection::AllSpinPreservationChance(profile)),
        ))
        .expect("compile nonempty initial-field All-Spin search");
        assert!(matches!(
            &response_kind,
            CooperativeSearchResponseKind::Scenario {
                render_contract: None,
                result_projection,
            } if result_projection.projection()
                == PcResultProjection::AllSpinPreservationChance(profile)
        ));

        let response = response_from_search(response_kind, incomplete_result("scenario-pc"));
        let result = core_result(&response);
        assert_eq!(
            result.field("pc_allspin_result_contract"),
            Some("pc-b2b-preservation-probability.v1")
        );
        assert_eq!(
            result.bool_field("pc_allspin_initial_field_supplied"),
            Some(true)
        );
        assert_eq!(
            result.bool_field("pc_allspin_target_field_supplied"),
            Some(false)
        );
        assert_eq!(result.bool_field("pc_allspin_complete"), Some(false));
    }

    #[test]
    fn scenario_allspin_rejects_render_contract_field_replacement() {
        let profile = SpinProfileSelection::AllMiniPlus;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            pattern_queue(),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(1)
        .with_objective(b2b_objective(profile));
        let command = ScenarioAppCommand::new(query)
            .with_result_projection(PcResultProjection::AllSpinPreservationChance(profile))
            .with_render_contract(ScenarioAppRenderContract::new(
                false,
                vec![(
                    "pc_allspin_target_field_supplied".to_owned(),
                    "true".to_owned(),
                )],
            ));

        assert_eq!(
            command.validate_result_projection(),
            Err("pc All-Spin does not accept a scenario render contract")
        );
        let response = match compile_search_command(AppCommand::Scenario(command)) {
            Err(response) => response,
            Ok(_) => panic!("cooperative scenario All-Spin must reject render contracts"),
        };
        assert_eq!(response.status(), AppStatus::ValidationFailed);
    }

    #[test]
    fn invalid_scenario_allspin_cannot_use_expected_unsupported_rendering() {
        let profile = SpinProfileSelection::AllMiniPlus;
        let command = || {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(7, 1),
                pattern_queue(),
                PieceWindow::new(1),
            )
            .with_exact_pieces(Some(1))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_retained_trace_limit(1)
            .with_objective(b2b_objective(profile));
            AppCommand::Scenario(
                ScenarioAppCommand::new(query)
                    .with_result_projection(PcResultProjection::AllSpinPreservationChance(profile))
                    .with_render_contract(
                        ScenarioAppRenderContract::new(
                            true,
                            vec![(
                                "pc_allspin_target_field_supplied".to_owned(),
                                "true".to_owned(),
                            )],
                        )
                        .with_expected(Some(
                            ScenarioAppExpected::new(false, Some(false))
                                .with_unsupported(true, None),
                        )),
                    ),
            )
        };

        let direct = AppContext::default().run(AppRequest::new(command()));
        assert_eq!(direct.status(), AppStatus::ValidationFailed);
        assert!(direct.render_model().is_none());

        let mut cooperative =
            AppContext::default().start_cooperative_execution(AppRequest::new(command()));
        let CooperativeAppAdvance::Completed(cooperative) =
            cooperative.advance(1, &ExecutionControl::default())
        else {
            panic!("invalid cooperative All-Spin must complete at validation");
        };
        assert_eq!(cooperative.status(), AppStatus::ValidationFailed);
        assert!(cooperative.render_model().is_none());

        let DistributedSearchPreparation::Ready(distributed) =
            AppContext::default().prepare_distributed_search(AppRequest::new(command()))
        else {
            panic!("invalid distributed All-Spin must stop at validation");
        };
        assert_eq!(distributed.status(), AppStatus::ValidationFailed);
        assert!(distributed.render_model().is_none());
    }

    #[test]
    fn invalid_typed_allspin_pair_fails_at_direct_cooperative_and_distributed_boundaries() {
        let profile = SpinProfileSelection::AllSpinPlus;
        let invalid_command = || {
            AppCommand::Pc(
                PcAppCommand::new(
                    OpeningPcSearchQuery::new(PcTarget::two_lines())
                        .with_queue(pattern_queue())
                        .with_objective(b2b_objective(profile)),
                )
                .with_result_projection(PcResultProjection::AllSpinSolution(profile)),
            )
        };

        let direct = AppContext::default().run(AppRequest::new(invalid_command()));
        assert_eq!(direct.status(), AppStatus::ValidationFailed);
        assert!(direct.render_model().is_none());
        assert!(direct.error().is_some_and(|error| error
            .message()
            .contains("matching product capability contract")));

        let cooperative = match compile_search_command(invalid_command()) {
            Err(response) => response,
            Ok(_) => panic!("cooperative compile must reject an invalid typed All-Spin pair"),
        };
        assert_eq!(cooperative.status(), AppStatus::ValidationFailed);
        assert!(cooperative.render_model().is_none());

        let DistributedSearchPreparation::Ready(distributed) =
            AppContext::default().prepare_distributed_search(AppRequest::new(invalid_command()))
        else {
            panic!("distributed preparation must fail before creating a search owner");
        };
        assert_eq!(distributed.status(), AppStatus::ValidationFailed);
        assert!(distributed.render_model().is_none());
    }

    #[test]
    fn cooperative_build_keeps_terminal_authority_through_b2b_report_rebuild() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let request = || {
            let query =
                one_piece_build_query_with_objective(b2b_objective(SpinProfileSelection::TSpins))
                    .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
            AppRequest::new(AppCommand::BuildProbability(
                BuildProbabilityAppCommand::new(query),
            ))
        };
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );

        let direct = context.run(request());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let mut execution = context.start_cooperative_execution(request());
        let cooperative = (0..256)
            .find_map(
                |_| match execution.advance(4_096, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                    CooperativeAppAdvance::Completed(response) => Some(response),
                    CooperativeAppAdvance::CompletedGoverned(_) => {
                        panic!("unbounded cooperative Build must use the compatibility response")
                    }
                    CooperativeAppAdvance::FailedFinite(error) => {
                        panic!("unbounded cooperative Build failed as finite: {error:?}")
                    }
                    CooperativeAppAdvance::Cancelled => {
                        panic!("uncancelled cooperative Build must not cancel")
                    }
                },
            )
            .expect("cooperative B2B Build completes");
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");

        let direct = core_result(&direct);
        let cooperative = core_result(&cooperative);
        assert_eq!(
            cooperative.bool_field("execution_constraint_materialized"),
            Some(true)
        );
        assert_eq!(
            cooperative.solution_probabilities(),
            direct.solution_probabilities()
        );
        for field in [
            "solution_probabilities_requested",
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(cooperative.field_occurrence_count(field), 1, "{field}");
            assert_eq!(cooperative.unique_field(field), direct.unique_field(field));
        }
    }
}

#[cfg(test)]
mod raw_pc_tiling_cooperative_tests {
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
    };
    #[cfg(feature = "native-c-core")]
    use clearra_core_executor::PcTilingMemoryAdmissionEvidence;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::CooperativeAppAdvance;
    #[cfg(feature = "native-c-core")]
    use crate::AppRenderModel;
    use crate::{
        AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppServices, AppStatus,
        PcAppCommand, PcResultProjection, PcTilingIngressOrigin, ProductCapabilityContract,
    };

    fn canonical_raw_tiling_request() -> AppRequest {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(1)
                    .with_allow_backend_fallback(false)
                    .with_max_candidates(5_000),
            )
            .with_objective(ObjectivePolicy::tiling());
        AppRequest::new(AppCommand::Pc(
            PcAppCommand::new(query).with_result_projection(PcResultProjection::TilingFamilyV1(
                PcTilingIngressOrigin::CanonicalPcTiling,
            )),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcTiling)
        .expect("canonical raw pc tiling product contract")
    }

    #[cfg(feature = "native-c-core")]
    fn core_result(response: &crate::AppResponse) -> &clearra_core_executor::CoreExecutionResult {
        response
            .render_model()
            .and_then(AppRenderModel::core_result)
            .expect("successful PC response must retain its typed Core result")
    }

    #[cfg(feature = "native-c-core")]
    fn complete_cooperative_tiling(context: &AppContext) -> crate::AppResponse {
        let mut execution = context.start_cooperative_execution(canonical_raw_tiling_request());
        let control = ExecutionControl::default();
        for _ in 0..512 {
            match execution.advance(4_096, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(response) => return response,
                CooperativeAppAdvance::CompletedGoverned(_) => {
                    panic!("non-Build cooperative tiling returned a governed Build response")
                }
                CooperativeAppAdvance::FailedFinite(error) => {
                    panic!("non-Build cooperative tiling failed as finite: {error:?}")
                }
                CooperativeAppAdvance::Cancelled => {
                    panic!("uncancelled cooperative RAW tiling execution was cancelled")
                }
            }
        }
        panic!("canonical RAW tiling fixture must complete within the bounded work budget")
    }

    #[cfg(feature = "native-c-core")]
    #[test]
    fn canonical_raw_pc_tiling_direct_and_cooperative_semantics_match() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let direct = AppContext::default().run(canonical_raw_tiling_request());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let direct_result = core_result(&direct);
        assert_eq!(
            direct_result.bool_field("packing_source_raw_geometry"),
            Some(true)
        );
        assert_eq!(
            direct_result.bool_field("tiling_objective_canonical"),
            Some(true)
        );
        assert_eq!(
            direct_result.bool_field("tiling_materialization_complete"),
            Some(true)
        );
        assert_eq!(direct_result.bool_field("buildup_executed"), Some(false));
        assert_eq!(
            direct_result.field("actual_solution_set_contract"),
            Some("normalized-tiling-set")
        );
        assert_eq!(
            direct_result.field_occurrence_count("probability_calculated"),
            1
        );
        assert_eq!(
            direct_result.pc_tiling_memory_admission_evidence(),
            Some(PcTilingMemoryAdmissionEvidence::NativeInternal)
        );
        assert!(direct_result.pc_tiling_family_publication_contract_is_valid());

        let cooperative_context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let cooperative = complete_cooperative_tiling(&cooperative_context);
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
        let cooperative_result = core_result(&cooperative);
        assert_eq!(cooperative_result.field("objective"), Some("tiling"));
        assert_eq!(
            cooperative_result.bool_field("packing_candidate_is_solution"),
            Some(true)
        );
        assert_eq!(
            cooperative_result.bool_field("buildability_verified"),
            Some(false)
        );
        assert_eq!(
            cooperative_result.bool_field("coverage_calculated"),
            Some(false)
        );
        assert_eq!(
            cooperative_result.bool_field("probability_calculated"),
            Some(false)
        );
        assert_eq!(cooperative_result.bool_field("count_complete"), Some(true));
        assert_eq!(
            cooperative_result.bool_field("solution_set_materialized"),
            Some(true)
        );
        assert_eq!(
            cooperative_result.bool_field("solution_keys_complete"),
            Some(true)
        );
        assert_eq!(
            cooperative_result.pc_tiling_memory_admission_evidence(),
            Some(PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority)
        );
        assert!(cooperative_result.pc_tiling_family_publication_contract_is_valid());

        assert!(!direct_result.normalized_solution_keys().is_empty());
        assert_eq!(
            cooperative_result.normalized_solution_keys(),
            direct_result.normalized_solution_keys()
        );
        assert_eq!(
            cooperative_result.field("normalized_solution_set_hash"),
            direct_result.field("normalized_solution_set_hash")
        );
        assert_eq!(
            cooperative_result.field("actual_normalized_solution_set_hash"),
            direct_result.field("actual_normalized_solution_set_hash")
        );
        assert_eq!(
            cooperative_result.usize_field("normalized_unique_solution_count"),
            direct_result.usize_field("normalized_unique_solution_count")
        );
        assert_eq!(
            cooperative_result.usize_field("solution_keys_materialized_count"),
            direct_result.usize_field("solution_keys_materialized_count")
        );
    }

    #[test]
    fn raw_pc_tiling_cooperative_cancellation_is_terminal_and_fail_closed() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let mut execution = context.start_cooperative_execution(canonical_raw_tiling_request());
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();

        assert_eq!(
            execution.advance(1, &ExecutionControl::new(cancellation)),
            CooperativeAppAdvance::Cancelled
        );

        let CooperativeAppAdvance::Completed(repeated) =
            execution.advance(1, &ExecutionControl::default())
        else {
            panic!("a cancelled cooperative owner must be terminal")
        };
        assert_eq!(repeated.status(), AppStatus::ExecutionFailed);
        assert!(repeated.render_model().is_none());
        assert!(repeated.error().is_some_and(|error| error
            .message()
            .contains("cooperative execution already finished")));
    }
}
