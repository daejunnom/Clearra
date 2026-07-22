# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.
function Invoke-Board128WideBackendValidation() {
$architectureDoc = Read-Text "docs/architecture.md"
$algorithmsDoc = Read-Text "docs/algorithms.md"
$futureCustomPiecesDoc = Read-Text "docs/future-custom-pieces.md"
$mvpScopeDoc = Read-Text "docs/mvp-scope.md"
foreach ($requiredMarker in @(
            "X6 MVP3 Board128 / Board256 / Wide Board",
            "Board256",
            "WideBoard descriptor",
            "generic row mask",
            "generic operation mask",
            "C board backend dispatch",
            "Rust geometry metadata bridge",
            "Board64 fast path unchanged",
            "unsupported board width silent fallback forbidden"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X6 Board128/Wide board marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "G3 Board128 / Board256 / Wide board runtime",
            "board_backend_not_connected",
            "wide_board_runtime_not_connected",
            "board_width_out_of_scope",
            "board128_basic_row_mask_collision_place_tests_pass",
            "wide_board_runtime_not_connected_reports_reason"
        )) {
        if ($mvpScopeDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/mvp-scope.md must document G3 Board128/Wide marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Board dispatch keeps the Board64 fast path unchanged",
            "clr_board_backend_kind_for_cell_count",
            "clr_board_backend_capability_for_cell_count",
            "clr_board_dispatch_row_mask",
            "clr_board_operation_mask_from_cells",
            "CLR_BOARD_UNSUPPORTED_BACKEND",
            "board_backend_not_connected",
            "wide_board_runtime_not_connected",
            "unsupported board width silent fallback forbidden"
        )) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must document X6 board dispatch marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Board128",
            "Wide",
            "C board backend dispatch",
            "clr_board_backend_capability",
            "generic row mask",
            "generic operation mask",
            "board_width_out_of_scope",
            "CBoardDescriptor.backend_kind"
        )) {
        if ($futureCustomPiecesDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/future-custom-pieces.md must document X6 custom-width board marker '$requiredMarker'"
        }
    }
$cMake = @(
    Read-Text "core-c/CMakeLists.txt"
    Read-Text "core-c/cmake/source_manifest.cmake"
) -join "`n"
foreach ($requiredMarker in @(
            "src/board/board_backend_dispatch.c",
            "src/board/board128.c",
            "src/board/board256.c",
            "src/board/standard_pc_extended_board.c",
            "src/board/wide_board.c",
            "board_backend_dispatch_tests",
            "scripts/board128-wide-runtime-check.ps1"
        )) {
        if ($requiredMarker -like "scripts/*") {
            if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredMarker))) {
                Add-ArchitectureError "G3 required Board128/Wide script missing: $requiredMarker"
            }
        } elseif ($cMake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must build X6 board backend dispatch marker '$requiredMarker'"
        }
    }
$boardHeader = Read-Text "core-c/include/clr_board.h"
foreach ($requiredMarker in @(
            "CLR_BOARD_BACKEND_BOARD64",
            "CLR_BOARD_BACKEND_BOARD128",
            "CLR_BOARD_BACKEND_BOARD256",
            "CLR_BOARD_BACKEND_WIDE",
            "CLR_BOARD_UNSUPPORTED_BACKEND",
            "CLR_BOARD_UNSUPPORTED_REASON_BOARD_WIDTH_OUT_OF_SCOPE",
            "CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED",
            "CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED",
            "clr_board_backend_capability",
            "clr_board128_descriptor",
            "clr_board256_descriptor",
            "clr_standard_pc_extended_board_descriptor",
            "clr_wide_board_descriptor",
            "clr_generic_board_mask",
            "clr_board_backend_capability_for_kind",
            "clr_board_backend_capability_for_cell_count",
            "clr_board_dispatch_row_mask",
            "clr_board_operation_mask_from_cells"
        )) {
        if ($boardHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/include/clr_board.h must expose X6 board backend ABI marker '$requiredMarker'"
        }
    }
