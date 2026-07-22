#include "geometry_exact_cover_internal.h"

#include "geometry_component_decomposition.h"
#include "geometry_component_policy.h"
#include "geometry_component_solution_table.h"
#include "geometry_exact_cover_proof.h"

#include "clr_execution_control.h"
#include "clr_search_profile.h"

#include <limits.h>

typedef struct ClearraGeometryFeasibleRowContext {
    const ClearraGeometryExactCoverSearch *search;
    const ClearraActivePieceFamily *active_family;
    uint64_t remaining_cells;
} ClearraGeometryFeasibleRowContext;

typedef struct ClearraGeometryFamilyAccumulator {
    ClearraGeometryFamilyRef levels[32];
} ClearraGeometryFamilyAccumulator;

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

static ClearraPackingStatus authorize_component_empty(
    ClearraGeometryExactCoverSearch *search,
    uint8_t depth,
    uint64_t remaining_cells,
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t discriminator,
    bool *out_authorized) {
    if (search == 0 || search->pruning_ledger == 0 ||
        decomposition == 0 || out_authorized == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_authorized = false;
    return clearra_geometry_authorize_component_infeasible(
        search,
        depth,
        remaining_cells,
        decomposition,
        discriminator,
        out_authorized);
}

static uint8_t single_bit_index(uint64_t bit) {
    uint8_t index = 0u;
    while ((bit & UINT64_C(1)) == 0u) {
        bit >>= 1u;
        index++;
    }
    return index;
}

static bool component_row_is_feasible(void *context, uint32_t row_id) {
    const ClearraGeometryFeasibleRowContext *row_context =
        (const ClearraGeometryFeasibleRowContext *)context;
    ClearraActivePieceFamily ignored;
    return row_context != 0 && clearra_geometry_row_is_feasible(
        row_context->search,
        row_context->active_family,
        row_id,
        row_context->remaining_cells,
        &ignored);
}

static bool select_component_pivot(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t component_cells,
    uint8_t *out_cell) {
    uint32_t best_count = UINT32_MAX;
    uint8_t best_cell = UINT8_MAX;
    uint64_t cells = component_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = single_bit_index(bit);
        uint32_t begin = search->catalog->cell_support_offsets[cell];
        uint32_t end = search->catalog->cell_support_offsets[cell + 1u];
        uint32_t count = 0u;
        for (uint32_t cursor = begin; cursor < end; ++cursor) {
            ClearraActivePieceFamily ignored;
            count += clearra_geometry_row_is_feasible(
                search,
                active_family,
                search->catalog->cell_support_row_ids[cursor],
                component_cells,
                &ignored)
                ? 1u
                : 0u;
        }
        if (count == 0u) {
            *out_cell = cell;
            return false;
        }
        if (count < best_count) {
            best_count = count;
            best_cell = cell;
        }
        cells &= ~bit;
    }
    *out_cell = best_cell;
    return best_cell != UINT8_MAX;
}

_Static_assert(
    CLEARRA_PACKING_MAX_PIECES <= 15u,
    "four-bit component signatures require the exact-cover depth limit");

static uint8_t signature_piece_count(uint32_t signature, uint8_t piece) {
    return (uint8_t)(
        (signature >> ((uint32_t)(piece - CLR_PIECE_I) * 4u)) & UINT32_C(0x0f));
}

static uint8_t signature_total_count(uint32_t signature) {
    uint8_t total = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        total = (uint8_t)(total + signature_piece_count(signature, piece));
    }
    return total;
}

static bool active_family_after_signature(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *base_family,
    uint32_t signature,
    ClearraActivePieceFamily *out_family) {
    ClearraActivePieceFamily active = *base_family;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint8_t count = signature_piece_count(signature, piece);
        for (uint8_t offset = 1u; offset <= count; ++offset) {
            ClearraActivePieceFamily next;
            if (!clearra_geometry_piece_family_advance(
                    &search->piece_family_domain,
                    &active,
                    piece,
                    (uint8_t)(search->used_piece_counts[piece] + offset),
                    &next)) {
                return false;
            }
            active = next;
        }
    }
    *out_family = active;
    return true;
}

static void add_signature_to_used_counts(
    ClearraGeometryExactCoverSearch *search,
    uint32_t signature) {
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        search->used_piece_counts[piece] = (uint8_t)(
            search->used_piece_counts[piece] +
            signature_piece_count(signature, piece));
    }
}

