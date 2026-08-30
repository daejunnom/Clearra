// SRP rationale: this module has one behavior-level change reason: serializing typed WASM
// worker events into the stable host JSON envelope.
use std::{fmt::Write, sync::Arc};

use clearra_app::{
    CoveragePortfolioAlternativeSet, CoveragePortfolioPageStore, PortfolioAlternativeAdvance,
    PortfolioAlternativeError, PortfolioAlternativePage, ProductPageSourceOwner,
    PORTFOLIO_MEMBER_PAGE_CONTRACT, PORTFOLIO_MEMBER_PAGE_SIZE,
};
use clearra_host_contract::{
    AppResponse, AppStatus, BackendReport, CapabilityReport, ContinuationReport, Diagnostic,
    DiagnosticReport, ParityReportPagePayload, ProductBuildIdentity, ProductResultPayload,
    ProductResultPayloadContent, ResourceReport, SolutionSetArtifactPayload,
};

use crate::wasm_worker_job::GovernedWasmWorkerEvents;
use crate::{
    BackendStatus, BudgetStatus, JobProgress, MemoryStatus, WasmCommandRuntimeError,
    WasmSearchReport, WasmWorkerJobEvent, WebGpuBackendOutcomeState, WebGpuBackendReport,
    WebGpuLimitsReport, WebGpuMemoryReport, WebGpuReportTrustState, WebGpuShaderReport,
};

impl WasmCommandRuntimeError {
    /// Serializes a machine-readable ABI failure. Resource-admission evidence
    /// is carried as a typed object; legacy errors retain an explicit null.
    pub fn structured_output(&self) -> String {
        serialize_json(|output| {
            let mut object = JsonObject::begin(output);
            object.string("code", self.code());
            object.string("message", self.message());
            object.optional_object(
                "resource_report",
                self.resource_report(),
                write_core_resource_report,
            );
            object.finish();
        })
        .unwrap_or_default()
    }
}

pub(crate) fn serialize_worker_events(
    events: &[WasmWorkerJobEvent],
) -> Result<String, WasmCommandRuntimeError> {
    serialize_json(|output| write_worker_events(output, events))
}

/// Serializes one already loaded exact alternative and one fixed-size member
/// page. Outer and member page numbers are one-based and all semantic IDs are
/// decimal strings on the wire.
pub fn serialize_coverage_portfolio_page(
    store: &CoveragePortfolioPageStore,
    outer_page_number: usize,
    member_page_number: usize,
) -> Result<String, WasmCommandRuntimeError> {
    let alternative_index_decimal = outer_page_number.to_string();
    let retained_slot = store
        .retained_page_slot(&alternative_index_decimal)
        .ok_or_else(|| product_page_error(PortfolioAlternativeError::PageNotLoaded))?;
    serialize_coverage_portfolio_retained_page(store, retained_slot, member_page_number)
}

