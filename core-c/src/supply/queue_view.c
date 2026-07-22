#include "clr_supply.h"
static uint32_t provenance_for_mode(uint8_t mode) {
    if (mode == CLR_QUEUE_FIXED_SEQUENCE) {
        return CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE;
    }
    if (mode == CLR_QUEUE_BAG_ALIGNED_PATTERN) {
        return CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN;
    }
    if (mode == CLR_QUEUE_OBSERVED) {
        return CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED;
    }
    return 0u;
}clr_queue_view clearra_queue_view_empty(uint8_t mode, uint32_t provenance_id) {
    clr_queue_view view = {0};
    view.mode = mode;
    view.provenance_id = provenance_id;
    return view;
}bool clearra_queue_view_is_fixed_sequence(const clr_queue_view *view) {
    return view != 0 && view->mode == CLR_QUEUE_FIXED_SEQUENCE &&
           view->provenance_id == CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE;
}bool clearra_queue_view_is_bag_pattern(const clr_queue_view *view) {
    return view != 0 && view->mode == CLR_QUEUE_BAG_ALIGNED_PATTERN &&
           view->provenance_id == CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN;
}bool clearra_queue_view_is_observed_rust_expanded(const clr_queue_view *view) {
    return view != 0 && view->mode == CLR_QUEUE_OBSERVED &&
           view->provenance_id == CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED;
}bool clearra_queue_view_preserves_provenance(
    const clr_queue_view *view,
    uint32_t expected_provenance_id) {
    return view != 0 && view->provenance_id == expected_provenance_id &&
           view->provenance_id == provenance_for_mode(view->mode);
}bool clearra_supply_profile_is_supported(uint32_t bag_profile_id) {
    return bag_profile_id == CLR_SUPPLY_PROFILE_STANDARD_7_BAG ||
           bag_profile_id == CLR_SUPPLY_PROFILE_FIXED_SEQUENCE ||
           bag_profile_id == CLR_SUPPLY_PROFILE_OBSERVED_STANDARD_7_BAG;
}clr_supply_identity_descriptor clearra_supply_identity_descriptor(
    uint32_t supply_provenance_id,
    uint32_t bag_profile_id,
    uint32_t piece_set_id,
    uint32_t observed_window_id,
    uint8_t bag_boundary_evidence,
    bool duplicate_witness,
    bool ambiguity_report) {
    clr_supply_identity_descriptor descriptor = {0};
    descriptor.supply_provenance_id = supply_provenance_id;
    descriptor.bag_profile_id = clearra_supply_profile_is_supported(bag_profile_id)
                                    ? bag_profile_id
                                    : CLR_SUPPLY_PROFILE_UNSUPPORTED;
    descriptor.piece_set_id = piece_set_id;
    descriptor.observed_window_id = observed_window_id;
    descriptor.bag_boundary_evidence = bag_boundary_evidence;
    descriptor.duplicate_witness = duplicate_witness ? 1u : 0u;
    descriptor.ambiguity_report = ambiguity_report ? 1u : 0u;
    return descriptor;
}bool clearra_supply_identity_descriptor_is_cache_key_material(
    const clr_supply_identity_descriptor *descriptor) {
    return descriptor != 0 && descriptor->supply_provenance_id != 0u &&
           clearra_supply_profile_is_supported(descriptor->bag_profile_id) &&
           descriptor->piece_set_id != 0u;
}
