function Assert-ProductBoundaryTextContains(
    [string]$Path,
    [string[]]$Markers,
    [string]$ErrorPrefix
) {
    $text = Read-Text $Path
    foreach ($marker in $Markers) {
        if ($text -notlike "*$marker*") {
            Add-ArchitectureError "$ErrorPrefix must contain marker '$marker' in $Path"
        }
    }
}

function Assert-ProductBoundaryCargoRule(
    [string]$ManifestPath,
    [string[]]$ForbiddenCrates,
    [string]$Diagnostic
) {
    Assert-CargoDoesNotDepend $ManifestPath $ForbiddenCrates $Diagnostic
}

function Assert-CurrentArchitectureContainsNoMigrationTense {
    $architectureText = Read-Text "docs/architecture.md"
    foreach ($forbiddenPhrase in @(
            "may remain temporarily",
            "to-be-removed",
            "while the C path is brought up",
            "clearra-search",
            "GenericPcSolver",
            "cutover",
            "legacy",
            "migration",
            "future implementation",
            "scaffold"
        )) {
        if ($architectureText -match [regex]::Escape($forbiddenPhrase)) {
            Add-ArchitectureError "current_architecture_contains_no_migration_tense: docs/architecture.md contains '$forbiddenPhrase'"
        }
    }

    if ($architectureText -match 'docs[\\/]+history') {
        Add-ArchitectureError "docs/architecture.md must not use a history document as current architecture authority"
    }
}

function Assert-HandoffRemainsImplementationGuide {
    $handoffText = Read-Text "Clearra 핸드오프.md"
    foreach ($forbiddenPattern in @(
            '(?im)^\s*[-*]\s+\[[ xX]\]',
            '(?im)^\s*(구현 현황|현재 상태|진행 상황|남은 작업|향후 구현 체크포인트)\s*$',
            '(?i)\b(TODO|FIXME|WIP)\b',
            '현재\s+',
            '(미구현|미완료|구현 중|연결 예정)'
        )) {
        if ($handoffText -match $forbiddenPattern) {
            Add-ArchitectureError "Clearra 핸드오프.md must remain a timeless implementation guide and contains status-tracking pattern '$forbiddenPattern'"
        }
    }
}

function Assert-RemovedTransitionValidatorsAbsent {
    $removedValidatorPaths = @(
        "scripts/architecture/validate_workspace_surface_legacy_contract.ps1",
        "scripts/architecture/validate_legacy_candidate_reachability_contract.ps1",
        "scripts/architecture/validate_legacy_checkpoint_board_contract.ps1",
        "scripts/architecture/validate_legacy_hot_path_contract.ps1",
        "scripts/architecture/validate_legacy_inventory_contract.ps1",
        "scripts/architecture/validate_legacy_physical_removal.ps1",
        "scripts/architecture/validate_legacy_removal_contract.ps1",
        "scripts/architecture/validate_legacy_result_dependency_contract.ps1",
        "scripts/architecture/validate_legacy_tests_docs_contract.ps1"
    )
    foreach ($removedPath in $removedValidatorPaths) {
        if (Test-Path -LiteralPath $removedPath) {
            Add-ArchitectureError "dead_legacy_validator_removed: obsolete validator still exists at $removedPath"
        }
    }

    $validatorFiles = Get-ChildItem -LiteralPath "scripts/architecture" -Filter "*.ps1" -File
    foreach ($validatorFile in $validatorFiles) {
        if ($validatorFile.Name -eq "validate_product_boundary.ps1") {
            continue
        }
        $validatorText = [IO.File]::ReadAllText($validatorFile.FullName)
        if ($validatorText -match 'docs[\\/]+history') {
            Add-ArchitectureError "default architecture validation must not read history documents: $($validatorFile.FullName)"
        }
    }
}

