#ifndef CLR_MEMORY_H
#define CLR_MEMORY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum ClrMemStatus {
    CLR_MEM_OK = 0,
    CLR_MEM_INVALID_ARGUMENT = 1,
    CLR_MEM_OUT_OF_MEMORY = 2,
    CLR_MEM_DOUBLE_RELEASE = 3,
    CLR_MEM_ABORTED = 4,
    CLR_MEM_CANARY_CORRUPTED = 5,
    CLR_MEM_DEBUG_POISONED = 6,
    CLR_MEM_NOT_FOUND = 7,
    CLR_MEM_INVALID_STATE = 8
} ClrMemStatus;

typedef enum ClrScopeKind {
    CLR_SCOPE_SEARCH = 1,
    CLR_SCOPE_BATCH = 2,
    CLR_SCOPE_WORKER = 3,
    CLR_SCOPE_GPU_TRANSFER = 4
} ClrScopeKind;

typedef enum ClrScopeState {
    CLR_SCOPE_ACTIVE = 1,
    CLR_SCOPE_PENDING_RELEASE = 2,
    CLR_SCOPE_RELEASED = 3,
    CLR_SCOPE_ABORTED = 4
} ClrScopeState;

typedef struct ClrMemContext ClrMemContext;
typedef struct ClrScope ClrScope;

typedef struct ClrMemLeakReport {
    uint64_t live_scopes;
    uint64_t live_allocations;
    uint64_t live_gpu_buffers;
    uint64_t pending_release_queue;
    uint64_t pending_gpu_buffer_releases;
    uint64_t released_scopes;
    uint64_t aborted_scopes;
    uint64_t double_releases;
    uint64_t canary_failures;
    uint64_t poison_detections;
} ClrMemLeakReport;

ClrMemStatus clr_mem_context_create(ClrMemContext **out_context);
ClrMemStatus clr_mem_context_release(ClrMemContext **context);
ClrMemStatus clr_mem_context_leak_report(
    const ClrMemContext *context,
    ClrMemLeakReport *out_report);

ClrMemStatus clr_scope_create(
    ClrMemContext *context,
    ClrScopeKind kind,
    ClrScope **out_scope);
ClrMemStatus clr_scope_release(ClrScope *scope);
ClrMemStatus clr_scope_abort(ClrScope *scope);
ClrMemStatus clr_scope_kind(const ClrScope *scope, ClrScopeKind *out_kind);
ClrMemStatus clr_scope_state(const ClrScope *scope, ClrScopeState *out_state);
bool clr_scope_is_released(const ClrScope *scope);

/* Scope allocations are uninitialized; callers initialize their active range. */
ClrMemStatus clr_arena_alloc(ClrScope *scope, size_t size, void **out_ptr);
ClrMemStatus clr_pool_alloc(ClrScope *scope, size_t size, void **out_ptr);
ClrMemStatus clr_scratch_alloc(ClrScope *scope, size_t size, void **out_ptr);

uint64_t clr_epoch_current(const ClrMemContext *context);
ClrMemStatus clr_epoch_advance(ClrMemContext *context, uint64_t *out_epoch);
ClrMemStatus clr_release_queue_defer_scope(
    ClrMemContext *context,
    ClrScope *scope,
    uint64_t release_epoch);
ClrMemStatus clr_release_queue_drain(ClrMemContext *context, uint64_t through_epoch);

ClrMemStatus clr_gpu_buffer_register(
    ClrMemContext *context,
    size_t byte_len,
    uint64_t *out_buffer_id);
ClrMemStatus clr_gpu_buffer_register_for_scope(
    ClrMemContext *context,
    const ClrScope *owner_scope,
    size_t byte_len,
    uint64_t *out_buffer_id);
ClrMemStatus clr_gpu_buffer_set_fence_epoch(
    ClrMemContext *context,
    uint64_t buffer_id,
    uint64_t fence_epoch);
ClrMemStatus clr_gpu_buffer_release(ClrMemContext *context, uint64_t buffer_id);

ClrMemStatus clr_memory_debug_check_scope(const ClrScope *scope);
ClrMemStatus clr_memory_debug_poison_scope_for_test(ClrScope *scope);
ClrMemStatus clr_memory_debug_corrupt_canary_for_test(ClrScope *scope);

#ifdef __cplusplus
}
#endif

#endif
