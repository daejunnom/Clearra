#include "../src/packing/packing_problem.h"
#include "../src/candidate/candidate.h"
#include "../src/supply/standard_bag_automaton.h"
#include "clr_build_variant.h"
#include "clr_problem.h"
#include "clr_rules.h"
#include "clr_search_profile.h"

#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <string.h>
#include <time.h>

#define EXTERNAL_PC_MAX_SOLUTIONS 512u

static bool debug_solution_output_requested(void) {
#if defined(_MSC_VER)
    char *value = 0;
    size_t value_length = 0u;
    if (_dupenv_s(
            &value,
            &value_length,
            "CLEARRA_DEBUG_EXTERNAL_PC_SOLUTIONS") != 0) {
        return false;
    }
    bool requested = value != 0 && value_length > 1u;
    free(value);
    return requested;
#else
    return getenv("CLEARRA_DEBUG_EXTERNAL_PC_SOLUTIONS") != 0;
#endif
}

typedef struct NormalizedPlacement {
    uint8_t piece;
    uint64_t mask;
} NormalizedPlacement;

typedef struct NormalizedSolution {
    uint8_t count;
    NormalizedPlacement placements[CLEARRA_PACKING_MAX_PIECES];
} NormalizedSolution;

typedef struct NormalizedSolutionSet {
    uint16_t count;
    NormalizedSolution solutions[EXTERNAL_PC_MAX_SOLUTIONS];
} NormalizedSolutionSet;

typedef struct ExternalPcPerformanceMetrics {
    clock_t packing_ticks;
    clock_t buildup_ticks;
    uint32_t multiset_group_count;
    uint64_t packing_candidate_count;
    size_t peak_frontier_states;
    size_t peak_cpu_bytes;
} ExternalPcPerformanceMetrics;

typedef struct ExternalPcMultisetFamily {
    uint16_t count;
    uint8_t counts[CLR_PIECE_MULTISET_FAMILY_CAPACITY][CLR_PIECE_L + 1u];
} ExternalPcMultisetFamily;

typedef struct ExternalPcFixture {
    const char *name;
    uint64_t initial_mask;
    uint8_t height;
    uint8_t placed_count;
    uint8_t initial_hold_piece;
    uint16_t expected_solution_count;
    bool enforce_expected_solution_count;
    const char *const *source_labels;
    uint16_t source_label_count;
    bool mirror_source_labels;
} ExternalPcFixture;

static const ExternalPcFixture PCO = {
    "pco-i-hold-6p", UINT64_C(0x000000e0f87e3f87), 4u, 4u, CLR_PIECE_I, 63u,
    true, 0, 0u, false};
static const ExternalPcFixture PCO_MIRROR = {
    "pco-i-hold-6p-mirror", UINT64_C(0x000000c1f87f1f87), 4u, 4u,
    CLR_PIECE_I, 63u, true, 0, 0u, false};

static const char *const TSAR_SOURCE_LABELS[] = {
    "TSOJIL", "TOZLJI", "TOZJIL", "TOLSJI", "TSZOLJ", "TZJLSI",
    "TZJOSL", "TZJSLI", "TZJLSI", "TIOJLS", "TSJLZI", "TZILJS",
    "TZLSJI", "TZJLOS", "TSILJZ", "TOJSLZ", "TZOSLJ", "TIJLSO",
    "TLIJOZ", "TJOLSZ", "SZJOLI", "TOZJIL", "LZOJST", "TOLIJZ",
    "SZTOJL", "JOTSIL", "JOZTIL", "OZJLTI", "ZOJSLT", "ZLOJSI",
    "IOZSLJ", "JOZSTL", "TZSLJI", "OZSLJT", "OZSLIJ", "SOZJLT",
    "SOZJIL", "ZSIOLJ", "JSLZTI", "TZIOSJ", "TZIOSL", "SZTOJL"};

