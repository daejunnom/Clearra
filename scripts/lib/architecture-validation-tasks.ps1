function New-ArchitectureValidationTask(
    [string]$Name,
    [string]$FunctionName,
    [switch]$RequiresWorkspaceDependencyGraph
) {
    return [pscustomobject]@{
        Name = $Name
        FunctionName = $FunctionName
        RequiresWorkspaceDependencyGraph = [bool]$RequiresWorkspaceDependencyGraph
    }
}

function New-CurrentArchitectureValidationTasks() {
    return @(
        New-ArchitectureValidationTask "Dependency Architecture" "Invoke-DependencyArchitectureValidation" -RequiresWorkspaceDependencyGraph
        New-ArchitectureValidationTask "PC4 Full Solution Authority" "Invoke-Pc4FullSolutionAuthorityContractValidation" -RequiresWorkspaceDependencyGraph
        New-ArchitectureValidationTask "No Product Debt Architecture" "Invoke-NoProductDebtStaticValidation"
        New-ArchitectureValidationTask "Forbidden API Architecture" "Invoke-ReleaseForbiddenApiValidation"
        New-ArchitectureValidationTask "Public ABI Architecture" "Invoke-PublicAbiContractValidation"
        New-ArchitectureValidationTask "Unsafe Boundary Architecture" "Invoke-UnsafeBoundaryArchitectureValidation"
        New-ArchitectureValidationTask "Unsupported Capability Architecture" "Invoke-UnsupportedCapabilityStaticValidation"
        New-ArchitectureValidationTask "PowerShell Syntax" "Assert-PowerShellSourcesParse"
    )
}

function New-AllArchitectureValidationTasks() {
    return @(New-CurrentArchitectureValidationTasks)
}
