#include "packing_problem.h"
static void sort_masks(uint64_t *masks, uint8_t count) {
    for (uint8_t index = 1; index < count; index++) {
        uint64_t value = masks[index];
        uint8_t cursor = index;
        while (cursor > 0 && masks[cursor - 1u] > value) {
            masks[cursor] = masks[cursor - 1u];
            cursor--;
        }
        masks[cursor] = value;
    }
}static uint64_t tiling_operation_tuple(
    uint8_t piece,
    uint8_t rotation,
    uint64_t operation_mask,
    uint16_t required_deleted_row_mask) {
    uint64_t descriptor = 0;
    descriptor |= ((uint64_t)piece) << 56;
    descriptor |= ((uint64_t)rotation) << 48;
    descriptor |= ((uint64_t)required_deleted_row_mask) << 32;
    descriptor ^= operation_mask;
    return descriptor;
}static void sort_tiling_descriptors(uint64_t *descriptors, uint8_t count) {
    for (uint8_t index = 1; index < count; index++) {
        uint64_t value = descriptors[index];
        uint8_t cursor = index;
        while (cursor > 0 && descriptors[cursor - 1u] > value) {
            descriptors[cursor] = descriptors[cursor - 1u];
            cursor--;
        }
        descriptors[cursor] = value;
    }
}uint64_t clearra_packing_cell_partition_key(
    ClearraBoard64Layout layout,
    const uint64_t *operation_masks,
    uint8_t operation_count) {
    if (operation_masks == 0 || operation_count > CLEARRA_PACKING_MAX_PIECES) {
        return 0;
    }

    uint64_t sorted_masks[CLEARRA_PACKING_MAX_PIECES];
    for (uint8_t index = 0; index < operation_count; index++) {
        sorted_masks[index] = operation_masks[index] & layout.all_cells_mask;
    }
    sort_masks(sorted_masks, operation_count);

    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b54494c45));
    hash = clearra_cache_key_mix_u64(hash, layout.width);
    hash = clearra_cache_key_mix_u64(hash, layout.height);
    hash = clearra_cache_key_mix_u64(hash, operation_count);
    for (uint8_t index = 0; index < operation_count; index++) {
        hash = clearra_cache_key_mix_u64(hash, sorted_masks[index]);
    }
    return hash;
}

typedef struct ClearraGeometryTilingTuple {
    uint64_t mask;
    uint8_t piece;
} ClearraGeometryTilingTuple;

static int compare_geometry_tiling_tuple(
    const ClearraGeometryTilingTuple *left,
    const ClearraGeometryTilingTuple *right) {
    if (left->piece != right->piece) {
        return left->piece < right->piece ? -1 : 1;
    }
    if (left->mask != right->mask) {
        return left->mask < right->mask ? -1 : 1;
    }
    return 0;
}

static void sort_geometry_tiling_tuples(
    ClearraGeometryTilingTuple *tuples,
    uint8_t count) {
    for (uint8_t index = 1u; index < count; ++index) {
        ClearraGeometryTilingTuple value = tuples[index];
        uint8_t cursor = index;
        while (cursor > 0u &&
               compare_geometry_tiling_tuple(&tuples[cursor - 1u], &value) > 0) {
            tuples[cursor] = tuples[cursor - 1u];
            cursor--;
        }
        tuples[cursor] = value;
    }
}

uint64_t clearra_packing_geometry_tiling_key(
    ClearraBoard64Layout layout,
    const uint8_t *pieces,
    const uint64_t *operation_masks,
    uint8_t operation_count) {
    if (pieces == 0 || operation_masks == 0 ||
        operation_count > CLEARRA_PACKING_MAX_PIECES) {
        return 0u;
    }
    ClearraGeometryTilingTuple tuples[CLEARRA_PACKING_MAX_PIECES];
    for (uint8_t index = 0u; index < operation_count; ++index) {
        tuples[index] = (ClearraGeometryTilingTuple){
            .mask = operation_masks[index] & layout.all_cells_mask,
            .piece = pieces[index],
        };
    }
    sort_geometry_tiling_tuples(tuples, operation_count);

    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b47454f4d));
    hash = clearra_cache_key_mix_u64(hash, layout.width);
    hash = clearra_cache_key_mix_u64(hash, layout.height);
    hash = clearra_cache_key_mix_u64(hash, operation_count);
    for (uint8_t index = 0u; index < operation_count; ++index) {
        hash = clearra_cache_key_mix_u64(hash, tuples[index].piece);
        hash = clearra_cache_key_mix_u64(hash, tuples[index].mask);
    }
    return hash;
}

