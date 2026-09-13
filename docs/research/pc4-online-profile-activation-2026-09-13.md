# Independent online PC4 profile qualification and public preview

## Scope and evidence levels

This extends, rather than overwrites, the historical
[4194 boundary record](pc4-empty-p7p4-4194-2026-09-13.md). The user explicitly
confirmed that upstream Jstris 180 is complete. A bounded compatibility probe
does not independently prove every upstream edge or every requested solution.
The earlier 13 unresolved observations remain in the
[nonempty differential record](pc4-hf-nonempty-differential-2026-09-13.md).

The user supplied the result-set reference **456,459 solutions** for empty
field / 4L / Jstris 180 / P7P4. Treat it as user-provided, not independently
reproduced yet. Comparison must use canonical solution fields, not graph node
count, raw paths, hold permutations or counts aggregated over source queues.
If needed, [Wirelyre tetra-tools](https://github.com/wirelyre/tetra-tools)
provides the independent comparison reference. Its README distinguishes the
web PC interface, 10-piece legal-board generator and SRS 4L library; matching
kick/hold and solution normalization is required before comparing counts.
No tetra-tools implementation or precomputed solution set was imported.

## Implemented responsibilities

- `scripts/release/pc4/discover-upstream-generation.mjs` resolves moving `main`
  to a public immutable dataset revision, then inventories that revision.
- `qualify-upstream-generation.mjs` independently evaluates five profile slots.
  Jstris 180 has an explicit completion declaration. Matching header layouts,
  index lengths/counts, ordinal records, offset boundaries and bounded graph
  samples are the host reader's automatic compatibility check.
- Each job pins the result. No production fixed digest allowlist is used.
  A five-minute readiness cache refreshes between jobs, never mid-job.
- The Range reader accepts exact 206 bodies only, rejects whole-file 200
  responses without consuming them, limits streamed bytes, caches bounded
  fragments in memory, and distinguishes cancellation/rate limits/offline.
- The WASM configuration consumes a trusted host qualification receipt. It is
  explicitly not an upstream signature or an independent completeness proof.
- `Pc4OnlineHostExecution` reuses the CLI/App request compiler, graph candidate
  session, materializer, candidate seal and existing product reducer. The host
  supplies network bytes, not calculated solutions. No offline fallback is
  started implicitly, including when an online host receives a different
  unsupported product.
- Public PC4 labels no longer say beta in EN/KO/JA. The separate dependency-DAG
  beta is unrelated and unchanged. Lookup progress counts HTTP requests, not
  Geometry nodes or estimated solutions.

## Live discovery observation

Observation revision: `ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`. This is an
evidence identity, not a product pin. The probe took **14,794.9774 ms** and read
**1,468 binary bytes** (metadata and HTTP headers excluded).

| Profile | Host readiness | Reason |
| --- | --- | --- |
| Jstris 180 | ready for 4L PC | Own canonical index pair and graph agree |
| SRS / no180 | unavailable | Profile-specific index pair absent |
| SRS+ | unavailable | Profile-specific index pair absent |
| SRS-X | unavailable | Profile-specific index pair absent |
| No kick | unavailable | Profile-specific index pair absent |

Canonical graph/index observations: 15,185,706 records; 510,917,451-byte graph;
121,485,664-byte field index; 60,742,844-byte offset index. The first node is
empty and the last is the qualified four-full-row terminal. No full graph or
index was downloaded. No value, policy, V* or Krylov asset was requested.

These readiness numbers are **not search timings**. Other profiles are not
enabled by borrowing the Jstris index. Setup, CLI/Discord online adapters and
release activation remain separate v0.9.0 work.

## Validation checkpoint

Rust source: `6ca1dde` (full identity is in the CI run).
[Non-publishing integration run 34753390612](https://github.com/daejunnom/Clearra/actions/runs/34753390612):
All five jobs passed: source, surface contracts, native CLI process tests,
PC4 contracts and the explicitly requested local-preview WASM producer.
The PC4 job includes 182 tablebase tests, two separately selected A/B tests,
88 App PC4 tests, two online WASM configuration tests, compact-source tests and
16 replay App tests. Ignored work is not counted as a pass. The new synthetic
host test sends both fixed and pattern inputs through Range admission into the
shared product; it also rejects a response with a substituted request ID.

Local focused checks passed: 18 discovery/qualification/Page tests; six
progress/I18N tests; three host transport tests; local-profile, progress-model
and ordinary host-yield TypeScript contracts. Svelte PC controls and workspace
compile in memory without warnings. These are not production release receipts.

## Browser run

The non-publishing producer's artifact `10316359268` was verified and imported
into 4194 using the repository's verified import path. Both the served
generation endpoint and the browser load use this exact artifact:

- Rust source/engine: `6ca1dde572979b1fd4201e045fc34af80a2503de`
- Rust source fingerprint: `c8215b5fa91600c444113e263755d6dcdde121824d7f55365298112ee4540103`
- WASM SHA256: `12019d54adaaf175029484f1beb5df83e2976fb93e7de01e3e32b914b7e661c9`
- WASM byte length: `21467266`
- Binding SHA256: `7ce39fef3b4ef814b9689538bbf5eca594e22b9e8d176b46e24f2dc98c160ecc`

These are observed local-preview identities, not hardcoded upstream pins or
production release authority. No new local Rust build was used. The 4194
server runs hidden in local-recovery mode, without hot-refreshing an active
search. The browser displays Jstris 180 ready and the four other profiles
unavailable, with no PC4 beta label.

### Completed real-HF canary

Input: four rows with columns 2 through 10 filled, column 1 empty; queue `I`;
4L; Jstris 180; hold off; all solutions; online PC4 selected.

- Result: **one solution**, rendered in the GUI, with copy controls.
- GUI elapsed: **44.4 seconds**.
- Host module preparation: **14,146.1 ms**.
- Online execution observation: **30,143.2 ms**, **31 HTTP requests**,
  **275 response-body bytes**.
- Host elapsed to terminal: **44,298.1 ms**.

The terminal was success, not an offline fallback. This is one real online
end-to-end success, not an independent proof of all graph edges or all inputs.
HTTP/body counters exclude metadata/headers and are not CPU-only timings.

### Empty field / P7P4 observation

Input: empty field; 4L; P7P4; Jstris 180; hold on; all supplied queue visible;
all solutions; online PC4 selected. Ordinary host capacity was 11 slots, with
the all-logical-processors option off. The online Range owner is not a
Geometry worker pool, so this is not an 11-worker Geometry benchmark.

- At **215.4 seconds**, progress showed 196 HTTP requests and 4,710 body bytes.
- The run was **manually cancelled at 244.8 seconds** while still in online
  partial lookup. Final telemetry: **224 HTTP requests**, **5,226 body bytes**,
  **244,770 ms** online elapsed.
- The UI reached `cancelled` and remained usable; no automatic fallback or
  background continuation was left running.
- No complete solution universe was returned, so **456,459 is still only the
  user-supplied reference**. Neither count equality nor field-set equality was
  established. Cancellation time is not a successful search time.

The current host awaits each requested range before the next Rust advance.
Initial field-hash binary search, ordinal-field lookup, offset-pair lookup and
graph-record lookup consequently incur sequential HTTP round trips. Its
bounded cache reuses exact `(content identity, path, offset, length)` fragments,
not containing/adjacent ranges. This explains why a tiny transferred body can
still take minutes; the observation does not establish that network is the
only remaining cost after lookup batching improves.

Remaining performance work must preserve bounded partial access, immutable
per-job identity, cancellation and exact candidate sealing. Candidate directions
are reuse of containing ranges, demand-derived coalesced index reads, and an
independent bounded I/O frontier for multiple known graph requests. Do not
silently download the whole graph/index, introduce a product hard timeout,
return a sampled count, or call offline computation a TB benchmark.

The preview proves activation and a completed small request. It does **not**
close large-pattern usability, whole-universe differential acceptance, setup,
CLI/Discord online adapters or v0.9.0 production release acceptance.
