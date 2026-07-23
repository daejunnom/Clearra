# Desktop GUI

Clearra has one desktop product surface:

```text
apps/clearra-desktop
  -> Tauri commands
  -> clearra-gui-host
  -> clearra-app
  -> AppResponse
```

The desktop frontend is the SvelteKit application in
`apps/clearra-desktop`. Its Tauri crate is
`apps/clearra-desktop/src-tauri`; it exposes only `run_request`,
`validate_request`, `start_job`, `cancel_job`, and `get_job_events`.
Those commands delegate to `DesktopTauriCommandBridge` and do not import
`clearra-app`, `clearra-core-ffi`, or C ABI symbols directly.

## Product Boundary

The UI builds a typed form envelope and invokes a Tauri command. The Rust host
converts the form to `AppRequest`, validates it, calls `AppContext::run`, and
serializes the resulting host `AppResponse`. It never builds CLI text, launches
`clearra.exe`, or parses CLI output.

The product build does not contain a native C GUI shell, shell-preview desktop
binary, or fixture response generator. It executes the exact WASM CPU backend
and connected WebGPU adapter through `clearra-app`. Backend unavailability is
reported explicitly and never replaced with a synthetic solution.

## Jobs

Long-running desktop work is owned by `clearra-gui-host/src/job`. A job emits
started, progress, diagnostic, completed, failed, or cancelled events. A
completed event carries the real host `AppResponse`; progress and cancellation
events never expose raw pointers. `get_job_events` returns every queued event
in order. A terminal event joins the worker and releases the queue slot before
another job may start. Cancellation uses a shared token that reaches WASM CPU
search loops; it does not use a process kill shortcut.

## Render State

Render controls consume the connected exact capability and validated default
PNG-atlas skin. An enabled render request is rejected only when its selected
skin manifest or provenance is invalid. Runtime raw SVG remains forbidden.

## Verification

Run:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task DesktopHost -ExecutionSurface Trusted
```

This checks the GUI host library, launches the existing GUI host contract test,
compiles the Tauri crate, builds the SvelteKit frontend, and runs the U6
architecture gate. This is the one desktop product surface gate.
