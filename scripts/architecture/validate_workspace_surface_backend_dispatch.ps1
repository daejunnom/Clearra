# Ordered SRP validators share the caller function scope.

$cliParserRouteSurface = Get-CliArgsParserSurface
$executionPolicyAssembler = Read-Text "crates/clearra-cli/src/assemble/execution_policy_assembler.rs"
$pcQueryAssembler = Read-Text "crates/clearra-cli/src/assemble/pc_query_assembler.rs"
$pcScenarioQueryAssembler = Read-Text "crates/clearra-cli/src/assemble/pc_scenario_query_assembler.rs"
$cliBackendAssemblySurface = "$cliParserRouteSurface`n$executionPolicyAssembler`n$pcQueryAssembler`n$pcScenarioQueryAssembler"
$twoLineCapability = Read-Text "crates/clearra-two-line/src/capability/two_line_capability.rs"
$twoLineFastPathAvailability = Read-Text "crates/clearra-two-line/src/capability/two_line_fast_path_availability.rs"
$twoLineDispatch = Read-Text "crates/clearra-two-line/src/capability/two_line_dispatch.rs"
$algorithmsDoc = Read-Text "docs/algorithms.md"
$mvpScopeDoc = Read-Text "docs/mvp-scope.md"
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
    "SpinTarget",
    "SpinTargetPredicate",
    "CoverageRowKind::SpinTarget",
    "SpecialSpinCaseRegistry",
    "KickEvidence",
    "VerifiedSpecialSpinProfile",
    "CandidateScoreStats",
    "PatternScoreContribution",
    "ScoreProfileObjectValidator",
    "BuildUpExecutionMode::EnumerateVariants"
)) {
    if ($architectureDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/architecture.md must document SpinTarget/scoring coverage architecture marker '$requiredMarker'"
    }
    if ($mvpScopeDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/mvp-scope.md must document SpinTarget/scoring MVP scope marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "KickTableProfileId::FinSpecial",
    "KickTableProfileId::IsoSpecial",
    "KickTableProfileId::NeoSpecial",
    "variant_probability_sum_for_spin",
    "verify_first_as_coverage_source",
    "retained_trace_average_as_expected_score"
)) {
    if ($architectureDoc -like "*$forbiddenMarker*" -or $mvpScopeDoc -like "*$forbiddenMarker*") {
        Add-ArchitectureError "architecture and MVP scope docs must not introduce forbidden spin/scoring marker '$forbiddenMarker'"
    }
}
foreach ($file in Get-RustFiles "crates") {
    $relativePath = Get-NormalizedRelativePath $file
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($forbiddenMarker in @(
        "KickTableProfileId::FinSpecial",
        "KickTableProfileId::IsoSpecial",
        "KickTableProfileId::NeoSpecial",
        "variant_probability_sum_for_spin",
        "verify_first_as_coverage_source",
        "retained_trace_average_as_expected_score"
    )) {
        if ($contents -like "*$forbiddenMarker*") {
            Add-ArchitectureError "$relativePath must not reintroduce forbidden spin/scoring marker '$forbiddenMarker'"
        }
    }
}
$tCoverageHeader = Read-Text "core-c/include/clr_coverage.h"
$tProblemHeader = Read-Text "core-c/include/clr_problem.h"
$tBuildVariantBuffer = Read-Text "core-c/src/buildup/build_variant_buffer.c"
$tBuildupTests = Get-BuildUpTestsValidationSurface
$tCoreFfiCoverageView = Read-Text "crates/clearra-core-ffi/src/buildup/coverage_row_view.rs"
$tCoreFfiBuildVariantView = Read-Text "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
$tCoreFfiNativeBuildup = Read-Text "crates/clearra-core-ffi/src/native/buildup.rs"
$tCoverageCargo = Read-Text "crates/clearra-coverage/Cargo.toml"
$tCoreDomainIds = Read-Text "crates/clearra-core-domain/src/ids/mod.rs"
$tCoreDomainSpinTargetId = Read-Text "crates/clearra-core-domain/src/ids/spin_target_id.rs"
$tCoreDomainScoreObjectiveCellId = Read-Text "crates/clearra-core-domain/src/ids/score_objective_cell_id.rs"
$tPatternBitSet = Read-Text "crates/clearra-coverage/src/pattern/pattern_bitset.rs"
$tPatternCoverageBitSet = Read-Text "crates/clearra-coverage/src/pattern/pattern_coverage_bitset.rs"
$tCoveragePatternBudget = Read-Text "crates/clearra-coverage/src/universe/coverage_pattern_budget.rs"
$tCoverageRow = Read-Text "crates/clearra-coverage/src/row/coverage_row.rs"
$tCoverageRowKind = Read-Text "crates/clearra-coverage/src/row/coverage_row_kind.rs"
$tSpinProbabilityResult = Read-Text "crates/clearra-coverage/src/probability/spin_probability_result.rs"
$tSpinCoverageMatrix = Read-Text "crates/clearra-coverage/src/matrix/spin_coverage_matrix.rs"
$tScoreCellMatrix = Read-Text "crates/clearra-coverage/src/matrix/score_cell_matrix.rs"
$tCoverageMatrixError = Read-Text "crates/clearra-coverage/src/matrix/coverage_matrix_error.rs"
$tCoverageContractTests = Read-Text "crates/clearra-coverage/src/coverage_contract_tests.rs"
$tObservedQueueExpansion = Read-Text "crates/clearra-supply/src/normalize/observed_queue_expansion.rs"
$tObservedQueueExpansionTests = Read-Text "crates/clearra-supply/src/normalize/observed_queue_expansion_tests.rs"
$tReplayEngine = Read-Text "crates/clearra-replay/src/replay/replay_engine.rs"
$tDiagnosticCode = Read-Text "crates/clearra-validation/src/diagnostic/diagnostic_code.rs"
$tCoreSecurityGate = Read-Text "crates/clearra-validation/src/validators/core_security_gate.rs"
$tCoreSecurityGateTests = Read-Text "crates/clearra-validation/src/validators/core_security_gate_tests.rs"
if (Test-DependencyLine $tCoverageCargo "clearra-scoring") {
    Add-ArchitectureError "clearra-coverage must not depend on clearra-scoring; coverage row kinds use core-domain opaque objective ids"
}
Assert-CargoDoesNotDepend "crates/clearra-coverage/Cargo.toml" @("clearra-scoring") "coverage probability layer must not depend on scoring"
Assert-ProductionImportAbsence "crates/clearra-coverage/src" @("clearra_scoring") "coverage row kinds must use opaque ids, not scoring crate types"
if (-not (Test-DependencyLine $tCoverageCargo "clearra-core-domain")) {
    Add-ArchitectureError "clearra-coverage must depend on clearra-core-domain for SpinTargetId and ScoreObjectiveCellId"
}
foreach ($requiredMarker in @("spin_target_id", "score_objective_cell_id", "pub use spin_target_id::SpinTargetId", "pub use score_objective_cell_id::ScoreObjectiveCellId")) {
    if ($tCoreDomainIds -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-domain ids module must expose coverage/scoring shared id marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("pub struct SpinTargetId", "pub fn new", "pub fn as_str")) {
    if ($tCoreDomainSpinTargetId -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTargetId must live in clearra-core-domain with marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("pub struct ScoreObjectiveCellId", "pub fn new", "pub fn as_str")) {
    if ($tCoreDomainScoreObjectiveCellId -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreObjectiveCellId must live in clearra-core-domain with marker '$requiredMarker'"
    }
}
foreach ($coverageSourcePath in @(
    "crates/clearra-coverage/src/row/coverage_row_kind.rs",
    "crates/clearra-coverage/src/row/spin_coverage_row.rs",
    "crates/clearra-coverage/src/probability/spin_probability_result.rs",
    "crates/clearra-coverage/src/matrix/spin_coverage_matrix.rs"
)) {
    $coverageSource = Read-Text $coverageSourcePath
    if ($coverageSource -like "*clearra_scoring*") {
        Add-ArchitectureError "$coverageSourcePath must not import clearra_scoring; use clearra_core_domain objective ids"
    }
}
foreach ($requiredMarker in @(
    "CLR_SCORE_MATRIX_CAPACITY_EXCEEDED",
    "CLR_SPIN_COVERAGE_CAPACITY_EXCEEDED"
)) {
    if ($tCoverageHeader -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clr_coverage.h must expose T memory/coverage budget status marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED",
    "CLR_BUILDUP_ENUMERATION_TRUNCATED",
    "CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING"
)) {
    if ($tProblemHeader -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clr_problem.h must expose T BuildUp budget status marker '$requiredMarker'"
    }
    if ($tCoreFfiNativeBuildup -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-ffi native buildup mirror must expose T status marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED",
    "kick_evidence_buffer_reports_capacity_exhausted",
    "kick_evidence_buffer_budget_rejects_exhaustion"
)) {
    $surface = "$tBuildVariantBuffer`n$tBuildupTests"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "C BuildVariant buffer must report kick evidence budget exhaustion marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "OwnedCorePatternBitSetSnapshot",
    "owned_snapshot",
    "c_pattern_bitset_words_do_not_escape_scope",
    "ffi_pattern_bitset_pointer_escape_is_blocked_by_owned_snapshot",
    "c_coverage_capacity_and_rust_pattern_limit_are_aligned"
)) {
    if ($tCoreFfiCoverageView -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-ffi coverage view must copy scope-bound words into owned snapshots marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "CoveragePatternBudget",
    "C_COVERAGE_DEFAULT_PATTERN_BUDGET",
    "c_bridge_default",
    "product_unbounded",
    "c_fixed_1024_limit_is_default_budget_not_product_invariant"
)) {
    if ($tCoveragePatternBudget -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CoveragePatternBudget must keep C coverage capacity as default budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "pattern_universe_id",
    "pattern_weight_model_id",
    "CoverageRowKind",
    "PatternCoverageBitSet",
    "pattern_coverage_bits"
)) {
    if ($tCoverageRow -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CoverageRow must expose universe/weight/kind and typed pattern coverage marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "Pc",
    "Setup",
    "Build",
    "SpinTarget",
    "ScoreCell",
    "clearra_core_domain::ids",
    "SpinTarget(SpinTargetId)",
    "ScoreCell(ScoreObjectiveCellId)"
)) {
    if ($tCoverageRowKind -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CoverageRowKind must expose coverage/probability row kind marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "PatternCoverageBitSet",
    "ShapeUnionMask",
    "shape_union_mask_is_not_pattern_coverage",
    "shape_mask_cannot_be_used_as_pattern_coverage",
    "candidate_shape_union_mask",
    "gpu_shape_union_mask",
    "pattern_coverage_bits",
    "pattern_bitset_union",
    "coverage_probability_bits"
)) {
    if ($tPatternCoverageBitSet -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PatternCoverageBitSet must be separate from shape union mask marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ObservedQueueProbabilityContract",
    "materialized_probability_mass",
    "renormalized",
    "truncation_reason",
    "observed_queue_pattern_limit"
)) {
    if ($tObservedQueueExpansion -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Observed queue truncation output contract must expose marker '$requiredMarker'"
    }
}
if ($tObservedQueueExpansionTests -notlike "*observed_queue_truncation_keeps_materialized_probability_mass*") {
    Add-ArchitectureError "Observed queue truncation tests must verify materialized mass is not renormalized"
}
foreach ($requiredMarker in @(
    "SpinProbabilityResult",
    "pattern_universe_id",
    "pattern_weight_model_id",
    "materialized_probability_mass",
    "renormalized",
    "truncation_reason"
)) {
    if ($tSpinProbabilityResult -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinProbabilityResult must expose probability output contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "coverage_row_rejects_universe_mismatch",
    "coverage_row_rejects_weight_model_mismatch",
    "coverage_row_rejects_row_kind_mismatch",
    "spin_probability_uses_pattern_bitset_union",
    "score_does_not_change_coverage_probability"
)) {
    if ($tCoverageContractTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "coverage contract tests must keep T coverage/probability marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "kick_evidence_buffer_respects_scope_lifetime",
    "ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape",
    "ffi_build_variant_view_preserves_hold_branch_kind",
    "hold_branch_kind",
    "to_vec()"
)) {
    if ($tCoreFfiBuildVariantView -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-ffi BuildVariant view must copy kick evidence before scope ends marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "new_with_word_budget",
    "WordCapacityExceeded",
    "pattern_bitset_dynamic_word_allocation_budget_is_checked",
    "pattern_bitset_dynamic_word_allocation_scope_is_enforced"
)) {
    if ($tPatternBitSet -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PatternBitSet must guard dynamic word allocation budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "SpinCoverageMatrixBudget",
    "SpinCoverageCapacityExceeded",
    "spin_coverage_matrix_respects_memory_budget",
    "spin_coverage_matrix_memory_budget_rejects_word_overflow"
)) {
    $surface = "$tSpinCoverageMatrix`n$tCoverageMatrixError"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinCoverageMatrix must expose memory budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ScoreCellMatrixBudget",
    "ScoreCellCapacityExceeded",
    "score_cell_matrix_reports_capacity_exceeded",
    "score_cell_matrix_memory_budget_rejects_word_overflow"
)) {
    $surface = "$tScoreCellMatrix`n$tCoverageMatrixError"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreCellMatrix must expose memory budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ReplayTraceBufferBudget",
    "ReplayTraceBufferBudgetExceeded",
    "replay_trace_buffer_respects_memory_budget"
)) {
    if ($tReplayEngine -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ReplayTrace buffer must expose memory budget marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "EScoreMatrixCapacityExceeded",
    "ESpinCoverageCapacityExceeded",
    "ESpinCoverageUniverseMismatch",
    "ECoverageCapacityExceeded",
    "EBuildUpVariantEnumerationTruncated",
    "EKickEvidenceBufferExhausted",
    "WSpinTargetProbabilityIncomplete",
    "WSpecialSpinDescriptorOnly",
    "WBuildUpEnumerationTruncated",
    "WObservedQueueProbabilityIncomplete",
    "WTraceRetentionTruncated"
)) {
    if ($tDiagnosticCode -notlike "*$requiredMarker*") {
        Add-ArchitectureError "DiagnosticCode must expose T memory/FFI budget diagnostic marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "coverage_capacity_exceeded",
    "coverage_capacity_exceeded_is_error_not_success",
    "score_matrix_capacity_exceeded",
    "score_matrix_capacity_exceeded_diagnostic",
    "spin_coverage_capacity_exceeded",
    "kick_evidence_buffer_exhausted",
    "buildup_enumeration_truncated",
    "buildup_enumeration_truncation_reports_diagnostic",
    "build_up_variant_enumeration_truncated",
    "build_up_count_reports_truncation",
    "observed_queue_probability_incomplete",
    "observed_queue_truncation_is_not_renormalized",
    "trace_retention_truncated"
)) {
    $surface = "$tCoreSecurityGate`n$tCoreSecurityGateTests"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CoreSecurityGate must report T budget/truncation diagnostics marker '$requiredMarker'"
    }
}
