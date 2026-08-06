# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# It owns the external PC WorkerE2E fixture/source/normalization contract.

function Invoke-WorkerE2EContractValidation() {
    
$requiredWorkerE2EFiles = @(
        "scripts/worker-e2e.ps1",
        "scripts/lib/worker-e2e-fixture.ps1",
        "scripts/lib/worker-e2e-source-registry.ps1",
        "scripts/lib/worker-e2e-fumen-normalize.ps1",
        "scripts/lib/worker-e2e-backend-assertions.ps1",
        "scripts/lib/worker-e2e-solution-set-assertions.ps1",
        "docs/external-pc-fixtures.md",
        "tests/fixtures/external-pc/source_registry.json",
        "tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json",
        "tests/fixtures/external-pc/pco_opener_full_63.source_solutions.json",
        "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json",
        "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.source_solutions.json",
        "tests/golden/external-pc/pco_i_hold_6p_second_bag_pc.json",
        "tests/golden/external-pc/tsar_cannon_after_2bag_full_42.json"
    )
foreach ($requiredWorkerE2EFile in $requiredWorkerE2EFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredWorkerE2EFile))) {
            Add-ArchitectureError "WorkerE2E required contract file is missing: $requiredWorkerE2EFile"
        }
    }
$workerE2EContractMarkerFiles = @(
        $requiredWorkerE2EFiles +
        @(
            "scripts/clearra.ps1",
            "docs/external-pc-fixtures.md",
            "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.normalize.json",
            "crates/clearra-core-executor/tests/external_pc_backend_equivalence.rs",
            "crates/clearra-core-executor/src/buildup/buildup_solution_set_contract.rs"
        )
    ) | Select-Object -Unique
$workerE2EContractSurface = (($workerE2EContractMarkerFiles | ForEach-Object { Read-Text $_ }) -join "`n")
foreach ($requiredMarker in @(
            "WorkerE2E",
            "WorkerE2EStress",
            "WorkerAcceptance",
            "pco_i_hold_6p_second_bag_pc",
            "four-pco-opener-full-63",
            "tsar_cannon_after_2bag_full_42",
            "hse30-tsar-cannon-full-42",
            "pcinfo-korea-pco-6p-i-hold",
            "external-pc-source-solution-set",
            "source_page_count",
            "operation_replay_available=false",
            "minimal_solve_set_is_metadata_only",
            "expected_unique_solution_count=42",
            "fumen-normalized-solution-set",
            "worker-e2e-normalized-solution-key-fnv64-v1",
            "initial_fumen_is_source_of_truth",
            "materialized_scenario_is_cache_only",
            "worker_e2e_rejects_trivial_stub_materialization",
            "actual_solution_set_contract",
            "normalized_solution_key_algorithm",
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_unique_solution_count",
            "actual_normalized_solution_set_hash",
            "backend_cpu_gpu_hybrid_equivalence",
            "packing_candidate_is_solution=false",
            "coverage_row_created_after_buildup=true"
        )) {
        if ($workerE2EContractSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WorkerE2E contract surface must pin required marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "image_pixel_golden",
            "raw_fumen_string_exact_match",
            "minimal_solve_count_as_unique_solution_count",
            "harddrop_tsar_solution_count_as_primary",
            "johnbeak_tsar_solution_count_as_primary"
        )) {
        if ($workerE2EContractSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "WorkerE2E contract surface must not contain forbidden marker '$forbiddenMarker'"
        }
    }
