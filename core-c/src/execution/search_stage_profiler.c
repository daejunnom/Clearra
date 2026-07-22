#include "clr_search_profile.h"

#include <stdlib.h>
#include <string.h>

#if defined(CLEARRA_ENABLE_STAGE_PROFILING)
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

static INIT_ONCE QPC_INIT_ONCE = INIT_ONCE_STATIC_INIT;
static LARGE_INTEGER QPC_FREQUENCY;

static BOOL CALLBACK initialize_qpc_frequency(
    PINIT_ONCE init_once,
    PVOID parameter,
    PVOID *context) {
    (void)init_once;
    (void)parameter;
    (void)context;
    return QueryPerformanceFrequency(&QPC_FREQUENCY);
}
#else
#include <stdatomic.h>
#include <time.h>
#endif

#if defined(_MSC_VER)
#define CLR_THREAD_LOCAL __declspec(thread)
#else
#define CLR_THREAD_LOCAL _Thread_local
#endif

typedef struct clr_search_thread_profile {
    struct clr_search_thread_profile *next;
    uint64_t duration_ns[CLR_PROFILE_STAGE_COUNT];
    uint64_t invocation_count[CLR_PROFILE_STAGE_COUNT];
    uint64_t work_item_count[CLR_PROFILE_STAGE_COUNT];
    uint64_t packing_depth_expand_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_reduce_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_emit_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_frontier_in[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_frontier_out[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_candidate_count[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint8_t packing_depth_incomplete[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
} clr_search_thread_profile;

typedef struct clr_search_profile_session {
    clr_search_stage_profile *owner;
    clr_search_thread_profile *threads;
#if defined(_WIN32)
    SRWLOCK thread_list_lock;
#else
    atomic_flag thread_list_lock;
#endif
    uint64_t generation;
} clr_search_profile_session;

#if defined(_WIN32)
static PVOID volatile ACTIVE_SESSION;
static volatile LONG64 NEXT_SESSION_GENERATION;
#else
static _Atomic(clr_search_profile_session *) ACTIVE_SESSION;
static atomic_uint_fast64_t NEXT_SESSION_GENERATION = 1u;
#endif
static CLR_THREAD_LOCAL clr_search_profile_session *THREAD_SESSION;
static CLR_THREAD_LOCAL clr_search_thread_profile *THREAD_PROFILE;
static CLR_THREAD_LOCAL uint64_t THREAD_SESSION_GENERATION;

static void lock_thread_list(clr_search_profile_session *session) {
#if defined(_WIN32)
    AcquireSRWLockExclusive(&session->thread_list_lock);
#else
    while (atomic_flag_test_and_set_explicit(
        &session->thread_list_lock, memory_order_acquire)) {
    }
#endif
}

static void unlock_thread_list(clr_search_profile_session *session) {
#if defined(_WIN32)
    ReleaseSRWLockExclusive(&session->thread_list_lock);
#else
    atomic_flag_clear_explicit(
        &session->thread_list_lock, memory_order_release);
#endif
}

static clr_search_profile_session *active_session_load(void) {
#if defined(_WIN32)
    return (clr_search_profile_session *)InterlockedCompareExchangePointer(
        &ACTIVE_SESSION, 0, 0);
#else
    return atomic_load_explicit(&ACTIVE_SESSION, memory_order_acquire);
#endif
}

static bool active_session_install(clr_search_profile_session *session) {
#if defined(_WIN32)
    return InterlockedCompareExchangePointer(
               &ACTIVE_SESSION, session, 0) == 0;
#else
    clr_search_profile_session *expected = 0;
    return atomic_compare_exchange_strong_explicit(
        &ACTIVE_SESSION,
        &expected,
        session,
        memory_order_release,
        memory_order_relaxed);
#endif
}

static bool active_session_remove(clr_search_profile_session *session) {
#if defined(_WIN32)
    return InterlockedCompareExchangePointer(
               &ACTIVE_SESSION, 0, session) == session;
#else
    clr_search_profile_session *expected = session;
    return atomic_compare_exchange_strong_explicit(
        &ACTIVE_SESSION,
        &expected,
        0,
        memory_order_acq_rel,
        memory_order_acquire);
#endif
}

static uint64_t next_session_generation(void) {
#if defined(_WIN32)
    return (uint64_t)InterlockedIncrement64(&NEXT_SESSION_GENERATION);
#else
    return atomic_fetch_add_explicit(
        &NEXT_SESSION_GENERATION, 1u, memory_order_relaxed);
#endif
}

static clr_search_thread_profile *current_thread_profile(void) {
    clr_search_profile_session *session = active_session_load();
    if (session == 0) {
        return 0;
    }
    if (THREAD_SESSION == session &&
        THREAD_SESSION_GENERATION == session->generation) {
        return THREAD_PROFILE;
    }

    clr_search_thread_profile *profile =
        (clr_search_thread_profile *)malloc(sizeof(*profile));
    if (profile == 0) {
        return 0;
    }
    memset(profile, 0, sizeof(*profile));

    lock_thread_list(session);
    if (active_session_load() != session) {
        unlock_thread_list(session);
        free(profile);
        return 0;
    }
    profile->next = session->threads;
    session->threads = profile;
    unlock_thread_list(session);

    THREAD_SESSION = session;
    THREAD_SESSION_GENERATION = session->generation;
    THREAD_PROFILE = profile;
    return profile;
}

static uint64_t monotonic_nanoseconds(void) {
#if defined(_WIN32)
    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);
    uint64_t quotient =
        (uint64_t)(counter.QuadPart / QPC_FREQUENCY.QuadPart);
    uint64_t remainder =
        (uint64_t)(counter.QuadPart % QPC_FREQUENCY.QuadPart);
    return quotient * UINT64_C(1000000000) +
           remainder * UINT64_C(1000000000) /
               (uint64_t)QPC_FREQUENCY.QuadPart;
#else
    struct timespec value;
    timespec_get(&value, TIME_UTC);
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) +
           (uint64_t)value.tv_nsec;
#endif
}

void clr_search_stage_profile_init(clr_search_stage_profile *profile) {
    if (profile != 0) {
        memset(profile, 0, sizeof(*profile));
        profile->enabled = 1u;
    }
}

bool clr_search_stage_profile_activate(clr_search_stage_profile *profile) {
    if (profile == 0 || profile->enabled == 0u) {
        return false;
    }
#if defined(_WIN32)
    if (!InitOnceExecuteOnce(
            &QPC_INIT_ONCE, initialize_qpc_frequency, 0, 0)) {
        return false;
    }
#endif
    clr_search_profile_session *session =
        (clr_search_profile_session *)malloc(sizeof(*session));
    if (session == 0) {
        return false;
    }
    session->owner = profile;
    session->threads = 0;
#if defined(_WIN32)
    InitializeSRWLock(&session->thread_list_lock);
#else
    atomic_flag_clear_explicit(
        &session->thread_list_lock, memory_order_relaxed);
#endif
    session->generation = next_session_generation();
    if (!active_session_install(session)) {
        free(session);
        return false;
    }
    return true;
}

static void aggregate_thread_profile(
    clr_search_stage_profile *target,
    const clr_search_thread_profile *source) {
    for (size_t stage = 0u; stage < CLR_PROFILE_STAGE_COUNT; ++stage) {
        target->duration_ns[stage] += source->duration_ns[stage];
        target->invocation_count[stage] += source->invocation_count[stage];
        target->work_item_count[stage] += source->work_item_count[stage];
    }
    for (size_t depth = 0u;
         depth < CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH;
         ++depth) {
        target->packing_depth_expand_ns[depth] +=
            source->packing_depth_expand_ns[depth];
        target->packing_depth_reduce_ns[depth] +=
            source->packing_depth_reduce_ns[depth];
        target->packing_depth_emit_ns[depth] +=
            source->packing_depth_emit_ns[depth];
        target->packing_depth_frontier_in[depth] +=
            source->packing_depth_frontier_in[depth];
        target->packing_depth_frontier_out[depth] +=
            source->packing_depth_frontier_out[depth];
        target->packing_depth_candidate_count[depth] +=
            source->packing_depth_candidate_count[depth];
        target->packing_depth_incomplete[depth] |=
            source->packing_depth_incomplete[depth];
    }
}

void clr_search_stage_profile_deactivate(clr_search_stage_profile *profile) {
    clr_search_profile_session *session = active_session_load();
    if (session == 0 || session->owner != profile) {
        return;
    }
    if (!active_session_remove(session)) {
        return;
    }

    clr_search_thread_profile *thread = session->threads;
    while (thread != 0) {
        clr_search_thread_profile *next = thread->next;
        aggregate_thread_profile(profile, thread);
        free(thread);
        thread = next;
    }
    if (THREAD_SESSION == session) {
        THREAD_SESSION = 0;
        THREAD_PROFILE = 0;
        THREAD_SESSION_GENERATION = 0u;
    }
    free(session);
}

clr_search_profile_span clr_search_profile_begin(clr_search_profile_stage stage) {
    clr_search_profile_span span = {0};
    if (stage >= CLR_PROFILE_STAGE_COUNT || current_thread_profile() == 0) {
        return span;
    }
    span.started_ns = monotonic_nanoseconds();
    span.stage = (uint16_t)stage;
    span.active = 1u;
    return span;
}

uint64_t clr_search_profile_end(
    clr_search_profile_span span,
    uint64_t work_items) {
    clr_search_thread_profile *profile = current_thread_profile();
    if (profile == 0 || span.active == 0u ||
        span.stage >= CLR_PROFILE_STAGE_COUNT) {
        return 0u;
    }
    uint64_t finished_ns = monotonic_nanoseconds();
    uint64_t elapsed_ns = finished_ns >= span.started_ns
                              ? finished_ns - span.started_ns
                              : 0u;
    profile->duration_ns[span.stage] += elapsed_ns;
    profile->invocation_count[span.stage]++;
    profile->work_item_count[span.stage] += work_items;
    return elapsed_ns;
}

void clr_search_profile_count(
    clr_search_profile_stage stage,
    uint64_t work_items) {
    clr_search_thread_profile *profile = current_thread_profile();
    if (profile == 0 || stage >= CLR_PROFILE_STAGE_COUNT) {
        return;
    }
    profile->invocation_count[stage]++;
    profile->work_item_count[stage] += work_items;
}

void clr_search_profile_observe_packing_depth(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns,
    uint64_t reduce_ns) {
    clr_search_thread_profile *profile = current_thread_profile();
    if (profile == 0 || depth >= CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH) {
        return;
    }
    profile->packing_depth_frontier_in[depth] += frontier_in;
    profile->packing_depth_frontier_out[depth] += frontier_out;
    profile->packing_depth_expand_ns[depth] += expand_ns;
    profile->packing_depth_reduce_ns[depth] += reduce_ns;
}

void clr_search_profile_observe_packing_depth_incomplete(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns) {
    clr_search_thread_profile *profile = current_thread_profile();
    if (profile == 0 || depth >= CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH) {
        return;
    }
    profile->packing_depth_frontier_in[depth] += frontier_in;
    profile->packing_depth_frontier_out[depth] += frontier_out;
    profile->packing_depth_expand_ns[depth] += expand_ns;
    profile->packing_depth_incomplete[depth] = 1u;
}

void clr_search_profile_observe_packing_emit(
    uint8_t depth,
    uint64_t candidate_count,
    uint64_t emit_ns) {
    clr_search_thread_profile *profile = current_thread_profile();
    if (profile == 0 || depth >= CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH) {
        return;
    }
    profile->packing_depth_candidate_count[depth] += candidate_count;
    profile->packing_depth_emit_ns[depth] += emit_ns;
}
#endif

const char *clr_search_profile_stage_name(clr_search_profile_stage stage) {
    static const char *names[CLR_PROFILE_STAGE_COUNT] = {
        "supply.multiset_family",
        "packing.total",
        "packing.validate_and_lower",
        "packing.context_allocate",
        "packing.output_clear",
        "packing.operation_tables",
        "packing.support_index_build",
        "packing.pull_cell_select",
        "packing.depth_expand",
        "packing.frontier_bucket_index_clear",
        "packing.frontier_exact_reduce",
        "packing.frontier_grow",
        "packing.frontier_swap",
        "packing.depth_emit",
        "packing.candidate_canonicalize",
        "packing.candidate_dedupe",
        "packing.piece_domain_skips",
        "packing.pull_support_candidates",
        "packing.static_prune_calls",
        "packing.static_prune_rejects",
        "packing.multiset_prefix_calls",
        "packing.multiset_prefix_rejects",
        "packing.child_appends",
        "packing.context_release",
        "buildup.total",
        "buildup.exists",
        "buildup.validate",
        "buildup.queue_hold_init",
        "buildup.search",
        "buildup.memo_lookup",
        "buildup.hold_branch_enumeration",
        "buildup.operation_variant_cache_lookups",
        "buildup.operation_variant_cache_hits",
        "buildup.operation_variant_generation",
        "buildup.geometry_transition_cache_lookups",
        "buildup.geometry_transition_cache_hits",
        "buildup.y_adjustment",
        "buildup.line_dependency",
        "buildup.grounded",
        "buildup.reachability_cache_lookups",
        "buildup.reachability_cache_hits",
        "buildup.reachability",
        "buildup.place_and_clear",
        "buildup.line_state_update",
        "buildup.memo_insert",
        "buildup.realization_feasibility",
        "buildup.realization_feasible",
        "buildup.realization_infeasible",
        "buildup.realization_unknown",
        "packing.geometry_residual_memo_lookups",
        "packing.geometry_residual_memo_hits",
        "packing.geometry_component_compositions",
        "buildup.clear_state_skips",
    };
    return stage < CLR_PROFILE_STAGE_COUNT ? names[stage] : "unknown";
}

clr_search_stage_profile *clr_search_stage_profile_create(void) {
    clr_search_stage_profile *profile =
        (clr_search_stage_profile *)malloc(sizeof(clr_search_stage_profile));
    if (profile != 0) {
        clr_search_stage_profile_init(profile);
    }
    return profile;
}

void clr_search_stage_profile_release(clr_search_stage_profile *profile) {
    if (profile == 0) {
        return;
    }
    clr_search_stage_profile_deactivate(profile);
    free(profile);
}

bool clr_search_stage_profile_start(clr_search_stage_profile *profile) {
    return clr_search_stage_profile_activate(profile);
}

void clr_search_stage_profile_stop(clr_search_stage_profile *profile) {
    clr_search_stage_profile_deactivate(profile);
}

size_t clr_search_stage_profile_stage_count(void) {
    return CLR_PROFILE_STAGE_COUNT;
}

uint64_t clr_search_stage_profile_duration_ns(
    const clr_search_stage_profile *profile,
    size_t stage) {
    return profile != 0 && stage < CLR_PROFILE_STAGE_COUNT
               ? profile->duration_ns[stage]
               : 0u;
}

uint64_t clr_search_stage_profile_invocation_count(
    const clr_search_stage_profile *profile,
    size_t stage) {
    return profile != 0 && stage < CLR_PROFILE_STAGE_COUNT
               ? profile->invocation_count[stage]
               : 0u;
}

uint64_t clr_search_stage_profile_work_item_count(
    const clr_search_stage_profile *profile,
    size_t stage) {
    return profile != 0 && stage < CLR_PROFILE_STAGE_COUNT
               ? profile->work_item_count[stage]
               : 0u;
}
