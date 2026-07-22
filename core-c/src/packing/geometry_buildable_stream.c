#include "packing_candidate_materializer.h"
#include "packing_problem.h"
#include "geometry_catalog_internal.h"

#include "../buildup/buildup_workspace.h"
#include "../buildup/realization_feasibility.h"
#include "clr_build_variant.h"
#include "clr_execution_control.h"
#include "clr_search_profile.h"

#include <limits.h>

typedef struct ClearraBuildableGeometryStreamContext {
    const ClearraGeometryCatalog *catalog;
    const clr_packing_problem *packing_problem;
    clr_buildup_problem *buildup_scratch;
    clr_buildup_workspace *buildup_workspace;
    const ClearraPackingCandidateSink *candidate_sink;
    clr_pruning_proof_ledger *pruning_ledger;
    ClearraBuildableGeometryStreamReport *report;
} ClearraBuildableGeometryStreamContext;

static size_t candidate_row_limit(const clr_packing_problem *problem) {
    return problem->budget.max_results == 0u
        ? SIZE_MAX
        : (size_t)problem->budget.max_results;
}

static size_t total_byte_limit(const clr_packing_problem *problem) {
    if (problem->budget.has_max_memory_mib == 0u) {
        return SIZE_MAX;
    }
    uint64_t bytes = (uint64_t)problem->budget.max_memory_mib *
                     UINT64_C(1024) * UINT64_C(1024);
    return bytes > SIZE_MAX ? SIZE_MAX : (size_t)bytes;
}

static bool buildup_status_is_logical_reject(clr_buildup_status status) {
    return status >= CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE &&
           status <= CLR_BUILDUP_COLLISION;
}

static ClearraPackingStatus packing_status_for_buildup(
    clr_buildup_status status) {
    if (status == CLR_BUILDUP_CANCELLED) {
        return CLEARRA_PACKING_CANCELLED;
    }
    if (status == CLR_BUILDUP_CAPACITY_EXCEEDED ||
        status == CLR_BUILDUP_ENUMERATION_TRUNCATED) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    return CLEARRA_PACKING_INVALID_ARGUMENT;
}

