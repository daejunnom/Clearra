#ifndef CLR_EXECUTION_CONTROL_H
#define CLR_EXECUTION_CONTROL_H

#include <stdbool.h>
#include <stdint.h>

typedef struct clr_execution_control {
    const volatile uint32_t *cancelled;
    uint32_t check_interval;
} clr_execution_control;

typedef enum clr_execution_control_status {
    CLR_EXECUTION_CONTROL_OK = 0,
    CLR_EXECUTION_CONTROL_INVALID_ARGUMENT = 1
} clr_execution_control_status;

clr_execution_control_status clr_execution_control_install(
    const clr_execution_control *control);
void clr_execution_control_clear(void);
bool clr_execution_cancel_requested(void);
bool clr_execution_control_poll(uint32_t *counter);

#endif
