# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# BuildUp/product product path checks are split by executor, setup, build coverage, and percent/path contracts.
. (Join-Path $PSScriptRoot "validate_core_executor_contract.ps1")
. (Join-Path $PSScriptRoot "validate_setup_search_product_path_contract.ps1")
. (Join-Path $PSScriptRoot "validate_build_coverage_product_path_contract.ps1")
. (Join-Path $PSScriptRoot "validate_percent_path_contract.ps1")