$externalPcFixtureDoc = Read-Text "docs/external-pc-fixtures.md"
foreach ($requiredMarker in @(
            "External PC fixtures are human-verified product E2E fixtures.",
            "They are not source image mirrors.",
            "They must use typed board masks or normalized fumen.",
            "input.initial_fumen",
            "source of truth",
            "input.materialized_scenario",
            "optional cache material",
            "initial_fumen_is_source_of_truth",
            "materialized_scenario_is_cache_only",
            "worker_e2e_rejects_trivial_stub_materialization",
            "E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH",
            "Minimal solve set is metadata unless explicitly used as a learning-cover test.",
            "Worker correctness uses unique normalized solution set.",
            "PCO fixture uses I-hold 6p PCO only.",
            "Tsar Cannon fixture uses hse30 full 42 solve fumen only.",
            "Hard Drop and John Beak Tsar data may be reference metadata but not primary correctness source.",
            "For Tsar Cannon, Clearra's worker correctness fixture uses the full unique solve set, not the minimal solve set.",
            "For PCO, Clearra uses the I-hold 6p PCO setup only. 7p PCO, no-hold PCO, and I-placed PCO are out of fixture scope."
        )) {
        if ($externalPcFixtureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/external-pc-fixtures.md must document external PC fixture policy marker '$requiredMarker'"
        }
    }
foreach ($requiredPath in @(
            "tests/fixtures/external-pc/source_registry.json",
            "tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json",
            "tests/fixtures/external-pc/pco_opener_full_63.source_solutions.json",
            "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json",
            "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.normalize.json",
            "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.source_solutions.json",
            "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_setup.fumen",
            "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_expected_any.fumen",
            "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_setup.fumen",
            "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen",
            "tests/golden/external-pc/pco_i_hold_6p_second_bag_pc.json",
            "tests/golden/external-pc/tsar_cannon_after_2bag_full_42.json",
            "scripts/worker-e2e.ps1",
            "scripts/lib/worker-e2e-fixture.ps1",
            "scripts/lib/worker-e2e-source-registry.ps1",
            "scripts/lib/worker-e2e-fumen-normalize.ps1",
            "scripts/lib/worker-e2e-json-assertions.ps1",
            "scripts/lib/worker-e2e-backend-assertions.ps1",
            "scripts/lib/worker-e2e-solution-set-assertions.ps1",
            "scripts/lib/worker-e2e-process.ps1",
            "scripts/lib/worker-e2e-runner.ps1",
            "crates/clearra-fumen/src/codec/fumen_like_reader.rs",
            "crates/clearra-fumen/src/normalize/fumen_normalizer.rs",
            "crates/clearra-fumen/src/normalize/normalized_fumen_document.rs",
            "crates/clearra-fumen/src/normalize/normalized_fumen_page.rs",
            "crates/clearra-fumen/src/normalize/normalized_solution_key.rs",
            "crates/clearra-cli/src/fixture/external_pc_fixture_materializer.rs",
            "crates/clearra-cli/src/fixture/external_pc_fixture_materializer_fields.rs",
            "crates/clearra-cli/src/fixture/external_pc_fixture_materializer_fumen.rs",
            "crates/clearra-cli/src/fixture/pc_scenario_fixture_tests.rs",
            "crates/clearra-cli/src/fixture/pc_scenario_fixture.rs",
            "crates/clearra-invariant-tests/tests/external_pc_worker_contract.rs",
            "crates/clearra-core-executor/tests/external_pc_backend_equivalence.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_external_pc_contract_tests.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "WorkerE2E contract file is missing: $requiredPath"
        }
    }
$sourceRegistry = Read-Text "tests/fixtures/external-pc/source_registry.json"
foreach ($requiredMarker in @(
            "external-pc-source-registry",
            "pcinfo-korea-pco-6p-i-hold",
            "https://sites.google.com/view/pcinfokorea/",
            "four-pco-opener-full-63",
            "https://four.lol/perfect-clears/opener/",
            "pco_opener_full_63.source_solutions.json",
            "hse30-tsar-cannon-full-42",
            "https://hse30.tistory.com/1224",
            "retrieved_at",
            "2026-07-07",
            "external-reference-metadata-only",
            "external-fumen-reference",
            "preferred_fumen_link_label",
            "preferred_fumen_source_url",
            "https://fumen.zui.jp/?D115@",
            "source_link_retrieved_at",
            "human_verified_required",
            "redistribution_note",
            "metadata-only; Clearra stores reconstructed typed board/fumen fixtures, not source images",
            "store normalized fumen fixture and source metadata; avoid image golden"
        )) {
        if ($sourceRegistry -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WorkerE2E source registry must pin marker '$requiredMarker'"
        }
    }
