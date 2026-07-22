#include "../include/clr_memory.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                   \
    do {                                                                                \
        ClrMemStatus actual_status = (EXPR);                                            \
        if (actual_status != (EXPECTED)) {                                              \
            fprintf(stderr, "%s:%d expected memory status %d but got %d\n", __FILE__,   \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                     \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                      \
    do {                                                                                \
        uint64_t actual_value = (uint64_t)(EXPR);                                       \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                 \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected %llu but got %llu\n", __FILE__, __LINE__,   \
                    (unsigned long long)expected_value,                                 \
                    (unsigned long long)actual_value);                                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                               \
    do {                                                                                \
        if (!(EXPR)) {                                                                  \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);              \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static void expect_zero_live_leaks(ClrMemContext *context) {
    ClrMemLeakReport report;
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.live_gpu_buffers, 0);
    EXPECT_U64(report.pending_release_queue, 0);
    EXPECT_U64(report.pending_gpu_buffer_releases, 0);
}static void context_create_release(void) {
    ClrMemContext *context = NULL;
    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_TRUE(context != NULL);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void memory_context_release_nulls_pointer(void) {
    ClrMemContext *context = NULL;
    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_TRUE(context != NULL);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
}static void memory_context_double_release_does_not_deref_freed_memory(void) {
    ClrMemContext *context = NULL;
    EXPECT_STATUS(clr_mem_context_release(NULL), CLR_MEM_INVALID_ARGUMENT);
    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_DOUBLE_RELEASE);
}static void memory_context_release_releases_live_scopes(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    void *memory = NULL;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_SEARCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_arena_alloc(scope, 64, &memory), CLR_MEM_OK);
    EXPECT_TRUE(memory != NULL);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
}static void memory_context_release_releases_gpu_records(void) {
    ClrMemContext *context = NULL;
    uint64_t buffer_id = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register(context, 2048, &buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
}static void memory_context_release_drains_release_queue_metadata(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_defer_scope(context, scope, 10), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
}static void memory_context_leak_report_before_release_reports_live_scopes(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void memory_context_leak_report_after_release_requires_snapshot(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport snapshot;
    ClrMemLeakReport after_release;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &snapshot), CLR_MEM_OK);
    EXPECT_U64(snapshot.live_scopes, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
    EXPECT_TRUE(context == NULL);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &after_release), CLR_MEM_INVALID_ARGUMENT);
    EXPECT_U64(snapshot.live_scopes, 1);
}static void search_scope_create_release(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    void *memory = NULL;
    ClrScopeKind kind = CLR_SCOPE_BATCH;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_SEARCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_kind(scope, &kind), CLR_MEM_OK);
    EXPECT_U64(kind, CLR_SCOPE_SEARCH);
    EXPECT_STATUS(clr_arena_alloc(scope, 64, &memory), CLR_MEM_OK);
    EXPECT_TRUE(memory != NULL);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void batch_scope_create_release(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    void *memory = NULL;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_pool_alloc(scope, 32, &memory), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void double_release_detect(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_SEARCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_DOUBLE_RELEASE);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.double_releases, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void scope_abort_releases_memory(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    void *memory = NULL;
    ClrMemLeakReport report;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_WORKER, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scratch_alloc(scope, 16, &memory), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_abort(scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.aborted_scopes, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void batch_scope_abort_releases_allocations(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    void *memory = NULL;
    ClrMemLeakReport report;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_pool_alloc(scope, 64, &memory), CLR_MEM_OK);
    EXPECT_TRUE(memory != NULL);
    EXPECT_STATUS(clr_scope_abort(scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.aborted_scopes, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void search_scope_release_releases_child_batch_scopes(void) {
    ClrMemContext *context = NULL;
    ClrScope *search_scope = NULL;
    ClrScope *batch_scope = NULL;
    void *search_memory = NULL;
    void *batch_memory = NULL;
    ClrMemLeakReport report;
    ClrScopeState batch_state = CLR_SCOPE_ACTIVE;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_SEARCH, &search_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &batch_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_arena_alloc(search_scope, 32, &search_memory), CLR_MEM_OK);
    EXPECT_STATUS(clr_pool_alloc(batch_scope, 32, &batch_memory), CLR_MEM_OK);
    EXPECT_TRUE(search_memory != NULL);
    EXPECT_TRUE(batch_memory != NULL);

    EXPECT_STATUS(clr_scope_release(search_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_state(batch_scope, &batch_state), CLR_MEM_OK);
    EXPECT_U64(batch_state, CLR_SCOPE_RELEASED);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void release_queue_uses_epoch_to_release_scope(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;
    ClrScopeState state = CLR_SCOPE_ACTIVE;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_defer_scope(context, scope, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_state(scope, &state), CLR_MEM_OK);
    EXPECT_U64(state, CLR_SCOPE_PENDING_RELEASE);
    EXPECT_STATUS(clr_release_queue_drain(context, 0), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 1);
    EXPECT_U64(report.pending_release_queue, 1);
    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_U64(epoch, 1);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_state(scope, &state), CLR_MEM_OK);
    EXPECT_U64(state, CLR_SCOPE_RELEASED);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void release_queue_drain_after_epoch_releases_gpu_buffer(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register_for_scope(context, scope, 512, &buffer_id),
                  CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 2), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);

    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_U64(report.pending_gpu_buffer_releases, 1);

    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 0);
    EXPECT_U64(report.pending_gpu_buffer_releases, 0);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void scope_deferred_for_release_cannot_be_released_directly_twice(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_BATCH, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_defer_scope(context, scope, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_INVALID_STATE);
    EXPECT_STATUS(clr_release_queue_defer_scope(context, scope, 1), CLR_MEM_INVALID_STATE);
    EXPECT_STATUS(clr_scope_abort(scope), CLR_MEM_INVALID_STATE);
    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_lifetime_is_reported(void) {
    ClrMemContext *context = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register(context, 1024, &buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_release_without_fence_rejected(void) {
    ClrMemContext *context = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register(context, 1024, &buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_INVALID_STATE);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_U64(report.pending_gpu_buffer_releases, 0);

    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_release_before_fence_is_deferred(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register_for_scope(context, scope, 1024, &buffer_id),
                  CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 2), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_U64(report.pending_gpu_buffer_releases, 1);

    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_U64(report.pending_gpu_buffer_releases, 1);

    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_release_before_fence_deferred(void) {
    gpu_buffer_release_before_fence_is_deferred();
}static void memory_leak_report_counts_pending_gpu_buffers(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register_for_scope(context, scope, 4096, &buffer_id),
                  CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 3), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);

    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 1);
    EXPECT_U64(report.pending_gpu_buffer_releases, 1);
    EXPECT_U64(report.live_scopes, 1);

    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_release_after_fence_is_clean(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register_for_scope(context, scope, 2048, &buffer_id),
                  CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_U64(epoch, 1);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_gpu_buffers, 0);
    EXPECT_U64(report.pending_gpu_buffer_releases, 0);
    EXPECT_STATUS(clr_scope_release(scope), CLR_MEM_OK);
    expect_zero_live_leaks(context);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void gpu_buffer_double_release_is_error(void) {
    ClrMemContext *context = NULL;
    ClrMemLeakReport report;
    uint64_t buffer_id = 0;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_register(context, 1024, &buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_set_fence_epoch(context, buffer_id, 1), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_OK);
    EXPECT_STATUS(clr_gpu_buffer_release(context, buffer_id), CLR_MEM_DOUBLE_RELEASE);
    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.double_releases, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}static void debug_poison_and_canary_are_detected(void) {
    ClrMemContext *context = NULL;
    ClrScope *poison_scope = NULL;
    ClrScope *canary_scope = NULL;
    void *memory = NULL;
    ClrMemLeakReport report;

    EXPECT_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &poison_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_arena_alloc(poison_scope, 8, &memory), CLR_MEM_OK);
    EXPECT_STATUS(clr_memory_debug_poison_scope_for_test(poison_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_memory_debug_check_scope(poison_scope), CLR_MEM_DEBUG_POISONED);
    EXPECT_STATUS(clr_scope_release(poison_scope), CLR_MEM_DEBUG_POISONED);

    EXPECT_STATUS(clr_scope_create(context, CLR_SCOPE_SEARCH, &canary_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_arena_alloc(canary_scope, 8, &memory), CLR_MEM_OK);
    EXPECT_STATUS(clr_memory_debug_corrupt_canary_for_test(canary_scope), CLR_MEM_OK);
    EXPECT_STATUS(clr_memory_debug_check_scope(canary_scope), CLR_MEM_CANARY_CORRUPTED);
    EXPECT_STATUS(clr_scope_release(canary_scope), CLR_MEM_CANARY_CORRUPTED);

    EXPECT_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.poison_detections, 1);
    EXPECT_U64(report.canary_failures, 1);
    EXPECT_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}
int main(void) {
    context_create_release();
    memory_context_release_nulls_pointer();
    memory_context_double_release_does_not_deref_freed_memory();
    memory_context_release_releases_live_scopes();
    memory_context_release_releases_gpu_records();
    memory_context_release_drains_release_queue_metadata();
    memory_context_leak_report_before_release_reports_live_scopes();
    memory_context_leak_report_after_release_requires_snapshot();
    search_scope_create_release();
    batch_scope_create_release();
    double_release_detect();
    scope_abort_releases_memory();
    batch_scope_abort_releases_allocations();
    search_scope_release_releases_child_batch_scopes();
    release_queue_uses_epoch_to_release_scope();
    release_queue_drain_after_epoch_releases_gpu_buffer();
    scope_deferred_for_release_cannot_be_released_directly_twice();
    gpu_buffer_lifetime_is_reported();
    gpu_buffer_release_without_fence_rejected();
    gpu_buffer_release_before_fence_is_deferred();
    gpu_buffer_release_before_fence_deferred();
    memory_leak_report_counts_pending_gpu_buffers();
    gpu_buffer_release_after_fence_is_clean();
    gpu_buffer_double_release_is_error();
    debug_poison_and_canary_are_detected();
    puts("core-c memory tests passed");
    return 0;
}
