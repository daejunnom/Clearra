# This file is dot-sourced by an architecture validation wrapper.
# Keep the grouped validation functions side-effect free at load time.
function Invoke-CMemoryScopeValidation() {
foreach ($requiredPath in @(
        "core-c/include/clr_memory.h",
        "core-c/src/memory/clr_mem_context.c",
        "core-c/src/memory/clr_scope.c",
        "core-c/src/memory/clr_allocators.c",
        "core-c/src/memory/clr_release_queue.c",
        "core-c/src/memory/clr_gpu_buffer_lifetime.c",
        "core-c/src/memory/clr_memory_debug.c",
        "core-c/tests/memory_tests.c",
        "crates/clearra-core-ffi/src/memory/memory_abi.rs",
        "crates/clearra-core-ffi/src/memory/contract_core_context.rs",
        "crates/clearra-core-ffi/src/memory/contract_search_scope.rs",
        "crates/clearra-core-ffi/src/memory/contract_batch_scope.rs",
        "crates/clearra-core-ffi/src/memory/memory_backend_kind.rs",
        "crates/clearra-core-ffi/src/memory/native_core_context.rs",
        "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs",
        "crates/clearra-core-ffi/src/memory/native_memory_error.rs",
        "crates/clearra-core-ffi/src/memory/native_scope.rs",
        "crates/clearra-core-ffi/src/memory/native_leak_report.rs",
        "crates/clearra-core-ffi/src/memory/core_context.rs",
        "crates/clearra-core-ffi/src/memory/search_scope.rs",
        "crates/clearra-core-ffi/src/memory/batch_scope.rs",
        "crates/clearra-core-ffi/src/memory/release_signal.rs",
        "crates/clearra-core-executor/src/memory/scope_guard.rs",
        "docs/memory-lifecycle.md"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M2 C memory scope required file is missing: $requiredPath"
        }
    }
$memoryHeader = Read-Text "core-c/include/clr_memory.h"
foreach ($requiredMarker in @(
        "ClrMemContext",
        "ClrScope",
        "CLR_SCOPE_SEARCH",
        "CLR_SCOPE_BATCH",
        "CLR_SCOPE_WORKER",
        "CLR_SCOPE_GPU_TRANSFER",
        "ClrScopeState",
        "CLR_SCOPE_PENDING_RELEASE",
        "CLR_SCOPE_RELEASED",
        "CLR_SCOPE_ABORTED",
        "clr_mem_context_create",
        "clr_mem_context_release",
        "clr_mem_context_release(ClrMemContext **context)",
        "clr_scope_release",
        "clr_scope_abort",
        "clr_scope_state",
        "clr_arena_alloc",
        "clr_pool_alloc",
        "clr_scratch_alloc",
        "clr_epoch_advance",
        "clr_release_queue_drain",
        "clr_gpu_buffer_register",
        "clr_gpu_buffer_register_for_scope",
        "clr_gpu_buffer_set_fence_epoch",
        "pending_gpu_buffer_releases",
        "clr_memory_debug_check_scope"
    )) {
        if ($memoryHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/include/clr_memory.h must expose M2 memory marker '$requiredMarker'"
        }
    }
$coreCmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/memory/clr_mem_context.c",
        "src/memory/clr_scope.c",
        "src/memory/clr_allocators.c",
        "src/memory/clr_release_queue.c",
        "src/memory/clr_gpu_buffer_lifetime.c",
        "src/memory/clr_memory_debug.c",
        "memory_tests"
    )) {
        if ($coreCmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must compile M2 memory marker '$requiredMarker'"
        }
    }
$memoryTests = Read-Text "core-c/tests/memory_tests.c"
foreach ($requiredMarker in @(
        "context_create_release",
        "memory_context_release_nulls_pointer",
        "memory_context_double_release_does_not_deref_freed_memory",
        "memory_context_release_releases_live_scopes",
        "memory_context_release_releases_gpu_records",
        "memory_context_release_drains_release_queue_metadata",
        "memory_context_leak_report_before_release_reports_live_scopes",
        "memory_context_leak_report_after_release_requires_snapshot",
        "search_scope_create_release",
        "batch_scope_create_release",
        "double_release_detect",
        "scope_abort_releases_memory",
        "expect_zero_live_leaks",
        "debug_poison_and_canary_are_detected",
        "release_queue_uses_epoch_to_release_scope",
        "scope_deferred_for_release_cannot_be_released_directly_twice",
        "gpu_buffer_lifetime_is_reported",
        "gpu_buffer_release_before_fence_is_deferred",
        "gpu_buffer_release_before_fence_deferred",
        "gpu_buffer_double_release_is_error"
    )) {
        if ($memoryTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/tests/memory_tests.c must verify M2 memory contract marker '$requiredMarker'"
        }
    }
