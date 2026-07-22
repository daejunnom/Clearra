#include "clr_memory_internal.h"

#include <string.h>

bool clr_memory_leak_report_is_zero(ClrMemLeakReport report) {
    return report.live_scopes == 0 && report.live_allocations == 0 &&
        report.live_gpu_buffers == 0 && report.pending_release_queue == 0 &&
        report.pending_gpu_buffer_releases == 0;
}

ClrMemStatus clr_memory_debug_check_scope(const ClrScope *scope) {
    return clr_memory_scope_check_impl(scope);
}ClrMemStatus clr_memory_debug_poison_scope_for_test(ClrScope *scope) {
    if (scope == NULL || scope->allocations == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    scope->allocations->poisoned = true;
    if (scope->allocations->payload != NULL && scope->allocations->size > 0) {
        memset(scope->allocations->payload, CLR_POISON_BYTE, scope->allocations->size);
    }
    return CLR_MEM_OK;
}ClrMemStatus clr_memory_debug_corrupt_canary_for_test(ClrScope *scope) {
    if (scope == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (scope->allocations != NULL) {
        scope->allocations->tail_canary ^= UINT32_C(0x1);
    } else {
        scope->canary ^= UINT32_C(0x1);
    }
    return CLR_MEM_OK;
}