static ClearraPackingStatus consume_buildable_geometry_path(
    void *opaque,
    const ClearraGeometryPathView *path) {
    ClearraBuildableGeometryStreamContext *context =
        (ClearraBuildableGeometryStreamContext *)opaque;
    if (context == 0 || path == 0 || path->skeleton_row_ids == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    context->report->generated_count++;

    ClearraRealizationFeasibilityResult feasibility;
    clr_search_profile_span feasibility_span = clr_search_profile_begin(
        CLR_PROFILE_BUILDUP_REALIZATION_FEASIBILITY);
    ClearraPackingStatus packing_status =
        clearra_realization_feasibility_analyze(
            context->catalog,
            context->packing_problem,
            path->skeleton_row_ids,
            path->operation_count,
            &context->buildup_workspace->realization_feasibility,
            context->pruning_ledger,
            &feasibility);
    (void)clr_search_profile_end(
        feasibility_span, feasibility.explored_state_count);
    if (packing_status != CLEARRA_PACKING_OK) {
        return packing_status;
    }
    if (feasibility.complete != 0u &&
        feasibility.kind == CLEARRA_REALIZATION_FEASIBILITY_INFEASIBLE &&
        feasibility.prune_authorized != 0u) {
        clr_search_profile_count(
            CLR_PROFILE_BUILDUP_REALIZATION_INFEASIBLE, 1u);
        return CLEARRA_PACKING_OK;
    }
    clr_search_profile_count(
        feasibility.complete != 0u &&
                feasibility.kind == CLEARRA_REALIZATION_FEASIBILITY_FEASIBLE
            ? CLR_PROFILE_BUILDUP_REALIZATION_FEASIBLE
            : CLR_PROFILE_BUILDUP_REALIZATION_UNKNOWN,
        1u);

    context->buildup_scratch->geometry_catalog = context->catalog;
    const uint8_t *representative_order_hint =
        feasibility.kind == CLEARRA_REALIZATION_FEASIBILITY_FEASIBLE &&
                feasibility.operation_count == path->operation_count
            ? feasibility.operation_order
            : 0;
    const uint16_t *required_predecessors =
        feasibility.complete != 0u &&
                feasibility.kind == CLEARRA_REALIZATION_FEASIBILITY_FEASIBLE &&
                feasibility.operation_count == path->operation_count
            ? feasibility.required_predecessors
            : 0;
    clr_buildup_status buildup_status =
        clearra_buildup_exists_catalog_rows_with_constraints_and_workspace(
            context->buildup_scratch,
            context->catalog,
            path->skeleton_row_ids,
            path->operation_count,
            representative_order_hint,
            required_predecessors,
            context->buildup_workspace);
    context->report->buildup_status = (int32_t)buildup_status;
    context->report->workspace_retained_bytes =
        clr_buildup_workspace_retained_bytes(context->buildup_workspace);
    if (buildup_status_is_logical_reject(buildup_status)) {
        return CLEARRA_PACKING_OK;
    }
    if (buildup_status != CLR_BUILDUP_OK) {
        return packing_status_for_buildup(buildup_status);
    }
    context->report->candidate_buildable = 1u;

    ClearraPackingCandidateView candidate;
    packing_status = clearra_packing_materialize_catalog_row_ids(
        context->catalog,
        context->packing_problem,
        path->skeleton_row_ids,
        path->operation_count,
        &candidate);
    if (packing_status != CLEARRA_PACKING_OK) {
        return packing_status;
    }
    /* Candidate identity belongs to the exact host reducer. The producer
       exports hashes only as lookup metadata and leaves both authorities
       unset until exact canonical tuple comparison has completed. */
    candidate.candidate_id = 0u;
    candidate.canonical_operation_set_id = 0u;

    uint8_t inserted = 0u;
    uint16_t truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    packing_status = context->candidate_sink->consume(
        context->candidate_sink->context,
        &candidate,
        (size_t)context->report->buildable_count,
        clearra_geometry_catalog_resident_bytes(context->catalog) +
            context->report->workspace_retained_bytes,
        candidate_row_limit(context->packing_problem),
        total_byte_limit(context->packing_problem),
        &inserted,
        &truncation_reason,
        &context->report->host_resident_bytes);
    context->report->truncation_reason = truncation_reason;
    if (packing_status == CLEARRA_PACKING_OK && inserted != 0u) {
        context->report->buildable_count++;
    }
    return packing_status;
}

ClearraPackingStatus clearra_geometry_catalog_rows_buildable_to_sink(
    const ClearraGeometryCatalog *catalog,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    const clr_packing_problem *packing_problem,
    clr_buildup_problem *buildup_scratch,
    clr_buildup_workspace *buildup_workspace,
    const ClearraPackingCandidateSink *sink,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraBuildableGeometryStreamReport *out_report) {
    if (catalog == 0 || skeleton_row_ids == 0 || operation_count == 0u ||
        operation_count > CLEARRA_PACKING_MAX_PIECES ||
        packing_problem == 0 || buildup_scratch == 0 ||
        buildup_workspace == 0 || sink == 0 || sink->consume == 0 ||
        out_pruning_ledger == 0 || out_report == 0 ||
        !clearra_geometry_catalog_matches_problem(catalog, packing_problem)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (clr_pruning_proof_ledger_init_with_policy(
            out_pruning_ledger, evidence_policy) != CLR_PRUNING_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_report = (ClearraBuildableGeometryStreamReport){0};
    ClearraBuildableGeometryStreamContext context = {
        .catalog = catalog,
        .packing_problem = packing_problem,
        .buildup_scratch = buildup_scratch,
        .buildup_workspace = buildup_workspace,
        .candidate_sink = sink,
        .pruning_ledger = out_pruning_ledger,
        .report = out_report,
    };
    ClearraGeometryPathView path = {
        .skeleton_row_ids = skeleton_row_ids,
        .operation_count = operation_count,
    };
    ClearraPackingStatus status = consume_buildable_geometry_path(
        &context, &path);
    if (status == CLEARRA_PACKING_CANCELLED ||
        clr_execution_cancel_requested()) {
        out_report->complete = 0u;
        return CLEARRA_PACKING_CANCELLED;
    }
    out_report->complete = (uint8_t)(status == CLEARRA_PACKING_OK);
    return status;
}

ClearraPackingStatus clearra_geometry_solution_graph_stream_buildable_task(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometryCatalog *catalog,
    const ClearraGeometrySolutionTask *task,
    const clr_packing_problem *packing_problem,
    clr_buildup_problem *buildup_scratch,
    clr_buildup_workspace *buildup_workspace,
    const ClearraPackingCandidateSink *sink,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraBuildableGeometryStreamReport *out_report) {
    if (graph == 0 || catalog == 0 || task == 0 || packing_problem == 0 ||
        buildup_scratch == 0 || buildup_workspace == 0 || sink == 0 ||
        sink->consume == 0 || out_pruning_ledger == 0 || out_report == 0 ||
        !clearra_geometry_solution_graph_matches_catalog(
            graph, clearra_geometry_catalog_identity(catalog))) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (clr_pruning_proof_ledger_init_with_policy(
            out_pruning_ledger, evidence_policy) != CLR_PRUNING_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_report = (ClearraBuildableGeometryStreamReport){0};
    ClearraBuildableGeometryStreamContext context = {
        .catalog = catalog,
        .packing_problem = packing_problem,
        .buildup_scratch = buildup_scratch,
        .buildup_workspace = buildup_workspace,
        .candidate_sink = sink,
        .pruning_ledger = out_pruning_ledger,
        .report = out_report,
    };
    ClearraGeometryPathSink path_sink = {
        .context = &context,
        .consume = consume_buildable_geometry_path,
    };
    uint64_t emitted_count = 0u;
    ClearraPackingStatus status =
        clearra_geometry_solution_graph_stream_task_paths(
            graph, task, &path_sink, &emitted_count);
    if (status == CLEARRA_PACKING_CANCELLED || clr_execution_cancel_requested()) {
        out_report->complete = 0u;
        return CLEARRA_PACKING_CANCELLED;
    }
    out_report->complete = (uint8_t)(status == CLEARRA_PACKING_OK);
    return status;
}