static const ExternalPcFixture TSAR = {
    "tsar-cannon-42", UINT64_C(0x000300c0399e3fdf), 5u, 6u,
    CLR_PIECE_NONE, 42u, true, TSAR_SOURCE_LABELS,
    (uint16_t)(sizeof(TSAR_SOURCE_LABELS) / sizeof(TSAR_SOURCE_LABELS[0])),
    false};
static const ExternalPcFixture TSAR_MIRROR = {
    "tsar-cannon-42-mirror", UINT64_C(0x00000300e67f1fef), 5u, 6u,
    CLR_PIECE_NONE, 42u, true, TSAR_SOURCE_LABELS,
    (uint16_t)(sizeof(TSAR_SOURCE_LABELS) / sizeof(TSAR_SOURCE_LABELS[0])),
    true};

static ClearraPackingCandidateBuffer PACKING_BUFFER;
static NormalizedSolutionSet PCO_SOLUTIONS;
static NormalizedSolutionSet PCO_MIRROR_SOLUTIONS;
static NormalizedSolutionSet TSAR_SOLUTIONS;
static NormalizedSolutionSet TSAR_MIRROR_SOLUTIONS;
static ExternalPcPerformanceMetrics PERFORMANCE_METRICS;
static clr_search_stage_profile STAGE_PROFILE;
static bool STAGE_PROFILE_ACTIVE;

static uint8_t mirror_piece(uint8_t piece);
static void print_search_stage_profile(const ExternalPcFixture *fixture);

static void fail_status(const char *fixture, const char *stage, int status) {
    if (STAGE_PROFILE_ACTIVE) {
        clr_search_stage_profile_deactivate(&STAGE_PROFILE);
        STAGE_PROFILE_ACTIVE = false;
        const ExternalPcFixture profile_fixture = {
            .name = fixture,
        };
        print_search_stage_profile(&profile_fixture);
        fflush(stdout);
    }
    fprintf(stderr, "%s: %s failed with status %d\n", fixture, stage, status);
    exit(1);
}

static uint64_t low_mask(uint8_t bit_count) {
    return bit_count >= 64u ? UINT64_MAX
                            : (UINT64_C(1) << bit_count) - UINT64_C(1);
}

static clr_packing_problem packing_problem(
    const ExternalPcFixture *fixture,
    const uint8_t counts[CLR_PIECE_L + 1u]) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    problem.max_pieces = fixture->placed_count;
    if (clr_board_descriptor_init(
            10u,
            fixture->height,
            fixture->height,
            fixture->initial_mask,
            0u,
            &problem.board) != CLR_BOARD_OK) {
        fail_status(fixture->name, "board descriptor", CLR_BOARD_INVALID_LAYOUT);
    }
    problem.goal_region_mask = low_mask((uint8_t)(10u * fixture->height));
    problem.required_fill_mask =
        problem.goal_region_mask & ~fixture->initial_mask;
    problem.exact_pieces = fixture->placed_count;
    problem.piece_window.max_pieces = fixture->placed_count;
    problem.piece_window.exact_pieces = fixture->placed_count;
    problem.piece_window.has_exact_pieces = 1u;
    problem.piece_multiset_window.total_count = fixture->placed_count;
    problem.piece_multiset_window.exact_count = fixture->placed_count;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        problem.piece_multiset_window.counts[piece] = counts[piece];
    }
    problem.piece_source = clearra_piece_source_descriptor_bag_universe(
        UINT64_C(0x4350524f),
        UINT32_C(0x53424731),
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    problem.piece_source.pattern_universe_id = UINT64_C(0x5041545445524e31);
    problem.piece_source.pattern_weight_model_id = UINT64_C(0x5745494748543031);
    problem.piece_source.materialized_pattern_count = 5040u;
    problem.piece_source.exact_bag_automaton_supported = 1u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.budget.max_frontier_states = 5000000u;
    problem.budget.max_nodes = 5000000u;
    problem.budget.max_results = CLEARRA_PACKING_MAX_CANDIDATES;
    problem.budget.max_patterns = 5040u;
    problem.flags = CLR_BUILDUP_FLAG_HOLD_ENABLED;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.backend.workers = 1u;
    problem.backend.deterministic = 1u;
    problem.backend.fallback_policy = CLR_BACKEND_FALLBACK_DENY;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_UNIQUE;
    problem.objective = CLR_OBJECTIVE_UNIQUE;
    return problem;
}

