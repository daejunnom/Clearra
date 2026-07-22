# This file is dot-sourced by scripts/worker-e2e.ps1.
function Get-WorkerE2EOutputExcerpt([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ""
    }
    return (($Text -split "`r?`n") | Select-Object -Last $script:WorkerE2EOutputExcerptLines) -join "`n"
}function New-WorkerE2ECommandArgs(
    [string]$FixturePath,
    [string]$Backend
) {
    $args = @(
        "--format", "json",
        "pc-scenario",
        "--fixture", $FixturePath,
        "--verify-expected",
        "--backend", $Backend,
        "--workers", ([string]$script:WorkerE2EWorkers)
    )
    if ($Backend -eq "gpu" -or $Backend -eq "hybrid") {
        $args += "--allow-backend-fallback"
    }
    return $args
}function New-WorkerE2EFailureMessage(
    [string]$CaseName,
    [string]$Reason,
    [string]$Command,
    [string]$Output
) {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("[worker-e2e] failed | $CaseName")
    if (-not [string]::IsNullOrWhiteSpace($Reason)) {
        $lines.Add($Reason)
    }
    if (-not [string]::IsNullOrWhiteSpace($Command)) {
        $lines.Add("command: $Command")
    }
    $excerpt = Get-WorkerE2EOutputExcerpt $Output
    if (-not [string]::IsNullOrWhiteSpace($excerpt)) {
        $lines.Add("---- last $script:WorkerE2EOutputExcerptLines output line(s) ----")
        $lines.Add($excerpt)
        $lines.Add("---- end output excerpt ----")
    }
    return ($lines -join "`n")
}function Invoke-WorkerE2EBackendRunCase(
    [string]$Name,
    [string]$FixturePath,
    [string]$GoldenPath,
    [string[]]$Backends
) {
    Invoke-ClearraProgressCase -Scope $script:WorkerE2EProgressScope -Name $Name -Body {
        $script:WorkerE2ECurrentCaseName = $Name
        $lastCommand = ""
        $lastOutput = ""
        try {
            $fixture = Read-WorkerE2EJsonFile -Root $script:WorkerE2ERoot -Path $FixturePath
            $golden = Read-WorkerE2EJsonFile -Root $script:WorkerE2ERoot -Path $GoldenPath
            $markerText = ConvertTo-WorkerE2EFixtureMarkerText -Root $script:WorkerE2ERoot -Fixture $fixture
            $requiredMarkers = Read-WorkerE2ERequiredMarkers -Root $script:WorkerE2ERoot -GoldenPath $GoldenPath
            Assert-WorkerE2EMarkers -CaseName $Name -MarkerText $markerText -RequiredMarkers $requiredMarkers
            Assert-WorkerE2ETypedGoldenAssertions -Fixture $fixture -Golden $golden
            Assert-WorkerE2EMinimalSolveSetIsMetadataOnly -Fixture $fixture -FixturePath $FixturePath
            Assert-WorkerE2ETsarUniqueSolveSetContract -Fixture $fixture
            Assert-WorkerE2EExecutableFixtureOracle -Fixture $fixture -FixturePath $FixturePath
            Assert-WorkerE2EFixtureFumenSolutionSet -Root $script:WorkerE2ERoot -Fixture $fixture

            $results = @{}
            foreach ($backend in $Backends) {
                $commandArgs = New-WorkerE2ECommandArgs -FixturePath $FixturePath -Backend $backend
                $result = Invoke-WorkerE2EClearra $commandArgs
                $lastCommand = $result.Command
                $lastOutput = $result.Output
                if ($result.ExitCode -ne 0) {
                    throw "$backend backend command failed with exit $($result.ExitCode)"
                }
                $json = ConvertFrom-WorkerE2EJsonOutput $result.Output
                Assert-WorkerE2EBackendOutput -Backend $backend -Json $json
                $results[$backend] = $json
            }
            if ($Backends.Count -eq 3) {
                Assert-WorkerE2EBackendEquivalence $results["cpu"] $results["gpu"] $results["hybrid"]
            }
            Assert-WorkerE2EBackendSolutionSetMatchesSource `
                -Root $script:WorkerE2ERoot `
                -Fixture $fixture `
                -Results $results
        } catch {
            throw (New-WorkerE2EFailureMessage -CaseName $Name -Reason $_.Exception.Message -Command $lastCommand -Output $lastOutput)
        }
    }
}
function Invoke-WorkerE2EMetadataFixtureCase(
    [string]$Name,
    [string]$FixturePath,
    [string]$GoldenPath
) {
    Invoke-ClearraProgressCase -Scope $script:WorkerE2EProgressScope -Name $Name -Body {
        $fixture = Read-WorkerE2EJsonFile -Root $script:WorkerE2ERoot -Path $FixturePath
        $golden = Read-WorkerE2EJsonFile -Root $script:WorkerE2ERoot -Path $GoldenPath
        $markerText = ConvertTo-WorkerE2EFixtureMarkerText -Root $script:WorkerE2ERoot -Fixture $fixture
        $requiredMarkers = Read-WorkerE2ERequiredMarkers -Root $script:WorkerE2ERoot -GoldenPath $GoldenPath
        Assert-WorkerE2EMarkers -CaseName $Name -MarkerText $markerText -RequiredMarkers $requiredMarkers
        Assert-WorkerE2ETypedGoldenAssertions -Fixture $fixture -Golden $golden
        Assert-WorkerE2EMetadataOnlyFixture -Fixture $fixture -FixturePath $FixturePath
        Assert-WorkerE2EMinimalSolveSetIsMetadataOnly -Fixture $fixture -FixturePath $FixturePath
        Assert-WorkerE2EFixtureFumenSolutionSet -Root $script:WorkerE2ERoot -Fixture $fixture
        [Console]::Out.WriteLine("[worker-e2e] metadata-only source | case=$Name | solver_oracle=false")
    }
}