function Assert-ProductionSymbolsUseCurrentNames {
    $sourceFiles = @(
        Get-ChildItem -LiteralPath "crates" -Recurse -File -Include "*.rs"
        Get-ChildItem -LiteralPath "core-c/include" -Recurse -File -Include "*.h"
        Get-ChildItem -LiteralPath "core-c/src" -Recurse -File -Include "*.c", "*.h"
    ) | Where-Object {
        $_.FullName -notlike "*\target\*" -and
        $_.FullName -notlike "*\build\*"
    }

    foreach ($sourceFile in $sourceFiles) {
        $sourceText = [IO.File]::ReadAllText($sourceFile.FullName)
        if ($sourceText -match '(?i)(\blegacy\b|_legacy\b|\bLegacy[A-Z])') {
            Add-ArchitectureError "production_symbols_have_no_legacy_name_without_versioned_compatibility_reason: $($sourceFile.FullName)"
        }
    }
}

function Assert-ChangelogReflectsCurrentImplementation {
    $changelogText = Read-Text "CHANGELOG.md"
    foreach ($requiredMarker in @(
            "AppRequest",
            "AppResponse",
            "C Geometry Skeleton Exact Cover",
            "C BuildUp BFS",
            "PieceSource",
            "HoldAutomaton",
            "PatternBitSet",
            "E_NATIVE_CORE_UNAVAILABLE",
            "version 1",
            "version 2"
        )) {
        if ($changelogText -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CHANGELOG_reflects_current_implementation: missing '$requiredMarker'"
        }
    }
    if ($changelogText -like "*initial workspace, crate, docs, tests, and local tooling skeleton*") {
        Add-ArchitectureError "CHANGELOG_reflects_current_implementation: initial skeleton entry is stale"
    }
}

function Invoke-ProductBoundaryValidation {
    Assert-CurrentArchitectureContainsNoMigrationTense
    Assert-HandoffRemainsImplementationGuide
    Assert-RemovedTransitionValidatorsAbsent
    Assert-ProductionSymbolsUseCurrentNames
    Assert-ChangelogReflectsCurrentImplementation

    $stableAbiFiles = @(
        Get-ChildItem -LiteralPath (Join-Path $Root "core-c/include") -Filter "*.h" -File -Recurse
        Get-Item -LiteralPath (Join-Path $Root "core-c/src/reachability/reachability.h")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-core-ffi/src/problem/mod.rs")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-core-ffi/src/problem/generic_buildup.rs")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-core-ffi/src/gpu/mod.rs")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-core-ffi/src/gpu/gpu_packing_batch_descriptor_view.rs")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-profiles/src/bag/bag_profile.rs")
        Get-Item -LiteralPath (Join-Path $Root "crates/clearra-supply/src/mixed/supply_profile.rs")
    )
    foreach ($stableAbiFile in $stableAbiFiles) {
        $stableAbiText = [IO.File]::ReadAllText($stableAbiFile.FullName)
        foreach ($forbiddenStableAbiPattern in @(
                "_FUTURE\b",
                "Future[A-Z_]",
                "SCHEMA_ONLY",
                "requires_future_dynamic_runtime",
                "ClearraGpuBatchDescriptor",
                "StandardGpuBatchDescriptor",
                "CGpuBatchDescriptorView"
            )) {
            if ($stableAbiText -match $forbiddenStableAbiPattern) {
                Add-ArchitectureError "stable product ABI must not reserve speculative or removed surface '$forbiddenStableAbiPattern' in $($stableAbiFile.FullName)"
            }
        }
    }

foreach ($requiredFile in @(
            "docs/dependency-boundary.md",
            "docs/search-postprocess-boundary.md",
            "scripts/architecture/validate_product_boundary.ps1",
            "crates/clearra-invariant-tests/tests/dependency_boundary_tests.rs"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "A architecture product boundary required file missing: $requiredFile"
        }
    }
Assert-ProductBoundaryTextContains "docs/dependency-boundary.md" @(
        "CLI / GUI / WASM Command Runtime -> AppRequest -> clearra-app",
        "C Geometry Skeleton Exact Cover",
        "C BuildUp BFS",
        "clearra-cli -> clearra-core-ffi",
        "clearra-gui-host -> clearra-cli",
        "clearra-render -> clearra-core-executor",
        "clearra-fumen -> clearra-core-executor",
        "clearra-coverage -> clearra-scoring",
        "clearra-spin -> clearra-scoring",
        "architecture_validation_rejects_cli_to_core_ffi",
        "architecture_validation_rejects_gui_to_cli",
        "architecture_validation_rejects_render_to_solver",
        "architecture_validation_rejects_fumen_to_solver",
        "architecture_validation_rejects_coverage_to_scoring",
        "architecture_validation_rejects_spin_to_scoring"
    ) "A dependency boundary contract"
