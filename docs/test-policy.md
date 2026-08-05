# Test Policy

Clearra has two explicit execution surfaces. The runner never infers one from
an error message and never changes surfaces after a failed launch.

## ManagedLocal

`ManagedLocal` is the default developer surface:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task Local
powershell -NoProfile -File scripts/clearra.ps1 -Task ProductE2E
powershell -NoProfile -File scripts/clearra.ps1 -Task NativeLocal
```

The surface deliberately has no generated test executable launch:

- Cargo uses `fmt` and `metadata --no-deps`; it does not compile workspace
  source, tests, build helpers, proc macros, or product binaries.
- CMake configures the library cache with `BUILD_TESTING=OFF`; CTest programs
  are neither generated nor launched by that run.
- ProductE2E checks the typed source contract without building its test harness
  and reports `product_e2e_route=source-contract`.
- NativeLocal builds only the C static library.
- Architecture validation is static PowerShell analysis.

A ManagedLocal pass reports `rust_test_execution=not-built`,
`c_core_test_execution=not-built`, and `policy_fallback_used=false`. It is a
source, metadata, C-library, and boundary gate, not executed release evidence.

Library-only consumers share `core-c-library-cache`. Executed CTest owns the
separate `core-c-test-cache` (or an explicitly named sanitizer/split cache), so
a library build cannot refresh or accidentally reuse a CTest executable graph.

## Trusted

`Trusted` requests repository-built process execution on a host that permits
it:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task ProductE2E -ExecutionSurface Trusted
powershell -NoProfile -File scripts/clearra.ps1 -Task Strict -ExecutionSurface Trusted
powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted
```

Trusted commands execute each requested test process once and fail closed. On
Windows, the runner first reads the UMCI state. Enforced UMCI makes the local
source surface compile-only, so the task stops before Cargo/CMake creates or
launches another transient executable. Executed evidence then requires a
release-pipeline-built, enterprise-approved package or a runner where source
artifacts are permitted. There is no launch retry, compile-only
substitution, local signing, file unblock operation, copied executable, or
policy-dependent fallback. `Strict`, `Acceptance`, `ReleaseAcceptance`, UX,
desktop, worker, sanitizer, adversarial, WASM, renderer, MVP2, and MVP3
execution tasks reject `ManagedLocal` before producing or launching their test
surface.

Windows source builds do not inject `/MANIFESTUAC`, `/MANIFEST:EMBED`, or a
runner-generated execution-level resource into Cargo and C test binaries.
Unpackaged console tools use the toolchain's ordinary process surface; the
release packaging owner supplies the reviewed application manifest together
with package signing and provenance. The runner does not request elevation,
sign generated test binaries, retry through another process surface, or weaken
application-control policy.

`Local` does not emit and probe hundreds of native Rust/C test processes.
Trusted gates execute their requested native surfaces once only when the host
permits source-generated executables. Device Guard is an execution-surface
boundary: enforced UMCI rejects the source-generated path before build, while a
reviewed prebuilt artifact is checked for a valid signature and still leaves the
final launch verdict to Windows. A blocked or preflight-rejected native test is
not replaced by WSL, WASM, or compile-only evidence. Release acceptance remains
blocked on a host that cannot execute its required native surfaces.

The execution choice is inherited by child scripts through
`CLEARRA_EXECUTION_SURFACE`. Direct process scripts such as UX, product E2E,
desktop, worker, WASM, WebGPU, and targeted Rust tests reject an absent Trusted
surface. The shared native runner also guards `cargo test`, `cargo run`, and
rooted generated `.exe` launches as defense in depth.

`Trusted` is not a policy bypass. The desktop gate records
`Win32_DeviceGuard`; enforced UMCI returns
`E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE` before a local Cargo
or C test image is produced. A policy transition race or independently launched
tool that reaches Windows process creation still preserves the actual 4551
evidence as `E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED`. Neither result is retried
through WSL, signed or unblocked locally, or reinterpreted as passing evidence.

