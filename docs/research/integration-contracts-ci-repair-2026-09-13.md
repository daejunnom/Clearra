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
