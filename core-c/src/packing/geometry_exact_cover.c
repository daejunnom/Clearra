#include "geometry_exact_cover_internal.h"
#include "geometry_exact_cover_proof.h"
#include "geometry_solution_graph_internal.h"

#include "../cache/cache_identity.h"
#include "clr_execution_control.h"

#include <limits.h>
#include <stdlib.h>

static size_t saturated_add_size(size_t left, size_t right) {
    return left > SIZE_MAX - right ? SIZE_MAX : left + right;
}

size_t clearra_geometry_search_resident_bytes(
    const ClearraGeometryExactCoverSearch *search) {
    return saturated_add_size(
        saturated_add_size(
            saturated_add_size(
                saturated_add_size(
                    search->catalog->resident_bytes, sizeof(*search)),
            search->residual_memo.resident_bytes),
            saturated_add_size(
                search->solution_family.resident_bytes,
                search->projection_cache.resident_bytes)),
        saturated_add_size(
            search->output.host_resident_bytes,
            search->component_workspace_bytes));
}

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

_Static_assert(
    CLEARRA_PACKING_MAX_PIECES <= 15u,
    "four-bit residual memo counts require the exact-cover depth limit");

static uint32_t pack_piece_counts(
    const uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    uint32_t packed = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        packed |= (uint32_t)counts[piece]
                  << ((uint32_t)(piece - CLR_PIECE_I) * 4u);
    }
    return packed;
}

bool clearra_geometry_row_is_feasible(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint32_t row_id,
    uint64_t remaining_cells,
    ClearraActivePieceFamily *out_next_family) {
    if (row_id >= search->catalog->skeleton_count) {
        return false;
    }
    uint64_t row_cells = search->catalog->skeleton_cell_mask[row_id];
    uint8_t piece = (uint8_t)search->catalog->skeleton_piece_kind[row_id];
    if (row_cells == 0u || (row_cells & remaining_cells) != row_cells ||
        piece < CLR_PIECE_I || piece > CLR_PIECE_L ||
        search->used_piece_counts[piece] >=
            search->problem->piece_multiset_window.counts[piece]) {
        return false;
    }
    return clearra_geometry_piece_family_advance(
        &search->piece_family_domain,
        active_family,
        piece,
        (uint8_t)(search->used_piece_counts[piece] + 1u),
        out_next_family);
}

static bool partition_owns_prefix(
    const ClearraGeometryExactCoverSearch *search,
    uint8_t next_depth,
    uint64_t prefix_hash) {
    return search->partition_count == 1u ||
           next_depth != search->partition_depth ||
           prefix_hash % search->partition_count == search->partition_index;
}

