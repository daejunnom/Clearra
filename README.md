# Clearra

Perfect-clear search and setup analysis for falling-block puzzles.

Clearra is a Rust toolkit for perfect-clear search, setup analysis, build
coverage, and queue-aware solving. It is an independent analysis toolkit. Public
docs and CLI output use generic falling-block terminology.

## MVP1

MVP1 targets standard 10-wide boards, tetromino pieces, standard 7-bag supply,
opening and scenario PC presets, setup/build coverage, validation, and
text/JSON/fumen-like output. Product requests compile to `SearchProblem`, then
lower through C `PackingProblem` and `BuildUpProblem`. Checkpoint labels remain
metadata rather than core success conditions.

Continuation-capable output includes `next_pc_available`, `continuation_token`,
and `continue_hint`.

## Probability Invariant

Probability is the measure of a PatternBitSet OR union. Variant probabilities
are never summed because multiple variants may cover the same queue pattern.

## CLI

```text
clearra pc
clearra path
clearra percent
clearra cover
clearra setup
clearra continue
clearra rules
clearra scoring
clearra convert
clearra inspect
clearra verify
clearra verify kicks
```

Global options such as `--format text|json|fumen-like`, `--lang en|ko`,
`--verbose`, `--diagnostics`, and `--verbose-paths` may appear before or after
the command. Default text output is a short human summary; JSON is the complete
stable contract.

## Verification

The standard developer command is:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task Local
```

`ManagedLocal` is the default execution surface. It performs formatting, Cargo
metadata validation, a C library build with `BUILD_TESTING=OFF`, a product
source-contract check, and architecture validation. It does not compile Rust
source artifacts, because Cargo compilation can execute generated build helpers.
It reports `policy_fallback_used=false`.

Executed evidence is always explicit:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task ProductE2E -ExecutionSurface Trusted
powershell -NoProfile -File scripts/clearra.ps1 -Task Strict -ExecutionSurface Trusted
powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted
```

`Trusted` runs each permitted process once and fails closed. Clearra does not retry a
blocked artifact, change to compile-only evidence after a failure, locally sign
generated files, unblock downloaded files, copy test executables, or alter the
PowerShell execution policy. `Trusted` expresses the requested evidence level;
it does not override host application-control capability. Windows gates record
Device Guard before generating a process surface. Enforced UMCI stops local
source execution with `E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE`;
permitted runners attempt the native command once, and an actual error 4551 is
reported as `E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED`. Neither path invokes WSL or
retries through another execution surface.

| Command | Meaning |
| --- | --- |
| `scripts/clearra.ps1 -Task Local` | Process-free source/metadata validation plus a C static-library build. Rust and C test harnesses are not built or launched. |
| `scripts/clearra.ps1 -Task ProductE2E` | Managed product source-contract check. With `Trusted`, executes the native-linked library route without launching `clearra.exe`. |
| `scripts/clearra.ps1 -Task ProductE2EBuilt` | Trusted-only product build followed by process E2E once. |
| `scripts/clearra.ps1 -Task NativeLocal` | Managed builds only the C static library. With `Trusted`, runs CTest and native Rust tests. |
| `scripts/clearra.ps1 -Task Validate` | Static dependency, unsafe-boundary, forbidden-API, and capability-contract checks. |
| `scripts/clearra.ps1 -Task DesktopHost -ExecutionSurface Trusted` | On a source-execution-capable runner, compiles the desktop UI in memory, executes the WASM CPU async GUI-host E2E, and attempts the Tauri compile once. Enforced UMCI requires the published release package instead. |
| `scripts/clearra.ps1 -Task Strict -ExecutionSurface Trusted` | Required Rust, C, native-link, ProductE2E, and desktop execution. |
| `scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted` | Ordered debt, adversarial, sanitizer, Rust exact, product, WASM, desktop, and render release gate. |

The gate summary distinguishes `not-built` from `launched`, and distinguishes
`source-contract`, `library`, and `process` product routes. A Local pass must
never be presented as release acceptance.

See [docs/test-policy.md](docs/test-policy.md) for the exact gate contract.

## Artifacts

Build/cache artifacts live below the platform Clearra artifact root, normally
`%LOCALAPPDATA%\Clearra\build` on Windows. Reports use the separate Clearra
report root. Repository-local `target`, `build`, and report output are rejected.
The external CMake/Cargo trees are reused across source changes and rely on
their native dependency tracking; only cache-budget overflow or a different
workspace/schema identity resets the complete tree. Disposable builds reuse a
locked `build/transient/<purpose>` slot and overwrite its previous contents;
the default runtime comparison report similarly replaces
`reports/runtime-environments/latest`. Pass an explicit output path only when
an additional local history is intentionally required.
The sole `_local` exception is `_local/bundle.py`, which writes the review
bundle under `_local` at the user's request.

## Web Runtime

The web app builds the Rust command runtime before Vite. This browser artifact
is independent of the Windows native product and is never a Windows fallback:

```text
cargo install wasm-bindgen-cli --version 0.2.126 --locked
npm install --ignore-scripts
npm run build -w @clearra/web
```

