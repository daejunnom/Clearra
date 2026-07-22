# Build System

## Workspace Root

The root Cargo.toml is a virtual workspace. The repository root does not own a Cargo build.rs.

## C Core Build Owner

The C core is built by CMake. The canonical developer runner is
`scripts/clearra.ps1`. The lower-level C runner lives in
`scripts/lib/core-c-tests.ps1`, with user-facing wrappers such as
`scripts/run-c-core-tests.ps1`.

CMake is script-owned.

## Native C Link Policy

`clearra-core-ffi` declares the native static library link with:

```rust
#[link(name = "clearra_core", kind = "static")]
```

The library search path is supplied by a target-scoped runner variable:

```text
CARGO_TARGET_<UPPER_SNAKE_TARGET_TRIPLE>_RUSTFLAGS="-L native=<clearra_core_lib_dir>"

Windows example:
CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-L native=<clearra_core_lib_dir>"
```

The runner must not put this path in global `RUSTFLAGS`, because host tools and
proc macros do not link the target C core. Target-scoped flags keep native
linking explicit without asking Cargo to launch a build script executable.

The native link path is runner-owned.

The native library content hash is not a Rust crate metadata suffix. Putting
that hash in global `-C metadata` flags would give every C rebuild a new Cargo
identity and duplicate the complete dependency graph. Runners instead share
the canonical external `CARGO_TARGET_DIR` and record the native library path
and SHA-256 in:

```text
<CARGO_TARGET_DIR>/.clearra-state/native-core-link.txt
```

The state file is UTF-8 because an absolute library path may contain non-ASCII
characters. When the fingerprint changes, the runner invalidates only the
debug and release artifacts for `clearra-core-ffi`; unchanged native builds
reuse the existing Cargo graph. This selective invalidation is runner-owned,
does not use `build.rs`, and does not invoke WSL.

The external artifact root is an incremental cache, not a disposable snapshot
of the complete repository. A source or script change updates the cache input
signature but preserves the CMake and Cargo trees so those build systems can
invalidate their own affected nodes. The complete artifact root is removed
only when its size budget is exceeded or its workspace/schema identity no
longer matches. This prevents unrelated Rust changes from rebuilding C and
prevents repeated runs from accumulating one full cache generation per input
signature.

## Standard Workspace Policy

Cargo build scripts are forbidden in the repository's standard verification
workspace and in every crate under `crates/`.

- root build.rs
- crate-local build.rs
- `Cargo.toml build = "build.rs"`
- automatic CMake invocation from a Cargo build script

## Tauri Desktop Exception

`apps/clearra-desktop/src-tauri` is excluded from the root Cargo workspace and
is built only by the explicit `DesktopHost` gate. Tauri requires its standard
`tauri-build` build script to generate application context and Windows resource
metadata. This build script does not build or link the C core. The runner builds
the C library once with CMake and supplies its directory through the
target-scoped `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`; it does not modify global
`RUSTFLAGS`.

`scripts/desktop-host-check.ps1` runs the isolated manifest with
the same canonical `CARGO_TARGET_DIR` used by every other Cargo task, normally
`%LOCALAPPDATA%\Clearra\build\cargo-target`. Task-specific target trees are
forbidden because they multiply unsigned Cargo build-script executables. Build
artifacts do not enter the repository. No other app or crate may use the Tauri
build-script exception.

The gate records
`Win32_DeviceGuard.UsermodeCodeIntegrityPolicyEnforcementStatus` before creating
a generated execution surface. Enforced UMCI keeps local source work
compile-only and returns
`E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE`; an approved prebuilt
runtime must have a valid Authenticode signature before its one launch attempt.
If policy changes after preflight and Windows returns error 4551, the runner
correlates Code Integrity events 3033/3077 with the generated artifact and
preserves the policy ID as `E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED`. The gate never invokes
WSL, signs, unblocks, copies, retries, or weakens the requested evidence.

## Windows-hosted WASM Product Surface

The Windows product computation surface is the browser-hosted WASM deployment
unit produced from `clearra-wasm-abi`: `clearra_wasm.js` and
`clearra_wasm_bg.wasm`. The source-built `wasm-bindgen` CLI matching
`Cargo.lock` creates the reviewed host imports needed by WebGPU/wgpu while the
implementation crate `clearra-wasm` remains an `rlib`. No generated PE solver,
native C core, subprocess, WSL runtime, signing mutation, or policy bypass
participates in product execution.

Windows application-control policy therefore applies only to the user's chosen
WebAssembly host, not to a source-generated Clearra executable. A host load or
execution failure is returned as that WASM runtime's explicit failure and is
never retried through Windows native or WSL. Build helpers remain development
inputs and do not ship with the artifact.

The release and command-runtime gates stage the exact binding/module pair,
verify the scalar/memory ABI through those bindings, and execute the PCO command
contract before publishing it. The browser worker and Node command probe
consume the same pair; preparation time is kept outside search timings. Empty
imports and direct raw-WASM instantiation are not a product surface because
WebGPU requires reviewed host imports.

## Reason

Cargo build scripts add executable launch points to a normal workspace build.
Clearra keeps CMake and native link setup in the developer runner so the default
`ManagedLocal` surface can compile the C library with `BUILD_TESTING=OFF`, but it
does not compile Rust workspace artifacts: Cargo compilation may execute a
newly linked build helper even when no final test or product binary is launched.
Trusted gates fail before source-generated execution when UMCI is enforced. On
permitted runners they attempt the requested native process once and classify
the actual result. They never retry a blocked executable through WSL, sign
local output, or substitute fallback evidence. The isolated Tauri exception is
explicit, `Trusted`-only, and does not alter the native-core policy.
