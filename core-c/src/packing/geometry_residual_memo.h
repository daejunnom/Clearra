#ifndef CLEARRA_GEOMETRY_RESIDUAL_MEMO_H
#define CLEARRA_GEOMETRY_RESIDUAL_MEMO_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct ClearraGeometryResidualMemoEntry {
    uint64_t remaining_cells;
    uint32_t packed_piece_counts;
    uint32_t suffix_family_ref;
} ClearraGeometryResidualMemoEntry;

_Static_assert(
    sizeof(ClearraGeometryResidualMemoEntry) == 16u,
    "geometry residual memo entries must remain cache-compact");

typedef struct ClearraGeometryResidualMemo {
    ClearraGeometryResidualMemoEntry *entries;
    uint64_t *occupied_words;
    size_t capacity;
    size_t count;
    size_t mask;
    size_t allocation_bytes;
    size_t resident_bytes;
    size_t max_bytes;
    size_t lookup_count;
    size_t hit_count;
    size_t max_probe_length;
    bool insertion_disabled;
} ClearraGeometryResidualMemo;

void clearra_geometry_residual_memo_init(
    ClearraGeometryResidualMemo *memo,
    size_t expected_rows,
    size_t max_bytes);
void clearra_geometry_residual_memo_release(
    ClearraGeometryResidualMemo *memo);
bool clearra_geometry_residual_memo_lookup(
    ClearraGeometryResidualMemo *memo,
    uint64_t remaining_cells,
    uint32_t packed_piece_counts,
    uint32_t *out_suffix_family_ref);
void clearra_geometry_residual_memo_insert(
    ClearraGeometryResidualMemo *memo,
    uint64_t remaining_cells,
    uint32_t packed_piece_counts,
    uint32_t suffix_family_ref);

#endif