Assert-ProductBoundaryTextContains "docs/search-postprocess-boundary.md" @(
        "Search enters through",
        "SearchProblem",
        "Geometry Skeleton Exact Cover",
        "BuildUp BFS verifies operation order, piece source, hold automaton",
        "Geometry exact-cover does not own queue or hold state",
        "clr_packing_problem uses a",
        "piece_multiset_window",
        "initial_hold_automaton",
        "search_problem_lowers_to_packing_problem",
        "packing_problem_uses_piece_multiset_not_fixed_order",
        "build_up_problem_owns_piece_source_ref_and_hold_automaton",
        "ffi_view_copies_native_buffers_to_owned_rust",
        "ffi_rejects_pointer_count_overflow_before_read",
        "PieceSource",
        "HoldAutomatonState",
        "BuildOrders(P) ∩ HoldReachableOrders(Q)",
        "GPU packing batches may carry compact piece ids",
        "Fumen-like data is an adapter format",
        "Internal fields are occupancy-only bitboards",
        'bit index y * width + x',
        'CoordinateFrame::TargetFrame',
        'lock-frame coordinates',
        "must not call search",
        "Unknown or incomplete spin classification",
        "PC pruning",
        "Resource-cap truncation produces incomplete output"
    ) "A search/postprocess boundary contract"
Assert-ProductBoundaryTextContains "docs/architecture.md" @(
        "C Geometry Skeleton Exact Cover",
        "C BuildUp BFS",
        "C Geometry Skeleton Exact Cover owns placement",
        "PieceSource",
        "HoldAutomatonState",
        "clr_piece_source_descriptor",
        "clr_hold_automaton_state",
        "clr_packing_problem",
        "clr_buildup_problem",
        "piece_multiset_window",
        "initial_hold_automaton",
        "Fumen-like is an adapter",
        'OccupancyField { width, height, mask }',
        'clr_occupancy_field { mask, width, height, reserved }',
        'target-frame',
        'lock-frame',
        "Search and PostProcess are separate layers"
    ) "A architecture overview"
foreach ($requiredFile in @(
            "crates/clearra-core-domain/src/field/occupancy_field.rs",
            "crates/clearra-core-domain/src/field/text_field_parser.rs",
            "crates/clearra-core-domain/src/operation/operation.rs",
            "crates/clearra-supply/src/piece_source/piece_source.rs",
            "crates/clearra-supply/src/hold_automaton/hold_automaton.rs",
            "crates/clearra-core-ffi/src/supply/piece_source_descriptor.rs",
            "crates/clearra-core-ffi/src/supply/hold_automaton_descriptor.rs",
            "crates/clearra-core-executor/src/problem_lowering/mod.rs",
            "crates/clearra-core-executor/src/problem_lowering/packing_problem_lowering.rs",
            "crates/clearra-core-executor/src/problem_lowering/buildup_problem_lowering.rs",
            "core-c/include/clr_field.h",
            "core-c/include/clr_piece_source.h",
            "core-c/include/clr_hold_automaton.h",
            "core-c/src/field/occupancy_field.c",
            "core-c/src/field/field_text_parser.c",
            "core-c/src/field/field_coordinate.c",
            "core-c/src/buildup/y_adjustment.c",
            "core-c/src/supply/piece_source_descriptor.c",
            "core-c/src/supply/hold_automaton.c",
            "core-c/src/buildup/hold_automaton_bridge.c"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "E field/operation boundary required file missing: $requiredFile"
        }
    }
$rustOccupancyField = Read-Text "crates/clearra-core-domain/src/field/occupancy_field.rs"
foreach ($marker in @(
            "pub struct OccupancyField",
            "pub width: u8",
            "pub height: u8",
            "pub mask: u64",
            "occupancy_field_has_no_color",
            "occupancy_field_has_no_owner"
        )) {
        if ($rustOccupancyField -notlike "*$marker*") {
            Add-ArchitectureError "E OccupancyField contract must expose marker '$marker'"
        }
    }