static void remove_signature_from_used_counts(
    ClearraGeometryExactCoverSearch *search,
    uint32_t signature) {
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        search->used_piece_counts[piece] = (uint8_t)(
            search->used_piece_counts[piece] -
            signature_piece_count(signature, piece));
    }
}

static ClearraPackingStatus family_accumulator_add(
    ClearraGeometryExactCoverSearch *search,
    ClearraGeometryFamilyAccumulator *accumulator,
    ClearraGeometryFamilyRef branch) {
    uint8_t level = 0u;
    while (level < 32u &&
           accumulator->levels[level] != CLEARRA_GEOMETRY_FAMILY_INVALID) {
        branch = clearra_geometry_solution_family_union(
            &search->solution_family, accumulator->levels[level], branch);
        if (branch == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        accumulator->levels[level] = CLEARRA_GEOMETRY_FAMILY_INVALID;
        level++;
    }
    if (level == 32u) {
        clr_resource_report_mark_truncated(
            search->resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    accumulator->levels[level] = branch;
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus family_accumulator_finish(
    ClearraGeometryExactCoverSearch *search,
    ClearraGeometryFamilyAccumulator *accumulator,
    ClearraGeometryFamilyRef *out_family) {
    *out_family = CLEARRA_GEOMETRY_FAMILY_INVALID;
    for (uint8_t level = 32u; level != 0u; --level) {
        ClearraGeometryFamilyRef branch = accumulator->levels[level - 1u];
        if (branch == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            continue;
        }
        if (*out_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            *out_family = branch;
            continue;
        }
        *out_family = clearra_geometry_solution_family_union(
            &search->solution_family, *out_family, branch);
        if (*out_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
    }
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus component_path_family(
    ClearraGeometryExactCoverSearch *search,
    const uint32_t *row_ids,
    uint8_t row_count,
    ClearraGeometryFamilyRef *out_family) {
    ClearraGeometryFamilyRef family = CLEARRA_GEOMETRY_FAMILY_EMPTY;
    for (uint8_t index = row_count; index != 0u; --index) {
        family = clearra_geometry_solution_family_append(
            &search->solution_family, row_ids[index - 1u], family);
        if (family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
    }
    *out_family = family;
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus enumerate_component_solutions(
    ClearraGeometryExactCoverSearch *search,
    uint64_t component_cells,
    uint8_t global_depth,
    uint8_t local_depth,
    uint32_t piece_signature,
    const ClearraActivePieceFamily *active_family,
    uint32_t row_ids[CLEARRA_PACKING_MAX_PIECES],
    ClearraGeometryComponentSolutionTable *table,
    bool *out_table_available) {
    if (!*out_table_available) {
        return CLEARRA_PACKING_OK;
    }
    if (clr_execution_control_poll(&search->cancellation_poll_counter)) {
        clr_resource_report_mark_truncated(
            search->resource_report, CLR_RESOURCE_TRUNCATION_CANCELLED);
        return CLEARRA_PACKING_CANCELLED;
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
        search->resource_report,
        (size_t)global_depth + local_depth + 1u);

    if (component_cells == 0u) {
        ClearraGeometryFamilyRef path = CLEARRA_GEOMETRY_FAMILY_INVALID;
        ClearraPackingStatus status = component_path_family(
            search, row_ids, local_depth, &path);
        if (status != CLEARRA_PACKING_OK) {
            return status;
        }
        ClearraGeometryComponentInsertStatus insert_status =
            clearra_geometry_component_solution_table_insert(
                table,
                &search->solution_family,
                piece_signature,
                path);
        if (insert_status == CLEARRA_GEOMETRY_COMPONENT_TABLE_UNAVAILABLE) {
            *out_table_available = false;
            return CLEARRA_PACKING_OK;
        }
        if (insert_status == CLEARRA_GEOMETRY_COMPONENT_FAMILY_UNAVAILABLE) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        return CLEARRA_PACKING_OK;
    }

    uint8_t pivot = 0u;
    if (!select_component_pivot(
            search, active_family, component_cells, &pivot)) {
        return CLEARRA_PACKING_OK;
    }
    uint32_t begin = search->catalog->cell_support_offsets[pivot];
    uint32_t end = search->catalog->cell_support_offsets[pivot + 1u];
    for (uint32_t cursor = begin; cursor < end; ++cursor) {
        uint32_t row_id = search->catalog->cell_support_row_ids[cursor];
        ClearraActivePieceFamily next_family;
        if (!clearra_geometry_row_is_feasible(
                search,
                active_family,
                row_id,
                component_cells,
                &next_family)) {
            continue;
        }
        if (local_depth >= CLEARRA_PACKING_MAX_PIECES) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        uint8_t piece = (uint8_t)search->catalog->skeleton_piece_kind[row_id];
        uint32_t shift = (uint32_t)(piece - CLR_PIECE_I) * 4u;
        uint8_t count = signature_piece_count(piece_signature, piece);
        if (count == UINT8_C(0x0f)) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        row_ids[local_depth] = row_id;
        search->used_piece_counts[piece]++;
        ClearraPackingStatus status = enumerate_component_solutions(
            search,
            component_cells & ~search->catalog->skeleton_cell_mask[row_id],
            global_depth,
            (uint8_t)(local_depth + 1u),
            piece_signature + (UINT32_C(1) << shift),
            &next_family,
            row_ids,
            table,
            out_table_available);
        search->used_piece_counts[piece]--;
        if (status != CLEARRA_PACKING_OK || !*out_table_available) {
            return status;
        }
    }
    return CLEARRA_PACKING_OK;
}

ClearraPackingStatus clearra_geometry_try_component_composition(
    ClearraGeometryExactCoverSearch *search,
    uint64_t remaining_cells,
    uint8_t depth,
    uint64_t prefix_hash,
    const ClearraActivePieceFamily *active_family,
    bool *out_applied,
    ClearraGeometryFamilyRef *out_family) {
    *out_applied = false;
    *out_family = CLEARRA_GEOMETRY_FAMILY_INVALID;

    if (!clearra_geometry_component_analysis_should_run(
            search->catalog, remaining_cells, depth)) {
        return CLEARRA_PACKING_OK;
    }

    ClearraGeometryFeasibleRowContext row_context = {
        .search = search,
        .active_family = active_family,
        .remaining_cells = remaining_cells,
    };
    ClearraGeometryComponentDecomposition decomposition;
    if (!clearra_geometry_component_decompose(
            search->catalog,
            remaining_cells,
            component_row_is_feasible,
            &row_context,
            &decomposition)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (decomposition.unsupported_cells != 0u) {
        bool authorized = false;
        ClearraPackingStatus status = authorize_component_empty(
            search,
            depth,
            remaining_cells,
            &decomposition,
            UINT64_C(1),
            &authorized);
        *out_applied = authorized;
        return status;
    }
    for (uint8_t index = 0u; index < decomposition.component_count; ++index) {
        if (popcount64(decomposition.component_masks[index]) %
                CLEARRA_TETROMINO_AREA !=
            0u) {
            bool authorized = false;
            ClearraPackingStatus status = authorize_component_empty(
                search,
                depth,
                remaining_cells,
                &decomposition,
                UINT64_C(2) + index,
                &authorized);
            *out_applied = authorized;
            return status;
        }
    }
    if (decomposition.component_count <= 1u ||
        (search->partition_count != 1u && depth < search->partition_depth)) {
        return CLEARRA_PACKING_OK;
    }

    ClearraGeometryComponentCompositionPlan composition_plan;
    if (!clearra_geometry_component_make_composition_plan(
            &decomposition, remaining_cells, &composition_plan)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    clr_search_profile_count(
        CLR_PROFILE_PACKING_GEOMETRY_COMPONENT_COMPOSITIONS, 1u);

    uint64_t component_cells = composition_plan.owner_component_mask;
    uint8_t component_piece_count =
        (uint8_t)(popcount64(component_cells) / CLEARRA_TETROMINO_AREA);
    size_t original_family_max_bytes = search->solution_family.max_bytes;
    size_t table_max_bytes = SIZE_MAX;
    if (search->output.max_total_bytes != SIZE_MAX) {
        size_t resident_bytes = clearra_geometry_search_resident_bytes(search);
        size_t global_headroom = resident_bytes >= search->output.max_total_bytes
            ? 0u
            : search->output.max_total_bytes - resident_bytes;
        size_t family_headroom = search->solution_family.resident_bytes >=
                search->solution_family.max_bytes
            ? 0u
            : search->solution_family.max_bytes -
                  search->solution_family.resident_bytes;
        size_t reservable = global_headroom < family_headroom
            ? global_headroom
            : family_headroom;
        table_max_bytes = reservable / 4u;
        if (table_max_bytes == 0u ||
            table_max_bytes > search->solution_family.max_bytes) {
            return CLEARRA_PACKING_OK;
        }
        search->solution_family.max_bytes -= table_max_bytes;
    }
    ClearraGeometryComponentSolutionTable table;
    if (!clearra_geometry_component_solution_table_init(
            &table, 64u, table_max_bytes)) {
        search->solution_family.max_bytes = original_family_max_bytes;
        return CLEARRA_PACKING_OK;
    }

    uint32_t row_ids[CLEARRA_PACKING_MAX_PIECES];
    bool table_available = true;
    clr_resource_report resource_before_component = *search->resource_report;
    ClearraGeometrySolutionFamilyCheckpoint family_checkpoint;
    clearra_geometry_solution_family_checkpoint_begin(
        &search->solution_family, &family_checkpoint);
    ClearraPackingStatus status = enumerate_component_solutions(
        search,
        component_cells,
        depth,
        0u,
        0u,
        active_family,
        row_ids,
        &table,
        &table_available);
    if (status != CLEARRA_PACKING_OK || !table_available) {
        bool optional_memory_failure =
            status == CLEARRA_PACKING_CAPACITY_EXCEEDED &&
            search->resource_report->truncation_reason ==
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED;
        clearra_geometry_solution_family_checkpoint_rollback(
            &search->solution_family, &family_checkpoint);
        clearra_geometry_component_solution_table_release(&table);
        search->solution_family.max_bytes = original_family_max_bytes;
        if (optional_memory_failure || !table_available) {
            *search->resource_report = resource_before_component;
            return CLEARRA_PACKING_OK;
        }
        return status;
    }
    clearra_geometry_solution_family_checkpoint_commit(
        &search->solution_family, &family_checkpoint);

    search->component_workspace_bytes += table.resident_bytes;
    clr_resource_report_observe_cpu_bytes(
        search->resource_report, clearra_geometry_search_resident_bytes(search));
    search->component_decomposition_count++;
    *out_applied = true;

    ClearraGeometryFamilyAccumulator accumulator = {0};
    ClearraGeometryComponentSolutionIterator iterator;
    clearra_geometry_component_solution_iterator_begin(&table, &iterator);
    const ClearraGeometryComponentSolutionEntry *entry = 0;
    while ((entry = clearra_geometry_component_solution_iterator_next(
                &iterator)) != 0) {
        if (signature_total_count(entry->piece_count_signature) !=
            component_piece_count) {
            status = CLEARRA_PACKING_INVALID_ARGUMENT;
            break;
        }
        ClearraActivePieceFamily next_active;
        if (!active_family_after_signature(
                search,
                active_family,
                entry->piece_count_signature,
                &next_active)) {
            continue;
        }
        add_signature_to_used_counts(search, entry->piece_count_signature);
        ClearraGeometryFamilyRef rest_family =
            CLEARRA_GEOMETRY_FAMILY_INVALID;
        status = clearra_geometry_search_exact_cover(
            search,
            composition_plan.remainder_mask,
            (uint8_t)(depth + component_piece_count),
            clearra_cache_key_mix_u64(
                prefix_hash,
                (uint64_t)entry->piece_count_signature + UINT64_C(1)),
            &next_active,
            &rest_family);
        remove_signature_from_used_counts(search, entry->piece_count_signature);
        if (status != CLEARRA_PACKING_OK) {
            break;
        }
        if (rest_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            continue;
        }
        ClearraGeometryFamilyRef product =
            clearra_geometry_solution_family_product(
                &search->solution_family, entry->family_ref, rest_family);
        if (product == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            status = CLEARRA_PACKING_CAPACITY_EXCEEDED;
            break;
        }
        status = family_accumulator_add(search, &accumulator, product);
        if (status != CLEARRA_PACKING_OK) {
            break;
        }
    }
    if (status == CLEARRA_PACKING_OK) {
        status = family_accumulator_finish(search, &accumulator, out_family);
    }
    if (status == CLEARRA_PACKING_OK &&
        *out_family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        bool authorized = false;
        status = authorize_component_empty(
            search,
            depth,
            remaining_cells,
            &decomposition,
            UINT64_C(0x100000000) | table.entry_count,
            &authorized);
        if (!authorized) {
            *out_applied = false;
        }
    }
    search->component_workspace_bytes -= table.resident_bytes;
    clearra_geometry_component_solution_table_release(&table);
    search->solution_family.max_bytes = original_family_max_bytes;
    return status;
}
