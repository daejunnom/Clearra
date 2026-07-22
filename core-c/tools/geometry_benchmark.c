#include "../src/packing/packing_problem.h"
#include "../src/supply/standard_bag_automaton.h"

#include "clr_build_variant.h"
#include "clr_problem.h"
#include "clr_rules.h"
#include "clr_search_profile.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef struct GeometryBenchmarkCollector {
    uint64_t candidate_count;
    uint64_t digest_xor;
    uint64_t digest_sum;
} GeometryBenchmarkCollector;

static uint64_t mix64(uint64_t value) {
    value = (value ^ (value >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27u)) * UINT64_C(0x94d049bb133111eb);
    return value ^ (value >> 31u);
}

static ClearraPackingStatus collect_candidate(
    void *context,
    const ClearraPackingCandidateView *candidate,
    size_t accepted_candidate_count,
    size_t engine_resident_bytes,
    size_t max_candidate_rows,
    size_t max_total_bytes,
    uint8_t *out_inserted,
    uint16_t *out_truncation_reason,
    size_t *out_host_resident_bytes) {
    GeometryBenchmarkCollector *collector =
        (GeometryBenchmarkCollector *)context;
    (void)engine_resident_bytes;
    (void)max_candidate_rows;
    (void)max_total_bytes;
    if (collector == 0 || candidate == 0 || out_inserted == 0 ||
        out_truncation_reason == 0 || out_host_resident_bytes == 0 ||
        accepted_candidate_count != collector->candidate_count) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    uint64_t digest = mix64(
        candidate->tiling_key ^
        (candidate->operation_set_key << 17u) ^
        (candidate->operation_set_key >> 47u));
    collector->candidate_count++;
    collector->digest_xor ^= digest;
    collector->digest_sum += digest;
    *out_inserted = 1u;
    *out_truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    *out_host_resident_bytes = sizeof(*collector);
    return CLEARRA_PACKING_OK;
}

static void add_family_member(
    clr_piece_multiset_family *family,
    const uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    if (family->count >= CLR_PIECE_MULTISET_FAMILY_CAPACITY) {
        return;
    }
    clr_piece_multiset_window *member = &family->members[family->count++];
    member->total_count = 10u;
    member->exact_count = 10u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        member->counts[piece] = counts[piece];
    }
}

static void enumerate_family(
    uint8_t piece,
    uint8_t remaining,
    uint8_t zero_count,
    uint8_t max_zero_count,
    uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT],
    clr_piece_multiset_family *family) {
    if (piece > CLR_PIECE_L) {
        if (remaining == 0u && zero_count <= max_zero_count) {
            add_family_member(family, counts);
        }
        return;
    }
    for (uint8_t count = 0u; count <= 2u && count <= remaining; ++count) {
        uint8_t next_zero_count = (uint8_t)(zero_count + (count == 0u));
        if (next_zero_count > max_zero_count) {
            continue;
        }
        counts[piece] = count;
        enumerate_family(
            (uint8_t)(piece + 1u),
            (uint8_t)(remaining - count),
            next_zero_count,
            max_zero_count,
            counts,
            family);
    }
    counts[piece] = 0u;
}

static clr_packing_problem empty_four_line_problem(bool include_hold_carry) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = 10u;
    if (clr_board_descriptor_init(
            10u, 4u, 4u, 0u, 0u, &problem.board) != CLR_BOARD_OK) {
        return clr_packing_problem_zero();
    }
    problem.goal_region_mask = (UINT64_C(1) << 40u) - UINT64_C(1);
    problem.required_fill_mask = problem.goal_region_mask;
    problem.exact_pieces = 10u;
    problem.piece_window = clearra_piece_window_descriptor(10u, 10u, true);
    problem.piece_multiset_window.total_count = 14u;
    problem.piece_multiset_window.exact_count = 10u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        problem.piece_multiset_window.counts[piece] = 2u;
    }
    uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT] = {0};
    enumerate_family(
        CLR_PIECE_I,
        10u,
        0u,
        include_hold_carry ? 1u : 0u,
        counts,
        &problem.piece_multiset_family);
    problem.piece_multiset_family.complete = 1u;
    problem.piece_source = clearra_piece_source_descriptor_bag_universe(
        UINT64_C(0x454d505459344c),
        UINT32_C(0x53424731),
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    problem.piece_source.exact_bag_automaton_supported = 1u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.backend.workers = 1u;
    problem.backend.deterministic = 1u;
    problem.backend.fallback_policy = CLR_BACKEND_FALLBACK_DENY;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_UNIQUE;
    problem.objective = CLR_OBJECTIVE_UNIQUE;
    return problem;
}

static bool parse_u32_argument(const char *text, uint32_t *out_value) {
    if (text == 0 || out_value == 0 || *text == '\0') {
        return false;
    }
    char *end = 0;
    unsigned long value = strtoul(text, &end, 10);
    if (*end != '\0' || value > UINT32_MAX) {
        return false;
    }
    *out_value = (uint32_t)value;
    return true;
}

