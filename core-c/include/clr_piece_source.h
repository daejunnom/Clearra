#ifndef CLR_PIECE_SOURCE_H
#define CLR_PIECE_SOURCE_H

#include <stdbool.h>
#include <stdint.h>

#define CLR_PIECE_SOURCE_FIXED_QUEUE 1u
#define CLR_PIECE_SOURCE_BAG_UNIVERSE 2u
#define CLR_PIECE_SOURCE_OBSERVED_WINDOW 3u
#define CLR_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE 4u

#define CLR_SUPPLY_TRUNCATION_NONE 0u
#define CLR_SUPPLY_TRUNCATION_OBSERVED_WINDOW_BUDGET_EXCEEDED 1u
#define CLR_SUPPLY_TRUNCATION_MATERIALIZED_PATTERN_BUDGET_EXCEEDED 2u
#define CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY 64u
typedef struct clr_piece_source_descriptor {
    uint64_t piece_source_id;
    uint32_t source_kind;
    uint32_t provenance_id;
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    uint32_t materialized_pattern_count;
    uint16_t fixed_sequence_len;
    uint8_t piece_set_profile_id;
    uint8_t complete;
    uint16_t truncation_reason;
    uint8_t exact_bag_automaton_supported;
    uint8_t reserved[5];
} clr_piece_source_descriptor;typedef struct clr_piece_source_pattern_reader {
    clr_piece_source_descriptor source;
    uint32_t pattern_id;
    const uint8_t *fixed_or_materialized_pieces;
    uint16_t len;
    uint8_t complete;
    uint16_t truncation_reason;
} clr_piece_source_pattern_reader;clr_piece_source_descriptor clearra_piece_source_descriptor_empty(void);
clr_piece_source_descriptor clearra_piece_source_descriptor_fixed_queue(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint16_t fixed_sequence_len,
    uint8_t piece_set_profile_id);
clr_piece_source_descriptor clearra_piece_source_descriptor_bag_universe(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint8_t piece_set_profile_id);
clr_piece_source_descriptor clearra_piece_source_descriptor_observed_window(
    uint64_t piece_source_id,
    uint32_t provenance_id,
    uint8_t piece_set_profile_id,
    bool complete,
    uint16_t truncation_reason);
bool clearra_piece_source_descriptor_valid(
    const clr_piece_source_descriptor *descriptor);
bool clearra_piece_source_descriptor_is_complete(
    const clr_piece_source_descriptor *descriptor);
bool clearra_piece_source_descriptor_is_cache_material(
    const clr_piece_source_descriptor *descriptor);
#endif
