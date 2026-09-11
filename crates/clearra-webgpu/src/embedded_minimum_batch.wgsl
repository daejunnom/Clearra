// Local minimum-cover A/B only. No global barrier or hardware subgroup assumption.
// Each dispatch reads an immutable state snapshot; a second dispatch seals the round.
struct Params {
    candidates: u32, states: u32, groups: u32, constraints: u32,
    mode: u32, reserved0: u32, reserved1: u32, reserved2: u32,
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> matrix_words: array<u32>;
@group(0) @binding(2) var<storage, read> input_words: array<u32>;
@group(0) @binding(3) var<storage, read_write> output_words: array<atomic<u32>>;

fn span() -> u32 { return params.candidates * params.groups; }
fn candidate_at(index: u32) -> u32 { return matrix_words[2u * params.constraints + 1u + index]; }
fn fail(group: u32, bits: u32) {
    // Experimental on/off control: a zero OR carries no information, but can
    // serialize thousands of invocations on the same global word.
    if (params.reserved0 == 0u || bits != 0u) {
        atomicOr(&output_words[2u * span() + group], bits);
    }
}
fn active_mask(group: u32) -> u32 {
    let remaining = params.states - 32u * group;
    if (remaining >= 32u) { return 0xffffffffu; }
    return (1u << remaining) - 1u;
}

fn flat(state: u32) {
    if (state >= params.states) { return; }
    let group = state / 32u;
    let bit = 1u << (state % 32u);
    for (var row = 0u; row < params.constraints; row++) {
        let begin = matrix_words[row];
        let end = matrix_words[row + 1u];
        let demand = matrix_words[params.constraints + 1u + row];
        var chosen = 0u;
        var available = 0u;
        for (var cursor = begin; cursor < end; cursor++) {
            let index = candidate_at(cursor) * params.groups + group;
            chosen += select(0u, 1u, (input_words[index] & bit) != 0u);
            available += select(0u, 1u, (input_words[span() + index] & bit) == 0u);
        }
        if (available < demand) { fail(group, bit); }
        if (chosen < demand && available == demand) {
            for (var cursor = begin; cursor < end; cursor++) {
                let index = candidate_at(cursor) * params.groups + group;
                atomicOr(&output_words[index], bit & ~input_words[span() + index]);
            }
        }
    }
}

fn sliced(row: u32, group: u32) {
    if (row >= params.constraints || group >= params.groups) { return; }
    let begin = matrix_words[row];
    let end = matrix_words[row + 1u];
    let demand = matrix_words[params.constraints + 1u + row];
    var one = 0u;
    var two = 0u;
    var three = 0u;
    var chosen_one = 0u;
    var chosen_two = 0u;
    for (var cursor = begin; cursor < end; cursor++) {
        let index = candidate_at(cursor) * params.groups + group;
        let possible = ~input_words[span() + index];
        three |= two & possible;
        two |= one & possible;
        one |= possible;
        chosen_two |= chosen_one & input_words[index];
        chosen_one |= input_words[index];
    }
    var possible = one;
    var satisfied = chosen_one;
    var exactly = one & ~two;
    if (demand == 2u) {
        possible = two;
        satisfied = chosen_two;
        exactly = two & ~three;
    }
    let live_bits = active_mask(group);
    fail(group, ~possible & live_bits);
    let forced = exactly & ~satisfied & live_bits;
    if (forced != 0u) {
        for (var cursor = begin; cursor < end; cursor++) {
            let index = candidate_at(cursor) * params.groups + group;
            atomicOr(&output_words[index], forced & ~input_words[span() + index]);
        }
    }
}

fn seal(state: u32) {
    if (state >= params.states) { return; }
    let group = state / 32u;
    let bit = 1u << (state % 32u);
    var count = 0u;
    for (var candidate = 0u; candidate < params.candidates; candidate++) {
        let index = candidate * params.groups + group;
        let chosen = atomicLoad(&output_words[index]);
        count += select(0u, 1u, (chosen & bit) != 0u);
        if ((chosen & input_words[span() + index] & bit) != 0u) { fail(group, bit); }
    }
    let limit = input_words[2u * span() + state];
    if (count > limit) { fail(group, bit); }
    if (count == limit) {
        for (var candidate = 0u; candidate < params.candidates; candidate++) {
            let index = candidate * params.groups + group;
            atomicOr(&output_words[span() + index], bit & ~atomicLoad(&output_words[index]));
        }
    }
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (params.mode == 2u) { seal(id.x); }
    else if (params.mode == 1u) {
        if (params.reserved1 != 0u) { sliced(id.y, id.x); }
        else { sliced(id.x, id.y); }
    }
    else { flat(id.x); }
}
