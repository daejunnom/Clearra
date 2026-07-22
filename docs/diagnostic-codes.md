# Diagnostic Codes

Diagnostic codes have stable namespaces and default severities. Scope violations, invalid duplicates, and probability invariant violations are errors; ambiguous observed windows and disabled fast paths are warnings or informational compatibility notes.

`docs/diagnostics.md` owns the broader evidence and severity policy for
SpinTarget, kick-sensitive spin classification, score matrix, and coverage
identity diagnostics. This file remains the compact namespace reference.

## Validation Diagnostics

Runtime query, capability, and invariant diagnostics are owned by `clearra-validation::diagnostic::DiagnosticCode`. These codes are rendered from `DiagnosticReport` and use stable domain namespaces such as `E_SUPPLY_*`, `W_FAST_PATH_*`, `W_RULE_*`, `I_PC_*`, `E_SETUP_*`, `E_BUILD_*`, and `E_SCORE_PROFILE_*`.

Score profile diagnostics are validation diagnostics, not CLI adapter errors. `E_SCORE_PROFILE_INVALID` covers invalid score profile JSON, unknown scoring fields, unsupported spin rules, unsupported profile-specific exact accuracy claims, and invalid combo/B2B policy settings. `I_SCORE_PROFILE_MVP2_SUPPORTED` confirms that a score profile is valid for the MVP2 post-processing scoring layer and must disclose whether it is a `basic-approximation`.

## Security Diagnostic Gate

S6 security and correctness failures use validation diagnostic codes, not ad hoc
stderr strings. The stable security gate codes are:

- `E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE`
- `E_CORE_MEMORY_SCOPE_INVALID`
- `E_CORE_MEMORY_LEAK_DETECTED`
- `E_CORE_FFI_BUFFER_BOUNDS`
- `E_CORE_INVALID_NATIVE_VIEW`
- `E_KICK_EVIDENCE_BUFFER_EXHAUSTED`
- `E_GPU_WORKER_MISSING_MEMORY_TICKET`
- `E_GPU_FENCE_EPOCH_MISSING`
- `E_GPU_UNCONFIRMED_PROBABILITY_SOURCE`
- `W_BACKEND_FALLBACK_USED`
- `W_TRACE_RETENTION_TRUNCATED`
- `W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE`
- `E_RENDER_RUNTIME_SVG_FORBIDDEN`
- `E_RENDER_ASSET_PROVENANCE_MISSING`
- `E_GUI_SUBPROCESS_FORBIDDEN`
- `E_FRONTEND_TYPED_REQUEST_REQUIRED`

JSON diagnostics include structured evidence and `suggested_next_step` through
`contract.diagnostics.items`. Text diagnostics print the same code and concise
message, then optional `location`, `evidence`, and `next` lines.

## CLI Adapter Errors

CLI parsing and adapter failures are not validation diagnostics. They are owned by `clearra-cli::error::CliErrorCode`, which is the only source for CLI stderr code strings. This keeps argv parsing, unsupported command routing, convert adapter failures, and command assembly failures stable without mixing them into the validation crate.

The CLI adapter namespace currently includes `E_CLI_*`, `E_CONVERT_*`, `E_CONTINUE_*`, `E_PC_*`, `E_SETUP_QUERY_INVALID`, and `E_VERIFY_TARGET_UNKNOWN`. Each `CliErrorCode` also owns its default `ExitCode`, so command handlers do not hand-format error prefixes or choose ad hoc exit codes.
