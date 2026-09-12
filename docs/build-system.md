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

## One build root and bounded generations

All managed compilation uses one physical root per host. Windows uses
`%LOCALAPPDATA%/Clearra/build`; WSL maps the same Windows directory via
`/mnt/<drive>/...`, without a second ext4 cache. Independent Linux CI hosts
use `${XDG_CACHE_HOME:-$HOME/.cache}/Clearra/build`. The platform selects the
root. Conflicting root/target/wrapper overrides and symlink/junction escapes
are rejected before creating a build transaction.

```text
Clearra/build/
  experiments/<source-root-id>/current/   one whole build per source root
    cargo-target/                       Rust, host and WASM targets
    core-c-.../                         C variants
    wasm-stage/                        unpublished staging
  products/<UTC-timestamp>-<session>/     newest five completed generations
    cargo-target/
  .leases/                              active ownership only
```

A purpose is the canonical source root, not the command, package or executable.
Its ID is the first 24 hexadecimal characters of SHA-256 over the normalized
absolute root; Windows case and WSL mount aliases normalize identically.
A new independent experiment removes the previous **whole** current generation,
including hashed dependencies and incremental variants. Nested commands share
one generation. Independent owners of the same purpose are rejected; different
source roots may build in parallel under the one physical root.

Products are explicitly selected with `CLEARRA_BUILD_PURPOSE=product` or
`-Purpose product` / `--purpose product`. Only owner-confirmed completion counts
toward the newest five generations. Normal failure/cancellation exit removes
failed products. Force-killed owners leave active records/leases that are not
silently taken over. Unknown or modified ownership records fail closed;
recovery requires explicitly authorized, path-verified maintenance.

Independent product owners on the same host cannot overlap: a catalog lease
also prevents crashed/incomplete product builds from accumulating new random
generations. Parallel child builds within the selected owner remain supported;
independent CI hosts are unaffected. A later product request with an unresolved
active record or lease fails before compiling and requires explicit cleanup.

The shared schema binds `CLEARRA_BUILD_ROOT`, `CLEARRA_BUILD_PURPOSE`,
`CLEARRA_BUILD_SOURCE_ROOT`, source/session IDs, owner PID,
`CLEARRA_BUILD_TRANSACTION_ROOT` and the exact canonical `CARGO_TARGET_DIR`.
`RUSTC_WRAPPER` validates ownership and compiler output paths. Repository
Cargo configuration rejects unmanaged Cargo before compilation. Use
`scripts/tools/invoke-clearra-build.ps1` or the Node counterpart
`scripts/tools/invoke-clearra-build.mjs` (also used by Bookworm).
On Windows the owner prepares a tiny native argv launcher inside the same
transaction's `build-tools` directory. It forwards directly to the shared Node
compiler guard without `cmd.exe`, whose shorter command-line ceiling rejects
large dependency feature lists. The launcher has no independent cache or policy;
nested commands reuse it and generation retirement removes it.
This prevents accidental path bypass; it is not an operating-system sandbox
against deliberately replacing configuration or using an unmodified old branch.

```powershell
./scripts/tools/invoke-clearra-build.ps1 -Purpose experiment -Command cargo `
  -ArgumentsJson '["check","--locked","-p","clearra-cli-command"]'
./scripts/tools/invoke-clearra-build.ps1 -Purpose product -Command cargo `
  -ArgumentsJson '["build","--locked","--release","-p","clearra-cli"]'
```

```sh
node scripts/tools/invoke-clearra-build.mjs --source-root "$PWD" \
  --purpose experiment -- cargo check --locked -p clearra-cli-command
```

The owner sets `CARGO_INCREMENTAL=0`. Clean independent generations trade
rebuild speed for the requested storage bound. Compiler reuse belongs inside
one owner; global dependency downloads/toolchains are not build generations.
CI must not restore compiled trees into new generations. Related producer,
verification and consumer commands share one owner step rather than inheriting
expired owner state from a previous step.

Publication is separate from compilation. The current 4194 WASM, accepted
release exports and deployed files keep their publication contracts.
Repository-local legacy build trees are not silently removed on build start.
The separately authorized one-time migration removes enumerated generated
outputs only, preserving sources, worktrees, reports and published runtimes.

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
the exact WASM CPU backend and WebGPU adapter directly into the Tauri
executable. The retired Windows native C execution path is not linked into the
desktop release.

`scripts/desktop-host-check.ps1` runs the isolated manifest with
the same canonical `CARGO_TARGET_DIR` used by other tasks in its owner transaction,
`<transaction-root>/cargo-target`. Task-specific target trees are
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

## Windows-hosted WASM Product Surfaces

The browser product computation surface is the WASM deployment unit produced
from `clearra-wasm-abi`: `clearra_wasm.js` and `clearra_wasm_bg.wasm`. The
source-built `wasm-bindgen` CLI matching
`Cargo.lock` creates the reviewed host imports needed by WebGPU/wgpu while the
implementation crate `clearra-wasm` remains an `rlib`.

Tagged releases also compile the same exact WASM CPU algorithm into standalone
Windows CLI and SvelteKit/Tauri GUI executables. The GUI embeds its frontend;
neither executable needs a sidecar `.wasm`, native C core, subprocess, WSL
runtime, signing mutation, or policy bypass.

Windows application-control policy applies normally to the published CLI and
GUI executables. A load or execution failure is returned directly and is never
retried through another path. Build helpers remain development inputs and do
not ship with either artifact.

The release and command-runtime gates stage the exact binding/module pair,
verify the scalar/memory ABI through those bindings, and execute the PCO command
contract before publishing it. The browser worker and Node command probe
consume the same pair; preparation time is kept outside search timings. Empty
imports and direct raw-WASM instantiation are not a product surface because
WebGPU requires reviewed host imports.

Successful development builds and explicit staging use the same generation
retention transaction. `clearra_wasm.manifest.json` remains the sole current
runtime authority and is published after the new binding/module pair. A
non-runtime retention sidecar records deterministic binding/WASM pairs by path,
byte count, and full SHA-256, and keeps the current generation plus the four
most recent complete generations. Publishing a sixth generation removes only
the oldest proven pair. Missing, malformed, orphaned, untracked, or hash-mismatched
managed files make cleanup retain every file; a deletion I/O failure fails the
publisher. This bounded history lets an already-running development worker keep
its generation while the next search adopts the newly verified manifest.

## Reason

Default product, test, desktop, and artifact commands never invoke `wsl.exe`
as a fallback after a Windows policy failure. Explicitly selected WSL developer
tools retain their independent environment/source binding. The separate browser product
is a WASM surface. A degraded result does not classify a policy failure as success.

Cargo build scripts add executable launch points to a normal workspace build.
Clearra keeps CMake and native link setup in the developer runner so the default
`ManagedLocal` surface can compile the C library with `BUILD_TESTING=OFF`, but it
does not compile Rust workspace artifacts: Cargo compilation may execute a
newly linked build helper even when no final test or product binary is launched.
Trusted gates fail before source-generated execution when UMCI is enforced. On
permitted runners they attempt the requested native process once and classify
the actual result. They never retry a blocked executable through WSL, sign
local output, or substitute fallback evidence. The isolated Tauri exception is
explicit, `Trusted`-only, and does not restore the retired Windows native C
product path.
