#include "packing_tests_support.h"
#include "../src/buildup/realization_feasibility.h"
#include "../src/buildup/buildup_worker.h"
#include "../src/buildup/buildup_workspace.h"
#include "../src/invariant/geometry_additive_invariant.h"
#include "../src/packing/geometry_component_decomposition.h"
#include "../src/packing/geometry_catalog_internal.h"
#include "../src/packing/geometry_full_placement_domain.h"
#include "../src/packing/geometry_exact_cover_proof.h"
#include "../src/packing/geometry_residual_memo.h"
#include "../src/packing/geometry_solution_family.h"
#include "../src/packing/geometry_solution_graph_internal.h"

#include <string.h>

typedef struct CandidateCollector {
    ClearraPackingCandidateBuffer *buffer;
} CandidateCollector;

typedef struct GeometryPathCollector {
    uint32_t row_ids[CLEARRA_PACKING_MAX_PIECES];
    uint8_t operation_count;
    uint64_t path_count;
} GeometryPathCollector;

typedef struct GeometryBuildUpCollector {
    const ClearraGeometryCatalog *catalog;
    const clr_packing_problem *packing;
    clr_buildup_workspace *workspace;
    uint64_t verified_path_count;
} GeometryBuildUpCollector;

typedef struct RealizationFeasibilityCollector {
    const ClearraGeometryCatalog *catalog;
    const clr_packing_problem *packing;
    clr_buildup_workspace *workspace;
    ClearraRealizationFeasibilityWorkspace feasibility_workspace;
    uint64_t path_count;
    uint64_t direct_buildable_count;
    uint64_t false_infeasible_count;
} RealizationFeasibilityCollector;

static bool accept_all_geometry_rows(void *context, uint32_t row_id) {
    (void)context;
    (void)row_id;
    return true;
}