static void print_stage_profile(const clr_search_stage_profile *profile) {
    for (size_t stage = 0u;
         stage < clr_search_stage_profile_stage_count();
         ++stage) {
        uint64_t invocations =
            clr_search_stage_profile_invocation_count(profile, stage);
        uint64_t duration_ns =
            clr_search_stage_profile_duration_ns(profile, stage);
        uint64_t work_items =
            clr_search_stage_profile_work_item_count(profile, stage);
        if (invocations == 0u && duration_ns == 0u && work_items == 0u) {
            continue;
        }
        printf(
            "profile stage=%s duration_s=%.9f invocations=%llu work=%llu\n",
            clr_search_profile_stage_name((clr_search_profile_stage)stage),
            (double)duration_ns / 1000000000.0,
            (unsigned long long)invocations,
            (unsigned long long)work_items);
    }
}

static int run_buildable_benchmark(
    const clr_packing_problem *problem,
    const ClearraGeometryCatalog *catalog,
    bool include_hold_carry,
    uint32_t requested_task_count,
    uint32_t selected_task_index,
    clock_t compile_started,
    clock_t compile_finished) {
    clr_resource_report graph_report;
    ClearraGeometrySolutionGraph *graph = 0;
    clock_t graph_started = clock();
    ClearraPackingStatus status = clearra_geometry_exact_cover_search_graph(
        catalog, problem, &graph, &graph_report);
    clock_t graph_finished = clock();
    if (status != CLEARRA_PACKING_OK || graph == 0) {
        fprintf(stderr, "geometry graph search failed status=%d\n", status);
        return 1;
    }

    if (requested_task_count > SIZE_MAX / sizeof(ClearraGeometrySolutionTask)) {
        fprintf(stderr, "geometry task allocation overflow\n");
        clearra_geometry_solution_graph_release(&graph);
        return 1;
    }
    ClearraGeometrySolutionTask *tasks =
        (ClearraGeometrySolutionTask *)malloc(
            (size_t)requested_task_count * sizeof(*tasks));
    if (tasks == 0) {
        fprintf(stderr, "geometry task allocation failed\n");
        clearra_geometry_solution_graph_release(&graph);
        return 1;
    }
    uint32_t task_count = 0u;
    size_t task_split_scratch_bytes = 0u;
    status = clearra_geometry_solution_graph_split_tasks(
        graph,
        tasks,
        requested_task_count,
        &task_count,
        &task_split_scratch_bytes);
    if (status != CLEARRA_PACKING_OK || task_count == 0u ||
        selected_task_index >= task_count) {
        fprintf(stderr, "geometry task split failed status=%d tasks=%u\n",
                status, (unsigned)task_count);
        free(tasks);
        clearra_geometry_solution_graph_release(&graph);
        return 1;
    }

    clr_buildup_problem buildup = clr_buildup_problem_from_packing(*problem);
    buildup.source_execution_mode = CLR_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON;
    buildup.initial_hold_automaton.bag_remainder_key =
        clearra_standard_bag_full_remainder_key();
    buildup.buildup_flags = CLR_BUILDUP_FLAG_HOLD_ENABLED;
    buildup.geometry_catalog = catalog;
    clr_buildup_workspace *workspace = clr_buildup_workspace_create();
    if (workspace == 0) {
        free(tasks);
        clearra_geometry_solution_graph_release(&graph);
        fprintf(stderr, "buildup workspace allocation failed\n");
        return 1;
    }

    GeometryBenchmarkCollector collector = {0};
    ClearraPackingCandidateSink sink = {&collector, collect_candidate};
    ClearraBuildableGeometryStreamReport stream_report;
    clr_pruning_proof_ledger pruning_ledger;
    clr_search_stage_profile stage_profile = {0};
    clr_search_stage_profile_init(&stage_profile);
    bool profile_active = clr_search_stage_profile_start(&stage_profile);
    clock_t buildup_started = clock();
    status = clearra_geometry_solution_graph_stream_buildable_task(
        graph,
        catalog,
        &tasks[selected_task_index],
        problem,
        &buildup,
        workspace,
        &sink,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        &pruning_ledger,
        &stream_report);
    clock_t buildup_finished = clock();
    if (profile_active) {
        clr_search_stage_profile_stop(&stage_profile);
    }
    size_t graph_bytes = clearra_geometry_solution_graph_resident_bytes(graph);
    size_t workspace_bytes = clr_buildup_workspace_retained_bytes(workspace);
    printf(
        "empty-4l mode=%s engine=geometry-buildorder-product "
        "family_count=%u task_index=%u task_count=%u raw_tilings=%llu "
        "buildable=%llu "
        "digest_xor=%016llx digest_sum=%016llx compile_s=%.6f "
        "graph_s=%.6f buildup_product_s=%.6f catalog_bytes=%zu "
        "graph_bytes=%zu workspace_bytes=%zu peak_cpu_bytes=%zu "
        "truncated=%u reason=%u\n",
        include_hold_carry ? "p7p4" : "p7p3",
        (unsigned)problem->piece_multiset_family.count,
        (unsigned)selected_task_index,
        (unsigned)task_count,
        (unsigned long long)stream_report.generated_count,
        (unsigned long long)collector.candidate_count,
        (unsigned long long)collector.digest_xor,
        (unsigned long long)collector.digest_sum,
        (double)(compile_finished - compile_started) / CLOCKS_PER_SEC,
        (double)(graph_finished - graph_started) / CLOCKS_PER_SEC,
        (double)(buildup_finished - buildup_started) / CLOCKS_PER_SEC,
        clearra_geometry_catalog_resident_bytes(catalog),
        graph_bytes,
        workspace_bytes,
        graph_report.peak_cpu_bytes,
        (unsigned)(status != CLEARRA_PACKING_OK || stream_report.complete == 0u),
        (unsigned)stream_report.truncation_reason);
    if (profile_active) {
        print_stage_profile(&stage_profile);
    }
    clr_buildup_workspace_release(workspace);
    free(tasks);
    clearra_geometry_solution_graph_release(&graph);
    return status == CLEARRA_PACKING_OK && stream_report.complete != 0u ? 0 : 1;
}

