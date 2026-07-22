// Reviewed Clearra packing shader.
// shader_version=geometry-exact-cover-webgpu-v2
// Packing only: no queue, hold, reachability, scoring, spin, or replay state.

struct FrontierState {
    occupied_lo: u32,
    occupied_hi: u32,
    used_counts: u32,
    family_counts_and_depth: u32,
}

struct TraceRecord {
    parent_index: u32,
    operation_index: u32,
}

struct OutputCounters {
    count: atomic<u32>,
    overflow: atomic<u32>,
    reserved_a: u32,
    reserved_b: u32,
}

struct BatchParams {
    current_count: u32,
    operation_count: u32,
    output_capacity: u32,
    cell_count: u32,
    board_width: u32,
    reserved_pivot_policy: u32,
    target_depth: u32,
    reserved_family_counts: u32,
    required_lo: u32,
    required_hi: u32,
    goal_lo: u32,
    goal_hi: u32,
    forbidden_lo: u32,
    forbidden_hi: u32,
    parent_index_base: u32,
    reserved_b: u32,
}

struct CertifiedConstraints {
    words: array<u32, 452>,
}

@group(0) @binding(0)
var<storage, read> frontier: array<FrontierState>;
@group(0) @binding(1)
var<storage, read> skeleton_masks: array<vec2<u32>>;
@group(0) @binding(2)
var<storage, read> skeleton_piece_kinds: array<u32>;
@group(0) @binding(3)
var<storage, read> support_offsets: array<u32>;
@group(0) @binding(4)
var<storage, read> support_operations: array<u32>;
@group(0) @binding(5)
var<storage, read_write> next_frontier: array<FrontierState>;
@group(0) @binding(6)
var<storage, read_write> next_trace: array<TraceRecord>;
@group(0) @binding(7)
var<storage, read_write> counters: OutputCounters;
@group(0) @binding(8)
var<uniform> params: BatchParams;
@group(0) @binding(9)
var<uniform> certified_constraints: CertifiedConstraints;

fn cell_is_set(lo: u32, hi: u32, cell: u32) -> bool {
    if (cell < 32u) {
        return (lo & (1u << cell)) != 0u;
    }
    return (hi & (1u << (cell - 32u))) != 0u;
}

fn masks_intersect(a_lo: u32, a_hi: u32, b_lo: u32, b_hi: u32) -> bool {
    return ((a_lo & b_lo) | (a_hi & b_hi)) != 0u;
}

fn skeleton_is_usable(state: FrontierState, skeleton_index: u32) -> bool {
    let piece = skeleton_piece_kinds[skeleton_index];
    let mask = skeleton_masks[skeleton_index];
    if (piece == 0u || piece > 7u) {
        return false;
    }
    let shift = (piece - 1u) * 4u;
    let used = (state.used_counts >> shift) & 15u;
    let desired = (state.family_counts_and_depth >> (shift + 4u)) & 15u;
    if (used >= desired) {
        return false;
    }
    if (masks_intersect(
        state.occupied_lo,
        state.occupied_hi,
        mask.x,
        mask.y,
    )) {
        return false;
    }
    if (masks_intersect(
        params.forbidden_lo,
        params.forbidden_hi,
        mask.x,
        mask.y,
    )) {
        return false;
    }
    return ((mask.x & ~params.goal_lo) |
            (mask.y & ~params.goal_hi)) == 0u;
}

fn missing_required(state: FrontierState, cell: u32) -> bool {
    return cell_is_set(params.required_lo, params.required_hi, cell) &&
           !cell_is_set(state.occupied_lo, state.occupied_hi, cell);
}

fn certified_constraints_enabled() -> bool {
    return (certified_constraints.words[0] & 1u) != 0u;
}

fn safe_separator_column(column: u32) -> bool {
    if (column < 32u) {
        return (certified_constraints.words[1] & (1u << column)) != 0u;
    }
    return (certified_constraints.words[2] & (1u << (column - 32u))) != 0u;
}

fn missing_column_count(state: FrontierState, column: u32) -> u32 {
    var count = 0u;
    for (var cell = column; cell < params.cell_count; cell += params.board_width) {
        if (missing_required(state, cell)) {
            count += 1u;
        }
    }
    return count;
}

fn region_missing_count(state: FrontierState, separator: u32, left: bool) -> u32 {
    var count = 0u;
    for (var cell = 0u; cell < params.cell_count; cell += 1u) {
        let x = cell % params.board_width;
        if (missing_required(state, cell) &&
            ((left && x < separator) || (!left && x > separator))) {
            count += 1u;
        }
    }
    return count;
}

