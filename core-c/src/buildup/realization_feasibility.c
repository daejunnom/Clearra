#include "realization_feasibility.h"
#include "realization_domain_propagation.h"

#include "../cache/cache_identity.h"
#include "../packing/geometry_catalog_internal.h"
#include "../packing/target_frame_projection.h"
#include "clr_execution_control.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>

static uint64_t realization_catalog_identity_digest(
    const ClearraGeometryCatalogIdentity *identity) {
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = clearra_cache_key_mix_u64(digest, identity->board_layout_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->compact_universe_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->target_geometry_digest);
    digest = clearra_cache_key_mix_u64(digest, identity->piece_catalog_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->skeleton_projection_version);
    digest = clearra_cache_key_mix_u64(
        digest, identity->rule_capability_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->realization_table_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->support_table_digest);
    return digest == 0u ? UINT64_C(1) : digest;
}

static ClearraPackingStatus authorize_complete_infeasible_result(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    uint64_t stage_discriminator,
    clr_pruning_proof_ledger *pruning_ledger,
    ClearraRealizationFeasibilityResult *result) {
    uint64_t catalog_digest = realization_catalog_identity_digest(
        clearra_geometry_catalog_identity(catalog));
    uint64_t evidence_digest = clearra_cache_key_mix_u64(
        catalog_digest, stage_discriminator);
    for (uint8_t index = 0u; index < operation_count; ++index) {
        evidence_digest = clearra_cache_key_mix_u64(
            evidence_digest, skeleton_row_ids[index]);
        evidence_digest = clearra_cache_key_mix_u64(
            evidence_digest, result->required_predecessors[index]);
    }
    evidence_digest = clearra_cache_key_mix_u64(
        evidence_digest, result->explored_state_count);
    evidence_digest = evidence_digest == 0u ? UINT64_C(1) : evidence_digest;
    result->kind = CLEARRA_REALIZATION_FEASIBILITY_INFEASIBLE;
    result->complete = 1u;
    result->evidence_digest = evidence_digest;
    result->prune_authorized = 0u;
    if (pruning_ledger == 0) {
        return CLEARRA_PACKING_OK;
    }

    uint64_t batch_id = clearra_cache_key_mix_u64(
        catalog_digest, problem->piece_source.piece_source_id);
    batch_id = clearra_cache_key_mix_u64(
        batch_id, problem->piece_source.pattern_universe_id);
    batch_id = clearra_cache_key_mix_u64(
        batch_id, problem->piece_source.pattern_weight_model_id);
    batch_id = clearra_cache_key_mix_u64(
        batch_id, UINT64_C(0x5245414c495a4154));
    clr_pruning_proof_ledger_entry entry = {
        .batch_id = batch_id == 0u ? UINT64_C(1) : batch_id,
        .producer_id = CLR_PRUNING_PRODUCER_REALIZATION_FEASIBILITY,
        .catalog_identity_digest = catalog_digest,
        .state_layer = operation_count,
        .prune_reason = CLR_PRUNE_REALIZATION_DOMAIN_EMPTY,
        .proof_level = CLR_PRUNE_PROOF_GLOBAL_SAFE,
        .fallback_if_invalid = CLR_PRUNE_FALLBACK_RUN_BUILDUP,
        .affected_candidate_count = 1u,
        .evidence_digest = evidence_digest,
    };
    clr_pruning_status status = clr_pruning_proof_ledger_record(
        pruning_ledger, entry);
    if (status == CLR_PRUNING_OK) {
        result->prune_authorized = 1u;
        return CLEARRA_PACKING_OK;
    }
    return status == CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_ARGUMENT;
}

static bool ensure_workspace(
    ClearraRealizationFeasibilityWorkspace *workspace,
    size_t required_states,
    uint8_t required_planes) {
    if ((required_planes != 1u && required_planes != 3u) ||
        required_states == 0u) {
        return false;
    }
    if (workspace->state_capacity >= required_states &&
        workspace->generation_plane_count >= required_planes) {
        return true;
    }
    size_t allocation_states = workspace->state_capacity > required_states
        ? workspace->state_capacity
        : required_states;
    if (allocation_states > SIZE_MAX / required_planes ||
        allocation_states * required_planes >
            SIZE_MAX / sizeof(*workspace->state_generations)) {
        return false;
    }

    size_t generation_count = allocation_states * required_planes;
    uint32_t *state_generations = (uint32_t *)malloc(
        generation_count * sizeof(*workspace->state_generations));
    if (state_generations == 0) {
        return false;
    }
    memset(
        state_generations,
        0,
        generation_count * sizeof(*state_generations));
    free(workspace->state_generations);
    workspace->state_generations = state_generations;
    workspace->state_capacity = allocation_states;
    workspace->generation = 0u;
    workspace->generation_plane_count = required_planes;
    return true;
}

