#include "geometry_component_decomposition.h"

#include <string.h>

static uint8_t lowest_cell(uint64_t cells) {
    uint8_t cell = 0u;
    while ((cells & UINT64_C(1)) == 0u) {
        cells >>= 1u;
        cell++;
    }
    return cell;
}

static uint8_t cell_count(uint64_t cells) {
    uint8_t count = 0u;
    while (cells != 0u) {
        cells &= cells - UINT64_C(1);
        count++;
    }
    return count;
}

static uint8_t find_root(uint8_t parent[64], uint8_t cell) {
    uint8_t root = cell;
    while (parent[root] != root) {
        root = parent[root];
    }
    while (parent[cell] != cell) {
        uint8_t next = parent[cell];
        parent[cell] = root;
        cell = next;
    }
    return root;
}

static void union_cells(uint8_t parent[64], uint8_t left, uint8_t right) {
    uint8_t left_root = find_root(parent, left);
    uint8_t right_root = find_root(parent, right);
    if (left_root == right_root) {
        return;
    }
    if (right_root < left_root) {
        uint8_t swap = left_root;
        left_root = right_root;
        right_root = swap;
    }
    parent[right_root] = left_root;
}

static bool component_precedes(uint64_t left, uint64_t right) {
    uint8_t left_count = cell_count(left);
    uint8_t right_count = cell_count(right);
    return left_count < right_count ||
           (left_count == right_count && lowest_cell(left) < lowest_cell(right));
}

static void sort_components(ClearraGeometryComponentDecomposition *result) {
    for (uint8_t index = 1u; index < result->component_count; ++index) {
        uint64_t value = result->component_masks[index];
        uint8_t cursor = index;
        while (cursor != 0u &&
               component_precedes(value, result->component_masks[cursor - 1u])) {
            result->component_masks[cursor] =
                result->component_masks[cursor - 1u];
            cursor--;
        }
        result->component_masks[cursor] = value;
    }
}

bool clearra_geometry_component_decompose(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    ClearraGeometryRowPredicate row_is_feasible,
    void *predicate_context,
    ClearraGeometryComponentDecomposition *out_decomposition) {
    if (catalog == 0 || row_is_feasible == 0 || out_decomposition == 0 ||
        (remaining_cells & ~catalog->required_fill_mask) != 0u) {
        return false;
    }

    *out_decomposition = (ClearraGeometryComponentDecomposition){0};
    if (remaining_cells == 0u) {
        return true;
    }

    uint8_t parent[64];
    for (uint8_t cell = 0u; cell < 64u; ++cell) {
        parent[cell] = cell;
    }

    uint64_t supported_cells = 0u;
    for (uint32_t row_id = 0u; row_id < catalog->skeleton_count; ++row_id) {
        uint64_t row_cells = catalog->skeleton_cell_mask[row_id];
        if (row_cells == 0u || (row_cells & remaining_cells) != row_cells ||
            !row_is_feasible(predicate_context, row_id)) {
            continue;
        }
        uint8_t first = lowest_cell(row_cells);
        uint64_t rest = row_cells & ~(UINT64_C(1) << first);
        while (rest != 0u) {
            uint64_t bit = rest & (~rest + UINT64_C(1));
            union_cells(parent, first, lowest_cell(bit));
            rest &= ~bit;
        }
        supported_cells |= row_cells;
        out_decomposition->feasible_row_count++;
    }

    out_decomposition->unsupported_cells = remaining_cells & ~supported_cells;
    uint64_t cells = remaining_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = lowest_cell(bit);
        uint8_t root = find_root(parent, cell);
        uint8_t component_index = UINT8_MAX;
        for (uint8_t index = 0u;
             index < out_decomposition->component_count;
             ++index) {
            if (lowest_cell(out_decomposition->component_masks[index]) == root) {
                component_index = index;
                break;
            }
        }
        if (component_index == UINT8_MAX) {
            if (out_decomposition->component_count >=
                CLEARRA_GEOMETRY_MAX_COMPONENTS) {
                return false;
            }
            component_index = out_decomposition->component_count++;
        }
        out_decomposition->component_masks[component_index] |= bit;
        cells &= ~bit;
    }
    sort_components(out_decomposition);
    return true;
}

bool clearra_geometry_component_make_composition_plan(
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t remaining_cells,
    ClearraGeometryComponentCompositionPlan *out_plan) {
    if (decomposition == 0 || out_plan == 0 || remaining_cells == 0u ||
        decomposition->unsupported_cells != 0u ||
        decomposition->component_count <= 1u) {
        return false;
    }

    uint64_t partition = 0u;
    for (uint8_t index = 0u; index < decomposition->component_count; ++index) {
        uint64_t component = decomposition->component_masks[index];
        if (component == 0u || (component & ~remaining_cells) != 0u ||
            (partition & component) != 0u ||
            (index != 0u &&
             !component_precedes(
                 decomposition->component_masks[index - 1u], component))) {
            return false;
        }
        partition |= component;
    }
    if (partition != remaining_cells) {
        return false;
    }

    uint64_t owner = decomposition->component_masks[0];
    uint64_t remainder = remaining_cells & ~owner;
    if (owner == 0u || remainder == 0u || (owner & remainder) != 0u ||
        (owner | remainder) != remaining_cells) {
        return false;
    }
    *out_plan = (ClearraGeometryComponentCompositionPlan){
        .owner_component_mask = owner,
        .remainder_mask = remainder,
        .component_count = decomposition->component_count,
    };
    return true;
}
