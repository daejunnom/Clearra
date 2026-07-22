# This file is dot-sourced by scripts/clearra.ps1.
# Keep this as a thin helper dispatcher; implementation lives in focused helper files.
. (Join-Path $ClearraScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-application-control.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-runtime-environment.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-verify-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-native-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-core-c-task-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-diagnostic-task-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-task-ui-helpers.ps1")
