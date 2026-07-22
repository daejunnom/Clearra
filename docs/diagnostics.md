# Diagnostics

Diagnostics explain why a result is exact, estimated, truncated, disabled, or
unsupported. They must not guess root causes. Errors require evidence; uncertain
inputs become warnings or explicit needs-evidence diagnostics.

## Spin, Coverage, And Score Codes

Additional diagnostic codes for SpinTarget, kick-sensitive spin, score matrix,
and coverage identity:

- `E_SPIN_TARGET_UNSUPPORTED`
- `E_SPIN_PROFILE_UNVERIFIED`
- `E_SPIN_KICK_EVIDENCE_MISSING`
- `E_SPIN_CLASSIFIER_INCOMPATIBLE`
- `E_SCORE_PROFILE_SPIN_POLICY_INCOMPATIBLE`
- `E_SCORE_MATRIX_CAPACITY_EXCEEDED`
- `E_SPIN_COVERAGE_CAPACITY_EXCEEDED`
- `E_COVERAGE_CAPACITY_EXCEEDED`
- `E_BUILDUP_VARIANT_ENUMERATION_TRUNCATED`
- `E_KICK_EVIDENCE_BUFFER_EXHAUSTED`
- `E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE`
- `E_CORE_MEMORY_SCOPE_INVALID`
- `E_CORE_MEMORY_LEAK_DETECTED`
- `E_CORE_FFI_BUFFER_BOUNDS`
- `E_CORE_INVALID_NATIVE_VIEW`
- `E_GPU_WORKER_MISSING_MEMORY_TICKET`
- `E_GPU_FENCE_EPOCH_MISSING`
- `E_GPU_UNCONFIRMED_PROBABILITY_SOURCE`
- `E_RENDER_RUNTIME_SVG_FORBIDDEN`
- `E_RENDER_ASSET_PROVENANCE_MISSING`
- `E_GUI_SUBPROCESS_FORBIDDEN`
- `E_FRONTEND_TYPED_REQUEST_REQUIRED`
- `E_SPIN_COVERAGE_UNIVERSE_MISMATCH`
- `E_COVERAGE_WEIGHT_MODEL_MISMATCH`
- `E_COVERAGE_ROW_KIND_UNSUPPORTED`
- `E_C_MEMORY_SCOPE_INVALID`
- `E_GPU_WORKER_TRUST_MISMATCH`
- `W_GPU_BACKEND_FALLBACK`
- `W_GPU_DEVICE_UNAVAILABLE`
- `W_HYBRID_BACKPRESSURE_ACTIVE`
- `W_SPIN_CLASSIFICATION_ESTIMATED`
- `W_SPIN_TARGET_PROBABILITY_INCOMPLETE`
- `W_SCORE_EXPECTATION_SAMPLE_ONLY`
- `W_SPECIAL_SPIN_DESCRIPTOR_ONLY`
- `W_BUILDUP_ENUMERATION_TRUNCATED`
- `W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE`
- `W_TRACE_RETENTION_TRUNCATED`

## Default Severities

`E_SPIN_TARGET_UNSUPPORTED` is an error.

`E_SPIN_PROFILE_UNVERIFIED` is an error when the user requested exact special
spin classification and a warning when optional score estimation is allowed.

`E_SPIN_KICK_EVIDENCE_MISSING` is an error for exact SpinTarget queries and a
warning for approximate score output.

`E_SPIN_CLASSIFIER_INCOMPATIBLE` and
`E_SCORE_PROFILE_SPIN_POLICY_INCOMPATIBLE` are errors.

`E_SCORE_MATRIX_CAPACITY_EXCEEDED` is an error unless the user explicitly
allowed truncated score-matrix results, in which case it becomes a truncated
result diagnostic with incomplete output.

`E_SPIN_COVERAGE_CAPACITY_EXCEEDED` is an error for exact SpinTarget queries
unless the caller explicitly accepts incomplete probability output.

`E_COVERAGE_CAPACITY_EXCEEDED` is an error. A coverage capacity failure must not
be converted to an empty successful row set.

`E_BUILDUP_VARIANT_ENUMERATION_TRUNCATED` is an error for exact all-solution,
min-cover, and coverage probability evidence. `W_BUILDUP_ENUMERATION_TRUNCATED`
is reserved for requests that explicitly permit partial results.

`E_KICK_EVIDENCE_BUFFER_EXHAUSTED` is an error for exact kick-sensitive spin
classification because exactness requires complete kick evidence.

`E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE`, `E_CORE_MEMORY_SCOPE_INVALID`, and
`E_CORE_MEMORY_LEAK_DETECTED` are errors. They must preserve the concrete C
memory status or leak evidence and must not be collapsed into a generic unknown
native error.

`E_CORE_FFI_BUFFER_BOUNDS`, `E_CORE_INVALID_NATIVE_VIEW`, and
`E_KICK_EVIDENCE_BUFFER_EXHAUSTED` are errors. Rust must reject malformed
pointer/count native views before copying or dereferencing C buffers.

