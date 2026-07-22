# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$buildCoverageCargo = Read-Text "crates/clearra-build-coverage/Cargo.toml"
if ($buildCoverageCargo -notlike "*[lib]*" -or $buildCoverageCargo -notlike "*test = false*") {
    Add-ArchitectureError "clearra-build-coverage must keep its library test harness disabled on Windows; cross-crate contracts live in clearra-invariant-tests"
}
$buildSlot = Read-Text "crates/clearra-build-coverage/src/template/build_slot.rs"
$buildTemplate = Read-Text "crates/clearra-build-coverage/src/template/build_template.rs"
$templateImport = Read-Text "crates/clearra-build-coverage/src/template/template_import.rs"
$templateJsonReader = Read-Text "crates/clearra-build-coverage/src/template/template_json_reader.rs"
$templateJsonWriter = Read-Text "crates/clearra-build-coverage/src/template/template_json_writer.rs"
$templateJsonError = Read-Text "crates/clearra-build-coverage/src/template/template_json_error.rs"
$templateJsonFields = Read-Text "crates/clearra-build-coverage/src/template/template_json_fields.rs"
$templateJsonEnums = Read-Text "crates/clearra-build-coverage/src/template/template_json_enums.rs"
$templateJsonSchema = Read-Text "crates/clearra-build-coverage/src/template/template_json_schema.rs"
$buildQueryValidator = Read-Text "crates/clearra-validation/src/validators/build_query_validator.rs"
$buildTemplateValidator = Read-Text "crates/clearra-validation/src/validators/build_template_validator.rs"
$buildSlotGeometryValidator = Read-Text "crates/clearra-validation/src/validators/build_slot_geometry_validator.rs"
$buildSlotOrderValidator = Read-Text "crates/clearra-validation/src/validators/build_slot_order_validator.rs"
$buildSlotDomainValidator = Read-Text "crates/clearra-validation/src/validators/build_slot_domain_validator.rs"
$buildConstraintValidator = Read-Text "crates/clearra-validation/src/validators/build_constraint_validator.rs"
$buildAssignmentFeasibilityValidator = Read-Text "crates/clearra-validation/src/validators/build_assignment_feasibility_validator.rs"
$buildLimitValidator = Read-Text "crates/clearra-validation/src/validators/build_limit_validator.rs"
foreach ($requiredMarker in @("label: Option<String>", "allowed_pieces: Vec<PieceKind>", "required_piece: Option<PieceKind>", "SlotHoldConstraint", "SlotOrderConstraint", "SlotSymmetry", "SlotCanonicalization", "build_slot_carries_editor_metadata_without_replacing_geometry")) {
    if ($buildSlot -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildSlot must own MVP2 editor slot contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("board_size: BoardSize", "label: Option<String>", "TemplateSymmetry", "TemplateCanonicalization", "build_template_carries_editor_import_export_metadata")) {
    if ($buildTemplate -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildTemplate must own MVP2 template geometry/import/export metadata marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("TemplateImportFormat", "TemplateExportFormat", "TemplateExport", "accepts_raw_text", "TemplateJsonReader::from_value", "TemplateJsonWriter::to_value", "from_json", "to_json", "native_json_template_import_export_roundtrips_editor_contract", "typed_template_import_export_contract_carries_interpreted_template_only", "native_json_template_import_rejects_out_of_bounds_cell", "native_json_template_import_rejects_duplicate_slot_cells", "native_json_template_import_rejects_duplicate_allowed_pieces", "native_json_template_import_rejects_required_piece_outside_allowed_pieces")) {
    if ($templateImport -notlike "*$requiredMarker*") {
        Add-ArchitectureError "TemplateImport facade must expose typed interpreted-template contract marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("fn parse_template", "fn parse_cell", "fn template_to_json", "fn required_field", "fn parse_template_symmetry", "fn parse_slot_order_constraint")) {
    if ($templateImport -like "*$forbiddenMarker*") {
        Add-ArchitectureError "TemplateImport facade must delegate parser/writer/schema detail marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("TemplateJsonReader", "parse_template", "parse_slot", "parse_cell", "CellCoord::new", "validate_unique_cells", "validate_unique_allowed_pieces", "validate_required_piece_allowed", "outside board", "duplicate cell", "duplicate allowed piece")) {
    if ($templateJsonReader -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_reader.rs must own JSON-to-BuildTemplate parsing and import-time validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("TemplateJsonWriter", "template_to_json", "slot_to_json", "order_constraint_to_json")) {
    if ($templateJsonWriter -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_writer.rs must own BuildTemplate-to-JSON writing marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("TemplateJsonError", "TemplateExportError", "InvalidJson", "UnsupportedSchemaVersion", "InvalidField")) {
    if ($templateJsonError -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_error.rs must own template JSON error contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("required_field", "optional_field", "required_array", "required_string", "required_u64", "required_u32", "required_u16", "invalid_field")) {
    if ($templateJsonFields -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_fields.rs must own template JSON field helper marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("parse_piece_array", "optional_piece", "parse_template_symmetry", "parse_template_canonicalization", "parse_slot_hold_constraint", "parse_slot_order_constraint", "parse_slot_symmetry", "parse_slot_canonicalization")) {
    if ($templateJsonEnums -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_enums.rs must own template enum parsing marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("NATIVE_TEMPLATE_SCHEMA_VERSION", "validate_template_schema", "validate_board_fields", "validate_slot_fields", "validate_order_constraint_fields", "validate_cell_fields")) {
    if ($templateJsonSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "template_json_schema.rs must own native JSON schema marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("validate_template(query", "validate_domains(query", "validate_constraints(query", "validate_impossible_assignment(query", "validate_limits(query", "build_query_supported_diagnostic")) {
    if ($buildQueryValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildQueryValidator orchestrator must delegate build validation marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("slot_cell_out_of_bounds", "overlapping_slot_cells", "domain_piece_not_allowed_by_template", "required_piece_conflict", "AssignmentCsp::new", "pattern_limit_exceeded")) {
    if ($buildQueryValidator -like "*$forbiddenMarker*") {
        Add-ArchitectureError "BuildQueryValidator orchestrator must not own validator detail marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("empty_template", "duplicate_slot_id", "validate_slot_geometry", "validate_template_slot_domain", "validate_slot_order_constraint")) {
    if ($buildTemplateValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_template_validator.rs must own template-level build validation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("empty_slot_cells", "slot_cell_out_of_bounds", "duplicate_slot_cell", "overlapping_slot_cells")) {
    if ($buildSlotGeometryValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_slot_geometry_validator.rs must own slot geometry marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("self_referential_slot_order", "unknown_slot_order_reference")) {
    if ($buildSlotOrderValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_slot_order_validator.rs must own slot order marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("empty_template_slot_domain", "template_required_piece_not_in_domain", "missing_slot_domain", "unknown_slot_domain", "empty_slot_domain", "domain_piece_not_allowed_by_template", "duplicate_domain_piece", "duplicate_slot_domain", "domain_for_slot")) {
    if ($buildSlotDomainValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_slot_domain_validator.rs must own slot domain marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("unknown_slot_constraint", "required_piece_not_in_domain", "required_piece_conflict")) {
    if ($buildConstraintValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_constraint_validator.rs must own slot constraint marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("AssignmentCsp::new", "assignment_contract_is_well_formed", "effective_constraints", "impossible_assignment")) {
    if ($buildAssignmentFeasibilityValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_assignment_feasibility_validator.rs must own assignment feasibility marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("zero_pattern_count", "zero_build_limit", "pattern_limit_exceeded")) {
    if ($buildLimitValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "build_limit_validator.rs must own build limit marker '$requiredMarker'"
    }
}
foreach ($file in Get-RustFiles "crates/clearra-build-coverage/src") {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($forbiddenMarker in @("clearra_fumen", "fumen_like", "FumenLike", "v115@", "FumenReader", "FumenWriter")) {
        if ($contents -like "*$forbiddenMarker*") {
            Add-ArchitectureError "$relativePath must not parse raw fumen-like input; clearra-fumen adapters must produce BuildTemplate first"
        }
    }
}
Invoke-TestPolicyArchitectureValidation
Invoke-DependencyArchitectureValidation $workspaceDependencyGraph
Invoke-OpeningScenarioPresetContractValidation
Invoke-SearchProblemCanonicalModelValidation
Invoke-CCompactProblemDescriptorValidation
Invoke-CBoard64CoreValidation
Invoke-CPieceOperationTableValidation
Invoke-CRuleKickCompactModelValidation
Invoke-CSfinderCandidateValidation
Invoke-CReachabilityValidation
Invoke-CGeometryPackingValidation
Invoke-CHostReducerValidation
Invoke-CBuildUpProblemBuilderValidation
Invoke-CBuildUpVerifierValidation
Invoke-CCoverageRowBridgeValidation
Invoke-RustCoverageObjectiveReducerValidation
Invoke-ReplayOutputBridgeValidation
Invoke-CoreExecutorValidation
Invoke-CliProductPathValidation
Invoke-BackendPolicyFallbackValidation
Invoke-SetupSearchProductPathValidation
Invoke-BuildCoverageProductPathValidation
Invoke-RulesKicksRuntimeValidation
Invoke-SupplyRuntimeValidation
Invoke-Board128WideBackendValidation
Invoke-ExactCoverDlxGeneralizationValidation
Invoke-GpuPackingBackendValidation
Invoke-HybridSchedulerValidation
Invoke-PercentPathProductSliceValidation
Invoke-ScoringPostProcessingValidation
Invoke-GuiSchemaValidation
Invoke-DiagnosticsSecurityGateValidation
$cliMarkers = @(
    "struct PcSearchQuery",
    "struct SetupSearchQuery",
    "struct BuildCoverageQuery",
    "struct CoverQueryAssembly",
    "struct SetupQueryAssembly"
)
foreach ($file in Get-RustFiles "crates/clearra-cli/src") {
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($marker in $cliMarkers) {
        if ($contents.Contains($marker)) {
            Add-ArchitectureError "$($file.FullName) defines temporary/canonical query owner marker '$marker'"
        }
    }
}
Assert-CliCommandSurfaceIsSynchronized
$coverAssembler = Read-Text "crates/clearra-cli/src/assemble/cover_query_assembler.rs"
if ($coverAssembler -notlike "*BuildCoverageQuery*" -or $coverAssembler -notlike "*TemplateImport::from_json*" -or $coverAssembler -notlike "*CoverQueryAssemblyError*") {
    Add-ArchitectureError "cover_query_assembler.rs must assemble the canonical BuildCoverageQuery"
}
foreach ($requiredMarker in @("read_json_file", "rejects_sensitive_template_file_paths_before_reading", "assembles_template_file_after_file_guard")) {
    if ($coverAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "cover_query_assembler.rs must guard --template-file input marker '$requiredMarker'"
    }
}
if ($coverAssembler -like "*fs::read_to_string*") {
    Add-ArchitectureError "cover_query_assembler.rs must use file_input_guard instead of direct fs::read_to_string"
}
$fumenFixtureDir = Join-Path $Root "tests/fixtures/fumens"
if (-not (Test-Path -LiteralPath $fumenFixtureDir)) {
    Add-ArchitectureError "tests/fixtures/fumens must exist for fumen-like IO contract fixtures"
} else {
    $fumenFixtures = @(Get-ChildItem -LiteralPath $fumenFixtureDir -File -Filter *.fumen)
    if ($fumenFixtures.Count -eq 0) {
        Add-ArchitectureError "tests/fixtures/fumens must contain at least one .fumen fixture, not only .gitkeep"
    }
    foreach ($fixture in $fumenFixtures) {
        if ($fixture.Length -eq 0) {
            Add-ArchitectureError "$($fixture.FullName) must not be an empty fumen fixture"
        }
    }
}
$fumenFixtureTests = Get-RustFiles "crates/clearra-output/tests"
$fumenFixtureTestFound = $false
foreach ($file in $fumenFixtureTests) {
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    if (
        $contents.Contains("tests") -and
        $contents.Contains("fixtures") -and
        $contents.Contains("fumens") -and
        $contents.Contains("FumenLikeReader") -and
        $contents.Contains("FumenLikeWriter")
    ) {
        $fumenFixtureTestFound = $true
    }
}
if (-not $fumenFixtureTestFound) {
    Add-ArchitectureError "clearra-output must have an integration test that roundtrips tests/fixtures/fumens via FumenLikeReader/FumenLikeWriter"
}
$customPieceFixtureDir = Join-Path $Root "tests/fixtures/pieces"
if (-not (Test-Path -LiteralPath $customPieceFixtureDir)) {
    Add-ArchitectureError "tests/fixtures/pieces must exist for MVP3 custom piece schema fixtures"
} else {
    $customPieceFixturePath = Join-Path $customPieceFixtureDir "mixed_custom_piece_set.json"
    if (-not (Test-Path -LiteralPath $customPieceFixturePath)) {
        Add-ArchitectureError "tests/fixtures/pieces/mixed_custom_piece_set.json must pin the MVP3 mixed custom piece schema"
    } else {
        try {
            $customPieceFixture = Get-Content -LiteralPath $customPieceFixturePath -Raw | ConvertFrom-Json
            if ($customPieceFixture.schema_version -ne 3) {
                Add-ArchitectureError "mixed_custom_piece_set.json must use schema_version 3"
            }
            if ($null -eq $customPieceFixture.piece_set -or
                $null -eq $customPieceFixture.piece_set.id -or
                $null -eq $customPieceFixture.piece_set.pieces) {
                Add-ArchitectureError "mixed_custom_piece_set.json must include piece_set.id and piece_set.pieces"
            }
            $customPieces = @($customPieceFixture.piece_set.pieces | Where-Object { $_.kind -eq "custom" })
            if ($customPieces.Count -eq 0) {
                Add-ArchitectureError "mixed_custom_piece_set.json must include at least one custom piece"
            }
            foreach ($piece in $customPieces) {
                foreach ($field in @("id", "label", "display", "spawn_bounds", "area", "symmetry", "canonical_key", "rotations")) {
                    if ($null -eq $piece.$field) {
                        Add-ArchitectureError "custom piece fixture entry must include '$field'"
                    }
                }
            }
            if ($null -eq $customPieceFixture.bag -or
                $null -eq $customPieceFixture.bag.piece_set_id -or
                $customPieceFixture.bag.piece_set_id -ne $customPieceFixture.piece_set.id) {
                Add-ArchitectureError "custom piece fixture bag must reference piece_set.id"
            }
            if ($null -eq $customPieceFixture.bag.entries -or
                $null -eq $customPieceFixture.bag.boundary_models -or
                $null -eq $customPieceFixture.bag.bag_size) {
                Add-ArchitectureError "custom piece fixture bag must define entries, bag_size, and boundary_models"
            } else {
                foreach ($entry in @($customPieceFixture.bag.entries)) {
                    foreach ($field in @("piece_id", "multiplicity", "weight")) {
                        if ($null -eq $entry.$field) {
                            Add-ArchitectureError "custom piece fixture bag entry must include '$field'"
                        }
                    }
                }
                foreach ($field in @("fixed_sequence", "observed_window", "bag_aligned_pattern")) {
                    if ($null -eq $customPieceFixture.bag.boundary_models.$field) {
                        Add-ArchitectureError "custom piece fixture bag boundary_models must include '$field'"
                    }
                }
            }
            if ($customPieceFixture.expected.diagnostic_code -ne "E_CUSTOM_PIECE_UNSUPPORTED_MVP") {
                Add-ArchitectureError "custom piece fixture must expect E_CUSTOM_PIECE_UNSUPPORTED_MVP until runtime support is connected"
            }
            if ($customPieceFixture.expected.bag_diagnostic_code -ne "E_CUSTOM_BAG_UNSUPPORTED_MVP") {
                Add-ArchitectureError "custom piece fixture must expect E_CUSTOM_BAG_UNSUPPORTED_MVP until supply runtime support is connected"
            }
        } catch {
            Add-ArchitectureError "mixed_custom_piece_set.json must be valid JSON"
        }
    }
}
$scenarioFixtureDir = Join-Path $Root "tests/fixtures/pc"
$obsoleteScenarioTraceKeyContract = "accepted_" + "sample_trace_keys"
if (-not (Test-Path -LiteralPath $scenarioFixtureDir)) {
    Add-ArchitectureError "tests/fixtures/pc must exist for normalized PC scenario fixtures"
} else {
    $scenarioFixtures = @(Get-ChildItem -LiteralPath $scenarioFixtureDir -File -Filter *.json)
    $scenarioProcessE2e = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
    if ($scenarioFixtures.Count -eq 0) {
        Add-ArchitectureError "tests/fixtures/pc must contain normalized JSON scenario fixtures, not only .gitkeep"
    }
    $supportedScenarioFound = $false
    $requires180UnsupportedFound = $false
    foreach ($fixture in $scenarioFixtures) {
        if ($fixture.Length -eq 0) {
            Add-ArchitectureError "$($fixture.FullName) must not be an empty scenario fixture"
            continue
        }
        try {
            $scenarioFixture = Get-Content -LiteralPath $fixture.FullName -Raw | ConvertFrom-Json
        } catch {
            Add-ArchitectureError "$($fixture.FullName) must be valid JSON"
            continue
        }
        $fixtureKind = if ($scenarioFixture.PSObject.Properties.Name -contains "kind") {
            [string]$scenarioFixture.kind
        } else {
            "scenario-pc-fixture"
        }
        if ($fixtureKind -eq "opening-pc-fixture") {
            foreach ($requiredOpeningProperty in @("name", "command", "problem", "expected")) {
                if (-not ($scenarioFixture.PSObject.Properties.Name -contains $requiredOpeningProperty)) {
                    Add-ArchitectureError "$($fixture.FullName) opening fixture must include '$requiredOpeningProperty'"
                }
            }
            if ($scenarioFixture.expected.solution_exists -ne $true -or
                $scenarioFixture.expected.packing_candidate_is_solution -ne $false) {
                Add-ArchitectureError "$($fixture.FullName) opening fixture must pin solved output while keeping PackingCandidate separate from solution"
            }
            continue
        }
        if ($null -eq $scenarioFixture.source -or
            $null -eq $scenarioFixture.source.site -or
            $null -eq $scenarioFixture.source.page -or
            $null -eq $scenarioFixture.source.section -or
            $null -eq $scenarioFixture.source.human_verified) {
            Add-ArchitectureError "$($fixture.FullName) must include source.site/page/section/human_verified metadata"
        }
        if ($null -eq $scenarioFixture.scenario) {
            Add-ArchitectureError "$($fixture.FullName) must include canonical scenario input"
        } else {
            $scenarioPropertyNames = $scenarioFixture.scenario.PSObject.Properties.Name
            foreach ($requiredScenarioProperty in @("board_width", "visible_height", "initial_board_mask", "remaining_queue", "goal", "max_pieces", "exact_pieces", "min_remaining_queue", "allow_hold", "count_policy", "retained_trace_limit")) {
                if (-not ($scenarioPropertyNames -contains $requiredScenarioProperty)) {
                    Add-ArchitectureError "$($fixture.FullName) must include canonical scenario property '$requiredScenarioProperty'"
                }
            }
        }
        if ($null -eq $scenarioFixture.expected -or
            -not ($scenarioFixture.expected.PSObject.Properties.Name -contains "expected_total_solution_count") -or
            $null -eq $scenarioFixture.expected.count_complete) {
            Add-ArchitectureError "$($fixture.FullName) must include expected_total_solution_count and count_complete contract fields"
        }
        if ($null -ne $scenarioFixture.expected) {
            $expectedPropertyNames = $scenarioFixture.expected.PSObject.Properties.Name
            if ($expectedPropertyNames -contains $obsoleteScenarioTraceKeyContract) {
                Add-ArchitectureError "$($fixture.FullName) must use accepted_retained_trace_keys; the removed trace-key fixture field is ambiguous"
            }
            if (-not ($expectedPropertyNames -contains "accepted_retained_trace_keys")) {
                Add-ArchitectureError "$($fixture.FullName) must include accepted_retained_trace_keys for retained trace allow-list semantics"
            }
        }
        if ($scenarioFixture.scenario.goal -ne "clear-to-empty") {
            Add-ArchitectureError "$($fixture.FullName) must use clear-to-empty as the scenario fixture goal"
        }
        if ($scenarioFixture.scenario.requires_180 -eq $true) {
            if ($scenarioFixture.expected.unsupported -eq $true -and
                $scenarioFixture.expected.unsupported_reason -eq "scenario_requires_180_unsupported") {
                $requires180UnsupportedFound = $true
            } else {
                Add-ArchitectureError "$($fixture.FullName) requires_180 fixture must be classified as unsupported with scenario_requires_180_unsupported"
            }
        } else {
            if ($scenarioFixture.expected.solution_exists -eq $true -and
                $null -ne $scenarioFixture.expected.expected_total_solution_count) {
                $supportedScenarioFound = $true
                $fixtureRelative = (Resolve-Path -LiteralPath $fixture.FullName -Relative).TrimStart(".", "\", "/").Replace("\", "/")
                foreach ($requiredE2eMarker in @($fixtureRelative, "--verify-expected", "expected_match", "expected_total_solution_count")) {
                    if (-not $scenarioProcessE2e.Contains($requiredE2eMarker)) {
                        Add-ArchitectureError "$fixtureRelative has expected_total_solution_count and must be compared against actual in process E2E marker '$requiredE2eMarker'"
                    }
                }
            }
        }
    }
    if (-not $supportedScenarioFound) {
        Add-ArchitectureError "tests/fixtures/pc must include at least one supported fixture with expected_total_solution_count"
    }
    if (-not $requires180UnsupportedFound) {
        Add-ArchitectureError "tests/fixtures/pc must include a requires_180 unsupported fixture"
    }
}
$productAcceptanceContracts = @(
    @{ Path = "tests/fixtures/pc/opening_2l_empty.json"; Marker = "opening-pc-fixture" },
    @{ Path = "tests/fixtures/pc/scenario_simple_4l.json"; Marker = "scenario_simple_4l" },
    @{ Path = "tests/fixtures/buildup/queue_order_mismatch.json"; Marker = "queue_order_impossible" },
    @{ Path = "tests/fixtures/buildup/hold_branch_required.json"; Marker = "preserves_hold_branch_kind" },
    @{ Path = "tests/fixtures/coverage/overlap_two_variants_one_pattern.json"; Marker = "variant_probability_sum" },
    @{ Path = "tests/fixtures/continuation/pc_then_next_pc_available.json"; Marker = "continuation_token_version" },
    @{ Path = "tests/fixtures/setup/simple_family_union.json"; Marker = "simple_family_union" },
    @{ Path = "tests/golden/pc/opening_2l_empty.json"; Marker = "coverage_source=portable-preview-not-pattern-specific" },
    @{ Path = "tests/golden/pc/scenario_simple_4l.json"; Marker = "scenario_replay_token_version=sr2" },
    @{ Path = "tests/golden/coverage/overlap_union_probability.json"; Marker = "variant_probability_sum=forbidden" },
    @{ Path = "tests/golden/continuation/next_pc_available.json"; Marker = "continuation_token_version=pc2" },
    @{ Path = "tests/golden/setup/simple_family_probability.json"; Marker = "coverage_probability=0.75" }
)
foreach ($contract in $productAcceptanceContracts) {
    $contractPath = Join-Path $Root $contract.Path
    if (-not (Test-Path -LiteralPath $contractPath)) {
        Add-ArchitectureError "MVP1.5 product acceptance contract file is missing: $($contract.Path)"
        continue
    }
    $contents = Get-Content -LiteralPath $contractPath -Raw
    if ($contents -notlike "*$($contract.Marker)*") {
        Add-ArchitectureError "MVP1.5 product acceptance contract '$($contract.Path)' must pin marker '$($contract.Marker)'"
    }
}