pub fn serialize_coverage_portfolio_page_exact(
    store: &mut CoveragePortfolioPageStore,
    alternative_index_decimal: &str,
    member_page_number: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<String, WasmCommandRuntimeError> {
    let retained_slot = store
        .load_page_by_alternative_index(alternative_index_decimal, cancelled)
        .map_err(product_page_error)?;
    serialize_coverage_portfolio_retained_page(store, retained_slot, member_page_number)
}

pub fn serialize_coverage_portfolio_retained_page(
    store: &CoveragePortfolioPageStore,
    retained_slot: usize,
    member_page_number: usize,
) -> Result<String, WasmCommandRuntimeError> {
    let page = store
        .retained_page(retained_slot)
        .ok_or_else(|| product_page_error(PortfolioAlternativeError::PageNotLoaded))?;
    let candidate_ids = page.portfolio().candidate_ids();
    let total_member_pages = candidate_ids
        .len()
        .div_ceil(PORTFOLIO_MEMBER_PAGE_SIZE)
        .max(1);
    if member_page_number == 0 || member_page_number > total_member_pages {
        return Err(product_page_error(
            PortfolioAlternativeError::InvalidMemberPage,
        ));
    }
    let member_start = (member_page_number - 1)
        .checked_mul(PORTFOLIO_MEMBER_PAGE_SIZE)
        .ok_or_else(|| product_page_error(PortfolioAlternativeError::InvalidMemberPage))?;
    let member_end = member_start
        .saturating_add(PORTFOLIO_MEMBER_PAGE_SIZE)
        .min(candidate_ids.len());
    let source = store.source();
    let candidates = source.candidates();
    for candidate_id in &candidate_ids[member_start..member_end] {
        let candidate_index = usize::try_from(*candidate_id)
            .ok()
            .and_then(|candidate_id| candidate_id.checked_sub(1))
            .ok_or_else(|| product_page_error(PortfolioAlternativeError::InvalidCandidateId))?;
        if candidates
            .get(candidate_index)
            .is_none_or(|candidate| candidate.candidate_id() != *candidate_id)
            || source.public_candidate_id(*candidate_id).is_none()
        {
            return Err(product_page_error(
                PortfolioAlternativeError::InvalidCandidateId,
            ));
        }
    }
    serialize_json(|output| {
        let mut object = JsonObject::begin(output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        object.string("product_page_kind", "coverage-portfolio");
        object.string("state", "page");
        object.object("page", |nested| {
            write_coverage_portfolio_runtime_page(
                nested,
                page,
                source,
                member_start,
                member_end,
                member_page_number,
                total_member_pages,
            )
        });
        object.finish();
    })
}

/// Serializes a bounded enumeration advance that did not materialize an outer
/// page. Hosts retry `work-budget-exhausted`, stop on `sealed`, and release the
/// handle on `cancelled`.
pub fn serialize_coverage_portfolio_advance_state(
    advance: &PortfolioAlternativeAdvance,
) -> Result<String, WasmCommandRuntimeError> {
    if advance.page().is_some() {
        return Err(WasmCommandRuntimeError::new(
            "E_WASM_PRODUCT_PAGE_PROJECTION",
            String::new(),
        ));
    }
    serialize_json(|output| {
        let mut object = JsonObject::begin(output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        object.string("product_page_kind", "coverage-portfolio");
        object.string("state", advance.stop().as_str());
        object.string(
            "known_alternative_count",
            advance.checkpoint().known_alternative_count_decimal(),
        );
        object.boolean(
            "enumeration_complete",
            advance.checkpoint().enumeration_complete(),
        );
        object.finish();
    })
}

/// Serializes one already loaded parity page without granting search,
/// feasibility, or pruning authority. Page numbers remain one-based.
pub fn serialize_parity_report_page(
    page: &ParityReportPagePayload,
) -> Result<String, WasmCommandRuntimeError> {
    serialize_json(|output| {
        let mut object = JsonObject::begin(output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        object.string("product_page_kind", "parity-report");
        object.string("state", "page");
        object.object("page", |nested| write_parity_report_page(nested, page));
        object.finish();
    })
}

/// Serializes the terminal state reached after the final bounded parity page.
pub fn serialize_parity_report_exhausted() -> Result<String, WasmCommandRuntimeError> {
    serialize_json(|output| {
        let mut object = JsonObject::begin(output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        object.string("product_page_kind", "parity-report");
        object.string("state", "exhausted");
        object.finish();
    })
}

fn product_page_error(error: PortfolioAlternativeError) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_PRODUCT_PAGE", error.as_str())
}

fn write_coverage_portfolio_runtime_page(
    object: &mut JsonObject<'_>,
    page: &PortfolioAlternativePage,
    source: &CoveragePortfolioAlternativeSet,
    member_start: usize,
    member_end: usize,
    member_page_number: usize,
    total_member_pages: usize,
) {
    let candidates = source.candidates();
    object.string("page_contract", page.contract_id());
    object.string("member_page_contract", PORTFOLIO_MEMBER_PAGE_CONTRACT);
    object.string("set_identity_sha256", page.set_identity_sha256());
    object.string("candidate_map_sha256", page.candidate_map_sha256());
    object.string("alternative_index", page.alternative_index_decimal());
    object.string(
        "optimal_cardinality",
        &page.optimal_cardinality().to_string(),
    );
    object.string(
        "known_alternative_count",
        page.known_alternative_count_decimal(),
    );
    object.optional_string(
        "total_alternative_count",
        page.total_alternative_count_decimal(),
    );
    object.boolean("enumeration_complete", page.enumeration_complete());
    object.string("member_page_number", &member_page_number.to_string());
    object.string("total_member_pages", &total_member_pages.to_string());
    object.array("members", |output| {
        output.push('[');
        for (member_index, candidate_id) in page.portfolio().candidate_ids()
            [member_start..member_end]
            .iter()
            .enumerate()
        {
            if member_index != 0 {
                output.push(',');
            }
            let candidate_index = (*candidate_id as usize) - 1;
            let candidate = &candidates[candidate_index];
            let mut nested = JsonObject::begin(&mut *output);
            let public_candidate_id = source
                .public_candidate_id(*candidate_id)
                .expect("coverage page IDs were validated before serialization");
            nested.string("candidate_id", &public_candidate_id.to_string());
            nested.string("normalized_solution_key", candidate.normalized_key());
            nested.finish();
        }
        output.push(']');
    });
}

fn write_parity_report_page(object: &mut JsonObject<'_>, page: &ParityReportPagePayload) {
    object.string("document_format", page.document_format());
    object.number("page_number", page.page_number());
    object.number("total_pages", page.total_pages());
    object.string("coordinate_basis", page.coordinate_basis());
    object.number("width", page.width());
    object.number("height", page.height());
    object.number("occupied_cell_count", page.occupied_cell_count());
    object.number("checker_black_count", page.checker_black_count());
    object.number("checker_white_count", page.checker_white_count());
    object.number("checker_delta", page.checker_delta());
    object.array("four_color_counts", |output| {
        output.push('[');
        for (index, count) in page.four_color_counts().iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let _ = write!(output, "{count}");
        }
        output.push(']');
    });
    object.number("even_column_count", page.even_column_count());
    object.number("odd_column_count", page.odd_column_count());
    object.number("column_parity_delta", page.column_parity_delta());
    object.number("occupied_area_mod_four", page.occupied_area_mod_four());
    object.number(
        "pending_garbage_occupied_cell_count",
        page.pending_garbage_occupied_cell_count(),
    );
    object.boolean("feasibility_claim", page.feasibility_claim());
    object.string("pruning_authority", page.pruning_authority());
    object.boolean("page_handle_available", page.page_handle_available());
}

fn write_worker_events(output: &mut JsonSink, events: &[WasmWorkerJobEvent]) {
    output.push('[');
    for (index, event) in events.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let mut object = JsonObject::begin(&mut *output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        write_event(&mut object, event);
        object.finish();
    }
    output.push(']');
}

/// Serialized JSON together with the finite authority that owns its output
/// buffer and any producer-authoritative shared page-store graph. The reported
/// actual bytes retain their payload meaning: this wrapper inline, the JSON
/// capacity, and the page-store backing, but not a caller's outer carrier.
/// This type is intentionally non-cloneable.
#[derive(Debug, Eq, PartialEq)]
pub struct GovernedWasmJson {
    json: String,
    completed_tiling_solution_page_store: Option<Arc<crate::TilingSolutionPageStore>>,
    completed_product_page_source_owner: Option<ProductPageSourceOwner>,
    memory_limit_bytes: u128,
    actual_retained_bytes: u128,
}

impl GovernedWasmJson {
    pub fn json(&self) -> &str {
        &self.json
    }

    pub const fn memory_limit_bytes(&self) -> u128 {
        self.memory_limit_bytes
    }

    pub const fn actual_retained_bytes(&self) -> u128 {
        self.actual_retained_bytes
    }

    pub fn completed_tiling_solution_page_store(
        &self,
    ) -> Option<&Arc<crate::TilingSolutionPageStore>> {
        self.completed_tiling_solution_page_store.as_ref()
    }

    pub fn completed_product_page_source_owner(&self) -> Option<&ProductPageSourceOwner> {
        self.completed_product_page_source_owner.as_ref()
    }

    /// Consumes the governed JSON while explicitly releasing any transferred
    /// page-store authority. This is the JSON-only caller path.
    pub fn into_json(self) -> String {
        let Self {
            json,
            completed_tiling_solution_page_store,
            completed_product_page_source_owner,
            memory_limit_bytes: _,
            actual_retained_bytes: _,
        } = self;
        drop(completed_tiling_solution_page_store);
        drop(completed_product_page_source_owner);
        json
    }

    /// Moves the JSON buffer and optional producer-authoritative page-store
    /// owner without cloning either allocation graph.
    pub fn into_parts(
        self,
    ) -> (
        String,
        Option<Arc<crate::TilingSolutionPageStore>>,
        Option<ProductPageSourceOwner>,
        u128,
        u128,
    ) {
        (
            self.json,
            self.completed_tiling_solution_page_store,
            self.completed_product_page_source_owner,
            self.memory_limit_bytes,
            self.actual_retained_bytes,
        )
    }
}

pub(crate) fn governed_event_source_carrier_inline_bytes() -> u128 {
    let wrapper = core::mem::size_of::<GovernedWasmWorkerEvents>();
    let parts = core::mem::size_of::<(
        Vec<WasmWorkerJobEvent>,
        Option<Arc<crate::TilingSolutionPageStore>>,
        Option<ProductPageSourceOwner>,
        u128,
        u128,
    )>();
    let result = core::mem::size_of::<Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError>>();
    let preserving_result = core::mem::size_of::<
        Result<GovernedWasmJson, (WasmCommandRuntimeError, GovernedWasmWorkerEvents)>,
    >();
    wrapper.max(parts).max(result).max(preserving_result) as u128
}

#[derive(Clone, Copy)]
enum GovernedJsonRoute {
    DirectReturn,
    AbiOptionStorage,
}

fn governed_json_returned_carrier_inline_bytes_for_route(route: GovernedJsonRoute) -> u128 {
    let wrapper = core::mem::size_of::<GovernedWasmJson>();
    let parts = core::mem::size_of::<(
        String,
        Option<Arc<crate::TilingSolutionPageStore>>,
        Option<ProductPageSourceOwner>,
        u128,
        u128,
    )>();
    let result = core::mem::size_of::<Result<GovernedWasmJson, WasmCommandRuntimeError>>();
    let preserving_result = core::mem::size_of::<
        Result<GovernedWasmJson, (WasmCommandRuntimeError, GovernedWasmWorkerEvents)>,
    >();
    let staged = core::mem::size_of::<StagedGovernedWasmJson>();
    let staged_result =
        core::mem::size_of::<Result<StagedGovernedWasmJson, WasmCommandRuntimeError>>();
    let direct = wrapper
        .max(parts)
        .max(result)
        .max(preserving_result)
        .max(staged)
        .max(staged_result);
    (match route {
        GovernedJsonRoute::DirectReturn => direct,
        GovernedJsonRoute::AbiOptionStorage => {
            direct.max(core::mem::size_of::<Option<GovernedWasmJson>>())
        }
    }) as u128
}

pub(crate) fn governed_json_returned_carrier_inline_bytes() -> u128 {
    governed_json_returned_carrier_inline_bytes_for_route(GovernedJsonRoute::DirectReturn)
}

pub(crate) fn governed_json_abi_storage_carrier_inline_bytes() -> u128 {
    governed_json_returned_carrier_inline_bytes_for_route(GovernedJsonRoute::AbiOptionStorage)
}

pub(crate) fn json_build_carrier_inline_bytes() -> u128 {
    core::mem::size_of::<String>()
        .max(core::mem::size_of::<JsonSink>())
        .max(core::mem::size_of::<StagedGovernedWasmJson>())
        .max(core::mem::size_of::<
            Result<StagedGovernedWasmJson, WasmCommandRuntimeError>,
        >()) as u128
}

/// Consumes a governed event batch while retaining its authority (including a
/// shared page-store owner) through the complete JSON allocation and write.
pub fn serialize_governed_worker_events(
    governed: GovernedWasmWorkerEvents,
) -> Result<GovernedWasmJson, WasmCommandRuntimeError> {
    match serialize_governed_worker_events_preserving_owner(governed) {
        Ok(output) => Ok(output),
        Err((error, governed)) => {
            drop(governed);
            Err(error)
        }
    }
}

/// Stages every fallible JSON operation while the original governed event
/// owner remains intact. The worker runtime uses the returned owner to restore
/// its completed batch transactionally when admission or allocation fails.
pub(crate) fn serialize_governed_worker_events_preserving_owner(
    governed: GovernedWasmWorkerEvents,
) -> Result<GovernedWasmJson, (WasmCommandRuntimeError, GovernedWasmWorkerEvents)> {
    serialize_governed_worker_events_preserving_owner_for_route(
        governed,
        GovernedJsonRoute::DirectReturn,
    )
}

pub(crate) fn serialize_governed_worker_events_for_abi_preserving_owner(
    governed: GovernedWasmWorkerEvents,
) -> Result<GovernedWasmJson, (WasmCommandRuntimeError, GovernedWasmWorkerEvents)> {
    serialize_governed_worker_events_preserving_owner_for_route(
        governed,
        GovernedJsonRoute::AbiOptionStorage,
    )
}

fn serialize_governed_worker_events_preserving_owner_for_route(
    governed: GovernedWasmWorkerEvents,
    route: GovernedJsonRoute,
) -> Result<GovernedWasmJson, (WasmCommandRuntimeError, GovernedWasmWorkerEvents)> {
    let staged = match stage_governed_worker_events_json(&governed, route) {
        Ok(staged) => staged,
        Err(error) => return Err((error, governed)),
    };
    let (events, page_store_owner, product_page_source_owner, confirmed_limit, confirmed_actual) =
        governed.into_serialization_parts();
    debug_assert_eq!(confirmed_limit, staged.memory_limit_bytes);
    debug_assert_eq!(confirmed_actual, staged.source_actual_bytes);
    drop(events);
    Ok(GovernedWasmJson {
        json: staged.json,
        completed_tiling_solution_page_store: page_store_owner,
        completed_product_page_source_owner: product_page_source_owner,
        memory_limit_bytes: staged.memory_limit_bytes,
        actual_retained_bytes: staged.actual_retained_bytes,
    })
}

struct StagedGovernedWasmJson {
    json: String,
    memory_limit_bytes: u128,
    source_actual_bytes: u128,
    actual_retained_bytes: u128,
}

fn stage_governed_worker_events_json(
    governed: &GovernedWasmWorkerEvents,
    route: GovernedJsonRoute,
) -> Result<StagedGovernedWasmJson, WasmCommandRuntimeError> {
    let memory_limit_bytes = governed.memory_limit_bytes();
    let source_actual = governed.actual_retained_bytes();
    let source_wrapper_inline = core::mem::size_of::<GovernedWasmWorkerEvents>() as u128;
    let source_payload_heap = source_actual
        .checked_sub(source_wrapper_inline)
        .ok_or_else(json_projection_error)?;
    let source_carrier_inline = governed_event_source_carrier_inline_bytes();
    let projection_workspace = if governed.completed_tiling_solution_page_store().is_some() {
        crate::TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes(
        )
        .ok_or_else(json_projection_error)?
    } else {
        0
    };
    let projection_peak = source_payload_heap
        .checked_add(source_carrier_inline)
        .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
        .and_then(|bytes| bytes.checked_add(projection_workspace))
        .ok_or_else(json_projection_error)?;
    if projection_peak > memory_limit_bytes {
        return Err(json_limit_error());
    }
    let source_event_heap = governed
        .checked_event_heap_bytes()
        .ok_or_else(json_projection_error)?;
    let shared_page_store_bytes = governed
        .checked_page_store_retained_bytes()
        .ok_or_else(json_projection_error)?;
    let expected_source_actual = source_wrapper_inline
        .checked_add(source_event_heap)
        .and_then(|bytes| bytes.checked_add(shared_page_store_bytes))
        .ok_or_else(json_projection_error)?;
    if expected_source_actual != source_actual {
        return Err(json_projection_error());
    }
    let counting_peak = source_payload_heap
        .checked_add(source_carrier_inline)
        .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
        .ok_or_else(json_projection_error)?;
    if counting_peak > memory_limit_bytes {
        return Err(json_limit_error());
    }
    let exact_len = count_json(|output| write_worker_events(output, governed.events()))?;
    let requested_peak = source_payload_heap
        .checked_add(source_carrier_inline)
        .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
        .and_then(|bytes| bytes.checked_add(exact_len as u128))
        .ok_or_else(json_projection_error)?;
    if requested_peak > memory_limit_bytes {
        return Err(json_limit_error());
    }
    let mut output = String::new();
    output
        .try_reserve_exact(exact_len)
        .map_err(|_| json_allocation_error())?;
    let actual_capacity = output.capacity();
    let actual_peak = source_payload_heap
        .checked_add(source_carrier_inline)
        .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
        .and_then(|bytes| bytes.checked_add(actual_capacity as u128))
        .ok_or_else(json_projection_error)?;
    if actual_peak > memory_limit_bytes {
        return Err(json_limit_error());
    }
    let mut sink = JsonSink::bounded(output, exact_len);
    write_worker_events(&mut sink, governed.events());
    let json = sink.finish_buffer()?;
    let target_payload_heap = (json.capacity() as u128)
        .checked_add(shared_page_store_bytes)
        .ok_or_else(json_projection_error)?;
    let actual_retained_bytes = (core::mem::size_of::<GovernedWasmJson>() as u128)
        .checked_add(target_payload_heap)
        .ok_or_else(json_projection_error)?;
    if actual_retained_bytes > memory_limit_bytes {
        return Err(json_limit_error());
    }
    let final_peak = target_payload_heap
        .checked_add(governed_json_returned_carrier_inline_bytes_for_route(route))
        .ok_or_else(json_projection_error)?;
    if final_peak > memory_limit_bytes {
        return Err(json_limit_error());
    }
    Ok(StagedGovernedWasmJson {
        json,
        memory_limit_bytes,
        source_actual_bytes: source_actual,
        actual_retained_bytes,
    })
}

/// Serializes the exact search payload used by the browser worker without
/// routing a native host through command text or duplicating the report schema.
pub fn serialize_search_report_from_app_response(
    response: &clearra_app::AppResponse,
) -> Option<String> {
    let report = WasmSearchReport::from_response(response)?;
    serialize_json(|output| {
        let mut object = JsonObject::begin(output);
        write_search_report(&mut object, &report);
        object.finish();
    })
    .ok()
}

fn write_event(object: &mut JsonObject<'_>, event: &WasmWorkerJobEvent) {
    match event {
        WasmWorkerJobEvent::Started { job_id } => {
            object.string("event", "started");
            object.number("job_id", job_id.get());
        }
        WasmWorkerJobEvent::Progress { job_id, progress } => {
            object.string("event", "progress");
            object.number("job_id", job_id.get());
            object.object("progress", |nested| write_progress(nested, progress));
        }
        WasmWorkerJobEvent::Diagnostic { job_id, diagnostic } => {
            object.string("event", "diagnostic");
            object.number("job_id", job_id.get());
            object.object("diagnostic", |nested| write_diagnostic(nested, diagnostic));
        }
        WasmWorkerJobEvent::PartialResult {
            job_id,
            partial,
            label,
            final_result,
        } => {
            object.string("event", "partial_result");
            object.number("job_id", job_id.get());
            object.boolean("partial", *partial);
            object.string("label", label);
            object.boolean("final_result", *final_result);
        }
        WasmWorkerJobEvent::FinalResponse {
            job_id,
            response,
            webgpu_backend,
            search_report,
        } => {
            object.string("event", "final_response");
            object.number("job_id", job_id.get());
            object.object("response", |nested| write_app_response(nested, response));
            object.object("webgpu_backend", |nested| {
                write_webgpu_report(nested, webgpu_backend)
            });
            object.optional_object("search_report", search_report.as_ref(), |nested, report| {
                write_search_report(nested, report)
            });
        }
        WasmWorkerJobEvent::Failed {
            job_id,
            diagnostics,
            response,
        } => {
            object.string("event", "failed");
            object.number("job_id", job_id.get());
            object.optional_object("response", response.as_ref(), |nested, response| {
                write_app_response(nested, response)
            });
            match response {
                Some(response) => write_terminal_resource_state(object, response.resource_report()),
                None => write_terminal_execution_state(
                    object,
                    "unavailable",
                    "not-executed",
                    "not-executed",
                ),
            }
            object.object("diagnostics", |nested| {
                write_diagnostic_report(nested, diagnostics)
            });
        }
        WasmWorkerJobEvent::Cancelled {
            job_id,
            scope_released,
        } => {
            object.string("event", "cancelled");
            object.number("job_id", job_id.get());
            object.boolean("scope_released", *scope_released);
            write_terminal_execution_state(
                object,
                "cancelled",
                "cancelled-by-caller",
                "incomplete",
            );
        }
    }
}

fn write_terminal_resource_state(object: &mut JsonObject<'_>, report: &ResourceReport) {
    let availability = report.execution_availability();
    object.object("execution_availability", |nested| {
        nested.string("state", availability.state().as_str());
        nested.optional_string(
            "reason",
            availability.reason().map(|reason| reason.as_str()),
        );
        // This envelope is emitted only by the browser worker ABI. Keep the
        // public surface stable even when the serializer is exercised in a
        // native unit test.
        nested.string("surface", "browser-wasm32");
        nested.optional_string(
            "descriptor_pattern_count",
            availability.descriptor_pattern_count(),
        );
        nested.optional_string("dense_pattern_count", availability.dense_pattern_count());
        nested.optional_string("required_dense_bytes", availability.required_dense_bytes());
        nested.optional_string(
            "required_memory_bytes",
            availability.required_memory_bytes(),
        );
    });
    object.string("result_completeness", report.result_completeness().as_str());
}

fn write_core_resource_report(
    object: &mut JsonObject<'_>,
    report: &clearra_core_domain::resource::ResourceReport,
) {
    object.boolean("solver_executed", report.execution_started());
    object.string(
        "memory_status",
        if report.execution_started() {
            "reported"
        } else {
            "not-executed"
        },
    );
    object.boolean("truncated", report.truncated);
    object.optional_string(
        "truncation_reason",
        report.truncation_reason.map(|reason| reason.as_str()),
    );
    object.number("peak_frontier_states", report.peak_frontier_states);
    object.number("peak_candidate_rows", report.peak_candidate_rows);
    object.number("peak_hash_buckets", report.peak_hash_buckets);
    object.number("peak_gpu_bytes", report.peak_gpu_bytes);
    object.number("peak_cpu_bytes", report.peak_cpu_bytes);
    object.number(
        "build_worker_backlog_peak",
        report.build_worker_backlog_peak,
    );
    object.number("coverage_rows_emitted", report.coverage_rows_emitted);
    object.boolean("probability_complete", report.probability_complete);
    object.object("execution_availability", |nested| {
        let availability = report.execution_availability();
        nested.string("state", availability.state().as_str());
        nested.optional_string(
            "reason",
            availability.reason().map(|reason| reason.as_str()),
        );
        nested.string("surface", "browser-wasm32");
        nested.optional_u128_string(
            "descriptor_pattern_count",
            availability.descriptor_pattern_count(),
        );
        nested.optional_u128_string("dense_pattern_count", availability.dense_pattern_count());
        nested.optional_u128_string("required_dense_bytes", availability.required_dense_bytes());
        nested.optional_u128_string(
            "required_memory_bytes",
            availability.required_memory_bytes(),
        );
    });
    object.string(
        "result_completeness",
        if !report.execution_started() {
            "not-executed"
        } else if !report.result_complete() || report.truncated {
            "incomplete"
        } else {
            "complete"
        },
    );
}

fn write_terminal_execution_state(
    object: &mut JsonObject<'_>,
    state: &str,
    reason: &str,
    completeness: &str,
) {
    object.object("execution_availability", |nested| {
        nested.string("state", state);
        nested.string("reason", reason);
        nested.string("surface", "browser-wasm32");
        nested.optional_string("descriptor_pattern_count", None);
        nested.optional_string("dense_pattern_count", None);
        nested.optional_string("required_dense_bytes", None);
        nested.optional_string("required_memory_bytes", None);
    });
    object.string("result_completeness", completeness);
}

fn write_progress(object: &mut JsonObject<'_>, progress: &JobProgress) {
    object.number("done", progress.done);
    object.number("total", progress.total);
    object.string("label", &progress.label);
    object.object("budget_status", |nested| {
        write_budget_status(nested, &progress.budget_status)
    });
    object.object("backend_status", |nested| {
        write_backend_status(nested, &progress.backend_status)
    });
    object.object("memory_status", |nested| {
        write_memory_status(nested, &progress.memory_status)
    });
}

fn write_budget_status(object: &mut JsonObject<'_>, status: &BudgetStatus) {
    object.string("state", &status.state);
    object.number("used", status.used);
    object.optional_number("limit", status.limit);
}

fn write_backend_status(object: &mut JsonObject<'_>, status: &BackendStatus) {
    object.string("backend_requested", &status.backend_requested);
    object.string("backend_selected", &status.backend_selected);
    object.boolean("fallback_used", status.fallback_used);
    object.optional_string("fallback_reason", status.fallback_reason.as_deref());
}

fn write_memory_status(object: &mut JsonObject<'_>, status: &MemoryStatus) {
    object.string("state", &status.state);
    object.boolean("raw_pointer_exposed", status.raw_pointer_exposed);
}

fn write_app_response(object: &mut JsonObject<'_>, response: &AppResponse) {
    object.object("runtime_identity", |nested| {
        write_runtime_identity(nested, response.runtime_identity())
    });
    object.optional_string(
        "command",
        response.command().map(|command| command.as_str()),
    );
    object.string(
        "status",
        match response.status() {
            AppStatus::Success => "success",
            AppStatus::ValidationFailed => "validation-failed",
            AppStatus::Unsupported => "unsupported",
            AppStatus::ExecutionFailed => "execution-failed",
        },
    );
    object.optional_object("result", response.result(), |nested, result| {
        nested.string("kind", result.kind())
    });
    // Product payloads are an additive, capability-specific surface.  Keep the
    // legacy response byte shape unchanged for commands that do not own one.
    if let Some(payload) = response.product_result_payload() {
        object.object("product_result_payload", |nested| {
            write_product_result_payload(nested, payload)
        });
    }
    if let Some(artifact) = response.solution_set_artifact() {
        object.object("solution_set_artifact", |nested| {
            write_solution_set_artifact(nested, artifact)
        });
    }
    object.array("diagnostics", |output| {
        write_object_array(output, response.diagnostics(), write_diagnostic)
    });
    object.object("backend_report", |nested| {
        write_backend_report(nested, response.backend_report())
    });
    object.object("resource_report", |nested| {
        write_resource_report(nested, response.resource_report())
    });
    object.object("capability_report", |nested| {
        write_capability_report(nested, response.capability_report())
    });
    object.optional_object(
        "continuation",
        response.continuation(),
        write_continuation_report,
    );
}

fn write_solution_set_artifact(object: &mut JsonObject<'_>, artifact: &SolutionSetArtifactPayload) {
    object.string("contract", artifact.contract());
    object.string("source_result_kind", artifact.source_result_kind());
    object.string(
        "source_solution_set_contract",
        artifact.source_solution_set_contract(),
    );
    object.string("selection_kind", artifact.selection_kind());
    object.string("selection_id", artifact.selection_id());
    object.optional_string(
        "page_source_identity_sha256",
        artifact.page_source_identity_sha256(),
    );
    object.string(
        "normalized_key_algorithm",
        artifact.normalized_key_algorithm(),
    );
    object.string(
        "normalized_set_hash_algorithm",
        artifact.normalized_set_hash_algorithm(),
    );
    object.string("normalized_set_hash", artifact.normalized_set_hash());
    object.number("solution_count", artifact.solution_count());
    object.string("completeness", artifact.completeness());
    object.array("formats", |output| {
        write_object_array(output, artifact.formats(), |format_object, format| {
            format_object.string("format", format.format());
            format_object.string("state", format.state());
            format_object.optional_string("unavailable_reason", format.unavailable_reason());
            format_object.optional_string("media_type", format.media_type());
            format_object.optional_string("filename", format.filename());
            format_object.optional_number("byte_length", format.byte_length());
            format_object.optional_string("sha256", format.sha256());
            format_object.optional_number("page_count", format.page_count());
            format_object.optional_string("document", format.document());
        })
    });
}

fn write_product_result_payload(object: &mut JsonObject<'_>, payload: &ProductResultPayload) {
    object.string("contract", payload.contract());
    object.string("result_kind", payload.result_kind());
    object.object("content", |nested| match payload.content() {
        ProductResultPayloadContent::CoveragePortfolio(page) => {
            nested.string("payload_kind", "coverage-portfolio");
            nested.object("payload", |page_object| {
                page_object.string("set_contract", page.set_contract());
                page_object.string("page_contract", page.page_contract());
                page_object.string("member_page_contract", page.member_page_contract());
                page_object.string("set_identity_sha256", page.set_identity_sha256());
                page_object.string("candidate_map_sha256", page.candidate_map_sha256());
                page_object.string("alternative_index", page.alternative_index());
                page_object.string("optimal_cardinality", page.optimal_cardinality());
                page_object.string("known_alternative_count", page.known_alternative_count());
                page_object
                    .optional_string("total_alternative_count", page.total_alternative_count());
                page_object.boolean("enumeration_complete", page.enumeration_complete());
                page_object.string("member_page_number", page.member_page_number());
                page_object.string("total_member_pages", page.total_member_pages());
                page_object.array("members", |output| {
                    write_object_array(output, page.members(), |member_object, member| {
                        member_object.string("candidate_id", member.candidate_id());
                        member_object
                            .string("normalized_solution_key", member.normalized_solution_key());
                    })
                });
                page_object.boolean("page_handle_available", page.page_handle_available());
            });
        }
        ProductResultPayloadContent::BuildCoveragePortfolioV2(portfolio) => {
            nested.string("payload_kind", "build-coverage-portfolio-v2");
            nested.object("payload", |value| {
                value.string("contract", portfolio.contract());
                value.string("objective", portfolio.objective());
                value.string("probability_basis", portfolio.probability_basis());
                value.string("source_candidate_count", portfolio.source_candidate_count());
                value.string(
                    "selected_candidate_count",
                    portfolio.selected_candidate_count(),
                );
                value.string("pattern_count", portfolio.pattern_count());
                value.string("required_pattern_count", portfolio.required_pattern_count());
                value.string("union_probability", portfolio.union_probability());
                value.string(
                    "normalized_solution_set_hash",
                    portfolio.normalized_solution_set_hash(),
                );
                value.string(
                    "canonical_first_candidate_id",
                    portfolio.canonical_first_candidate_id(),
                );
                let completeness = portfolio.completeness();
                value.object("completeness", |evidence| {
                    evidence.boolean(
                        "source_universe_complete",
                        completeness.source_universe_complete(),
                    );
                    evidence.boolean(
                        "coverage_rows_complete",
                        completeness.coverage_rows_complete(),
                    );
                    evidence.boolean(
                        "probability_weights_complete",
                        completeness.probability_weights_complete(),
                    );
                    evidence.boolean("exact_minimum_proven", completeness.exact_minimum_proven());
                    evidence.boolean("query_bound", completeness.query_bound());
                });
                value.boolean("page_source_available", portfolio.page_source_available());
                value.optional_string(
                    "page_source_identity_sha256",
                    portfolio.page_source_identity_sha256(),
                );
            });
        }
        ProductResultPayloadContent::BuildSetupFamilyV1(family) => {
            nested.string("payload_kind", "build-setup-family-v1");
            nested.object("payload", |value| {
                value.string("contract", family.contract());
                value.string("input_identity_sha256", family.input_identity_sha256());
                value.string(
                    "evaluation_identity_sha256",
                    family.evaluation_identity_sha256(),
                );
                value.string("objective", family.objective());
                value.string("source_candidate_count", family.source_candidate_count());
                value.string(
                    "reachable_candidate_count",
                    family.reachable_candidate_count(),
                );
                value.string("pattern_count", family.pattern_count());
                value.string("covered_pattern_count", family.covered_pattern_count());
                value.string("union_probability", family.union_probability());
                let completeness = family.completeness();
                value.object("completeness", |evidence| {
                    evidence.boolean("input_identity_bound", completeness.input_identity_bound());
                    evidence.boolean(
                        "producer_filter_bound",
                        completeness.producer_filter_bound(),
                    );
                    evidence.boolean(
                        "buildability_replay_complete",
                        completeness.buildability_replay_complete(),
                    );
                    evidence.boolean(
                        "coverage_rows_complete",
                        completeness.coverage_rows_complete(),
                    );
                    evidence.boolean(
                        "probability_weights_complete",
                        completeness.probability_weights_complete(),
                    );
                });
                value.array("candidates", |output| {
                    write_object_array(output, family.candidates(), |row, candidate| {
                        row.string("candidate_key", candidate.candidate_key());
                        row.string("covered_pattern_count", candidate.covered_pattern_count());
                    })
                });
            });
        }
        ProductResultPayloadContent::BuildV2(build) => {
            nested.string("payload_kind", "build-v2");
            nested.object("payload", |value| write_build_v2_payload(value, build));
        }
        ProductResultPayloadContent::SetupRankedFamily(family) => {
            nested.string("payload_kind", "setup-ranked-family");
            nested.object("payload", |value| {
                value.string("schema_id", family.schema_id());
                value.string("query_identity_sha256", family.query_identity_sha256());
                value.string("rule_profile", family.rule_profile());
                value.string("supply_identity_sha256", family.supply_identity_sha256());
                value.string(
                    "universe_identity_sha256",
                    family.universe_identity_sha256(),
                );
                value.string("product_build", family.product_build());
                value.string("ordering", family.ordering());
                value.string(
                    "resolved_length_preference",
                    family.resolved_length_preference(),
                );
                value.string("candidate_count", family.candidate_count());
                value.array("candidates", |output| {
                    write_object_array(output, family.candidates(), |row, candidate| {
                        row.string("candidate_id", candidate.candidate_id());
                        row.string("condition_id", candidate.condition_id());
                        row.string("setup_id", candidate.setup_id());
                    })
                });
            });
        }
        ProductResultPayloadContent::SetupScoreRanking(ranking) => {
            nested.string("payload_kind", "setup-score-ranking");
            nested.object("payload", |value| {
                value.string("schema_id", ranking.schema_id());
                value.string("input_identity_sha256", ranking.input_identity_sha256());
                value.string(
                    "evaluation_identity_sha256",
                    ranking.evaluation_identity_sha256(),
                );
                value.string("document_format", ranking.document_format());
                value.string("rule_profile", ranking.rule_profile());
                value.string("score_profile", ranking.score_profile());
                value.string("initial_b2b", ranking.initial_b2b());
                value.string("ordering", ranking.ordering());
                value.string("source_page_count", ranking.source_page_count());
                value.string("candidate_count", ranking.candidate_count());
                value.string("setup_pattern_count", ranking.setup_pattern_count());
                value.string("average_priority_score", ranking.average_priority_score());
                value.boolean("complete", ranking.complete());
                value.array("candidates", |output| {
                    write_object_array(output, ranking.candidates(), |row, candidate| {
                        row.string("rank", candidate.rank());
                        row.string("candidate_id", candidate.candidate_id());
                        row.string("completed_board_mask", candidate.completed_board_mask());
                        row.string(
                            "setup_covered_pattern_count",
                            candidate.setup_covered_pattern_count(),
                        );
                        row.string(
                            "setup_covered_probability",
                            candidate.setup_covered_probability(),
                        );
                        row.string(
                            "continuation_probability",
                            candidate.continuation_probability(),
                        );
                        row.string(
                            "unconditional_expected_score",
                            candidate.unconditional_expected_score(),
                        );
                    })
                });
            });
        }
        ProductResultPayloadContent::SpinStructureFamily(family) => {
            nested.string("payload_kind", "spin-structure-family");
            nested.object("payload", |value| {
                value.string("schema_id", family.schema_id());
                value.string("query_identity_sha256", family.query_identity_sha256());
                value.string("rule_profile", family.rule_profile());
                value.string("spin_profile", family.spin_profile());
                value.string("supply_identity_sha256", family.supply_identity_sha256());
                value.string(
                    "universe_identity_sha256",
                    family.universe_identity_sha256(),
                );
                value.string("product_build", family.product_build());
                value.string("ordering", family.ordering());
                value.optional_string("minimum_placements", family.minimum_placements());
                value.optional_string("guaranteed_final_piece", family.guaranteed_final_piece());
                value.optional_string("guarantee_basis", family.guarantee_basis());
                value.optional_boolean(
                    "dependency_report_included",
                    family.dependency_report_included(),
                );
                value.optional_string("dependency_relation", family.dependency_relation());
                value.optional_string("dependency_edge_count", family.dependency_edge_count());
                value.string("regular_count", family.regular_count());
                value.string("mini_count", family.mini_count());
                value.string("candidate_count", family.candidate_count());
                value.boolean("complete", family.complete());
                value.array("candidates", |output| {
                    write_object_array(output, family.candidates(), |row, candidate| {
                        row.string("candidate_id", candidate.candidate_id());
                        row.string("partition", candidate.partition());
                        row.string("placement_count", candidate.placement_count());
                    })
                });
            });
        }
        ProductResultPayloadContent::ScorePatternWinnerFamily(family) => {
            nested.string("payload_kind", "score-pattern-winner-family");
            nested.object("payload", |family_object| {
                family_object.string("winner_contract", family.winner_contract());
                family_object.string("ordering", family.ordering());
                family_object.string("equality", family.equality());
                family_object.string(
                    "informational_attack_basis",
                    family.informational_attack_basis(),
                );
                family_object.string("page_size", family.page_size());
                family_object.string("winner_count", family.winner_count());
                family_object.array("winners", |output| {
                    write_object_array(output, family.winners(), |winner_object, winner| {
                        winner_object.string("pattern_id", winner.pattern_id());
                        winner_object.string("candidate_id", winner.candidate_id());
                        winner_object
                            .string("normalized_solution_key", winner.normalized_solution_key());
                        winner_object.string("score", winner.score());
                        winner_object.string("informational_attack", winner.informational_attack());
                    })
                });
            });
        }
        ProductResultPayloadContent::PcPathFamily(family) => {
            nested.string("payload_kind", "pc-path-family");
            nested.object("payload", |family_object| {
                family_object.string("witness_contract", family.witness_contract());
                family_object.string("ordering", family.ordering());
                family_object.string("problem_id", family.problem_id());
                family_object.string(
                    "materialized_pattern_count",
                    family.materialized_pattern_count(),
                );
                family_object.string("witness_count", family.witness_count());
                family_object.boolean("complete", family.complete());
                family_object.array("witnesses", |output| {
                    write_object_array(output, family.witnesses(), |witness_object, witness| {
                        witness_object.string("candidate_id", witness.candidate_id());
                        witness_object
                            .string("producer_candidate_id", witness.producer_candidate_id());
                        witness_object.string("pattern_id", witness.pattern_id());
                        witness_object.string("trace_identity", witness.trace_identity());
                        witness_object
                            .string("normalized_trace_key", witness.normalized_trace_key());
                        witness_object
                            .string("consumed_piece_count", witness.consumed_piece_count());
                        witness_object
                            .optional_string("terminal_hold_piece", witness.terminal_hold_piece());
                        witness_object.array("steps", |step_output| {
                            write_object_array(step_output, witness.steps(), |step_object, step| {
                                step_object.string("step_index", step.step_index());
                                step_object.string("operation_id", step.operation_id());
                                step_object.string("active_piece", step.active_piece());
                                step_object.string("input_cursor", step.input_cursor());
                                step_object.string("output_cursor", step.output_cursor());
                                step_object
                                    .optional_string("input_hold_piece", step.input_hold_piece());
                                step_object
                                    .optional_string("output_hold_piece", step.output_hold_piece());
                                step_object.string("hold_decision", step.hold_decision());
                                step_object.string("rotation", step.rotation());
                                step_object.string("x", step.x());
                                step_object.string("y", step.y());
                                step_object.string("placement_mask", step.placement_mask());
                                step_object.string("board_before_mask", step.board_before_mask());
                                step_object.string(
                                    "board_after_placement_mask",
                                    step.board_after_placement_mask(),
                                );
                                step_object.string(
                                    "board_after_line_clear_mask",
                                    step.board_after_line_clear_mask(),
                                );
                                step_object.string("cleared_row_mask", step.cleared_row_mask());
                                step_object.string("cleared_lines", step.cleared_lines());
                                step_object
                                    .string("line_clear_identity", step.line_clear_identity());
                            })
                        });
                    })
                });
            });
        }
        ProductResultPayloadContent::PcSaveGroups(family) => {
            nested.string("payload_kind", "pc-save-groups");
            nested.object("payload", |family_object| {
                family_object.string("schema_id", family.schema_id());
                family_object.string("page_size", family.page_size());
                family_object.string("group_count", family.group_count());
                family_object.object("metadata", |metadata_object| {
                    write_pc_save_metadata(metadata_object, family.metadata())
                });
                family_object.array("groups", |output| {
                    write_object_array(output, family.groups(), write_pc_save_group)
                });
            });
        }
        ProductResultPayloadContent::PcBestSave(family) => {
            nested.string("payload_kind", "pc-best-save");
            nested.object("payload", |family_object| {
                family_object.string("schema_id", family.schema_id());
                family_object.string("probability_basis", family.probability_basis());
                family_object.string("ordering", family.ordering());
                family_object.string("equality", family.equality());
                family_object.string("page_size", family.page_size());
                family_object.string("winner_count", family.winner_count());
                family_object.object("metadata", |metadata_object| {
                    write_pc_save_metadata(metadata_object, family.metadata())
                });
                family_object.array("winners", |output| {
                    write_object_array(output, family.winners(), |winner_object, winner| {
                        winner_object.string("weighted_total", winner.weighted_total());
                        winner_object.string("balanced_jl_count", winner.balanced_jl_count());
                        winner_object
                            .string("exact_group_probability", winner.exact_group_probability());
                        winner_object.object("group", |group_object| {
                            write_pc_save_group(group_object, winner.group())
                        });
                    })
                });
            });
        }
        ProductResultPayloadContent::ParityReportPage(page) => {
            nested.string("payload_kind", "parity-report-page");
            nested.object("payload", |page_object| {
                page_object.string("document_format", page.document_format());
                page_object.number("page_number", page.page_number());
                page_object.number("total_pages", page.total_pages());
                page_object.string("coordinate_basis", page.coordinate_basis());
                page_object.number("width", page.width());
                page_object.number("height", page.height());
                page_object.number("occupied_cell_count", page.occupied_cell_count());
                page_object.number("checker_black_count", page.checker_black_count());
                page_object.number("checker_white_count", page.checker_white_count());
                page_object.number("checker_delta", page.checker_delta());
                page_object.array("four_color_counts", |output| {
                    output.push('[');
                    for (index, count) in page.four_color_counts().iter().enumerate() {
                        if index != 0 {
                            output.push(',');
                        }
                        let _ = write!(output, "{count}");
                    }
                    output.push(']');
                });
                page_object.number("even_column_count", page.even_column_count());
                page_object.number("odd_column_count", page.odd_column_count());
                page_object.number("column_parity_delta", page.column_parity_delta());
                page_object.number("occupied_area_mod_four", page.occupied_area_mod_four());
                page_object.number(
                    "pending_garbage_occupied_cell_count",
                    page.pending_garbage_occupied_cell_count(),
                );
                page_object.boolean("feasibility_claim", page.feasibility_claim());
                page_object.string("pruning_authority", page.pruning_authority());
                page_object.boolean("page_handle_available", page.page_handle_available());
            });
        }
        ProductResultPayloadContent::FieldDocument(document) => {
            nested.string("payload_kind", "field-document");
            nested.object("payload", |document_object| {
                write_field_document_payload(document_object, document)
            });
        }
        ProductResultPayloadContent::FieldDocumentSet(set) => {
            nested.string("payload_kind", "field-document-set");
            nested.object("payload", |set_object| {
                set_object.string("document_contract", set.document_contract());
                set_object.array("documents", |output| {
                    write_object_array(output, set.documents(), write_field_document_payload)
                });
            });
        }
        ProductResultPayloadContent::RenderArtifact(artifact) => {
            nested.string("payload_kind", "render-artifact");
            nested.object("payload", |artifact_object| {
                artifact_object.string("document_format", artifact.document_format());
                artifact_object.string("artifact_format", artifact.artifact_format());
                artifact_object
                    .optional_number("selected_page_number", artifact.selected_page_number());
                artifact_object.number("document_page_count", artifact.document_page_count());
                artifact_object.string("media_type", artifact.media_type());
                artifact_object.string("filename", artifact.filename());
                artifact_object.number("byte_length", artifact.byte_length());
                artifact_object.string("sha256", artifact.sha256());
                artifact_object.string("bytes_base64", artifact.bytes_base64());
                artifact_object.boolean("render_exact", artifact.render_exact());
                artifact_object.string("skin_id", artifact.skin_id());
                artifact_object.number("product_max_bytes", artifact.product_max_bytes());
                artifact_object.number("transport_max_bytes", artifact.transport_max_bytes());
            });
        }
    });
}

fn write_build_v2_payload(
    object: &mut JsonObject<'_>,
    payload: &clearra_host_contract::BuildV2ProductPayload,
) {
    object.string(
        "kind",
        match payload.kind() {
            clearra_host_contract::BuildV2PayloadKind::CandidateFamily => "candidate-family",
            clearra_host_contract::BuildV2PayloadKind::Probability => "probability",
            clearra_host_contract::BuildV2PayloadKind::Portfolio => "portfolio",
            clearra_host_contract::BuildV2PayloadKind::ScorePortfolio => "score-portfolio",
        },
    );
    object.string("capability_id", payload.capability_id());
    object.string("result_contract", payload.result_contract());
    object.string("input_identity_sha256", payload.input_identity_sha256());
    object.optional_string(
        "evaluation_identity_sha256",
        payload.evaluation_identity_sha256(),
    );
    object.optional_string("replay_basis", payload.replay_basis());
    object.string("objective", payload.objective());
    object.optional_string("score_profile", payload.score_profile());
    object.optional_string("initial_b2b", payload.initial_b2b());
    object.optional_string("score_accuracy", payload.score_accuracy());
    object.optional_boolean("profile_specific_exact", payload.profile_specific_exact());
    object.optional_string("score_equality_basis", payload.score_equality_basis());
    object.optional_string(
        "informational_attack_basis",
        payload.informational_attack_basis(),
    );
    object.string("source_candidate_count", payload.source_candidate_count());
    object.string(
        "reachable_candidate_count",
        payload.reachable_candidate_count(),
    );
    object.optional_string(
        "selected_candidate_count",
        payload.selected_candidate_count(),
    );
    object.string("pattern_count", payload.pattern_count());
    object.optional_string("covered_pattern_count", payload.covered_pattern_count());
    object.optional_string("required_pattern_count", payload.required_pattern_count());
    object.optional_string("union_probability", payload.union_probability());
    object.optional_boolean(
        "b2b_preservation_required",
        payload.b2b_preservation_required(),
    );
    object.array("candidates", |output| {
        write_object_array(output, payload.candidates(), |row, candidate| {
            row.string("candidate_key", candidate.candidate_key());
            row.string("covered_pattern_count", candidate.covered_pattern_count());
        })
    });
    object.array("canonical_candidate_keys", |output| {
        write_string_array(output, payload.canonical_candidate_keys())
    });
    object.array("winners", |output| {
        write_object_array(output, payload.winners(), |row, winner| {
            row.string("pattern_id", winner.pattern_id());
            row.string("candidate_key", winner.candidate_key());
            row.string("score", winner.score());
            row.string("informational_attack", winner.informational_attack());
        })
    });
    let completeness = payload.completeness();
    object.object("completeness", |evidence| {
        evidence.boolean("input_identity_bound", completeness.input_identity_bound());
        evidence.boolean(
            "producer_filter_bound",
            completeness.producer_filter_bound(),
        );
        evidence.boolean(
            "buildability_replay_complete",
            completeness.buildability_replay_complete(),
        );
        evidence.boolean(
            "coverage_rows_complete",
            completeness.coverage_rows_complete(),
        );
        evidence.boolean(
            "probability_weights_complete",
            completeness.probability_weights_complete(),
        );
        evidence.boolean("exact_minimum_proven", completeness.exact_minimum_proven());
        evidence.boolean(
            "score_evidence_complete",
            completeness.score_evidence_complete(),
        );
    });
    object.boolean("page_source_available", payload.page_source_available());
    object.optional_string(
        "page_source_identity_sha256",
        payload.page_source_identity_sha256(),
    );
}

fn write_pc_save_metadata(
    object: &mut JsonObject<'_>,
    metadata: &clearra_host_contract::PcSaveRunMetadataPayload,
) {
    object.string("origin", metadata.origin());
    object.string("problem_preset", metadata.problem_preset());
    object.string("problem_id", metadata.problem_id());
    object.string("piece_source_id", metadata.piece_source_id());
    object.string("pattern_universe_id", metadata.pattern_universe_id());
    object.string(
        "pattern_weight_model_id",
        metadata.pattern_weight_model_id(),
    );
    object.string(
        "materialized_pattern_count",
        metadata.materialized_pattern_count(),
    );
    object.string(
        "pc_success_pattern_count",
        metadata.pc_success_pattern_count(),
    );
    object.string("pc_probability", metadata.pc_probability());
    object.object("completeness", |completeness_object| {
        let completeness = metadata.completeness();
        completeness_object.boolean(
            "source_universe_complete",
            completeness.source_universe_complete(),
        );
        completeness_object.boolean(
            "fixed_bag_boundary_proven",
            completeness.fixed_bag_boundary_proven(),
        );
        completeness_object.boolean(
            "execution_batch_complete",
            completeness.execution_batch_complete(),
        );
        completeness_object.boolean(
            "pattern_weights_complete",
            completeness.pattern_weights_complete(),
        );
        completeness_object.boolean("count_complete", completeness.count_complete());
        completeness_object.boolean("probability_complete", completeness.probability_complete());
        completeness_object.boolean("complete", completeness.complete());
    });
}

fn write_pc_save_group(
    object: &mut JsonObject<'_>,
    group: &clearra_host_contract::PcSaveGroupPayload,
) {
    object.string("identity_contract", group.identity_contract());
    object.object("identity", |identity_object| {
        write_pc_save_piece_multiset(identity_object, group.identity())
    });
    object.string("successful_pattern_count", group.successful_pattern_count());
    object.string(
        "unconditional_probability",
        group.unconditional_probability(),
    );
    object.string(
        "conditional_probability_given_pc",
        group.conditional_probability_given_pc(),
    );
    object.string("canonical_candidate_id", group.canonical_candidate_id());
    object.array("witnesses", |output| {
        write_object_array(output, group.witnesses(), |witness_object, witness| {
            witness_object.string("pattern_index", witness.pattern_index());
            witness_object.string("candidate_id", witness.candidate_id());
            witness_object.string("trace_identity", witness.trace_identity());
            witness_object.string("source_cursor", witness.source_cursor());
            witness_object.optional_string("terminal_hold", witness.terminal_hold());
            witness_object.object("active_bag_remainder", |remainder_object| {
                write_pc_save_piece_multiset(remainder_object, witness.active_bag_remainder())
            });
        })
    });
}

fn write_pc_save_piece_multiset(
    object: &mut JsonObject<'_>,
    multiset: &clearra_host_contract::PcSavePieceMultisetPayload,
) {
    let [t, i, o, j, l, s, z] = multiset.counts();
    object.string("canonical_id", multiset.canonical_id());
    object.number("t", t);
    object.number("i", i);
    object.number("o", o);
    object.number("j", j);
    object.number("l", l);
    object.number("s", s);
    object.number("z", z);
    object.number("total_count", multiset.total_count());
}

fn write_field_document_payload(
    object: &mut JsonObject<'_>,
    document: &clearra_host_contract::FieldDocumentPayload,
) {
    object.string("format", document.format());
    object.string("document", document.document());
    object.number("page_count", document.page_count());
    object.string("canonical_sha256", document.canonical_sha256());
    object.string("filename", document.filename());
}

fn write_runtime_identity(object: &mut JsonObject<'_>, identity: &ProductBuildIdentity) {
    object.string("engine_build_id", identity.engine_build_id());
    object.string("source_commit", identity.source_commit());
    object.string(
        "contract_schema_version",
        identity.contract_schema_version(),
    );
    object.string("supply_semantics_id", identity.supply_semantics_id());
    object.string(
        "artifact_schema_version",
        identity.artifact_schema_version(),
    );
}

fn write_diagnostic(object: &mut JsonObject<'_>, diagnostic: &Diagnostic) {
    object.string("code", diagnostic.code());
    object.string("severity", diagnostic.severity());
    object.string("message", diagnostic.message());
}

fn write_diagnostic_report(object: &mut JsonObject<'_>, report: &DiagnosticReport) {
    object.array("diagnostics", |output| {
        write_object_array(output, report.diagnostics(), write_diagnostic)
    });
}

fn write_backend_report(object: &mut JsonObject<'_>, report: &BackendReport) {
    object.string("backend_requested", report.backend_requested());
    object.string("backend_selected", report.backend_selected());
    object.boolean("fallback_used", report.fallback_used());
    object.optional_string("fallback_reason", report.fallback_reason());
    object.optional_string("backend_fallback_reason", report.backend_fallback_reason());
    object.optional_string("fallback_backend", report.fallback_backend());
    object.optional_string("gpu_failure_class", report.gpu_failure_class());
    object.optional_string("gpu_failure_stage", report.gpu_failure_stage());
    object.boolean(
        "discarded_partial_gpu_result",
        report.discarded_partial_gpu_result(),
    );
    object.optional_string("gpu_device_requested", report.gpu_device_requested());
    object.optional_number(
        "gpu_device_selected_index",
        report.gpu_device_selected_index(),
    );
    object.optional_string(
        "gpu_device_selected_name",
        report.gpu_device_selected_name(),
    );
    object.optional_string(
        "gpu_device_selected_type",
        report.gpu_device_selected_type(),
    );
    object.optional_string(
        "gpu_device_selected_backend",
        report.gpu_device_selected_backend(),
    );
}

fn write_resource_report(object: &mut JsonObject<'_>, report: &ResourceReport) {
    object.boolean("solver_executed", report.solver_executed());
    object.string("memory_status", report.memory_status());
    object.boolean("truncated", report.truncated);
    object.optional_string("truncation_reason", report.truncation_reason.as_deref());
    object.number("peak_frontier_states", report.peak_frontier_states);
    object.number("peak_candidate_rows", report.peak_candidate_rows);
    object.number("peak_hash_buckets", report.peak_hash_buckets);
    object.number("peak_gpu_bytes", report.peak_gpu_bytes);
    object.number("peak_cpu_bytes", report.peak_cpu_bytes);
    object.number(
        "build_worker_backlog_peak",
        report.build_worker_backlog_peak,
    );
    object.number("coverage_rows_emitted", report.coverage_rows_emitted);
    object.boolean("probability_complete", report.probability_complete);
    object.object("execution_availability", |nested| {
        let availability = report.execution_availability();
        nested.string("state", availability.state().as_str());
        nested.optional_string(
            "reason",
            availability.reason().map(|reason| reason.as_str()),
        );
        // AppResponse is nested in the browser worker ABI here. Its public
        // surface must match the terminal envelope even in native unit tests.
        nested.string("surface", "browser-wasm32");
        nested.optional_string(
            "descriptor_pattern_count",
            availability.descriptor_pattern_count(),
        );
        nested.optional_string("dense_pattern_count", availability.dense_pattern_count());
        nested.optional_string("required_dense_bytes", availability.required_dense_bytes());
        nested.optional_string(
            "required_memory_bytes",
            availability.required_memory_bytes(),
        );
    });
    object.string("result_completeness", report.result_completeness().as_str());
}

fn write_capability_report(object: &mut JsonObject<'_>, report: &CapabilityReport) {
    object.string("app_request_boundary", report.app_request_boundary());
    object.string("executor_boundary", report.executor_boundary());
    object.optional_object(
        "render_capability",
        report.render_capability(),
        |nested, render| {
            nested.boolean("png_supported", render.png_supported());
            nested.boolean("gif_supported", render.gif_supported());
            nested.boolean("render_exact", render.render_exact());
            nested.optional_string("unsupported_reason", render.unsupported_reason());
        },
    );
}

fn write_continuation_report(object: &mut JsonObject<'_>, report: &ContinuationReport) {
    object.boolean("available", report.available());
    object.optional_string("token", report.token());
}

fn write_webgpu_report(object: &mut JsonObject<'_>, report: &WebGpuBackendReport) {
    object.string(
        "outcome_state",
        match report.outcome_state {
            WebGpuBackendOutcomeState::NotRequested => "NotRequested",
            WebGpuBackendOutcomeState::Connected => "Connected",
            WebGpuBackendOutcomeState::Unavailable => "Unavailable",
        },
    );
    object.boolean("webgpu_available", report.webgpu_available);
    object.string(
        "webgpu_adapter_label_or_redacted",
        &report.webgpu_adapter_label_or_redacted,
    );
    object.object("webgpu_limits", |nested| {
        write_webgpu_limits(nested, &report.webgpu_limits)
    });
    object.object("webgpu_required_limits", |nested| {
        write_webgpu_limits(nested, &report.webgpu_required_limits)
    });
    object.optional_string(
        "webgpu_unavailable_reason",
        report.webgpu_unavailable_reason.as_deref(),
    );
    object.optional_string("expected_digest", report.expected_digest.as_deref());
    object.optional_string("actual_digest", report.actual_digest.as_deref());
    object.object("shader", |nested| {
        write_webgpu_shader(nested, &report.shader)
    });
    object.object("memory", |nested| {
        write_webgpu_memory(nested, &report.memory)
    });
    object.boolean("fallback_used", report.fallback_used);
    object.optional_string("fallback_backend", report.fallback_backend.as_deref());
    object.boolean("gpu_warmup_requested", report.gpu_warmup_requested);
    object.boolean("gpu_warmup_performed", report.gpu_warmup_performed);
    object.boolean("gpu_session_reused", report.gpu_session_reused);
    object.string(
        "gpu_trust_state",
        match report.gpu_trust_state {
            WebGpuReportTrustState::NotUsed => "NotUsed",
            WebGpuReportTrustState::TrustedCpuSampleConfirmed => "TrustedCpuSampleConfirmed",
            WebGpuReportTrustState::Unavailable => "Unavailable",
        },
    );
    object.boolean("cpu_confirmed", report.cpu_confirmed);
    object.boolean(
        "can_source_exact_probability",
        report.can_source_exact_probability,
    );
}

fn write_webgpu_limits(object: &mut JsonObject<'_>, report: &WebGpuLimitsReport) {
    object.number(
        "max_storage_buffer_binding_size",
        report.max_storage_buffer_binding_size,
    );
    object.number(
        "max_compute_workgroup_storage_size",
        report.max_compute_workgroup_storage_size,
    );
    object.number(
        "max_compute_invocations_per_workgroup",
        report.max_compute_invocations_per_workgroup,
    );
}

fn write_webgpu_shader(object: &mut JsonObject<'_>, report: &WebGpuShaderReport) {
    object.string("shader_compile_status", &report.shader_compile_status);
    object.optional_string("shader_hash", report.shader_hash.as_deref());
    object.optional_string("shader_version", report.shader_version.as_deref());
    object.boolean("embedded_reviewed", report.embedded_reviewed);
    object.boolean("user_shader_allowed", report.user_shader_allowed);
    object.boolean(
        "runtime_shader_injection_allowed",
        report.runtime_shader_injection_allowed,
    );
}

fn write_webgpu_memory(object: &mut JsonObject<'_>, report: &WebGpuMemoryReport) {
    object.string("wasm_memory_usage", &report.wasm_memory_usage);
    object.string("wasm_memory_pressure", &report.wasm_memory_pressure);
}

fn write_search_report(object: &mut JsonObject<'_>, report: &WasmSearchReport) {
    object.string("backend_selected", &report.backend_selected);
    object.number("workers_used", report.workers_used);
    object.boolean("cpu_parallel_execution", report.cpu_parallel_execution);
    object.string(
        "cpu_parallel_decision_reason",
        &report.cpu_parallel_decision_reason,
    );
    object.boolean("cpu_warmup_requested", report.cpu_warmup_requested);
    object.boolean("cpu_warmup_performed", report.cpu_warmup_performed);
    object.string("supply_window_resolution", &report.supply_window_resolution);
    object.boolean(
        "projects_unplaced_lookahead",
        report.projects_unplaced_lookahead,
    );
    object.boolean(
        "projects_standard_bag_lookahead",
        report.projects_standard_bag_lookahead,
    );
    object.number("source_sequence_length", report.source_sequence_length);
    object.string(
        "total_possible_pattern_count",
        &report.total_possible_pattern_count,
    );
    object.boolean("solution_found", report.solution_found);
    object.number("packing_candidate_count", report.packing_candidate_count);
    object.string(
        "geometry_candidate_family_count",
        &report.geometry_candidate_family_count,
    );
    object.string(
        "packing_candidate_set_digest",
        &report.packing_candidate_set_digest,
    );
    object.array("packing_candidate_keys", |output| {
        write_string_array(output, &report.packing_candidate_keys)
    });
    object.number("unique_solution_count", report.unique_solution_count);
    object.boolean(
        "solution_count_calculated",
        report.solution_count_calculated,
    );
    object.boolean(
        "solution_set_materialized",
        report.solution_set_materialized,
    );
    object.number(
        "solution_keys_materialized_count",
        report.solution_keys_materialized_count,
    );
    object.boolean("solution_keys_complete", report.solution_keys_complete);
    object.boolean("solution_page_available", report.solution_page_available);
    object.string(
        "normalized_solution_set_hash",
        &report.normalized_solution_set_hash,
    );
    object.array("normalized_solution_keys", |output| {
        write_string_array(output, &report.normalized_solution_keys)
    });
    object.array("solution_probabilities", |output| {
        output.push('[');
        for (index, entry) in report.solution_probabilities.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("solution_key", &entry.solution_key);
            nested.string("probability", &entry.probability);
            nested.number("covered_pattern_count", entry.covered_pattern_count);
            nested.number("pattern_count", entry.pattern_count);
            nested.boolean("probability_complete", entry.probability_complete);
            nested.finish();
        }
        output.push(']');
    });
    object.array("solution_average_scores", |output| {
        output.push('[');
        for (index, entry) in report.solution_average_scores.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("solution_key", &entry.solution_key);
            nested.string("average_score", &entry.average_score);
            nested.number("covered_pattern_count", entry.covered_pattern_count);
            nested.number("pattern_count", entry.pattern_count);
            nested.boolean("score_complete", entry.score_complete);
            nested.finish();
        }
        output.push(']');
    });
    object.number("build_variant_count", report.build_variant_count);
    object.string(
        "build_variant_count_exact",
        &report.build_variant_count_exact,
    );
    object.boolean("buildability_verified", report.buildability_verified);
    object.boolean("coverage_calculated", report.coverage_calculated);
    object.boolean("probability_calculated", report.probability_calculated);
    object.number(
        "materialized_pattern_count",
        report.materialized_pattern_count,
    );
    object.number("covered_pattern_count", report.covered_pattern_count);
    object.string("coverage_probability", &report.coverage_probability);
    object.boolean("probability_complete", report.probability_complete);
    object.boolean("count_complete", report.count_complete);
    object.number("searched_nodes", report.searched_nodes);
    object.number(
        "geometry_domain_pruned_states",
        report.geometry_domain_pruned_states,
    );
    object.number(
        "geometry_hall_pruned_states",
        report.geometry_hall_pruned_states,
    );
    object.number(
        "geometry_column_pruned_states",
        report.geometry_column_pruned_states,
    );
    object.number(
        "geometry_component_compositions",
        report.geometry_component_compositions,
    );
    object.number("peak_frontier_states", report.peak_frontier_states);
    object.number("peak_cpu_bytes", report.peak_cpu_bytes);
    object.number("peak_build_order_nodes", report.peak_build_order_nodes);
    object.number("total_build_order_nodes", report.total_build_order_nodes);
    object.number("coverage_product_words", report.coverage_product_words);
    object.number("coverage_product_states", report.coverage_product_states);
    object.number(
        "coverage_product_edge_checks",
        report.coverage_product_edge_checks,
    );
    object.number(
        "piece_language_coverage_cache_hits",
        report.piece_language_coverage_cache_hits,
    );
    object.number(
        "piece_language_coverage_cache_misses",
        report.piece_language_coverage_cache_misses,
    );
    object.number(
        "standard_bag_symbolic_cache_hits",
        report.standard_bag_symbolic_cache_hits,
    );
    object.number(
        "standard_bag_symbolic_cache_misses",
        report.standard_bag_symbolic_cache_misses,
    );
    object.number("peak_reachability_states", report.peak_reachability_states);
    object.number(
        "total_reachability_states",
        report.total_reachability_states,
    );
    object.number(
        "reachability_lock_queries",
        report.reachability_lock_queries,
    );
    object.number(
        "reachability_harddrop_queries",
        report.reachability_harddrop_queries,
    );
    object.number(
        "reachability_harddrop_hits",
        report.reachability_harddrop_hits,
    );
    object.number(
        "reachability_cache_reachable_hits",
        report.reachability_cache_reachable_hits,
    );
    object.number(
        "reachability_cache_unreachable_hits",
        report.reachability_cache_unreachable_hits,
    );
    object.number(
        "reachability_cache_key_misses",
        report.reachability_cache_key_misses,
    );
    object.number(
        "reachability_partial_searches",
        report.reachability_partial_searches,
    );
    object.number(
        "reachability_exhaustive_searches",
        report.reachability_exhaustive_searches,
    );
    object.number(
        "realization_feasibility_states",
        report.realization_feasibility_states,
    );
    object.number(
        "realization_feasibility_rejected_candidates",
        report.realization_feasibility_rejected_candidates,
    );
    object.boolean("resource_truncated", report.resource_truncated);
    object.string(
        "resource_truncation_reason",
        &report.resource_truncation_reason,
    );
    object.optional_string(
        "representative_candidate_id",
        report.representative_candidate_id.as_deref(),
    );
    object.optional_number(
        "representative_pattern_id",
        report.representative_pattern_id,
    );
    object.array("representative_path", |output| {
        output.push('[');
        for (index, step) in report.representative_path.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("piece", &step.piece);
            nested.number("rotation", step.rotation);
            nested.number("x", step.x);
            nested.number("y", step.y);
            nested.string("hold", &step.hold);
            nested.number("cleared_lines", step.cleared_lines);
            nested.finish();
        }
        output.push(']');
    });
    object.array("summary_fields", |output| {
        output.push('[');
        for (index, (key, value)) in report.summary_fields.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push('[');
            write_json_string(output, key);
            output.push(',');
            write_json_string(output, value);
            output.push(']');
        }
        output.push(']');
    });
    object.optional_string("forward_search_kind", report.forward_search_kind.as_deref());
    object.optional_string(
        "forward_initial_board_mask",
        report.forward_initial_board_mask.as_deref(),
    );
    object.optional_number("maximum_damage", report.maximum_damage);
    object.optional_number("maximum_ren", report.maximum_ren);
    object.array("forward_outcomes", |output| {
        output.push('[');
        for (index, outcome) in report.forward_outcomes.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("id", &outcome.id);
            nested.number("source_pattern_index", outcome.source_pattern_index);
            nested.string("source_queue", &outcome.source_queue);
            nested.optional_string("group", outcome.group.as_deref());
            nested.string("final_board_mask", &outcome.final_board_mask);
            nested.optional_string("spin_piece", outcome.spin_piece.as_deref());
            nested.boolean("spin_mini", outcome.spin_mini);
            nested.number("spin_lines", outcome.spin_lines);
            nested.optional_number("ren_count", outcome.ren_count);
            nested.number("total_damage", outcome.total_damage);
            nested.string("evidence_path_count", &outcome.evidence_path_count);
            nested.boolean("evidence_complete", outcome.evidence_complete);
            nested.array("path", |output| {
                output.push('[');
                for (step_index, step) in outcome.path.iter().enumerate() {
                    if step_index != 0 {
                        output.push(',');
                    }
                    let mut step_object = JsonObject::begin(output);
                    step_object.string("piece", &step.piece);
                    step_object.number("rotation", step.rotation);
                    step_object.number("x", step.x);
                    step_object.number("y", step.y);
                    step_object.string("hold", &step.hold);
                    step_object.number("cleared_lines", step.cleared_lines);
                    step_object.optional_string("spin_piece", step.spin_piece.as_deref());
                    step_object.boolean("spin_mini", step.spin_mini);
                    step_object.number("damage", step.damage);
                    step_object.number("total_damage", step.total_damage);
                    step_object.string("placement_mask", &step.placement_mask);
                    step_object.number("cleared_row_mask", step.cleared_row_mask);
                    step_object.string("board_after_mask", &step.board_after_mask);
                    step_object.finish();
                }
                output.push(']');
            });
            nested.finish();
        }
        output.push(']');
    });
    object.optional_object(
        "setup_report",
        report.setup_report.as_ref(),
        |nested, setup| {
            nested.string("search_mode", &setup.search_mode);
            nested.number("cycle", setup.cycle);
            nested.string("remaining_pieces", &setup.remaining_pieces);
            nested.string("queue_based_pieces", &setup.queue_based_pieces);
            nested.string(
                "next_cycle_remaining_pieces",
                &setup.next_cycle_remaining_pieces,
            );
            nested.boolean("post_cycle_borrow_enabled", setup.post_cycle_borrow_enabled);
            nested.string("coverage_semantics", &setup.coverage_semantics);
            nested.string(
                "continuation_supply_semantics",
                &setup.continuation_supply_semantics,
            );
            nested.string("geometry_family_count", &setup.geometry_family_count);
            nested.number("partial_build_node_count", setup.partial_build_node_count);
            nested.boolean("complete", setup.complete);
            nested.array("hold_conditions", |output| {
                output.push('[');
                for (condition_index, condition) in setup.hold_conditions.iter().enumerate() {
                    if condition_index != 0 {
                        output.push(',');
                    }
                    let mut condition_object = JsonObject::begin(output);
                    condition_object.string("condition_id", &condition.condition_id);
                    condition_object
                        .optional_string("initial_hold", condition.initial_hold.as_deref());
                    condition_object.string("pattern_expression", &condition.pattern_expression);
                    condition_object.number("pattern_count", condition.pattern_count);
                    condition_object.number("candidate_count", condition.candidate_count);
                    condition_object.boolean("result_truncated", condition.result_truncated);
                    condition_object.boolean("complete", condition.complete);
                    condition_object.array("candidates", |output| {
                        output.push('[');
                        for (candidate_index, candidate) in condition.candidates.iter().enumerate()
                        {
                            if candidate_index != 0 {
                                output.push(',');
                            }
                            let mut candidate_object = JsonObject::begin(output);
                            candidate_object.string("candidate_id", &candidate.candidate_id);
                            candidate_object.string("setup_id", &candidate.setup_id);
                            candidate_object.string("board_mask", &candidate.board_mask);
                            candidate_object.number("min_locks", candidate.min_locks);
                            candidate_object.number("max_locks", candidate.max_locks);
                            candidate_object
                                .number("build_covered_patterns", candidate.build_covered_patterns);
                            candidate_object
                                .number("joint_covered_patterns", candidate.joint_covered_patterns);
                            candidate_object
                                .string("build_probability", &candidate.build_probability);
                            candidate_object
                                .string("joint_probability", &candidate.joint_probability);
                            candidate_object.string(
                                "conditional_pc_probability",
                                &candidate.conditional_pc_probability,
                            );
                            candidate_object.array("representative_path", |output| {
                                output.push('[');
                                for (step_index, step) in
                                    candidate.representative_path.iter().enumerate()
                                {
                                    if step_index != 0 {
                                        output.push(',');
                                    }
                                    let mut step_object = JsonObject::begin(output);
                                    step_object.string("piece", &step.piece);
                                    step_object.number("rotation", step.rotation);
                                    step_object.number("x", step.x);
                                    step_object.number("y", step.y);
                                    step_object.string("hold", &step.hold);
                                    step_object.number("cleared_lines", step.cleared_lines);
                                    step_object.finish();
                                }
                                output.push(']');
                            });
                            if candidate.solution_paths_complete {
                                candidate_object
                                    .number("solution_path_count", candidate.solution_path_count);
                                candidate_object.boolean("solution_paths_complete", true);
                                candidate_object.array("solution_paths", |output| {
                                    output.push('[');
                                    for (path_index, path) in
                                        candidate.solution_paths.iter().enumerate()
                                    {
                                        if path_index != 0 {
                                            output.push(',');
                                        }
                                        output.push('[');
                                        for (step_index, step) in path.iter().enumerate() {
                                            if step_index != 0 {
                                                output.push(',');
                                            }
                                            let mut step_object = JsonObject::begin(output);
                                            step_object.string("piece", &step.piece);
                                            step_object.number("rotation", step.rotation);
                                            step_object.number("x", step.x);
                                            step_object.number("y", step.y);
                                            step_object.string("hold", &step.hold);
                                            step_object.number("cleared_lines", step.cleared_lines);
                                            step_object.finish();
                                        }
                                        output.push(']');
                                    }
                                    output.push(']');
                                });
                            }
                            candidate_object.finish();
                        }
                        output.push(']');
                    });
                    condition_object.finish();
                }
                output.push(']');
            });
        },
    );
    object.optional_object(
        "finesse_report",
        report.finesse_report.as_ref(),
        |nested, finesse| {
            nested.string("mode", &finesse.mode);
            nested.string("metric", &finesse.metric);
            nested.string("pattern_knowledge", &finesse.pattern_knowledge);
            nested.boolean("complete", finesse.complete);
            nested.optional_string("exact_total_inputs", finesse.exact_total_inputs.as_deref());
            nested.optional_object(
                "representative_witness",
                finesse.representative_witness.as_ref(),
                |witness_object, witness| {
                    witness_object.string("policy", &witness.policy);
                    witness_object.optional_string("solution_key", witness.solution_key.as_deref());
                    witness_object.array("pattern_ids", |output| {
                        output.push('[');
                        for (index, pattern_id) in witness.pattern_ids.iter().enumerate() {
                            if index != 0 {
                                output.push(',');
                            }
                            let _ = write!(output, "{pattern_id}");
                        }
                        output.push(']');
                    });
                    witness_object
                        .array("queue", |output| write_string_array(output, &witness.queue));
                    witness_object.number("total_inputs", witness.total_inputs);
                    witness_object.array("input_sequence", |output| {
                        write_string_array(output, &witness.input_sequence)
                    });
                    witness_object.array("placements", |output| {
                        output.push('[');
                        for (index, placement) in witness.placements.iter().enumerate() {
                            if index != 0 {
                                output.push(',');
                            }
                            let mut placement_object = JsonObject::begin(output);
                            placement_object.string("piece", &placement.piece);
                            placement_object.number("rotation", placement.rotation);
                            placement_object.number("x", placement.x);
                            placement_object.number("y", placement.y);
                            placement_object.finish();
                        }
                        output.push(']');
                    });
                },
            );
            nested.array("policy_results", |output| {
                output.push('[');
                for (index, policy) in finesse.policy_results.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    let mut policy_object = JsonObject::begin(output);
                    policy_object.string("policy", &policy.policy);
                    policy_object.string("overall_average_inputs", &policy.overall_average_inputs);
                    policy_object.boolean("complete", policy.complete);
                    policy_object.optional_string(
                        "oracle_on_covered_average_inputs",
                        policy.oracle_on_covered_average_inputs.as_deref(),
                    );
                    policy_object.optional_string(
                        "information_penalty_inputs",
                        policy.information_penalty_inputs.as_deref(),
                    );
                    policy_object.optional_string(
                        "success_probability_gap",
                        policy.success_probability_gap.as_deref(),
                    );
                    policy_object.optional_string(
                        "successful_probability_mass",
                        policy.successful_probability_mass.as_deref(),
                    );
                    policy_object.optional_number(
                        "successful_unique_queue_count",
                        policy.successful_unique_queue_count,
                    );
                    policy_object.optional_number(
                        "total_unique_queue_count",
                        policy.total_unique_queue_count,
                    );
                    policy_object.array("solution_averages", |output| {
                        output.push('[');
                        for (solution_index, solution) in
                            policy.solution_averages.iter().enumerate()
                        {
                            if solution_index != 0 {
                                output.push(',');
                            }
                            let mut solution_object = JsonObject::begin(output);
                            solution_object.string("solution_key", &solution.solution_key);
                            solution_object.string("average_inputs", &solution.average_inputs);
                            solution_object.boolean("complete", solution.complete);
                            solution_object.finish();
                        }
                        output.push(']');
                    });
                    policy_object.finish();
                }
                output.push(']');
            });
        },
    );
    object.optional_object(
        "spin_structure_report",
        report.spin_structure_report.as_ref(),
        |nested, spin| {
            nested.string("initial_board_mask", &spin.initial_board_mask);
            nested.number("height", spin.height);
            nested.string("inventory", &spin.inventory);
            nested.string("spin_profile", &spin.spin_profile);
            nested.string("line_requirement", &spin.line_requirement);
            nested.number("fill_bottom", spin.fill_bottom);
            nested.number("fill_top", spin.fill_top);
            nested.string("rule_profile", &spin.rule_profile);
            nested.string("minimality", &spin.minimality);
            nested.optional_number("minimum_placements", spin.minimum_placements);
            nested.number("workers_used", spin.workers_used);
            nested.boolean("complete", spin.complete);
            for (key, outcomes) in [("regular", &spin.regular), ("mini", &spin.mini)] {
                nested.array(key, |output| {
                    write_object_array(output, outcomes, |row, outcome| {
                        row.string("candidate_id", &outcome.candidate_id);
                        row.string("partition", &outcome.partition);
                        row.number("placement_count", outcome.placement_count);
                        row.string("board_before_spin", &outcome.board_before_spin);
                        row.string("final_board", &outcome.final_board);
                        row.number("cleared_lines", outcome.cleared_lines);
                        row.number(
                            "logical_spin_cleared_rows",
                            outcome.logical_spin_cleared_rows,
                        );
                        row.object("logical_spin", |operation| {
                            write_spin_structure_operation(operation, &outcome.logical_spin)
                        });
                        row.array("logical_operations", |output| {
                            write_object_array(
                                output,
                                &outcome.logical_operations,
                                write_spin_structure_operation,
                            )
                        });
                    })
                });
            }
        },
    );
}