`E_GPU_WORKER_MISSING_MEMORY_TICKET`, `E_GPU_FENCE_EPOCH_MISSING`, and
`E_GPU_UNCONFIRMED_PROBABILITY_SOURCE` are errors. A GPU result without memory
ticket/fence evidence, or without CPU-confirmed trust, cannot source exact
probability, score, spin, coverage, or BuildUp output.

`E_RENDER_RUNTIME_SVG_FORBIDDEN`, `E_RENDER_ASSET_PROVENANCE_MISSING`,
`E_GUI_SUBPROCESS_FORBIDDEN`, and `E_FRONTEND_TYPED_REQUEST_REQUIRED` are errors.
They protect the render asset pipeline and GUI/frontend AppRequest boundary.

`E_C_MEMORY_SCOPE_INVALID` is an error. C memory scope failure means the FFI
views cannot be trusted.

`E_GPU_WORKER_TRUST_MISMATCH` is an error. A GPU worker mismatch cannot source
exact probability, score, spin, coverage, or BuildUp output.

`W_GPU_BACKEND_FALLBACK`, `W_GPU_DEVICE_UNAVAILABLE`, and
`W_HYBRID_BACKPRESSURE_ACTIVE` are warnings when policy permits fallback or
partial backend capability reporting. Strict gates may promote these to failure
when exact GPU execution was required.

`W_BUILDUP_ENUMERATION_TRUNCATED` is a warning only when the request allows
partial results. Otherwise it must be promoted to a failed strict result.

`W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE` is a warning that observed queue
expansion was budget-truncated. It must carry `renormalized=false` and
`probability_complete=false`.

`W_TRACE_RETENTION_TRUNCATED` is a warning that replay trace retention was
bounded while the solution count may still be complete. It must not become a
count truncation reason.

`W_SPIN_CLASSIFICATION_ESTIMATED`, `W_SPIN_TARGET_PROBABILITY_INCOMPLETE`, and
`W_SCORE_EXPECTATION_SAMPLE_ONLY` are warnings.

## Diagnostic Evidence

Diagnostics in this area should include the stable evidence that explains the
decision:

- `score_profile_id`
- `spin_classifier_id`
- `spin_target_id`
- `special_spin_case_id`
- `required_kick_evidence`
- `available_trace_completeness`
- `pattern_universe_id`
- `pattern_weight_model_id`
- `backend_trust_state`
- `gpu_worker_state`
- `gpu_worker_trust_state`
- `memory_ticket_id`
- `fence_epoch`
- `fallback_reason`
- `unavailable_reason`
- `throttle_reason`
- `truncation_reason`
- `row_count`
- `row_limit`
- `word_count`
- `word_limit`
- `variant_limit`
- `kick_evidence_count`
- `kick_evidence_limit`
- `suggested_next_step`

## FFI And Budget Diagnostics

FFI diagnostics must separate lifetime failures from product absence. A pointer
escape, invalid C result buffer, or memory-scope error means Clearra cannot
trust the evidence. A budget diagnostic means evidence was bounded by policy.
Neither case may be reported as "no solution" without the diagnostic evidence.

Score and spin capacity diagnostics must identify the active matrix type:
`ScoreCellMatrix`, `SpinCoverageMatrix`, BuildUp variant enumeration, or
KickEvidence buffer. This makes local policy fallback, strict CI failures, and
GUI disabled states explainable from the same diagnostic report.

## Security Diagnostic Gate

S6 security diagnostics must render through the same `DiagnosticReport` path as
normal validation diagnostics. JSON output includes `contract.diagnostics.items`
with stable `code`, `severity`, `message`, `location`, `evidence`, and
`suggested_next_step` fields whenever diagnostics are present. Text output keeps
the first line compact and adds indented `location`, `evidence`, and `next`
lines only when those fields exist.

Security errors must not be downgraded to warnings. Backend fallback must carry
`W_BACKEND_FALLBACK_USED` or a more specific backend warning when fallback is
allowed. Strict security failures such as unbounded FFI buffers, GUI subprocess
execution, runtime raw SVG rendering, frontend typed-request bypass, missing GPU
memory ticket, or missing GPU fence epoch remain errors.

## Required Tests

- `exact_spin_target_missing_kick_evidence_is_error`
- `approximate_score_missing_kick_evidence_is_warning`
- `unverified_special_spin_profile_reports_disabled_reason`
- `coverage_weight_model_mismatch_is_error`
- `score_sample_only_output_reports_warning`
- `score_matrix_capacity_exceeded_diagnostic`
- `buildup_enumeration_truncation_reports_diagnostic`
- `build_up_count_reports_truncation`
- `coverage_capacity_exceeded_is_error_not_success`
- `observed_queue_truncation_is_not_renormalized`
- `diagnostic_reports_gpu_worker_fallback_reason`
- `diagnostic_reports_gpu_worker_unavailable_reason`
- `security_diagnostic_gate_maps_all_s_stage_errors`
- `security_errors_are_not_downgraded_to_warnings`
- `json_diagnostic_renders_structured_evidence_and_suggested_next_step`
- `json_validation_failure_uses_stdout_json_contract`