static bool ensure_realization_word_workspace(
    ClearraRealizationFeasibilityWorkspace *workspace,
    size_t required_words) {
    if (workspace == 0 || required_words == 0u) {
        return false;
    }
    if (workspace->realization_word_capacity >= required_words) {
        return true;
    }
    if (required_words > SIZE_MAX / 2u ||
        required_words * 2u > SIZE_MAX / sizeof(*workspace->realization_words)) {
        return false;
    }
    uint64_t *words = (uint64_t *)malloc(
        required_words * 2u * sizeof(*workspace->realization_words));
    if (words == 0) {
        return false;
    }
    free(workspace->realization_words);
    workspace->realization_words = words;
    workspace->realization_word_capacity = required_words;
    return true;
}

static void advance_generation(
    ClearraRealizationFeasibilityWorkspace *workspace) {
    workspace->generation++;
    if (workspace->generation != 0u) {
        return;
    }
    memset(
        workspace->state_generations,
        0,
        workspace->state_capacity * workspace->generation_plane_count *
            sizeof(*workspace->state_generations));
    workspace->generation = 1u;
}

static uint32_t *workspace_generation_plane(
    ClearraRealizationFeasibilityWorkspace *workspace,
    size_t plane) {
    if (workspace == 0 || plane >= workspace->generation_plane_count) {
        return 0;
    }
    return workspace->state_generations + workspace->state_capacity * plane;
}

static bool compress_target_mask(
    ClearraBoard64Layout layout,
    uint64_t target_mask,
    uint16_t deleted_rows,
    uint64_t *out_current_mask) {
    uint64_t current_mask = 0u;
    uint8_t current_row = 0u;
    for (uint8_t target_row = 0u; target_row < layout.height; ++target_row) {
        uint16_t row_bit = (uint16_t)(UINT16_C(1) << target_row);
        uint64_t target_row_mask = 0u;
        if (clearra_board64_row_mask(layout, target_row, &target_row_mask) !=
            CLEARRA_BOARD64_OK) {
            return false;
        }
        if ((deleted_rows & row_bit) != 0u) {
            continue;
        }
        uint64_t row_cells =
            (target_mask & target_row_mask) >> (target_row * layout.width);
        current_mask |= row_cells << (current_row * layout.width);
        current_row++;
    }
    *out_current_mask = current_mask;
    return true;
}

static bool grounded_in_clear_state(
    ClearraBoard64Layout layout,
    uint64_t target_board,
    uint64_t target_placement,
    uint16_t deleted_rows) {
    uint64_t current_board = 0u;
    uint64_t current_placement = 0u;
    if (!compress_target_mask(
            layout, target_board, deleted_rows, &current_board) ||
        !compress_target_mask(
            layout, target_placement, deleted_rows, &current_placement)) {
        return false;
    }
    uint64_t floor_mask = layout.width == 64u
        ? UINT64_MAX
        : (UINT64_C(1) << layout.width) - UINT64_C(1);
    return (current_placement & floor_mask) != 0u ||
           (layout.width < 64u &&
            ((current_placement >> layout.width) & current_board) != 0u);
}

typedef enum ClearraForwardReplayMatch {
    CLEARRA_FORWARD_REPLAY_NO_MATCH = 0,
    CLEARRA_FORWARD_REPLAY_MATCH = 1,
    CLEARRA_FORWARD_REPLAY_INCOMPLETE = 2
} ClearraForwardReplayMatch;