fn write_spin_structure_operation(
    object: &mut JsonObject<'_>,
    operation: &crate::WasmStructureOperation,
) {
    object.string("piece", &operation.piece);
    object.number("rotation", operation.rotation);
    object.number("x", operation.x);
    object.number("y", operation.y);
    object.string("logical_mask", &operation.logical_mask);
    object.number("need_deleted_rows", operation.need_deleted_rows);
}

enum JsonSink {
    Counting {
        len: usize,
        failed: bool,
    },
    Bounded {
        output: String,
        exact_len: usize,
        initial_capacity: usize,
        failed: bool,
    },
}

impl JsonSink {
    fn counting() -> Self {
        Self::Counting {
            len: 0,
            failed: false,
        }
    }

    fn bounded(output: String, exact_len: usize) -> Self {
        let initial_capacity = output.capacity();
        Self::Bounded {
            output,
            exact_len,
            initial_capacity,
            failed: false,
        }
    }

    fn push(&mut self, value: char) {
        let mut encoded = [0_u8; 4];
        self.push_str(value.encode_utf8(&mut encoded));
    }

    fn push_str(&mut self, value: &str) {
        match self {
            Self::Counting { len, failed } => match len.checked_add(value.len()) {
                Some(next) => *len = next,
                None => *failed = true,
            },
            Self::Bounded {
                output,
                exact_len,
                initial_capacity,
                failed,
            } => {
                let Some(next_len) = output.len().checked_add(value.len()) else {
                    *failed = true;
                    return;
                };
                if *failed || next_len > *exact_len || next_len > *initial_capacity {
                    *failed = true;
                    return;
                }
                output.push_str(value);
                if output.capacity() != *initial_capacity {
                    *failed = true;
                }
            }
        }
    }

