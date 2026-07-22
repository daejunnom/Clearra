function Invoke-ClearraProgressCase {
    param(
        [Parameter(Mandatory)]
        $Scope,

        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [scriptblock]$Body,

        [switch]$PreserveOutput
    )

    $Scope.Running = 1
    $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
    Write-ClearraProgressLine $Scope $Name

    try {
        if ($PreserveOutput.IsPresent) {
            Complete-ClearraProgressLine $Scope
            & $Body
        } else {
            $bodyOutput = @(& $Body)
            if ($Scope.VerboseLog -and $bodyOutput.Count -gt 0) {
                Complete-ClearraProgressLine $Scope
                foreach ($line in $bodyOutput) {
                    if ($null -ne $line) {
                        [Console]::Out.WriteLine($line.ToString())
                    }
                }
            }
        }
        $Scope.Done += 1
        $Scope.Running = 0
        $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done)
        Write-ClearraProgressLine $Scope
    } catch {
        $Scope.Failed += 1
        $Scope.Running = 0
        $Scope.Pending = [Math]::Max(0, $Scope.Total - $Scope.Done - 1)
        Write-ClearraProgressLine $Scope $Name
        Complete-ClearraProgressLine $Scope
        [Console]::Out.WriteLine("[$($Scope.Name)] failed | $Name")
        if ($null -ne $_.Exception -and -not [string]::IsNullOrWhiteSpace($_.Exception.Message)) {
            [Console]::Out.WriteLine($_.Exception.Message)
        }
        throw
    }
}
