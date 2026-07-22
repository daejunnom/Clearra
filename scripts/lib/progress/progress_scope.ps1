function New-ClearraProgressScope {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [int]$Total,

        [int]$Workers = 1,

        [switch]$Quiet,
        [switch]$VerboseLog
    )

    [pscustomobject]@{
        Name = $Name
        Total = [Math]::Max(0, $Total)
        Workers = [Math]::Max(1, $Workers)
        Done = 0
        Running = 0
        Pending = [Math]::Max(0, $Total)
        Failed = 0
        StartedAt = Get-Date
        LastLineLength = 0
        Quiet = $Quiet.IsPresent
        VerboseLog = $VerboseLog.IsPresent
    }
}
