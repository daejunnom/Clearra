# GUI Host

`clearra-gui-host` is a library boundary used by the Tauri desktop product. It
is not an executable and does not own a second desktop shell.

Its responsibilities are:

- map GUI form state to typed `clearra-app/AppRequest` values;
- run GUI validation before application execution;
- call `clearra-app` through `AppContext`;
- convert the application result to the shared host `AppResponse` contract;
- own desktop job, cancellation, progress, settings, and display models;
- keep backend, memory, diagnostic, replay, and render capability fields
  explicit for the UI.

It must not:

- spawn `clearra.exe` or any CLI subprocess;
- create CLI text and parse it again;
- import raw C core pointers or call C ABI functions;
- manufacture preview, fixture, placeholder, or example final responses;
- claim renderer, GPU, or solver execution when that capability did not run.

## Request Route

`DesktopTauriCommandBridge::run_request` parses the desktop form envelope and
builds a typed `AppRequest`. PC and PC-scenario retain the GUI form builder;
setup, build probability, damage, and spin finder are translated through their
typed web-command inputs. No route creates CLI text. `validate_request` and
`start_job` use the same dispatcher, so validation and execution cannot disagree
about the command family.

## Job Route

`start_job` queues the same typed request. `GuiJobRunner` executes it on the
host worker thread and emits a completed event containing the real host
`AppResponse` plus the structured search report required by the shared result
UI. The report is embedded with a borrowed raw-JSON serializer, avoiding a
second parsed `serde_json::Value` tree for large setup and forward-search
results. It does not emit a synthetic partial or marker-only final response.
`cancel_job` sets the cooperative cancellation token; event JSON reports
cancellation without exposing an execution scope or raw pointer.

## Desktop Ownership

Only `apps/clearra-desktop` owns desktop startup and window lifecycle. Tauri
commands call this library, and the SvelteKit UI consumes their JSON. The root
CMake project builds only `core-c`; no GUI product target is registered there.

Use `scripts/clearra.ps1 -Task DesktopHost` for the desktop acceptance path.
