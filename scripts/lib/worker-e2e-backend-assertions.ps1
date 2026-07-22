# This file is dot-sourced by scripts/worker-e2e.ps1.
function Assert-WorkerE2EExecutableFixtureOracle(
    [Parameter(Mandatory)]
    [object]$Fixture,

    [Parameter(Mandatory)]
    [string]$FixturePath
) {
    if ($Fixture.expected.PSObject.Properties.Name -contains "worker_correctness_gate_enabled" -or
        $Fixture.expected.PSObject.Properties.Name -contains "worker_correctness_blocked_reason") {
        throw "WorkerE2E executable fixture '$FixturePath' must not contain a disabled correctness gate"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.source.human_verified) -ne "true") {
        throw "WorkerE2E executable fixture '$FixturePath' must be human verified"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.input.source_fumen_from_registry) -ne "true") {
        throw "WorkerE2E executable fixture '$FixturePath' must derive its oracle from the source fumen registry"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.expected.correctness_oracle) -ne "source-fumen-count-and-tiling-set") {
        throw "WorkerE2E executable fixture '$FixturePath' must declare its limited source-fumen tiling oracle"
    }
}
function Assert-WorkerE2EMetadataOnlyFixture(
    [Parameter(Mandatory)]
    [object]$Fixture,

    [Parameter(Mandatory)]
    [string]$FixturePath
) {
    if ((ConvertTo-WorkerE2EScalar $Fixture.expected.oracle_kind) -ne "metadata-only-source-labels") {
        throw "WorkerE2E metadata fixture '$FixturePath' must declare oracle_kind=metadata-only-source-labels"
    }
    if ($Fixture.input.PSObject.Properties.Name -contains "materialized_expected") {
        throw "WorkerE2E metadata fixture '$FixturePath' must not expose executable materialized_expected data"
    }
    if ($Fixture.expected.PSObject.Properties.Name -contains "worker_correctness_gate_enabled" -or
        $Fixture.expected.PSObject.Properties.Name -contains "worker_correctness_blocked_reason") {
        throw "WorkerE2E metadata fixture '$FixturePath' must not use a disabled correctness gate"
    }
}
function Assert-WorkerE2EBackendOutput(
    [string]$Backend,
    [object]$Json
) {
    Assert-WorkerE2EJsonFieldEquals $Json "kind" "pc-scenario"
    Assert-WorkerE2EJsonFieldEquals $Json "expected_match" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "solution_found" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "packing_candidate_is_solution" "false"
    Assert-WorkerE2EJsonFieldEquals $Json "memory_leak_report_clean" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "backend_requested" $Backend
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_set_checked" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_set_match" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_oracle" "source-fumen-count-and-tiling-set"
    Assert-WorkerE2EJsonFieldEquals $Json "missing_solution_keys" "none"
    Assert-WorkerE2EJsonFieldEquals $Json "unexpected_solution_keys" "none"

    switch ($Backend) {
        "cpu" {
            Assert-WorkerE2EJsonFieldEquals $Json "backend_selected" "cpu"
            Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_used" "false"
        }
        "gpu" {
            $selected = Get-WorkerE2EJsonFieldScalar $Json "backend_selected"
            $fallback = Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_used"
            if ($selected -eq "gpu") {
                Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_used" "false"
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_result_cpu_confirmed" "true"
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_cpu_reference_match" "true"
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_assisted_buildup_reached" "true"
            } elseif ($fallback -eq "true") {
                Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_reason" "gpu_kernel_unavailable"
            } else {
                throw "GPU backend must either select gpu or report explicit fallback"
            }
        }
        "hybrid" {
            $selected = Get-WorkerE2EJsonFieldScalar $Json "backend_selected"
            $fallback = Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_used"
            if ($selected -eq "hybrid" -or $selected -eq "gpu") {
                Assert-WorkerE2EJsonFieldEquals $Json "hybrid_memory_leak_report_clean" "true"
            } elseif ($fallback -eq "true") {
                Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_reason" "gpu_kernel_unavailable"
            } else {
                throw "Hybrid backend must either select hybrid/gpu or report explicit fallback"
            }
        }
    }
}function Assert-WorkerE2EBackendEquivalence(
    [object]$CpuJson,
    [object]$GpuJson,
    [object]$HybridJson
) {
    foreach ($field in @(
            "coverage_probability",
            "covered_pattern_count",
            "total_solution_count",
            "unique_solution_count",
            "actual_solution_set_contract",
            "normalized_solution_key_algorithm",
            "normalized_unique_solution_count",
            "actual_normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
            "count_complete",
            "solution_found"
        )) {
        Assert-WorkerE2EJsonFieldSame $CpuJson $GpuJson $field
        Assert-WorkerE2EJsonFieldSame $CpuJson $HybridJson $field
    }
}function Assert-WorkerE2EBackendSolutionSetMatchesSource(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [object]$Fixture,

    [Parameter(Mandatory)]
    [hashtable]$Results
) {
    $expectedCount = ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.expected_unique_solution_count
    foreach ($backend in @($Results.Keys)) {
        $json = $Results[$backend]
        $expectedHash = Get-WorkerE2EJsonFieldScalar $json "expected_normalized_solution_set_hash"
        Assert-WorkerE2EJsonFieldEquals $json "actual_solution_set_contract" "normalized-tiling-set"
        Assert-WorkerE2EJsonFieldEquals $json "normalized_solution_key_algorithm" "clearra-normalized-tiling-key-v1"
        Assert-WorkerE2EJsonFieldEquals $json "normalized_solution_set_hash_algorithm" "clearra-normalized-tiling-set-fnv64-v1"
        Assert-WorkerE2EJsonFieldEquals $json "normalized_unique_solution_count" $expectedCount
        Assert-WorkerE2EJsonFieldEquals $json "actual_normalized_unique_solution_count" $expectedCount
        Assert-WorkerE2EJsonFieldEquals $json "normalized_solution_set_hash" $expectedHash
        Assert-WorkerE2EJsonFieldEquals $json "actual_normalized_solution_set_hash" $expectedHash
    }
}