$ffiMemoryMod = Read-Text "crates/clearra-core-ffi/src/memory/mod.rs"
foreach ($requiredMarker in @("memory_abi", "contract_core_context", "contract_search_scope", "contract_batch_scope", "memory_backend_kind", "native_core_context", "native_memory_bindings", "native_memory_error", "native_scope", "native_leak_report", "core_context", "search_scope", "batch_scope", "release_signal")) {
        if ($ffiMemoryMod -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi memory module must export M2 wrapper marker '$requiredMarker'"
        }
    }
$coreFfiCargoToml = Read-Text "crates/clearra-core-ffi/Cargo.toml"
foreach ($requiredMarker in @(
            "native-memory-binding = []",
            'native-c-core = ["native-memory-binding"]'
        )) {
        if (-not $coreFfiCargoToml.Contains($requiredMarker)) {
            Add-ArchitectureError "clearra-core-ffi Cargo.toml must gate native memory binding marker '$requiredMarker'"
        }
    }
if ($coreFfiCargoToml.Contains('native-memory-binding = ["native-c-core"]')) {
        Add-ArchitectureError "clearra-core-ffi native-memory-binding must not depend on native-c-core"
    }
$ffiLib = Read-Text "crates/clearra-core-ffi/src/lib.rs"
foreach ($requiredMarker in @("pub mod memory", "ContractCoreContext", "NativeCoreContext", "MemoryBackendKind", "NativeScopeKind", "CoreContext", "SearchScope", "BatchScope", "ReleaseSignal")) {
        if ($ffiLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi lib must export M2 memory wrapper marker '$requiredMarker'"
        }
    }
$ffiMemoryAbi = Read-Text "crates/clearra-core-ffi/src/memory/memory_abi.rs"
foreach ($requiredMarker in @("CClrMemContext", "CClrScope", "CClrMemStatus", "CClrScopeKind", "CClrMemLeakReport")) {
        if ($ffiMemoryAbi -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi memory_abi.rs must own M2 ABI mirror marker '$requiredMarker'"
        }
    }
$ffiContractContext = Read-Text "crates/clearra-core-ffi/src/memory/contract_core_context.rs"
foreach ($requiredMarker in @("ContractCoreContext", "CoreLeakReport", "MemoryBackendKind::Contract", "contract_core_context_drop_records_release_signal", "Drop for ContractCoreContextInner", "Rust-side memory lifetime contract")) {
        if ($ffiContractContext -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi contract_core_context.rs must own M2 contract wrapper marker '$requiredMarker'"
        }
    }
$ffiMemoryBackendKind = Read-Text "crates/clearra-core-ffi/src/memory/memory_backend_kind.rs"
foreach ($requiredMarker in @("MemoryBackendKind", "Contract", "NativeSkeleton", "NativeBound", "memory_backend_kind_has_stable_labels")) {
        if ($ffiMemoryBackendKind -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi memory_backend_kind.rs must own M2 backend-kind marker '$requiredMarker'"
        }
    }
$ffiContext = Read-Text "crates/clearra-core-ffi/src/memory/core_context.rs"
foreach ($requiredMarker in @("Compatibility facade", "ContractCoreContext", "pub type CoreContext")) {
        if ($ffiContext -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi core_context.rs must remain a thin M2 facade marker '$requiredMarker'"
        }
    }
$ffiContractSearchScope = Read-Text "crates/clearra-core-ffi/src/memory/contract_search_scope.rs"
foreach ($requiredMarker in @("pub struct ContractSearchScope", "Drop for ContractSearchScope", "handle.release()")) {
        if ($ffiContractSearchScope -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi contract_search_scope.rs must own M2 RAII release marker '$requiredMarker'"
        }
    }
$ffiSearchScope = Read-Text "crates/clearra-core-ffi/src/memory/search_scope.rs"
foreach ($requiredMarker in @("Compatibility facade", "ContractSearchScope", "pub type SearchScope")) {
        if ($ffiSearchScope -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi search_scope.rs must remain a thin M2 facade marker '$requiredMarker'"
        }
    }