$boardDispatch = Read-Text "core-c/src/board/board_backend_dispatch.c"
foreach ($requiredMarker in @(
            "clr_board_backend_kind_for_cell_count",
            "clr_board_backend_capability_for_kind",
            "CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED",
            "CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED",
            "clr_board_descriptor_init",
            "clr_board_dispatch_row_mask",
            "clr_board_operation_mask_from_cells",
            "CLR_BOARD_UNSUPPORTED_BACKEND"
        )) {
        if ($boardDispatch -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c board backend dispatch must implement routing marker '$requiredMarker'"
        }
    }
$board128Source = Read-Text "core-c/src/board/board128.c"
foreach ($requiredMarker in @(
            "clr_board128_make_descriptor",
            "clr_board128_row_mask",
            "clr_board128_collision",
            "clr_board128_place"
        )) {
        if ($board128Source -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c Board128 source must implement marker '$requiredMarker'"
        }
    }
$board256Source = Read-Text "core-c/src/board/board256.c"
foreach ($requiredMarker in @(
            "clr_board256_make_descriptor",
            "clr_board256_row_mask",
            "clr_board256_collision",
            "clr_board256_place"
        )) {
        if ($board256Source -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c Board256 source must implement marker '$requiredMarker'"
        }
    }
$extendedBoardSource = Read-Text "core-c/src/board/standard_pc_extended_board.c"
foreach ($requiredMarker in @(
            "clr_standard_pc_extended_board_descriptor_init",
            "clr_standard_pc_extended_board_descriptor_is_valid",
            "CLR_STANDARD_PC_EXTENDED_MIN_LINES",
            "CLR_STANDARD_PC_MAX_LINES"
        )) {
        if ($extendedBoardSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c extended standard PC descriptor must implement marker '$requiredMarker'"
        }
    }
$wideBoardSource = Read-Text "core-c/src/board/wide_board.c"
foreach ($requiredMarker in @(
            "clr_wide_board_make_descriptor",
            "clr_wide_board_descriptor_is_valid"
        )) {
        if ($wideBoardSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c WideBoard source must implement marker '$requiredMarker'"
        }
    }
$boardDispatchTests = Read-Text "core-c/tests/board_backend_dispatch_tests.c"
foreach ($requiredMarker in @(
        "board64_fast_path_row_mask_is_unchanged",
            "board128_descriptor_validates",
            "board128_basic_row_mask_collision_place_tests_pass",
            "wide_board_descriptor_validates",
            "wide_board_runtime_not_connected_reports_reason",
            "unsupported_board_width_silent_fallback_forbidden"
        )) {
        if ($boardDispatchTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c board backend dispatch tests must verify X6 marker '$requiredMarker'"
        }
    }
$coreFfiBoard = Read-Text "crates/clearra-core-ffi/src/board/mod.rs"
foreach ($requiredMarker in @(
            "C_BOARD_BACKEND_BOARD64",
            "C_BOARD_BACKEND_BOARD128",
            "C_BOARD_BACKEND_BOARD256",
            "C_BOARD_BACKEND_WIDE",
            "CBoardBackendCapability",
            "C_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED",
            "C_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED",
            "CBoard128Descriptor",
            "CBoard256Descriptor",
            "CStandardPcExtendedBoardDescriptor",
            "CWideBoardDescriptor",
            "CGenericBoardMask",
            "UnsupportedBackend = 5"
        )) {
        if ($coreFfiBoard -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi board module must mirror X6 C ABI marker '$requiredMarker'"
        }
    }
