# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Workspace surface validation keeps broad cross-crate invariant markers out of the dispatcher.
function Convert-PascalToKebab([string]$Name) {
    $chars = New-Object System.Collections.Generic.List[char]
    for ($index = 0; $index -lt $Name.Length; $index++) {
        $ch = $Name[$index]
        if ($index -gt 0 -and [char]::IsUpper($ch)) {
            $chars.Add('-')
        }
        $chars.Add([char]::ToLowerInvariant($ch))
    }
    return -join $chars
}
function Convert-PascalToSnake([string]$Name) {
    $chars = New-Object System.Collections.Generic.List[char]
    for ($index = 0; $index -lt $Name.Length; $index++) {
        $ch = $Name[$index]
        if ($index -gt 0 -and [char]::IsUpper($ch)) {
            $chars.Add('_')
        }
        $chars.Add([char]::ToLowerInvariant($ch))
    }
    return -join $chars
}
function Get-RustEnumVariants([string]$Contents, [string]$EnumName) {
    $escaped = [regex]::Escape($EnumName)
    $match = [regex]::Match($Contents, "(?s)pub enum $escaped\s*\{(?<body>.*?)\n\}")
    $variants = New-Object System.Collections.Generic.List[string]
    if (-not $match.Success) {
        Add-ArchitectureError "Could not find Rust enum '$EnumName'"
        return $variants
    }
    foreach ($line in ($match.Groups["body"].Value -split "`n")) {
        if ($line -match '^\s*([A-Z][A-Za-z0-9]*)(?:\(|,|\s)') {
            $variants.Add($Matches[1])
        }
    }
    return $variants
}
function Get-CliCommandNames([string]$EnumContents) {
    $names = New-Object System.Collections.Generic.List[string]
    foreach ($variant in Get-RustEnumVariants $EnumContents "CliCommand") {
        $names.Add((Convert-PascalToKebab $variant))
    }
    return $names
}
function Assert-CliCommandSurfaceIsSynchronized() {
    $cliParser = Read-Text "crates/clearra-cli/src/args/cli_parser.rs"
    $cliCommandParser = Read-Text "crates/clearra-cli/src/args/cli_command_parser.rs"
    $cliLib = Read-Text "crates/clearra-cli/src/lib.rs"
    $commandsMod = Read-Text "crates/clearra-cli/src/commands/mod.rs"

    $commandVariants = @(Get-RustEnumVariants $cliParser "ParsedCliCommand" |
        Where-Object { $_ -ne "Unsupported" -and $_ -ne "Help" })
    $helpVariants = @(Get-RustEnumVariants $cliParser "CliHelpTopic" |
        Where-Object { $_ -ne "TopLevel" })

    foreach ($variant in $commandVariants) {
        $kebab = Convert-PascalToKebab $variant
        $snake = Convert-PascalToSnake $variant
        $handler = "${variant}Command"
        $handlerFile = "crates/clearra-cli/src/commands/${snake}_command.rs"
        $parserFile = "crates/clearra-cli/src/args/parse_${snake}_args.rs"

        if (-not ($helpVariants -contains $variant)) {
            Add-ArchitectureError "CLI command '$variant' must have a matching CliHelpTopic variant"
        }
        if ($cliCommandParser -notmatch "`"$([regex]::Escape($kebab))`"\s*=>\s*parse_$([regex]::Escape($snake))\(command_args\)") {
            Add-ArchitectureError "CLI parser must route command '$kebab' to parse_$snake(command_args)"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $Root $parserFile))) {
            Add-ArchitectureError "CLI parser must define command-specific parser file $parserFile for command '$kebab'"
        } else {
            $parserContents = Read-Text $parserFile
            if ($parserContents -notmatch "fn\s+parse_$([regex]::Escape($snake))\(") {
                Add-ArchitectureError "CLI parser file $parserFile must define parse_$snake for command '$kebab'"
            }
        }
        if ($cliLib -notmatch "ParsedCliCommand::$([regex]::Escape($variant))\(") {
            Add-ArchitectureError "CLI route_invocation must match ParsedCliCommand::$variant"
        }
        if ($cliLib -notmatch "$([regex]::Escape($handler))::run\(") {
            Add-ArchitectureError "CLI route_invocation must call $handler::run for command '$kebab'"
        }
        if ($commandsMod -notmatch "pub mod $([regex]::Escape($snake))_command;") {
            Add-ArchitectureError "commands/mod.rs must export module ${snake}_command for command '$kebab'"
        }
        if ($commandsMod -notmatch "pub use $([regex]::Escape($snake))_command::$([regex]::Escape($handler));") {
            Add-ArchitectureError "commands/mod.rs must re-export $handler for command '$kebab'"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $Root $handlerFile))) {
            Add-ArchitectureError "CLI command '$kebab' must have handler file $handlerFile"
        }
        if ($cliParser -notmatch "Self::$([regex]::Escape($variant))\s*=>") {
            Add-ArchitectureError "CliHelpTopic::into_output must render help for command '$kebab'"
        }
    }

    foreach ($variant in $helpVariants) {
        if (-not ($commandVariants -contains $variant)) {
            Add-ArchitectureError "CliHelpTopic::$variant must correspond to a concrete ParsedCliCommand variant"
        }
    }
}
function Invoke-WorkspaceSurfaceArchitectureValidation() {
    . (Join-Path $PSScriptRoot "validate_workspace_surface_base.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_board_piece.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_build_fumen_fixtures.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_output_contract.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_ui_gui.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_spin_score.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_setup_score.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_pc_graph.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_backend_dispatch.ps1")
    . (Join-Path $PSScriptRoot "validate_workspace_surface_buildup_gpu.ps1")
}