    fn finish_count(self) -> Result<usize, WasmCommandRuntimeError> {
        match self {
            Self::Counting { len, failed: false } => Ok(len),
            _ => Err(json_projection_error()),
        }
    }

    fn finish_buffer(self) -> Result<String, WasmCommandRuntimeError> {
        match self {
            Self::Bounded {
                output,
                exact_len,
                initial_capacity,
                failed: false,
            } if output.len() == exact_len && output.capacity() == initial_capacity => Ok(output),
            _ => Err(json_projection_error()),
        }
    }
}

impl std::fmt::Write for JsonSink {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push_str(value);
        match self {
            Self::Counting { failed, .. } | Self::Bounded { failed, .. } if *failed => {
                Err(std::fmt::Error)
            }
            _ => Ok(()),
        }
    }
}

fn count_json(write_value: impl FnOnce(&mut JsonSink)) -> Result<usize, WasmCommandRuntimeError> {
    let mut sink = JsonSink::counting();
    write_value(&mut sink);
    sink.finish_count()
}

fn serialize_json(
    mut write_value: impl FnMut(&mut JsonSink),
) -> Result<String, WasmCommandRuntimeError> {
    let mut counter = JsonSink::counting();
    write_value(&mut counter);
    let exact_len = counter.finish_count()?;
    let mut output = String::new();
    output
        .try_reserve_exact(exact_len)
        .map_err(|_| json_allocation_error())?;
    if output.capacity() < exact_len {
        return Err(json_projection_error());
    }
    let mut sink = JsonSink::bounded(output, exact_len);
    write_value(&mut sink);
    sink.finish_buffer()
}