int geometry_benchmark_main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "p7p3";
    bool buildable = strcmp(mode, "p7p3-buildable") == 0 ||
                     strcmp(mode, "p7p4-buildable") == 0;
    bool include_hold_carry = strcmp(mode, "p7p4") == 0 ||
                              strcmp(mode, "p7p4-buildable") == 0;
    if (strcmp(mode, "p7p3") != 0 && strcmp(mode, "p7p4") != 0 &&
        !buildable) {
        fprintf(stderr,
                "usage: clearra_geometry_benchmark "
                "[p7p3|p7p4|p7p3-buildable|p7p4-buildable] "
                "[task-count] [task-index]\n");
        return 2;
    }
    if ((!buildable && argc > 2) || argc > 4) {
        fprintf(stderr,
                "usage: clearra_geometry_benchmark "
                "[p7p3|p7p4|p7p3-buildable|p7p4-buildable] "
                "[task-count] [task-index]\n");
        return 2;
    }
    uint32_t requested_task_count = 1u;
    uint32_t selected_task_index = 0u;
    if (buildable && argc > 2 &&
        (!parse_u32_argument(argv[2], &requested_task_count) ||
         requested_task_count == 0u)) {
        fprintf(stderr, "task-count must be a positive u32\n");
        return 2;
    }
    if (buildable && argc > 3 &&
        !parse_u32_argument(argv[3], &selected_task_index)) {
        fprintf(stderr, "task-index must be a u32\n");
        return 2;
    }
    clr_packing_problem problem = empty_four_line_problem(include_hold_carry);
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    ClearraGeometryCatalog *catalog = 0;
    clock_t compile_started = clock();
    ClearraPackingStatus status = clearra_geometry_catalog_compile(
        &problem,
        &compile_report,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        &ledger,
        &catalog);
    clock_t compile_finished = clock();
    if (status != CLEARRA_PACKING_OK || catalog == 0) {
        fprintf(stderr, "catalog compile failed status=%d\n", status);
        return 1;
    }
    if (buildable) {
        int result = run_buildable_benchmark(
            &problem,
            catalog,
            include_hold_carry,
            requested_task_count,
            selected_task_index,
            compile_started,
            compile_finished);
        clearra_geometry_catalog_release(&catalog);
        return result;
    }
    GeometryBenchmarkCollector collector = {0};
    ClearraPackingCandidateSink sink = {&collector, collect_candidate};
    clock_t search_started = clock();
    status = clearra_geometry_exact_cover_search_to_sink(
        catalog, &problem, 0u, 1u, 2u, &sink, &search_report);
    clock_t search_finished = clock();
    printf(
        "empty-4l mode=%s engine=%s family_count=%u candidates=%llu "
        "digest_xor=%016llx digest_sum=%016llx compile_s=%.6f search_s=%.6f "
        "catalog_bytes=%zu peak_cpu_bytes=%zu truncated=%u reason=%u\n",
        include_hold_carry ? "p7p4" : "p7p3",
        "geometry",
        (unsigned)problem.piece_multiset_family.count,
        (unsigned long long)collector.candidate_count,
        (unsigned long long)collector.digest_xor,
        (unsigned long long)collector.digest_sum,
        (double)(compile_finished - compile_started) / CLOCKS_PER_SEC,
        (double)(search_finished - search_started) / CLOCKS_PER_SEC,
        clearra_geometry_catalog_resident_bytes(catalog),
        search_report.peak_cpu_bytes,
        (unsigned)search_report.truncated,
        (unsigned)search_report.truncation_reason);
    clearra_geometry_catalog_release(&catalog);
    if (status != CLEARRA_PACKING_OK || search_report.truncated != 0u) {
        return 1;
    }
    return 0;
}
