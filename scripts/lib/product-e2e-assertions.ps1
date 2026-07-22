# Product E2E marker, golden, and failure assertion stage.

function Get-ProductE2EExcerpt([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ""
    }
    return (($Text -split "`r?`n") | Select-Object -Last $OutputExcerptLines) -join "`n"
}

function ConvertTo-ProductE2EScalar([object]$Value) {
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
}

function Add-ProductE2EJsonMarkers(
    [object]$Node,
    [string]$Path,
    [System.Collections.Generic.List[string]]$Markers
) {
    if ($null -eq $Node) {
        if (-not [string]::IsNullOrWhiteSpace($Path)) {
            $Markers.Add("$Path=null")
        }
        return
    }

    if ($Node -is [System.Management.Automation.PSCustomObject]) {
        foreach ($property in $Node.PSObject.Properties) {
            $nextPath = if ([string]::IsNullOrWhiteSpace($Path)) {
                $property.Name
            } else {
                "$Path.$($property.Name)"
            }
            Add-ProductE2EJsonMarkers $property.Value $nextPath $Markers
        }
        return
    }

    if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string])) {
        $index = 0
        foreach ($item in $Node) {
            Add-ProductE2EJsonMarkers $item "$Path[$index]" $Markers
            $index += 1
        }
        return
    }

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    $value = ConvertTo-ProductE2EScalar $Node
    $Markers.Add("$Path=$value")
    $leaf = (($Path -split "\.")[-1] -replace "\[\d+\]$", "")
    if (-not [string]::IsNullOrWhiteSpace($leaf) -and $leaf -ne $Path) {
        $Markers.Add("$leaf=$value")
    }
}

function ConvertTo-ProductE2EMarkerText([string]$Text) {
    $markers = New-Object System.Collections.Generic.List[string]
    if (-not [string]::IsNullOrWhiteSpace($Text)) {
        foreach ($line in @($Text -split "`r?`n")) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                $markers.Add($line.Trim())
            }
        }
    }

    $trimmed = $Text.Trim()
    $start = $trimmed.IndexOf("{")
    $end = $trimmed.LastIndexOf("}")
    if ($start -ge 0 -and $end -gt $start) {
        $jsonText = $trimmed.Substring($start, $end - $start + 1)
        try {
            $json = $jsonText | ConvertFrom-Json
            Add-ProductE2EJsonMarkers $json "" $markers
        } catch {
            if ($VerboseLog.IsPresent) {
                Write-Output "[product-e2e] JSON parse skipped: $($_.Exception.Message)"
            }
        }
    }

    return ($markers -join "`n")
}

function Read-ProductE2ERequiredMarkers([string]$GoldenPath) {
    $resolved = Join-Path $Root $GoldenPath
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "Product E2E golden file is missing: $GoldenPath"
    }
    $json = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
    if (-not ($json.PSObject.Properties.Name -contains "required_markers")) {
        throw "Product E2E golden file must expose required_markers: $GoldenPath"
    }
    return @($json.required_markers | ForEach-Object { [string]$_ })
}

function Assert-ProductE2EMarkers(
    [string]$CaseName,
    [string]$MarkerText,
    [string[]]$RequiredMarkers
) {
    foreach ($marker in $RequiredMarkers) {
        if ($MarkerText -notlike "*$marker*") {
            throw "missing marker: $marker"
        }
    }
}

function New-ProductE2EFailureMessage(
    [string]$CaseName,
    [string]$Reason,
    [string]$Command,
    [string]$FixturePath,
    [string]$GoldenPath,
    [string]$Output
) {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("[product-e2e] failed | $CaseName")
    if (-not [string]::IsNullOrWhiteSpace($Reason)) {
        $lines.Add($Reason)
    }
    if (-not [string]::IsNullOrWhiteSpace($Command)) {
        $lines.Add("command: $Command")
    }
    if (-not [string]::IsNullOrWhiteSpace($FixturePath)) {
        $lines.Add("fixture: $FixturePath")
    }
    if (-not [string]::IsNullOrWhiteSpace($GoldenPath)) {
        $lines.Add("golden: $GoldenPath")
    }
    $excerpt = Get-ProductE2EExcerpt $Output
    if (-not [string]::IsNullOrWhiteSpace($excerpt)) {
        $lines.Add("---- last $OutputExcerptLines output line(s) ----")
        $lines.Add($excerpt)
        $lines.Add("---- end output excerpt ----")
    }
    return ($lines -join "`n")
}

function Get-ProductE2EFixtureMaterial([string]$FixturePath) {
    $resolved = Join-Path $Root $FixturePath
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "Product E2E fixture is missing: $FixturePath"
    }
    $fixtureText = Get-Content -LiteralPath $resolved -Raw
    $markers = ConvertTo-ProductE2EMarkerText $fixtureText
    $fixture = $fixtureText | ConvertFrom-Json

    if ($FixturePath -like "*overlap_two_variants_one_pattern.json") {
        if ($null -ne $fixture.expected.probability) {
            $markers += "`ncoverage_probability=$(ConvertTo-ProductE2EScalar $fixture.expected.probability)"
        }
        if ($fixture.expected.forbidden_algorithm) {
            $markers += "`n$($fixture.expected.forbidden_algorithm)=forbidden"
        }
    }
    if ($FixturePath -like "*simple_family_union.json") {
        $markers += "`nPatternBitSet OR union"
        if ($null -ne $fixture.expected.probability) {
            $markers += "`ncoverage_probability=$(ConvertTo-ProductE2EScalar $fixture.expected.probability)"
        }
        if ($fixture.expected.forbidden_algorithm) {
            $markers += "`n$($fixture.expected.forbidden_algorithm)=forbidden"
        }
    }

    return $markers
}