$geometryBoardCapability = Read-Text "crates/clearra-geometry/src/board/board_backend_capability.rs"
foreach ($requiredMarker in @(
            "BoardBackendCapability",
            "BoardRuntimeUnsupportedReason",
            "board_backend_capability_for_size",
            "board_width_out_of_scope",
            "board_backend_not_connected",
            "wide_board_runtime_not_connected",
            "board64_fast_path_unchanged",
            "board128_descriptor_validates_while_packing_runtime_is_guarded",
            "wide_board_descriptor_validates_but_runtime_reports_reason"
        )) {
        if ($geometryBoardCapability -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-geometry BoardBackendCapability must expose G3 marker '$requiredMarker'"
        }
    }
$boardValidator = Read-Text "crates/clearra-validation/src/validators/board_validator.rs"
foreach ($requiredMarker in @(
            "EBoardWidthOutOfScope",
            "EBoardBackendNotConnected",
            "EWideBoardRuntimeNotConnected",
            "board_width_out_of_scope",
            "board_backend_not_connected",
            "wide_board_runtime_not_connected",
            "unsupported_board_width_reports_reason_without_silent_fallback"
        )) {
        if ($boardValidator -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BoardValidator must expose G3 unsupported board reason marker '$requiredMarker'"
        }
    }
$coreFfiProblem = Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
foreach ($requiredMarker in @(
            "pub initial_mask_hi: u64",
            "pub backend_kind: u32",
            "pub cell_count: u32"
        )) {
        if ($coreFfiProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CBoardDescriptor must mirror X6 C descriptor field marker '$requiredMarker'"
        }
    }
$packingBuilder = @(
        Read-Text "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_board_descriptor_builder.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "backend_kind_for_size",
            "BoardBackendKind",
            "board_backend_code",
            "C_BOARD_BACKEND_BOARD64",
            "C_BOARD_BACKEND_BOARD128",
            "C_BOARD_BACKEND_BOARD256",
            "C_BOARD_BACKEND_WIDE",
            "board_descriptor_uses_active_packing_height_for_backend_selection"
        )) {
        if ($packingBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CPackingProblemBuilder must bridge Rust geometry metadata to C board descriptor marker '$requiredMarker'"
        }
    }
$invariantBoardBackendContracts = Read-Text "crates/clearra-invariant-tests/tests/board_backend_contract_tests.rs"
foreach ($requiredMarker in @(
            "rust_geometry_metadata_bridge_tracks_board_backend_identity",
            "board_backend_capability_reports_g3_runtime_boundaries",
            "BoardLayoutBackend",
            "BoardRuntimeUnsupportedReason",
            "Board128State",
            "WideBoardState",
            "ECustomBoardUnsupportedMvp"
        )) {
        if ($invariantBoardBackendContracts -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-invariant-tests must carry X6 board backend invariant marker '$requiredMarker'"
        }
    }
}
function Invoke-ExactCoverDlxGeneralizationValidation() {
$architectureDoc = Read-Text "docs/architecture.md"
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @(
            "X7 MVP3 Exact-Cover / DLX Generalization",
            "GenericExactCoverCandidate",
            "CellUniverseBuilder",
            "PieceAreaConstraint",
            "DlxBuildUpBridge",
            "standard setup tiling still works",
            "custom piece tiling can be represented",
            "area infeasible shape rejected before expensive search",
            "DLX result maps to BuildUpProblem"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X7 exact-cover/DLX marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "GenericExactCoverCandidate",
            "CellUniverseBuilder",
            "PieceAreaConstraint",
            "GenericExactCoverBridge::enumerate_tilings",
            "CustomPieceBridge::enumerate_tilings",
            "DlxBuildUpBridge",
            "DlxSolver::solve_all_limited",
            "not a replacement for PC queue/hold/reachability search"
        )) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must document X7 exact-cover/DLX ownership marker '$requiredMarker'"
        }
    }
$cellUniverseBuilder = Read-Text "crates/clearra-exact-cover/src/builder/cell_universe_builder.rs"
foreach ($requiredMarker in @(
            "CellUniverse",
            "compact_by_cell",
            "compact_column_for_cell",
            "compact_columns_for_cells",
            "cell_universe_builder_remaps_sparse_cells_to_compact_columns"
        )) {
        if ($cellUniverseBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CellUniverseBuilder must own X7 sparse cell universe marker '$requiredMarker'"
        }
    }