fn json_limit_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_JSON_MEMORY_LIMIT", String::new())
}

fn json_allocation_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_JSON_ALLOCATION", String::new())
}

fn json_projection_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_JSON_MEMORY_PROJECTION", String::new())
}

fn write_object_array<T>(
    output: &mut JsonSink,
    values: &[T],
    write_value: fn(&mut JsonObject<'_>, &T),
) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let mut nested = JsonObject::begin(output);
        write_value(&mut nested, value);
        nested.finish();
    }
    output.push(']');
}

fn write_string_array(output: &mut JsonSink, values: &[String]) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        write_json_string(output, value);
    }
    output.push(']');
}

struct JsonObject<'a> {
    output: &'a mut JsonSink,
    first: bool,
}

impl<'a> JsonObject<'a> {
    fn begin(output: &'a mut JsonSink) -> Self {
        output.push('{');
        Self {
            output,
            first: true,
        }
    }

    fn finish(self) {
        self.output.push('}');
    }

    fn key(&mut self, key: &str) {
        if !self.first {
            self.output.push(',');
        }
        self.first = false;
        write_json_string(self.output, key);
        self.output.push(':');
    }

    fn string(&mut self, key: &str, value: &str) {
        self.key(key);
        write_json_string(self.output, value);
    }

    fn optional_string(&mut self, key: &str, value: Option<&str>) {
        self.key(key);
        if let Some(value) = value {
            write_json_string(self.output, value);
        } else {
            self.output.push_str("null");
        }
    }

