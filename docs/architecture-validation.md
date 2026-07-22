# Architecture Validation Authority

Clearra separates static architecture authority from executed correctness
evidence. Runtime correctness is not inferred from marker presence.

## Release-Blocking Static Checks

Static marker or source scans may block release only for these contracts:

- dependency boundary
- forbidden API and forbidden algorithm surface
- public ABI field shape
- unsafe boundary isolation
- unsupported capability contract and exactness disclosure
- NoProductDebt product-source policy with test fixtures and `docs/history`
  excluded by explicit allowlist

The default `Validate` task also checks SRP source structure. That structural
check is not marker-based solver-correctness evidence. Advisory implementation
and release-wiring audits remain individually callable, but they are not part
of the default release-blocking validation set.

The SRP task validates physical module ownership rather than counting marker
strings. It checks that the BuildUp, JSON, GPU worker, and spin-target test
owners index concrete behavior modules while executable cases live in those
modules. It parses Product E2E PowerShell with the PowerShell AST and verifies
that build, assertions, run/cases, and report functions are owned by their
corresponding stage files. It also rejects
`mod helper_*` shells, action-per-file TypeScript helper clusters, and
1,000-line modules without a permanent single-change-reason rationale.

Examples of claims that marker presence cannot prove:

- BuildOrders and HoldReachableOrders types do not prove an independent
  language intersection implementation.
- Proof-carrying pruning types do not prove that a hot path prunes safely.
- PostProcess GPU types do not prove a connected backend. The executed WebGPU
  batch and deterministic CPU confirmation provide that evidence; static scans
  enforce only its public outcome and API boundaries.

## Executed Correctness Authority

`ReleaseAcceptance` starts with `NoProductDebt`, then runs
`AdversarialCorrectness`, C sanitizers, Rust exact suites, Product E2E, WASM,
desktop, and renderer evidence. Build-only, non-executed, zero-test, and
filtered-without-a-match outcomes fail the gate.

`NoProductDebt` also executes product-level probes. Native-unavailable requests
must return an explicit error, WASM and Tauri must return real AppResponse data,
score summary must materialize a nonzero profile matrix, CompleteRequired evidence
pressure must keep the candidate, HoldLanguageEmpty must remain impossible to
construct without an independent engine proof, and the connected renderer must
produce byte-checked PNG/GIF artifacts.

| Contract | Executed evidence |
| --- | --- |
| Forced hash collision | C packing frontier collision test and Rust candidate hash-collision test |
| Distinct hold states with same hash | C BuildUp failed-memo exact-key test |
| Reachability capacity exhaustion | C incomplete-vs-impossible status test |
| Alternate successful BuildUp order | C actual-order export and Rust legal replay test |
| Nonuniform pattern weights | Rust objective probability test |
| All-pattern minimum cover | Rust required-pattern cover test |
| Ledger strict retention | C pruning suite plus Rust domain and executor CompleteRequired tests |
| Same candidate, different pattern execution | Rust ExecutionVariantSet retention test |

The C harness is `clearra_adversarial_tests`. The runner requires each named
Rust case to appear as passed in its complete library suite. It executes the suites for the
core domain, executor, FFI, coverage, and guarded PostProcess GPU crates so the
focused proofs do not narrow the surrounding regression gate.
