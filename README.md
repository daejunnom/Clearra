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
clearra pc-replay
clearra percent
clearra failed-queue
clearra build-coverage
clearra setup-finder
clearra continue
clearra rules
clearra scoring
clearra convert
clearra inspect
```

`clearra failed-queue` runs the reverse PC search and returns the exact
complement of queues that reach the target. Use `--failed-count N` to bound the
materialized queue list without changing the exact failed count or probability.

The historical `path`, `cover`, and `setup` spellings remain compatibility
aliases. Their canonical Clearra names avoid implying Sfinder command semantics.

Sfinder-style invocations use a separate namespace so those three names cannot
silently change meaning:

```text
clearra sfinder path <fumen> <pattern> [lines]
clearra sfinder cover <solution-fumen> <pattern> [lines]
clearra sfinder setup <colored-target-fumen> <pattern>
clearra sfinder --help
```

The product GUI exposes seven tools: PC search, setup finder, build probability,
maximum damage, spin finder, CTK, and a local Player. The Player can invoke PC
or setup search from its own drawer without changing pages; those requests still
use the existing typed search boundary. The compatibility boundary translates
input and output contracts; it does not run a Java process or bundle another
solver.

Global options such as `--format text|json|fumen-like`, `--lang en|ko`,
`--verbose`, and `--verbose-paths` may appear before or after the command.
Default text output is a short human summary; JSON is the complete stable
contract.

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

The local `ReleaseAcceptance` command remains the complete serial authority.
The canonical release workflow runs the same eight stages in four physically
isolated Windows jobs and admits the result only after a closed evidence fan-in:
Foundation (`NoProductDebt`, `AdversarialCorrectness`, `DesktopHost`), Sanitizer,
Rust (`RustExactTests`, `ProductE2E`, `RenderGolden`), and Pages
(`WasmBuildTest`). The split never runs concurrent stages in one workspace and
does not share mutable Cargo, C, or Pages build directories between jobs.

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

`apps/clearra-discord-bot` exposes the represented Sfinder-compatible contracts
as individual slash commands such as `/path`, `/percent`, and `/setup`, plus
local syntax guidance through `/help`. Search commands receive structured
`field`, `next`, or `remaining` inputs; `/cover` instead receives separate
`base`, `target`, and `next` inputs, where `target` contains only cells to add.
One command-specific `options` string is used only for allow-listed optional
settings. CTK3 is decoded directly by the npm `ctk3` package and is never
converted to Fumen. CTK3 and Fumen compute inputs must each be one static
10-column page; every non-empty input color is projected to the same occupancy
bit before the typed Rust validation boundary. `/cover` sends its colorless
base and target-delta masks to the existing build-probability command.

Discord search solution documents are emitted only as CTK3. Generated pieces
retain their tetromino colors and cells that were occupied in the input field
are `G`; the active Discord result path does not emit Fumen. This presentation
contract does not change PC/build candidate generation or pruning. One Gateway
process receives Discord slash, Modal, Message-command, `$`, and `>` events,
acknowledges interactions within Discord's deadline, renders bounded previews,
and owns response delivery. Heavy searches are sent to the Tokyo
(`asia-northeast1`) `clearra-current-job` service; that compute service receives
neither Discord credentials nor interaction delivery tokens.

There is no active `/clearra` catch-all or `/view` command. Ordinary-message
commands pass through the same curated command policy and resource limits as
their slash equivalents. Standalone CTK3/Fumen documents and strict `#`/`_`
grids can use the bounded image path, while search-command fields are rendered
inside their command.

The raw CLI compatibility form remains available as a separate legacy boundary:
`clearra sfinder cover <solution-fumen> <pattern> [lines]`. It is not the
Discord `/cover` contract and is not converted into the two-field ingress.

The approved Cloud Run shape scales from zero to four instances. Each instance
has 8 vCPUs and 16 GiB, accepts request concurrency 1, runs one native
runtime-selected full-capacity search at a time, and keeps CPU throttling
disabled. The native hard limit remains authoritative if the container's
effective Linux parallelism is lower than its configured vCPU count. The
service can therefore run four searches across four instances; it is not
globally serial. Cloud Run's eight-vCPU per-instance maximum rules out a single
16-vCPU instance. Slash command registration is a separate trusted operation;
the compute service does not need `DISCORD_TOKEN`. See
[apps/clearra-discord-bot/README.md](apps/clearra-discord-bot/README.md) for
the command catalog, Discord timing boundary, Cloud Run settings, and the
active Gateway/job-service boundaries.

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