uint64_t clearra_packing_tiling_key_with_piece_identity(
    ClearraBoard64Layout layout,
    const uint8_t *pieces,
    const uint8_t *rotations,
    const uint64_t *operation_masks,
    const uint16_t *operation_deleted_row_masks,
    uint8_t operation_count) {
    if (pieces == 0 || rotations == 0 || operation_masks == 0 ||
        operation_deleted_row_masks == 0 ||
        operation_count > CLEARRA_PACKING_MAX_PIECES) {
        return 0;
    }

    uint64_t descriptors[CLEARRA_PACKING_MAX_PIECES];
    for (uint8_t index = 0; index < operation_count; index++) {
        descriptors[index] = tiling_operation_tuple(
            pieces[index],
            rotations[index],
            operation_masks[index] & layout.all_cells_mask,
            operation_deleted_row_masks[index]);
    }
    sort_tiling_descriptors(descriptors, operation_count);

    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b54494c32));
    hash = clearra_cache_key_mix_u64(hash, layout.width);
    hash = clearra_cache_key_mix_u64(hash, layout.height);
    hash = clearra_cache_key_mix_u64(hash, operation_count);
    for (uint8_t index = 0; index < operation_count; index++) {
        hash = clearra_cache_key_mix_u64(hash, descriptors[index]);
    }
    return hash;
}

#include "packing_problem.h"

uint64_t clearra_packing_shape_key(ClearraBoard64Layout layout, uint64_t shape_mask) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b53485045));
    hash = clearra_cache_key_mix_u64(hash, layout.width);
    hash = clearra_cache_key_mix_u64(hash, layout.height);
    hash = clearra_cache_key_mix_u64(hash, shape_mask & layout.all_cells_mask);
    return hash;
}

#include "packing_problem.h"
static uint64_t operation_descriptor(
    const ClearraPackingCandidateView *candidate,
    uint8_t index) {
    uint64_t descriptor = 0;
    descriptor |= ((uint64_t)candidate->pieces[index]) << 56;
    descriptor |= ((uint64_t)candidate->rotations[index]) << 48;
    descriptor |= ((uint64_t)(uint8_t)(candidate->xs[index] + 64)) << 40;
    descriptor |= ((uint64_t)(uint8_t)(candidate->ys[index] + 64)) << 32;
    descriptor |= ((uint64_t)candidate->operation_ids[index]) << 16;
    descriptor ^=
        ((uint64_t)candidate->operation_deleted_row_masks[index]) << 1;
    descriptor ^= candidate->operation_masks[index];
    return descriptor;
}static void sort_operation_set_descriptors(uint64_t *descriptors, uint8_t count) {
    for (uint8_t index = 1; index < count; index++) {
        uint64_t value = descriptors[index];
        uint8_t cursor = index;
        while (cursor > 0 && descriptors[cursor - 1u] > value) {
            descriptors[cursor] = descriptors[cursor - 1u];
            cursor--;
        }
        descriptors[cursor] = value;
    }
}uint64_t clearra_packing_operation_set_key(
    const ClearraPackingCandidateView *candidate) {
    if (candidate == 0 || candidate->placed_count > CLEARRA_PACKING_MAX_PIECES) {
        return 0;
    }

    uint64_t descriptors[CLEARRA_PACKING_MAX_PIECES];
    for (uint8_t index = 0; index < candidate->placed_count; index++) {
        descriptors[index] = operation_descriptor(candidate, index);
    }
    sort_operation_set_descriptors(descriptors, candidate->placed_count);

    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b4f505345));
    hash = clearra_cache_key_mix_u64(hash, candidate->placed_count);
    for (uint8_t index = 0; index < candidate->placed_count; index++) {
        hash = clearra_cache_key_mix_u64(hash, descriptors[index]);
    }
    return hash;
}
