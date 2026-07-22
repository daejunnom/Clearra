#ifndef CLEARRA_REACHABILITY_FIELD_H
#define CLEARRA_REACHABILITY_FIELD_H

#include "reachability.h"

ClearraReachabilityStatus clearra_reachability_field_is_placeable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_placeable);

ClearraReachabilityStatus clearra_reachability_field_is_grounded(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_grounded);

ClearraReachabilityStatus clearra_reachability_field_has_harddrop_path(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable);

ClearraReachabilityStatus clearra_reachability_field_first_success_kick(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t x,
    int8_t y,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateOperation *out_operation);

#endif
