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
    return ConvertTo-ProductE2ETypedScalar $values[0]
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
}function Assert-ProductE2EU0BackendCapabilityReport(
    [object]$CpuJson,
    [object]$GpuJson,
    [object]$HybridJson
) {
    foreach ($json in @($CpuJson, $GpuJson, $HybridJson)) {
        Assert-ProductE2EBackendReportFieldsPresent $json
    }

    Assert-ProductE2EBackendReportFieldEquals $CpuJson "backend_requested" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "gpu_available" "false"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "gpu_disabled_reason" "not_requested"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "fallback_used" "false"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "fallback_backend" "none"
    Assert-ProductE2EBackendReportFieldEquals $CpuJson "hybrid_status" "not-requested"

    Assert-ProductE2EBackendReportFieldEquals $GpuJson "backend_requested" "gpu"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "gpu_available" "false"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "gpu_disabled_reason" "gpu_kernel_unavailable"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "fallback_used" "true"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "fallback_backend" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $GpuJson "backend_fallback_reason" "gpu_kernel_unavailable"

    Assert-ProductE2EBackendReportFieldEquals $HybridJson "backend_requested" "hybrid"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "backend_selected" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "fallback_used" "true"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "fallback_backend" "cpu"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "hybrid_status" "disabled"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "hybrid_disabled_reason" "gpu_kernel_unavailable"
    Assert-ProductE2EBackendReportFieldEquals $HybridJson "backend_fallback_reason" "gpu_kernel_unavailable"
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
