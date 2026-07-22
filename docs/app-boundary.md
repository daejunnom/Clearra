# App Boundary

Clearra product hosts use one typed boundary:

`CLI / GUI / WASM Command Runtime / Desktop host -> AppRequest -> clearra-app -> validation -> executor -> AppResponse`

CLI args, GUI forms, WASM command text, and desktop host JSON must build
`AppRequest`. They must not call solver internals directly, spawn `clearra.exe`,
parse CLI text as an intermediate GUI contract, or model browser execution as a
native process/exit-code runtime.

## AppRequest Contract

The product request contract contains:

- `AppCommandKind`
- `QueryEnvelope`
- `BackendPolicy`
- `OutputPolicy`
- `DiagnosticsPolicy`
- `LocalePolicy`
- `ResourceBudget`

The Rust `AppCommand` enum keeps typed payloads for existing app execution, but
every request exposes an `AppCommandKind` so external hosts see the stable
command set:

- `Pc`
- `Path`
- `Percent`
- `Setup`
- `Cover`
- `Continue`
- `Rules`
- `Scoring`
- `Convert`
- `InspectUnsupported`
- `Verify`
- `VerifyKicks`

`pc-scenario` is a `Pc` command kind with a `PcScenario` query envelope.

## AppResponse Contract

The product response contract contains:

- command
- status
- result
- diagnostics
- backend report
- resource report
- capability report
- continuation report

`clearra-output` and CLI renderers consume `AppResponse`; they do not validate
queries or call the executor. Validation errors return `ValidationFailed` and do
not execute the solver. Warnings may execute, but the diagnostics remain
attached to the response.

## R Host Runtime Contract

Native CLI, the desktop host, the WASM command runtime, and WebGPU use the same
product contract. Host-side event streams use
`clearra-host-contract::JobEvent`:

- `Started(JobStarted)`
- `Progress(JobProgress)`
- `BackendStatus(BackendStatusReport)`
- `ResourceStatus(ResourceReport)`
- `PartialResult(PartialResult)`
- `Diagnostic(DiagnosticEvent)`
- `Completed(AppResponse)`
- `Cancelled(CancelledReport)`
- `Failed(DiagnosticReport)`

WASM command runtime flow is:

`command text -> WebCommandParser -> AppRequest -> clearra-wasm -> Web Worker -> WASM CPU or WebGPU -> AppResponse / JobEvent`

The browser worker forwards the Rust `start_job`, `advance_job(work_budget)`,
`cancel_job`, and event-drain ABI. Each search slice preserves its Packing or
BuildUp frontier and returns control to the browser event loop. It does not
construct final responses. Parser failures are
`Failed(DiagnosticReport)` events, while completed jobs carry the serialized
`AppResponse` returned by `clearra-app`. Cancellation first signals the Rust
token. The next bounded slice observes it, releases the computation scope, and
emits `Cancelled`; it never emits a final solution, count, or probability.

The web build compiles `clearra-wasm-abi` for `wasm32-unknown-unknown` and uses
the source-built `wasm-bindgen` version pinned by `Cargo.lock` to publish
`clearra_wasm.js` with `clearra_wasm_bg.wasm`. The browser worker loads that
deployment unit once and binds the small scalar/memory ABI. The bindings provide
reviewed browser/WebGPU imports only; there is no native-process or WSL
fallback.

The desktop host flow is:

`SvelteKit static SPA -> Tauri command -> clearra-gui-host -> clearra-app -> AppResponse / JobEvent`

Allowed Tauri commands are `run_request`, `validate_request`, `start_job`,
`cancel_job`, and `get_job_events`. The event command returns an ordered JSON
array and releases the active worker slot after `Completed`, `Failed`, or
`Cancelled`. Tauri commands must call the GUI host only;
they must not call C ABI functions, spawn `clearra.exe`, or parse CLI text as an
execution shortcut.

WebGPU uses pre-reviewed embedded shaders only. User-provided WGSL and runtime
shader injection are forbidden. Reports must expose shader hash/version,
adapter/device limit checks, storage-buffer limits, chunked batch policy,
browser memory pressure, and fallback to `wasm-cpu` with a visible reason.

## WASM Geometry Exact-Cover Scope

The stable WASM command boundary parses commands into typed `AppRequest` values
and executes the immutable geometry catalog, canonical exact-cover family DAG,
realization feasibility, BuildOrder language, exact reachability, and symbolic
standard-bag coverage pipeline inside the module. Its WASM CPU executor is the
baseline exact implementation. A WebGPU executor may accelerate the same state
graph only when it consumes the same catalog identity and produces the same
canonical tiling and coverage contracts. WebGPU unavailability falls back only
to this WASM CPU executor and never to a legacy algorithm, native process, or
fixture result.

R completion markers: `cli_gui_wasm_share_app_request_schema`,
`job_event_reports_resource_budget`,
`job_event_reports_search_and_post_backend`,
`wasm_runtime_does_not_spawn_process`, `webgpu_user_shader_rejected`,
`shader_hash_reported`, `desktop_tauri_command_calls_gui_host_only`, and
`gui_does_not_spawn_clearra_exe`.

Completion markers:

- `cli_pc_builds_app_request`
- `gui_form_builds_app_request`
- `wasm_command_builds_app_request`
- `app_validation_runs_before_executor`
- `app_error_does_not_execute_solver`
- `output_consumes_app_response_only`