foreach ($forbiddenMarker in @("color:", "owner:", "fumen", "Fumen")) {
        if ($rustOccupancyField -like "*$forbiddenMarker*") {
            Add-ArchitectureError "E OccupancyField must stay occupancy-only; forbidden marker '$forbiddenMarker'"
        }
    }
$rustTextFieldParser = Read-Text "crates/clearra-core-domain/src/field/text_field_parser.rs"
if ($rustTextFieldParser -notlike "*text_field_top_down_parses_to_bottom_up_mask*") {
        Add-ArchitectureError "E text field parser must verify top-down to bottom-up mask parsing"
    }
$rustOperation = Read-Text "crates/clearra-core-domain/src/operation/operation.rs"
foreach ($marker in @(
            "pub struct Operation",
            "pub enum CoordinateFrame",
            "TargetFrame",
            "LockFrame",
            "search_operation_defaults_to_target_frame",
            "replay_uses_lock_frame_coordinate"
        )) {
        if ($rustOperation -notlike "*$marker*") {
            Add-ArchitectureError "E Operation coordinate-frame contract must expose marker '$marker'"
        }
    }
$cFieldHeader = Read-Text "core-c/include/clr_field.h"
foreach ($marker in @(
            "typedef struct clr_occupancy_field",
            "uint64_t mask",
            "uint8_t width",
            "uint8_t height",
            "uint16_t reserved"
        )) {
        if ($cFieldHeader -notlike "*$marker*") {
            Add-ArchitectureError "E C occupancy field ABI must expose marker '$marker'"
        }
    }
$pieceSource = @(
        Read-Text "crates/clearra-supply/src/piece_source/piece_source.rs"
        Read-Text "crates/clearra-supply/src/piece_source/piece_source_kind.rs"
        Read-Text "crates/clearra-supply/src/piece_source/supply_truncation_reason.rs"
    ) -join "`n"
foreach ($marker in @(
            "pub struct PieceSource",
            "pub enum PieceSourceKind",
            "FixedQueue",
            "BagUniverse",
            "ObservedWindow",
            "MaterializedPatternUniverse",
            "pub const fn complete(&self) -> bool",
            "pub const fn truncation_reason(&self) -> Option<SupplyTruncationReason>",
            "piece_source_fixed_queue_roundtrip",
            "piece_source_materialized_universe_roundtrip"
        )) {
        if ($pieceSource -notlike "*$marker*") {
            Add-ArchitectureError "F PieceSource contract must expose marker '$marker'"
        }
    }
$holdAutomaton = Read-Text "crates/clearra-supply/src/hold_automaton/hold_automaton.rs"
foreach ($marker in @(
            "pub struct HoldAutomatonState",
            "pub piece_source_id: PieceSourceId",
            "pub cursor: u16",
            "pub hold_piece: Option<PieceKind>",
            "pub bag_epoch: u16",
            "pub bag_remainder_key: u64",
            "HoldTransition",
            "StoreCurrentThenUseNext",
            "hold_automaton_state_in_buildup_memo_key"
        )) {
        if ($holdAutomaton -notlike "*$marker*") {
            Add-ArchitectureError "F HoldAutomaton contract must expose marker '$marker'"
        }
    }
$problemSearchFields = Read-Text "crates/clearra-problem/src/search_problem_fields.rs"
foreach ($forbiddenMarker in @(
            "pub struct PieceSource {",
            "pub enum PieceSourceKind {",
            "pub struct HoldAutomatonState {"
        )) {
        if ($problemSearchFields -like "*$forbiddenMarker*") {
            Add-ArchitectureError "F SearchProblem must reuse shared clearra-supply PieceSource/HoldAutomaton, not redefine '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @(
            "hold_automaton::HoldAutomatonState",
            "piece_source::PieceSource"
        )) {
        if ($problemSearchFields -notlike "*$requiredMarker*") {
            Add-ArchitectureError "F SearchProblem fields must re-export shared supply marker '$requiredMarker'"
        }
    }
