#include "clr_memory_internal.h"

ClrMemStatus clr_arena_alloc(ClrScope *scope, size_t size, void **out_ptr) {
    return clr_memory_scope_alloc_impl(scope, size, out_ptr);
}

ClrMemStatus clr_pool_alloc(ClrScope *scope, size_t size, void **out_ptr) {
    return clr_memory_scope_alloc_impl(scope, size, out_ptr);
}

ClrMemStatus clr_scratch_alloc(ClrScope *scope, size_t size, void **out_ptr) {
    return clr_memory_scope_alloc_impl(scope, size, out_ptr);
}
