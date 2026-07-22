# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-RustFfiSafetyTestsContractValidation() {
$coreFfiManifest = Read-Text "crates/clearra-core-ffi/Cargo.toml"
foreach ($requiredMarker in @(
            "default = []",
            "native-memory-binding = []",
            'native-c-core = ["native-memory-binding"]'
        )) {
        if (-not $coreFfiManifest.Contains($requiredMarker)) {
            Add-ArchitectureError "T2 Rust FFI safety feature gate must keep marker '$requiredMarker'"
        }
    }
if ($coreFfiManifest.Contains('native-memory-binding = ["native-c-core"]')) {
        Add-ArchitectureError "T2 Rust FFI safety requires native-c-core to depend on native-memory-binding, not the reverse"
    }
$nativeBindings = Read-Text "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs"
$nativeContext = Read-Text "crates/clearra-core-ffi/src/memory/native_core_context.rs"
$nativeScope = Read-Text "crates/clearra-core-ffi/src/memory/native_scope.rs"
$nativeLeakReport = Read-Text "crates/clearra-core-ffi/src/memory/native_leak_report.rs"
$buildVariantView = Read-Text "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
$kickEvidenceView = Read-Text "crates/clearra-core-ffi/src/buildup/kick_evidence_view.rs"
$nativeMod = Read-Text "crates/clearra-core-ffi/src/native/mod.rs"
$securityDiagnostics = Read-Text "crates/clearra-validation/src/validators/security_diagnostic_gate.rs"
$diagnosticCodeStrings = Read-Text "crates/clearra-validation/src/diagnostic/diagnostic_code_string.rs"
foreach ($requiredMarker in @(
            "native_memory_binding_is_feature_gated",
            "BindingUnavailable",
            "#[cfg(feature = `"native-memory-binding`")]",
            "#[cfg(not(feature = `"native-memory-binding`"))]",
            "clr_mem_context_release(context: *mut *mut CClrMemContext)",
            "context_release(handle: NativeMemContextHandle)",
            "NativeMemContextHandle",
            "NativeScopeHandle"
        )) {
        if (-not $nativeBindings.Contains($requiredMarker)) {
            Add-ArchitectureError "native_memory_bindings.rs must keep T2 feature-gated native binding marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "NativeCoreContext",
            "handle: Option<NativeMemContextHandle>",
            "native_core_context_default_build_returns_binding_unavailable",
            "native_core_context_drop_releases_c_mem_context",
            "native_core_context_explicit_release_then_drop_does_not_double_free",
            "NativeCoreContext::create().expect_err",
            "BindingUnavailable",
            "impl Drop for NativeCoreContext",
            "self.handle.take()",
            "context_release(handle)"
        )) {
        if (-not $nativeContext.Contains($requiredMarker)) {
            Add-ArchitectureError "native_core_context.rs must keep T2 RAII/default-unavailable marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "NativeSearchScope",
            "NativeBatchScope",
            "BorrowedNativeView",
            "PhantomData<&'scope ()>",
            "PhantomData<&'ctx NativeCoreContext>",
            "native_search_scope_drop_releases_c_scope",
            "native_batch_scope_drop_releases_c_scope",
            "borrowed_view_cannot_escape_scope",
            "owned_snapshot_survives_scope_release",
            "impl Drop for NativeSearchScope",
            "impl Drop for NativeBatchScope"
        )) {
        if (-not $nativeScope.Contains($requiredMarker)) {
            Add-ArchitectureError "native_scope.rs must keep T2 scope lifetime marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "NativeLeakReport",
            "NativeMemoryDiagnosticMaterial",
            "to_diagnostic_material",
            "native_memory_leak_report_maps_to_diagnostic_material",
            "live_scopes",
            "live_allocations",
            "live_gpu_buffers",
            "pending_release_queue",
            "pending_gpu_buffer_releases",
            "double_releases",
            "canary_failures",
            "poison_detections"
        )) {
        if (-not $nativeLeakReport.Contains($requiredMarker)) {
            Add-ArchitectureError "native_leak_report.rs must keep T2 diagnostic material marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
            "C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
            "16"
        )) {
        if (-not $nativeMod.Contains($requiredMarker)) {
            Add-ArchitectureError "native/mod.rs must keep T2 C ABI max mirror marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "CBuildVariantViewError",
            "MissingKickEvidencePointer",
            "KickEvidenceCountExceeded",
            "if kick_evidence_count > C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
            "core::ptr::NonNull::new",
            "from_raw_parts",
            ".to_vec()",
            "ffi_build_variant_rejects_kick_evidence_count_above_c_limit",
            "ffi_build_variant_rejects_missing_kick_evidence_pointer",
            "ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit",
            "ffi_build_variant_copies_kick_evidence_to_owned_vec",
            "ffi_build_variant_preserves_hold_branch_kind"
        )) {
        if (-not $buildVariantView.Contains($requiredMarker)) {
            Add-ArchitectureError "build_variant_view.rs must keep T2 pointer/count safety marker '$requiredMarker'"
        }
    }
if ($buildVariantView.IndexOf("if kick_evidence_count > C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT") -gt
        $buildVariantView.IndexOf("core::ptr::NonNull::new")) {
        Add-ArchitectureError "T2 malformed pointer/count must be rejected before pointer validation or deref"
    }
if ($buildVariantView.IndexOf("core::ptr::NonNull::new") -lt
        $buildVariantView.IndexOf("from_raw_parts")) {
        # Expected order: NonNull null check before from_raw_parts.
    } else {
        Add-ArchitectureError "T2 kick evidence pointer must be null-checked before from_raw_parts"
    }
foreach ($requiredMarker in @(
            "CKickEvidenceView",
            "#[repr(C)]",
            "first_success",
            "kick_index"
        )) {
        if (-not $kickEvidenceView.Contains($requiredMarker)) {
            Add-ArchitectureError "kick_evidence_view.rs must keep T2 ABI mirror marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "ECoreFfiBufferBounds",
            "ECoreInvalidNativeView",
            "EKickEvidenceBufferExhausted",
            "KickEvidenceCountExceeded",
            "MissingKickEvidencePointer"
        )) {
        if (-not $securityDiagnostics.Contains($requiredMarker)) {
            Add-ArchitectureError "security diagnostic gate must map T2 FFI safety marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "E_CORE_FFI_BUFFER_BOUNDS",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_KICK_EVIDENCE_BUFFER_EXHAUSTED"
        )) {
        if (-not $diagnosticCodeStrings.Contains($requiredMarker)) {
            Add-ArchitectureError "diagnostic_code_string.rs must expose T2 stable diagnostic code '$requiredMarker'"
        }
    }
$memoryDoc = Read-Text "docs/memory-lifecycle.md"
foreach ($requiredMarker in @(
            "native_memory_binding_is_feature_gated",
            "native_core_context_drop_releases_c_mem_context",
            "native_search_scope_drop_releases_c_scope",
            "native_batch_scope_drop_releases_c_scope",
            "owned_snapshot_survives_scope_release",
            "borrowed_view_cannot_escape_scope"
        )) {
        if (-not $memoryDoc.Contains($requiredMarker)) {
            Add-ArchitectureError "docs/memory-lifecycle.md must document T2 memory safety marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "T2 Rust FFI Safety Tests",
            "default build keeps native binding unavailable",
            "native-memory-binding feature uses RAII",
            "no borrowed view escapes scope",
            "malformed pointer/count rejected before deref"
        )) {
        if (-not $architectureDoc.Contains($requiredMarker)) {
            Add-ArchitectureError "docs/architecture.md must document T2 marker '$requiredMarker'"
        }
    }
}