static ClearraForwardReplayMatch replay_realization_transition(
    const ClearraGeometryCatalog *catalog,
    ClearraConcreteRealization *realization,
    uint64_t target_board_before,
    uint64_t target_placement,
    uint16_t deleted_rows_before,
    uint16_t deleted_rows_after) {
    uint64_t current_board = 0u;
    uint64_t expected_successor = 0u;
    if (!compress_target_mask(
            catalog->layout,
            target_board_before,
            deleted_rows_before,
            &current_board) ||
        !compress_target_mask(
            catalog->layout,
            target_board_before | target_placement,
            deleted_rows_after,
            &expected_successor)) {
        return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }

    ClearraBoard64LineClearResult predecessor_clear;
    if (clearra_board64_clear_lines(
            catalog->layout,
            current_board,
            &predecessor_clear) != CLEARRA_BOARD64_OK) {
        return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }
    if (predecessor_clear.cleared_lines != 0u) {
        return CLEARRA_FORWARD_REPLAY_NO_MATCH;
    }

    uint64_t pre_clear_board = 0u;
    ClearraBoard64Status place_status = clearra_board64_place(
        catalog->layout,
        current_board,
        realization->world_cell_mask,
        &pre_clear_board);
    if (place_status == CLEARRA_BOARD64_COLLISION) {
        return CLEARRA_FORWARD_REPLAY_NO_MATCH;
    }
    if (place_status != CLEARRA_BOARD64_OK) {
        return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }

    ClearraBoard64LineClearResult clear_result;
    if (clearra_board64_clear_lines(
            catalog->layout,
            pre_clear_board,
            &clear_result) != CLEARRA_BOARD64_OK) {
        return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }
    uint16_t merged_deleted_rows = 0u;
    if (clearra_target_frame_merge_deleted_rows(
            catalog->layout.height,
            deleted_rows_before,
            clear_result.deleted_row_mask,
            &merged_deleted_rows) != CLEARRA_PACKING_OK) {
        return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }
    if (merged_deleted_rows != deleted_rows_after ||
        clear_result.board != expected_successor) {
        return CLEARRA_FORWARD_REPLAY_NO_MATCH;
    }

    uint16_t cleared_rows = clear_result.deleted_row_mask;
    while (cleared_rows != 0u) {
        uint16_t row_bit = (uint16_t)(
            cleared_rows & (uint16_t)(~cleared_rows + UINT16_C(1)));
        uint8_t current_row = 0u;
        for (uint16_t cursor = row_bit;
             (cursor & UINT16_C(1)) == 0u;
             cursor >>= 1u) {
            current_row++;
        }
        uint64_t row_mask = 0u;
        if (clearra_board64_row_mask(
                catalog->layout,
                current_row,
                &row_mask) != CLEARRA_BOARD64_OK) {
            return CLEARRA_FORWARD_REPLAY_INCOMPLETE;
        }
        if ((realization->world_cell_mask & row_mask) == 0u) {
            return CLEARRA_FORWARD_REPLAY_NO_MATCH;
        }
        cleared_rows = (uint16_t)(cleared_rows & ~row_bit);
    }
    realization->inserted_row_mask = (uint16_t)(
        merged_deleted_rows & ~deleted_rows_before);
    realization->completed_row_mask = realization->inserted_row_mask;
    uint64_t replay_evidence = clearra_cache_key_mix_u64(
        realization->projection_evidence_digest,
        current_board);
    replay_evidence = clearra_cache_key_mix_u64(
        replay_evidence, pre_clear_board);
    replay_evidence = clearra_cache_key_mix_u64(
        replay_evidence, clear_result.board);
    replay_evidence = clearra_cache_key_mix_u64(
        replay_evidence, merged_deleted_rows);
    realization->forward_replay_evidence_digest =
        replay_evidence == 0u ? UINT64_C(1) : replay_evidence;
    return CLEARRA_FORWARD_REPLAY_MATCH;
}