$pcoFixture = Read-Text "tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json"
foreach ($requiredMarker in @(
            "pco_i_hold_6p_second_bag_pc",
            "external-pc-worker-fixture",
            "pcinfo-korea-pco-6p-i-hold",
            "`"human_verified`": true",
            "Use only I-hold 6p PCO.",
            "Exclude 7p PCO and no-hold PCO.",
            "Source images are not stored; fixture is reconstructed.",
            "`"family`": `"pco`"",
            "`"phase`": `"second-bag-pc-entry`"",
            "`"setup_kind`": `"i-hold-6p-pco`"",
            "`"command`": `"pc-scenario`"",
            "pco_i_hold_6p_second_bag_pc_setup.fumen",
            "`"hold_piece`": `"I`"",
            "`"hold_empty`": false",
            "`"setup_contract`": `"human-verified-reconstructed-pco-entry`"",
            "`"placed_piece_count`": 6",
            "`"placed_piece_sequence`": `"TSZLOJ`"",
            "`"left_side_piece_sequence`": `"TSZ`"",
            "`"left_tsz_side_mask`": `"0x00000000000f0f0f`"",
            "`"right_4x3_box`": true",
            "`"right_4x3_box_mask`": `"0x000000003c0f03c0`"",
            "`"right_4x3_box_columns`": `"6-9`"",
            "`"right_4x3_box_rows`": `"0-2`"",
            "`"second_bag_pc_entry`": true",
            "`"remaining_queue_source`": `"second-bag`"",
            "`"piece_window`": 4",
            "`"exact_pieces`": 4",
            "`"goal`": `"clear-to-empty`"",
            "`"rule`": `"srs-plus`"",
            "`"allow_hold`": true",
            "`"backend_modes`"",
            "source_solution_labels",
            "source_solution_sets",
            "pco_opener_full_63.source_solutions.json",
            "`"source_page_count`": 63",
            "`"source_unique_label_count`": 58",
            "`"operation_replay_available`": false",
            "`"worker_correctness_basis`": false",
            "I-OIJ",
            "I-OLZ",
            "I-OJZ",
            "I-IJS",
            "`"may_require_180`": true",
            "with-t-and-lj",
            "I-TJZ",
            "`"solution_exists`": true",
            "`"final_board_empty`": true",
            "`"worker_correctness_gate_enabled`": false",
            "operation_replay_unavailable_for_pco_source_labels",
            "`"packing_candidate_is_solution`": false",
            "`"coverage_row_created_after_buildup`": true",
            "`"normalized_fumen_output_required`": true",
            "at-least-one-source-label-family",
            "`"exact_unique_solve_count_required`": false",
            "`"max_candidates`": 50000",
            "`"max_patterns`": 5040",
            "`"timeout_ms`": 15000",
            "`"trace_retention`": `"representative`""
        )) {
        if ($pcoFixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PCO external PC fixture must pin marker '$requiredMarker'"
        }
    }
