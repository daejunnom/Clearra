#include "gpu_backend.h"
#include "gpu_batch_descriptor_internal.h"

typedef char clearra_gpu_batch_piece_capacity_must_match_packing
    [(CLEARRA_GPU_BATCH_MAX_PIECES == CLEARRA_PACKING_MAX_PIECES) ? 1 : -1];
typedef char clearra_gpu_packing_batch_descriptor_abi_size_must_match_v5
    [(sizeof(ClearraGpuPackingBatchDescriptor) ==
      CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_SIZE)
         ? 1
         : -1];

uint64_t clearra_gpu_low_mask_for_cells(uint8_t cell_count) {
    if (cell_count >= 64u) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << cell_count) - UINT64_C(1);
}

ClearraGpuStatus clearra_gpu_batch_descriptor_init(
    ClearraBoard64Layout layout,
    uint64_t initial_board,
    uint8_t active_packing_rows,
    const uint8_t *pieces,
    uint8_t piece_count,
    ClearraGpuPackingBatchDescriptor *out_batch) {
    if (pieces == 0 || out_batch == 0 || piece_count == 0 ||
        piece_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    if (!clearra_board64_layout_is_valid(layout)) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    out_batch->batch_id = 1u;
    out_batch->board_width = layout.width;
    out_batch->board_height = layout.height;
    out_batch->active_packing_rows = active_packing_rows;
    out_batch->goal_clear_lines_hint = 0u;
    out_batch->piece_window = piece_count;
    out_batch->piece_count = piece_count;
    out_batch->exact_piece_count = piece_count;
    out_batch->piece_source_kind = CLEARRA_GPU_PIECE_SOURCE_FIXED_SEQUENCE;
    out_batch->piece_source_id = 1u;
    out_batch->piece_multiset_window =
        clearra_gpu_piece_multiset_window_from_pieces(pieces, piece_count);
    out_batch->initial_board_mask = initial_board;
    out_batch->operation_table_id = 1u;
    out_batch->rule_profile_id = CLR_RULE_SRS_PLUS;
    out_batch->kick_profile_id = CLR_KICK_SRS_PLUS_180;
    out_batch->candidate_capacity = CLEARRA_PACKING_MAX_CANDIDATES;
    out_batch->max_frontier_states = 2048u;
    out_batch->pattern_count = 1u;
    out_batch->shape_hash_seed = UINT64_C(14695981039346656037);
    out_batch->pattern_universe_id = 1u;
    out_batch->pattern_weight_model_id = 1u;
    return clearra_gpu_batch_descriptor_validate(out_batch);
}

ClearraGpuStatus clearra_gpu_batch_descriptor_layout(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraBoard64Layout *out_layout) {
    if (batch == 0 || out_layout == 0 ||
        clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    return clearra_board64_make_layout(
               batch->board_width, batch->board_height, out_layout) ==
                   CLEARRA_BOARD64_OK
               ? CLEARRA_GPU_OK
               : CLEARRA_GPU_INVALID_ARGUMENT;
}

ClearraGpuStatus clearra_gpu_batch_descriptor_validate(
    const ClearraGpuPackingBatchDescriptor *batch) {
    uint16_t cell_count;
    uint16_t active_cell_count;
    if (batch == 0 || batch->batch_id == 0 || batch->board_width == 0 ||
        batch->board_height == 0 || batch->active_packing_rows == 0 ||
        batch->piece_window == 0 || batch->piece_count == 0 ||
        batch->piece_count > batch->piece_window ||
        batch->exact_piece_count > batch->piece_window ||
        batch->piece_source_kind == CLEARRA_GPU_PIECE_SOURCE_UNKNOWN ||
        batch->piece_source_kind > CLEARRA_GPU_PIECE_SOURCE_OBSERVED_WINDOW ||
        batch->piece_source_id == 0 ||
        batch->piece_count > CLEARRA_PACKING_MAX_PIECES ||
        batch->candidate_capacity == 0 || batch->max_frontier_states == 0u ||
        batch->pattern_count == 0u || batch->operation_table_id == 0 ||
        batch->rule_profile_id == 0 || batch->rule_profile_id > UINT32_MAX ||
        batch->kick_profile_id == 0 || batch->kick_profile_id > UINT32_MAX ||
        batch->shape_hash_seed == 0 || batch->pattern_universe_id == 0 ||
        batch->pattern_weight_model_id == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    if (!clearra_gpu_piece_multiset_window_is_valid(
            &batch->piece_multiset_window) ||
        batch->piece_multiset_window.total_count != batch->piece_count ||
        (batch->piece_multiset_window.exact_count != 0u &&
         batch->piece_multiset_window.exact_count != batch->exact_piece_count)) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    cell_count = (uint16_t)batch->board_width * (uint16_t)batch->board_height;
    active_cell_count =
        (uint16_t)batch->board_width * (uint16_t)batch->active_packing_rows;
    if (cell_count == 0 || cell_count > 64u ||
        batch->active_packing_rows > batch->board_height ||
        batch->goal_clear_lines_hint > batch->board_height ||
        active_cell_count == 0 || active_cell_count > 64u ||
        (batch->initial_board_mask &
         ~clearra_gpu_low_mask_for_cells((uint8_t)active_cell_count)) != 0u ||
        batch->candidate_capacity > UINT16_MAX) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    return CLEARRA_GPU_OK;
}

clr_gpu_piece_multiset_window clearra_gpu_piece_multiset_window_from_pieces(
    const uint8_t *pieces,
    uint8_t piece_count) {
    clr_gpu_piece_multiset_window window = {0};
    if (pieces == 0) {
        return window;
    }

    window.total_count = piece_count;
    window.exact_count = piece_count;
    for (uint8_t index = 0u; index < piece_count; index++) {
        uint8_t piece = pieces[index];
        if (clearra_piece_is_standard_tetromino(piece)) {
            window.counts[piece]++;
        }
    }
    return window;
}

clr_piece_multiset_window clearra_gpu_piece_multiset_window_to_c(
    clr_gpu_piece_multiset_window gpu_window) {
    clr_piece_multiset_window window = clearra_piece_multiset_window_empty();
    for (uint8_t piece = CLR_PIECE_NONE; piece <= CLR_PIECE_L; piece++) {
        window.counts[piece] = gpu_window.counts[piece];
    }
    window.total_count = gpu_window.total_count;
    window.exact_count = gpu_window.exact_count;
    return window;
}

bool clearra_gpu_piece_multiset_window_is_valid(
    const clr_gpu_piece_multiset_window *window) {
    uint16_t counted = 0u;
    if (window == 0 || window->total_count == 0u ||
        window->total_count > CLEARRA_GPU_BATCH_MAX_PIECES ||
        window->exact_count > window->total_count ||
        window->counts[CLR_PIECE_NONE] != 0u) {
        return false;
    }
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; piece++) {
        counted = (uint16_t)(counted + window->counts[piece]);
    }
    return counted == window->total_count;
}

ClearraGpuStatus clearra_gpu_batch_descriptor_piece_multiset_window(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_gpu_piece_multiset_window *out_window) {
    if (batch == 0 || out_window == 0 ||
        clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    *out_window = batch->piece_multiset_window;
    return CLEARRA_GPU_OK;
}

ClearraGpuStatus clearra_gpu_piece_source_from_batch(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_piece_source_descriptor *out_source) {
    if (batch == 0 || out_source == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    switch (batch->piece_source_kind) {
        case CLEARRA_GPU_PIECE_SOURCE_FIXED_SEQUENCE:
            *out_source = clearra_piece_source_descriptor_fixed_queue(
                batch->piece_source_id,
                CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
                batch->piece_multiset_window.total_count,
                CLR_PIECE_SET_STANDARD_TETROMINOES);
            return CLEARRA_GPU_OK;
        case CLEARRA_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN:
            *out_source = clearra_piece_source_descriptor_bag_universe(
                batch->piece_source_id,
                CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
                CLR_PIECE_SET_STANDARD_TETROMINOES);
            return CLEARRA_GPU_OK;
        case CLEARRA_GPU_PIECE_SOURCE_OBSERVED_WINDOW:
            *out_source = clearra_piece_source_descriptor_observed_window(
                batch->piece_source_id,
                CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED,
                CLR_PIECE_SET_STANDARD_TETROMINOES,
                true,
                CLR_SUPPLY_TRUNCATION_NONE);
            return CLEARRA_GPU_OK;
        default:
            return CLEARRA_GPU_INVALID_ARGUMENT;
    }
}

ClearraGpuStatus clearra_gpu_batch_descriptor_product_source_of_truth(
    const ClearraGpuPackingBatchDescriptor *batch,
    uint64_t *out_piece_source_id,
    uint64_t *out_pattern_universe_id,
    uint64_t *out_pattern_weight_model_id,
    clr_gpu_piece_multiset_window *out_window) {
    if (batch == 0 || out_piece_source_id == 0 ||
        out_pattern_universe_id == 0 || out_pattern_weight_model_id == 0 ||
        out_window == 0 ||
        clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    *out_piece_source_id = batch->piece_source_id;
    *out_pattern_universe_id = batch->pattern_universe_id;
    *out_pattern_weight_model_id = batch->pattern_weight_model_id;
    *out_window = batch->piece_multiset_window;
    return CLEARRA_GPU_OK;
}

ClearraGpuStatus clearra_gpu_batch_descriptor_to_packing_problem(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_packing_problem *out_problem) {
    if (batch == 0 || out_problem == 0 ||
        clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraBoard64Layout layout;
    if (clearra_gpu_batch_descriptor_layout(batch, &layout) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    uint64_t target_mask = 0u;
    if (clearra_packing_target_mask_for_lines(
            layout, batch->active_packing_rows, &target_mask) !=
        CLEARRA_PACKING_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = batch->piece_window;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.initial_mask = batch->initial_board_mask;
    problem.board.initial_mask_hi = 0u;
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = target_mask;
    problem.required_fill_mask = target_mask & ~batch->initial_board_mask;
    problem.forbidden_mask = 0u;
    problem.exact_pieces = batch->exact_piece_count;
    problem.piece_window = clearra_piece_window_descriptor(
        batch->piece_window,
        batch->exact_piece_count,
        batch->exact_piece_count != 0u);
    problem.piece_multiset_window =
        clearra_gpu_piece_multiset_window_to_c(batch->piece_multiset_window);
    if (clearra_gpu_piece_source_from_batch(batch, &problem.piece_source) !=
        CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    problem.piece_source.pattern_universe_id = batch->pattern_universe_id;
    problem.piece_source.pattern_weight_model_id =
        batch->pattern_weight_model_id;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = (uint32_t)batch->rule_profile_id;
    problem.rule.kick_profile_id = (uint32_t)batch->kick_profile_id;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.budget.max_results = batch->candidate_capacity;
    problem.budget.max_frontier_states = batch->max_frontier_states;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    *out_problem = problem;
    return CLEARRA_GPU_OK;
}