$searchProblem = Read-Text "crates/clearra-problem/src/search_problem.rs"
foreach ($requiredMarker in @(
            "fn piece_source_for(query: &PcScenarioQuery) -> PieceSource",
            "fn initial_hold_automaton_for",
            "piece_source.id()",
            "SupplyProvenanceId(piece_source.provenance().supply_provenance_id())"
        )) {
        if ($searchProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "F SearchProblem must materialize shared PieceSource/HoldAutomaton marker '$requiredMarker'"
        }
    }
$pieceSourceFfi = Read-Text "crates/clearra-core-ffi/src/supply/piece_source_descriptor.rs"
foreach ($marker in @(
            "pub struct CPieceSourceDescriptor",
            "piece_source_id: u64",
            "source_kind: u32",
            "pattern_universe_id: u64",
            "materialized_pattern_count: u32",
            "fixed_sequence_len: u16"
        )) {
        if ($pieceSourceFfi -notlike "*$marker*") {
            Add-ArchitectureError "F CPieceSourceDescriptor ABI mirror must expose marker '$marker'"
        }
    }
$holdAutomatonFfi = Read-Text "crates/clearra-core-ffi/src/supply/hold_automaton_descriptor.rs"
foreach ($marker in @(
            "pub struct CHoldAutomatonStateDescriptor",
            "piece_source_id: u64",
            "cursor: u16",
            "bag_epoch: u16",
            "bag_remainder_key: u64",
            "hold_piece: u8"
        )) {
        if ($holdAutomatonFfi -notlike "*$marker*") {
            Add-ArchitectureError "F CHoldAutomatonStateDescriptor ABI mirror must expose marker '$marker'"
        }
    }
$cPieceSource = Read-Text "core-c/include/clr_piece_source.h"
foreach ($marker in @(
            "typedef struct clr_piece_source_descriptor",
            "uint64_t piece_source_id",
            "uint32_t source_kind",
            "uint64_t pattern_universe_id",
            "uint32_t materialized_pattern_count",
            "uint16_t fixed_sequence_len"
        )) {
        if ($cPieceSource -notlike "*$marker*") {
            Add-ArchitectureError "F C piece source descriptor must expose marker '$marker'"
        }
    }
$cHoldAutomaton = Read-Text "core-c/include/clr_hold_automaton.h"
foreach ($marker in @(
            "typedef struct clr_hold_automaton_state",
            "uint64_t piece_source_id",
            "uint16_t cursor",
            "uint16_t bag_epoch",
            "uint64_t bag_remainder_key",
            "clr_buildup_hold_automaton_memo_key"
        )) {
        if ($cHoldAutomaton -notlike "*$marker*") {
            Add-ArchitectureError "F C hold automaton descriptor must expose marker '$marker'"
        }
    }
$cProblem = Read-Text "core-c/include/clr_problem.h"
foreach ($marker in @(
            "typedef struct clr_packing_problem",
            "clr_piece_window_descriptor piece_window",
            "clr_piece_multiset_window piece_multiset_window",
            "clr_piece_source_descriptor piece_source",
            "typedef struct clr_buildup_problem",
            "clr_hold_automaton_state initial_hold_automaton"
        )) {
        if ($cProblem -notlike "*$marker*") {
            Add-ArchitectureError "G C problem descriptor ABI must expose marker '$marker'"
        }
    }
foreach ($forbiddenMarker in @(
            "clr_queue_view queue;",
            "clr_hold_state hold;"
        )) {
        if ($cProblem -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G clr_packing_problem must not own removed queue/hold field '$forbiddenMarker'"
        }
    }
$cBuildUpState = Read-Text "core-c/src/buildup/buildup_state.h"
foreach ($requiredMarker in @(
            "clr_hold_automaton_state hold_automaton_state",
            "ClearraLineClearState line_clear_state",
            "uint64_t reachability_relevant_state"
        )) {
        if ($cBuildUpState -notlike "*$requiredMarker*") {
            Add-ArchitectureError "J C BuildUp state must keep full memo source marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "uint16_t cursor;",
            "uint8_t hold_piece;",
            "uint8_t hold_empty;"
        )) {
        if ($cBuildUpState -like "*$forbiddenMarker*") {
            Add-ArchitectureError "J C BuildUp state must use hold_automaton_state as source of truth, not duplicate '$forbiddenMarker'"
        }
    }