The canonical CTest runner checks UMCI both before generating a test surface and
immediately before CTest execution. If Windows changes policy between those
checks, the runner correlates the failed launch with Code Integrity events 3033
and 3077 and reports the blocked artifact, parent process, and policy ID. Direct
`ctest` invocation is not a supported way to evade this boundary. On enforced
UMCI, local unsigned test images remain compile/link evidence only; native
runtime acceptance requires an enterprise-approved release artifact and is
never reported as passed by the compile-only path.

## Task Meaning

| Task | ManagedLocal | Trusted |
| --- | --- | --- |
| `Local` | format/metadata, C library-only build, product source contract, architecture validation | native C/Rust tests; an actual policy block fails rather than switching execution surface |
| `ProductE2E` | source contract; no Rust compilation or product launch | native-linked `clearra_cli::run_with_args` contract; product binary still not launched |
| `ProductE2EBuilt` | rejected before compilation | build and execute process E2E once |
| `NativeLocal` | C static-library build only | CTest plus native core-executor tests |
| `COnly`, `COnlySplit`, `COnlyAsan`, `COnlyUbsan` | corresponding C library configuration only | corresponding CTest execution |
| `Validate` | static architecture validation | same static validation |
| `DesktopHost` | rejected before execution | application-control diagnostics, in-memory UI compile, WASM CPU GUI-host async E2E, and one Tauri compile attempt |
| `Strict`, `ReleaseAcceptance` | rejected before execution | mandatory executed evidence; any failure blocks release |

`ReleaseAcceptance` runs `NoProductDebt -> AdversarialCorrectness ->
CSanitizer -> RustExactTests -> ProductE2E -> WasmBuildTest -> DesktopHost ->
RenderGolden`. It cannot pass with build-only, zero-test, unavailable toolchain,
or policy-skipped evidence. In short: release acceptance cannot pass when an
executed security or correctness gate fails.

## Output Contract

Gate summaries distinguish what happened:

- `rust_test_execution=not-built | launched`
- `wasm_exact_execution=not-run | launched`
- `c_core_test_execution=not-built | launched`
- `product_e2e_route=source-contract | library | process`
- `native_c_binding=disabled | enabled`
- `architecture_validation=passed | failed`
- `policy_fallback_used=false`
- `tauri_compile_attempted=true | false`
- `application_control_umci=off | audit | enforced | unknown`

Build/cache artifacts use the platform Clearra artifact root. Reports use the
platform report root. Repository-local `target`, `build`, and report output are
forbidden. `_local/bundle.py` and its review bundle output are the sole narrow
`_local` exception.

## Search Stage Profiling

High-detail C search timing is an opt-in diagnostic build surface. Configure it
with `CLEARRA_ENABLE_STAGE_PROFILING=ON`, initialize one
`clr_search_stage_profile` object, and activate that object only around the
search being investigated. The default product build keeps the option off; its
profiling calls compile to inline no-ops and do not read a clock or attach a
thread-local recorder.

The profile separates supply materialization, Packing validation/allocation,
operation-table and support-index construction, per-depth expansion/reduction,
candidate line-order filtering, BuildUp memo/hold/y-adjustment/reachability, and
resource release. Per-depth records include frontier input/output and whether a
depth ended incomplete. Counter-only stages report work volume without adding a
clock read to every candidate.

Stage spans are nested and therefore are not additive. Active profiling adds
clock and counter overhead to wall time, so it diagnoses relative stage weight
and work amplification; performance acceptance uses a profiling-disabled build
and compares repeated baselines. A capacity, cancellation, or fatal return must
flush the active profile before the diagnostic process exits.

## Test Matrix Evidence

### T1 C core unit fixture matrix

Trusted C gates cover `memory_tests`, `board64_tests`,
`board_backend_dispatch_tests`, `operation_table_tests`, `rule_profile_tests`,
`supply_tests`, `cache_identity_tests`, `candidate_tests`,
`reachability_tests`, `packing_tests`, `gpu_tests`, `scheduler_tests`,
`buildup_tests`, `coverage_tests`, and `scoring_event_tests`. Aggregate and
split CTest plus `COnlyAsan` and `COnlyUbsan` provide the sanitizer matrix.
`capacity_exceeded_tests_pass` is required.

