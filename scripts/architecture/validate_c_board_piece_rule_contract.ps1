# This file is dot-sourced by an architecture validation wrapper.
# Keep the grouped validation functions side-effect free at load time.
function Invoke-CBoard64CoreValidation() {
foreach ($requiredPath in @(
        "core-c/src/board/board64.h",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/src/board/board64.c",
        "core-c/tests/board64_tests.c",
        "core-c/tests/test_board64.c"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M5 C Board64 core required file is missing: $requiredPath"
        }
    }
$boardHeader = Read-Text "core-c/src/board/board64.h"
foreach ($requiredMarker in @(
        "clearra_board64_empty",
        "clearra_board64_occupied_mask",
        "clearra_board64_cell_index",
        "clearra_board64_cell_mask",
        "clearra_board64_row_mask",
        "clearra_board64_row_is_full",
        "clearra_board64_collision",
        "clearra_board64_place",
        "clearra_board64_clear_lines",
        "clearra_board64_hash",
        "clearra_board64_equal"
    )) {
        if ($boardHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "board64.h must expose M5 Board64 operation marker '$requiredMarker'"
        }
    }
$board64Source = Read-Text "core-c/src/board/board64.c"
foreach ($requiredMarker in @("clearra_board64_empty", "clearra_board64_occupied_mask", "MASK_OUTSIDE_LAYOUT")) {
        if ($board64Source -notlike "*$requiredMarker*") {
            Add-ArchitectureError "board64.c must implement M5 occupied/empty marker '$requiredMarker'"
        }
    }
$layoutSource = Read-Text "core-c/src/board/board64.c"
foreach ($requiredMarker in @("clearra_board64_cell_index", "clearra_board64_cell_mask", "bottom-left")) {
        if ($layoutSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "board_layout.c must implement M5 cell index/mask marker '$requiredMarker'"
        }
    }
$rowMaskSource = Read-Text "core-c/src/board/board64.c"
foreach ($requiredMarker in @("clearra_board64_row_mask", "clearra_board64_row_is_full")) {
        if ($rowMaskSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "row_mask.c must implement M5 row mask/full marker '$requiredMarker'"
        }
    }
$hashSource = Read-Text "core-c/src/board/board64.c"
foreach ($requiredMarker in @("clearra_board64_hash", "clearra_board64_equal")) {
        if ($hashSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "board_hash.c must implement M5 hash/equality marker '$requiredMarker'"
        }
    }
$boardTests = Read-Text "core-c/tests/board64_tests.c"
foreach ($requiredMarker in @(
        "empty_board_is_zero",
        "single_cell_mask_uses_cell_index_mapping",
        "occupied_mask_rejects_bits_outside_layout",
        "row_full_detection_uses_exact_row_mask",
        "collision_reports_true_and_false",
        "place_result_or_collision_is_explicit",
        "line_clear_compacts_rows_above",
        "multi_line_clear_compacts_remaining_rows",
        "line_clear_after_placement_clears_completed_row",
        "board_hash_is_stable_and_layout_scoped",
        "board_equality_is_layout_scoped"
    )) {
        if ($boardTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "board64_tests.c must verify M5 Board64 fixture marker '$requiredMarker'"
        }
    }
$testBoard64 = Read-Text "core-c/tests/test_board64.c"
if ($testBoard64 -notlike "*board64_tests.c*") {
        Add-ArchitectureError "test_board64.c must be the M5 named Board64 fixture entrypoint"
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @("board64_tests", "test_board64", "tests/test_board64.c")) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must register M5 Board64 fixture marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M5 C Board64 Core", "single cell mask", "row full detection", "occupied mask validation", "line clear after placement", "board equality", "core-c/tests/test_board64.c")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M5 Board64 core marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("clearra_board64_occupied_mask", "clearra_board64_cell_mask", "clearra_board64_row_is_full", "clearra_board64_equal")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M5 Board64 operation marker '$requiredMarker'"
        }
    }
}
function Invoke-CPieceOperationTableValidation() {
foreach ($requiredPath in @(
        "core-c/src/piece/operation.h",
        "core-c/src/piece/tetromino.c",
        "core-c/src/piece/rotation.c",
        "core-c/src/piece/operation.c",
        "core-c/src/piece/operation_table.c",
        "core-c/src/piece/operation_set.c",
        "core-c/tests/operation_table_tests.c"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M6 C Piece / Operation Table required file is missing: $requiredPath"
        }
    }
$operationHeader = Read-Text "core-c/src/piece/operation.h"
foreach ($requiredMarker in @(
        "CLEARRA_STANDARD_TETROMINO_COUNT",
        "CLEARRA_ROTATION_STATE_COUNT",
        "ClearraCellOffset",
        "ClearraOperationBounds",
        "ClearraOperation",
        "ClearraOperationTable",
        "ClearraOperationSet",
        "clearra_piece_is_standard_tetromino",
        "clearra_piece_area",
        "clearra_tetromino_shape",
        "clearra_rotation_count_for_piece",
        "clearra_operation_id",
        "clearra_operation_mask",
        "clearra_operation_table_generate",
        "clearra_operation_set_count_piece"
    )) {
        if ($operationHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "operation.h must expose M6 piece/operation marker '$requiredMarker'"
        }
    }
$tetrominoSource = Read-Text "core-c/src/piece/tetromino.c"
foreach ($requiredMarker in @("CLR_PIECE_I", "CLR_PIECE_O", "CLR_PIECE_T", "CLR_PIECE_S", "CLR_PIECE_Z", "CLR_PIECE_J", "CLR_PIECE_L", "STANDARD_SHAPES", "clearra_piece_area")) {
        if ($tetrominoSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "tetromino.c must define M6 standard tetromino marker '$requiredMarker'"
        }
    }
$rotationSource = Read-Text "core-c/src/piece/rotation.c"
foreach ($requiredMarker in @("clearra_rotation_state_is_valid", "clearra_rotation_state_name", "clearra_rotation_count_for_piece", "CLEARRA_ROTATION_STATE_COUNT")) {
        if ($rotationSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "rotation.c must define M6 rotation marker '$requiredMarker'"
        }
    }
$operationSource = Read-Text "core-c/src/piece/operation.c"
foreach ($requiredMarker in @("bounds_for_shape", "base_shape_mask", "clearra_operation_id", "clearra_operation_from_shape", "clearra_operation_mask", "clearra_board64_cell_mask")) {
        if ($operationSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "operation.c must implement M6 operation marker '$requiredMarker'"
        }
    }
$operationTableSource = Read-Text "core-c/src/piece/operation_table.c"
foreach ($requiredMarker in @("clearra_operation_table_generate", "CLEARRA_STANDARD_TETROMINO_COUNT", "CLEARRA_ROTATION_STATE_COUNT", "operation.operation_id")) {
        if ($operationTableSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "operation_table.c must generate M6 deterministic table marker '$requiredMarker'"
        }
    }
$operationSetSource = Read-Text "core-c/src/piece/operation_set.c"
foreach ($requiredMarker in @("clearra_operation_set_clear", "clearra_operation_set_push", "clearra_operation_set_count_piece", "clearra_operation_set_from_table_for_piece")) {
        if ($operationSetSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "operation_set.c must implement M6 operation set marker '$requiredMarker'"
        }
    }
$operationTests = Read-Text "core-c/tests/operation_table_tests.c"
foreach ($requiredMarker in @(
        "standard_seven_tetrominoes_exist",
        "each_piece_has_four_rotation_operations",
        "operation_mask_stays_stable",
        "bounds_are_correct",
        "piece_area_is_four",
        "operation_id_is_deterministic",
        "operation_table_generates_standard_28_operations",
        "operation_set_counts_piece_rotations"
    )) {
        if ($operationTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "operation_table_tests.c must verify M6 fixture marker '$requiredMarker'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/piece/tetromino.c",
        "src/piece/rotation.c",
        "src/piece/operation.c",
        "src/piece/operation_table.c",
        "src/piece/operation_set.c",
        "operation_table_tests"
    )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must register M6 operation table marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M6 C Piece / Operation Table", "tetromino.c", "rotation.c", "operation_table.c", "I/O/T/S/Z/J/L", "each piece has four rotation operations", "operation mask stable", "bounds correct", "piece area = 4", "operation id deterministic")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M6 operation table marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("clearra_operation_table_generate", "clearra_operation_mask", "clearra_operation_id", "clearra_operation_set_count_piece", "deterministic operation ids")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M6 operation table marker '$requiredMarker'"
        }
    }
}
function Invoke-CRuleKickCompactModelValidation() {
foreach ($requiredPath in @(
        "core-c/src/rules/rules.h",
        "core-c/src/rules/rule_profile.c",
        "core-c/src/rules/srs_kicks.c",
        "core-c/src/rules/no_kick.c",
        "core-c/src/rules/kick_table.c",
        "core-c/src/rules/spawn_profile.c",
        "core-c/tests/rule_profile_tests.c"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M7 C Rule / Kick Compact Model required file is missing: $requiredPath"
        }
    }
$rulesHeader = Read-Text "core-c/src/rules/rules.h"
foreach ($requiredMarker in @(
        "ClearraRuleStatus",
        "ClearraCompactKickOffset",
        "ClearraCompactKickSequence",
        "ClearraCompactKickTransition",
        "ClearraCompactKickTable",
        "ClearraCompactSpawnProfile",
        "ClearraCompactRuleProfile",
        "clearra_rule_profile_from_descriptor",
        "clearra_srs_kick_table",
        "clearra_srs_plus_kick_table",
        "clearra_no_kick_table",
        "clearra_kick_table_sequence_for",
        "clearra_kick_table_supports_180",
        "clearra_kick_table_zero_offsets_only",
        "clearra_spawn_profile_from_id"
    )) {
        if ($rulesHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "rules.h must expose M7 compact rule/kick marker '$requiredMarker'"
        }
    }
$ruleProfile = Read-Text "core-c/src/rules/rule_profile.c"
foreach ($requiredMarker in @("CLR_RULE_SRS_PLUS", "CLR_RULE_SRS", "CLR_RULE_NO_KICK", "CLEARRA_RULE_UNSUPPORTED_RULE", "CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE", "clearra_rule_profile_from_descriptor")) {
        if ($ruleProfile -notlike "*$requiredMarker*") {
            Add-ArchitectureError "rule_profile.c must convert M7 compact descriptors and reject unsupported marker '$requiredMarker'"
        }
    }
$srsKicks = Read-Text "core-c/src/rules/srs_kicks.c"
foreach ($requiredMarker in @("clearra_srs_kick_table", "clearra_srs_plus_kick_table", "EIGHT_DIRECTION_TRANSITIONS", "ONE_EIGHTY_TRANSITIONS", "srs_plus_180_sequence", "CLR_KICK_SRS_PLUS_180")) {
        if ($srsKicks -notlike "*$requiredMarker*") {
            Add-ArchitectureError "srs_kicks.c must implement M7 SRS/SRS+ compact marker '$requiredMarker'"
        }
    }
$noKick = Read-Text "core-c/src/rules/no_kick.c"
foreach ($requiredMarker in @("clearra_no_kick_sequence", "clearra_no_kick_table", "CLR_KICK_NO_KICK", "CLR_RULE_NO_KICK")) {
        if ($noKick -notlike "*$requiredMarker*") {
            Add-ArchitectureError "no_kick.c must implement M7 NoKick compact marker '$requiredMarker'"
        }
    }
$kickTable = Read-Text "core-c/src/rules/kick_table.c"
foreach ($requiredMarker in @("clearra_kick_table_push", "clearra_kick_table_sequence_for", "clearra_kick_table_supports_180", "clearra_kick_table_zero_offsets_only", "clearra_rule_transition_is_180")) {
        if ($kickTable -notlike "*$requiredMarker*") {
            Add-ArchitectureError "kick_table.c must expose M7 compact kick table marker '$requiredMarker'"
        }
    }
$spawnProfile = Read-Text "core-c/src/rules/spawn_profile.c"
foreach ($requiredMarker in @("clearra_spawn_profile_from_id", "CLR_SPAWN_STANDARD_10", "CLEARRA_RULE_UNSUPPORTED_SPAWN_PROFILE")) {
        if ($spawnProfile -notlike "*$requiredMarker*") {
            Add-ArchitectureError "spawn_profile.c must expose M7 compact spawn profile marker '$requiredMarker'"
        }
    }
$ruleTests = Read-Text "core-c/tests/rule_profile_tests.c"
foreach ($requiredMarker in @(
        "kick_transition_count_fixture",
        "srs_transition_offsets_are_compact_runtime_view",
        "no_kick_has_zero_offset_only_fixture",
        "unsupported_rule_returns_status_fixture",
        "srs_plus_capability_reported_fixture"
    )) {
        if ($ruleTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "rule_profile_tests.c must verify M7 compact rule fixture marker '$requiredMarker'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/rules/rule_profile.c",
        "src/rules/srs_kicks.c",
        "src/rules/no_kick.c",
        "src/rules/kick_table.c",
        "src/rules/spawn_profile.c",
        "rule_profile_tests"
    )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must register M7 compact rule marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M7 C Rule / Kick Compact Model", "clearra-rules` remains the owner", "SRS has 56 compact transitions", "SRS+ has 80 compact transitions", "NoKick has 56 zero-offset transitions", "unsupported status")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M7 compact rule marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("clearra_rule_profile_from_descriptor", "clearra_srs_plus_kick_table", "clearra_kick_table_zero_offsets_only", "clearra-rules` remains the source/verify/import/export owner", "unsupported status")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M7 compact rule marker '$requiredMarker'"
        }
    }
}