static ClearraForwardReplayMatch realization_transition_exists(
    const ClearraGeometryCatalog *catalog,
    const ClearraRealizationDomainPropagationInput *domain_input,
    uint8_t operation,
    const ClearraRealizationCandidateDomain *domain,
    uint64_t target_board_before,
    uint64_t target_placement,
    uint16_t deleted_rows_before,
    uint16_t deleted_rows_after) {
    uint32_t begin = catalog->skeleton_realization_offset[domain->skeleton_id];
    uint32_t count = catalog->skeleton_realization_count[domain->skeleton_id];
    bool incomplete = false;
    for (uint32_t index = 0u; index < count; ++index) {
        if (domain_input != 0 &&
            !clearra_realization_domain_value_is_active(
                domain_input, operation, index)) {
            continue;
        }
        uint32_t realization_index = begin + index;
        if (!clearra_geometry_catalog_realization_supports_clear_state(
                catalog, realization_index, deleted_rows_before)) {
            continue;
        }
        const ClearraInverseClearTemplate *template_value =
            clearra_geometry_catalog_template_at_index(
                catalog, realization_index);
        ClearraConcreteRealization realization;
        if (!clearra_geometry_catalog_instantiate_realization(
                catalog,
                template_value,
                deleted_rows_before,
                &realization)) {
            continue;
        }
        ClearraForwardReplayMatch match = replay_realization_transition(
            catalog,
            &realization,
            target_board_before,
            target_placement,
            deleted_rows_before,
            deleted_rows_after);
        if (match == CLEARRA_FORWARD_REPLAY_MATCH) {
            return match;
        }
        incomplete = incomplete || match == CLEARRA_FORWARD_REPLAY_INCOMPLETE;
    }
    return incomplete ? CLEARRA_FORWARD_REPLAY_INCOMPLETE
                      : CLEARRA_FORWARD_REPLAY_NO_MATCH;
}

static bool compile_candidate_domains(
    const ClearraGeometryCatalog *catalog,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraRealizationCandidateDomain
        domains[CLR_BUILDUP_MAX_OPERATIONS],
    size_t *out_realization_word_count) {
    if (out_realization_word_count == 0) {
        return false;
    }
    size_t realization_word_count = 0u;
    for (uint8_t operation = 0u; operation < operation_count; ++operation) {
        uint32_t skeleton_id = skeleton_row_ids[operation];
        if (skeleton_id >= catalog->skeleton_count ||
            catalog->skeleton_realization_count[skeleton_id] == 0u) {
            return false;
        }
        ClearraRealizationCandidateDomain *domain = &domains[operation];
        uint32_t realization_count =
            catalog->skeleton_realization_count[skeleton_id];
        uint32_t active_word_count = realization_count / UINT32_C(64) +
            (uint32_t)((realization_count % UINT32_C(64)) != 0u);
        if (active_word_count == 0u ||
            realization_word_count > UINT32_MAX ||
            active_word_count > UINT32_MAX - realization_word_count) {
            return false;
        }
        *domain = (ClearraRealizationCandidateDomain){
            .skeleton_id = skeleton_id,
            .realization_begin =
                catalog->skeleton_realization_offset[skeleton_id],
            .realization_count = realization_count,
            .active_word_offset = (uint32_t)realization_word_count,
            .active_word_count = active_word_count,
            .compact_deleted_states =
                catalog->skeleton_deleted_state_bits[skeleton_id],
            .contributing_rows =
                catalog->skeleton_using_row_mask[skeleton_id],
            .required_deleted_rows =
                catalog->skeleton_required_deleted_rows[skeleton_id],
            .compact = (uint8_t)(catalog->layout.height <= 6u),
        };
        if (domain->compact != 0u && domain->compact_deleted_states == 0u) {
            return false;
        }
        realization_word_count += active_word_count;
    }
    *out_realization_word_count = realization_word_count;
    return true;
}

static void initialize_active_realization_domains(
    const ClearraRealizationCandidateDomain
        domains[CLR_BUILDUP_MAX_OPERATIONS],
    uint8_t operation_count,
    uint64_t *active_words,
    size_t active_word_count) {
    memset(active_words, 0, active_word_count * sizeof(*active_words));
    for (uint8_t operation = 0u;
         operation < operation_count;
         ++operation) {
        const ClearraRealizationCandidateDomain *domain = &domains[operation];
        for (uint32_t local_index = 0u;
             local_index < domain->realization_count;
             ++local_index) {
            size_t word_index = domain->active_word_offset +
                                local_index / 64u;
            active_words[word_index] |=
                UINT64_C(1) << (local_index % 64u);
        }
    }
}

