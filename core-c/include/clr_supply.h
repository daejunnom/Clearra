#ifndef CLR_SUPPLY_H
#define CLR_SUPPLY_H

#include <stdbool.h>
#include <stdint.h>

#define CLR_QUEUE_VIEW_CAPACITY 64u

#define CLR_QUEUE_FIXED_SEQUENCE 1u
#define CLR_QUEUE_BAG_ALIGNED_PATTERN 2u
#define CLR_QUEUE_OBSERVED 3u

#define CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE 4097u
#define CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN 8193u
#define CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED 12289u

#define CLR_SUPPLY_PROFILE_UNSUPPORTED 0u
#define CLR_SUPPLY_PROFILE_STANDARD_7_BAG 1u
#define CLR_SUPPLY_PROFILE_FIXED_SEQUENCE 2u
#define CLR_SUPPLY_PROFILE_OBSERVED_STANDARD_7_BAG 3u

#define CLR_SUPPLY_BOUNDARY_NOT_EVALUATED 0u
#define CLR_SUPPLY_BOUNDARY_FIXED 1u
#define CLR_SUPPLY_BOUNDARY_OBSERVED_COMPATIBLE 2u
#define CLR_SUPPLY_BOUNDARY_OBSERVED_AMBIGUOUS 3u
#define CLR_SUPPLY_BOUNDARY_DUPLICATE_REJECTED 4u
typedef struct clr_queue_view {
    uint8_t mode;
    uint8_t truncated;
    uint16_t len;
    uint16_t stored_len;
    uint16_t reserved;
    uint32_t provenance_id;
    uint8_t pieces[CLR_QUEUE_VIEW_CAPACITY];
} clr_queue_view;typedef struct clr_supply_identity_descriptor {
    uint32_t supply_provenance_id;
    uint32_t bag_profile_id;
    uint32_t piece_set_id;
    uint32_t observed_window_id;
    uint8_t bag_boundary_evidence;
    uint8_t duplicate_witness;
    uint8_t ambiguity_report;
    uint8_t reserved;
} clr_supply_identity_descriptor;typedef struct clr_hold_state {
    uint8_t enabled;
    uint8_t empty;
    uint8_t piece;
    uint8_t reserved;
} clr_hold_state;clr_queue_view clearra_queue_view_empty(uint8_t mode, uint32_t provenance_id);
bool clearra_queue_view_is_fixed_sequence(const clr_queue_view *view);
bool clearra_queue_view_is_bag_pattern(const clr_queue_view *view);
bool clearra_queue_view_is_observed_rust_expanded(const clr_queue_view *view);
bool clearra_queue_view_preserves_provenance(
    const clr_queue_view *view,
    uint32_t expected_provenance_id);clr_supply_identity_descriptor clearra_supply_identity_descriptor(
    uint32_t supply_provenance_id,
    uint32_t bag_profile_id,
    uint32_t piece_set_id,
    uint32_t observed_window_id,
    uint8_t bag_boundary_evidence,
    bool duplicate_witness,
    bool ambiguity_report);
bool clearra_supply_profile_is_supported(uint32_t bag_profile_id);
bool clearra_supply_identity_descriptor_is_cache_key_material(
    const clr_supply_identity_descriptor *descriptor);clr_hold_state clearra_hold_state_empty(uint8_t enabled);
clr_hold_state clearra_hold_state_occupied(uint8_t piece);
bool clearra_hold_state_has_piece(const clr_hold_state *hold);
#endif