$packingLowering = Read-Text "crates/clearra-core-executor/src/problem_lowering/packing_problem_lowering.rs"
foreach ($marker in @(
            "search_problem_lowers_to_packing_problem",
            "packing_problem_uses_piece_multiset_not_fixed_order",
            "PackingProblemLowering"
        )) {
        if ($packingLowering -notlike "*$marker*") {
            Add-ArchitectureError "G PackingProblem lowering must expose marker '$marker'"
        }
    }
$buildupLowering = Read-Text "crates/clearra-core-executor/src/problem_lowering/buildup_problem_lowering.rs"
foreach ($marker in @(
            "build_up_problem_owns_piece_source_ref_and_hold_automaton",
            "BuildUpProblemLowering"
        )) {
        if ($buildupLowering -notlike "*$marker*") {
            Add-ArchitectureError "G BuildUpProblem lowering must expose marker '$marker'"
        }
    }
$ffiBuildVariant = Read-Text "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
foreach ($marker in @(
            "ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape",
            "ffi_build_variant_rejects_kick_evidence_count_above_c_limit",
            "ffi_view_copies_native_buffers_to_owned_rust",
            "ffi_rejects_pointer_count_overflow_before_read"
        )) {
        if ($ffiBuildVariant -notlike "*$marker*") {
            Add-ArchitectureError "G FFI native view safety must expose marker '$marker'"
        }
    }
$productionFiles = Get-ChildItem -Path "crates", "core-c" -Recurse -File |
        Where-Object {
            $_.FullName -notlike "*\target\*" -and
            $_.FullName -notlike "*\node_modules\*" -and
            $_.FullName -notlike "*\dist\*" -and
            $_.FullName -notlike "*\build\*" -and
            ($_.Extension -eq ".rs" -or $_.Extension -eq ".c" -or $_.Extension -eq ".h")
    }
foreach ($file in $productionFiles) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -like "*struct PackingBfsState*" -and
            ($text -like "*Vec<Piece*" -or $text -like "*hold_piece*")) {
            Add-ArchitectureError "F PackingBfsState must not store Vec<Piece> or hold_piece: $($file.FullName)"
        }
    }
Assert-ProductBoundaryCargoRule "crates/clearra-cli/Cargo.toml" @(
        "clearra-core-ffi",
        "clearra-core-executor"
    ) "architecture_validation_rejects_cli_to_core_ffi"
Assert-ProductBoundaryCargoRule "crates/clearra-gui-host/Cargo.toml" @(
        "clearra-cli"
    ) "architecture_validation_rejects_gui_to_cli"
Assert-ProductBoundaryCargoRule "crates/clearra-render/Cargo.toml" @(
        "clearra-core-executor"
    ) "architecture_validation_rejects_render_to_solver"
Assert-ProductBoundaryCargoRule "crates/clearra-fumen/Cargo.toml" @(
        "clearra-core-executor"
    ) "architecture_validation_rejects_fumen_to_solver"
Assert-ProductBoundaryCargoRule "crates/clearra-coverage/Cargo.toml" @(
        "clearra-scoring"
    ) "architecture_validation_rejects_coverage_to_scoring"
if (Test-Path -LiteralPath "crates/clearra-spin/Cargo.toml") {
        Assert-ProductBoundaryCargoRule "crates/clearra-spin/Cargo.toml" @(
            "clearra-scoring"
        ) "architecture_validation_rejects_spin_to_scoring"
    }
$dependencyValidator = Read-Text "scripts/architecture/validate_dependencies.ps1"
foreach ($marker in @(
            "architecture_validation_rejects_cli_to_core_ffi",
            "architecture_validation_rejects_gui_to_cli",
            "architecture_validation_rejects_render_to_solver",
            "architecture_validation_rejects_fumen_to_solver",
            "architecture_validation_rejects_coverage_to_scoring",
            "architecture_validation_rejects_spin_to_scoring"
        )) {
        if ($dependencyValidator -notlike "*$marker*" -and
            (Read-Text "scripts/architecture/validate_product_boundary.ps1") -notlike "*$marker*") {
            Add-ArchitectureError "dependency/product boundary validators must expose marker '$marker'"
        }
    }
}
