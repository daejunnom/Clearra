#ifndef CLEARRA_BUILDUP_REACHABILITY_RESULT_H
#define CLEARRA_BUILDUP_REACHABILITY_RESULT_H

#include "buildup_internal.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_BUILDUP_REACHABLE_FLAG UINT8_C(0x01)
#define CLEARRA_BUILDUP_USED_KICK_FLAG UINT8_C(0x02)
#define CLEARRA_BUILDUP_USED_180_FLAG UINT8_C(0x04)
#define CLEARRA_BUILDUP_ROTATION_EVIDENCE_FLAG UINT8_C(0x08)
#define CLEARRA_BUILDUP_FIRST_SUCCESS_FLAG UINT8_C(0x10)
#define CLEARRA_BUILDUP_PATH_COMPLETE_FLAG UINT8_C(0x20)
#define CLEARRA_BUILDUP_LAST_ACTION_ROTATION_FLAG UINT8_C(0x40)

typedef struct ClearraBuildUpReachabilityResult {
    uint64_t path_digest;
    uint16_t visited_states;
    uint8_t flags;
    uint8_t rotation_from;
    uint8_t rotation_to;
    uint8_t rotation_request;
    uint8_t kick_index;
    int8_t kick_dx;
    int8_t kick_dy;
    int8_t predecessor_x;
    int8_t predecessor_y;
    int8_t result_x;
    int8_t result_y;
    uint8_t reserved[3];
} ClearraBuildUpReachabilityResult;

_Static_assert(
    sizeof(ClearraBuildUpReachabilityResult) == 24u,
    "BuildUp reachability results must remain compact");

uint64_t clearra_buildup_reachability_path_digest(
    const ClearraReachabilityReport *report);
void clearra_buildup_reachability_result_from_report(
    const ClearraReachabilityReport *report,
    ClearraBuildUpReachabilityResult *out_result);
bool clearra_buildup_reachability_result_has_flag(
    const ClearraBuildUpReachabilityResult *result,
    uint8_t flag);
clr_buildup_status clearra_buildup_reachability_check_compiled(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraBuildUpReachabilityResult *out_result);

#endif
