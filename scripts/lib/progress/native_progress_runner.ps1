function ConvertTo-ClearraProcessArgument {
    param([AllowNull()][string]$Argument)

    $value = if ($null -eq $Argument) { "" } else { [string]$Argument }
    if ($value.Length -gt 0 -and $value -notmatch '[\s"]') {
        return $value
    }

    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $backslashCount = 0

    foreach ($char in $value.ToCharArray()) {
        if ($char -eq '\') {
            $backslashCount += 1
            continue
        }

        if ($char -eq '"') {
            [void]$builder.Append(('\' * (($backslashCount * 2) + 1)))
            [void]$builder.Append('"')
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void]$builder.Append(('\' * $backslashCount))
            $backslashCount = 0
        }
        [void]$builder.Append($char)
    }

    if ($backslashCount -gt 0) {
        [void]$builder.Append(('\' * ($backslashCount * 2)))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}function ConvertTo-ClearraProcessArgumentString {
    param([string[]]$Arguments)

    return (@($Arguments) | ForEach-Object {
        ConvertTo-ClearraProcessArgument $_
    }) -join " "
}function Resolve-ClearraNativeFileName {
    param([Parameter(Mandatory)][string]$FileName)

    if ([System.IO.Path]::IsPathRooted($FileName) -or
        -not [string]::IsNullOrWhiteSpace([System.IO.Path]::GetDirectoryName($FileName))) {
        return $FileName
    }

    $command = Get-Command -Name $FileName -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $command -and -not [string]::IsNullOrWhiteSpace($command.Source)) {
        return $command.Source
    }

    return $FileName
}function Invoke-NativeWithProgress {
    param(
        [Parameter(Mandatory)]
        $Scope,

        [Parameter(Mandatory)]
        [string]$Label,

        [Parameter(Mandatory)]
        [string]$FileName,

        [string[]]$Arguments = @(),

        [int]$HeartbeatMs = 500
    )

    $commandLeaf = [System.IO.Path]::GetFileNameWithoutExtension($FileName)
    $cargoCommand = $commandLeaf -eq "cargo"
    $cargoLaunch = $cargoCommand -and
        $Arguments.Count -gt 0 -and
        $Arguments[0] -in @("test", "run")
    $generatedExecutableLaunch = $false
    if ([System.IO.Path]::IsPathRooted($FileName) -and
        [System.IO.Path]::GetExtension($FileName) -ieq ".exe") {
        $candidatePath = [System.IO.Path]::GetFullPath($FileName)
        $artifactRoot = [System.IO.Path]::GetFullPath((Get-ClearraArtifactRoot))
        $comparison = if (Test-StartTestsWindows) {
            [System.StringComparison]::OrdinalIgnoreCase
        } else {
            [System.StringComparison]::Ordinal
        }
        $artifactPrefix = $artifactRoot.TrimEnd('\', '/') +
            [System.IO.Path]::DirectorySeparatorChar
        $generatedExecutableLaunch =
            $candidatePath.StartsWith($artifactPrefix, $comparison) -or
            (Test-ClearraPathInsideRepository $candidatePath)
    }
    if ($cargoLaunch -or $generatedExecutableLaunch) {
        Assert-ClearraTrustedExecutionSurface "" $Label
    }

    $resolvedFileName = Resolve-ClearraNativeFileName $FileName
    $argumentText = ConvertTo-ClearraProcessArgumentString $Arguments
    $cargoTargetDir = $null
    if ($cargoCommand) {
        Ensure-ClearraBuildArtifactCache
        $cargoTargetDir = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
            Get-ClearraCargoTargetDir
        } else {
            Assert-ClearraCanonicalCargoTargetDir $env:CARGO_TARGET_DIR
        }
    }
    $process = $null
    $stdoutTask = $null
    $stderrTask = $null

    try {
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $resolvedFileName
        $psi.Arguments = $argumentText
        $psi.UseShellExecute = $false
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.CreateNoWindow = $true
        if (-not [string]::IsNullOrWhiteSpace($cargoTargetDir)) {
            $psi.EnvironmentVariables['CARGO_TARGET_DIR'] = $cargoTargetDir
            $psi.EnvironmentVariables['CARGO_INCREMENTAL'] = '0'
        }

        $process = New-Object System.Diagnostics.Process
        $process.StartInfo = $psi
        try {
            [void]$process.Start()
        } catch {
            throw "failed to start native command '$FileName' resolved as '$resolvedFileName': $($_.Exception.Message)"
        }

        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()

        while (-not $process.HasExited) {
            Write-ClearraProgressLine $Scope $Label
            Start-Sleep -Milliseconds ([Math]::Max(50, $HeartbeatMs))
        }
        $process.WaitForExit()
        $stdoutTask.Wait()
        $stderrTask.Wait()

        $exitCode = $process.ExitCode
        $stdout = $stdoutTask.Result
        $stderr = $stderrTask.Result
        $text = ($stdout + "`n" + $stderr).Trim()
    } finally {
        if ($null -ne $process) {
            try {
                if (-not $process.HasExited) {
                    $process.Kill()
                }
            } catch {
            }
            try {
                $process.Dispose()
            } catch {
            }
        }
    }

    if ($cargoCommand -and $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('build', 'test', 'check', 'run', 'rustc', 'clippy') -and
        '--release' -notin $Arguments) {
        # Cargo and its requested test/run process have exited, both output
        # streams are drained, and the process handle is disposed. Keep this
        # debug-only cleanup separate from the whole-artifact size policy.
        # The helper skips cleanup if another Cargo/compiler/linker is active.
        try {
            $debugRetention = & (Join-Path $PSScriptRoot '../../tools/retain-clearra-debug-builds.ps1') `
                -CargoTargetDirectory $cargoTargetDir `
                -Apply
            if ($debugRetention.Status -eq 'pruned' -and $debugRetention.DeletedCount -gt 0) {
                $text = ($text + "`n[debug-build-retention] deleted=$($debugRetention.deletedCount) freed_bytes=$($debugRetention.freedBytes)").Trim()
            }
        } catch {
            Write-Warning "Clearra debug-build retention was skipped: $($_.Exception.Message)"
        }
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = $text
    }
}
