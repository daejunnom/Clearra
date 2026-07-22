function Format-ClearraProgressElapsed {
    param([timespan]$Elapsed)

    if ($Elapsed.TotalSeconds -lt 1) {
        return "$([math]::Round($Elapsed.TotalMilliseconds))ms"
    }

    return "$([math]::Round($Elapsed.TotalSeconds, 2))s"
}function Write-ClearraProgressLine {
    param(
        [Parameter(Mandatory)]
        $Scope,

        [string]$Current = ""
    )

    if ($Scope.Quiet) {
        return
    }

    $elapsed = (Get-Date) - $Scope.StartedAt
    $suffix = if ([string]::IsNullOrWhiteSpace($Current)) {
        ""
    } else {
        " | $Current"
    }

    $message = "[{0}] {1}/{2} done | running {3} | pending {4} | failed {5} | workers {6} | {7}{8}" -f `
        $Scope.Name,
        $Scope.Done,
        $Scope.Total,
        $Scope.Running,
        $Scope.Pending,
        $Scope.Failed,
        $Scope.Workers,
        (Format-ClearraProgressElapsed $elapsed),
        $suffix

    $paddingLength = [Math]::Max(0, $Scope.LastLineLength - $message.Length)
    $padding = " " * $paddingLength

    [Console]::Out.Write("`r$message$padding")
    $Scope.LastLineLength = $message.Length
}function Complete-ClearraProgressLine {
    param($Scope)

    if (-not $Scope.Quiet -and $Scope.LastLineLength -gt 0) {
        [Console]::Out.WriteLine("")
    }

    $Scope.LastLineLength = 0
}function Write-ClearraProgressVerboseLine {
    param(
        [Parameter(Mandatory)]
        $Scope,

        [Parameter(Mandatory)]
        [string]$Message
    )

    Complete-ClearraProgressLine $Scope
    [Console]::Out.WriteLine($Message)
}