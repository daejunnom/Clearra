// Reviewed Clearra postprocess shader.
// shader_version=pattern-bitset-union-webgpu-v1
// Input is a dense row-major matrix of u32 PatternBitSet words.

struct BatchParams {
    word_count: u32,
    row_count: u32,
    reserved_a: u32,
    reserved_b: u32,
}

@group(0) @binding(0)
var<storage, read> rows: array<u32>;

@group(0) @binding(1)
var<storage, read_write> union_words: array<u32>;

@group(0) @binding(2)
var<uniform> params: BatchParams;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let word_index = invocation.x;
    if (word_index >= params.word_count) {
        return;
    }

    var value = 0u;
    for (var row_index = 0u; row_index < params.row_count; row_index += 1u) {
        value |= rows[row_index * params.word_count + word_index];
    }
    union_words[word_index] = value;
}
