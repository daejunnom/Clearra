$ValidationTaskRunnerLibRoot = Split-Path -Parent $PSCommandPath
. (Join-Path $ValidationTaskRunnerLibRoot "architecture-validation-common.ps1")

$script:LastValidationProgressLength = 0
$script:LastValidationProgressState = ""
$script:LastValidationProgressAt = [datetime]::MinValue
function Format-ValidationElapsed([TimeSpan]$Elapsed) {
    if ($Elapsed.TotalSeconds -lt 1) {
        return "$([math]::Round($Elapsed.TotalMilliseconds))ms"
    }
    return "$([math]::Round($Elapsed.TotalSeconds, 2))s"
}function Write-ValidationProgress(
    [int]$Done,
    [int]$Running,
    [int]$Pending,
    [int]$Failed,
    [int]$Workers,
    [TimeSpan]$Elapsed,
    [int]$Total
) {
    $state = "$Done|$Running|$Pending|$Failed|$Workers|$Total"
    $now = Get-Date
    if ($state -eq $script:LastValidationProgressState -and
        (($now - $script:LastValidationProgressAt).TotalMilliseconds -lt 500)) {
        return
    }

    $message = "[validate] $Done/$Total done | running $Running | pending $Pending | failed $Failed | workers $Workers | $(Format-ValidationElapsed $Elapsed)"
    $paddingLength = [Math]::Max(0, $script:LastValidationProgressLength - $message.Length)
    $padding = " " * $paddingLength
    [Console]::Out.Write("`r$message$padding")
    $script:LastValidationProgressLength = $message.Length
    $script:LastValidationProgressState = $state
    $script:LastValidationProgressAt = $now
}function Complete-ValidationProgressLine() {
    if ($script:LastValidationProgressLength -gt 0) {
        [Console]::Out.WriteLine("")
    }
    $script:LastValidationProgressLength = 0
    $script:LastValidationProgressState = ""
    $script:LastValidationProgressAt = [datetime]::MinValue
}function New-ValidationTaskFailureResult([string]$Name, [string]$Message, [TimeSpan]$Duration) {
    return New-ArchitectureValidationTaskResult `
        -Name $Name `
        -Status "Failed" `
        -Errors @("$Name failed: $Message") `
        -Warnings @() `
        -DurationMs ([int64]$Duration.TotalMilliseconds)
}function Invoke-ValidationTaskRunner(
    [object[]]$Tasks,
    [string]$ArchitectureValidationScript,
    [int]$Workers = 1,
    [switch]$QuietProgress
) {
    $taskCount = @($Tasks).Count
    if ($taskCount -eq 0) {
        return @()
    }

    $workerCount = [Math]::Max(1, [Math]::Min($Workers, $taskCount))
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $pending = New-Object System.Collections.Queue
    foreach ($task in $Tasks) {
        $pending.Enqueue($task)
    }

    $results = New-Object System.Collections.Generic.List[object]
    $running = New-Object System.Collections.Generic.List[object]
    $failed = 0
    $pool = [runspacefactory]::CreateRunspacePool(1, $workerCount)
    $pool.Open()

    $taskScript = {
        param(
            [string]$ValidationScript,
            [string]$TaskName
        )

        . $ValidationScript
        Invoke-ArchitectureValidation -TaskName $TaskName -QuietProgress
    }

    try {
        if (-not $QuietProgress.IsPresent) {
            Write-ValidationProgress 0 0 $pending.Count 0 $workerCount $stopwatch.Elapsed $taskCount
        }

        while ($pending.Count -gt 0 -or $running.Count -gt 0) {
            while ($pending.Count -gt 0 -and $running.Count -lt $workerCount) {
                $task = $pending.Dequeue()
                $powershell = [powershell]::Create()
                $powershell.RunspacePool = $pool
                [void]$powershell.AddScript($taskScript).AddArgument($ArchitectureValidationScript).AddArgument($task.Name)
                $asyncResult = $powershell.BeginInvoke()
                $running.Add([pscustomobject]@{
                    Task = $task
                    PowerShell = $powershell
                    AsyncResult = $asyncResult
                    Started = [System.Diagnostics.Stopwatch]::StartNew()
                })
            }

            if (-not $QuietProgress.IsPresent) {
                Write-ValidationProgress $results.Count $running.Count $pending.Count $failed $workerCount $stopwatch.Elapsed $taskCount
            }

            $finished = @($running | Where-Object { $_.AsyncResult.IsCompleted })
            if ($finished.Count -eq 0) {
                Start-Sleep -Milliseconds 50
                continue
            }

            foreach ($item in $finished) {
                try {
                    $output = @($item.PowerShell.EndInvoke($item.AsyncResult))
                    if ($output.Count -eq 0) {
                        $result = New-ValidationTaskFailureResult $item.Task.Name "task returned no result" $item.Started.Elapsed
                    } else {
                        $result = $output[-1]
                    }
                }
                catch {
                    $result = New-ValidationTaskFailureResult $item.Task.Name $_.Exception.Message $item.Started.Elapsed
                }
                finally {
                    $item.PowerShell.Dispose()
                }

                if ($result.Status -eq "Failed") {
                    $failed++
                }
                $results.Add($result)
                [void]$running.Remove($item)
            }
        }
    }
    finally {
        for ($cleanupIndex = 0; $cleanupIndex -lt $running.Count; $cleanupIndex++) {
            $item = $running[$cleanupIndex]
            try {
                $item.PowerShell.Stop()
                $item.PowerShell.Dispose()
            } catch {
            }
        }
        $pool.Close()
        $pool.Dispose()

        if (-not $QuietProgress.IsPresent) {
            Write-ValidationProgress $results.Count 0 0 $failed $workerCount $stopwatch.Elapsed $taskCount
            Complete-ValidationProgressLine
        }
    }

    return @($results.ToArray())
}