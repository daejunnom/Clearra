# Desktop Runtime and Non-PC Follow-up

Date: 2026-08-02

This follow-up audits the post-v0.5.1 worktree after the Sfinder command-role
review. Custom piece, bag, and board runtime configuration and the documented
scoring restrictions are intentional product limits. They are not classified
as missing Sfinder compatibility and were not expanded.

## Tauri Scope Recovery

The `v0.5.1` desktop route rendered only `SolverWorkspace`, even though the
shared product tabs already advertised six tools. The current GitHub `main` and
tag point at the same commit, so this was an existing v0.5.1 product-boundary
defect rather than a later upstream regression.

The desktop entry now dispatches all six modes:

- PC search;
- setup finder, including lazy exact solution-path expansion;
- build probability;
- damage search;
- spin finder;
- CTK drawer.

Setup, build probability, damage, and spin finder use typed JSON to construct
their native `AppRequest`. `run_request`, `validate_request`, and `start_job`
share that dispatcher. CLI text fields are rejected, and no desktop route starts
or parses the CLI. The completed job event now includes the same structured
search-report schema consumed by the browser UI. A non-success `AppResponse` is
shown as a failed job rather than a green completion.

The desktop setup UI previously rejected path-detail requests before reaching a
Rust parser that already supported them. Path expansion now runs as a separate
typed job, polls and drains its own events, supports cancellation, and retains
the main candidate report while attaching only the selected candidate's paths.

## Result and Job Lifetime

Browser and desktop result stores are process-wide, while result board height,
mask, aggregation, and score-mode snapshots live in individual workspace
components. Retaining a terminal report across a tool route could therefore
replay correct search data against the wrong local snapshot. Idle and terminal
results are now cleared on workspace disposal, and tool tabs are disabled while
a main job or setup path-detail job is active. Normal mobile navigation uses
SvelteKit routing instead of a full WebView reload.

The native bridge also reaps an already-finished job before accepting the next
job request. This covers a lost terminal poll without accepting a second job
while the prior worker is still running. A full reload during an actively
running native job still has no active-job discovery API; recovery for that case
would require a new explicit lifecycle contract rather than guessing the lost
request identity.

## Memory and Worker Accounting

Large setup and forward reports were first serialized to a JSON string, parsed
again into a `serde_json::Value` tree, and then serialized into the job-event
array. Job-event serialization now borrows `serde_json::value::RawValue`, so the
large report is validated and embedded without constructing that duplicate
tree. The wire schema is unchanged. This targets peak host memory rather than a
small instruction-level speed optimization.

The browser verifier pool previously reported its requested worker count instead
of the verifier count that actually completed finalization. `workers_used` now
comes from the finalized verifiers plus the coordinator. A progressive
"dispatch after the first verifier is ready" prototype was measured and rejected,
so initialization still completes for the selected pool before the first batch
is assigned. The exact candidate merge and pruning logic are unchanged; the
accounting fix only makes performance and resource reports truthful when a plan
adaptively selects fewer verifiers.

Desktop damage and spin currently execute the exact serial
`ForwardSearchSession` path. The typed worker budget is preserved, but the native
application command has no native thread driver for the existing
`ForwardParallelCoordinator`; only the browser distributed runtime currently
drives its wire tasks and merge. A new driver was not introduced without a
differential exactness contract; the UI must not claim the worker budget is
already consumed by that native path.

The browser build-probability command also requests a CPU warmup while the
desktop typed request currently leaves that option off. It changes first-run
performance policy, not result identity. Because build search was explicitly
outside this follow-up's optimization scope, it is recorded rather than changed;
web and desktop timings must not be pooled as the same warmup condition.

## Optimization Decisions

The two-run v0.5.1/current engine comparison and speed-sorted table remain in
[`sfinder-command-role-audit.md`](sfinder-command-role-audit.md). PC and build
search cores were not changed in this follow-up.

An exact setup visible-seven summary evaluator was prototyped to avoid retaining
the selected pattern vector and bitset. The representative command was:

```text
clearra setup-finder --remaining IOTS --queue-knowledge visible-7 \
  --setup-length longer --max-setup-pieces 2 --no-tablebase --workers 1
```

The baseline runs took 61,006.73 ms and 67,688.45 ms. The candidate run took
61,946.99 ms, but all runs failed at the same
`wasm_observation_policy_storage_unavailable` point and reported the same
3,011,837,952-byte WASM memory. Inspection showed the dominant allocation was
the observation-policy memo table, not the removed materialization. The
candidate was reverted instead of retaining an unmeasurable micro-optimization;
no new pruning was added.

Previously rejected reachability caching, first-ready dispatch, and coverage
summary rerouting remain rejected under their original research records. They
must not be re-applied without a new representative corpus and evidence.

## Validation Evidence

- desktop production SvelteKit build: passed, 3,998 SSR and 4,032 client modules;
- in-memory desktop UI compile: 15 changed Svelte and 7 TypeScript sources,
  six tool-route markers, no artifact write;
- UI Node contracts: 3/3 passed;
- `clearra-gui-host` feature compile with `wasm-cpu-runtime,webgpu-search`:
  passed;
- GUI host tests before the final serialization-only change: 34/34 passed;
- setup queue-observation tests after reverting the ineffective prototype: 3/3;
- setup-finder tests after revert: 9 passed, 4 existing ignored;
- verifier-pool contracts and web TypeScript check: passed;
- `git diff --check`: no whitespace errors.

The final GUI-host test executable compiled but Windows application control
blocked execution with OS error 4551. The feature `cargo check` and formatting
gate passed after the final Rust changes. A direct check of the out-of-workspace
Tauri crate was also attempted, but the same policy blocked the newly built
`quote` and `serde_core` build-script executables with error 4551. No SSH key or
credential file was read.
