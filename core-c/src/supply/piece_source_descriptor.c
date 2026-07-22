#include "clr_piece.h"
#include "clr_problem.h"
clr_piece_source_descriptor clearra_piece_source_descriptor_empty(void) {
    clr_piece_source_descriptor descriptor = {0};
    return descriptor;
}clr_piece_source_descriptor clearra_piece_source_descriptor_fixed_queue(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint16_t fixed_sequence_len,
    uint8_t piece_set_profile_id) {
    clr_piece_source_descriptor descriptor = clearra_piece_source_descriptor_empty();
    descriptor.piece_source_id = piece_source_id;
    descriptor.source_kind = CLR_PIECE_SOURCE_FIXED_QUEUE;
    descriptor.provenance_id = provenance_id;
    descriptor.fixed_sequence_len = fixed_sequence_len;
    descriptor.piece_set_profile_id = piece_set_profile_id;
    descriptor.complete = 1u;
    descriptor.truncation_reason = CLR_SUPPLY_TRUNCATION_NONE;
    return descriptor;
}clr_piece_source_descriptor clearra_piece_source_descriptor_bag_universe(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint8_t piece_set_profile_id) {
    clr_piece_source_descriptor descriptor = clearra_piece_source_descriptor_empty();
    descriptor.piece_source_id = piece_source_id;
    descriptor.source_kind = CLR_PIECE_SOURCE_BAG_UNIVERSE;
    descriptor.provenance_id = provenance_id;
    descriptor.piece_set_profile_id = piece_set_profile_id;
    descriptor.complete = 1u;
    descriptor.truncation_reason = CLR_SUPPLY_TRUNCATION_NONE;
    return descriptor;
}clr_piece_source_descriptor clearra_piece_source_descriptor_observed_window(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint8_t piece_set_profile_id,
    bool complete,
    uint16_t truncation_reason) {
    clr_piece_source_descriptor descriptor = clearra_piece_source_descriptor_empty();
    descriptor.piece_source_id = piece_source_id;
    descriptor.source_kind = CLR_PIECE_SOURCE_OBSERVED_WINDOW;
    descriptor.provenance_id = provenance_id;
    descriptor.piece_set_profile_id = piece_set_profile_id;
    descriptor.complete = complete ? 1u : 0u;
    descriptor.truncation_reason =
        complete ? CLR_SUPPLY_TRUNCATION_NONE : truncation_reason;
    return descriptor;
}bool clearra_piece_source_descriptor_valid(
    const clr_piece_source_descriptor *descriptor) {
    if (descriptor == 0) {
        return false;
    }
    if (descriptor->piece_source_id == 0u || descriptor->piece_set_profile_id == 0u) {
        return false;
    }
    if (descriptor->source_kind < CLR_PIECE_SOURCE_FIXED_QUEUE ||
        descriptor->source_kind > CLR_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE) {
        return false;
    }
    if (descriptor->source_kind == CLR_PIECE_SOURCE_FIXED_QUEUE &&
        descriptor->fixed_sequence_len == 0u) {
        return false;
    }
    if (descriptor->source_kind == CLR_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE &&
        (descriptor->pattern_universe_id == 0u ||
         descriptor->pattern_weight_model_id == 0u ||
         descriptor->materialized_pattern_count == 0u)) {
        return false;
    }
    if (descriptor->complete != 0u && descriptor->complete != 1u) {
        return false;
    }
    if (descriptor->exact_bag_automaton_supported != 0u &&
        descriptor->exact_bag_automaton_supported != 1u) {
        return false;
    }
    return true;
}bool clearra_piece_source_descriptor_is_complete(
    const clr_piece_source_descriptor *descriptor) {
    return descriptor != 0 && descriptor->complete == 1u &&
           descriptor->truncation_reason == CLR_SUPPLY_TRUNCATION_NONE;
}bool clearra_piece_source_descriptor_is_cache_material(
    const clr_piece_source_descriptor *descriptor) {
    return clearra_piece_source_descriptor_valid(descriptor) &&
           descriptor->provenance_id != 0u;
}clr_buildup_status clr_piece_source_pattern_piece_at(
    const clr_piece_source_pattern_reader *reader,
    const clr_hold_automaton_state *state,
    uint16_t cursor,
    uint8_t *out_piece) {
    if (reader == 0 || state == 0 || out_piece == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (!clearra_piece_source_descriptor_valid(&reader->source) ||
        state->piece_source_id != reader->source.piece_source_id) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (reader->source.provenance_id != 0u &&
        state->provenance_id != (uint64_t)reader->source.provenance_id) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (reader->complete == 0u) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (reader->source.source_kind == CLR_PIECE_SOURCE_FIXED_QUEUE &&
        reader->source.fixed_sequence_len > reader->len) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (reader->len > CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (reader->len > 0u && reader->fixed_or_materialized_pieces == 0) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (cursor >= reader->len) {
        return CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE;
    }

    uint8_t piece = reader->fixed_or_materialized_pieces[cursor];
    if (piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    *out_piece = piece;
    return CLR_BUILDUP_OK;
}
