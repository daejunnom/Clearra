# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Workspace dependency product path checks are split by CLI, backend policy, rules, and supply contracts.
. (Join-Path $PSScriptRoot "validate_cli_product_path_contract.ps1")
. (Join-Path $PSScriptRoot "validate_backend_policy_contract.ps1")
. (Join-Path $PSScriptRoot "validate_rules_kicks_runtime_contract.ps1")
. (Join-Path $PSScriptRoot "validate_supply_runtime_contract.ps1")