$tsarFixture = Read-Text "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json"
foreach ($requiredMarker in @(
            "tsar_cannon_after_2bag_full_42",
            "external-pc-worker-fixture",
            "hse30-tsar-cannon-full-42",
            "`"human_verified`": true",
            "Use only the post-2-bag PC phase.",
            "Use hse30 full 42 fumen.zui.jp solve set as primary source.",
            "Do not use Hard Drop minimal solve count as correctness basis.",
            "Do not use John Beak extra O/O placement variants as correctness basis.",
            "`"family`": `"tsar-cannon`"",
            "`"phase`": `"third-bag-pc-after-fixed-second-bag`"",
            "`"setup_kind`": `"after-2bag-tst-tsd`"",
            "`"stress_level`": `"stress`"",
            "`"command`": `"pc-scenario`"",
            "tsar_cannon_after_2bag_setup.fumen",
            "tsar_cannon_after_2bag_full_42.fumen",
            "tsar_cannon_after_2bag_full_42.normalize.json",
            "tsar_cannon_after_2bag_full_42.source_solutions.json",
            "`"source_unique_label_count`": 39",
            "`"operation_replay_available`": false",
            "`"remaining_queue_source`": `"third-bag`"",
            "`"piece_window`": 6",
            "`"exact_pieces`": 6",
            "`"goal`": `"clear-to-empty`"",
            "`"rule`": `"srs-plus`"",
            "`"allow_hold`": true",
            "`"backend_modes`"",
            "`"minimal_solve_set`": 18",
            "`"minimal_plus_tspin_extra`": 25",
            "`"unique_solve_set`": 42",
            "`"worker_correctness_basis`": `"unique_solve_set`"",
            "`"expected_unique_solution_count`": 42",
            "`"minimal_solve_set_is_metadata_only`": true",
            "`"solution_exists`": true",
            "`"final_board_empty`": true",
            "`"unique_solution_count_basis`": `"normalized-fumen-solution-set`"",
            "`"pc_probability_source_percent`": `"98.69`"",
            "`"tsd_pc_probability_source_percent`": `"73.2`"",
            "`"packing_candidate_is_solution`": false",
            "`"coverage_row_created_after_buildup`": true",
            "`"normalized_fumen_output_required`": true",
            "`"max_candidates`": 250000",
            "`"max_patterns`": 5040",
            "`"timeout_ms`": 60000",
            "`"trace_retention`": `"bounded-representative-set`"",
            "`"allow_count_incomplete`": false"
        )) {
        if ($tsarFixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Tsar external PC fixture must pin marker '$requiredMarker'"
        }
    }
$workerE2E = Read-Text "scripts/worker-e2e.ps1"
$workerE2EFixtureLib = Read-Text "scripts/lib/worker-e2e-fixture.ps1"
$workerE2ESourceRegistry = Read-Text "scripts/lib/worker-e2e-source-registry.ps1"
$workerE2EJsonAssertions = Read-Text "scripts/lib/worker-e2e-json-assertions.ps1"
$workerE2EBackendAssertions = Read-Text "scripts/lib/worker-e2e-backend-assertions.ps1"
$workerE2ESolutionAssertions = Read-Text "scripts/lib/worker-e2e-solution-set-assertions.ps1"
$workerE2EProcess = Read-Text "scripts/lib/worker-e2e-process.ps1"
$workerE2ERunner = Read-Text "scripts/lib/worker-e2e-runner.ps1"
$pcScenarioFixture = Read-Text "crates/clearra-cli/src/fixture/pc_scenario_fixture.rs"
$externalPcFixtureMaterializer = Read-Text "crates/clearra-cli/src/fixture/external_pc_fixture_materializer.rs"
$externalPcFixtureMaterializerFields = Read-Text "crates/clearra-cli/src/fixture/external_pc_fixture_materializer_fields.rs"
$externalPcFixtureMaterializerFumen = Read-Text "crates/clearra-cli/src/fixture/external_pc_fixture_materializer_fumen.rs"
$pcScenarioFixtureTests = Read-Text "crates/clearra-cli/src/fixture/pc_scenario_fixture_tests.rs"
$externalPcInvariantTests = Read-Text "crates/clearra-invariant-tests/tests/external_pc_worker_contract.rs"
$externalPcBackendEquivalence = Read-Text "crates/clearra-core-executor/tests/external_pc_backend_equivalence.rs"
foreach ($requiredMarker in @(
            "UseBuiltBinary",
            "ExePath",
            "Extended",
            "Stress",
            "Workers",
            "OutputExcerptLines",
            "Assert-WorkerE2ESourceRegistryContract",
            "Assert-WorkerE2EExternalPcFumenContracts",
            "worker-e2e-json-assertions.ps1",
            "worker-e2e-backend-assertions.ps1",
            "worker-e2e-process.ps1",
            "Invoke-WorkerE2EBackendRunCase",
            "PCO I-hold 6p second-bag PC",
            "Tsar Cannon after 2-bag full 42",
            "New-ClearraProgressScope",
            "external PC worker contracts passed"
        )) {
        if ($workerE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/worker-e2e.ps1 must enforce WorkerE2E marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Invoke-WorkerE2EClearra",
            "Refusing to launch stale CLI binary",
            "clearra-cli.exe"
        )) {
        if ($workerE2EProcess -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-process.ps1 must enforce WorkerE2E process marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "ConvertFrom-WorkerE2EJsonOutput",
            "Add-WorkerE2EJsonFieldValues",
            "Get-WorkerE2EJsonFieldScalar",
            "Assert-WorkerE2EJsonFieldEquals",
            "Assert-WorkerE2EJsonFieldSame",
            "reported inconsistent values"
        )) {
        if ($workerE2EJsonAssertions -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-json-assertions.ps1 must enforce WorkerE2E JSON marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Assert-WorkerE2EBackendOutput",
            "Assert-WorkerE2EBackendEquivalence",
            "Assert-WorkerE2EBackendSolutionSetMatchesSource",
            "Assert-WorkerE2EBackendGateIsNotSilentlyEnabled",
            "Assert-WorkerE2EGpuUnavailableReason",
            "Assert-WorkerE2EHybridUnavailableReason",
            "Assert-WorkerE2ENoFallbackReason",
            "backend_fallback_reason",
            "actual_solution_set_contract",
            "normalized_solution_key_algorithm",
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_unique_solution_count",
            "actual_normalized_solution_set_hash",
            "gpu_backend_not_connected",
            "gpu_device_not_found",
            "gpu_kernel_unavailable",
            "gpu_transient_before_commit",
            "gpu_resource_incomplete",
            "cpu-selected"
        )) {
        if ($workerE2EBackendAssertions -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-backend-assertions.ps1 must enforce WorkerE2E backend marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Assert-WorkerE2EMinimalSolveSetIsMetadataOnly",
            "Assert-WorkerE2ETsarUniqueSolveSetContract"
        )) {
        if ($workerE2ESolutionAssertions -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-solution-set-assertions.ps1 must enforce WorkerE2E solution marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "New-WorkerE2ECommandArgs",
            "New-WorkerE2EFailureMessage",
            "Invoke-WorkerE2EBackendRunCase",
            "Assert-WorkerE2EBackendEquivalence",
            "Test-WorkerE2EFixtureBackendExecutionEnabled",
            "metadata-only fixture"
        )) {
        if ($workerE2ERunner -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-runner.ps1 must enforce WorkerE2E runner marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "ExternalPcFixtureMaterializer",
            "external-pc-worker-fixture",
            "input.initial_fumen",
            "external_pc_fixture_materializes_from_initial_fumen",
            "external_pc_materialized_scenario_mismatch_is_error",
            "pco_runtime_scenario_uses_setup_fumen_mask",
            "tsar_runtime_scenario_uses_setup_fumen_mask",
            "external_pc_fixture_rejects_trivial_stub_materialization",
            "E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH",
            "input.materialized_expected"
        )) {
        $fixtureMaterializationSurface = "$pcScenarioFixture`n$externalPcFixtureMaterializer`n$externalPcFixtureMaterializerFields`n$externalPcFixtureMaterializerFumen`n$pcScenarioFixtureTests"
        if ($fixtureMaterializationSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PcScenarioFixture external materializer must enforce WorkerE2E marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "pco_i_hold_fixture_preserves_source_metadata",
            "pco_i_hold_fixture_has_required_solution_labels",
            "tsar_cannon_fixture_uses_unique_solve_set_not_minimal_set",
            "tsar_cannon_expected_solution_count_is_42",
            "tsar_cannon_normalize_report_pins_full_42_solution_set",
            "external_pc_source_solution_sets_pin_user_confirmed_counts",
            "external_pc_fixture_does_not_use_raw_image_golden",
            "external_pc_fixture_requires_human_verified_fumen",
            "packing_candidate_is_solution",
            "coverage_row_created_after_buildup",
            "setup_contract",
            "placed_piece_count",
            "right_4x3_box",
            "left_tsz_side_mask",
            "second_bag_pc_entry",
            "image_path",
            "v115@"
        )) {
        if ($externalPcInvariantTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-invariant-tests external PC worker contract must enforce marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "pco_i_hold_worker_correctness_gate_is_metadata_only",
            "pco_i_hold_source_labels_are_not_backend_oracle",
            "pco_i_hold_cpu_gpu_hybrid_use_fumen_materialized_scenario",
            "tsar_cannon_full_42_cpu_finds_42_unique_solutions",
            "tsar_cannon_cpu_finds_42_actual_unique_solutions",
            "tsar_solver_solution_set_hash_matches_full_42_fumen",
            "tsar_cannon_full_42_gpu_request_falls_back_to_cpu_unique_set",
            "tsar_cannon_gpu_fallback_matches_cpu_actual_solution_set",
            "tsar_cannon_full_42_hybrid_matches_cpu_unique_set",
            "tsar_cannon_hybrid_matches_cpu_actual_solution_set",
            "tsar_cannon_gpu_unavailable_without_fallback",
            "tsar_cannon_gpu_request_does_not_fallback_in_acceptance_mode",
            "tsar_cannon_memory_leak_report_clean",
            "assert_equivalent_result_contract",
            "assert_matches_source_solution_set",
            "final_board_empty",
            "unique_solution_count",
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_unique_solution_count",
            "actual_normalized_solution_set_hash",
            "normalized_solution_key_algorithm",
            "actual_solution_set_contract",
            "source_normalized_unique_solution_count",
            "coverage_probability",
            "count_complete",
            "backend_requested",
            "backend_selected",
            "memory_ticket",
            "fence_epoch",
            "raw candidate",
            "trace retention sample order"
        )) {
        if ($externalPcBackendEquivalence -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-executor external PC backend equivalence tests must enforce marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "normalized_solution_set_hash: stable_hash",
            "final_board_empty=",
            "nssk1:",
            "source_hash()"
        )) {
        if ($externalPcBackendEquivalence -like "*$forbiddenMarker*") {
            Add-ArchitectureError "external PC backend equivalence must not use summary/hash injection marker '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @(
            "ConvertTo-WorkerE2EFixtureMarkerText",
            "Assert-WorkerE2ETypedGoldenAssertions",
            "backend_cpu_status=success",
            "backend_gpu_assisted_status=success",
            "backend_hybrid_status=success",
            "gpu_result_cpu_confirmed=true",
            "gpu_cpu_reference_match=true",
            "gpu_assisted_buildup_reached=true",
            "unique_solution_count=",
            "minimal_solve_set_is_metadata_only=",
            "source_solution_label_count_min",
            "count_complete",
            "pc_probability_source_percent",
            "tsd_pc_probability_source_percent",
            "fumen_like_prefix=v115@",
            "ConvertFrom-WorkerE2EFumenLikePages",
            "ConvertFrom-WorkerE2EPageFields",
            "initial_fumen.",
            "normalize_report"
        )) {
        if ($workerE2EFixtureLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-fixture.ps1 must enforce PCO WorkerE2E marker '$requiredMarker'"
        }
    }
$tsarGolden = Read-Text "tests/golden/external-pc/tsar_cannon_after_2bag_full_42.json"
foreach ($requiredMarker in @(
            "worker_correctness_gate_enabled=false",
            "worker_correctness_blocked_reason=native_solver_solution_set_execution_not_connected",
            "unique_solution_count=42",
            "normalized_unique_solution_count=42",
            "source_unique_solution_count=42",
            "page_count=42",
            "decoded_page_count=44",
            "comment_ignored=true",
            "mirror_policy=none",
            "solution_set_hash=wes1:548277ae9ac32701",
            "source_solution_set_id=hse30-tsar-cannon-full-42-v115",
            "source_unique_label_count=39",
            "operation_replay_available=false",
            "worker_correctness_basis=true",
            "minimal_solve_set_is_metadata_only=true",
            "fumen_like_prefix=v115@",
            "typed_assertions",
            "`"expected_unique_solution_count`": 42",
            "`"count_complete`": true",
            "`"pc_probability_source_percent`": `"98.69`"",
            "`"tsd_pc_probability_source_percent`": `"73.2`""
        )) {
        if ($tsarGolden -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Tsar external PC golden must pin marker '$requiredMarker'"
        }
    }