static bool compile_row_contributors(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const ClearraRealizationCandidateDomain
        domains[CLR_BUILDUP_MAX_OPERATIONS],
    uint8_t operation_count,
    uint16_t contributors[16],
    uint16_t *out_clearable_rows) {
    uint64_t final_board = problem->board.initial_mask;
    for (uint8_t operation = 0u; operation < operation_count; ++operation) {
        final_board |= catalog->skeleton_cell_mask[domains[operation].skeleton_id];
    }

    uint16_t clearable_rows = 0u;
    for (uint8_t row = 0u; row < catalog->layout.height; ++row) {
        uint64_t row_mask = 0u;
        if (clearra_board64_row_mask(catalog->layout, row, &row_mask) !=
            CLEARRA_BOARD64_OK) {
            return false;
        }
        if ((problem->board.initial_mask & row_mask) == row_mask) {
            return false;
        }
        if ((final_board & row_mask) != row_mask) {
            continue;
        }
        clearable_rows =
            (uint16_t)(clearable_rows | (uint16_t)(UINT16_C(1) << row));
        for (uint8_t operation = 0u; operation < operation_count; ++operation) {
            if ((domains[operation].contributing_rows &
                 (uint16_t)(UINT16_C(1) << row)) != 0u) {
                contributors[row] = (uint16_t)(
                    contributors[row] |
                    (uint16_t)(UINT16_C(1) << operation));
            }
        }
        if (contributors[row] == 0u) {
            return false;
        }
    }
    *out_clearable_rows = clearable_rows;
    return true;
}

static bool close_required_predecessors(
    uint8_t operation_count,
    uint16_t required_predecessors[CLR_BUILDUP_MAX_OPERATIONS]) {
    for (uint8_t pivot = 0u; pivot < operation_count; ++pivot) {
        uint16_t pivot_bit = (uint16_t)(UINT16_C(1) << pivot);
        for (uint8_t operation = 0u;
             operation < operation_count;
             ++operation) {
            if ((required_predecessors[operation] & pivot_bit) != 0u) {
                required_predecessors[operation] = (uint16_t)(
                    required_predecessors[operation] |
                    required_predecessors[pivot]);
            }
        }
    }
    for (uint8_t operation = 0u;
         operation < operation_count;
         ++operation) {
        if ((required_predecessors[operation] &
             (uint16_t)(UINT16_C(1) << operation)) != 0u) {
            return false;
        }
    }
    return true;
}

static bool compile_required_predecessors(
    const ClearraRealizationCandidateDomain
        domains[CLR_BUILDUP_MAX_OPERATIONS],
    uint8_t operation_count,
    const uint16_t contributors[16],
    uint16_t clearable_rows,
    uint16_t required_predecessors[CLR_BUILDUP_MAX_OPERATIONS]) {
    uint16_t operation_mask = (uint16_t)(
        ((uint32_t)UINT16_C(1) << operation_count) - UINT32_C(1));
    for (uint8_t operation = 0u;
         operation < operation_count;
         ++operation) {
        uint16_t required_rows = domains[operation].required_deleted_rows;
        if ((required_rows & ~clearable_rows) != 0u) {
            return false;
        }
        uint16_t predecessors = 0u;
        while (required_rows != 0u) {
            uint16_t row_bit = (uint16_t)(
                required_rows & (uint16_t)(~required_rows + 1u));
            uint8_t row = 0u;
            for (uint16_t cursor = row_bit;
                 (cursor & UINT16_C(1)) == 0u;
                 cursor >>= 1u) {
                row++;
            }
            predecessors = (uint16_t)(
                predecessors | contributors[row]);
            required_rows = (uint16_t)(required_rows & ~row_bit);
        }
        predecessors = (uint16_t)(predecessors & operation_mask);
        if ((predecessors & (uint16_t)(UINT16_C(1) << operation)) != 0u) {
            return false;
        }
        required_predecessors[operation] = predecessors;
    }

    return close_required_predecessors(
        operation_count, required_predecessors);
}

typedef enum ClearraRealizationSearchOutcome {
    CLEARRA_REALIZATION_SEARCH_EXHAUSTED = 0,
    CLEARRA_REALIZATION_SEARCH_FOUND = 1,
    CLEARRA_REALIZATION_SEARCH_CANCELLED = 2,
    CLEARRA_REALIZATION_SEARCH_INCOMPLETE = 3
} ClearraRealizationSearchOutcome;