The default build uses the host-native Rust and `wasm-bindgen` toolchain. A
Windows host never crosses into WSL implicitly. Builders that explicitly own a
WSL toolchain can instead run `npm run build:wsl -w @clearra/web`. Neither path
falls back to the other after a failure.

For local development, run `npm run dev -w @clearra/web` and open the URL
printed by Vite. Use `dev:wsl` only when WSL was selected explicitly. To preview the deployable static SPA, run
`npm exec -w @clearra/web -- vite preview --host 127.0.0.1` after the build.

It uses the source-built `wasm-bindgen` CLI rather than a downloaded helper
executable. Generated JS/WASM lives in `apps/clearra-web/static/wasm` and is not
review or release source.

## Clearrabot

`apps/clearra-discord-bot` exposes Clearra searches through `/clearra`,
`!clearra`, and short product commands. It launches only the configured
Clearra executable with an argument array; no shell or external solver is part
of the command path.

Fumen and CTK3 documents can be passed to `/view` or posted in a normal
message. Clearrabot decodes them through the Clearra document contract and
uses its own indexed-pixel GIF89a/LZW renderer. Search text and image previews
are separate replies. Clearrabot links directly to the loaded CTK workspace
when the reply fits Discord's 2,000-character limit; otherwise it attaches a
canonical CTK3 document and links to the CTK renderer:

```text
https://daejunnom.github.io/Clearra/?tool=ctk&ctk=ctk3_...&viewer=1
https://daejunnom.github.io/Clearra/?tool=ctk&fumen=v115@...&viewer=1
```

See [apps/clearra-discord-bot/README.md](apps/clearra-discord-bot/README.md)
for Discord application settings and startup commands.

## Published Products

The browser GUI is published from the exact `main` source through GitHub Pages:

- https://daejunnom.github.io/Clearra/

The Pages workflow builds the WASM module and static SvelteKit application in
one job. Project-site assets use `/Clearra` as their deployment base, while
local development keeps the empty-root base.

Version tags publish three standalone GitHub Release executables:

- `Clearra-CLI-*-linux-x86_64`: Linux CLI
- `Clearra-CLI-*-windows-x86_64.exe`: Windows CLI
- `Clearra-GUI-*-windows-x86_64.exe`: SvelteKit/Tauri desktop GUI

The CLI and desktop executables compile the exact WASM CPU search backend and
WebGPU adapter into the host binary; they do not use fixture fallbacks or need
a separate `.wasm` file. The Tauri executable embeds the built SvelteKit
assets. GitHub records an immutable digest for each release asset, so Clearra
does not publish duplicate `.sha256` sidecar files.

## Runtime Environments

Runtime selection is explicit and host-preserving:

```powershell
scripts/compare-pc-runtime-environments.ps1 -Environment windows
scripts/compare-pc-runtime-environments.ps1 -Environment wsl
scripts/compare-pc-runtime-environments.ps1 -Environment wasm
scripts/compare-pc-runtime-environments.ps1 -Environment all
scripts/compare-pc-runtime-environments.ps1 -Environment wasm -Prepare -PrepareEnvironment wsl
```

`auto` never crosses from Windows to WSL or from WSL to Windows. Explicit WSL
execution syncs source bytes to a persistent ext4 workspace and uses WSL-native
Cargo/C caches without exposing a Windows path to Linux build or runtime
commands. Windows, WSL, and WASM reports remain independent; one environment is
never accepted as evidence that another succeeded. `-PrepareEnvironment` only
selects where a WASM deployment artifact is built. Once prepared, the WASM
comparison uses the deployed bindgen/module pair and Node host without Cargo,
WSL, or another `wasm-bindgen` invocation.

## Desktop

The only desktop product is `apps/clearra-desktop`:

```text
SvelteKit/Tauri -> clearra-gui-host -> clearra-app -> AppResponse
```

On a source-execution-capable development host, start it with
`npm run tauri -w @clearra/desktop -- dev`.

The tagged Windows GUI is the same product surface packaged as one Tauri
executable; no separate CLI window or frontend directory is distributed.

The desktop gate is intentionally Trusted-only:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task DesktopHost -ExecutionSurface Trusted
```

There is no C GUI executable, CLI subprocess shortcut, or shell-preview desktop
binary. On Windows, the gate records `Win32_DeviceGuard` and recent Code
Integrity events, then invokes the Tauri build once. A real application
control rejection produces `E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED` and a release
failure. The runner does not use WSL, sign or unblock local artifacts, or turn
another runtime's evidence into a desktop success.

## Build Ownership

The repository root is a virtual Cargo workspace and has no `build.rs`. C core
builds are owned by CMake; native linking uses the static C library plus a
runner-supplied `RUSTFLAGS=-L native=<dir>`. The isolated Tauri crate is outside
the root workspace because Tauri requires its own build script. Every Cargo task
shares one external `%LOCALAPPDATA%\Clearra\build\cargo-target` tree.

See [docs/build-system.md](docs/build-system.md) for build ownership details.