$tsarFixture = Read-Text "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json"
foreach ($requiredMarker in @(
            "`"worker_correctness_gate_enabled`": false",
            "`"worker_correctness_blocked_reason`": `"native_solver_solution_set_execution_not_connected`""
        )) {
        if ($tsarFixture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Tsar external PC fixture must pin marker '$requiredMarker'"
        }
    }
$pcoGolden = Read-Text "tests/golden/external-pc/pco_i_hold_6p_second_bag_pc.json"
foreach ($requiredMarker in @(
            "worker_correctness_gate_enabled=false",
            "worker_correctness_blocked_reason=operation_replay_unavailable_for_pco_source_labels",
            "coverage_row_created_after_buildup=true",
            "setup_contract=human-verified-reconstructed-pco-entry",
            "placed_piece_count=6",
            "placed_piece_sequence=TSZLOJ",
            "left_side_piece_sequence=TSZ",
            "left_tsz_side_mask=0x00000000000f0f0f",
            "right_4x3_box=true",
            "right_4x3_box_mask=0x000000003c0f03c0",
            "second_bag_pc_entry=true",
            "source_solution_set_id=four-pco-opener-full-63",
            "source_page_count=63",
            "source_unique_label_count=58",
            "operation_replay_available=false",
            "worker_correctness_basis=false",
            "fumen_like_prefix=v115@",
            "typed_assertions",
            "source_solution_label_count_min"
        )) {
        if ($pcoGolden -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PCO external PC golden must pin marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Assert-WorkerE2ESourceRegistryShape",
            "Assert-WorkerE2ESourceEntry",
            "source_id",
            "source_url",
            "retrieved_at",
            "source_kind",
            "human_verified_required",
            "preferred_fumen_link_label",
            "preferred_fumen_source_url",
            "source_link_retrieved_at",
            "redistribution_note",
            "external-reference-metadata-only",
            "external-fumen-reference",
            "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID",
            "E_EXTERNAL_PC_SOURCE_MISSING_RETRIEVED_AT",
            "E_EXTERNAL_PC_SOURCE_REQUIRES_HUMAN_VERIFICATION"
        )) {
        if ($workerE2ESourceRegistry -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/lib/worker-e2e-source-registry.ps1 must enforce source registry marker '$requiredMarker'"
        }
    }
