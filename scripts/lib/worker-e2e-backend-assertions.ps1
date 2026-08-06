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
function Assert-WorkerE2EGpuUnavailableReason([object]$Json) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-WorkerE2EJsonFieldValues $Json "backend_fallback_reason" $values
    if ($values.Count -eq 0) {
        throw "WorkerE2E expected backend_fallback_reason to exist"
    }
    $actualReasons = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-WorkerE2EScalar $_ } |
            Sort-Object -Unique
    )
    $invalidReasons = @($actualReasons | Where-Object { $_ -notin @("gpu_device_not_found", "gpu_kernel_unavailable") })
    if ($invalidReasons.Count -gt 0) {
        throw "WorkerE2E backend reported unsupported GPU unavailable reason '$($invalidReasons -join ', ')'"
    }
    if ($actualReasons.Count -ne 1) {
        throw "WorkerE2E backend reported inconsistent GPU unavailable reasons '$($actualReasons -join ', ')'"
    }
}
function Assert-WorkerE2EHybridUnavailableReason([object]$Json) {
    $gpuReason = Get-WorkerE2EJsonFieldScalar $Json "gpu_disabled_reason"
    $hybridReason = Get-WorkerE2EJsonFieldScalar $Json "hybrid_disabled_reason"
    foreach ($reason in @($gpuReason, $hybridReason)) {
        if ($reason -notin @("gpu_backend_not_connected", "gpu_device_not_found", "gpu_kernel_unavailable")) {
            throw "WorkerE2E backend reported unsupported hybrid unavailable reason '$reason'"
        }
    }
    if ($gpuReason -ne $hybridReason) {
        throw "WorkerE2E hybrid GPU disabled reasons must match"
    }
}
function Assert-WorkerE2ENoFallbackReason([object]$Json) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-WorkerE2EJsonFieldValues $Json "backend_fallback_reason" $values
    if ($values.Count -eq 0) {
        throw "WorkerE2E expected backend_fallback_reason to exist"
    }
    $unexpected = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-WorkerE2EScalar $_ } |
            Where-Object { $_ -notin @("none", "null") } |
            Sort-Object -Unique
    )
    if ($unexpected.Count -gt 0) {
        throw "WorkerE2E no-fallback result reported reason '$($unexpected -join ', ')'"
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
    $requested = Get-WorkerE2EJsonFieldScalar $Json "backend_requested"
    if ($requested -ne $Backend) {
        throw "WorkerE2E backend request mismatch expected '$Backend' actual '$requested'"
    }
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_set_checked" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_set_match" "true"
    Assert-WorkerE2EJsonFieldEquals $Json "normalized_solution_oracle" "source-fumen-count-and-tiling-set"
    Assert-WorkerE2EJsonFieldEquals $Json "missing_solution_keys" "none"
    Assert-WorkerE2EJsonFieldEquals $Json "unexpected_solution_keys" "none"

    switch ($Backend) {
        "cpu" {
            if ((Get-WorkerE2EJsonFieldScalar $Json "backend_selected") -ne "cpu") {
                throw "CPU backend must select cpu"
            }
            if ((Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_used") -ne "false") {
                throw "CPU backend must not report fallback"
            }
            Assert-WorkerE2ENoFallbackReason $Json
        }
        "gpu" {
            $selected = Get-WorkerE2EJsonFieldScalar $Json "backend_selected"
            $fallback = Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_used"
            if ($selected -eq "gpu") {
                Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_used" "false"
                if ((Get-WorkerE2EJsonFieldScalar $Json "fallback_backend") -ne "none") {
                    throw "GPU execution must not report a fallback backend"
                }
                Assert-WorkerE2ENoFallbackReason $Json
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_result_cpu_confirmed" "true"
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_cpu_reference_match" "true"
                Assert-WorkerE2EJsonFieldEquals $Json "gpu_assisted_buildup_reached" "true"
            } elseif ($selected -eq "cpu" -and $fallback -eq "true") {
                if ((Get-WorkerE2EJsonFieldScalar $Json "fallback_backend") -ne "cpu") {
                    throw "GPU fallback must select the CPU fallback backend"
                }
                Assert-WorkerE2EGpuUnavailableReason $Json
            } else {
                throw "GPU backend must either select gpu or report explicit fallback"
            }
        }
        "hybrid" {
            $selected = Get-WorkerE2EJsonFieldScalar $Json "backend_selected"
            $fallback = Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_used"
            if ($selected -eq "hybrid" -or $selected -eq "gpu") {
                Assert-WorkerE2EJsonFieldEquals $Json "backend_fallback_used" "false"
                if ((Get-WorkerE2EJsonFieldScalar $Json "fallback_backend") -ne "none") {
                    throw "Hybrid execution must not report a fallback backend"
                }
                Assert-WorkerE2ENoFallbackReason $Json
                Assert-WorkerE2EJsonFieldEquals $Json "hybrid_memory_leak_report_clean" "true"
            } elseif ($selected -eq "cpu" -and $fallback -eq "false") {
                Assert-WorkerE2ENoFallbackReason $Json
                if ((Get-WorkerE2EJsonFieldScalar $Json "fallback_backend") -ne "none") {
                    throw "Hybrid CPU selection must not report a fallback backend"
                }
                Assert-WorkerE2EJsonFieldEquals $Json "hybrid_status" "cpu-selected"
                Assert-WorkerE2EHybridUnavailableReason $Json
            } elseif ($selected -eq "cpu" -and $fallback -eq "true") {
                if ((Get-WorkerE2EJsonFieldScalar $Json "fallback_backend") -ne "cpu") {
                    throw "Hybrid execution fallback must select the CPU fallback backend"
                }
                $reason = Get-WorkerE2EJsonFieldScalar $Json "backend_fallback_reason"
                if ($reason -notin @("gpu_transient_before_commit", "gpu_resource_incomplete")) {
                    throw "Hybrid execution fallback reported unsupported reason '$reason'"
                }
            } else {
                throw "Hybrid backend must select hybrid/gpu, select CPU normally, or report an execution fallback"
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