$ffiContractBatchScope = Read-Text "crates/clearra-core-ffi/src/memory/contract_batch_scope.rs"
foreach ($requiredMarker in @("pub struct ContractBatchScope", "Drop for ContractBatchScope", "handle.release()")) {
        if ($ffiContractBatchScope -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi contract_batch_scope.rs must own M2 RAII release marker '$requiredMarker'"
        }
    }
$ffiBatchScope = Read-Text "crates/clearra-core-ffi/src/memory/batch_scope.rs"
foreach ($requiredMarker in @("Compatibility facade", "ContractBatchScope", "pub type BatchScope")) {
        if ($ffiBatchScope -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi batch_scope.rs must remain a thin M2 facade marker '$requiredMarker'"
        }
    }
$ffiNativeBindings = Read-Text "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs"
foreach ($requiredMarker in @(
            "native_memory_binding_is_feature_gated",
            "unsafe extern `"C`"",
            "#[link(name = `"clearra_core`", kind = `"static`")]",
            "clr_mem_context_create",
            "clr_mem_context_release",
            "*mut *mut CClrMemContext",
            "clr_mem_context_leak_report",
            "clr_scope_create",
            "clr_scope_release",
            "NativeMemContextHandle",
            "NativeScopeHandle"
        )) {
        if (-not $ffiNativeBindings.Contains($requiredMarker)) {
            Add-ArchitectureError "clearra-core-ffi native_memory_bindings.rs must keep raw native memory ABI private marker '$requiredMarker'"
        }
    }
$ffiNativeMemoryError = Read-Text "crates/clearra-core-ffi/src/memory/native_memory_error.rs"
foreach ($requiredMarker in @("NativeMemoryError", "BindingUnavailable", "InvalidState", "from_status", "native_memory_error_maps_c_status", "native_memory_release_error_maps_to_diagnostic_material")) {
        if ($ffiNativeMemoryError -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi native_memory_error.rs must map C memory status marker '$requiredMarker'"
        }
    }
$ffiNativeContext = Read-Text "crates/clearra-core-ffi/src/memory/native_core_context.rs"
foreach ($requiredMarker in @("NativeCoreContext", "MemoryBackendKind::NativeSkeleton", "MemoryBackendKind::NativeBound", "NativeMemoryBindingUnavailable", "BindingUnavailable", "try_create", "leak_report", "release", "native_core_context_drop_releases_c_mem_context", "native_core_context_explicit_release_then_drop_does_not_double_free")) {
        if ($ffiNativeContext -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi native_core_context.rs must expose explicit native memory binding marker '$requiredMarker'"
        }
    }
$ffiNativeScope = Read-Text "crates/clearra-core-ffi/src/memory/native_scope.rs"
foreach ($requiredMarker in @("NativeScopeKind", "NativeSearchScope", "NativeBatchScope", "BorrowedNativeView", "CClrScopeKind", "native_search_scope_drop_releases_c_scope", "native_batch_scope_drop_releases_c_scope", "borrowed_view_cannot_escape_scope", "owned_snapshot_survives_scope_release")) {
        if ($ffiNativeScope -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi native_scope.rs must expose explicit native scope binding marker '$requiredMarker'"
        }
    }
$ffiNativeLeakReport = Read-Text "crates/clearra-core-ffi/src/memory/native_leak_report.rs"
foreach ($requiredMarker in @("NativeLeakReport", "NativeMemoryDiagnosticMaterial", "CClrMemLeakReport", "to_core_leak_report", "to_diagnostic_material", "native_memory_leak_report_maps_to_core_leak_report", "native_memory_leak_report_maps_to_diagnostic_material")) {
        if ($ffiNativeLeakReport -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi native_leak_report.rs must expose explicit native leak-report skeleton marker '$requiredMarker'"
        }
    }
$executorLib = Read-Text "crates/clearra-core-executor/src/lib.rs"
if ($executorLib -notlike "*pub mod memory*" -or $executorLib -notlike "*ScopeGuard*") {
        Add-ArchitectureError "clearra-core-executor must export M2 memory ScopeGuard"
    }
