# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# C core validation is split by memory/problem, board/rule, candidate/reachability, and packing/BuildUp contracts.
. (Join-Path $PSScriptRoot "validate_c_memory_problem_contract.ps1")
. (Join-Path $PSScriptRoot "validate_c_board_piece_rule_contract.ps1")
. (Join-Path $PSScriptRoot "validate_c_candidate_reachability_contract.ps1")
. (Join-Path $PSScriptRoot "validate_c_packing_buildup_contract.ps1")