static int compare_placement(
    const NormalizedPlacement *left,
    const NormalizedPlacement *right) {
    if (left->piece != right->piece) {
        return left->piece < right->piece ? -1 : 1;
    }
    if (left->mask == right->mask) {
        return 0;
    }
    return left->mask < right->mask ? -1 : 1;
}

static void normalize_candidate(
    const ClearraPackingCandidateView *candidate,
    NormalizedSolution *out_solution) {
    *out_solution = (NormalizedSolution){0};
    out_solution->count = candidate->placed_count;
    for (uint8_t index = 0; index < candidate->placed_count; ++index) {
        out_solution->placements[index].piece = candidate->pieces[index];
        out_solution->placements[index].mask = candidate->operation_masks[index];
    }
    for (uint8_t index = 1u; index < out_solution->count; ++index) {
        NormalizedPlacement value = out_solution->placements[index];
        uint8_t cursor = index;
        while (cursor > 0u &&
               compare_placement(&value, &out_solution->placements[cursor - 1u]) < 0) {
            out_solution->placements[cursor] =
                out_solution->placements[cursor - 1u];
            --cursor;
        }
        out_solution->placements[cursor] = value;
    }
}

static bool normalized_solution_equal(
    const NormalizedSolution *left,
    const NormalizedSolution *right) {
    if (left->count != right->count) {
        return false;
    }
    for (uint8_t index = 0; index < left->count; ++index) {
        if (compare_placement(
                &left->placements[index], &right->placements[index]) != 0) {
            return false;
        }
    }
    return true;
}

static void solution_set_insert(
    const ExternalPcFixture *fixture,
    NormalizedSolutionSet *set,
    const ClearraPackingCandidateView *candidate) {
    NormalizedSolution normalized;
    (void)fixture;
    normalize_candidate(candidate, &normalized);
    for (uint16_t index = 0; index < set->count; ++index) {
        if (normalized_solution_equal(&set->solutions[index], &normalized)) {
            return;
        }
    }
    if (set->count >= EXTERNAL_PC_MAX_SOLUTIONS) {
        fail_status(fixture->name, "normalized solution capacity", set->count);
    }
    set->solutions[set->count++] = normalized;
}

