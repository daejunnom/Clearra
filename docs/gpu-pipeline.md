# GPU, WebGPU, And PostProcess Backends

Clearra exposes only implementations that execute real work. A backend outcome
is one of:

- `Connected`: the backend ran and its result carries an accepted trust state;
- `Unavailable`: no result was produced and a reason is present;
- `RejectedMismatch`: compute ran, but host confirmation rejected the result.

Preview, scaffold, placeholder, fixture fallback, and CPU code labeled as GPU
are not product states.

## Current Backend Matrix

| Surface | Default state | Exact result policy |
| --- | --- | --- |
| Native C search GPU | Unavailable: `native_gpu_backend_not_built` | Produces no candidates |
| Windows WebGPU geometry exact cover | Connected when a hardware adapter/device is available | Reviewed deterministic kernel + exact host reduce + per-dispatch CPU transition samples |
| Native hybrid search | Connected through `clearra-core-executor` + `clearra-webgpu` | Prepared GPU-only, otherwise CPU-only; never merges two searches |
| WebGPU PatternBitSet union | Connected when an adapter/device is available | Full CPU union confirmation required |
| PostProcess GPU coverage union | Connected through `clearra-webgpu` | Exact only after deterministic CPU match |
| Generic/custom GPU | Unsupported | No default ABI or runtime path |

The C ABI native GPU worker remains unavailable in the default build. Native
desktop search instead registers the reviewed WebGPU exact-cover executor
through the stable `webgpu-search` feature. CUDA and OpenCL entries are not
registered and no placeholder kernels ship.

## Native Search GPU

`ClearraGpuPackingBatchDescriptor` is the stable request ABI. Its packing
source of truth is:

- `piece_source_id`;
- `piece_multiset_window`;
- `pattern_universe_id`;
- `pattern_weight_model_id`.

ABI v5 removes the former ordered piece preview array. The descriptor is a
112-byte multiset/PieceSource request and cannot be mistaken for a fixed queue.

The descriptor does not make a backend available. The default native adapter
returns `CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE`, emits zero candidates,
and sets `can_source_exact_probability=0`.

The native C fallback/readback adapter is compiled only with `BUILD_TESTING` as
an equivalence checkpoint. Product fallback is selected and executed by
`clearra-core-executor`: it discards any incomplete GPU state, runs the CPU
Geometry Skeleton Exact Cover backend, and reports the original GPU reason and
the actual CPU backend separately. A disabled fallback returns the unavailable
error without CPU work.

## WebGPU Compute

The search path runs `embedded_geometry_exact_cover.wgsl`. It performs layered
BFS over the same canonical skeleton exact-cover state used by CPU Algorithm X,
then exact-reduces states on the host. Hashes only select buckets; occupied
cells, packed counts, depth, and predecessor edges are compared exactly.
Queue, hold, score, spin, Fumen, and render state never enter a GPU batch.

At every dispatch the host independently expands deterministic first, middle,
and last parent samples with the CPU transition reference and compares exact
children and row ids. All dispatch samples plus exact host reduction are needed
for `TrustedCpuSampleConfirmed`. A mismatch becomes
`RejectedTrustMismatch`; it cannot silently fall back as GPU success. This
trust validation is bounded and does not run the complete CPU search again.

`clearra-webgpu` also owns dense PatternBitSet word union. That implementation:

1. validates a rectangular non-empty batch;
2. requests an adapter and device;
3. checks storage-buffer limits;
4. compiles the embedded reviewed shader;
5. dispatches compute workgroups;
6. reads the result back;
7. computes the complete CPU reference union;
8. returns `Connected` only when both outputs match.

The shader is `embedded_pattern_bitset_union.wgsl`. User-provided WGSL and
runtime shader injection are rejected. Reports include shader version/hash,
adapter label or redaction, device limits, CPU confirmation, fallback backend,
and unavailable or mismatch evidence.

`WebGpuBatchOutcome` has exactly these compute outcomes:

- `Connected(WebGpuConnectedResult)`;
- `Unavailable(WebGpuUnavailableResult)`;
- `RejectedMismatch(WebGpuRejectedMismatch)`.

Only `DeterministicReferenceMatched` with `cpu_confirmed=true` can claim exact.

## PostProcess GPU

Search GPU and PostProcess GPU are separate job types. PostProcess uses the
connected WebGPU batch only for PatternBitSet union. It does not accept packing
candidates, alter PC coverage rows, or rewrite the search backend report.

`PostGpuCapabilityState` is `Connected`, `Unavailable`, or
`RejectedMismatch`. Trusted results are created inside
`PostProcessGpuBackend`; callers cannot construct a public trusted result.

An unavailable PostProcess GPU may fall back to CPU only when
`BackendFallbackPolicy::AllowWithDiagnostic` is selected. The result remains an
`Unavailable` GPU outcome with `fallback_used=true`, `fallback_backend=cpu`,
and the original reason. A mismatch never falls back silently.

## Exactness And Trust

Internal worker states may represent an unconfirmed result while readback and
confirmation are in progress. Such a state is never a final exact source.

Allowed exact sources:

- CPU;
- WebGPU with reviewed deterministic shader identity, exact host reduction, and
  complete per-dispatch CPU sample confirmation;
- a future native GPU result after complete CPU confirmation.

Forbidden exact sources:

- unavailable backend;
- fallback label without the fallback result's own CPU contract;
- unconfirmed GPU result;
- rejected mismatch;
- candidate hash without exact payload comparison.

## Lifetime Contract

Adapter/device creation and shader/pipeline compilation are process-lifetime
resources. A process-global device context is retained per selected adapter;
sequential and concurrent jobs share its `Device`, `Queue`, and compute
pipeline. Mutable geometry buffers and frontier/readback scratch remain
session-owned and are recycled only after a job releases them. Desktop starts
this initialization in the background as soon as the user selects GPU or
hybrid; an in-flight warmup is shared with an immediate Run request. Small
`auto` and CPU-only requests do not wake the GPU. A one-shot explicit-GPU CLI
process still pays one physical cold initialization.

Cold context creation is not serialized ahead of independent host work. The
runtime overlaps it with supply materialization, batch planning, and the CPU
exact-reference/confirmation path where possible. Background warmup prepares
only the reviewed embedded pipeline; it never runs a fixture candidate or
synthesizes a search result. Geometry-specific uploads and the first real
dispatch remain attached to the real batch and are reported separately, so a
one-shot process never claims that unavoidable driver work disappeared.

Warmup and execution use the same context cache. Capability probing cannot
create a second device for execution, and concurrent requests cannot create a
second device merely because the first session is checked out. Reports expose
the real initialization duration and whether an existing session/context was
reused.

The native unavailable worker still validates the memory boundary. Requests
carry `memory_ticket_id`, `fence_epoch`, `scope_epoch`, and `byte_budget`.
`gpu_worker_scheduler_bridge.c` registers and releases the transfer record only
after the safe fence epoch. It does not synthesize candidates when the worker
is unavailable.

## Verification

- `cargo test -p clearra-webgpu tests::webgpu_backend_runs_real_batch -- --exact`
  executes the reviewed shader and checks the CPU-confirmed result.
- `cargo test -p clearra-postprocess-gpu` checks stable outcomes, visible
  fallback reasons, and exactness gating.
- CTest checks native unavailable behavior, explicit CPU fallback, memory
  tickets/fences, and host exact-confirm helpers.
- Architecture validation rejects placeholder shader files, portable GPU
  labels, preview capability states, and unregistered native API kinds.
