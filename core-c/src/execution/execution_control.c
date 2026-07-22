#include "clr_execution_control.h"

#if defined(_MSC_VER)
#include <intrin.h>
#define CLR_THREAD_LOCAL __declspec(thread)
#else
#define CLR_THREAD_LOCAL _Thread_local
#endif

static CLR_THREAD_LOCAL clr_execution_control current_control;

static uint32_t atomic_cancelled_value(const volatile uint32_t *cancelled) {
#if defined(_MSC_VER)
    return (uint32_t)_InterlockedCompareExchange(
        (volatile long *)cancelled, 0L, 0L);
#else
    return __atomic_load_n(cancelled, __ATOMIC_ACQUIRE);
#endif
}

clr_execution_control_status clr_execution_control_install(
    const clr_execution_control *control) {
    if (control == 0 || control->cancelled == 0 || control->check_interval == 0u) {
        return CLR_EXECUTION_CONTROL_INVALID_ARGUMENT;
    }
    current_control = *control;
    return CLR_EXECUTION_CONTROL_OK;
}

void clr_execution_control_clear(void) {
    current_control = (clr_execution_control){0};
}

bool clr_execution_cancel_requested(void) {
    return current_control.cancelled != 0 &&
           atomic_cancelled_value(current_control.cancelled) != 0u;
}

bool clr_execution_control_poll(uint32_t *counter) {
    if (current_control.cancelled == 0) {
        return false;
    }
    if (counter == 0) {
        return clr_execution_cancel_requested();
    }
    *counter = *counter + 1u;
    if (*counter < current_control.check_interval) {
        return false;
    }
    *counter = 0u;
    return clr_execution_cancel_requested();
}