$pieceAreaConstraint = Read-Text "crates/clearra-exact-cover/src/builder/piece_area_constraint.rs"
foreach ($requiredMarker in @(
            "PieceAreaConstraint",
            "can_fill_target",
            "bounded_area_subset_sum",
            "piece_area_constraint_rejects_area_infeasible_shape_before_dlx"
        )) {
        if ($pieceAreaConstraint -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PieceAreaConstraint must reject area-infeasible exact-cover shapes marker '$requiredMarker'"
        }
    }
$genericCandidate = Read-Text "crates/clearra-exact-cover/src/model/generic_exact_cover_candidate.rs"
foreach ($requiredMarker in @(
            "GenericExactCoverCandidate",
            "piece_id",
            "piece_area",
            "CellOutsideUniverse",
            "PieceAreaDoesNotMatchCells",
            "to_exact_cover_candidate",
            "generic_exact_cover_candidate_maps_cells_to_compact_columns"
        )) {
        if ($genericCandidate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GenericExactCoverCandidate must carry interpreted custom/setup tiling metadata marker '$requiredMarker'"
        }
    }
$genericBridge = Read-Text "crates/clearra-exact-cover/src/bridge/generic_exact_cover_bridge.rs"
foreach ($requiredMarker in @(
            "GenericExactCoverBridge",
            "problem_from_candidates",
            "enumerate_tilings",
            "AreaInfeasibleShape",
            "DlxSolver::solve_all_limited",
            "generic_exact_cover_bridge_enumerates_custom_piece_tiling_candidates",
            "generic_exact_cover_bridge_rejects_area_infeasible_shape_before_dlx"
        )) {
        if ($genericBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GenericExactCoverBridge must connect generic candidates, area guard, and DLX marker '$requiredMarker'"
        }
    }
$setupTilingBridge = Read-Text "crates/clearra-exact-cover/src/bridge/setup_tiling_bridge.rs"
foreach ($requiredMarker in @(
            "CellUniverseBuilder::universe_from_mask",
            "compact_columns_for_mask",
            "compact_column_for_cell",
            "remaps_sparse_shape_bits_to_compact_exact_cover_columns",
            "enumerate_uses_dlx_solver_for_setup_shape_tiling_candidates"
        )) {
        if ($setupTilingBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SetupTilingBridge must keep standard setup tiling on compact DLX columns marker '$requiredMarker'"
        }
    }
$dlxBuildUpBridge = Read-Text "crates/clearra-core-ffi/src/problem/dlx_buildup_bridge.rs"
foreach ($requiredMarker in @(
            "DlxBuildUpBridge",
            "DlxBuildUpOperationCandidate",
            "packing_candidate_from_solution",
            "buildup_problem_from_solution",
            "CBuildUpProblemBuilder::from_packing_candidate",
            "dlx_result_maps_to_buildup_problem_without_treating_dlx_as_solution"
        )) {
        if ($dlxBuildUpBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "DlxBuildUpBridge must map DLX result to BuildUpProblem through CPackingCandidate marker '$requiredMarker'"
        }
    }
$invariantExactCoverDlxContracts = Read-Text "crates/clearra-invariant-tests/tests/exact_cover_dlx_contract_tests.rs"
foreach ($requiredMarker in @(
            "setup_tiling_bridge_uses_dlx_after_sparse_shape_column_remap",
            "generic_exact_cover_candidate_represents_custom_piece_tiling_cells",
            "area_infeasible_shape_rejected_before_expensive_search",
            "dlx_result_maps_to_buildup_problem"
        )) {
        if ($invariantExactCoverDlxContracts -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-invariant-tests must carry X7 exact-cover/DLX contract marker '$requiredMarker'"
        }
    }
}