static ClearraPackingStatus collect_geometry_path(
    void *context,
    const ClearraGeometryPathView *path) {
    GeometryPathCollector *collector = (GeometryPathCollector *)context;
    if (collector == 0 || path == 0 || path->skeleton_row_ids == 0 ||
        path->operation_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    memcpy(
        collector->row_ids,
        path->skeleton_row_ids,
        (size_t)path->operation_count * sizeof(*collector->row_ids));
    collector->operation_count = path->operation_count;
    collector->path_count++;
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus verify_geometry_path_with_buildup(
    void *context,
    const ClearraGeometryPathView *path) {
    GeometryBuildUpCollector *collector = (GeometryBuildUpCollector *)context;
    if (collector == 0 || collector->catalog == 0 || collector->packing == 0 ||
        collector->workspace == 0 || path == 0 ||
        path->skeleton_row_ids == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    ClearraPackingCandidateView candidate;
    ClearraPackingStatus status = clearra_packing_materialize_catalog_row_ids(
        collector->catalog,
        collector->packing,
        path->skeleton_row_ids,
        path->operation_count,
        &candidate);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    candidate.candidate_id = collector->verified_path_count + UINT64_C(1);
    candidate.canonical_operation_set_id = candidate.candidate_id;

    clr_buildup_problem buildup;
    status = clearra_buildup_problem_from_packing_candidate(
        collector->packing, &candidate, 0u, &buildup);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    buildup.geometry_catalog = collector->catalog;
    if (clr_buildup_exists_with_workspace(&buildup, collector->workspace) !=
        CLR_BUILDUP_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    collector->verified_path_count++;
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus compare_realization_feasibility_with_buildup(
    void *context,
    const ClearraGeometryPathView *path) {
    RealizationFeasibilityCollector *collector =
        (RealizationFeasibilityCollector *)context;
    if (collector == 0 || collector->catalog == 0 ||
        collector->packing == 0 || collector->workspace == 0 || path == 0 ||
        path->skeleton_row_ids == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    collector->path_count++;

    ClearraPackingCandidateView candidate;
    ClearraPackingStatus status = clearra_packing_materialize_catalog_row_ids(
        collector->catalog,
        collector->packing,
        path->skeleton_row_ids,
        path->operation_count,
        &candidate);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    candidate.candidate_id = collector->path_count;
    candidate.canonical_operation_set_id = collector->path_count;
    clr_buildup_problem buildup;
    status = clearra_buildup_problem_from_packing_candidate(
        collector->packing, &candidate, 0u, &buildup);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    buildup.geometry_catalog = collector->catalog;

    ClearraRealizationFeasibilityResult feasibility;
    status = clearra_realization_feasibility_analyze(
        collector->catalog,
        collector->packing,
        path->skeleton_row_ids,
        path->operation_count,
        &collector->feasibility_workspace,
        0,
        &feasibility);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    clr_buildup_status buildup_status =
        clearra_buildup_exists_catalog_rows_with_constraints_and_workspace(
            &buildup,
            collector->catalog,
            path->skeleton_row_ids,
            path->operation_count,
            0,
            0,
            collector->workspace);
    if (buildup_status == CLR_BUILDUP_OK) {
        collector->direct_buildable_count++;
        if (feasibility.complete != 0u &&
            feasibility.kind == CLEARRA_REALIZATION_FEASIBILITY_INFEASIBLE) {
            collector->false_infeasible_count++;
        }
    }
    return CLEARRA_PACKING_OK;
}

static clr_packing_problem exact_cover_problem(
    ClearraBoard64Layout layout,
    const uint8_t *pieces,
    uint8_t piece_count) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = piece_count;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = layout.all_cells_mask;
    problem.required_fill_mask = layout.all_cells_mask;
    problem.exact_pieces = piece_count;
    problem.piece_window =
        clearra_piece_window_descriptor(piece_count, piece_count, true);
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, piece_count);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        UINT64_C(17),
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        piece_count,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    memcpy(problem.piece_source_pattern_pieces, pieces, piece_count);
    problem.piece_source_pattern_len = piece_count;
    problem.piece_source_pattern_complete = 1u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_NO_KICK;
    problem.rule.kick_profile_id = CLR_KICK_NO_KICK;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}

static ClearraGeometryCatalog *compile_catalog(
    const clr_packing_problem *problem,
    clr_resource_report *report,
    clr_pruning_proof_ledger *ledger) {
    ClearraGeometryCatalog *catalog = 0;
    EXPECT_STATUS(
        clearra_geometry_catalog_compile(
            problem,
            report,
            CLR_PRUNING_EVIDENCE_BEST_EFFORT,
            ledger,
            &catalog),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(catalog != 0);
    return catalog;
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
    CandidateCollector *collector = (CandidateCollector *)context;
    bool inserted = false;
    (void)accepted_candidate_count;
    (void)engine_resident_bytes;
    (void)max_candidate_rows;
    (void)max_total_bytes;
    ClearraPackingStatus status = clearra_packing_deduper_push_unique(
        collector->buffer, candidate, 0, &inserted);
    *out_inserted = inserted ? 1u : 0u;
    *out_truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    *out_host_resident_bytes = sizeof(*collector->buffer);
    return status;
}

static bool contains_candidate(
    const ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate) {
    for (uint16_t index = 0u; index < buffer->count; ++index) {
        if (clearra_packing_hash_confirm_exact(buffer, index, candidate)) {
            return true;
        }
    }
    return false;
}

void geometry_catalog_collapses_skeletons_without_losing_realizations(void) {
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    ClearraGeometryCatalog *catalog = compile_catalog(&problem, &report, &ledger);
    ClearraGeometryCatalogView view;

    EXPECT_TRUE(clearra_geometry_catalog_borrow_view(catalog, &view));
    EXPECT_U64(view.skeleton_count, 1u);
    EXPECT_TRUE(view.realization_count >= view.skeleton_count);
    EXPECT_U64(view.skeleton_realization_counts[0], view.realization_count);
    EXPECT_U64(view.skeleton_cell_masks[0], layout.all_cells_mask);
    EXPECT_U64(view.skeleton_piece_kinds[0], CLR_PIECE_O);
    EXPECT_TRUE(catalog->skeleton_additive_signatures != 0);
    uint8_t expected_signatures[CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT];
    EXPECT_TRUE(clearra_geometry_additive_invariant_compile_signatures(
        layout,
        view.skeleton_cell_masks,
        view.skeleton_count,
        expected_signatures));
    EXPECT_TRUE(memcmp(
        expected_signatures,
        catalog->skeleton_additive_signatures,
        sizeof(expected_signatures)) == 0);
    clearra_geometry_catalog_release(&catalog);
    EXPECT_TRUE(catalog == 0);
}

void geometry_catalog_preserves_prefix_deleted_clear_state_realization(void) {
    ClearraBoard64Layout layout;
    EXPECT_U64(
        clearra_board64_make_layout(10u, 4u, &layout),
        CLEARRA_BOARD64_OK);
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    ClearraGeometryCatalog *catalog = compile_catalog(&problem, &report, &ledger);

    ClearraOperation operation;
    EXPECT_U64(
        clearra_operation_from_shape(
            CLR_PIECE_O, CLEARRA_ROTATION_SPAWN, &operation),
        CLEARRA_OPERATION_OK);
    uint64_t upper_o_mask = 0u;
    EXPECT_U64(
        clearra_operation_mask(layout, &operation, 2, 2, &upper_o_mask),
        CLEARRA_OPERATION_OK);

    ClearraPlacementCandidate variants[CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS];
    uint8_t variant_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_catalog_realizations_for_clear_state(
            catalog,
            CLR_PIECE_O,
            upper_o_mask,
            UINT16_C(0x0003),
            variants,
            &variant_count),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(variant_count > 0u);
    EXPECT_U64(variants[0].required_deleted_row_mask, UINT16_C(0x0003));
    EXPECT_U64((uint8_t)variants[0].y, 2u);

    uint64_t lower_o_mask = 0u;
    EXPECT_U64(
        clearra_operation_mask(layout, &operation, 2, 0, &lower_o_mask),
        CLEARRA_OPERATION_OK);
    uint32_t skeleton_id = UINT32_MAX;
    EXPECT_TRUE(clearra_geometry_catalog_find_skeleton(
        catalog, CLR_PIECE_O, upper_o_mask, &skeleton_id));
    bool concrete_found = false;
    uint32_t begin = catalog->skeleton_realization_offset[skeleton_id];
    uint32_t count = catalog->skeleton_realization_count[skeleton_id];
    for (uint32_t index = 0u; index < count; ++index) {
        const ClearraInverseClearTemplate *template_value =
            clearra_geometry_catalog_template_at_index(catalog, begin + index);
        ClearraConcreteRealization concrete;
        if (!clearra_geometry_catalog_instantiate_realization(
                catalog,
                template_value,
                UINT16_C(0x0003),
                &concrete)) {
            continue;
        }
        EXPECT_U64(concrete.world_cell_mask, lower_o_mask);
        EXPECT_U64(concrete.canonical_cell_ownership, upper_o_mask);
        EXPECT_U64((uint8_t)concrete.lock_y, 0u);
        EXPECT_U64((uint8_t)concrete.target_anchor_y, 2u);
        EXPECT_TRUE(concrete.projection_evidence_digest != 0u);
        concrete_found = true;
        break;
    }
    EXPECT_TRUE(concrete_found);

    clearra_geometry_catalog_release(&catalog);
}

void full_placement_domain_rejects_overlapping_exact_owner_sets(void) {
    static uint32_t piece_kinds[4] = {
        CLR_PIECE_I, CLR_PIECE_I, CLR_PIECE_I, CLR_PIECE_I};
    static uint64_t row_masks[4] = {
        UINT64_C(0x27),
        UINT64_C(0x47),
        UINT64_C(0x9c),
        UINT64_C(0x3c),
    };
    static uint32_t support_rows[16] = {
        0u, 1u,
        0u, 1u,
        0u, 1u, 2u, 3u,
        2u, 3u,
        2u, 3u,
        0u, 3u,
        1u,
        2u,
    };
    ClearraGeometryCatalog catalog = {
        .layout = {
            .width = 8u,
            .height = 1u,
            .cell_count = 8u,
            .all_cells_mask = UINT64_C(0xff),
        },
        .required_fill_mask = UINT64_C(0xff),
        .skeleton_count = 4u,
        .skeleton_piece_kind = piece_kinds,
        .skeleton_cell_mask = row_masks,
        .cell_support_offsets = {
            0u, 2u, 4u, 8u, 10u, 12u, 14u, 15u, 16u,
        },
        .cell_support_row_ids = support_rows,
    };
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.piece_multiset_window.counts[CLR_PIECE_I] = 2u;
    ClearraGeometryExactCoverSearch search = {
        .catalog = &catalog,
        .problem = &problem,
    };
    ClearraActivePieceFamily active_family = {0};
    ClearraGeometryDomainPropagation propagation;

    EXPECT_U64(
        clearra_geometry_full_placement_domain_propagate(
            &search,
            &active_family,
            UINT64_C(0xff),
            &propagation),
        CLEARRA_GEOMETRY_DOMAIN_EMPTY);
    EXPECT_TRUE(propagation.same_tile_certificate_count > 1u);
    EXPECT_TRUE(
        (propagation.pivot_required_cells & UINT64_C(0x04)) != 0u);
    EXPECT_TRUE(
        (propagation.pivot_required_cells & UINT64_C(0x08)) != 0u);
    EXPECT_TRUE(propagation.pivot_support_count == 0u);
    EXPECT_TRUE(propagation.evidence_digest != 0u);
}

void geometry_dynamic_pruning_respects_evidence_policy(void) {
    clr_pruning_proof_ledger ledger;
    ClearraGeometryExactCoverSearch search = {
        .pruning_ledger = &ledger,
        .pruning_batch_id = UINT64_C(99),
        .pruning_catalog_identity_digest = UINT64_C(0xabc),
    };
    const ClearraGeometryInvariantResult invariant_result = {
        .evidence_digest = UINT64_C(0x1234),
        .failed_bank = 1u,
        .checked_bank_count = 3u,
    };
    bool authorized = true;

    EXPECT_U64(
        clr_pruning_proof_ledger_init_with_policy(
            &ledger, CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED),
        CLR_PRUNING_OK);
    for (uint16_t index = 0u; index < CLR_PRUNING_LEDGER_MAX_ENTRIES; ++index) {
        clr_pruning_proof_ledger_entry retained = {
            .batch_id = UINT64_C(7),
            .producer_id = CLR_PRUNING_PRODUCER_STATIC_PLACEMENT_FILTER,
            .catalog_identity_digest = UINT64_C(0x55),
            .state_layer = 1u,
            .prune_reason = CLR_PRUNE_PLACEMENT_COLLISION,
            .proof_level = CLR_PRUNE_PROOF_GLOBAL_SAFE,
            .fallback_if_invalid = CLR_PRUNE_FALLBACK_KEEP_CANDIDATE,
            .affected_candidate_count = 1u,
            .evidence_digest = (uint64_t)index + UINT64_C(1),
        };
        EXPECT_U64(
            clr_pruning_proof_ledger_record(&ledger, retained),
            CLR_PRUNING_OK);
    }
    EXPECT_STATUS(
        clearra_geometry_authorize_additive_invariant(
            &search,
            3u,
            CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE,
            &invariant_result,
            &authorized),
        CLEARRA_PACKING_OK);
    EXPECT_FALSE(authorized);
    EXPECT_U64(ledger.complete_required_capacity_hit, 1u);
    EXPECT_U64(ledger.candidates_kept_due_to_evidence_capacity, 1u);

    clr_pruning_proof_ledger_init(&ledger);
    for (uint16_t index = 0u; index < CLR_PRUNING_LEDGER_MAX_ENTRIES; ++index) {
        clr_pruning_proof_ledger_entry retained = {
            .batch_id = UINT64_C(7),
            .producer_id = CLR_PRUNING_PRODUCER_STATIC_PLACEMENT_FILTER,
            .catalog_identity_digest = UINT64_C(0x55),
            .state_layer = 1u,
            .prune_reason = CLR_PRUNE_PLACEMENT_COLLISION,
            .proof_level = CLR_PRUNE_PROOF_GLOBAL_SAFE,
            .fallback_if_invalid = CLR_PRUNE_FALLBACK_KEEP_CANDIDATE,
            .affected_candidate_count = 1u,
            .evidence_digest = (uint64_t)index + UINT64_C(1),
        };
        EXPECT_U64(
            clr_pruning_proof_ledger_record(&ledger, retained),
            CLR_PRUNING_OK);
    }
    EXPECT_STATUS(
        clearra_geometry_authorize_additive_invariant(
            &search,
            3u,
            CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE,
            &invariant_result,
            &authorized),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(authorized);
    EXPECT_U64(ledger.evidence_truncated, 1u);
    EXPECT_U64(ledger.minimal_record_count, 2u);
    EXPECT_U64(
        ledger.minimal_records[1].producer_id,
        CLR_PRUNING_PRODUCER_GEOMETRY_ADDITIVE_INVARIANT);
}

void geometry_catalog_view_is_pointer_stable_across_search(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometryCatalogView before;
    ClearraGeometryCatalogView after;

    EXPECT_TRUE(clearra_geometry_catalog_borrow_view(catalog, &before));
    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_to_buffer(
            catalog, &problem, &buffer, &search_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(clearra_geometry_catalog_borrow_view(catalog, &after));
    EXPECT_TRUE(before.skeleton_cell_masks == after.skeleton_cell_masks);
    EXPECT_TRUE(before.skeleton_piece_kinds == after.skeleton_piece_kinds);
    EXPECT_TRUE(before.cell_support_offsets == after.cell_support_offsets);
    EXPECT_TRUE(before.cell_support_row_ids == after.cell_support_row_ids);
    EXPECT_U64(buffer.count, 1u);
    clearra_geometry_catalog_release(&catalog);
}

void geometry_catalog_identity_is_independent_of_piece_multiset(void) {
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t o_piece[1] = {CLR_PIECE_O};
    const uint8_t i_piece[1] = {CLR_PIECE_I};
    clr_resource_report left_report;
    clr_resource_report right_report;
    clr_pruning_proof_ledger left_ledger;
    clr_pruning_proof_ledger right_ledger;
    clr_packing_problem left = exact_cover_problem(layout, o_piece, 1u);
    clr_packing_problem right = exact_cover_problem(layout, i_piece, 1u);
    ClearraGeometryCatalog *left_catalog =
        compile_catalog(&left, &left_report, &left_ledger);
    ClearraGeometryCatalog *right_catalog =
        compile_catalog(&right, &right_report, &right_ledger);

    EXPECT_TRUE(memcmp(
                    clearra_geometry_catalog_identity(left_catalog),
                    clearra_geometry_catalog_identity(right_catalog),
                    sizeof(ClearraGeometryCatalogIdentity)) == 0);
    clearra_geometry_catalog_release(&left_catalog);
    clearra_geometry_catalog_release(&right_catalog);
}

void exact_cover_partition_union_matches_serial(void) {
    static ClearraPackingCandidateBuffer serial;
    static ClearraPackingCandidateBuffer partitioned;
    ClearraBoard64Layout layout;
    const uint8_t pieces[5] = {
        CLR_PIECE_I, CLR_PIECE_I, CLR_PIECE_O, CLR_PIECE_O, CLR_PIECE_O};
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;

    EXPECT_U64(clearra_board64_make_layout(10u, 2u, &layout), CLEARRA_BOARD64_OK);
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 5u);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_to_buffer(
            catalog, &problem, &serial, &search_report),
        CLEARRA_PACKING_OK);

    clearra_packing_candidate_buffer_clear(&partitioned);
    CandidateCollector collector = {&partitioned};
    const ClearraPackingCandidateSink sink = {&collector, collect_candidate};
    for (uint16_t partition = 0u; partition < 4u; ++partition) {
        EXPECT_STATUS(
            clearra_geometry_exact_cover_search_to_sink(
                catalog,
                &problem,
                partition,
                4u,
                2u,
                &sink,
                &search_report),
            CLEARRA_PACKING_OK);
    }
    EXPECT_U64(partitioned.count, serial.count);
    for (uint16_t index = 0u; index < serial.count; ++index) {
        ClearraPackingCandidateView candidate;
        EXPECT_STATUS(
            clearra_packing_candidate_buffer_candidate_at(
                &serial, index, &candidate),
            CLEARRA_PACKING_OK);
        EXPECT_TRUE(contains_candidate(&partitioned, &candidate));
    }
    clearra_geometry_catalog_release(&catalog);
}

void residual_memo_requires_exact_piece_counts(void) {
    ClearraGeometryResidualMemo memo;
    uint32_t family = 0u;
    clearra_geometry_residual_memo_init(&memo, 1u, 1024u * 1024u);
    clearra_geometry_residual_memo_insert(
        &memo,
        UINT64_C(0x1234),
        UINT32_C(0x001001),
        UINT32_C(17));
    EXPECT_TRUE(clearra_geometry_residual_memo_lookup(
        &memo, UINT64_C(0x1234), UINT32_C(0x001001), &family));
    EXPECT_U64(family, 17u);
    EXPECT_FALSE(clearra_geometry_residual_memo_lookup(
        &memo, UINT64_C(0x1234), UINT32_C(0x001002), &family));
    EXPECT_FALSE(clearra_geometry_residual_memo_lookup(
        &memo, UINT64_C(0x1235), UINT32_C(0x001001), &family));
    clearra_geometry_residual_memo_release(&memo);
}

void residual_memo_saturation_keeps_search_authority(void) {
    ClearraGeometryResidualMemo memo;
    uint32_t family = 0u;
    clearra_geometry_residual_memo_init(&memo, 1u, 1u);
    clearra_geometry_residual_memo_insert(
        &memo,
        UINT64_C(0x55),
        UINT32_C(0x11),
        CLEARRA_GEOMETRY_FAMILY_EMPTY);
    EXPECT_TRUE(memo.insertion_disabled);
    EXPECT_FALSE(clearra_geometry_residual_memo_lookup(
        &memo, UINT64_C(0x55), UINT32_C(0x11), &family));
    clearra_geometry_residual_memo_release(&memo);
}

void solution_family_uses_stable_row_id_dag(void) {
    ClearraGeometrySolutionFamily family;
    clearra_geometry_solution_family_init(&family, SIZE_MAX);
    ClearraGeometryFamilyRef left = clearra_geometry_solution_family_append(
        &family, 3u, CLEARRA_GEOMETRY_FAMILY_EMPTY);
    ClearraGeometryFamilyRef right = clearra_geometry_solution_family_append(
        &family, 7u, CLEARRA_GEOMETRY_FAMILY_EMPTY);
    ClearraGeometryFamilyRef root =
        clearra_geometry_solution_family_union(&family, left, right);
    EXPECT_U64(
        clearra_geometry_solution_family_append(
            &family, 3u, CLEARRA_GEOMETRY_FAMILY_EMPTY),
        left);
    EXPECT_U64(
        clearra_geometry_solution_family_union(&family, right, left), root);
    ClearraGeometryFamilyRef product =
        clearra_geometry_solution_family_product(&family, left, right);
    EXPECT_U64(
        clearra_geometry_solution_family_product(&family, right, left),
        product);
    const ClearraGeometryFamilyNode *stable_left_node =
        clearra_geometry_solution_family_node(&family, left);
    EXPECT_TRUE(stable_left_node != 0);
    for (uint32_t row_id = 1000u; row_id < 2500u; ++row_id) {
        EXPECT_TRUE(clearra_geometry_solution_family_append(
            &family,
            row_id,
            CLEARRA_GEOMETRY_FAMILY_EMPTY) !=
            CLEARRA_GEOMETRY_FAMILY_INVALID);
    }
    EXPECT_TRUE(
        clearra_geometry_solution_family_node(&family, left) ==
        stable_left_node);
    EXPECT_U64(
        clearra_geometry_solution_family_append(
            &family, 1000u, CLEARRA_GEOMETRY_FAMILY_EMPTY),
        product + 1u);
    const ClearraGeometryFamilyNode *root_node =
        clearra_geometry_solution_family_node(&family, root);
    const ClearraGeometryFamilyNode *left_node =
        clearra_geometry_solution_family_node(&family, left);
    EXPECT_TRUE(root_node != 0);
    EXPECT_U64(root_node->kind, CLEARRA_GEOMETRY_FAMILY_UNION);
    EXPECT_U64(root_node->left, left);
    EXPECT_U64(root_node->right, right);
    EXPECT_TRUE(left_node != 0);
    EXPECT_U64(left_node->kind, CLEARRA_GEOMETRY_FAMILY_APPEND);
    EXPECT_U64(left_node->row_id, 3u);
    EXPECT_U64(left_node->left, CLEARRA_GEOMETRY_FAMILY_EMPTY);
    const ClearraGeometryFamilyNode *product_node =
        clearra_geometry_solution_family_node(&family, product);
    EXPECT_TRUE(product_node != 0);
    EXPECT_U64(product_node->kind, CLEARRA_GEOMETRY_FAMILY_PRODUCT);
    clearra_geometry_solution_family_release(&family);
}

void solution_family_product_streams_cartesian_paths_lazily(void) {
    ClearraGeometrySolutionGraph graph = {0};
    clearra_geometry_solution_family_init(&graph.family, SIZE_MAX);
    ClearraGeometryFamilyRef left = clearra_geometry_solution_family_append(
        &graph.family, 0u, CLEARRA_GEOMETRY_FAMILY_EMPTY);
    ClearraGeometryFamilyRef right = clearra_geometry_solution_family_append(
        &graph.family, 1u, CLEARRA_GEOMETRY_FAMILY_EMPTY);
    graph.root = clearra_geometry_solution_family_product(
        &graph.family, left, right);
    graph.skeleton_count = 2u;
    graph.target_depth = 2u;
    graph.complete = 1u;

    ClearraGeometrySolutionTask task = {.family_ref = graph.root};
    GeometryPathCollector collector = {0};
    const ClearraGeometryPathSink sink = {&collector, collect_geometry_path};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            &graph, &task, &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, 1u);
    EXPECT_U64(collector.operation_count, 2u);
    EXPECT_U64(collector.row_ids[0], 0u);
    EXPECT_U64(collector.row_ids[1], 1u);
    clearra_geometry_solution_family_release(&graph.family);
}

void hypergraph_components_require_absent_cross_component_rows(void) {
    const uint64_t left = UINT64_C(0x000f);
    const uint64_t right = UINT64_C(0x00f0);
    uint64_t masks[3] = {left, right, UINT64_C(0x0033)};
    ClearraGeometryCatalog catalog = {
        .required_fill_mask = left | right,
        .skeleton_count = 2u,
        .skeleton_cell_mask = masks,
    };
    ClearraGeometryComponentDecomposition decomposition;

    EXPECT_TRUE(clearra_geometry_component_decompose(
        &catalog,
        left | right,
        accept_all_geometry_rows,
        0,
        &decomposition));
    EXPECT_U64(decomposition.component_count, 2u);
    EXPECT_U64(decomposition.component_masks[0], left);
    EXPECT_U64(decomposition.component_masks[1], right);
    EXPECT_U64(decomposition.unsupported_cells, 0u);

    catalog.skeleton_count = 3u;
    EXPECT_TRUE(clearra_geometry_component_decompose(
        &catalog,
        left | right,
        accept_all_geometry_rows,
        0,
        &decomposition));
    EXPECT_U64(decomposition.component_count, 1u);
}

void disconnected_exact_cover_uses_lazy_component_product(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout;
    const uint8_t pieces[2] = {CLR_PIECE_O, CLR_PIECE_O};
    const uint64_t target = UINT64_C(0x033) | (UINT64_C(0x033) << 6u);
    clr_resource_report compile_report;
    clr_resource_report graph_report;
    clr_resource_report buffer_report;
    clr_pruning_proof_ledger ledger;

    EXPECT_U64(clearra_board64_make_layout(6u, 2u, &layout), CLEARRA_BOARD64_OK);
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 2u);
    problem.goal_region_mask = target;
    problem.required_fill_mask = target;
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometrySolutionGraph *graph = 0;

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &graph_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);
    const ClearraGeometryFamilyNode *root =
        clearra_geometry_solution_family_node(&graph->family, graph->root);
    EXPECT_TRUE(root != 0);
    EXPECT_U64(root->kind, CLEARRA_GEOMETRY_FAMILY_PRODUCT);

    ClearraGeometrySolutionTask task = {.family_ref = graph->root};
    GeometryPathCollector collector = {0};
    const ClearraGeometryPathSink sink = {&collector, collect_geometry_path};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            graph, &task, &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, 1u);
    EXPECT_U64(collector.operation_count, 2u);

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_to_buffer(
            catalog, &problem, &buffer, &buffer_report),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);

    clearra_geometry_solution_graph_release(&graph);
    clearra_geometry_catalog_release(&catalog);
}