    fn optional_u128_string(&mut self, key: &str, value: Option<u128>) {
        self.key(key);
        if let Some(value) = value {
            self.output.push('"');
            let _ = write!(self.output, "{value}");
            self.output.push('"');
        } else {
            self.output.push_str("null");
        }
    }

    fn number(&mut self, key: &str, value: impl std::fmt::Display) {
        self.key(key);
        let _ = write!(self.output, "{value}");
    }

    fn optional_number(&mut self, key: &str, value: Option<impl std::fmt::Display>) {
        self.key(key);
        if let Some(value) = value {
            let _ = write!(self.output, "{value}");
        } else {
            self.output.push_str("null");
        }
    }

    fn boolean(&mut self, key: &str, value: bool) {
        self.key(key);
        self.output.push_str(if value { "true" } else { "false" });
    }

    fn optional_boolean(&mut self, key: &str, value: Option<bool>) {
        self.key(key);
        if let Some(value) = value {
            self.output.push_str(if value { "true" } else { "false" });
        } else {
            self.output.push_str("null");
        }
    }

    fn object(&mut self, key: &str, write_value: impl FnOnce(&mut JsonObject<'_>)) {
        self.key(key);
        let mut nested = JsonObject::begin(self.output);
        write_value(&mut nested);
        nested.finish();
    }

    fn optional_object<T>(
        &mut self,
        key: &str,
        value: Option<&T>,
        write_value: impl FnOnce(&mut JsonObject<'_>, &T),
    ) {
        self.key(key);
        if let Some(value) = value {
            let mut nested = JsonObject::begin(self.output);
            write_value(&mut nested, value);
            nested.finish();
        } else {
            self.output.push_str("null");
        }
    }

    fn array(&mut self, key: &str, write_value: impl FnOnce(&mut JsonSink)) {
        self.key(key);
        write_value(self.output);
    }
}

fn write_json_string(output: &mut JsonSink, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' => {
                let _ = write!(output, "\\u{:04x}", control as u32);
            }
            other => output.push(other),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod exact_json_tests {
    use super::*;
    use crate::{wasm_worker_job::GovernedWasmWorkerEvents, WasmWorkerJobId};
    use clearra_app::{CoveragePortfolioAlternativeSet, PortfolioAlternativeSetIdentity};
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    #[test]
    fn build_coverage_portfolio_payload_is_lossless_json() {
        let build = clearra_host_contract::BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact-union",
            "9",
            "2",
            "5040",
            "5040",
            "1",
            "normalized-set",
            "candidate-1",
            clearra_host_contract::BuildCoverageCompletenessPayload::new(
                true, true, true, true, true,
            ),
            true,
            Some("a".repeat(64)),
        )
        .expect("valid Build payload");
        let payload = ProductResultPayload::new(
            "build.cover",
            "build-coverage-portfolio.v2",
            ProductResultPayloadContent::BuildCoveragePortfolioV2(build.clone()),
        );
        let json = serialize_json(|output| {
            let mut object = JsonObject::begin(output);
            write_product_result_payload(&mut object, &payload);
            object.finish();
        })
        .expect("Build payload JSON");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            value["content"]["payload_kind"],
            "build-coverage-portfolio-v2"
        );
        assert_eq!(
            value["content"]["payload"]["canonical_first_candidate_id"],
            "candidate-1"
        );
        assert_eq!(
            serde_json::from_value::<clearra_host_contract::BuildCoveragePortfolioV2Payload>(
                value["content"]["payload"].clone(),
            )
            .expect("lossless typed payload"),
            build,
        );
    }