### T3 coverage probability invariants

The executable suites pin `coverage_row_rejects_universe_mismatch`,
`coverage_row_rejects_piece_source_mismatch`,
`coverage_row_rejects_weight_model_mismatch`,
`coverage_union_does_not_sum_variant_probability`,
`build_coverage_uses_union_probability`, and
`observed_queue_truncation_not_renormalized`. PatternBitSet OR is the only
probability union source.

### T4 product E2E golden tests

Trusted ProductE2E covers `pc_2l_fixed_queue`, `pc_4l_fixed_candidate_budget`,
`scenario_clear_to_empty`, path, percent, setup, cover, continuation, rules,
diagnostics, `render_capability_exact`, and the `DesktopHost` product boundary.
JSON and text goldens remain separate.

### T5 security regression tests

Release evidence includes
`memory_context_double_release_does_not_deref_freed_memory`,
`ffi_kick_evidence_count_exceeded_rejected_before_pointer_read`,
`gpu_worker_missing_memory_ticket_rejected`,
`gpu_buffer_release_without_fence_rejected`,
`gpu_unconfirmed_probability_rejected`, `runtime_raw_svg_rejected`,
`gui_subprocess_forbidden`, and `wasm_user_shader_rejected`.

### T6 MVP2 Acceptance Tests

The trusted MVP2 gate pins `score_profile_reports_accuracy_level`,
`tetrio_not_profile_specific_exact_until_exact_supported`,
`spin_target_requires_classifier`,
`missing_kick_evidence_is_incomplete_not_exact`,
`max_score_cover_does_not_double_count_probability`,
`setup_raw_metrics_no_condition_summary`, `renderer_connected_exact`,
`renderer_capability_matches_runtime_report`, and
`render_status_ui_uses_product_capability`.

The X10 MVP2 Acceptance Gate runs MVP1 ProductE2E first and enforces
`mvp2_acceptance_runs_mvp1_product_e2e_first`, `mvp2_exact_claims_guarded`,
`mvp2_scoring_basic_approximation_disclosed`,
`mvp2_renderer_exact_only_when_supported`, and
`mvp2_gpu_fallback_reason_visible`. MVP2 feature failure must not break MVP1
pc/path/percent.

### T7 MVP3 Acceptance Tests

The trusted MVP3 gate pins `custom_piece_schema_validates_but_runtime_guarded`,
`mixed_piece_area_multiset_feasibility`,
`missing_cells_mod_4_not_used_for_generic_feasibility`,
`board128_descriptor_tests`, the Board256 fixed-word contract,
`wide_board_runtime_not_connected`,
`custom_bag_not_silent_standard_fallback`, and
`generic_cache_key_includes_piece_definition_id`.

The G11 MVP3 Acceptance Gate enforces
`standard_fast_path_unchanged_under_mvp3`,
`custom_features_guarded_until_runtime_connected`,
`no_silent_fallback_to_standard_path`, and
`generic_cache_keys_include_piece_board_rule_supply_identity`.

## WASM Exact CPU Search Scope

The WASM product runtime executes `OpeningPc` and `ScenarioPc` on `Board64` for
fixed, materialized, and standard-bag PieceSource inputs with a piece window of
1 through 15. Acceptance compares canonical candidate count, normalized
solution-set hash, pattern coverage, and completeness for PCO, Tsar Cannon, and
bounded Full 4L searches. Score summary without a complete matrix reports
`score_matrix_not_materialized`; a resource limit is reported as incomplete.

WASM search is cooperatively sliced by `advance_job(work_budget)`. Cancellation
must be observed between Packing/BuildUp slices, release the active scope, and
must not emit a final probability.

## Progress Output

Each progress scope renders `[scope] done | running | pending | failed | workers`.
Acceptance progress delegates to child scopes: parent scopes only show their own worker count, and child scopes report their own worker counts.
`VerboseLog restores command output`; `ShowCases restores per-case output`;
long native commands provide a `Native command heartbeat`.