void disconnected_component_join_has_single_canonical_owner(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout;
    const uint8_t pieces[3] = {CLR_PIECE_O, CLR_PIECE_O, CLR_PIECE_O};
    const uint64_t row = UINT64_C(0xdb);
    const uint64_t target = row | (row << 8u);
    clr_resource_report compile_report;
    clr_resource_report graph_report;
    clr_resource_report buffer_report;
    clr_pruning_proof_ledger ledger;

    EXPECT_U64(clearra_board64_make_layout(8u, 2u, &layout), CLEARRA_BOARD64_OK);
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 3u);
    problem.goal_region_mask = target;
    problem.required_fill_mask = target;
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);

    ClearraGeometryComponentDecomposition decomposition;
    EXPECT_TRUE(clearra_geometry_component_decompose(
        catalog,
        target,
        accept_all_geometry_rows,
        0,
        &decomposition));
    EXPECT_U64(decomposition.component_count, 3u);
    ClearraGeometryComponentCompositionPlan plan;
    EXPECT_TRUE(clearra_geometry_component_make_composition_plan(
        &decomposition, target, &plan));
    EXPECT_U64(plan.owner_component_mask, decomposition.component_masks[0]);
    EXPECT_U64(plan.remainder_mask, target & ~plan.owner_component_mask);
    EXPECT_U64(plan.component_count, 3u);

    ClearraGeometrySolutionGraph *graph = 0;
    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &graph_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);
    ClearraGeometrySolutionTask task = {.family_ref = graph->root};
    GeometryPathCollector collector = {0};
    const ClearraGeometryPathSink sink = {&collector, collect_geometry_path};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            graph, &task, &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, 1u);
    EXPECT_U64(collector.path_count, 1u);
    EXPECT_U64(collector.operation_count, 3u);

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_to_buffer(
            catalog, &problem, &buffer, &buffer_report),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);

    clearra_geometry_solution_graph_release(&graph);
    clearra_geometry_catalog_release(&catalog);
}

