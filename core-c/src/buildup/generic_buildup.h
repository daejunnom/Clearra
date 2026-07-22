#ifndef CLEARRA_GENERIC_BUILDUP_H
#define CLEARRA_GENERIC_BUILDUP_H

#include "clr_problem.h"

#include <stdint.h>
uint16_t clearra_buildup_mvp1_max_operations(void);
clr_buildup_status clearra_buildup_operation_set_runtime_status(
    uint32_t operation_count);
clr_buildup_status clearra_buildup_runtime_status_for_board(
    const clr_board_descriptor *board);
#endif