$scopeGuard = Read-Text "crates/clearra-core-executor/src/memory/scope_guard.rs"
foreach ($requiredMarker in @("ScopeGuard::search", "ScopeGuard::batch", "search_scope_guard_releases_on_drop", "batch_scope_guard_releases_on_drop")) {
        if ($scopeGuard -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-executor scope_guard.rs must verify M2 RAII guard marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M2 C Memory Scope", "scope-based pseudo-GC", "SearchScope", "BatchScope", "GpuTransferScope", "RAII guards", "ContractCoreContext", "NativeCoreContext", "MemoryBackendKind", "native_scope.rs", "native_leak_report.rs")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M2 memory scope marker '$requiredMarker'"
        }
    }
$memoryLifecycleDoc = Read-Text "docs/memory-lifecycle.md"
foreach ($requiredMarker in @("GPU Worker v0.1", "GpuMemoryTicket", "GpuWorkerResult", "memory_ticket_id", "fence_epoch", "release queue", "GpuTransferScope", "ContractCoreContext", "borrowed native views escaping")) {
        if ($memoryLifecycleDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/memory-lifecycle.md must document memory lifecycle marker '$requiredMarker'"
        }
    }
}
function Invoke-SearchProblemCanonicalModelValidation() {
foreach ($requiredPath in @(
        "crates/clearra-problem/src/query/pc_query.rs",
        "crates/clearra-problem/src/query/scenario_query.rs",
        "crates/clearra-problem/src/query/setup_query.rs",
        "crates/clearra-problem/src/query/setup_grouping.rs",
        "crates/clearra-problem/src/query/setup_hold_policy.rs",
        "crates/clearra-problem/src/query/setup_limits.rs",
        "crates/clearra-problem/src/query/setup_piece_budget.rs",
        "crates/clearra-problem/src/query/setup_probability_filter.rs",
        "crates/clearra-problem/src/query/setup_queue_input.rs",
        "crates/clearra-problem/src/query/build_query.rs",
        "crates/clearra-problem/src/query/spin_target_query.rs",
        "crates/clearra-problem/src/goal/spin_target_goal.rs",
        "crates/clearra-problem/src/goal/search_goal_request.rs",
        "crates/clearra-problem/src/preset/opening_preset.rs",
        "crates/clearra-problem/src/preset/scenario_preset.rs",
        "crates/clearra-problem/src/preset/setup_preset.rs",
        "crates/clearra-problem/src/preset/build_preset.rs",
        "crates/clearra-problem/src/compile/problem_compiler.rs",
        "crates/clearra-problem/src/compile/packing_problem_compiler.rs",
        "crates/clearra-problem/src/compile/spin_target_compiler.rs",
        "crates/clearra-problem/src/compile/compile_error.rs",
        "crates/clearra-problem/src/search_problem.rs",
        "crates/clearra-problem/src/search_problem_fields.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M3 SearchProblem canonical model required file is missing: $requiredPath"
        }
    }
$problemCargo = Read-Text "crates/clearra-problem/Cargo.toml"
foreach ($crateName in @("clearra-core-domain", "clearra-pc-graph", "clearra-profiles", "clearra-rules", "clearra-objectives", "clearra-supply")) {
        if (-not (Test-DependencyLine $problemCargo $crateName)) {
            Add-ArchitectureError "clearra-problem must depend on $crateName for the M3 SearchProblem canonical model"
        }
    }
Assert-CargoDoesNotDepend "crates/clearra-problem/Cargo.toml" @("clearra-scoring") "clearra-problem owns SpinTargetRequest query contracts without depending on scoring implementation"
Assert-ProductionImportAbsence "crates/clearra-problem/src" @("clearra_scoring", "SpinClassifier", "SpinTargetPredicate", "ScoreProfileObjectValidator", "CandidateScoreStats") "clearra-problem must not import scoring implementation; use SpinTargetRequest only"
$searchProblem = Read-Text "crates/clearra-problem/src/search_problem.rs"
foreach ($requiredMarker in @(
        "pub struct SearchProblem",
        "SearchProblemPreset",
        "board: SearchProblemBoard",
        "piece_window: PieceWindow",
        "exact_pieces: Option<usize>",
        "supply: SupplyProvenance",
        "piece_set: PieceSetProfile",
        "rule_profile: RuleProfileSelection",
        "kick_profile: KickProfile",
        "spawn_profile: SpawnProfile",
        "search_goal: SearchGoal",
        "pub fn search_goal(&self) -> &SearchGoal",
        "pub fn with_search_goal(mut self, search_goal: SearchGoal) -> Self",
        "exact_target_policy: ExactTargetPolicy",
        "count_policy: CountPolicy",
        "objective: ObjectivePolicy",
        "budget: SearchProblemBudget",
        "resource_budget: ResourceBudget",
        "backend_policy: BackendPolicy",
        "output_policy: SearchOutputPolicy",
        "replay_trace_policy: SearchReplayTracePolicy",
        "trace_policy: TracePolicy",
        "continuation_policy: ContinuationPolicy",
        "labels: Vec<String>",
        "SearchProblemPreset::Setup",
        "SearchProblemPreset::Build"
    )) {
        if ($searchProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SearchProblem must own M3 executor-facing contract marker '$requiredMarker'"
        }
    }
$searchGoalRequest = Read-Text "crates/clearra-problem/src/goal/search_goal_request.rs"
foreach ($requiredMarker in @(
        "pub enum SearchGoal",
        "ClearToEmpty",
        "BuildTemplate(BuildTemplateGoal)",
        "SpinTarget(SpinTargetRequest)",
        "Composite(CompositeGoal)"
    )) {
        if ($searchGoalRequest -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SearchGoal request contract must own O spin-target goal marker '$requiredMarker'"
        }
    }
$spinTargetQuery = Read-Text "crates/clearra-problem/src/query/spin_target_query.rs"
foreach ($requiredMarker in @(
        "PercentGoalSpin",
        "SetupGoalSpin",
        "PcThenSpin",
        "percent_goal_spin",
        "setup_goal_spin",
        "pc_then_spin"
    )) {
        if ($spinTargetQuery -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SpinTargetQuery must own product query mapping marker '$requiredMarker'"
        }
    }
$spinTargetCompiler = Read-Text "crates/clearra-problem/src/compile/spin_target_compiler.rs"
foreach ($requiredMarker in @(
        "percent_spin_target_query_compiles_to_search_problem",
        "setup_spin_target_query_preserves_threshold",
        "pc_then_spin_compiles_to_composite_goal",
        "spin_target_query_requires_score_profile_when_profile_specific",
        "ProfileSpecificSpinTargetRequiresScoreProfile"
    )) {
        if ($spinTargetCompiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SpinTargetCompiler must keep O spin-target compile contract marker '$requiredMarker'"
        }
    }
$searchProblemFields = Read-Text "crates/clearra-problem/src/search_problem_fields.rs"
foreach ($requiredMarker in @(
        "pub struct SearchProblemBoard",
        "pub struct SearchProblemId",
        "pub enum SearchProblemKind",
        "piece_source::PieceSource",
        "hold_automaton::HoldAutomatonState",
        "pub struct KickProfile",
        "pub enum ExactTargetPolicy",
        "pub struct SupplyProvenance",
        "pub struct RuleProfileSelection",
        "pub struct SearchProblemBudget",
        "pub enum SearchOutputPolicy",
        "pub struct SearchReplayTracePolicy",
        "pub struct ContinuationPolicy"
    )) {
        if ($searchProblemFields -notlike "*$requiredMarker*") {
            Add-ArchitectureError "search_problem_fields.rs must expose M3 SearchProblem field type marker '$requiredMarker'"
        }
    }
$scenarioQuery = Read-Text "crates/clearra-problem/src/query/scenario_query.rs"
foreach ($requiredMarker in @(
        "SetupPreset",
        "BuildPreset",
        "setup_query: Option<SetupSearchQuery>",
        "build_query: Option<BuildQuery>",
        "pub fn setup_preset",
        "pub fn build_preset"
    )) {
        if ($scenarioQuery -notlike "*$requiredMarker*") {
            Add-ArchitectureError "ScenarioQuery must carry setup/build canonical preset ownership marker '$requiredMarker'"
        }
    }
$setupQuery = Read-Text "crates/clearra-problem/src/query/setup_query.rs"
foreach ($requiredMarker in @("pub struct SetupSearchQuery", "board_size:", "target:", "queue:", "hold_policy:", "piece_budget:", "probability_filter:", "grouping_mode:", "limits:")) {
        if ($setupQuery -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SetupSearchQuery must own the canonical setup query contract marker '$requiredMarker'"
        }
    }
$buildQuery = Read-Text "crates/clearra-problem/src/query/build_query.rs"
foreach ($requiredMarker in @("pub struct BuildQuery", "BuildTemplateBridge", "BuildProblemLimits", "coverage_bridge")) {
        if ($buildQuery -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BuildQuery must own the M3 build coverage bridge marker '$requiredMarker'"
        }
    }
$problemCompiler = Read-Text "crates/clearra-problem/src/compile/problem_compiler.rs"
foreach ($requiredMarker in @("compile_opening_pc", "compile_scenario_pc", "compile_setup", "compile_build", "compile_continuation_token", "OpeningPreset::try_from_pc_query", "ScenarioPreset::from_query", "SetupPostPcPreset::from_query", "BuildPreset::from_query", "SearchProblem::new", "continue_token_compiles_to_search_problem", "pc_target_remains_label_not_core_success_condition")) {
        if ($problemCompiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "ProblemCompiler must lower all M3 presets into SearchProblem marker '$requiredMarker'"
        }
    }
$packingCompiler = Read-Text "crates/clearra-problem/src/compile/packing_problem_compiler.rs"
foreach ($requiredMarker in @("PackingProblemKind::OpeningPc", "PackingProblemKind::ScenarioPc", "PackingProblemKind::Setup", "PackingProblemKind::Build", "PackingProblemCompiler::compile")) {
        if ($packingCompiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PackingProblemCompiler must bridge M3 SearchProblem presets into packing specs marker '$requiredMarker'"
        }
    }
$setupSearchQueryFile = "crates/clearra-setup-search/src/query/mod.rs"
$setupSearchQuery = Read-Text $setupSearchQueryFile
if ($setupSearchQuery -notlike "*pub use clearra_problem::query::setup_query*") {
    Add-ArchitectureError "$setupSearchQueryFile must re-export the canonical M3 setup query types from clearra-problem"
}
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M3 SearchProblem Canonical Model", "query/setup_query.rs", "query/build_query.rs", "preset/setup_preset.rs", "preset/build_preset.rs", "visible/search height", "queue/hold/bag supply provenance", "replay/trace policy", "continuation policy", "setup post-PC -> post-PC board/queue/hold", "continuation token -> canonical opening/scenario query")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M3 SearchProblem canonical model marker '$requiredMarker'"
        }
    }
}
function Invoke-CCompactProblemDescriptorValidation() {
foreach ($requiredPath in @(
        "core-c/include/clr_problem.h",
        "core-c/include/clr_board.h",
        "core-c/include/clr_piece.h",
        "core-c/include/clr_rules.h",
        "core-c/include/clr_supply.h",
        "core-c/src/problem/packing_problem.c",
        "core-c/src/problem/buildup_problem.c",
        "core-c/src/problem/problem_defaults.c",
        "core-c/tests/problem_descriptor_tests.c",
        "crates/clearra-core-ffi/src/problem/mod.rs",
        "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs",
        "crates/clearra-core-ffi/src/problem/buildup_problem_builder.rs",
        "crates/clearra-core-ffi/src/problem/ffi_problem_error.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M4 C compact problem descriptor required file is missing: $requiredPath"
        }
    }
$clearraCoreHeader = Read-Text "core-c/include/clearra_core.h"
foreach ($requiredMarker in @("#include `"clr_problem.h`"")) {
        if ($clearraCoreHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra_core.h must expose M4 compact problem descriptor marker '$requiredMarker'"
        }
    }
$problemHeader = Read-Text "core-c/include/clr_problem.h"
foreach ($requiredMarker in @(
        "typedef struct clr_packing_problem",
        "clr_board_descriptor board",
        "clr_piece_window_descriptor piece_window",
        "clr_piece_multiset_window piece_multiset_window",
        "clr_piece_source_descriptor piece_source",
        "clr_rule_profile_descriptor rule",
        "clr_problem_budget budget",
        "clr_backend_request backend",
        "uint32_t goal",
        "clr_packing_problem_zero",
        "clr_packing_problem_is_valid",
        "clr_buildup_problem_from_packing"
    )) {
        if ($problemHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clr_problem.h must define M4 compact descriptor marker '$requiredMarker'"
        }
    }
foreach ($headerAndMarker in @(
        @("core-c/include/clr_board.h", "initial_mask"),
        @("core-c/include/clr_piece.h", "clr_piece_window_descriptor"),
        @("core-c/include/clr_rules.h", "CLR_KICK_SRS_PLUS_180"),
        @("core-c/include/clr_supply.h", "CLR_QUEUE_VIEW_CAPACITY")
    )) {
        $contents = Read-Text $headerAndMarker[0]
        if ($contents -notlike "*$($headerAndMarker[1])*") {
            Add-ArchitectureError "$($headerAndMarker[0]) must define M4 marker '$($headerAndMarker[1])'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @("src/problem/packing_problem.c", "src/problem/buildup_problem.c", "src/problem/problem_defaults.c", "problem_descriptor_tests")) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must compile M4 compact descriptor marker '$requiredMarker'"
        }
    }
$ffiProblemMod = @(
        Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/problem_descriptors.rs"
        Read-Text "crates/clearra-core-ffi/src/supply/piece_source_descriptor.rs"
    ) -join "`n"
foreach ($requiredMarker in @("pub struct CPackingProblem", "CBoardDescriptor", "CPieceWindowDescriptor", "CPieceMultisetWindow", "CPieceSourceDescriptor", "CRuleProfileDescriptor", "CProblemBudget", "CBackendRequest", "CBuildUpProblem")) {
        if ($ffiProblemMod -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi problem module must mirror M4 C layout marker '$requiredMarker'"
        }
    }
$packingBuilder = @(
        Read-Text "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_board_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_supply_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_rule_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_budget_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_backend_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_goal_descriptor_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/ffi_problem_error.rs"
        Read-Text "crates/clearra-core-ffi/src/supply/supply_descriptor_compiler.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
        "pub struct CPackingProblemBuilder",
        "from_search_problem",
        "board_descriptor",
        "SupplyDescriptorCompiler::compile",
        "RuleDescriptorCompiler::compile",
        "budget_descriptor",
        "backend_descriptor",
        "goal_code",
        "count_policy_code",
        "objective_code",
        "packing_problem_builder_preserves_board_descriptor",
        "packing_problem_uses_piece_multiset_not_fixed_order",
        "packing_problem_builder_preserves_rule_profile",
        "packing_problem_builder_rejects_unsupported_board",
        "packing_problem_builder_rejects_unverified_kick_profile",
        "QueueTruncatedButExactNeeded"
    )) {
        if ($packingBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CPackingProblemBuilder must convert M4 SearchProblem fields marker '$requiredMarker'"
        }
    }
$cacheIdentity = @(
        Read-Text "core-c/src/cache/cache_identity.h"
        Read-Text "core-c/src/cache/cache_identity.c"
        Read-Text "core-c/tests/cache_identity_tests.c"
    ) -join "`n"
foreach ($requiredMarker in @(
        "clearra_cache_identity_from_packing_problem",
        "piece_definition_id_fingerprint",
        "piece_area_multiset_fingerprint",
        "piece_source_pattern_id_from_problem",
        "cache_identity_includes_supply_rule_piece_goal"
    )) {
        if ($cacheIdentity -notlike "*$requiredMarker*") {
            Add-ArchitectureError "C cache identity must include M4 compact descriptor identity marker '$requiredMarker'"
        }
    }
$buildupBuilder = Read-Text "crates/clearra-core-ffi/src/problem/buildup_problem_builder.rs"
foreach ($requiredMarker in @("pub struct CBuildUpProblemBuilder", "from_search_problem", "CPackingProblemBuilder::from_search_problem")) {
        if ($buildupBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CBuildUpProblemBuilder must wrap M4 compact packing descriptor marker '$requiredMarker'"
        }
    }
$coreExecutor = Read-Text "crates/clearra-core-executor/src/core_executor.rs"
$packingRunner = Read-Text "crates/clearra-core-executor/src/packing/packing_runner.rs"
$packingProblemPreparer = Read-Text "crates/clearra-core-executor/src/packing/packing_problem_preparer.rs"
$pcService = Get-PcServiceValidationSurface
foreach ($requiredMarker in @("CPackingProblemBuilder::from_search_problem", "compact_problem_descriptor", "clr_packing_problem", "compact_rule_profile_id", "compact_kick_profile_id", "compact_backend_request")) {
        $descriptorSurface = "$coreExecutor`n$packingRunner`n$packingProblemPreparer`n$pcService"
        if ($descriptorSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CoreExecutor service path must route through M4 compact descriptor marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M4 C Compact Problem Descriptor", "M3 SearchProblem to C compact descriptor mapping table", "clr_packing_problem", "clr_buildup_problem", "board width and height", "initial board mask", "queue view", "rule profile id", "effective kick profile id", "backend request", "queue truncated", "exact needed", "Unsupported board descriptors", "Board64")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M4 compact descriptor marker '$requiredMarker'"
        }
    }
}