static void verify_candidates_for_family(
    const ExternalPcFixture *fixture,
    const ExternalPcMultisetFamily *family,
    NormalizedSolutionSet *out_solutions) {
    uint8_t envelope[CLR_PIECE_L + 1u] = {0};
    for (uint16_t index = 0u; index < family->count; ++index) {
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            if (envelope[piece] < family->counts[index][piece]) {
                envelope[piece] = family->counts[index][piece];
            }
        }
    }
    clr_packing_problem packing = packing_problem(fixture, envelope);
    packing.piece_multiset_window.total_count = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        packing.piece_multiset_window.total_count =
            (uint8_t)(packing.piece_multiset_window.total_count + envelope[piece]);
    }
    packing.piece_multiset_window.exact_count = fixture->placed_count;
    packing.piece_multiset_family.count = family->count;
    packing.piece_multiset_family.complete = 1u;
    for (uint16_t index = 0u; index < family->count; ++index) {
        clr_piece_multiset_window *member =
            &packing.piece_multiset_family.members[index];
        member->total_count = fixture->placed_count;
        member->exact_count = fixture->placed_count;
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            member->counts[piece] = family->counts[index][piece];
        }
    }
    clr_resource_report resource_report;
    clock_t packing_started = clock();
    ClearraPackingStatus packing_status =
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
            &packing, &PACKING_BUFFER, &resource_report);
    PERFORMANCE_METRICS.packing_ticks += clock() - packing_started;
    PERFORMANCE_METRICS.multiset_group_count = family->count;
    PERFORMANCE_METRICS.packing_candidate_count += PACKING_BUFFER.count;
    if (PERFORMANCE_METRICS.peak_frontier_states <
        resource_report.peak_frontier_states) {
        PERFORMANCE_METRICS.peak_frontier_states =
            resource_report.peak_frontier_states;
    }
    if (PERFORMANCE_METRICS.peak_cpu_bytes < resource_report.peak_cpu_bytes) {
        PERFORMANCE_METRICS.peak_cpu_bytes = resource_report.peak_cpu_bytes;
    }
    if (packing_status != CLEARRA_PACKING_OK) {
        fprintf(stderr,
                "%s: packing resource reason=%u frontier=%zu candidates=%zu hashes=%zu\n",
                fixture->name,
                resource_report.truncation_reason,
                resource_report.peak_frontier_states,
                resource_report.peak_candidate_rows,
                resource_report.peak_hash_buckets);
        fail_status(fixture->name, "packing", packing_status);
    }

    clock_t buildup_started = clock();
    for (uint16_t index = 0; index < PACKING_BUFFER.count; ++index) {
        ClearraPackingCandidateView candidate;
        if (clearra_packing_candidate_buffer_candidate_at(
                &PACKING_BUFFER, index, &candidate) != CLEARRA_PACKING_OK) {
            fail_status(fixture->name, "candidate read", index);
        }
        clr_buildup_problem buildup;
        if (clearra_buildup_problem_from_packing_candidate(
                &packing, &candidate, 0u, &buildup) != CLEARRA_PACKING_OK) {
            fail_status(fixture->name, "buildup lowering", index);
        }
        buildup.source_execution_mode = CLR_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON;
        buildup.initial_hold_automaton.piece_source_id =
            packing.piece_source.piece_source_id;
        buildup.initial_hold_automaton.cursor = 0u;
        buildup.initial_hold_automaton.bag_epoch = 0u;
        buildup.initial_hold_automaton.bag_remainder_key =
            clearra_standard_bag_full_remainder_key();
        buildup.initial_hold_automaton.provenance_id =
            packing.piece_source.provenance_id;
        buildup.initial_hold_automaton.hold_piece = fixture->initial_hold_piece;
        buildup.initial_hold_automaton.hold_empty =
            fixture->initial_hold_piece == CLR_PIECE_NONE ? 1u : 0u;
        buildup.buildup_flags = CLR_BUILDUP_FLAG_HOLD_ENABLED;

        clr_buildup_verification verification;
        clr_buildup_status buildup_status =
            clr_buildup_worker_verify(&buildup, &verification);
        if (buildup_status == CLR_BUILDUP_OK && verification.accepted != 0u) {
            solution_set_insert(fixture, out_solutions, &candidate);
        } else if (buildup_status < CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE ||
                   buildup_status > CLR_BUILDUP_COLLISION) {
            fail_status(fixture->name, "buildup", buildup_status);
        }
    }
    PERFORMANCE_METRICS.buildup_ticks += clock() - buildup_started;
}

static void multiset_family_add(
    const ExternalPcFixture *fixture,
    ExternalPcMultisetFamily *family,
    const uint8_t counts[CLR_PIECE_L + 1u]) {
    for (uint16_t index = 0u; index < family->count; ++index) {
        if (memcmp(family->counts[index], counts, CLR_PIECE_L + 1u) == 0) {
            return;
        }
    }
    if (family->count >= CLR_PIECE_MULTISET_FAMILY_CAPACITY) {
        fail_status(fixture->name, "multiset family capacity", family->count);
    }
    memcpy(family->counts[family->count++], counts, CLR_PIECE_L + 1u);
}