void exact_cover_graph_streams_immutable_row_paths(void) {
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometrySolutionGraph *graph = 0;

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &search_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);
    EXPECT_TRUE(clearra_geometry_solution_graph_matches_catalog(
        graph, clearra_geometry_catalog_identity(catalog)));
    EXPECT_TRUE(clearra_geometry_solution_graph_node_count(graph) > 0u);
    EXPECT_TRUE(clearra_geometry_solution_graph_resident_bytes(graph) > 0u);

    ClearraGeometrySolutionTask tasks[4];
    uint32_t task_count = 0u;
    size_t task_split_scratch_bytes = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_split_tasks(
            graph, tasks, 4u, &task_count, &task_split_scratch_bytes),
        CLEARRA_PACKING_OK);
    EXPECT_U64(task_count, 1u);

    GeometryPathCollector collector = {0};
    const ClearraGeometryPathSink sink = {&collector, collect_geometry_path};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            graph, &tasks[0], &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, 1u);
    EXPECT_U64(collector.path_count, 1u);
    EXPECT_U64(collector.operation_count, 1u);
    EXPECT_U64(collector.row_ids[0], 0u);

    clearra_geometry_solution_graph_release(&graph);
    EXPECT_TRUE(graph == 0);
    clearra_geometry_catalog_release(&catalog);
}

