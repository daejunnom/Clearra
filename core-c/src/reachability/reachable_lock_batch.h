#ifndef CLEARRA_REACHABLE_LOCK_BATCH_H
#define CLEARRA_REACHABLE_LOCK_BATCH_H

#include "locked_reachability_internal.h"

typedef struct ClearraReachableLockSet {
    uint64_t anchor_bits[CLEARRA_ROTATION_STATE_COUNT];
    uint16_t visited_state_count;
    uint8_t complete;
    uint8_t reserved[5];
} ClearraReachableLockSet;

void clearra_reachable_lock_set_clear(ClearraReachableLockSet *set);
bool clearra_reachable_lock_set_contains(
    const ClearraReachableLockSet *set,
    ClearraBoard64Layout layout,
    uint8_t rotation,
    int8_t x,
    int8_t y);

/* Computes every reachable grounded lock state for one immutable board/piece
 * pair in one traversal. The caller owns and reuses the frontier. */
ClearraCandidateStatus clearra_reachable_lock_batch_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraCompactRuleProfile *rule,
    uint8_t mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachableLockSet *out_locks);

#endif