static void enumerate_multisets(
    const ExternalPcFixture *fixture,
    uint8_t piece,
    uint8_t remaining,
    uint8_t counts[CLR_PIECE_L + 1u],
    ExternalPcMultisetFamily *family) {
    if (piece > CLR_PIECE_L) {
        if (remaining == 0u) {
            multiset_family_add(fixture, family, counts);
        }
        return;
    }

    uint8_t max_count = piece == CLR_PIECE_I &&
                                fixture->initial_hold_piece == CLR_PIECE_I
                            ? 2u
                            : 1u;
    if (max_count > remaining) {
        max_count = remaining;
    }
    for (uint8_t count = 0u; count <= max_count; ++count) {
        counts[piece] = count;
        enumerate_multisets(
            fixture,
            (uint8_t)(piece + 1u),
            (uint8_t)(remaining - count),
            counts,
            family);
    }
    counts[piece] = 0u;
}

static uint8_t piece_from_label_char(char label) {
    switch (label) {
    case 'I':
        return CLR_PIECE_I;
    case 'O':
        return CLR_PIECE_O;
    case 'T':
        return CLR_PIECE_T;
    case 'S':
        return CLR_PIECE_S;
    case 'Z':
        return CLR_PIECE_Z;
    case 'J':
        return CLR_PIECE_J;
    case 'L':
        return CLR_PIECE_L;
    default:
        return CLR_PIECE_NONE;
    }
}

static void source_label_multiset_family(
    const ExternalPcFixture *fixture,
    ExternalPcMultisetFamily *family) {
    for (uint16_t label_index = 0u; label_index < fixture->source_label_count;
         ++label_index) {
        uint8_t counts[CLR_PIECE_L + 1u] = {0};
        const char *label = fixture->source_labels[label_index];
        for (uint8_t index = 0u; label[index] != '\0'; ++index) {
            uint8_t piece = piece_from_label_char(label[index]);
            if (piece == CLR_PIECE_NONE) {
                fail_status(fixture->name, "source label piece", label[index]);
            }
            if (fixture->mirror_source_labels) {
                piece = mirror_piece(piece);
            }
            ++counts[piece];
        }
        multiset_family_add(fixture, family, counts);
    }
}

static void print_search_stage_profile(const ExternalPcFixture *fixture) {
    uint64_t timed_invocations = 0u;
    puts("profile stages are nested; stage durations are not additive");
    for (uint16_t stage = 0u; stage < CLR_PROFILE_STAGE_COUNT; ++stage) {
        uint64_t invocations = STAGE_PROFILE.invocation_count[stage];
        uint64_t work_items = STAGE_PROFILE.work_item_count[stage];
        uint64_t duration_ns = STAGE_PROFILE.duration_ns[stage];
        if (invocations == 0u && work_items == 0u) {
            continue;
        }
        timed_invocations += duration_ns == 0u ? 0u : invocations;
        printf(
            "%s stage %-38s seconds=%9.6f calls=%llu work=%llu\n",
            fixture->name,
            clr_search_profile_stage_name((clr_search_profile_stage)stage),
            (double)duration_ns / 1000000000.0,
            (unsigned long long)invocations,
            (unsigned long long)work_items);
    }
    for (uint8_t depth = 0u; depth < CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH; ++depth) {
        if (STAGE_PROFILE.packing_depth_frontier_in[depth] == 0u &&
            STAGE_PROFILE.packing_depth_frontier_out[depth] == 0u &&
            STAGE_PROFILE.packing_depth_emit_ns[depth] == 0u) {
            continue;
        }
        printf(
            "%s depth=%u frontier_in=%llu frontier_out=%llu expand_s=%.6f "
            "reduce_s=%.6f emit_s=%.6f candidates=%llu incomplete=%s\n",
            fixture->name,
            (unsigned)depth,
            (unsigned long long)STAGE_PROFILE.packing_depth_frontier_in[depth],
            (unsigned long long)STAGE_PROFILE.packing_depth_frontier_out[depth],
            (double)STAGE_PROFILE.packing_depth_expand_ns[depth] / 1000000000.0,
            (double)STAGE_PROFILE.packing_depth_reduce_ns[depth] / 1000000000.0,
            (double)STAGE_PROFILE.packing_depth_emit_ns[depth] / 1000000000.0,
            (unsigned long long)STAGE_PROFILE.packing_depth_candidate_count[depth],
            STAGE_PROFILE.packing_depth_incomplete[depth] != 0u ? "true" : "false");
    }
    printf(
        "%s profiler_clock_pairs=%llu (diagnostic overhead is included in wall time)\n",
        fixture->name,
        (unsigned long long)timed_invocations);
}