void exact_cover_graph_represents_complete_empty_result(void) {
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    problem.required_fill_mask = UINT64_C(0x7);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometrySolutionGraph *graph = 0;

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &search_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);
    EXPECT_TRUE(clearra_geometry_solution_graph_matches_catalog(
        graph, clearra_geometry_catalog_identity(catalog)));
    EXPECT_U64(clearra_geometry_solution_graph_node_count(graph), 0u);

    ClearraGeometrySolutionTask task;
    uint32_t task_count = UINT32_MAX;
    size_t task_split_scratch_bytes = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_split_tasks(
            graph, &task, 1u, &task_count, &task_split_scratch_bytes),
        CLEARRA_PACKING_OK);
    EXPECT_U64(task_count, 0u);

    clearra_geometry_solution_graph_release(&graph);
    clearra_geometry_catalog_release(&catalog);
}

void realization_feasibility_preserves_buildable_four_line_geometry(void) {
    ClearraBoard64Layout layout;
    EXPECT_U64(
        clearra_board64_make_layout(10u, 4u, &layout),
        CLEARRA_BOARD64_OK);
    const uint8_t pieces[10] = {
        CLR_PIECE_I,
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_I,
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 10u);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometrySolutionGraph *graph = 0;
    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &search_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);

    ClearraGeometrySolutionTask task;
    uint32_t task_count = 0u;
    size_t task_split_scratch_bytes = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_split_tasks(
            graph, &task, 1u, &task_count, &task_split_scratch_bytes),
        CLEARRA_PACKING_OK);
    EXPECT_U64(task_count, 1u);

    clr_buildup_workspace *workspace = clr_buildup_workspace_create();
    EXPECT_TRUE(workspace != 0);
    RealizationFeasibilityCollector collector = {
        .catalog = catalog,
        .packing = &problem,
        .workspace = workspace,
    };
    const ClearraGeometryPathSink sink = {
        &collector, compare_realization_feasibility_with_buildup};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            graph, &task, &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, collector.path_count);
    EXPECT_TRUE(collector.direct_buildable_count > 0u);
    EXPECT_U64(collector.false_infeasible_count, 0u);

    clearra_realization_feasibility_workspace_release(
        &collector.feasibility_workspace);
    clr_buildup_workspace_release(workspace);
    clearra_geometry_solution_graph_release(&graph);
    clearra_geometry_catalog_release(&catalog);
}

