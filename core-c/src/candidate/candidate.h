#ifndef CLEARRA_CORE_C_CANDIDATE_H
#define CLEARRA_CORE_C_CANDIDATE_H

#include "../board/board64.h"
#include "../cache/cache_identity.h"
#include "../piece/operation.h"
#include "../rules/rules.h"
#include "clr_board.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_CANDIDATE_MAX_OPERATIONS 256
typedef enum ClearraCandidateStatus {
    CLEARRA_CANDIDATE_OK = 0,
    CLEARRA_CANDIDATE_INVALID_ARGUMENT = 1,
    CLEARRA_CANDIDATE_INVALID_PIECE = 2,
    CLEARRA_CANDIDATE_INVALID_ROTATION = 3,
    CLEARRA_CANDIDATE_OUT_OF_BOUNDS = 4,
    CLEARRA_CANDIDATE_COLLISION = 5,
    CLEARRA_CANDIDATE_UNREACHABLE = 6,
    CLEARRA_CANDIDATE_CAPACITY_EXCEEDED = 7
} ClearraCandidateStatus;
typedef enum ClearraCandidatePiece {
    CLEARRA_CANDIDATE_PIECE_I = CLR_PIECE_I,
    CLEARRA_CANDIDATE_PIECE_O = CLR_PIECE_O,
    CLEARRA_CANDIDATE_PIECE_T = CLR_PIECE_T,
    CLEARRA_CANDIDATE_PIECE_S = CLR_PIECE_S,
    CLEARRA_CANDIDATE_PIECE_Z = CLR_PIECE_Z,
    CLEARRA_CANDIDATE_PIECE_J = CLR_PIECE_J,
    CLEARRA_CANDIDATE_PIECE_L = CLR_PIECE_L
} ClearraCandidatePiece;
typedef enum ClearraCandidateRotation {
    CLEARRA_CANDIDATE_ROTATION_ZERO = 0,
    CLEARRA_CANDIDATE_ROTATION_RIGHT = 1,
    CLEARRA_CANDIDATE_ROTATION_TWO = 2,
    CLEARRA_CANDIDATE_ROTATION_LEFT = 3
} ClearraCandidateRotation;
typedef enum ClearraCandidateMode {
    CLEARRA_CANDIDATE_MODE_HARDDROP = 1,
    CLEARRA_CANDIDATE_MODE_LOCKED = 2,
    CLEARRA_CANDIDATE_MODE_LOCKED_180 = 3
} ClearraCandidateMode;
typedef enum ClearraRotationTransitionKind {
    CLEARRA_ROTATION_TRANSITION_NONE = 0,
    CLEARRA_ROTATION_TRANSITION_CLOCKWISE = 1,
    CLEARRA_ROTATION_TRANSITION_COUNTER_CLOCKWISE = 2,
    CLEARRA_ROTATION_TRANSITION_HALF_TURN = 3
} ClearraRotationTransitionKind;
struct ClearraReachabilityKickTable;
typedef struct ClearraReachabilityKickTable ClearraReachabilityKickTable;typedef ClearraCompactKickOffset ClearraKickOffset;
typedef struct ClearraCandidateOperation {
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint64_t mask;
    uint8_t transition_kind;
    uint8_t kick_index;
    int8_t kick_dx;
    int8_t kick_dy;
} ClearraCandidateOperation;
typedef struct ClearraCandidateList {
    ClearraCandidateOperation operations[CLEARRA_CANDIDATE_MAX_OPERATIONS];
    uint16_t count;
} ClearraCandidateList;typedef struct ClearraCandidateCacheEntry {
    uint64_t key;
    uint16_t count;
    bool occupied;
} ClearraCandidateCacheEntry;
void clearra_candidate_list_clear(ClearraCandidateList *list);
ClearraCandidateStatus clearra_candidate_shape_bounds(
    uint8_t piece,
    uint8_t rotation,
    uint8_t *out_width,
    uint8_t *out_height);
uint8_t clearra_candidate_unique_rotation_count(uint8_t piece);
ClearraCandidateStatus clearra_candidate_mask_for_piece(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t *out_mask);
ClearraCandidateStatus clearra_candidate_transition_kind(
    uint8_t from_rotation,
    uint8_t to_rotation,
    ClearraRotationTransitionKind *out_kind);
ClearraCandidateStatus clearra_candidate_push_operation(
    ClearraCandidateList *list,
    ClearraCandidateOperation operation);
ClearraCandidateStatus clearra_candidate_is_reachable_operation(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable);
ClearraCandidateStatus clearra_candidate_first_success_kick(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t anchor_x,
    int8_t anchor_y,
    const ClearraKickOffset *offsets,
    uint8_t offset_count,
    ClearraCandidateOperation *out_operation);
ClearraCandidateStatus clearra_candidate_normalized_kick_delta(
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t kick_dx,
    int8_t kick_dy,
    int8_t *out_dx,
    int8_t *out_dy);
ClearraCandidateStatus clearra_harddrop_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list);
ClearraCandidateStatus clearra_locked_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list);
ClearraCandidateStatus clearra_locked_candidates_generate_with_kicks(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateList *out_list);
ClearraCandidateStatus clearra_locked180_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list);
ClearraCandidateStatus clearra_locked180_candidates_generate_with_kicks(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateList *out_list);
ClearraCandidateStatus clearra_candidate_search(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t active_piece,
    const ClearraCompactRuleProfile *rule,
    uint8_t mode,
    ClearraCandidateList *out_list);
uint64_t clearra_candidate_cache_key(
    ClearraCacheIdentity identity,
    uint8_t active_piece,
    uint8_t rule_kick_mode);
void clearra_candidate_cache_entry_clear(ClearraCandidateCacheEntry *entry);
void clearra_candidate_cache_entry_store(
    ClearraCandidateCacheEntry *entry,
    uint64_t key,
    uint16_t count);
bool clearra_candidate_cache_entry_matches(
    ClearraCandidateCacheEntry entry,
    uint64_t key);
#endif
