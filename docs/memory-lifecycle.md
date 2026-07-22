# Memory Lifecycle

Clearra uses scoped memory ownership for native and future GPU-assisted work.
Search, batch, worker, and GPU-transfer buffers must belong to an explicit
scope. Release can be immediate only when the owning scope and all dependent
epochs are safe.

## GPU Worker v0.1

The native GPU worker ABI is a lifetime contract. The default product build
reports that backend as unavailable; CPU test models are not registered or
reported as GPU execution. Its memory contract is intentionally strict:

- every `GpuWorkerRequest` carries a `GpuMemoryTicket`;
- every `GpuWorkerResult` is created from a `GpuMemoryTicket`;
- ticket id, scope epoch, and byte budget must be nonzero;
- every C worker request and result carries `request_id`, `memory_ticket_id`,
  `fence_epoch`, `cpu_confirm_required`, `scope_epoch`, and `byte_budget`;
- GPU buffer release before the fence epoch is deferred through the release
  queue;
- release queue draining happens only after the relevant fence epoch is safe.

## Scopes

`SearchScope` owns search-wide allocations. `BatchScope` owns per-batch
scratch. `GpuTransferScope` owns GPU transfer buffers and records the fence
epoch that protects them from early release.

`ClrMemContext` release uses the pointer-to-pointer public API
`clr_mem_context_release(ClrMemContext **context)`. `context == NULL` is
`CLR_MEM_INVALID_ARGUMENT`; `*context == NULL` is `CLR_MEM_DOUBLE_RELEASE`.
Successful release frees the owned context and always writes `*context = NULL`,
so double release never reads a tombstone from freed memory.

Batch success calls `clr_scope_release(batch_scope)`. Budget exhaustion or
cancelled work calls `clr_scope_abort(batch_scope)`. Releasing a `SearchScope`
also releases active child batch/worker/GPU-transfer scopes in the same memory
context. If a GPU fence has not reached the safe epoch, the buffer release is
deferred and remains visible in the leak report until
`clr_release_queue_drain` observes a safe epoch.

`ClrMemLeakReport` is the release-before-output snapshot. Diagnostic and output
material must preserve `live_scopes`, `live_allocations`, `live_gpu_buffers`,
`pending_release_queue`, `pending_gpu_buffer_releases`, `double_releases`,
`canary_failures`, and `poison_detections`.

Rust-side `ContractCoreContext` is a contract/mock wrapper. Native memory
binding skeletons are separate from the contract wrapper so callers can tell
whether a test is exercising Rust-side lifetime accounting or actual C memory
APIs.

## Native Memory Binding v0.2

`clearra-core-ffi` owns the only Rust unsafe boundary for native memory. The
binding is feature gated by `native-memory-binding`, which also enables the
runner-owned `native-c-core` link policy. Default builds keep
`NativeCoreContext` in `NativeSkeleton` mode and return
`BindingUnavailable`.

When `native-memory-binding` is enabled, raw `extern "C"` declarations and raw
C pointers are private to
`crates/clearra-core-ffi/src/memory/native_memory_bindings.rs`. Public callers
only receive RAII wrappers:

- `NativeCoreContext`
- `NativeSearchScope`
- `NativeBatchScope`
- `NativeLeakReport`
- `NativeMemoryDiagnosticMaterial`

The public contract requires `Drop` to release C scopes/context, explicit
double release to return an error, leak reports to map to diagnostic material,
borrowed native views to be lifetime-bound to their owning scope, and owned
snapshots to survive scope release.

Required Rust memory binding contract tests:

- `native_memory_binding_is_feature_gated`
- `native_core_context_drop_releases_c_mem_context`
- `native_core_context_explicit_release_then_drop_does_not_double_free`
- `native_search_scope_drop_releases_c_scope`
- `native_batch_scope_drop_releases_c_scope`
- `native_memory_leak_report_maps_to_diagnostic_material`
- `native_memory_release_error_maps_to_diagnostic_material`
- `owned_snapshot_survives_scope_release`
- `borrowed_view_cannot_escape_scope`
- `unsafe_allowed_only_in_core_ffi_raw`

Required C memory/GPU lifetime contract tests include
`memory_context_release_nulls_pointer`,
`memory_context_double_release_does_not_deref_freed_memory`,
`gpu_buffer_release_before_fence_deferred`, and
`release_queue_drain_after_epoch_releases_gpu_buffer`.

## Forbidden

- GPU worker result without a memory ticket;
- GPU buffer release without a fence epoch;
- CPU fallback hidden as a successful GPU result;
- exact probability sourced from `GpuComputedUnconfirmed`;
- borrowed native views escaping the scope that owns their buffer.
