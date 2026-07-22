#ifndef CLEARRA_BUILDUP_COMPLETION_MEMO_H
#define CLEARRA_BUILDUP_COMPLETION_MEMO_H

#include "buildup_memo.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_BUILDUP_COMPLETION_MEMO_MIN_CAPACITY 2048u
#define CLEARRA_BUILDUP_COMPLETION_MEMO_MAX_CAPACITY 65536u

typedef struct ClearraBuildUpCompletionMemoEntry {
    uint64_t key_hash;
    uint64_t completion_count;
    uint64_t current_board_mask;
    uint64_t reachability_relevant_state;
    uint64_t bag_remainder_key;
    uint16_t remaining_ops_bitset;
    uint16_t deleted_row_mask;
    uint16_t hold_cursor;
    uint16_t piece_source_cursor;
    uint16_t bag_epoch;
    uint8_t deleted_count;
    uint8_t hold_piece;
    uint8_t hold_empty;
    uint8_t cleared_lines;
    uint8_t reserved[10];
} ClearraBuildUpCompletionMemoEntry;

_Static_assert(
    sizeof(ClearraBuildUpCompletionMemoEntry) == 64u,
    "BuildUp completion memo entries must fit one 64-byte cache line");

typedef struct ClearraBuildUpCompletionMemoStorage
    ClearraBuildUpCompletionMemoStorage;

typedef struct ClearraBuildUpCompletionMemo {
    ClearraBuildUpCompletionMemoEntry *entries;
    uint16_t *occupied_generations;
    ClearraBuildUpCompletionMemoStorage *storage;
    uint32_t capacity;
    uint32_t max_load;
    uint32_t count;
    uint32_t max_probe_length;
    uint64_t cache_identity_hash;
    uint64_t piece_source_id;
    uint64_t provenance_id;
    uint64_t probes;
    uint64_t hits;
    uint64_t insertions;
    uint64_t saturation_skips;
    uint16_t generation;
    uint8_t owns_entries;
    uint8_t reserved[5];
} ClearraBuildUpCompletionMemo;

struct ClearraBuildUpCompletionMemoStorage {
    ClearraBuildUpCompletionMemoEntry *entries;
    uint16_t *occupied_generations;
    uint32_t capacity;
    uint32_t max_probe_length;
    uint64_t saturation_skips;
    uint16_t generation;
    uint8_t reserved[2];
};

void clearra_buildup_completion_memo_init(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_problem *problem);
void clearra_buildup_completion_memo_init_with_storage(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_problem *problem,
    ClearraBuildUpCompletionMemoStorage *storage);
void clearra_buildup_completion_memo_release(
    ClearraBuildUpCompletionMemo *memo);
void clearra_buildup_completion_memo_storage_release(
    ClearraBuildUpCompletionMemoStorage *storage);
bool clearra_buildup_completion_memo_lookup(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_memo_key *key,
    uint64_t *out_completion_count);
void clearra_buildup_completion_memo_insert(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_memo_key *key,
    uint64_t completion_count);

#endif