$clearra = Read-Text "scripts/clearra.ps1"
foreach ($requiredMarker in @(
            "`"WorkerE2E`"",
            "`"WorkerE2EStress`"",
            "`"WorkerAcceptance`"",
            "`"WorkerRelease`"",
            "scripts/worker-e2e.ps1"
        )) {
        if ($clearra -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/clearra.ps1 must expose WorkerE2E task marker '$requiredMarker'"
        }
    }
$fumenHarness = Read-Text "crates/clearra-fumen/src/codec/fumen_like_reader.rs"
$fumenNormalizer = Read-Text "crates/clearra-fumen/src/normalize/fumen_normalizer.rs"
$fumenSolutionKey = Read-Text "crates/clearra-fumen/src/normalize/normalized_solution_key.rs"
$workerE2EFumenNormalize = Read-Text "scripts/lib/worker-e2e-fumen-normalize.ps1"
$gpuHarness = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_external_pc_contract_tests.rs"
foreach ($requiredMarker in @(
            "pco_external_pc_fumen_files_decode_as_contract_payloads",
            "tsar_external_pc_fumen_contract_pins_full_42_metadata"
        )) {
        if ($fumenHarness -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-fumen unit tests must enforce WorkerE2E fumen marker '$requiredMarker' without adding a new integration test executable"
        }
    }
