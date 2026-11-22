# Retired GPU Backends

The product search and postprocess implementation is CPU-only. Native CLI,
Desktop and Discord run the Rust CPU runtime; Web runs that runtime compiled
to WebAssembly. Automatic selection and an explicit CPU request use the CPU
backend with the requested worker, evidence, exactness and memory contracts.

## Current request contract

An explicit `gpu` or `hybrid` search request is unsupported with
`webgpu_backend_retired`, including requests that allow backend fallback.
It does not allocate an adapter, enumerate hardware, start a GPU warmup, or
execute a CPU search on behalf of the retired backend. The browser host no
longer probes `navigator.gpu`, and the UI advertises only Auto and CPU.

The `clearra-webgpu` and `clearra-postprocess-gpu` crates, embedded shaders,
WebGPU native/distributed producers, GPU postprocess union adapter, associated
features and GPU-only vendor patches have been removed. CPU coverage reduction
and postprocess scoring retain their existing implementations.

## Compatibility and independent reference

Existing backend enums, result/report fields and WASM ABI discriminants may
still name GPU or WebGPU so persisted requests and reports can be decoded.
Those names do not register an executable backend. Compatibility warmup calls
return unavailable with the retirement reason; they perform no device work.
Historical GPU execution evidence remains distinguishable from a CPU result.

The independent native C reference keeps its unavailable GPU descriptor ABI
and test-only fallback/readback checkpoints. `experimental-native-gpu` remains
default-off; it is not a product WebGPU implementation. Its request lifetime
continues to carry `memory_ticket_id`, `fence_epoch`, `scope_epoch` and
`byte_budget`. The old materializing checkpoints stay `BUILD_TESTING`-only.

The former connected-WebGPU implementation and measurements remain in Git
history and dated research records. They do not describe the current product
support matrix. GPU trust labels alone are never new exact CPU evidence.

## Verification scope

The focused retirement regressions check explicit GPU and Hybrid requests,
both fallback policies, PC and Build entry points, distributed preparation,
and browser capability reporting. Native CPU and WASM compilation and the
existing CPU execution checks remain the relevant product gates. This document
describes the required contracts, not an unexecuted test result.
