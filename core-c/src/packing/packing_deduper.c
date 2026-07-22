#include "packing_problem.h"

#include <stdlib.h>

#define CLEARRA_PACKING_DEDUP_BUCKETS 257u

typedef struct ClearraPackingHostReduceScratch {
    ClearraPackingCandidateBuffer unique_buffer;
    uint16_t raw_to_unique[CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t unique_to_canonical[CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t sorted_indices[CLEARRA_PACKING_MAX_CANDIDATES];
} ClearraPackingHostReduceScratch;
ClearraPackingStatus clearra_packing_deduper_push_unique(
    ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate,
    uint16_t *out_index,
    bool *out_inserted) {
    if (buffer == 0 || candidate == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint16_t candidate_bucket = clearra_packing_hash_bucket(
        candidate->operation_set_key, CLEARRA_PACKING_DEDUP_BUCKETS);
    for (uint16_t index = 0; index < buffer->count; index++) {
        uint16_t existing_bucket = clearra_packing_hash_bucket(
            buffer->operation_set_keys[index], CLEARRA_PACKING_DEDUP_BUCKETS);
        if (existing_bucket != candidate_bucket) {
            continue;
        }
        if (clearra_packing_hash_confirm_exact(buffer, index, candidate)) {
            if (out_index != 0) {
                *out_index = index;
            }
            if (out_inserted != 0) {
                *out_inserted = false;
            }
            return CLEARRA_PACKING_OK;
        }
    }

    ClearraPackingStatus status =
        clearra_packing_candidate_buffer_push(buffer, candidate, out_index);
    if (status == CLEARRA_PACKING_OK && out_inserted != 0) {
        *out_inserted = true;
    }
    return status;
}void clearra_canonical_packing_table_clear(ClearraCanonicalPackingTable *table) {
    if (table != 0) {
        clearra_packing_candidate_buffer_clear(&table->candidates);
        table->raw_count = 0;
        for (uint16_t index = 0; index < CLEARRA_PACKING_MAX_CANDIDATES; index++) {
            table->candidate_ids[index] = 0;
            table->raw_to_canonical_ids[index] = 0;
        }
    }
}static int compare_u64(uint64_t left, uint64_t right) {
    if (left < right) {
        return -1;
    }
    if (left > right) {
        return 1;
    }
    return 0;
}static int compare_i8(int8_t left, int8_t right) {
    if (left < right) {
        return -1;
    }
    if (left > right) {
        return 1;
    }
    return 0;
}static int compare_candidate_at(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t left,
    uint16_t right) {
    int comparison =
        compare_u64(buffer->shape_keys[left], buffer->shape_keys[right]);
    if (comparison != 0) {
        return comparison;
    }
    comparison =
        compare_u64(buffer->tiling_keys[left], buffer->tiling_keys[right]);
    if (comparison != 0) {
        return comparison;
    }
    comparison = compare_u64(buffer->operation_set_keys[left],
                             buffer->operation_set_keys[right]);
    if (comparison != 0) {
        return comparison;
    }
    comparison =
        compare_u64(buffer->placed_counts[left], buffer->placed_counts[right]);
    if (comparison != 0) {
        return comparison;
    }
    comparison = compare_u64(
        buffer->geometry_variant_domains[left],
        buffer->geometry_variant_domains[right]);
    if (comparison != 0) {
        return comparison;
    }

    for (uint8_t piece_index = 0; piece_index < buffer->placed_counts[left];
         piece_index++) {
        comparison = compare_u64(buffer->pieces[piece_index][left],
                                 buffer->pieces[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison = compare_u64(buffer->rotations[piece_index][left],
                                 buffer->rotations[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison =
            compare_i8(buffer->xs[piece_index][left], buffer->xs[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison =
            compare_i8(buffer->ys[piece_index][left], buffer->ys[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison = compare_u64(buffer->operation_ids[piece_index][left],
                                 buffer->operation_ids[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison = compare_u64(
            buffer->operation_deleted_row_masks[piece_index][left],
            buffer->operation_deleted_row_masks[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
        comparison = compare_u64(buffer->operation_masks[piece_index][left],
                                 buffer->operation_masks[piece_index][right]);
        if (comparison != 0) {
            return comparison;
        }
    }

    comparison =
        compare_u64(buffer->final_boards[left], buffer->final_boards[right]);
    if (comparison != 0) {
        return comparison;
    }
    comparison =
        compare_u64(buffer->shape_masks[left], buffer->shape_masks[right]);
    if (comparison != 0) {
        return comparison;
    }
    return compare_u64(buffer->cleared_lines[left], buffer->cleared_lines[right]);
}static void sorted_unique_indices(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t *indices) {
    for (uint16_t index = 0; index < buffer->count; index++) {
        indices[index] = index;
    }

    for (uint16_t index = 1; index < buffer->count; index++) {
        uint16_t value = indices[index];
        uint16_t cursor = index;
        while (cursor > 0 &&
               compare_candidate_at(buffer, indices[cursor - 1u], value) > 0) {
            indices[cursor] = indices[cursor - 1u];
            cursor--;
        }
        indices[cursor] = value;
    }
}static ClearraPackingStatus push_unique_by_operation_set(
    ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate,
    uint16_t *out_index) {
    uint16_t candidate_bucket = clearra_packing_hash_bucket(
        clearra_packing_candidate_identity_key(candidate), CLEARRA_PACKING_DEDUP_BUCKETS);
    for (uint16_t index = 0; index < buffer->count; index++) {
        ClearraPackingCandidateView existing;
        ClearraPackingStatus status =
            clearra_packing_candidate_buffer_candidate_at(buffer, index, &existing);
        if (status != CLEARRA_PACKING_OK) {
            return status;
        }
        uint16_t existing_bucket = clearra_packing_hash_bucket(
            clearra_packing_candidate_identity_key(&existing),
            CLEARRA_PACKING_DEDUP_BUCKETS);
        if (existing_bucket != candidate_bucket) {
            continue;
        }
        if (clearra_packing_hash_confirm_same_operation_set(buffer, index, candidate)) {
            if (out_index != 0) {
                *out_index = index;
            }
            return CLEARRA_PACKING_OK;
        }
    }

    return clearra_packing_candidate_buffer_push(buffer, candidate, out_index);
}ClearraPackingStatus clearra_packing_host_reduce(
    const ClearraPackingCandidateBuffer *raw_buffer,
    ClearraCanonicalPackingTable *out_table) {
    if (raw_buffer == 0 || out_table == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    ClearraPackingHostReduceScratch *scratch =
        (ClearraPackingHostReduceScratch *)malloc(sizeof(*scratch));
    if (scratch == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }

    ClearraPackingStatus final_status = CLEARRA_PACKING_OK;
    clearra_packing_candidate_buffer_clear(&scratch->unique_buffer);
    clearra_canonical_packing_table_clear(out_table);
    out_table->raw_count = raw_buffer->count;

    for (uint16_t raw_index = 0; raw_index < raw_buffer->count; raw_index++) {
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status = clearra_packing_candidate_buffer_candidate_at(
            raw_buffer, raw_index, &candidate);
        if (status != CLEARRA_PACKING_OK) {
            final_status = status;
            goto cleanup;
        }

        uint16_t unique_index = 0;
        status =
            push_unique_by_operation_set(
                &scratch->unique_buffer, &candidate, &unique_index);
        if (status != CLEARRA_PACKING_OK) {
            final_status = status;
            goto cleanup;
        }
        scratch->raw_to_unique[raw_index] = unique_index;
    }

    sorted_unique_indices(&scratch->unique_buffer, scratch->sorted_indices);
    for (uint16_t canonical_index = 0;
         canonical_index < scratch->unique_buffer.count;
         canonical_index++) {
        uint16_t unique_index = scratch->sorted_indices[canonical_index];
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status = clearra_packing_candidate_buffer_candidate_at(
            &scratch->unique_buffer, unique_index, &candidate);
        if (status != CLEARRA_PACKING_OK) {
            final_status = status;
            goto cleanup;
        }
        status = clearra_packing_candidate_buffer_push(
            &out_table->candidates, &candidate, 0);
        if (status != CLEARRA_PACKING_OK) {
            final_status = status;
            goto cleanup;
        }
        out_table->candidate_ids[canonical_index] = canonical_index;
        scratch->unique_to_canonical[unique_index] = canonical_index;
    }

    for (uint16_t raw_index = 0; raw_index < raw_buffer->count; raw_index++) {
        out_table->raw_to_canonical_ids[raw_index] =
            scratch->unique_to_canonical[scratch->raw_to_unique[raw_index]];
    }

cleanup:
    free(scratch);
    return final_status;
}

#include "packing_problem.h"
uint64_t clearra_packing_candidate_identity_key(
    const ClearraPackingCandidateView *candidate) {
    if (candidate == 0) {
        return 0;
    }

    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x5041434b484f5354));
    hash = clearra_cache_key_mix_u64(hash, candidate->shape_key);
    hash = clearra_cache_key_mix_u64(hash, candidate->tiling_key);
    hash = clearra_cache_key_mix_u64(hash, candidate->operation_set_key);
    hash = clearra_cache_key_mix_u64(hash, candidate->final_board);
    hash = clearra_cache_key_mix_u64(hash, candidate->cleared_lines);
    return hash;
}uint16_t clearra_packing_hash_bucket(uint64_t key, uint16_t bucket_count) {
    if (bucket_count == 0) {
        return 0;
    }
    uint64_t mixed = key ^ (key >> 33);
    mixed *= UINT64_C(0xff51afd7ed558ccd);
    mixed ^= mixed >> 33;
    return (uint16_t)(mixed % bucket_count);
}

#include "packing_problem.h"
bool clearra_packing_hash_confirm_exact(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    const ClearraPackingCandidateView *candidate) {
    if (buffer == 0 || candidate == 0 || index >= buffer->count) {
        return false;
    }
    if (buffer->final_boards[index] != candidate->final_board ||
        buffer->shape_masks[index] != candidate->shape_mask ||
        buffer->shape_keys[index] != candidate->shape_key ||
        buffer->tiling_keys[index] != candidate->tiling_key ||
        buffer->operation_set_keys[index] != candidate->operation_set_key ||
        buffer->placed_counts[index] != candidate->placed_count ||
        buffer->geometry_variant_domains[index] !=
            candidate->geometry_variant_domains ||
        buffer->cleared_lines[index] != candidate->cleared_lines) {
        return false;
    }

    for (uint8_t piece_index = 0; piece_index < candidate->placed_count;
         piece_index++) {
        if (buffer->pieces[piece_index][index] != candidate->pieces[piece_index] ||
            buffer->rotations[piece_index][index] != candidate->rotations[piece_index] ||
            buffer->xs[piece_index][index] != candidate->xs[piece_index] ||
            buffer->ys[piece_index][index] != candidate->ys[piece_index] ||
            buffer->operation_ids[piece_index][index] !=
                candidate->operation_ids[piece_index] ||
            buffer->operation_deleted_row_masks[piece_index][index] !=
                candidate->operation_deleted_row_masks[piece_index] ||
            buffer->operation_masks[piece_index][index] !=
                candidate->operation_masks[piece_index]) {
            return false;
        }
    }

    return true;
}bool clearra_packing_hash_confirm_same_operation_set(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    const ClearraPackingCandidateView *candidate) {
    if (buffer == 0 || candidate == 0 || index >= buffer->count) {
        return false;
    }
    if (buffer->shape_keys[index] != candidate->shape_key ||
        buffer->tiling_keys[index] != candidate->tiling_key ||
        buffer->operation_set_keys[index] != candidate->operation_set_key ||
        buffer->placed_counts[index] != candidate->placed_count ||
        buffer->geometry_variant_domains[index] !=
            candidate->geometry_variant_domains ||
        buffer->final_boards[index] != candidate->final_board ||
        buffer->cleared_lines[index] != candidate->cleared_lines) {
        return false;
    }

    for (uint8_t piece_index = 0; piece_index < candidate->placed_count;
         piece_index++) {
        if (buffer->pieces[piece_index][index] != candidate->pieces[piece_index] ||
            buffer->rotations[piece_index][index] != candidate->rotations[piece_index] ||
            buffer->xs[piece_index][index] != candidate->xs[piece_index] ||
            buffer->ys[piece_index][index] != candidate->ys[piece_index] ||
            buffer->operation_ids[piece_index][index] !=
                candidate->operation_ids[piece_index] ||
            buffer->operation_deleted_row_masks[piece_index][index] !=
                candidate->operation_deleted_row_masks[piece_index] ||
            buffer->operation_masks[piece_index][index] !=
                candidate->operation_masks[piece_index]) {
            return false;
        }
    }

    return true;
}

#include "packing_problem.h"

bool clearra_packing_candidate_buffer_exactly_matches(
    const ClearraPackingCandidateBuffer *left,
    const ClearraPackingCandidateBuffer *right) {
    if (left == 0 || right == 0 || left->count != right->count) {
        return false;
    }

    for (uint16_t index = 0; index < left->count; index++) {
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status =
            clearra_packing_candidate_buffer_candidate_at(right, index, &candidate);
        if (status != CLEARRA_PACKING_OK ||
            !clearra_packing_hash_confirm_exact(left, index, &candidate)) {
            return false;
        }
    }

    return true;
}