    #[test]
    fn every_build_v2_payload_shape_is_lossless_json() {
        use clearra_host_contract::{
            BuildV2CandidateCoveragePayload, BuildV2CompletenessPayload, BuildV2ProductPayload,
            BuildV2ScoreWinnerPayload,
        };

        let replay = BuildV2CompletenessPayload::new(true, true, true, true, true, false, false);
        let portfolio = BuildV2CompletenessPayload::new(true, true, true, true, true, true, false);
        let score = BuildV2CompletenessPayload::new(true, true, true, true, true, true, true);
        let payloads = vec![
            BuildV2ProductPayload::try_candidate_family(
                "build.congruent",
                "build-congruence-family.v1",
                "a".repeat(64),
                "b".repeat(64),
                "unique",
                "2",
                "1",
                "1",
                "1",
                "1",
                None,
                vec![
                    BuildV2CandidateCoveragePayload::try_new("candidate-a", "1").unwrap(),
                    BuildV2CandidateCoveragePayload::try_new("candidate-b", "0").unwrap(),
                ],
                replay,
            )
            .unwrap(),
            BuildV2ProductPayload::try_probability(
                "build.evaluate.cover-percent",
                "build-supplied-probability.v1",
                "a".repeat(64),
                "b".repeat(64),
                Some("supplied-colored-identity-filter-plus-buildability-replay".to_owned()),
                "unique",
                "2",
                "1",
                "1",
                "1",
                "1",
                replay,
            )
            .unwrap(),
            BuildV2ProductPayload::try_portfolio(
                "build.evaluate.minimals",
                "build-supplied-minimum-cover.v1",
                "a".repeat(64),
                Some("supplied-colored-identity-filter-plus-buildability-replay".to_owned()),
                "min-cover",
                "2",
                "1",
                "1",
                "1",
                "1",
                "1",
                vec!["candidate-a".to_owned()],
                portfolio,
                "c".repeat(64),
            )
            .unwrap(),
            BuildV2ProductPayload::try_score_portfolio(
                "build.evaluate.score",
                "build-supplied-score.v1",
                "a".repeat(64),
                "tetrio",
                "0",
                "basic-approximation",
                false,
                "score-only",
                "canonical-equal-score-trace",
                "2",
                "1",
                "1",
                "1",
                "1",
                vec!["candidate-a".to_owned()],
                vec![BuildV2ScoreWinnerPayload::try_new("0", "candidate-a", "1200", "4").unwrap()],
                score,
                "c".repeat(64),
            )
            .unwrap(),
        ];

        for build in payloads {
            let payload = ProductResultPayload::new(
                build.capability_id(),
                build.result_contract(),
                ProductResultPayloadContent::BuildV2(build.clone()),
            );
            let json = serialize_json(|output| {
                let mut object = JsonObject::begin(output);
                write_product_result_payload(&mut object, &payload);
                object.finish();
            })
            .expect("Build v2 payload JSON");
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
            assert_eq!(value["content"]["payload_kind"], "build-v2");
            assert_eq!(
                serde_json::from_value::<BuildV2ProductPayload>(
                    value["content"]["payload"].clone(),
                )
                .expect("lossless typed Build v2 payload"),
                build,
            );
        }
    }

