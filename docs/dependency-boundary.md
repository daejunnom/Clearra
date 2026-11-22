# Dependency Boundary

Clearra routes every product request through one typed boundary:

`CLI / GUI / WASM Command Runtime / Discord adapter -> AppRequest -> clearra-app -> validated SearchProblem -> clearra-core-executor -> backend evidence -> coverage and objective reducers -> replay / postprocess / output -> AppResponse`.

The executor dispatches to the existing Rust CPU kernels or, with
`native-c-core`, the typed FFI adapter for native C. These paths do not
currently support every identical request. The
[host contract](research/v090-rust-host-contract-2026-11-20.md) distinguishes
product service coverage from kernel availability. Changing backend does not
change requested result meaning, including exactness and resource outcomes.

GPU search and GPU postprocess have been retired. Explicit `gpu` and `hybrid`
product requests return `webgpu_backend_retired` without CPU fallback; current
builds have no WebGPU crate or runtime dependency. See [GPU retirement](gpu-pipeline.md).

## Owners

- `clearra-piece-registry` and `clearra-rules` own Rust shape and ordered kick
  semantics; `clearra-supply` owns source, occurrence and hold transitions.
- The executor owns active frame, catalog, physical state, temporary workspace
  and evidence emission. Geometry membership, Boolean reachability and full
  execution evidence are different claims, even when they use the same mask.
- `clearra-coverage` and `clearra-objectives` own coverage and objective policy;
  replay and postprocess own interpretation and presentation of physical evidence.
- `clearra-core-ffi` owns native ABI representation, status mapping and lifetime
  adapters, not the Rust rule or replay identity. The native C implementation
  owns its current board/search/GPU and memory operations while the corresponding
  native feature routes to it. Test-only references do not become product owners.

The [operation, equivalence and ABI contract](research/v090-ownership-and-abi-contract.md)
indexes the actual owners, per-operation key and lifetime, and conditions for
retiring a C implementation without changing the product or external ABI.

## Allowed Edges

- `clearra-cli -> clearra-app`
- `clearra-app -> clearra-validation`
- `clearra-app -> clearra-problem`
- `clearra-app -> clearra-core-executor`
- `clearra-app -> clearra-postprocess`
- `clearra-app -> clearra-output`
- `clearra-core-executor -> clearra-geometry / clearra-piece-registry / clearra-rules / clearra-supply`
- `clearra-core-executor -> clearra-core-ffi -> core-c`
- `clearra-coverage -> clearra-core-domain`
- `clearra-objectives -> clearra-coverage`
- `clearra-postprocess -> clearra-replay`
- `clearra-postprocess -> clearra-spin`
- `clearra-postprocess -> clearra-scoring`
- `clearra-output -> clearra-replay`
- `clearra-output -> clearra-fumen`
- `clearra-output -> clearra-render`

## Forbidden Edges

- `clearra-cli -> clearra-core-ffi`
- `clearra-cli -> core-c`
- `clearra-gui-host -> clearra-cli`
- `clearra-gui-host -> core-c raw pointer`
- `clearra-coverage -> clearra-scoring`
- `clearra-spin -> clearra-scoring`
- `clearra-scoring -> core-c raw binding`
- `clearra-postprocess -> clearra-core-executor`
- `clearra-render -> clearra-core-executor`
- `clearra-fumen -> clearra-core-executor`
- `clearra-core-executor -> clearra-render`
- `clearra-core-executor -> clearra-fumen codec internals`
- `clearra-core-executor -> clearra-scoring` at runtime

`clearra-core-executor` test support may use spin-target fixtures, but product
execution ends at BuildVariant, CoverageRow, and replay seed.
`clearra-app` hands that seed to `clearra-postprocess`, which owns scoring and spin interpretation.

## Required Gates

- `architecture_validation_rejects_cli_to_core_ffi`
- `architecture_validation_rejects_gui_to_cli`
- `architecture_validation_rejects_render_to_solver`
- `architecture_validation_rejects_fumen_to_solver`
- `architecture_validation_rejects_coverage_to_scoring`
- `architecture_validation_rejects_spin_to_scoring`
- `architecture_validation_rejects_core_executor_runtime_scoring`

Temporary boundary exceptions are forbidden. New product features must add typed contracts instead of weakening these dependency rules.
