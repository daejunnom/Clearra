#include "geometry_component_policy.h"

static uint8_t cell_count(uint64_t cells) {
    uint8_t count = 0u;
    while (cells != 0u) {
        cells &= cells - UINT64_C(1);
        count++;
    }
    return count;
}

static bool has_empty_vertical_separator(
    ClearraBoard64Layout layout,
    uint64_t remaining_cells) {
    uint64_t cells_left = 0u;
    for (uint8_t x = 0u; x < layout.width; ++x) {
        uint64_t column = 0u;
        for (uint8_t y = 0u; y < layout.height; ++y) {
            uint64_t cell = 0u;
            if (clearra_board64_cell_mask(layout, x, y, &cell) !=
                CLEARRA_BOARD64_OK) {
                return false;
            }
            column |= cell;
        }
        uint64_t column_cells = remaining_cells & column;
        uint64_t cells_right = remaining_cells & ~(cells_left | column);
        if (column_cells == 0u && cells_left != 0u && cells_right != 0u) {
            return true;
        }
        cells_left |= column_cells;
    }
    return false;
}

static bool has_bumper_trigger_column(
    const ClearraGeometryCatalog *catalog) {
    ClearraBoard64Layout layout = catalog->layout;
    if (layout.height < 2u) {
        return false;
    }
    for (uint8_t x = 0u; x < layout.width; ++x) {
        uint64_t top = 0u;
        if (clearra_board64_cell_mask(
                layout, x, (uint8_t)(layout.height - 1u), &top) !=
            CLEARRA_BOARD64_OK) {
            return false;
        }
        if ((catalog->required_fill_mask & top) == 0u ||
            (catalog->initial_board & top) != 0u) {
            continue;
        }
        bool lower_cells_filled = true;
        for (uint8_t y = 0u; y + 1u < layout.height; ++y) {
            uint64_t cell = 0u;
            if (clearra_board64_cell_mask(layout, x, y, &cell) !=
                    CLEARRA_BOARD64_OK ||
                (catalog->initial_board & cell) == 0u) {
                lower_cells_filled = false;
                break;
            }
        }
        if (lower_cells_filled) {
            return true;
        }
    }
    return false;
}

bool clearra_geometry_component_analysis_should_run(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    uint8_t depth) {
    if (catalog == 0 || !clearra_board64_layout_is_valid(catalog->layout)) {
        return false;
    }
    uint8_t remaining_cell_count = cell_count(remaining_cells);
    if (remaining_cell_count < 8u) {
        return false;
    }
    return depth == 0u || remaining_cell_count <= 16u ||
           has_empty_vertical_separator(catalog->layout, remaining_cells) ||
           (depth == 1u && has_bumper_trigger_column(catalog));
}
