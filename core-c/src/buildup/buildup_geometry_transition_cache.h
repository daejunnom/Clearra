#ifndef CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_H
#define CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_H

#include "buildup_reachability_result.h"

#define CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES 64u

typedef struct ClearraBuildUpGeometryTransitionKey {
    uint64_t board_mask;
    uint64_t operation_mask;
    uint64_t reachability_relevant_state;
    uint16_t deleted_row_mask;
    uint16_t required_deleted_row_mask;
    uint16_t operation_id;
    uint8_t deleted_count;
    uint8_t cleared_lines;
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint8_t trace_mode;
    uint8_t reserved[3];
} ClearraBuildUpGeometryTransitionKey;

typedef struct ClearraBuildUpGeometryTransitionResult {
    uint64_t board_mask;
    uint64_t reachability_relevant_state;
    ClearraLineClearState line_clear_state;
    uint32_t status;
    uint8_t cleared_lines;
    uint8_t reserved[3];
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
} ClearraBuildUpGeometryTransitionResult;

typedef struct ClearraBuildUpGeometryTransitionHotResult {
    uint64_t board_mask;
    uint64_t reachability_relevant_state;
    ClearraLineClearState line_clear_state;
    uint8_t status;
    uint8_t cleared_lines;
    uint8_t reserved[2];
} ClearraBuildUpGeometryTransitionHotResult;

typedef struct ClearraBuildUpGeometryTransitionColdResult {
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
} ClearraBuildUpGeometryTransitionColdResult;

typedef struct ClearraBuildUpGeometryTransitionHotEntry {
    ClearraBuildUpGeometryTransitionKey key;
    ClearraBuildUpGeometryTransitionHotResult result;
} ClearraBuildUpGeometryTransitionHotEntry;

typedef struct ClearraBuildUpGeometryTransitionCache {
    ClearraBuildUpGeometryTransitionHotEntry *hot_entries;
    void *hot_entries_allocation;
    ClearraBuildUpGeometryTransitionColdResult *cold_results;
    void *cold_results_allocation;
    uint32_t *epochs;
    uint32_t *cold_epochs;
    uint32_t capacity;
    uint32_t epoch;
    uint64_t insertion_count;
    uint64_t collision_count;
} ClearraBuildUpGeometryTransitionCache;

void clearra_buildup_geometry_transition_cache_prepare(
    ClearraBuildUpGeometryTransitionCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries);
void clearra_buildup_geometry_transition_cache_release(
    ClearraBuildUpGeometryTransitionCache *cache);
bool clearra_buildup_geometry_transition_cache_lookup(
    const ClearraBuildUpGeometryTransitionCache *cache,
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation,
    uint8_t trace_mode,
    ClearraBuildUpGeometryTransitionResult *out_result);
void clearra_buildup_geometry_transition_cache_insert(
    ClearraBuildUpGeometryTransitionCache *cache,
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation,
    clr_buildup_status status,
    const ClearraBuildUpState *next_state,
    const clr_buildup_trace_step *trace_step,
    const clr_kick_evidence_view *kick_evidence,
    uint8_t trace_mode);

#endif
