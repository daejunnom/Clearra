#include "candidate_tests_support.h"

void harddrop_candidate_matches_fixture(void);
void harddrop_candidate_count_fixture(void);
void harddrop_candidate_rejects_blocked_fall_path(void);
void harddrop_o_piece_uses_canonical_rotation_fixture(void);
void locked_candidate_matches_fixture(void);
void locked_candidate_count_fixture(void);
void locked_candidate_uses_reverse_graph_not_harddrop_alias(void);
void locked_candidate_rejects_collision_free_unreachable_placement(void);
void locked180_candidate_matches_fixture(void);
void locked180_candidate_count_fixture(void);
void srs_plus_i_half_turn_displacements_match_tetrio_fixture(void);
void locked180_candidate_finds_half_turn_only_placement(void);
void unreachable_placement_reject_fixture(void);
void kick_first_success_ordering_fixture(void);
void kick_first_success_prefers_earliest_valid_offset_fixture(void);
void rotation_transition_correctness_fixture(void);
void candidate_cache_key_is_mode_scoped(void);
void candidate_cache_key_includes_board_rule_piece(void);
void duplicate_candidate_removed(void);

int main(void) {
    harddrop_candidate_matches_fixture();
    harddrop_candidate_count_fixture();
    harddrop_candidate_rejects_blocked_fall_path();
    harddrop_o_piece_uses_canonical_rotation_fixture();
    locked_candidate_matches_fixture();
    locked_candidate_count_fixture();
    locked_candidate_uses_reverse_graph_not_harddrop_alias();
    locked_candidate_rejects_collision_free_unreachable_placement();
    locked180_candidate_matches_fixture();
    locked180_candidate_count_fixture();
    srs_plus_i_half_turn_displacements_match_tetrio_fixture();
    locked180_candidate_finds_half_turn_only_placement();
    unreachable_placement_reject_fixture();
    kick_first_success_ordering_fixture();
    kick_first_success_prefers_earliest_valid_offset_fixture();
    rotation_transition_correctness_fixture();
    candidate_cache_key_is_mode_scoped();
    candidate_cache_key_includes_board_rule_piece();
    duplicate_candidate_removed();
    puts("core-c candidate tests passed");
    return 0;
}
