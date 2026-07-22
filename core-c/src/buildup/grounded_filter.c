#include "buildup_internal.h"

clr_buildup_status clearra_buildup_grounded_filter_accepts(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement_mask) {
    if (!clearra_board64_layout_is_valid(layout) ||
        (placement_mask & ~layout.all_cells_mask) != 0) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }

    uint64_t floor_mask = layout.width == 64u
                              ? UINT64_MAX
                              : (UINT64_C(1) << layout.width) - UINT64_C(1);
    if ((placement_mask & floor_mask) != 0u) {
        return CLR_BUILDUP_OK;
    }
    if (layout.width < 64u &&
        ((placement_mask >> layout.width) & board) != 0u) {
        return CLR_BUILDUP_OK;
    }

    return CLR_BUILDUP_NOT_GROUNDED;
}