fn bumper_row_compatible(
    state: FrontierState,
    bumper_cell: u32,
    mask: vec2<u32>,
) -> bool {
    let separator = bumper_cell % params.board_width;
    var separator_cells = 0u;
    var left_demand = 0u;
    var right_demand = 0u;
    var left_supply = 0u;
    var right_supply = 0u;
    for (var cell = 0u; cell < params.cell_count; cell += 1u) {
        let x = cell % params.board_width;
        let in_row = cell_is_set(mask.x, mask.y, cell);
        if (x == separator && in_row) {
            separator_cells += 1u;
        }
        if (x < separator) {
            if (missing_required(state, cell)) {
                left_demand += 1u;
            }
            if (in_row) {
                left_supply += 1u;
            }
        } else if (x > separator) {
            if (missing_required(state, cell)) {
                right_demand += 1u;
            }
            if (in_row) {
                right_supply += 1u;
            }
        }
    }
    return separator_cells == 1u &&
           left_supply <= left_demand && right_supply <= right_demand &&
           ((left_demand - left_supply) & 3u) == 0u &&
           ((right_demand - right_supply) & 3u) == 0u;
}

fn skeleton_is_usable_for_pivot(
    state: FrontierState,
    skeleton_index: u32,
    bumper_cell: u32,
) -> bool {
    if (!skeleton_is_usable(state, skeleton_index)) {
        return false;
    }
    if (bumper_cell == 0xffffffffu) {
        return true;
    }
    return bumper_row_compatible(state, bumper_cell, skeleton_masks[skeleton_index]);
}