void exact_cover_graph_feeds_buildup_without_candidate_batch(void) {
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_resource_report compile_report;
    clr_resource_report search_report;
    clr_pruning_proof_ledger ledger;
    clr_packing_problem problem = exact_cover_problem(layout, pieces, 1u);
    ClearraGeometryCatalog *catalog =
        compile_catalog(&problem, &compile_report, &ledger);
    ClearraGeometrySolutionGraph *graph = 0;

    EXPECT_STATUS(
        clearra_geometry_exact_cover_search_graph(
            catalog, &problem, &graph, &search_report),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(graph != 0);

    ClearraGeometrySolutionTask task;
    uint32_t task_count = 0u;
    size_t task_split_scratch_bytes = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_split_tasks(
            graph, &task, 1u, &task_count, &task_split_scratch_bytes),
        CLEARRA_PACKING_OK);
    EXPECT_U64(task_count, 1u);

    clr_buildup_workspace *workspace = clr_buildup_workspace_create();
    EXPECT_TRUE(workspace != 0);
    GeometryBuildUpCollector collector = {
        .catalog = catalog,
        .packing = &problem,
        .workspace = workspace,
        .verified_path_count = 0u,
    };
    const ClearraGeometryPathSink sink = {
        &collector, verify_geometry_path_with_buildup};
    uint64_t emitted_count = 0u;
    EXPECT_STATUS(
        clearra_geometry_solution_graph_stream_task_paths(
            graph, &task, &sink, &emitted_count),
        CLEARRA_PACKING_OK);
    EXPECT_U64(emitted_count, 1u);
    EXPECT_U64(collector.verified_path_count, 1u);

    clr_buildup_workspace_release(workspace);
    clearra_geometry_solution_graph_release(&graph);
    clearra_geometry_catalog_release(&catalog);
}
/* SRP rationale: this module has one behavior-level change reason: executable exact-cover geometry behavior across supported board cases. */