static void solve_fixture(
    const ExternalPcFixture *fixture,
    NormalizedSolutionSet *out_solutions) {
    memset(out_solutions, 0, sizeof(*out_solutions));
    PERFORMANCE_METRICS = (ExternalPcPerformanceMetrics){0};
    bool profiling = getenv("CLEARRA_PROFILE_EXTERNAL_PC") != 0;
    if (profiling) {
        clr_search_stage_profile_init(&STAGE_PROFILE);
        if (!clr_search_stage_profile_activate(&STAGE_PROFILE)) {
            fail_status(fixture->name, "stage profiler activation", 1);
        }
        STAGE_PROFILE_ACTIVE = true;
    }
    clr_search_profile_span supply_span =
        clr_search_profile_begin(CLR_PROFILE_SUPPLY_MULTISET_FAMILY);
    ExternalPcMultisetFamily family = {0};
    if (fixture->source_labels != 0 && fixture->source_label_count > 0u) {
        source_label_multiset_family(fixture, &family);
    } else {
        uint8_t counts[CLR_PIECE_L + 1u] = {0};
        enumerate_multisets(
            fixture,
            CLR_PIECE_I,
            fixture->placed_count,
            counts,
            &family);
    }
    (void)clr_search_profile_end(supply_span, family.count);
    verify_candidates_for_family(fixture, &family, out_solutions);
    if (profiling) {
        clr_search_stage_profile_deactivate(&STAGE_PROFILE);
        STAGE_PROFILE_ACTIVE = false;
        print_search_stage_profile(fixture);
    }
    printf("%s normalized buildable solutions: %u\n",
           fixture->name,
           (unsigned)out_solutions->count);
    printf(
        "%s profile: multisets=%u packing_candidates=%llu peak_frontier=%zu "
        "peak_cpu_bytes=%zu packing_cpu_s=%.3f buildup_cpu_s=%.3f\n",
        fixture->name,
        (unsigned)PERFORMANCE_METRICS.multiset_group_count,
        (unsigned long long)PERFORMANCE_METRICS.packing_candidate_count,
        PERFORMANCE_METRICS.peak_frontier_states,
        PERFORMANCE_METRICS.peak_cpu_bytes,
        (double)PERFORMANCE_METRICS.packing_ticks / (double)CLOCKS_PER_SEC,
        (double)PERFORMANCE_METRICS.buildup_ticks / (double)CLOCKS_PER_SEC);
    fflush(stdout);
    if (debug_solution_output_requested()) {
        for (uint16_t index = 0u; index < out_solutions->count; ++index) {
            printf("%s solution %03u ", fixture->name, (unsigned)(index + 1u));
            for (uint8_t placement = 0u;
                 placement < out_solutions->solutions[index].count;
                 ++placement) {
                printf("%u:%016llx",
                       (unsigned)out_solutions->solutions[index]
                           .placements[placement]
                           .piece,
                       (unsigned long long)out_solutions->solutions[index]
                           .placements[placement]
                           .mask);
                if (placement + 1u < out_solutions->solutions[index].count) {
                    putchar(',');
                }
            }
            putchar('\n');
        }
    }
    if (fixture->enforce_expected_solution_count &&
        out_solutions->count != fixture->expected_solution_count) {
        fprintf(stderr,
                "%s: expected %u normalized solutions but got %u\n",
                fixture->name,
                (unsigned)fixture->expected_solution_count,
                (unsigned)out_solutions->count);
        exit(1);
    }
}