fn residual_constraints_allow(state: FrontierState) -> bool {
    if (!certified_constraints_enabled()) {
        return true;
    }
    for (var column = 0u; column < params.board_width; column += 1u) {
        let demand = missing_column_count(state, column);
        var minimum = 0u;
        var maximum = 0u;
        for (var piece = 0u; piece < 7u; piece += 1u) {
            let shift = piece * 4u;
            let used = (state.used_counts >> shift) & 15u;
            let desired = (state.family_counts_and_depth >> (shift + 4u)) & 15u;
            let remaining_count = desired - used;
            let bounds = certified_constraints.words[
                4u + piece * params.board_width + column
            ];
            minimum += remaining_count * (bounds & 255u);
            maximum += remaining_count * ((bounds >> 8u) & 255u);
        }
        if (demand < minimum || demand > maximum) {
            return false;
        }
    }

    if ((certified_constraints.words[0] & 2u) != 0u) {
        var checker_delta = 0i;
        for (var cell = 0u; cell < params.cell_count; cell += 1u) {
            if (!missing_required(state, cell)) {
                continue;
            }
            let x = cell % params.board_width;
            let y = cell / params.board_width;
            checker_delta += select(-1i, 1i, ((x + y) & 1u) == 0u);
        }
        let t_shift = 2u * 4u;
        let t_used = (state.used_counts >> t_shift) & 15u;
        let t_desired = (state.family_counts_and_depth >> (t_shift + 4u)) & 15u;
        let remaining_t = i32(t_desired - t_used);
        let scaled = abs(checker_delta) / 2i;
        if ((checker_delta & 1i) != 0i || scaled > remaining_t ||
            ((remaining_t - scaled) & 1i) != 0i) {
            return false;
        }
    }
    return true;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let state_index = invocation.x;
    if (state_index >= params.current_count) {
        return;
    }
    let state = frontier[state_index];
    if ((state.family_counts_and_depth & 15u) >= params.target_depth) {
        return;
    }

    if (!certified_constraints_enabled()) {
        var base_selected_cell = 0xffffffffu;
        var base_selected_support = 0xffffffffu;
        for (var cell = 0u; cell < params.cell_count; cell += 1u) {
            if (!missing_required(state, cell)) {
                continue;
            }
            var support_count = 0u;
            let begin = support_offsets[cell];
            let end = support_offsets[cell + 1u];
            for (var cursor = begin; cursor < end; cursor += 1u) {
                let operation_index = support_operations[cursor];
                if (operation_index < params.operation_count &&
                    skeleton_is_usable(state, operation_index)) {
                    support_count += 1u;
                }
            }
            if (support_count < base_selected_support) {
                base_selected_support = support_count;
                base_selected_cell = cell;
            }
        }
        if (base_selected_cell == 0xffffffffu || base_selected_support == 0u) {
            return;
        }

        let base_begin = support_offsets[base_selected_cell];
        let base_end = support_offsets[base_selected_cell + 1u];
        for (var cursor = base_begin; cursor < base_end; cursor += 1u) {
            let operation_index = support_operations[cursor];
            if (operation_index >= params.operation_count ||
                !skeleton_is_usable(state, operation_index)) {
                continue;
            }
            let output_index = atomicAdd(&counters.count, 1u);
            if (output_index >= params.output_capacity) {
                atomicStore(&counters.overflow, 1u);
                continue;
            }
            let piece = skeleton_piece_kinds[operation_index];
            let mask = skeleton_masks[operation_index];
            let shift = (piece - 1u) * 4u;
            var child = state;
            child.occupied_lo |= mask.x;
            child.occupied_hi |= mask.y;
            child.used_counts += 1u << shift;
            child.family_counts_and_depth += 1u;
            next_frontier[output_index] = child;
            next_trace[output_index] = TraceRecord(
                params.parent_index_base + state_index,
                operation_index,
            );
        }
        return;
    }

    var separator_column = 0xffffffffu;
    var separator_left = false;
    var separator_owner_count = 0xffffffffu;
    var bumper_cell = 0xffffffffu;
    let residual_cell_count = countOneBits(params.required_lo & ~state.occupied_lo) +
                              countOneBits(params.required_hi & ~state.occupied_hi);
    for (var column = 0u; column < params.board_width; column += 1u) {
        if (!safe_separator_column(column)) {
            continue;
        }
        let column_missing = missing_column_count(state, column);
        if (column_missing == 0u) {
            let left_count = region_missing_count(state, column, true);
            let right_count = region_missing_count(state, column, false);
            if (left_count != 0u && right_count != 0u) {
                let owner_count = min(left_count, right_count);
                if (owner_count < separator_owner_count) {
                    separator_owner_count = owner_count;
                    separator_column = column;
                    separator_left = left_count <= right_count;
                }
            }
        } else if (column_missing == 1u && residual_cell_count <= 24u) {
            let top_cell = (params.cell_count / params.board_width - 1u) *
                           params.board_width + column;
            if (missing_required(state, top_cell)) {
                bumper_cell = top_cell;
            }
        }
    }

    var selected_cell = 0xffffffffu;
    var selected_support = 0xffffffffu;
    for (var cell = 0u; cell < params.cell_count; cell += 1u) {
        if (!missing_required(state, cell)) {
            continue;
        }
        if (separator_column != 0xffffffffu) {
            let x = cell % params.board_width;
            if ((separator_left && x >= separator_column) ||
                (!separator_left && x <= separator_column)) {
                continue;
            }
        }
        var support_count = 0u;
        let begin = support_offsets[cell];
        let end = support_offsets[cell + 1u];
        for (var cursor = begin; cursor < end; cursor += 1u) {
            let operation_index = support_operations[cursor];
            let active_bumper = select(0xffffffffu, bumper_cell, cell == bumper_cell);
            if (operation_index < params.operation_count &&
                skeleton_is_usable_for_pivot(state, operation_index, active_bumper)) {
                support_count += 1u;
            }
        }
        if (support_count < selected_support) {
            selected_support = support_count;
            selected_cell = cell;
        }
    }
    if (selected_cell == 0xffffffffu || selected_support == 0u) {
        return;
    }

    let begin = support_offsets[selected_cell];
    let end = support_offsets[selected_cell + 1u];
    for (var cursor = begin; cursor < end; cursor += 1u) {
        let operation_index = support_operations[cursor];
        if (operation_index >= params.operation_count) {
            continue;
        }
        let active_bumper = select(0xffffffffu, bumper_cell, selected_cell == bumper_cell);
        if (!skeleton_is_usable_for_pivot(state, operation_index, active_bumper)) {
            continue;
        }
        let piece = skeleton_piece_kinds[operation_index];
        let mask = skeleton_masks[operation_index];
        let shift = (piece - 1u) * 4u;
        var child = state;
        child.occupied_lo |= mask.x;
        child.occupied_hi |= mask.y;
        child.used_counts += 1u << shift;
        child.family_counts_and_depth += 1u;
        if (!residual_constraints_allow(child)) {
            continue;
        }
        let output_index = atomicAdd(&counters.count, 1u);
        if (output_index >= params.output_capacity) {
            atomicStore(&counters.overflow, 1u);
            continue;
        }
        next_frontier[output_index] = child;
        next_trace[output_index] = TraceRecord(
            params.parent_index_base + state_index,
            operation_index,
        );
    }
}
