# This owner checks executable syntax, not comments, file sizes, or source layout.

function Assert-PowerShellSourcesParse() {
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -in @('.ps1', '.psm1') }) {
        $tokens = $null
        $parseErrors = $null
        [System.Management.Automation.Language.Parser]::ParseFile(
            $file.FullName,
            [ref]$tokens,
            [ref]$parseErrors
        ) | Out-Null
        if ($parseErrors.Count -gt 0) {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) does not parse as PowerShell"
        }
    }
}
