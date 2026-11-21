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

        [int]$HeartbeatMs = 500,

        [ValidateRange(0, 2147483647)]
        [int]$TimeoutMs = 0
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
        foreach ($argument in $Arguments) {
            if ($argument -eq '--') { break }
            if ($argument -match '^(?:--target-dir|--build-dir|--artifact-dir|--out-dir|--config)(?:=|$)') {
                throw 'Cargo build path/config overrides are forbidden; use the selected build transaction.'
            }
        }
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
        $timer = [System.Diagnostics.Stopwatch]::StartNew()

        while (-not $process.HasExited) {
            Write-ClearraProgressLine $Scope $Label
            $waitMs = [Math]::Max(50, $HeartbeatMs)
            if ($TimeoutMs -gt 0) {
                $remainingMs = $TimeoutMs - $timer.ElapsedMilliseconds
                if ($remainingMs -le 0) {
                    throw [System.TimeoutException]::new(
                        "'$Label' exceeded its ${TimeoutMs}ms process-test deadline."
                    )
                }
                $waitMs = [int][Math]::Min($waitMs, $remainingMs)
            }
            # Return immediately when a short probe exits; the heartbeat is
            # a maximum observation interval, not a minimum probe duration.
            [void]$process.WaitForExit($waitMs)
        }
        if ($TimeoutMs -gt 0) {
            # Process exit and pipe EOF are different events. A leaked handle
            # must not turn the output-read half of a bounded probe unbounded.
            $remainingMs = $TimeoutMs - $timer.ElapsedMilliseconds
            $captures = [System.Threading.Tasks.Task[]]@($stdoutTask, $stderrTask)
            if ($remainingMs -le 0 -or
                -not [System.Threading.Tasks.Task]::WaitAll($captures, [int]$remainingMs)) {
                throw [System.TimeoutException]::new(
                    "'$Label' exceeded its ${TimeoutMs}ms process-test deadline while draining output."
                )
            }
        } else {
            $process.WaitForExit()
            $stdoutTask.Wait()
            $stderrTask.Wait()
        }

        $exitCode = $process.ExitCode
        $stdout = $stdoutTask.Result
        $stderr = $stderrTask.Result
        $text = ($stdout + "`n" + $stderr).Trim()
    } finally {
        if ($null -ne $process) {
            try {
                if (-not $process.HasExited) {
                    if ($TimeoutMs -gt 0 -and
                        $null -ne $process.GetType().GetMethod('Kill', [type[]]@([bool]))) {
                        $process.Kill($true)
                    } else {
                        $process.Kill()
                    }
                    if ($TimeoutMs -gt 0) {
                        [void]$process.WaitForExit(2000)
                    }
                }
            } catch {
            }
            try {
                $process.Dispose()
            } catch {
            }
        }
    }

    # The build owner retires whole generations. Do not prune a shared current
    # target between commands while later tests may still consume its binaries.

    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = $text
    }
}
