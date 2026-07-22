#include "packing_problem.h"

#include <string.h>
void clearra_packing_candidate_view_clear(ClearraPackingCandidateView *candidate) {
    if (candidate != 0) {
        memset(candidate, 0, sizeof(*candidate));
    }
}void clearra_packing_candidate_buffer_clear(ClearraPackingCandidateBuffer *buffer) {
    if (buffer != 0) {
        buffer->count = 0u;
    }
}ClearraPackingStatus clearra_packing_candidate_buffer_push(
    ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate,
    uint16_t *out_index) {
    if (buffer == 0 || candidate == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (candidate->placed_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (buffer->count >= CLEARRA_PACKING_MAX_CANDIDATES) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }

    uint16_t index = buffer->count;
    buffer->final_boards[index] = candidate->final_board;
    buffer->shape_masks[index] = candidate->shape_mask;
    buffer->shape_keys[index] = candidate->shape_key;
    buffer->tiling_keys[index] = candidate->tiling_key;
    buffer->operation_set_keys[index] = candidate->operation_set_key;
    buffer->placed_counts[index] = candidate->placed_count;
    buffer->cleared_lines[index] = candidate->cleared_lines;
    buffer->geometry_variant_domains[index] =
        candidate->geometry_variant_domains;

    for (uint8_t piece_index = 0; piece_index < candidate->placed_count;
         piece_index++) {
        buffer->pieces[piece_index][index] = candidate->pieces[piece_index];
        buffer->rotations[piece_index][index] = candidate->rotations[piece_index];
        buffer->xs[piece_index][index] = candidate->xs[piece_index];
        buffer->ys[piece_index][index] = candidate->ys[piece_index];
        buffer->operation_ids[piece_index][index] =
            candidate->operation_ids[piece_index];
        buffer->operation_deleted_row_masks[piece_index][index] =
            candidate->operation_deleted_row_masks[piece_index];
        buffer->operation_masks[piece_index][index] =
            candidate->operation_masks[piece_index];
    }

    buffer->count++;
    if (out_index != 0) {
        *out_index = index;
    }
    return CLEARRA_PACKING_OK;
}ClearraPackingStatus clearra_packing_candidate_buffer_candidate_at(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    ClearraPackingCandidateView *out_candidate) {
    if (buffer == 0 || out_candidate == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (index >= buffer->count) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }

    clearra_packing_candidate_view_clear(out_candidate);
    out_candidate->candidate_id = (uint64_t)index + UINT64_C(1);
    out_candidate->canonical_operation_set_id = out_candidate->candidate_id;
    out_candidate->final_board = buffer->final_boards[index];
    out_candidate->shape_mask = buffer->shape_masks[index];
    out_candidate->shape_key = buffer->shape_keys[index];
    out_candidate->tiling_key = buffer->tiling_keys[index];
    out_candidate->operation_set_key = buffer->operation_set_keys[index];
    out_candidate->placed_count = buffer->placed_counts[index];
    out_candidate->cleared_lines = buffer->cleared_lines[index];
    out_candidate->geometry_variant_domains =
        buffer->geometry_variant_domains[index];

    for (uint8_t piece_index = 0; piece_index < out_candidate->placed_count;
         piece_index++) {
        out_candidate->pieces[piece_index] = buffer->pieces[piece_index][index];
        out_candidate->rotations[piece_index] =
            buffer->rotations[piece_index][index];
        out_candidate->xs[piece_index] = buffer->xs[piece_index][index];
        out_candidate->ys[piece_index] = buffer->ys[piece_index][index];
        out_candidate->operation_ids[piece_index] =
            buffer->operation_ids[piece_index][index];
        out_candidate->operation_deleted_row_masks[piece_index] =
            buffer->operation_deleted_row_masks[piece_index][index];
        out_candidate->operation_masks[piece_index] =
            buffer->operation_masks[piece_index][index];
    }

    return CLEARRA_PACKING_OK;
}
