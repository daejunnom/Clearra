#ifndef CLR_MEMORY_INTERNAL_H
#define CLR_MEMORY_INTERNAL_H

#include "../../include/clr_memory.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLR_SCOPE_CANARY UINT32_C(0xc1ea44a1)
#define CLR_ALLOC_CANARY UINT32_C(0xc1ea44b2)
#define CLR_POISON_BYTE 0xdd
typedef struct ClrAllocation {
    void *payload;
    size_t size;
    uint32_t head_canary;
    uint32_t tail_canary;
    bool poisoned;
    struct ClrAllocation *next;
} ClrAllocation;typedef struct ClrReleaseQueueEntry {
    ClrScope *scope;
    uint64_t release_epoch;
    struct ClrReleaseQueueEntry *next;
} ClrReleaseQueueEntry;typedef struct ClrGpuBufferRecord {
    uint64_t buffer_id;
    uint64_t owner_scope_id;
    uint64_t fence_epoch;
    uint64_t released_epoch;
    size_t byte_len;
    bool fence_epoch_set;
    bool pending_release;
    bool released;
    struct ClrGpuBufferRecord *next;
} ClrGpuBufferRecord;
struct ClrMemContext {
    uint64_t epoch;
    uint64_t next_scope_id;
    uint64_t next_gpu_buffer_id;
    ClrScope *scopes;
    ClrReleaseQueueEntry *release_queue;
    ClrGpuBufferRecord *gpu_buffers;
    ClrMemLeakReport counters;
};

struct ClrScope {
    ClrMemContext *context;
    uint64_t scope_id;
    ClrScopeKind kind;
    ClrScopeState state;
    bool released;
    bool aborted;
    uint32_t canary;
    ClrAllocation *allocations;
    struct ClrScope *next;
};
ClrMemStatus clr_memory_scope_alloc_impl(ClrScope *scope, size_t size, void **out_ptr);
ClrMemStatus clr_memory_scope_release_impl(ClrScope *scope, bool aborted);
ClrMemStatus clr_memory_scope_release_pending_impl(ClrScope *scope);
ClrMemStatus clr_memory_scope_check_impl(const ClrScope *scope);
void clr_memory_scope_free_metadata(ClrScope *scope);
ClrMemStatus clr_gpu_buffer_drain_pending_impl(
    ClrMemContext *context,
    uint64_t through_epoch);
#endif
