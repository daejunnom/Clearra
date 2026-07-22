# This file is dot-sourced by scripts/worker-e2e.ps1.
function ConvertFrom-WorkerE2EJsonOutput([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        throw "WorkerE2E output did not contain JSON"
    }
    $trimmed = $Text.Trim()
    $start = $trimmed.IndexOf("{")
    $end = $trimmed.LastIndexOf("}")
    if ($start -lt 0 -or $end -le $start) {
        throw "WorkerE2E output did not contain a JSON object"
    }
    return ($trimmed.Substring($start, $end - $start + 1) | ConvertFrom-Json)
}function Add-WorkerE2EJsonFieldValues(
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
            Add-WorkerE2EJsonFieldValues $property.Value $FieldName $Values
        }
        return
    }
    if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string])) {
        foreach ($item in $Node) {
            Add-WorkerE2EJsonFieldValues $item $FieldName $Values
        }
    }
}function Get-WorkerE2EJsonFieldScalar(
    [object]$Json,
    [string]$FieldName
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-WorkerE2EJsonFieldValues $Json $FieldName $values
    if ($values.Count -eq 0) {
        throw "expected field '$FieldName' to exist"
    }
    return ConvertTo-WorkerE2EScalar $values[0]
}function Assert-WorkerE2EJsonFieldEquals(
    [object]$Json,
    [string]$FieldName,
    [object]$Expected
) {
    $values = New-Object System.Collections.Generic.List[object]
    Add-WorkerE2EJsonFieldValues $Json $FieldName $values
    $expectedScalar = ConvertTo-WorkerE2EScalar $Expected
    foreach ($value in $values) {
        if ((ConvertTo-WorkerE2EScalar $value) -eq $expectedScalar) {
            return
        }
    }
    $actual = if ($values.Count -eq 0) {
        "<missing>"
    } else {
        (@($values.ToArray()) | ForEach-Object { ConvertTo-WorkerE2EScalar $_ }) -join ", "
    }
    throw "expected field '$FieldName' to contain '$expectedScalar' (actual: $actual)"
}function Assert-WorkerE2EJsonFieldSame(
    [object]$ExpectedJson,
    [object]$ActualJson,
    [string]$FieldName
) {
    $expected = Get-WorkerE2EJsonFieldScalar $ExpectedJson $FieldName
    $actual = Get-WorkerE2EJsonFieldScalar $ActualJson $FieldName
    if ($expected -ne $actual) {
        throw "field '$FieldName' mismatch expected '$expected' actual '$actual'"
    }
}