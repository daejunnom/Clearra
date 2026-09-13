# Non-publishing integration CI startup repair

Run [34717601139](https://github.com/daejunnom/Clearra/actions/runs/34717601139),
source `699b902`, completed with failure before the three product test payloads:

- Both Rust jobs ran unowned `cargo fetch --locked`. Fetch also probes rustc,
  so the deliberate unmanaged-build wrapper rejected it. No CLI/PC4 assertion
  ran in those jobs.
- The CTK compiler completed, but its managed owner could not complete because
  PowerShell `Get-Item` omitted a hidden transaction marker on Linux. The
  surface assertion step did not run.

Commit `7a48b51` keeps the canonical build root and native execution policy,
fetches inside each existing owner before its offline tests, and adds `-Force`
only to the already literal/safety-checked metadata size read. A real hidden
marker lifecycle regression was added, including Windows Hidden attributes.
The workflow rejects a bare Cargo fetch through a mutation test.

Focused Node tests: 20 passed. The isolated PowerShell artifact-path lifecycle
suite passed, including hidden-marker reading, five-product retention,
single-experiment retention and preservation of foreign/unowned paths.

Retry [34717896918](https://github.com/daejunnom/Clearra/actions/runs/34717896918)
was observed queued on exact source `7a48b514dfa5518d8d0d5ff7afda66e1535f4f4f`.
Only creation was checked; no successful hosted test result is claimed here.
No main merge, production deployment, release receipt, or Secrets change occurred.

## Hosted test outcome and native context correction

The later bounded check of run `34717896918` found source, surface-contracts
and pc4-contracts successful. Native CLI built and executed its 16 process
tests, with five passing and eleven failing. The helper had enabled both
`native-c-core` and `wasm-cpu-runtime`: CLI `product_app_context` deliberately
chooses its WASM context when the latter is enabled. That is incompatible with
the native C routing/count contracts asserted by this suite.

Commit `044dc3d` removes only the forced-WASM feature from this native helper.
The workflow boundary test rejects reintroducing that combination; its nine
focused tests passed locally. This is a diagnosed context-selection mismatch,
not a claim that all eleven failures have already disappeared.

Retry [34736690362](https://github.com/daejunnom/Clearra/actions/runs/34736690362)
was observed in progress on exact source
`044dc3ddbf5d54095fc91a690dad8c12b068bcd2`. Only its creation was checked here.
It also includes `382edfb`'s newer nonempty HF observations, which were not in
the previous hosted PC4 success. This remains isolated, non-publishing CI.

## Product contract drift, multi-edge fixture budget, and jobserver forwarding

A subsequent bounded check found run `34736690362` completed: source, PC4 and
surface jobs passed; native CLI improved to 11/16 passing with five failures.
Run [34737351001](https://github.com/daejunnom/Clearra/actions/runs/34737351001)
on `4a1d899` reproduced those five failures, passed surface checks, and exposed
three new App fixture failures (core 7 and TB 156 passed; App 61 passed/3 failed).

Source comparison identified obsolete process-test expectations, not reasons
to restore the old product behavior:

- Native `buildup_witness_from_c_results` reports accepted field candidates,
  not build-order/rotation witness multiplicity. `IIOOO` in a 10x2 rectangle
  has four fields: aligned horizontal Is at x=0,2,4,6 and three Os in the other
  columns. The old 1536 total/64 retained expectation is replaced by exact
  four-field/four-retained assertions with unique count and completeness checks.
- The one-I scenario has one solution. The existing Core independent identity
  test explicitly rejects storing the last current piece in an empty hold
  without a next piece; the process tests still expected two.
- Redesigned Setup uses an unordered remainder and ranked-family v2 output.
  An independent process test now exercises `setup-finder` and its `setup`
  alias, and checks that obsolete `--fixed` remains rejected. It no longer
  demands the old generic SearchProblem route for the redesigned command.
- Plain unsupported output is intentionally public-safe; explicit verbose
  output retains the inspection hint. Both are now checked independently.

The new 2L through 4L App fixtures accidentally used both traversal and
materialization edge budgets of one. Their budgets now match the exact path
length; observation page size remains one. The App session also preserves the
typed budget stage, limit and attempted size instead of collapsing every such
failure into the same reason string. No product resource limit was raised.

The Linux logs also showed a broken Cargo jobserver. The Node rustc wrapper
inherited only descriptors 0-2, closing the two additional token-pipe FDs while
forwarding `CARGO_MAKEFLAGS`. The wrapper now passes only a verified matching
pipe pair, retaining Cargo's parallelism authority; it rejects stale or
unrelated handles and leaves Windows named-semaphore/FIFO-name transport alone.
This follows [Cargo's jobserver contract](https://doc.rust-lang.org/cargo/reference/build-scripts.html#jobserver)
and [Node's explicit stdio FD forwarding](https://nodejs.org/api/child_process.html#optionsstdio).
An actual two-process pipe/token test is included in the Linux source job.

Local Node checks: 12 passed and the real POSIX pipe test was skipped on Windows.
The PC4 authority mutation suite and diff/format checks passed. No local Rust
execution is claimed after the already verified application-control block.
Hosted execution of this correction remains pending until its exact-source
test run completes; no release/activation claim is made here.