typedef struct ClearraRealizationSearchContext {
    const ClearraGeometryCatalog *catalog;
    const ClearraRealizationCandidateDomain *domains;
    const ClearraRealizationDomainPropagationInput *domain_input;
    ClearraRealizationFeasibilityWorkspace *workspace;
    ClearraRealizationFeasibilityResult *result;
    const uint16_t *contributors;
    const uint16_t *required_predecessors;
    uint16_t clearable_rows;
    uint16_t terminal_state;
    uint8_t operation_count;
    const uint32_t *live_generations;
    uint32_t generation;
    uint8_t operation_priority[CLR_BUILDUP_MAX_OPERATIONS];
    uint8_t path[CLR_BUILDUP_MAX_OPERATIONS];
} ClearraRealizationSearchContext;

static uint8_t lowest_target_row(
    ClearraBoard64Layout layout,
    uint64_t mask) {
    for (uint8_t row = 0u; row < layout.height; ++row) {
        uint64_t row_mask = 0u;
        if (clearra_board64_row_mask(layout, row, &row_mask) ==
                CLEARRA_BOARD64_OK &&
            (mask & row_mask) != 0u) {
            return row;
        }
    }
    return layout.height;
}

static void compile_operation_priority(
    const ClearraGeometryCatalog *catalog,
    const ClearraRealizationCandidateDomain *domains,
    uint8_t operation_count,
    uint8_t priority[CLR_BUILDUP_MAX_OPERATIONS]) {
    for (uint8_t index = 0u; index < operation_count; ++index) {
        priority[index] = index;
    }
    for (uint8_t index = 1u; index < operation_count; ++index) {
        uint8_t operation = priority[index];
        uint8_t row = lowest_target_row(
            catalog->layout,
            catalog->skeleton_cell_mask[domains[operation].skeleton_id]);
        uint8_t cursor = index;
        while (cursor > 0u) {
            uint8_t previous = priority[cursor - 1u];
            uint8_t previous_row = lowest_target_row(
                catalog->layout,
                catalog->skeleton_cell_mask[domains[previous].skeleton_id]);
            if (previous_row < row ||
                (previous_row == row && previous < operation)) {
                break;
            }
            priority[cursor] = previous;
            cursor--;
        }
        priority[cursor] = operation;
    }
}

static ClearraRealizationSearchOutcome search_realization_order(
    ClearraRealizationSearchContext *context,
    uint16_t state,
    uint64_t target_board,
    uint8_t depth) {
    if (state == context->terminal_state) {
        memcpy(
            context->result->operation_order,
            context->path,
            context->operation_count);
        context->result->operation_count = context->operation_count;
        return CLEARRA_REALIZATION_SEARCH_FOUND;
    }
    if (context->live_generations != 0 &&
        context->live_generations[state] != context->generation) {
        return CLEARRA_REALIZATION_SEARCH_EXHAUSTED;
    }
    uint32_t *visited_generations = workspace_generation_plane(
        context->workspace, 0u);
    if (visited_generations[state] ==
        context->workspace->generation) {
        return CLEARRA_REALIZATION_SEARCH_EXHAUSTED;
    }
    visited_generations[state] = context->workspace->generation;
    context->result->explored_state_count++;
    if ((context->result->explored_state_count & UINT64_C(255)) == 0u &&
        clr_execution_cancel_requested()) {
        return CLEARRA_REALIZATION_SEARCH_CANCELLED;
    }

    uint16_t deleted_rows = clearra_realization_deleted_rows_for_state(
        context->contributors, context->clearable_rows, state);
    for (uint8_t priority_index = 0u;
         priority_index < context->operation_count;
         ++priority_index) {
        uint8_t operation = context->operation_priority[priority_index];
        uint16_t operation_bit =
            (uint16_t)(UINT16_C(1) << operation);
        uint16_t next_state = (uint16_t)(state | operation_bit);
        if ((state & operation_bit) != 0u ||
            (context->live_generations != 0 &&
             context->live_generations[next_state] != context->generation) ||
            (state & context->required_predecessors[operation]) !=
                context->required_predecessors[operation] ||
            !clearra_realization_domain_supports_deleted_state(
                context->catalog, &context->domains[operation], deleted_rows)) {
            continue;
        }
        uint64_t operation_mask = context->catalog->skeleton_cell_mask
            [context->domains[operation].skeleton_id];
        if (!grounded_in_clear_state(
                context->catalog->layout,
                target_board,
                operation_mask,
                deleted_rows)) {
            continue;
        }
        uint16_t next_deleted_rows = clearra_realization_deleted_rows_for_state(
            context->contributors,
            context->clearable_rows,
            next_state);
        ClearraForwardReplayMatch replay = realization_transition_exists(
            context->catalog,
            context->domain_input,
            operation,
            &context->domains[operation],
            target_board,
            operation_mask,
            deleted_rows,
            next_deleted_rows);
        if (replay == CLEARRA_FORWARD_REPLAY_INCOMPLETE) {
            return CLEARRA_REALIZATION_SEARCH_INCOMPLETE;
        }
        if (replay != CLEARRA_FORWARD_REPLAY_MATCH) {
            continue;
        }
        context->path[depth] = operation;
        ClearraRealizationSearchOutcome outcome = search_realization_order(
            context,
            next_state,
            target_board | operation_mask,
            (uint8_t)(depth + 1u));
        if (outcome != CLEARRA_REALIZATION_SEARCH_EXHAUSTED) {
            return outcome;
        }
    }
    return CLEARRA_REALIZATION_SEARCH_EXHAUSTED;
}

