# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-CCoreTestMatrixContractValidation() {
$requiredTestGroups = @(
        "memory_tests",
        "board64_tests",
        "board_backend_dispatch_tests",
        "operation_table_tests",
        "rule_profile_tests",
        "supply_tests",
        "cache_identity_tests",
        "candidate_tests",
        "reachability_tests",
        "packing_tests",
        "gpu_tests",
        "scheduler_tests",
        "buildup_tests",
        "coverage_tests",
        "scoring_event_tests"
    )
$cmake = Read-Text "core-c/CMakeLists.txt"
$aggregate = Read-Text "core-c/tests/all_tests.c"
foreach ($testGroup in $requiredTestGroups) {
        $testSourcePath = "core-c/tests/$testGroup.c"
        if (-not (Test-Path -LiteralPath (Join-Path $Root $testSourcePath))) {
            Add-ArchitectureError "T1 C core test matrix required test source is missing: $testSourcePath"
        }
        if ($cmake -notlike "*$testGroup*") {
            Add-ArchitectureError "T1 C core test matrix must register '$testGroup' in core-c/CMakeLists.txt"
        }
        $testMainMarker = "${testGroup}_main"
        $runMarker = "run_core_test(`"$testGroup`""
        if (-not $aggregate.Contains($testMainMarker) -or -not $aggregate.Contains($runMarker)) {
            Add-ArchitectureError "T1 C core aggregate runner must call '$testGroup'"
        }
    }
foreach ($requiredMarker in @(
            "CLEARRA_CORE_TEST_NAMES",
            "CLEARRA_CORE_TEST_SOURCES",
            "clearra_core_all_tests",
            "add_test(NAME clearra_core_all_tests",
            "CLEARRA_CORE_SPLIT_TESTS",
            "clearra_core_add_test",
            'foreach(test_index RANGE 0 ${CLEARRA_CORE_TEST_LAST_INDEX})'
        )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T1 C core CMake test matrix must keep marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "candidate_tests_EXTRA_SOURCES",
            "candidate_harddrop_tests.c",
            "candidate_locked_tests.c",
            "candidate_kick_transition_tests.c",
            "candidate_cache_dedupe_tests.c",
            "packing_tests_EXTRA_SOURCES",
            "packing_problem_tests.c",
            "packing_window_tests.c",
            "packing_buffer_hash_tests.c",
            "packing_operation_set_tests.c",
            "gpu_tests_EXTRA_SOURCES",
            "gpu_descriptor_tests.c",
            "gpu_backend_adapter_tests.c",
            "gpu_expander_tests.c",
            "gpu_kernel_tests.c",
            "gpu_reference_tests.c",
            "gpu_worker_tests.c",
            "scheduler_tests_EXTRA_SOURCES",
            "scheduler_gpu_product_tests.c",
            "scheduler_backpressure_tests.c",
            "scheduler_autotune_tests.c",
            "scheduler_memory_fallback_tests.c",
            "buildup_tests_EXTRA_SOURCES",
            "buildup_problem_tests.c",
            "buildup_impossible_fixture_tests.c",
            "buildup_enumeration_tests.c",
            "buildup_hold_enumeration_tests.c",
            "buildup_export_tests.c"
        )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T1 C core split fixture matrix must keep marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "CLEARRA_CORE_ENABLE_ASAN",
            "CLEARRA_CORE_ENABLE_UBSAN",
            "clearra_core_sanitizer_options",
            "-fsanitize=address",
            "-fsanitize=undefined",
            "/fsanitize=address"
        )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T1 C core sanitizer matrix must keep marker '$requiredMarker'"
        }
    }
$runCoreTests = Read-Text "scripts/run-c-core-tests.ps1"
foreach ($requiredMarker in @(
            "[switch]`$Split",
            "[switch]`$EnableAsan",
            "[switch]`$EnableUbsan",
            "-DCLEARRA_CORE_SPLIT_TESTS=ON",
            "-DCLEARRA_CORE_ENABLE_ASAN=ON",
            "-DCLEARRA_CORE_ENABLE_UBSAN=ON"
        )) {
        if (-not $runCoreTests.Contains($requiredMarker)) {
            Add-ArchitectureError "scripts/run-c-core-tests.ps1 must expose T1 runner marker '$requiredMarker'"
        }
    }
$wslCoreTests = Read-Text "scripts/tools/wsl-core-c-tests.sh"
foreach ($requiredMarker in @(
            "read_cmake_set CLEARRA_CORE_TEST_ORACLE_SOURCES",
            '"${TEST_ORACLE_SOURCES[@]}"',
            '"${oracle_objects[@]}"',
            '"$CORE_ROOT/tools/geometry_benchmark.c"'
        )) {
        if (-not $wslCoreTests.Contains($requiredMarker)) {
            Add-ArchitectureError "WSL C aggregate runner must preserve CMake-equivalent oracle/tool source marker '$requiredMarker'"
        }
    }
$clearraRunner = Read-Text "scripts/clearra.ps1"
foreach ($requiredMarker in @(
            "COnly",
            "COnlySplit",
            "COnlyAsan",
            "COnlyUbsan",
            "-DCLEARRA_CORE_SPLIT_TESTS=ON",
            "-DCLEARRA_CORE_ENABLE_ASAN=ON",
            "-DCLEARRA_CORE_ENABLE_UBSAN=ON"
        )) {
        if ($clearraRunner -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/clearra.ps1 must expose T1 task marker '$requiredMarker'"
        }
    }
$coverageTests = Read-Text "core-c/tests/coverage_tests.c"
$buildupTests = Get-BuildUpTestsValidationSurface
$schedulerTests = Get-SchedulerTestsValidationSurface
foreach ($requiredMarker in @(
            "CLR_COVERAGE_CAPACITY_EXCEEDED",
            "CLR_SCORE_MATRIX_CAPACITY_EXCEEDED",
            "CLR_SPIN_COVERAGE_CAPACITY_EXCEEDED",
            "c_coverage_capacity_statuses_are_distinct_contracts"
        )) {
        if ($coverageTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_tests.c must keep T1 capacity marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "build_up_count_reports_truncation",
            "enumerate_variants_sets_count_complete_false_when_truncated",
            "kick_evidence_buffer_reports_capacity_exhausted"
        )) {
        if ($buildupTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup tests must keep T1 capacity/truncation marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "autotune_never_drops_coverage_rows_silently",
            "partial_result_reports_truncation_reason"
        )) {
        if ($schedulerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scheduler tests must keep T1 capacity/backpressure marker '$requiredMarker'"
        }
    }
$testPolicy = Read-Text "docs/test-policy.md"
foreach ($requiredMarker in @(
            "T1 C core unit fixture matrix",
            "memory_tests",
            "scoring_event_tests",
            "COnlyAsan",
            "COnlyUbsan",
            "capacity_exceeded_tests_pass"
        )) {
        if ($testPolicy -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/test-policy.md must document T1 marker '$requiredMarker'"
        }
    }
}
