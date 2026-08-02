[CmdletBinding()]
param(
    [string]$ApplicationId = $env:DISCORD_APPLICATION_ID
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($ApplicationId -and $ApplicationId -notmatch '^\d{17,20}$') {
    throw "ApplicationId must be the 17-20 digit Discord application ID."
}

$createdToken = $false
$secureToken = $null
$tokenPointer = [IntPtr]::Zero
$previousApplicationId = $env:DISCORD_APPLICATION_ID
$changedApplicationId = $false

try {
    if ([string]::IsNullOrWhiteSpace($env:DISCORD_TOKEN)) {
        $secureToken = Read-Host `
            "Discord bot token (Developer Portal > Bot > Token)" `
            -AsSecureString
        $tokenPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR(
            $secureToken
        )
        $plainToken = [Runtime.InteropServices.Marshal]::PtrToStringBSTR(
            $tokenPointer
        )
        if ([string]::IsNullOrWhiteSpace($plainToken)) {
            throw "The Discord bot token cannot be empty."
        }
        $env:DISCORD_TOKEN = $plainToken
        $plainToken = $null
        $createdToken = $true
    }
    elseif ($env:DISCORD_TOKEN -eq "System.Security.SecureString") {
        throw @"
DISCORD_TOKEN is a SecureString object name instead of the bot token. Remove it
and rerun this script so the masked prompt can convert it safely for the child
Node process.
"@
    }

    if ($ApplicationId) {
        $env:DISCORD_APPLICATION_ID = $ApplicationId
        $changedApplicationId = $true
    }

    $entryPoint = Join-Path $PSScriptRoot "..\src\register-commands.mjs"
    & node $entryPoint
    if ($LASTEXITCODE -ne 0) {
        throw "Discord command registration failed with exit code $LASTEXITCODE."
    }
}
finally {
    if ($createdToken) {
        Remove-Item Env:DISCORD_TOKEN -ErrorAction SilentlyContinue
    }
    if ($tokenPointer -ne [IntPtr]::Zero) {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($tokenPointer)
    }
    if ($secureToken -is [IDisposable]) {
        $secureToken.Dispose()
    }
    if ($changedApplicationId) {
        if ($null -eq $previousApplicationId) {
            Remove-Item Env:DISCORD_APPLICATION_ID -ErrorAction SilentlyContinue
        }
        else {
            $env:DISCORD_APPLICATION_ID = $previousApplicationId
        }
    }
}