ClearraPackingStatus clearra_realization_feasibility_analyze(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraRealizationFeasibilityWorkspace *workspace,
    clr_pruning_proof_ledger *pruning_ledger,
    ClearraRealizationFeasibilityResult *out_result) {
    if (catalog == 0 || problem == 0 || skeleton_row_ids == 0 ||
        workspace == 0 || out_result == 0 || operation_count == 0u ||
        operation_count > CLR_BUILDUP_MAX_OPERATIONS ||
        catalog->layout.height > 16u ||
        !clearra_geometry_catalog_matches_problem(catalog, problem)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_result = (ClearraRealizationFeasibilityResult){
        .kind = CLEARRA_REALIZATION_FEASIBILITY_UNKNOWN,
    };

    size_t state_count = (size_t)1u << operation_count;
    ClearraRealizationCandidateDomain domains[CLR_BUILDUP_MAX_OPERATIONS];
    size_t realization_word_count = 0u;
    if (!compile_candidate_domains(
            catalog,
            skeleton_row_ids,
            operation_count,
            domains,
            &realization_word_count)) {
        return CLEARRA_PACKING_OK;
    }
    uint16_t contributors[16] = {0u};
    uint16_t clearable_rows = 0u;
    if (!compile_row_contributors(
            catalog,
            problem,
            domains,
            operation_count,
            contributors,
            &clearable_rows)) {
        return CLEARRA_PACKING_OK;
    }
    uint16_t required_predecessors[CLR_BUILDUP_MAX_OPERATIONS] = {0u};
    if (!compile_required_predecessors(
            domains,
            operation_count,
            contributors,
            clearable_rows,
            required_predecessors)) {
        return authorize_complete_infeasible_result(
            catalog,
            problem,
            skeleton_row_ids,
            operation_count,
            UINT64_C(1),
            pruning_ledger,
            out_result);
    }
    memcpy(
        out_result->required_predecessors,
        required_predecessors,
        operation_count * sizeof(*required_predecessors));

    uint16_t terminal_state = (uint16_t)(state_count - 1u);
    bool run_domain_propagation = clearable_rows != 0u;
    if (!ensure_workspace(
            workspace, state_count, run_domain_propagation ? 3u : 1u)) {
        return CLEARRA_PACKING_OK;
    }
    if (run_domain_propagation &&
        !ensure_realization_word_workspace(
            workspace, realization_word_count)) {
        return CLEARRA_PACKING_OK;
    }
    advance_generation(workspace);
    uint32_t *live_generations = 0;
    ClearraRealizationDomainPropagationInput propagation_input = {0};
    if (run_domain_propagation) {
        initialize_active_realization_domains(
            domains,
            operation_count,
            workspace->realization_words,
            realization_word_count);
        propagation_input = (ClearraRealizationDomainPropagationInput){
            .catalog = catalog,
            .domains = domains,
            .contributors = contributors,
            .required_predecessors = required_predecessors,
            .active_realization_words = workspace->realization_words,
            .supported_realization_words =
                workspace->realization_words +
                workspace->realization_word_capacity,
            .realization_word_count = realization_word_count,
            .clearable_rows = clearable_rows,
            .terminal_state = terminal_state,
            .operation_count = operation_count,
        };
        ClearraRealizationDomainPropagationResult propagation_result;
        uint32_t *reachable_generations =
            workspace_generation_plane(workspace, 1u);
        live_generations = workspace_generation_plane(workspace, 2u);
        ClearraRealizationDomainPropagationStatus propagation_status =
            clearra_realization_domain_propagate(
                &propagation_input,
                reachable_generations,
                live_generations,
                workspace->state_capacity,
                workspace->generation,
                &propagation_result);
        if (propagation_status == CLEARRA_REALIZATION_DOMAIN_INVALID) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        if (propagation_status == CLEARRA_REALIZATION_DOMAIN_INFEASIBLE) {
            out_result->explored_state_count =
                propagation_result.reachable_state_count;
            if (propagation_result.complete == 0u) {
                return CLEARRA_PACKING_OK;
            }
            return authorize_complete_infeasible_result(
                catalog,
                problem,
                skeleton_row_ids,
                operation_count,
                UINT64_C(2),
                pruning_ledger,
                out_result);
        }
        if (!clearra_realization_domain_common_predecessors(
                &propagation_input, required_predecessors) ||
            !close_required_predecessors(
                operation_count, required_predecessors)) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        memcpy(
            out_result->required_predecessors,
            required_predecessors,
            operation_count * sizeof(*required_predecessors));
    }
    ClearraRealizationSearchContext search = {
        .catalog = catalog,
        .domains = domains,
        .domain_input = run_domain_propagation ? &propagation_input : 0,
        .workspace = workspace,
        .result = out_result,
        .contributors = contributors,
        .required_predecessors = required_predecessors,
        .clearable_rows = clearable_rows,
        .terminal_state = terminal_state,
        .operation_count = operation_count,
        .live_generations = live_generations,
        .generation = workspace->generation,
    };
    compile_operation_priority(
        catalog, domains, operation_count, search.operation_priority);
    ClearraRealizationSearchOutcome outcome = search_realization_order(
        &search, 0u, problem->board.initial_mask, 0u);
    if (outcome == CLEARRA_REALIZATION_SEARCH_CANCELLED) {
        return CLEARRA_PACKING_CANCELLED;
    }
    if (outcome == CLEARRA_REALIZATION_SEARCH_INCOMPLETE) {
        return CLEARRA_PACKING_OK;
    }
    if (outcome == CLEARRA_REALIZATION_SEARCH_FOUND) {
        out_result->kind = CLEARRA_REALIZATION_FEASIBILITY_FEASIBLE;
        out_result->complete = 1u;
        return CLEARRA_PACKING_OK;
    }

    return authorize_complete_infeasible_result(
        catalog,
        problem,
        skeleton_row_ids,
        operation_count,
        UINT64_C(3),
        pruning_ledger,
        out_result);
}

void clearra_realization_feasibility_workspace_release(
    ClearraRealizationFeasibilityWorkspace *workspace) {
    if (workspace == 0) {
        return;
    }
    free(workspace->state_generations);
    free(workspace->realization_words);
    *workspace = (ClearraRealizationFeasibilityWorkspace){0};
}

size_t clearra_realization_feasibility_workspace_retained_bytes(
    const ClearraRealizationFeasibilityWorkspace *workspace) {
    if (workspace == 0) {
        return 0u;
    }
    if (workspace->state_capacity == 0u ||
        workspace->generation_plane_count == 0u) {
        return 0u;
    }
    size_t per_state = sizeof(*workspace->state_generations) *
                       workspace->generation_plane_count;
    size_t state_bytes = workspace->state_capacity > SIZE_MAX / per_state
        ? SIZE_MAX
        : workspace->state_capacity * per_state;
    if (workspace->realization_word_capacity >
        SIZE_MAX / (2u * sizeof(*workspace->realization_words))) {
        return SIZE_MAX;
    }
    size_t realization_bytes = workspace->realization_word_capacity * 2u *
                               sizeof(*workspace->realization_words);
    return state_bytes > SIZE_MAX - realization_bytes
        ? SIZE_MAX
        : state_bytes + realization_bytes;
}