ClearraPackingStatus clearra_geometry_search_exact_cover(
    ClearraGeometryExactCoverSearch *search,
    uint64_t remaining_cells,
    uint8_t depth,
    uint64_t prefix_hash,
    const ClearraActivePieceFamily *active_family,
    ClearraGeometryFamilyRef *out_family) {
    *out_family = CLEARRA_GEOMETRY_FAMILY_INVALID;
    if (clr_execution_control_poll(&search->cancellation_poll_counter)) {
        clr_resource_report_mark_truncated(
            search->resource_report, CLR_RESOURCE_TRUNCATION_CANCELLED);
        return CLEARRA_PACKING_CANCELLED;
    }
    uint32_t packed_counts = pack_piece_counts(search->used_piece_counts);
    bool memo_authorized = search->partition_count == 1u ||
                           depth >= search->partition_depth;
    if (memo_authorized) {
        uint32_t memo_family = CLEARRA_GEOMETRY_FAMILY_INVALID;
        if (clearra_geometry_residual_memo_lookup(
                &search->residual_memo,
                remaining_cells,
                packed_counts,
                &memo_family)) {
            *out_family = memo_family;
            return CLEARRA_PACKING_OK;
        }
    }
    if (search->problem->budget.max_nodes != 0u &&
        search->expanded_nodes >= search->problem->budget.max_nodes) {
        clr_resource_report_mark_truncated(
            search->resource_report,
            CLR_RESOURCE_TRUNCATION_FRONTIER_BUDGET_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    search->expanded_nodes++;
    clr_resource_report_observe_frontier_states(
        search->resource_report, (size_t)depth + 1u);

    if (remaining_cells == 0u) {
        if (depth == search->target_depth &&
            clearra_geometry_piece_family_exact_match(
                &search->piece_family_domain,
                active_family,
                search->used_piece_counts)) {
            *out_family = CLEARRA_GEOMETRY_FAMILY_EMPTY;
        }
        if (memo_authorized) {
            clearra_geometry_residual_memo_insert(
                &search->residual_memo,
                remaining_cells,
                packed_counts,
                *out_family);
        }
        return CLEARRA_PACKING_OK;
    }
    if (depth >= search->target_depth ||
        popcount64(remaining_cells) !=
            (uint8_t)((search->target_depth - depth) * CLEARRA_TETROMINO_AREA)) {
        if (memo_authorized) {
            clearra_geometry_residual_memo_insert(
                &search->residual_memo,
                remaining_cells,
                packed_counts,
                CLEARRA_GEOMETRY_FAMILY_INVALID);
        }
        return CLEARRA_PACKING_OK;
    }

    bool component_composition_applied = false;
    ClearraPackingStatus component_status = clearra_geometry_try_component_composition(
        search,
        remaining_cells,
        depth,
        prefix_hash,
        active_family,
        &component_composition_applied,
        out_family);
    if (component_status != CLEARRA_PACKING_OK) {
        return component_status;
    }
    if (component_composition_applied) {
        if (memo_authorized) {
            clearra_geometry_residual_memo_insert(
                &search->residual_memo,
                remaining_cells,
                packed_counts,
                *out_family);
        }
        return CLEARRA_PACKING_OK;
    }

    ClearraGeometryDomainPropagation propagation;
    ClearraGeometryDomainStatus domain_status =
        clearra_geometry_full_placement_domain_propagate(
            search, active_family, remaining_cells, &propagation);
    if (domain_status == CLEARRA_GEOMETRY_DOMAIN_INVALID) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (domain_status == CLEARRA_GEOMETRY_DOMAIN_EMPTY) {
        bool prune_authorized = false;
        ClearraPackingStatus proof_status =
            clearra_geometry_authorize_full_placement_domain(
                search,
                depth,
                domain_status,
                &propagation,
                &prune_authorized);
        if (proof_status != CLEARRA_PACKING_OK) {
            return proof_status;
        }
        if (prune_authorized) {
            if (memo_authorized) {
                clearra_geometry_residual_memo_insert(
                    &search->residual_memo,
                    remaining_cells,
                    packed_counts,
                    CLEARRA_GEOMETRY_FAMILY_INVALID);
            }
            return CLEARRA_PACKING_OK;
        }
        /*
         * Complete-evidence mode may decline the proof when its retention
         * budget is exhausted. Evidence capacity is not search capacity:
         * keep the state and fall back to the ordinary exact-cover pivot.
         * A truly unsupported cell then exhausts its empty support range;
         * a rejected multi-cell SameTile certificate is simply ignored.
         */
        propagation.pivot_required_cells =
            UINT64_C(1) << propagation.pivot_cell;
    } else if (propagation.pivot_filtered_row_count != 0u) {
        bool prune_authorized = false;
        ClearraPackingStatus proof_status =
            clearra_geometry_authorize_full_placement_domain(
                search,
                depth,
                domain_status,
                &propagation,
                &prune_authorized);
        if (proof_status != CLEARRA_PACKING_OK) {
            return proof_status;
        }
        if (!prune_authorized) {
            propagation.pivot_required_cells =
                UINT64_C(1) << propagation.pivot_cell;
        }
    }
    bool bumper_constraint_active = false;
    uint8_t active_bumper_cell = UINT8_MAX;
    ClearraGeometryBumperResult bumper_result;
    ClearraGeometryBumperStatus bumper_status =
        clearra_geometry_bumper_domain_propagate(
            search,
            active_family,
            remaining_cells,
            &bumper_result);
    if (bumper_status == CLEARRA_GEOMETRY_BUMPER_INVALID) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    bool bumper_improves_pivot =
        bumper_status == CLEARRA_GEOMETRY_BUMPER_SUPPORTED &&
        bumper_result.filtered_parent_row_count != 0u &&
        (bumper_result.exact_parent_row_count <
             propagation.pivot_support_count ||
         bumper_result.exact_parent_row_count <= 5u);
    if (bumper_status == CLEARRA_GEOMETRY_BUMPER_EMPTY ||
        bumper_improves_pivot) {
        bool prune_authorized = false;
        ClearraPackingStatus proof_status =
            clearra_geometry_authorize_bumper_domain(
                search,
                depth,
                bumper_status,
                &bumper_result,
                &prune_authorized);
        if (proof_status != CLEARRA_PACKING_OK) {
            return proof_status;
        }
        if (prune_authorized) {
            if (bumper_status == CLEARRA_GEOMETRY_BUMPER_EMPTY) {
                if (memo_authorized) {
                    clearra_geometry_residual_memo_insert(
                        &search->residual_memo,
                        remaining_cells,
                        packed_counts,
                        CLEARRA_GEOMETRY_FAMILY_INVALID);
                }
                return CLEARRA_PACKING_OK;
            }
            propagation.pivot_cell = bumper_result.bumper_cell;
            propagation.pivot_required_cells =
                UINT64_C(1) << bumper_result.bumper_cell;
            propagation.pivot_support_count =
                bumper_result.exact_parent_row_count;
            propagation.pivot_piece_mask = bumper_result.parent_piece_mask;
            active_bumper_cell = bumper_result.bumper_cell;
            bumper_constraint_active = true;
        }
    }

    bool apdp_parent_constraint_active = false;
    if (domain_status == CLEARRA_GEOMETRY_DOMAIN_SUPPORTED &&
        popcount64(propagation.pivot_required_cells) == 3u) {
        ClearraGeometryApdpResult apdp_result;
        ClearraGeometryApdpStatus apdp_status =
            clearra_geometry_apdp_propagate(
                search,
                active_family,
                remaining_cells,
                propagation.pivot_required_cells,
                &apdp_result);
        if (apdp_status == CLEARRA_GEOMETRY_APDP_INVALID) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        if (apdp_status == CLEARRA_GEOMETRY_APDP_EMPTY ||
            (apdp_status == CLEARRA_GEOMETRY_APDP_SUPPORTED &&
             apdp_result.exact_parent_row_count <
                 propagation.pivot_support_count)) {
            bool prune_authorized = false;
            ClearraPackingStatus proof_status =
                clearra_geometry_authorize_apdp_domain(
                    search,
                    depth,
                    apdp_status,
                    &apdp_result,
                    &prune_authorized);
            if (proof_status != CLEARRA_PACKING_OK) {
                return proof_status;
            }
            if (prune_authorized) {
                if (apdp_status == CLEARRA_GEOMETRY_APDP_EMPTY) {
                    if (memo_authorized) {
                        clearra_geometry_residual_memo_insert(
                            &search->residual_memo,
                            remaining_cells,
                            packed_counts,
                            CLEARRA_GEOMETRY_FAMILY_INVALID);
                    }
                    return CLEARRA_PACKING_OK;
                }
                propagation.pivot_support_count =
                    apdp_result.exact_parent_row_count;
                propagation.pivot_piece_mask = apdp_result.parent_piece_mask;
                apdp_parent_constraint_active = true;
            }
        }
    }
    uint8_t remaining_piece_count = (uint8_t)(search->target_depth - depth);
    if (depth <= 2u || propagation.pivot_support_count >= 5u) {
        ClearraGeometryHallResult hall_result;
        ClearraGeometryHallStatus hall_status =
            clearra_geometry_parent_hall_bound_propagate(
                search,
                active_family,
                &propagation,
                remaining_cells,
                &hall_result);
        if (hall_status == CLEARRA_GEOMETRY_HALL_INVALID) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        if (hall_status == CLEARRA_GEOMETRY_HALL_IMPOSSIBLE) {
            bool prune_authorized = false;
            ClearraPackingStatus proof_status =
                clearra_geometry_authorize_hall_bound(
                    search,
                    depth,
                    hall_status,
                    &hall_result,
                    &prune_authorized);
            if (proof_status != CLEARRA_PACKING_OK) {
                return proof_status;
            }
            if (prune_authorized) {
                if (memo_authorized) {
                    clearra_geometry_residual_memo_insert(
                        &search->residual_memo,
                        remaining_cells,
                        packed_counts,
                        CLEARRA_GEOMETRY_FAMILY_INVALID);
                }
                return CLEARRA_PACKING_OK;
            }
        }
    }
    ClearraGeometryColumnProjectionResult projection_result;
    ClearraGeometryColumnProjectionStatus projection_status =
        clearra_geometry_column_projection_propagate(
            search,
            active_family,
            remaining_cells,
            remaining_piece_count,
            &projection_result);
    if (projection_status == CLEARRA_GEOMETRY_COLUMN_PROJECTION_INVALID) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (projection_status == CLEARRA_GEOMETRY_COLUMN_PROJECTION_IMPOSSIBLE) {
        bool prune_authorized = false;
        ClearraPackingStatus proof_status =
            clearra_geometry_authorize_column_projection(
                search,
                depth,
                projection_status,
                &projection_result,
                &prune_authorized);
        if (proof_status != CLEARRA_PACKING_OK) {
            return proof_status;
        }
        if (prune_authorized) {
            if (memo_authorized) {
                clearra_geometry_residual_memo_insert(
                    &search->residual_memo,
                    remaining_cells,
                    packed_counts,
                    CLEARRA_GEOMETRY_FAMILY_INVALID);
            }
            return CLEARRA_PACKING_OK;
        }
    }
    if (remaining_piece_count >= 4u &&
        (depth <= 1u || propagation.pivot_support_count >= 8u) &&
        search->catalog->piece_column_projections != 0) {
        ClearraGeometryProjectionReachabilityResult reachability_result;
        ClearraGeometryProjectionReachabilityStatus reachability_status =
            clearra_geometry_projection_reachability_propagate(
                search,
                active_family,
                remaining_cells,
                remaining_piece_count,
                &reachability_result);
        if (reachability_status == CLEARRA_GEOMETRY_PROJECTION_INVALID) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        if (reachability_status == CLEARRA_GEOMETRY_PROJECTION_UNREACHABLE) {
            bool prune_authorized = false;
            ClearraPackingStatus proof_status =
                clearra_geometry_authorize_projection_reachability(
                    search,
                    depth,
                    reachability_status,
                    &reachability_result,
                    &prune_authorized);
            if (proof_status != CLEARRA_PACKING_OK) {
                return proof_status;
            }
            if (prune_authorized) {
                if (memo_authorized) {
                    clearra_geometry_residual_memo_insert(
                        &search->residual_memo,
                        remaining_cells,
                        packed_counts,
                        CLEARRA_GEOMETRY_FAMILY_INVALID);
                }
                return CLEARRA_PACKING_OK;
            }
        }
    }
    if (remaining_piece_count >= 4u &&
        (depth <= 2u || propagation.pivot_support_count >= 5u)) {
        ClearraGeometryInvariantResult invariant_result;
        ClearraGeometryInvariantStatus invariant_status =
            clearra_geometry_additive_invariant_propagate(
                search,
                active_family,
                remaining_cells,
                remaining_piece_count,
                &invariant_result);
        if (invariant_status == CLEARRA_GEOMETRY_INVARIANT_INVALID) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        if (invariant_status == CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE) {
            bool prune_authorized = false;
            ClearraPackingStatus proof_status =
                clearra_geometry_authorize_additive_invariant(
                    search,
                    depth,
                    invariant_status,
                    &invariant_result,
                    &prune_authorized);
            if (proof_status != CLEARRA_PACKING_OK) {
                return proof_status;
            }
            if (prune_authorized) {
                if (memo_authorized) {
                    clearra_geometry_residual_memo_insert(
                        &search->residual_memo,
                        remaining_cells,
                        packed_counts,
                        CLEARRA_GEOMETRY_FAMILY_INVALID);
                }
                return CLEARRA_PACKING_OK;
            }
        }
    }
    uint8_t pivot = propagation.pivot_cell;
    ClearraGeometryFamilyRef union_levels[32] = {0};
    uint32_t begin = search->catalog->cell_support_offsets[pivot];
    uint32_t end = search->catalog->cell_support_offsets[pivot + 1u];
    for (uint32_t cursor = begin; cursor < end; ++cursor) {
        uint32_t row_id = search->catalog->cell_support_row_ids[cursor];
        uint64_t row_cells = search->catalog->skeleton_cell_mask[row_id];
        ClearraActivePieceFamily next_family;
        if ((row_cells & propagation.pivot_required_cells) !=
                propagation.pivot_required_cells ||
            (bumper_constraint_active &&
             !clearra_geometry_bumper_row_is_compatible(
                 search->catalog,
                 remaining_cells,
                 active_bumper_cell,
                 row_id)) ||
            (apdp_parent_constraint_active &&
             !clearra_geometry_apdp_row_supports_required_cells(
                 search->catalog,
                 row_id,
                 propagation.pivot_required_cells)) ||
            !clearra_geometry_row_is_feasible(
                search,
                active_family,
                row_id,
                remaining_cells,
                &next_family)) {
            continue;
        }
        uint8_t piece = (uint8_t)search->catalog->skeleton_piece_kind[row_id];
        uint8_t next_depth = (uint8_t)(depth + 1u);
        uint64_t next_hash = clearra_cache_key_mix_u64(
            prefix_hash, (uint64_t)row_id + UINT64_C(1));
        if (!partition_owns_prefix(search, next_depth, next_hash)) {
            continue;
        }
        search->used_piece_counts[piece]++;
        ClearraGeometryFamilyRef child_family =
            CLEARRA_GEOMETRY_FAMILY_INVALID;
        ClearraPackingStatus status = clearra_geometry_search_exact_cover(
            search,
            remaining_cells & ~row_cells,
            next_depth,
            next_hash,
            &next_family,
            &child_family);
        search->used_piece_counts[piece]--;
        if (status != CLEARRA_PACKING_OK) {
            return status;
        }
        if (child_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            continue;
        }
        ClearraGeometryFamilyRef branch =
            clearra_geometry_solution_family_append(
                &search->solution_family, row_id, child_family);
        if (branch == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        uint8_t level = 0u;
        while (level < 32u &&
               union_levels[level] != CLEARRA_GEOMETRY_FAMILY_INVALID) {
            branch = clearra_geometry_solution_family_union(
                &search->solution_family, union_levels[level], branch);
            if (branch == CLEARRA_GEOMETRY_FAMILY_INVALID) {
                clr_resource_report_mark_truncated(
                    search->resource_report,
                    CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
                return CLEARRA_PACKING_CAPACITY_EXCEEDED;
            }
            union_levels[level] = CLEARRA_GEOMETRY_FAMILY_INVALID;
            level++;
        }
        if (level == 32u) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        union_levels[level] = branch;
    }
    for (uint8_t level = 32u; level != 0u; --level) {
        ClearraGeometryFamilyRef branch = union_levels[level - 1u];
        if (branch == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            continue;
        }
        if (*out_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            *out_family = branch;
        } else {
            *out_family = clearra_geometry_solution_family_union(
                &search->solution_family, *out_family, branch);
            if (*out_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
                clr_resource_report_mark_truncated(
                    search->resource_report,
                    CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
                return CLEARRA_PACKING_CAPACITY_EXCEEDED;
            }
        }
    }
    if (memo_authorized) {
        clearra_geometry_residual_memo_insert(
            &search->residual_memo,
            remaining_cells,
            packed_counts,
            *out_family);
    }
    return CLEARRA_PACKING_OK;
}

static size_t max_total_bytes(const clr_packing_problem *problem) {
    if (problem->budget.has_max_memory_mib == 0u) {
        return SIZE_MAX;
    }
    uint64_t bytes = (uint64_t)problem->budget.max_memory_mib *
                     UINT64_C(1024) * UINT64_C(1024);
    return bytes > SIZE_MAX ? SIZE_MAX : (size_t)bytes;
}

static ClearraPackingStatus publish_empty_graph(
    const ClearraGeometryCatalog *catalog,
    uint8_t target_depth,
    size_t byte_limit,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report) {
    if (out_graph == 0) {
        return CLEARRA_PACKING_OK;
    }
    if (sizeof(ClearraGeometrySolutionGraph) > byte_limit) {
        clr_resource_report_mark_truncated(
            out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    ClearraGeometrySolutionGraph *graph =
        (ClearraGeometrySolutionGraph *)malloc(sizeof(*graph));
    if (graph == 0) {
        clr_resource_report_mark_truncated(
            out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    *graph = (ClearraGeometrySolutionGraph){
        .catalog_identity = catalog->identity,
        .root = CLEARRA_GEOMETRY_FAMILY_INVALID,
        .resident_bytes = sizeof(*graph),
        .skeleton_count = catalog->skeleton_count,
        .target_depth = target_depth,
        .complete = 1u,
    };
    *out_graph = graph;
    clr_resource_report_observe_cpu_bytes(
        out_resource_report, graph->resident_bytes);
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus run_search(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t family_begin,
    uint16_t family_end,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    ClearraGeometryExactCoverOutput output,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *pruning_ledger) {
    bool graph_output = out_graph != 0;
    uint8_t output_count = (uint8_t)(output.buffer != 0) +
                           (uint8_t)(output.sink != 0) +
                           (uint8_t)graph_output;
    if (!clearra_geometry_catalog_matches_problem(catalog, problem) ||
        out_resource_report == 0 || pruning_ledger == 0 ||
        pruning_ledger->capacity != CLR_PRUNING_LEDGER_MAX_ENTRIES ||
        pruning_ledger->minimal_record_capacity !=
            CLR_PRUNING_MINIMAL_RECORD_MAX_ENTRIES ||
        partition_count == 0u ||
        partition_index >= partition_count ||
        output_count != 1u ||
        (output.sink != 0 && output.sink->consume == 0)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (graph_output) {
        *out_graph = 0;
    }
    clr_resource_report_clear(out_resource_report);
    if (output.buffer != 0) {
        clearra_packing_candidate_buffer_clear(output.buffer);
    }
    uint16_t family_count = problem->piece_multiset_family.count;
    if ((family_count == 0u && (family_begin != 0u || family_end != 0u)) ||
        (family_count != 0u &&
         (family_begin >= family_end || family_end > family_count))) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    uint8_t required_cells = popcount64(catalog->required_fill_mask);
    if (required_cells == 0u ||
        required_cells % CLEARRA_TETROMINO_AREA != 0u) {
        return publish_empty_graph(
            catalog,
            0u,
            max_total_bytes(problem),
            out_graph,
            out_resource_report);
    }
    uint8_t target_depth =
        (uint8_t)(required_cells / CLEARRA_TETROMINO_AREA);
    if (target_depth == 0u || target_depth > CLEARRA_PACKING_MAX_PIECES ||
        partition_depth == 0u || partition_depth > target_depth) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (problem->piece_window.has_exact_pieces != 0u &&
        problem->piece_window.exact_pieces != target_depth) {
        return publish_empty_graph(
            catalog,
            target_depth,
            max_total_bytes(problem),
            out_graph,
            out_resource_report);
    }
    if (target_depth > problem->piece_window.max_pieces ||
        target_depth > problem->piece_multiset_window.total_count) {
        return publish_empty_graph(
            catalog,
            target_depth,
            max_total_bytes(problem),
            out_graph,
            out_resource_report);
    }

    if (output.buffer != 0) {
        output.host_resident_bytes = sizeof(*output.buffer);
        output.max_candidate_rows = problem->budget.max_results == 0u
            ? CLEARRA_PACKING_MAX_CANDIDATES
            : problem->budget.max_results;
        if (output.max_candidate_rows > CLEARRA_PACKING_MAX_CANDIDATES) {
            output.max_candidate_rows = CLEARRA_PACKING_MAX_CANDIDATES;
        }
    } else {
        output.host_resident_bytes = 0u;
        output.max_candidate_rows = problem->budget.max_results == 0u
            ? SIZE_MAX
            : problem->budget.max_results;
    }
    output.max_total_bytes = max_total_bytes(problem);
    size_t engine_bytes = saturated_add_size(
        saturated_add_size(
            catalog->resident_bytes,
            sizeof(ClearraGeometryExactCoverSearch)),
        output.host_resident_bytes);
    if (graph_output) {
        engine_bytes = saturated_add_size(
            engine_bytes, sizeof(ClearraGeometrySolutionGraph));
    }
    if (engine_bytes > output.max_total_bytes) {
        clr_resource_report_mark_truncated(
            out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    ClearraGeometryExactCoverSearch search = {
        .catalog = catalog,
        .problem = problem,
        .resource_report = out_resource_report,
        .output = output,
        .target_depth = target_depth,
        .family_begin = family_begin,
        .family_end = family_end,
        .partition_index = partition_index,
        .partition_count = partition_count,
        .partition_depth = partition_depth,
        .pruning_ledger = pruning_ledger,
        .pruning_batch_id = clearra_geometry_search_batch_id(
            catalog,
            problem,
            family_begin,
            family_end,
            partition_index,
            partition_count),
        .pruning_catalog_identity_digest =
            clearra_geometry_catalog_identity_digest(&catalog->identity),
    };
    if (!clearra_geometry_piece_family_domain_compile(
            &problem->piece_multiset_family,
            family_begin,
            family_end,
            &search.piece_family_domain)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    size_t memo_max_bytes = SIZE_MAX;
    size_t family_max_bytes = SIZE_MAX;
    size_t projection_max_bytes = SIZE_MAX;
    if (output.max_total_bytes != SIZE_MAX) {
        size_t remaining_budget = output.max_total_bytes - engine_bytes;
        projection_max_bytes = remaining_budget / 8u;
        memo_max_bytes = (remaining_budget - projection_max_bytes) / 4u;
        family_max_bytes = remaining_budget - projection_max_bytes -
                           memo_max_bytes;
    }
    clearra_geometry_residual_memo_init(
        &search.residual_memo,
        catalog->skeleton_count,
        memo_max_bytes);
    clearra_geometry_solution_family_init(
        &search.solution_family, family_max_bytes);
    clearra_geometry_projection_cache_init(
        &search.projection_cache, projection_max_bytes);
    clr_resource_report_observe_cpu_bytes(
        out_resource_report,
        clearra_geometry_search_resident_bytes(&search));
    ClearraGeometryFamilyRef root_family =
        CLEARRA_GEOMETRY_FAMILY_INVALID;
    ClearraPackingStatus status = clearra_geometry_search_exact_cover(
        &search,
        catalog->required_fill_mask,
        0u,
        UINT64_C(1469598103934665603),
        &search.piece_family_domain.initial,
        &root_family);
    if (status == CLEARRA_PACKING_OK && graph_output) {
        ClearraGeometrySolutionGraph *graph =
            (ClearraGeometrySolutionGraph *)malloc(sizeof(*graph));
        if (graph == 0) {
            clr_resource_report_mark_truncated(
                out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            status = CLEARRA_PACKING_CAPACITY_EXCEEDED;
        } else {
            *graph = (ClearraGeometrySolutionGraph){
                .family = search.solution_family,
                .catalog_identity = catalog->identity,
                .root = root_family,
                .resident_bytes = saturated_add_size(
                    sizeof(*graph), search.solution_family.resident_bytes),
                .skeleton_count = catalog->skeleton_count,
                .target_depth = target_depth,
                .complete = 1u,
            };
            search.solution_family = (ClearraGeometrySolutionFamily){0};
            *out_graph = graph;
            clr_resource_report_observe_cpu_bytes(
                out_resource_report,
                saturated_add_size(
                    clearra_geometry_search_resident_bytes(&search),
                    graph->resident_bytes));
        }
    } else if (status == CLEARRA_PACKING_OK) {
        status = clearra_geometry_emit_solution_family(
            &search, root_family, 0u);
    }
    clr_resource_report_observe_cpu_bytes(
        out_resource_report,
        clearra_geometry_search_resident_bytes(&search));
    clearra_geometry_solution_family_release(&search.solution_family);
    clearra_geometry_residual_memo_release(&search.residual_memo);
    clearra_geometry_projection_cache_release(&search.projection_cache);
    return status;
}

ClearraPackingStatus clearra_geometry_exact_cover_search_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    clr_pruning_proof_ledger_init(&pruning_ledger);
    return run_search(
        catalog,
        problem,
        0u,
        problem->piece_multiset_family.count,
        partition_index,
        partition_count,
        partition_depth,
        (ClearraGeometryExactCoverOutput){.sink = sink},
        0,
        out_resource_report,
        &pruning_ledger);
}

ClearraPackingStatus clearra_geometry_exact_cover_search_family_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t family_begin,
    uint16_t family_end,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    clr_pruning_proof_ledger_init(&pruning_ledger);
    return run_search(
        catalog,
        problem,
        family_begin,
        family_end,
        partition_index,
        partition_count,
        partition_depth,
        (ClearraGeometryExactCoverOutput){.sink = sink},
        0,
        out_resource_report,
        &pruning_ledger);
}

ClearraPackingStatus clearra_geometry_exact_cover_search_to_buffer(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    clr_pruning_proof_ledger_init(&pruning_ledger);
    return run_search(
        catalog,
        problem,
        0u,
        problem->piece_multiset_family.count,
        0u,
        1u,
        1u,
        (ClearraGeometryExactCoverOutput){.buffer = out_buffer},
        0,
        out_resource_report,
        &pruning_ledger);
}

ClearraPackingStatus clearra_geometry_exact_cover_search_graph(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    clr_pruning_proof_ledger_init(&pruning_ledger);
    return clearra_geometry_exact_cover_search_graph_with_pruning_ledger(
        catalog,
        problem,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        out_graph,
        out_resource_report,
        &pruning_ledger);
}

ClearraPackingStatus
clearra_geometry_exact_cover_search_graph_with_pruning_ledger(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    clr_pruning_evidence_policy evidence_policy,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    if (problem == 0 || out_pruning_ledger == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (clr_pruning_proof_ledger_init_with_policy(
            out_pruning_ledger, evidence_policy) != CLR_PRUNING_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return run_search(
        catalog,
        problem,
        0u,
        problem->piece_multiset_family.count,
        0u,
        1u,
        1u,
        (ClearraGeometryExactCoverOutput){0},
        out_graph,
        out_resource_report,
        out_pruning_ledger);
}