static uint64_t mirror_mask(uint64_t mask, uint8_t height) {
    uint64_t mirrored = 0u;
    for (uint8_t y = 0u; y < height; ++y) {
        for (uint8_t x = 0u; x < 10u; ++x) {
            uint8_t source = (uint8_t)(y * 10u + x);
            if ((mask & (UINT64_C(1) << source)) != 0u) {
                uint8_t target = (uint8_t)(y * 10u + (9u - x));
                mirrored |= UINT64_C(1) << target;
            }
        }
    }
    return mirrored;
}

static uint8_t mirror_piece(uint8_t piece) {
    switch (piece) {
    case CLR_PIECE_S:
        return CLR_PIECE_Z;
    case CLR_PIECE_Z:
        return CLR_PIECE_S;
    case CLR_PIECE_J:
        return CLR_PIECE_L;
    case CLR_PIECE_L:
        return CLR_PIECE_J;
    default:
        return piece;
    }
}

static NormalizedSolution mirror_solution(
    const NormalizedSolution *solution,
    uint8_t height) {
    NormalizedSolution mirrored = *solution;
    for (uint8_t index = 0u; index < mirrored.count; ++index) {
        mirrored.placements[index].piece =
            mirror_piece(mirrored.placements[index].piece);
        mirrored.placements[index].mask =
            mirror_mask(mirrored.placements[index].mask, height);
    }
    for (uint8_t index = 1u; index < mirrored.count; ++index) {
        NormalizedPlacement value = mirrored.placements[index];
        uint8_t cursor = index;
        while (cursor > 0u &&
               compare_placement(&value, &mirrored.placements[cursor - 1u]) < 0) {
            mirrored.placements[cursor] = mirrored.placements[cursor - 1u];
            --cursor;
        }
        mirrored.placements[cursor] = value;
    }
    return mirrored;
}

static void assert_mirror_set(
    const ExternalPcFixture *original_fixture,
    const NormalizedSolutionSet *original,
    const ExternalPcFixture *mirror_fixture,
    const NormalizedSolutionSet *mirror) {
    if (original->count != mirror->count) {
        fail_status(mirror_fixture->name, "mirror count", mirror->count);
    }
    for (uint16_t index = 0u; index < original->count; ++index) {
        NormalizedSolution expected =
            mirror_solution(&original->solutions[index], original_fixture->height);
        bool found = false;
        for (uint16_t mirror_index = 0u; mirror_index < mirror->count; ++mirror_index) {
            if (normalized_solution_equal(&expected, &mirror->solutions[mirror_index])) {
                found = true;
                break;
            }
        }
        if (!found) {
            fprintf(stderr,
                    "%s: mirrored solution %u missing from %s\n",
                    original_fixture->name,
                    (unsigned)index,
                    mirror_fixture->name);
            exit(1);
        }
    }
}

int main(void) {
    const char *selected_case = getenv("CLEARRA_EXTERNAL_PC_CASE");
    bool run_pco = selected_case == 0 || strcmp(selected_case, "pco") == 0;
    bool run_tsar = selected_case == 0 || strcmp(selected_case, "tsar") == 0;

    if (run_pco) {
        solve_fixture(&PCO, &PCO_SOLUTIONS);
        solve_fixture(&PCO_MIRROR, &PCO_MIRROR_SOLUTIONS);
        assert_mirror_set(
            &PCO, &PCO_SOLUTIONS, &PCO_MIRROR, &PCO_MIRROR_SOLUTIONS);
    }

    if (run_tsar) {
        solve_fixture(&TSAR, &TSAR_SOLUTIONS);
        solve_fixture(&TSAR_MIRROR, &TSAR_MIRROR_SOLUTIONS);
        assert_mirror_set(
            &TSAR, &TSAR_SOLUTIONS, &TSAR_MIRROR, &TSAR_MIRROR_SOLUTIONS);
    }

    if (!run_pco && !run_tsar) {
        fprintf(stderr, "unknown CLEARRA_EXTERNAL_PC_CASE: %s\n", selected_case);
        return 2;
    }
    puts("external PC solution tests passed");
    return 0;
}
