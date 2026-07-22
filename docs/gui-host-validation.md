# GUI Host Validation

G9 adds a validation layer under `crates/clearra-gui-host/src/validation`.
The GUI host must validate `GuiAppState` before constructing a typed
`clearra-app/AppRequest`; GUI execution without validation is forbidden.

Required files:

- `gui_form_validator.rs`
- `gui_backend_validator.rs`
- `gui_file_path_validator.rs`
- `gui_render_validator.rs`
- `gui_validation_diagnostic.rs`
- `gui_validation_summary.rs`

The validation checks are product-facing, not CLI parsing:

- `lines > 0`
- queue piece valid
- field mask valid
- backend supported or fallback allowed
- fixture file path safe
- skin asset valid
- render option supported

The file path policy mirrors the CLI sensitive path guard. By default, absolute
paths are redacted with `.../<file-name>`, sensitive-looking file path values
are blocked, and names such as `service-account.json`, `.env`, SSH key names,
credential, api_key, apikey, and secret are refused before metadata is read.
Verbose path display is explicit only.

Render validation accepts the connected default PNG-atlas renderer and rejects
an enabled request only when the selected skin id, manifest, atlas, or
provenance is invalid. `E_GUI_RENDER_UNSUPPORTED` remains the typed diagnostic
for an invalid requested render surface; text and JSON requests remain
independent of bitmap export.

Stable diagnostics:

- `E_GUI_FORM_INVALID`
- `E_GUI_FILE_PATH_UNSAFE`
- `E_GUI_RENDER_UNSUPPORTED`
- `W_GUI_BACKEND_FALLBACK_REQUIRED`

The executable desktop contract is checked by the existing
`clearra-gui-host/tests/gui_host_contract.rs` target through
`scripts/clearra.ps1 -Task DesktopHost`. Static architecture validation remains
responsible only for dependency, forbidden API, public ABI, unsafe boundary,
and unsupported capability checks.
