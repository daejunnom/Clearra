# This file is dot-sourced by an architecture validation wrapper.
# Keep the grouped validation functions side-effect free at load time.
function Invoke-CSfinderCandidateValidation() {
foreach ($requiredPath in @(
        "core-c/src/candidate/candidate.h",
        "core-c/src/candidate/candidate_search_dispatch.c",
        "core-c/src/candidate/harddrop_candidate.c",
        "core-c/src/candidate/locked_candidate.c",
        "core-c/src/candidate/candidate_cache.c",
        "core-c/tests/candidate_tests.c",
        "tests/fixtures/packing/harddrop_candidates.json",
        "tests/fixtures/packing/locked_candidates.json",
        "tests/fixtures/packing/locked180_candidates.json"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M8 Sfinder-compatible candidate required file is missing: $requiredPath"
        }
    }
$candidateHeader = Read-Text "core-c/src/candidate/candidate.h"
foreach ($requiredMarker in @(
        "CLEARRA_CANDIDATE_PIECE_I = CLR_PIECE_I",
        "CLEARRA_CANDIDATE_PIECE_L = CLR_PIECE_L",
        "ClearraCandidateOperation",
        "ClearraCandidateList",
        "clearra_candidate_search",
        "ClearraCompactRuleProfile",
        "clearra_harddrop_candidates_generate",
        "clearra_locked_candidates_generate",
        "clearra_locked180_candidates_generate"
    )) {
        if ($candidateHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "candidate.h must expose M8 Candidate.search marker '$requiredMarker'"
        }
    }
$candidateGenerator = Read-Text "core-c/src/candidate/candidate_search_dispatch.c"
foreach ($requiredMarker in @(
        "../piece/operation.h",
        "clearra_operation_from_shape",
        "clearra_operation_mask",
        "clearra_candidate_push_operation",
        "existing->piece == operation.piece",
        "existing->rotation == operation.rotation",
        "existing->mask == operation.mask",
        "clearra_candidate_search",
        "rule->rule_profile_id == CLR_RULE_NO_KICK",
        "rule->supports_180"
    )) {
        if ($candidateGenerator -notlike "*$requiredMarker*") {
            Add-ArchitectureError "candidate_search_dispatch.c must implement M8 operation-table/rule candidate marker '$requiredMarker'"
        }
    }
$harddrop = Read-Text "core-c/src/candidate/harddrop_candidate.c"
foreach ($requiredMarker in @("clearra_harddrop_candidates_generate", "append_landing", "clearra_candidate_unique_rotation_count", "clearra_candidate_push_operation")) {
        if ($harddrop -notlike "*$requiredMarker*") {
            Add-ArchitectureError "harddrop_candidate.c must implement M8 harddrop candidate marker '$requiredMarker'"
        }
    }
$locked = Read-Text "core-c/src/candidate/locked_candidate.c"
foreach ($requiredMarker in @(
        "clearra_locked_candidates_generate",
        "clearra_locked_candidates_generate_with_kicks",
        "append_reachable_locked_placements",
        "placement_is_grounded",
        "clearra_reachability_check",
        "clearra_candidate_push_operation"
    )) {
        if ($locked -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked_candidate.c must implement M8 Sfinder-style locked candidate marker '$requiredMarker'"
        }
    }
if ($locked -match 'return\s+clearra_harddrop_candidates_generate') {
        Add-ArchitectureError "locked_candidate.c must not alias locked candidate generation to harddrop generation"
    }
$locked180 = Read-Text "core-c/src/candidate/locked_candidate.c"
foreach ($requiredMarker in @(
        "clearra_locked180_candidates_generate",
        "clearra_locked180_candidates_generate_with_kicks"
    )) {
        if ($locked180 -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked180_candidate.c must implement M8 locked180 candidate marker '$requiredMarker'"
        }
    }
if ($locked180 -like "*append_half_turn_landings*") {
        Add-ArchitectureError "locked180_candidate.c must not append synthetic half-turn landings outside locked reachability"
    }
$candidateCache = Read-Text "core-c/src/candidate/candidate_cache.c"
foreach ($requiredMarker in @("clearra_candidate_cache_key", "clearra_cache_identity_hash", "active_piece", "rule_kick_mode")) {
        if ($candidateCache -notlike "*$requiredMarker*") {
            Add-ArchitectureError "candidate_cache.c must keep M8 board/rule/piece-scoped cache marker '$requiredMarker'"
        }
    }
$candidateTests = Get-CandidateTestsValidationSurface
foreach ($requiredMarker in @(
        "harddrop_candidate_matches_fixture",
        "locked_candidate_matches_fixture",
        "locked180_candidate_matches_fixture",
        "candidate_cache_key_includes_board_rule_piece",
        "duplicate_candidate_removed",
        "harddrop_candidate_count_fixture",
        "harddrop_candidate_rejects_blocked_fall_path",
        "harddrop_o_piece_uses_canonical_rotation_fixture",
        "locked_candidate_count_fixture",
        "locked_candidate_uses_reverse_graph_not_harddrop_alias",
        "locked_candidate_rejects_collision_free_unreachable_placement",
        "locked180_candidate_count_fixture",
        "locked180_candidate_finds_half_turn_only_placement",
        "kick_first_success_prefers_earliest_valid_offset_fixture",
        "candidate_list_has_transition"
    )) {
        if ($candidateTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "candidate_tests.c must verify M8 candidate fixture marker '$requiredMarker'"
        }
    }
foreach ($fixturePath in @(
        "tests/fixtures/packing/harddrop_candidates.json",
        "tests/fixtures/packing/locked_candidates.json",
        "tests/fixtures/packing/locked180_candidates.json"
    )) {
        $fixture = Read-Text $fixturePath
        foreach ($requiredMarker in @("candidate_count", "duplicates_removed", "active_piece", "rule")) {
            if ($fixture -notlike "*$requiredMarker*") {
                Add-ArchitectureError "$fixturePath must document M8 packing fixture marker '$requiredMarker'"
            }
        }
    }
$lockedFixture = Read-Text "tests/fixtures/packing/locked_candidates.json"
foreach ($requiredMarker in @("reverse_graph_not_harddrop_alias", "collision_free_but_unreachable_rejected", "harddrop_reachable", "locked_reachable", "locked_candidate_present")) {
        if ($lockedFixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked_candidates.json must document Sfinder-compatible locked candidate oracle marker '$requiredMarker'"
        }
    }
$locked180Fixture = Read-Text "tests/fixtures/packing/locked180_candidates.json"
foreach ($requiredMarker in @("locked180_only_operation", "locked90_candidate_present", "locked180_candidate_present", "half_turn_transition_present")) {
        if ($locked180Fixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked180_candidates.json must document Sfinder-compatible 180-only candidate oracle marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M8 Sfinder-Compatible Candidate", "Candidate.search(board", "tests/fixtures/packing", "harddrop candidate matches fixture", "locked candidate matches fixture", "locked180 candidate matches fixture", "harddrop impossible but locked reachable", "collision-free but unreachable", "locked180-only placement", "kick first-success earliest valid offset", "candidate cache key includes board/rule/piece", "duplicate candidate removed")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M8 candidate marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("clearra_candidate_search", "possible Action/Operation list", "harddrop candidate matches fixture", "locked candidate matches fixture", "locked180 candidate matches fixture", "harddrop impossible but locked reachable", "collision-free but unreachable", "locked180-only placement", "kick first-success earliest valid offset", "duplicate candidate removed")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M8 candidate marker '$requiredMarker'"
        }
    }
}
function Invoke-CReachabilityValidation() {
foreach ($requiredPath in @(
        "core-c/src/reachability/reachability.h",
        "core-c/src/reachability/reachability_checker.c",
        "core-c/src/reachability/harddrop_reachability.c",
        "core-c/src/reachability/locked_reachability.c",
        "core-c/src/reachability/kick_first_success.c",
        "core-c/src/reachability/reachability_cache.c",
        "core-c/tests/reachability_tests.c",
        "tests/fixtures/packing/reachability_collision_free_unreachable.json",
        "tests/fixtures/packing/reachability_harddrop_reachable.json",
        "tests/fixtures/packing/reachability_locked_multiple_movements.json",
        "tests/fixtures/packing/reachability_kick_first_success.json",
        "tests/fixtures/packing/reachability_180_reachable.json",
        "tests/fixtures/packing/reachability_kick_order_mismatch_rejected.json"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M9 C Reachability required file is missing: $requiredPath"
        }
    }
$reachabilityHeader = Read-Text "core-c/src/reachability/reachability.h"
foreach ($requiredMarker in @(
        "ClearraReachabilityPolicy",
        "CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY",
        "CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH",
        "CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH",
        "clearra_reachability_policy_for_mode",
        "ClearraReachabilityKickTable",
        "ClearraReachabilityReport",
        "clearra_reachability_kick_offsets_for_transition",
        "owned_compact_table",
        "clearra_reachability_kick_table_from_rule"
    )) {
        if ($reachabilityHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability.h must expose M9 policy/reachability marker '$requiredMarker'"
        }
    }
$reachabilityKickTable = Read-Text "core-c/src/reachability/locked_reachability.c"
foreach ($requiredMarker in @(
        "clearra_rule_profile_from_descriptor",
        "profile.kick_table",
        "out_table->compact_table",
        "out_table->piece"
    )) {
        if ($reachabilityKickTable -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked_reachability.c must compile rule descriptors into reachability kick tables marker '$requiredMarker'"
        }
    }
$checker = Read-Text "core-c/src/reachability/reachability_checker.c"
foreach ($requiredMarker in @(
        "clearra_reachability_policy_for_mode",
        "CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY",
        "CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH",
        "CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH",
        "clearra_reachability_mode_supports_180",
        "clearra_reachability_mode_uses_kicks"
    )) {
        if ($checker -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_checker.c must route M9 policy marker '$requiredMarker'"
        }
    }
$harddrop = Read-Text "core-c/src/reachability/harddrop_reachability.c"
foreach ($requiredMarker in @("clearra_harddrop_reachability_is_reachable", "is_grounded", "cursor_y", "collision", "out_reachable")) {
        if ($harddrop -notlike "*$requiredMarker*") {
            Add-ArchitectureError "harddrop_reachability.c must implement M9 harddrop path marker '$requiredMarker'"
        }
    }
$locked = Read-Text "core-c/src/reachability/locked_reachability.c"
foreach ($requiredMarker in @("clearra_locked_reachability_is_reachable", "push_kick_predecessor", "allow_180", "used_180", "visited_states", "clearra_reachability_kick_offsets_for_transition", "after.x - offsets", "operation.kick_dx")) {
        if ($locked -notlike "*$requiredMarker*") {
            Add-ArchitectureError "locked_reachability.c must implement M9 reverse graph marker '$requiredMarker'"
        }
    }
$kick = Read-Text "core-c/src/reachability/kick_first_success.c"
foreach ($requiredMarker in @("clearra_kick_first_success", "offsets_for_transition", "clearra_candidate_first_success_kick", "CLEARRA_ROTATION_TRANSITION_HALF_TURN")) {
        if ($kick -notlike "*$requiredMarker*") {
            Add-ArchitectureError "kick_first_success.c must implement M9 kick first-success marker '$requiredMarker'"
        }
    }
$cache = Read-Text "core-c/src/reachability/reachability_cache.c"
foreach ($requiredMarker in @("clearra_reachability_cache_key", "clearra_cache_identity_hash", "piece", "rotation", "mode")) {
        if ($cache -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_cache.c must keep M9 cache identity marker '$requiredMarker'"
        }
    }
$tests = Read-Text "core-c/tests/reachability_tests.c"
foreach ($requiredMarker in @(
        "collision_free_but_unreachable_fixture",
        "harddrop_reachable_fixture",
        "locked_reachable_via_multiple_movements_fixture",
        "kick_reachable_only_with_first_success_offset_fixture",
        "one_eighty_reachable_fixture",
        "kick_order_mismatch_rejected_fixture",
        "kick_first_success_prefers_earliest_valid_offset_fixture",
        "reverse_kick_predecessor_uses_offset_inverse_fixture"
    )) {
        if ($tests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_tests.c must verify M9 fixture marker '$requiredMarker'"
        }
    }
foreach ($fixturePath in @(
        "tests/fixtures/packing/reachability_collision_free_unreachable.json",
        "tests/fixtures/packing/reachability_harddrop_reachable.json",
        "tests/fixtures/packing/reachability_locked_multiple_movements.json",
        "tests/fixtures/packing/reachability_kick_first_success.json",
        "tests/fixtures/packing/reachability_180_reachable.json",
        "tests/fixtures/packing/reachability_kick_order_mismatch_rejected.json"
    )) {
        $fixture = Read-Text $fixturePath
        foreach ($requiredMarker in @("policy", "layout", "operation", "expected")) {
            if ($fixture -notlike "*$requiredMarker*") {
                Add-ArchitectureError "$fixturePath must document M9 reachability fixture marker '$requiredMarker'"
            }
        }
    }
$reachabilityKickFixture = Read-Text "tests/fixtures/packing/reachability_kick_first_success.json"
foreach ($requiredMarker in @("later_offset_used_when_earlier_offset_collides", "first_success_prefers_earliest_valid_offset", "expected_kick_index")) {
        if ($reachabilityKickFixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_kick_first_success.json must document first-success ordering oracle marker '$requiredMarker'"
        }
    }
$reachability180Fixture = Read-Text "tests/fixtures/packing/reachability_180_reachable.json"
foreach ($requiredMarker in @("locked90_reachable", "used_180")) {
        if ($reachability180Fixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_180_reachable.json must document locked90-vs-locked180 oracle marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M9 C Reachability", "HarddropOnly", "LockedReverseGraph", "Locked180ReverseGraph", "SpawnAwareMovementGraph", "collision-free but unreachable", "harddrop reachable", "locked reachable via multiple movements", "kick reachable only with", "first-success earliest valid offset", "180 reachable", "locked90 rejects the same target", "kick order mismatch rejected")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M9 reachability marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("clearra_reachability_policy_for_mode", "HarddropOnly", "LockedReverseGraph", "Locked180ReverseGraph", "SpawnAwareMovementGraph future", "collision-free but unreachable", "first-success earliest valid offset", "kick order mismatch rejected")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M9 reachability marker '$requiredMarker'"
        }
    }
}
