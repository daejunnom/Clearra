# This file is dot-sourced by scripts/product-e2e.ps1.
function ConvertTo-ProductE2ETypedScalar([object]$Value) {
    if ($null -eq $Value) {
        return "null"
    }
    if ($Value -is [bool]) {
        return $Value.ToString().ToLowerInvariant()
    }
    if ($Value -is [double] -or $Value -is [float] -or $Value -is [decimal]) {
        return ([string]::Format([System.Globalization.CultureInfo]::InvariantCulture, "{0}", $Value))
    }
    return [string]$Value
}function ConvertFrom-ProductE2EJsonOutput([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        throw "typed assertion failed: output did not contain JSON"
    }

    $trimmed = $Text.Trim()
    $start = $trimmed.IndexOf("{")
    $end = $trimmed.LastIndexOf("}")
    if ($start -lt 0 -or $end -le $start) {
        throw "typed assertion failed: output did not contain a JSON object"
    }

    $jsonText = $trimmed.Substring($start, $end - $start + 1)
    try {
        return ($jsonText | ConvertFrom-Json)
    } catch {
        throw "typed assertion failed: output JSON could not be parsed: $($_.Exception.Message)"
    }
}function Read-ProductE2EJsonFile(
    [string]$Root,
    [string]$Path
) {
    $resolved = Join-Path $Root $Path
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "typed assertion failed: JSON fixture is missing: $Path"
    }
    try {
        return (Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json)
    } catch {
        throw "typed assertion failed: JSON fixture could not be parsed: $Path`: $($_.Exception.Message)"
    }
}function Add-ProductE2EJsonFieldValues(
    [object]$Node,
    [string]$FieldName,
    [System.Collections.Generic.List[object]]$Values
) {
    if ($null -eq $Node) {
        return
    }

    if ($Node -is [System.Management.Automation.PSCustomObject]) {
        foreach ($property in $Node.PSObject.Properties) {
            if ($property.Name -eq $FieldName) {
                $Values.Add($property.Value)
            }
            Add-ProductE2EJsonFieldValues $property.Value $FieldName $Values
        }
        return
    }

    if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string])) {
        foreach ($item in $Node) {
            Add-ProductE2EJsonFieldValues $item $FieldName $Values
        }
    }
}function Assert-ProductE2EJsonFieldEquals(
    [object]$Json,
    [string]$FieldName,
    [object]$Expected
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-ProductE2EJsonFieldValues $Json $FieldName $values

    $expectedScalar = ConvertTo-ProductE2ETypedScalar $Expected
    foreach ($value in $values) {
        if ((ConvertTo-ProductE2ETypedScalar $value) -eq $expectedScalar) {
            return
        }
    }

    $actual = if ($values.Count -eq 0) {
        "<missing>"
    } else {
        (@($values.ToArray()) | ForEach-Object { ConvertTo-ProductE2ETypedScalar $_ }) -join ", "
    }
    throw "typed assertion failed: expected field '$FieldName' to contain '$expectedScalar' (actual: $actual)"
}function Get-ProductE2EJsonFieldScalar(
    [object]$Json,
    [string]$FieldName
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-ProductE2EJsonFieldValues $Json $FieldName $values
    if ($values.Count -eq 0) {
        throw "typed assertion failed: expected field '$FieldName' to exist"
    }
    $actualValues = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-ProductE2ETypedScalar $_ } |
            Sort-Object -Unique
    )
    if ($actualValues.Count -ne 1) {
        throw "typed assertion failed: field '$FieldName' reported inconsistent values '$($actualValues -join ', ')'"
    }
    return $actualValues[0]
}function Test-ProductE2EGpuUnavailableReason([string]$Reason) {
    return $Reason -in @(
        "gpu_device_not_found",
        "gpu_kernel_unavailable"
    )
}function Test-ProductE2EHybridUnavailableReason([string]$Reason) {
    return $Reason -in @(
        "gpu_backend_not_connected",
        "gpu_device_not_found",
        "gpu_kernel_unavailable"
    )
}function Assert-ProductE2EJsonFieldGpuUnavailableReason(
    [object]$Json,
    [string]$FieldName
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-ProductE2EJsonFieldValues $Json $FieldName $values
    if ($values.Count -eq 0) {
        throw "typed assertion failed: expected field '$FieldName' to exist"
    }

    $actualReasons = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-ProductE2ETypedScalar $_ } |
            Sort-Object -Unique
    )
    $invalidReasons = @($actualReasons | Where-Object { -not (Test-ProductE2EGpuUnavailableReason $_) })
    if ($invalidReasons.Count -gt 0) {
        throw "typed assertion failed: field '$FieldName' reported unsupported GPU unavailable reason '$($invalidReasons -join ', ')'"
    }
    if ($actualReasons.Count -ne 1) {
        throw "typed assertion failed: field '$FieldName' reported inconsistent GPU unavailable reasons '$($actualReasons -join ', ')'"
    }
    return $actualReasons[0]
}function Assert-ProductE2EJsonFieldHybridUnavailableReason(
    [object]$Json,
    [string]$FieldName
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-ProductE2EJsonFieldValues $Json $FieldName $values
    if ($values.Count -eq 0) {
        throw "typed assertion failed: expected field '$FieldName' to exist"
    }

    $actualReasons = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-ProductE2ETypedScalar $_ } |
            Sort-Object -Unique
    )
    $invalidReasons = @($actualReasons | Where-Object { -not (Test-ProductE2EHybridUnavailableReason $_) })
    if ($invalidReasons.Count -gt 0) {
        throw "typed assertion failed: field '$FieldName' reported unsupported hybrid unavailable reason '$($invalidReasons -join ', ')'"
    }
    if ($actualReasons.Count -ne 1) {
        throw "typed assertion failed: field '$FieldName' reported inconsistent hybrid unavailable reasons '$($actualReasons -join ', ')'"
    }
    return $actualReasons[0]
}function Assert-ProductE2EJsonFieldNoFallbackReason([object]$Json) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-ProductE2EJsonFieldValues $Json "backend_fallback_reason" $values
    if ($values.Count -eq 0) {
        throw "typed assertion failed: expected field 'backend_fallback_reason' to exist"
    }
    $unexpected = @(
        $values.ToArray() |
            ForEach-Object { ConvertTo-ProductE2ETypedScalar $_ } |
            Where-Object { $_ -notin @("none", "null") } |
            Sort-Object -Unique
    )
    if ($unexpected.Count -gt 0) {
        throw "typed assertion failed: no-fallback result reported reason '$($unexpected -join ', ')'"
    }
}function Assert-ProductE2EOutputGpuUnavailableReason([string]$Output) {
    $actualReasons = @(
        @("gpu_device_not_found", "gpu_kernel_unavailable") |
            Where-Object { $Output -like "*$_*" }
    )
    if ($actualReasons.Count -eq 0) {
        throw "typed assertion failed: output must report gpu_device_not_found or gpu_kernel_unavailable"
    }
    if ($actualReasons.Count -ne 1) {
        throw "typed assertion failed: output reported inconsistent GPU unavailable reasons '$($actualReasons -join ', ')'"
    }
    return $actualReasons[0]
}function Assert-ProductE2EJsonFieldSame(
    [object]$ExpectedJson,
    [object]$ActualJson,
    [string]$FieldName
) {
    $expected = Get-ProductE2EJsonFieldScalar $ExpectedJson $FieldName
    $actual = Get-ProductE2EJsonFieldScalar $ActualJson $FieldName
    if ($expected -ne $actual) {
        throw "typed assertion failed: field '$FieldName' mismatch expected '$expected' actual '$actual'"
    }
}function Assert-ProductE2EJsonFieldUniqueEquals(
    [object]$Json,
    [string]$FieldName,
    [object]$Expected
) {
    $actual = Get-ProductE2EJsonFieldScalar $Json $FieldName
    $expectedScalar = ConvertTo-ProductE2ETypedScalar $Expected
    if ($actual -ne $expectedScalar) {
        throw "typed assertion failed: field '$FieldName' expected '$expectedScalar' actual '$actual'"
    }
}function Get-ProductE2EBackendReport([object]$Json) {
    if ($null -eq $Json) {
        throw "typed assertion failed: backend_report object must be present"
    }
    if (($Json.PSObject.Properties.Name -contains "contract") -and
        $null -ne $Json.contract -and
        ($Json.contract.PSObject.Properties.Name -contains "pc") -and
        $null -ne $Json.contract.pc -and
        ($Json.contract.pc.PSObject.Properties.Name -contains "backend_report") -and
        $Json.contract.pc.backend_report -is [System.Management.Automation.PSCustomObject]) {
        return $Json.contract.pc.backend_report
    }
    if (($Json.PSObject.Properties.Name -contains "backend_report") -and
        $Json.backend_report -is [System.Management.Automation.PSCustomObject]) {
        return $Json.backend_report
    }
    throw "typed assertion failed: backend_report object must be present"
}function Assert-ProductE2EBackendReportFieldsPresent([object]$Json) {
    $report = Get-ProductE2EBackendReport $Json
    foreach ($field in @(
            "backend_requested",
            "backend_selected",
            "candidate_backend",
            "buildup_backend",
            "gpu_available",
            "gpu_disabled_reason",
            "gpu_trust_state",
            "cpu_confirm_required",
            "cpu_reference_matched",
            "fallback_used",
            "fallback_backend",
            "backend_fallback_reason",
            "hybrid_status",
            "hybrid_disabled_reason",
            "memory_pressure_level",
            "backpressure"
        )) {
        if (-not ($report.PSObject.Properties.Name -contains $field)) {
            throw "typed assertion failed: backend_report.$field must be present"
        }
    }
}function Assert-ProductE2EBackendReportFieldEquals(
    [object]$Json,
    [string]$FieldName,
    [object]$Expected
) {
    $report = Get-ProductE2EBackendReport $Json
    if (-not ($report.PSObject.Properties.Name -contains $FieldName)) {
        throw "typed assertion failed: backend_report.$FieldName must be present"
    }
    $actual = ConvertTo-ProductE2ETypedScalar $report.$FieldName
    $expectedScalar = ConvertTo-ProductE2ETypedScalar $Expected
    if ($actual -ne $expectedScalar) {
        throw "typed assertion failed: backend_report.$FieldName expected '$expectedScalar' actual '$actual'"
    }
}function Assert-ProductE2EBackendReportGpuUnavailableReason(
    [object]$Json,
    [string]$FieldName
) {
    $report = Get-ProductE2EBackendReport $Json
    if (-not ($report.PSObject.Properties.Name -contains $FieldName)) {
        throw "typed assertion failed: backend_report.$FieldName must be present"
    }
    $actual = ConvertTo-ProductE2ETypedScalar $report.$FieldName
    if (-not (Test-ProductE2EGpuUnavailableReason $actual)) {
        throw "typed assertion failed: backend_report.$FieldName reported unsupported GPU unavailable reason '$actual'"
    }
    return $actual
}function Assert-ProductE2EBackendReportHybridUnavailableReason(
    [object]$Json,
    [string]$FieldName
) {
    $report = Get-ProductE2EBackendReport $Json
    if (-not ($report.PSObject.Properties.Name -contains $FieldName)) {
        throw "typed assertion failed: backend_report.$FieldName must be present"
    }
    $actual = ConvertTo-ProductE2ETypedScalar $report.$FieldName
    if (-not (Test-ProductE2EHybridUnavailableReason $actual)) {
        throw "typed assertion failed: backend_report.$FieldName reported unsupported hybrid unavailable reason '$actual'"
    }
    return $actual
}function Assert-ProductE2ECpuSelectionReport([object]$Json) {
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_requested" "cpu"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_fallback_used" "false"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_requested" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_used" "false"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_backend" "none"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_fallback_reason" $null
    Assert-ProductE2EJsonFieldNoFallbackReason $Json
}function Assert-ProductE2EGpuCpuFallbackReport([object]$Json) {
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_requested" "gpu"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_fallback_used" "true"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_requested" "gpu"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $Json "gpu_available" "false"
    $disabledReason = Assert-ProductE2EBackendReportGpuUnavailableReason $Json "gpu_disabled_reason"
    $recursiveDisabledReason = Assert-ProductE2EJsonFieldGpuUnavailableReason $Json "gpu_disabled_reason"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_used" "true"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_backend" "cpu"
    $fallbackReason = Assert-ProductE2EBackendReportGpuUnavailableReason $Json "backend_fallback_reason"
    $recursiveFallbackReason = Assert-ProductE2EJsonFieldGpuUnavailableReason $Json "backend_fallback_reason"
    if ($fallbackReason -ne $disabledReason) {
        throw "typed assertion failed: GPU disabled and fallback reasons must match"
    }
    if ($disabledReason -ne $recursiveDisabledReason) {
        throw "typed assertion failed: GPU disabled reason must match every product layer"
    }
    if ($fallbackReason -ne $recursiveFallbackReason) {
        throw "typed assertion failed: GPU fallback reason must match every product layer"
    }
}function Assert-ProductE2EHybridCpuSelectionReport([object]$Json) {
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_requested" "hybrid"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EJsonFieldUniqueEquals $Json "backend_fallback_used" "false"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_requested" "hybrid"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_used" "false"
    Assert-ProductE2EBackendReportFieldEquals $Json "fallback_backend" "none"
    Assert-ProductE2EBackendReportFieldEquals $Json "backend_fallback_reason" $null
    Assert-ProductE2EJsonFieldNoFallbackReason $Json
    Assert-ProductE2EBackendReportFieldEquals $Json "hybrid_status" "cpu-selected"
    Assert-ProductE2EBackendReportFieldEquals $Json "gpu_available" "false"
    $gpuDisabledReason = Assert-ProductE2EBackendReportHybridUnavailableReason $Json "gpu_disabled_reason"
    $hybridDisabledReason = Assert-ProductE2EBackendReportHybridUnavailableReason $Json "hybrid_disabled_reason"
    $recursiveGpuDisabledReason = Assert-ProductE2EJsonFieldHybridUnavailableReason $Json "gpu_disabled_reason"
    $recursiveHybridDisabledReason = Assert-ProductE2EJsonFieldHybridUnavailableReason $Json "hybrid_disabled_reason"
    if ($gpuDisabledReason -ne $hybridDisabledReason) {
        throw "typed assertion failed: hybrid GPU disabled reasons must match"
    }
    if ($gpuDisabledReason -ne $recursiveGpuDisabledReason -or
        $hybridDisabledReason -ne $recursiveHybridDisabledReason) {
        throw "typed assertion failed: hybrid GPU disabled reasons must match every product layer"
    }
}function Assert-ProductE2EU0BackendCapabilityReport(
    [object]$CpuJson,
    [object]$GpuJson,
    [object]$HybridJson
) {
    foreach ($json in @($CpuJson, $GpuJson, $HybridJson)) {
        Assert-ProductE2EBackendReportFieldsPresent $json
    }

    Assert-ProductE2ECpuSelectionReport $CpuJson
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "gpu_available" "false"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "gpu_disabled_reason" "not_requested"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "hybrid_status" "not-requested"

    Assert-ProductE2EGpuCpuFallbackReport $GpuJson

    Assert-ProductE2EHybridCpuSelectionReport $HybridJson
}function Assert-ProductE2EFixtureForbidsAlgorithm(
    [object]$Fixture,
    [string]$AlgorithmName
) {
    if ((ConvertTo-ProductE2ETypedScalar $Fixture.expected.forbidden_algorithm) -ne $AlgorithmName) {
        throw "typed assertion failed: expected forbidden_algorithm '$AlgorithmName'"
    }
}function Assert-ProductE2ETypedCommandAssertions(
    [string]$CaseName,
    [string]$Output
) {
    switch -Wildcard ($CaseName) {
        "opening 2L product pipeline" {
            $json = ConvertFrom-ProductE2EJsonOutput $Output
            Assert-ProductE2EJsonFieldEquals $json "kind" "pc"
            Assert-ProductE2EJsonFieldEquals $json "problem_preset" "opening-pc"
            Assert-ProductE2EJsonFieldEquals $json "compiled_goal" "clear-to-empty"
            Assert-ProductE2EJsonFieldEquals $json "compiled_piece_window" "5"
            Assert-ProductE2EJsonFieldEquals $json "packing_candidate_is_solution" "false"
        }
        default {
            return
        }
    }
}function Assert-ProductE2ETypedFixtureAssertions(
    [string]$CaseName,
    [string]$Root,
    [string]$FixturePath
) {
    switch -Wildcard ($CaseName) {
        "coverage overlap uses PatternBitSet union" {
            $fixture = Read-ProductE2EJsonFile $Root $FixturePath
            Assert-ProductE2EJsonFieldEquals $fixture "row_kind" "Build"
            Assert-ProductE2EJsonFieldEquals $fixture "pattern_universe_id" "1001"
            Assert-ProductE2EJsonFieldEquals $fixture "pattern_weight_model_id" "2001"
            Assert-ProductE2EJsonFieldEquals $fixture "covered_pattern_count" "1"
            Assert-ProductE2EJsonFieldEquals $fixture "probability" "0.4"
            Assert-ProductE2EFixtureForbidsAlgorithm $fixture "variant_probability_sum"
        }
        "setup family probability uses PatternBitSet union" {
            $fixture = Read-ProductE2EJsonFile $Root $FixturePath
            Assert-ProductE2EJsonFieldEquals $fixture "shape_family_id" "1"
            Assert-ProductE2EJsonFieldEquals $fixture "covered_pattern_count" "3"
            Assert-ProductE2EJsonFieldEquals $fixture "probability" "0.75"
            Assert-ProductE2EFixtureForbidsAlgorithm $fixture "variant_probability_sum"
        }
        default {
            return
        }
    }
}