    #[test]
    fn explicitly_attached_solution_set_artifact_is_lossless_json() {
        use clearra_host_contract::{SolutionSetArtifactFormatPayload, SolutionSetArtifactPayload};

        let ctk3 = "ctk3_test";
        let artifact = SolutionSetArtifactPayload::try_new(
            "build-supplied-minimum-cover.v1",
            "normalized-tiling-set",
            "portfolio-alternative",
            "alternative-1",
            Some("a".repeat(64)),
            "normalized-tiling-key-v1",
            "normalized-tiling-set-hash-v1",
            "b".repeat(64),
            1,
            vec![
                SolutionSetArtifactFormatPayload::try_available(
                    "ctk3",
                    "application/vnd.clearra.ctk3",
                    "clearra-solutions.ctk3",
                    ctk3.len() as u64,
                    "c".repeat(64),
                    1,
                    ctk3,
                )
                .expect("valid CTK3 sidecar"),
                SolutionSetArtifactFormatPayload::try_unavailable("fumen", "page-limit-exceeded")
                    .expect("valid unavailable Fumen sidecar"),
            ],
        )
        .expect("valid solution-set sidecar");
        let response = AppResponse::new(None, AppStatus::Success)
            .with_solution_set_artifact(Some(artifact.clone()));
        let json = serialize_json(|output| {
            let mut object = JsonObject::begin(output);
            write_app_response(&mut object, &response);
            object.finish();
        })
        .expect("solution-set sidecar JSON");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            value["solution_set_artifact"]["formats"][0]["document"],
            ctk3
        );
        assert_eq!(
            value["solution_set_artifact"]["formats"][1]["state"],
            "unavailable"
        );
        assert_eq!(
            serde_json::from_value::<SolutionSetArtifactPayload>(
                value["solution_set_artifact"].clone(),
            )
            .expect("lossless typed solution-set sidecar"),
            artifact,
        );
    }

    #[test]
    fn build_setup_family_payload_is_lossless_typed_json_without_portfolio_metadata() {
        let candidates = vec![
            clearra_host_contract::BuildSetupCandidateCoverageV1Payload::try_new(
                "candidate-a",
                "1",
            )
            .expect("valid reachable candidate"),
            clearra_host_contract::BuildSetupCandidateCoverageV1Payload::try_new(
                "candidate-b",
                "0",
            )
            .expect("valid unreachable candidate"),
        ];
        let family = clearra_host_contract::BuildSetupFamilyV1Payload::try_new(
            "build-target-family.v2",
            "a".repeat(64),
            "b".repeat(64),
            "unique",
            "2",
            "1",
            "3",
            "1",
            "0.5",
            clearra_host_contract::BuildSetupCompletenessPayload::new(true, true, true, true, true),
            candidates,
        )
        .expect("valid Build setup family");
        let payload = ProductResultPayload::new(
            "build.setup",
            "build-target-family.v2",
            ProductResultPayloadContent::BuildSetupFamilyV1(family.clone()),
        );

        let json = serialize_json(|output| {
            let mut object = JsonObject::begin(output);
            write_product_result_payload(&mut object, &payload);
            object.finish();
        })
        .expect("Build setup payload JSON");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(value["content"]["payload_kind"], "build-setup-family-v1");
        assert_eq!(
            serde_json::from_value::<clearra_host_contract::BuildSetupFamilyV1Payload>(
                value["content"]["payload"].clone(),
            )
            .expect("lossless typed payload"),
            family,
        );
        let content = value["content"].as_object().expect("content object");
        assert!(!content.contains_key("page_source_owner"));
        let projected = content["payload"].as_object().expect("payload object");
        for forbidden in [
            "tie",
            "ties",
            "alternatives",
            "page",
            "page_source_identity_sha256",
        ] {
            assert!(!projected.contains_key(forbidden));
        }
    }

    fn governed_events_fixture(job_id: WasmWorkerJobId) -> GovernedWasmWorkerEvents {
        GovernedWasmWorkerEvents::from_events_for_test(
            vec![WasmWorkerJobEvent::Started { job_id }],
            u128::MAX,
        )
    }

    #[test]
    fn score_portfolio_pages_project_public_candidate_ids_on_every_outer_page() {
        let row = |patterns: &[u32]| {
            PatternBitSet::from_pattern_indices(3, patterns.to_vec()).expect("coverage row")
        };
        let set = CoveragePortfolioAlternativeSet::new(
            PortfolioAlternativeSetIdentity::new(
                "score-query",
                "score-source",
                "tetrio-score-only",
                "score-pattern-universe",
                "score-build",
            )
            .expect("score portfolio identity"),
            ["a", "b", "c", "d", "e", "f"]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            PatternBitSet::all(3),
            vec![
                row(&[0]),
                row(&[1]),
                row(&[2]),
                row(&[0]),
                row(&[1]),
                row(&[2]),
            ],
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        )
        .and_then(|set| set.with_public_candidate_ids(vec![101, 205, 309, 401, 505, 609]))
        .expect("mapped score portfolio");
        let mut store = CoveragePortfolioPageStore::new(Arc::new(set)).expect("page store");
        let mut alternative_indices = vec!["1".to_owned()];
        loop {
            let advance = store
                .next_page(u64::MAX, &mut || false)
                .expect("advance exact score tie page");
            if let Some(page) = advance.page() {
                alternative_indices.push(page.alternative_index_decimal().to_owned());
            }
            assert!(
                advance.page().is_some() || advance.checkpoint().enumeration_complete(),
                "an unbounded exact advance must load a page or seal enumeration"
            );
            if advance.checkpoint().enumeration_complete() {
                break;
            }
        }

        assert_eq!(alternative_indices.len(), 8, "all 2^3 optimal ties");
        assert_eq!(
            store.loaded_page_count(),
            clearra_app::PORTFOLIO_RETAINED_OUTER_PAGE_LIMIT,
            "runtime cache remains bounded"
        );
        let public_ids = [101_u64, 205, 309, 401, 505, 609];
        for alternative_index in alternative_indices {
            let json = serialize_coverage_portfolio_page_exact(
                &mut store,
                &alternative_index,
                1,
                &mut || false,
            )
            .expect("serialize score portfolio page");
            let value: serde_json::Value = serde_json::from_str(&json).expect("page JSON");
            assert_eq!(value["page"]["alternative_index"], alternative_index);
            let members = value["page"]["members"].as_array().expect("page members");
            assert_eq!(members.len(), 3);
            assert!(members.iter().all(|member| {
                member["candidate_id"]
                    .as_str()
                    .and_then(|candidate_id| candidate_id.parse::<u64>().ok())
                    .is_some_and(|candidate_id| public_ids.contains(&candidate_id))
            }));
        }
    }

    #[test]
    fn counting_pass_matches_escaped_control_and_non_ascii_output_exactly() {
        let events = vec![WasmWorkerJobEvent::PartialResult {
            job_id: WasmWorkerJobId::new(11),
            partial: true,
            label: "quote\" slash\\ newline\n control\u{1} 한글".to_owned(),
            final_result: false,
        }];
        let counted = count_json(|output| write_worker_events(output, &events))
            .expect("counting pass succeeds");
        let serialized = serialize_worker_events(&events).expect("bounded write succeeds");

        assert_eq!(counted, serialized.len());
        assert!(serialized.contains("quote\\\" slash\\\\ newline\\n control\\u0001 한글"));
    }

    #[test]
    fn governed_json_abi_carrier_accepts_exact_peak_and_preserves_peak_minus_one_owner() {
        let job_id = WasmWorkerJobId::new(17);
        let measured_source = governed_events_fixture(job_id);
        let source_payload_heap = measured_source
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmWorkerEvents>() as u128)
            .expect("source payload remains representable");
        let measured_output = serialize_governed_worker_events(measured_source)
            .expect("unbounded fixture serialization");
        let exact_len = measured_output.json().len() as u128;
        let target_payload_heap = measured_output
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmJson>() as u128)
            .expect("target payload remains representable");
        let source_carrier = governed_event_source_carrier_inline_bytes();
        let required_peak = source_payload_heap
            .checked_add(source_carrier)
            .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
            .expect("projection peak")
            .max(
                source_payload_heap
                    .checked_add(source_carrier)
                    .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
                    .expect("counting peak"),
            )
            .max(
                source_payload_heap
                    .checked_add(source_carrier)
                    .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
                    .and_then(|bytes| bytes.checked_add(exact_len))
                    .expect("requested allocation peak"),
            )
            .max(
                source_payload_heap
                    .checked_add(source_carrier)
                    .and_then(|bytes| bytes.checked_add(json_build_carrier_inline_bytes()))
                    .and_then(|bytes| bytes.checked_add(target_payload_heap))
                    .expect("actual allocation peak"),
            )
            .max(
                target_payload_heap
                    .checked_add(governed_json_abi_storage_carrier_inline_bytes())
                    .expect("ABI storage peak"),
            );
        drop(measured_output);

        assert!(
            governed_json_abi_storage_carrier_inline_bytes()
                >= core::mem::size_of::<Option<GovernedWasmJson>>() as u128
        );
        assert!(
            governed_json_abi_storage_carrier_inline_bytes()
                >= governed_json_returned_carrier_inline_bytes()
        );
        let exact = governed_events_fixture(job_id).with_memory_limit_for_test(required_peak);
        let exact_output = serialize_governed_worker_events_for_abi_preserving_owner(exact)
            .expect("the exact governed JSON/ABI storage peak is admitted");
        assert!(exact_output.actual_retained_bytes() <= required_peak);
        drop(exact_output);

        let below = governed_events_fixture(job_id).with_memory_limit_for_test(required_peak - 1);
        let below_pointer = below.events().as_ptr();
        let (error, below) = serialize_governed_worker_events_for_abi_preserving_owner(below)
            .expect_err("one byte below the exact peak is rejected transactionally");
        assert_eq!(error.code(), "E_WASM_JSON_MEMORY_LIMIT");
        assert_eq!(error.message_capacity_for_test(), 0);
        assert_eq!(below.events().as_ptr(), below_pointer);
    }
}