foreach ($requiredMarker in @(
            "FumenNormalizer",
            "NormalizedFumenDocument",
            "NormalizedFumenPage",
            "NormalizedSolutionKey",
            "external_pc_fumen_decode_preserves_initial_board",
            "external_pc_fumen_decode_preserves_solution_pages",
            "external_pc_fumen_normalization_ignores_comments",
            "external_pc_fumen_normalization_preserves_piece_sequence",
            "external_pc_fumen_solution_key_is_stable",
            "tsar_cannon_full_42_fumen_has_42_unique_solution_keys",
            "tsar_cannon_full_42_fumen_decodes_to_42_unique_solution_keys",
            "normalized_solution_key_hash_is_shared_between_rust_and_worker_e2e",
            "pco_i_hold_setup_fumen_decodes_to_scenario_board",
            "normalized_solution_key_ignores_comments",
            "normalized_solution_key_preserves_piece_sequence",
            "normalized_solution_key_preserves_hold_decisions",
            "normalized_solution_key_preserves_line_clear_events",
            "solution_key_count()",
            "hold_decision_sequence",
            "line_clear_events",
            "mirror_policy",
            "0x0000_0000_3c0f_0fcf",
            "placed_piece_count",
            "right_4x3_box",
            "left_tsz_side_mask",
            "second_bag_pc_entry",
            "0x0000_0000_00f3_c3f0"
        )) {
        if ($fumenNormalizer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-fumen normalizer must enforce WorkerE2E normalize marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "initial_board_mask",
            "final_board_mask",
            "piece_sequence",
            "hold_decision_sequence",
            "operation_sequence",
            "cleared_line_sequence",
            "mirror_policy",
            "normalized_shape_key",
            "normalized_tiling_key"
        )) {
        if ($fumenSolutionKey -notlike "*$requiredMarker*") {
            Add-ArchitectureError "NormalizedSolutionKey must preserve marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "raw_fumen_string_exact_equality=true",
            "expected_fumen_string == actual_fumen_string",
            "expected_fumen_string==actual_fumen_string"
        )) {
        if ($workerE2EFumenNormalize -notlike "*$forbiddenMarker*") {
            Add-ArchitectureError "WorkerE2E fumen normalizer policy must forbid raw equality marker '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @(
            "Get-WorkerE2ENormalizedSolutionSetHash",
            "expected_solution_normalize_report",
            "external-pc-normalize-report",
            "source_unique_solution_count",
            "normalized_unique_solution_count",
            "solution_set_hash",
            "wes1:"
        )) {
        if ($workerE2EFumenNormalize -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WorkerE2E fumen normalizer must enforce normalize report marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "worker_external_pc_gpu_descriptor_uses_piece_source_and_multiset",
            "pco_worker_external_pc_fixture_declares_human_verified_backend_contract",
            "tsar_worker_external_pc_fixture_uses_full_42_unique_set_contract"
        )) {
        if ($gpuHarness -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU worker unit tests must enforce WorkerE2E marker '$requiredMarker' without adding a new integration test executable"
        }
    }
}
