# Architecture Validation Authority

Clearra separates interface and safety contracts from executed correctness
evidence. Runtime correctness is not inferred from marker presence. SRP is a
design and review guideline in `AGENTS.md`, not an automated acceptance gate.

## Interface and Safety Contract Checks

Source-level checks may enforce these contracts:

- dependency boundary
- forbidden API and forbidden algorithm surface
- public ABI field shape
- unsafe boundary isolation
- unsupported capability contract and exactness disclosure
- NoProductDebt product-source policy with test fixtures and `docs/history`
  excluded by explicit allowlist

These checks concern the actual dependencies, supported interfaces, and safety
contracts. Adding a comment, a type name, or an expected string does not prove
that the corresponding behavior is implemented. Advisory implementation and
release-wiring audits remain separate from executed correctness evidence.

## Code Organization Is a Work Guideline

There is no 1,000-line limit, required `SRP rationale:` comment, or static SRP
gate. File counts, source layout, helper names, and marker text are not SRP
acceptance criteria. Do not restore the former size or wording rules when
updating validation.

The existing `scripts/architecture/validate_srp_policy.ps1` defines
`Assert-PowerShellSourcesParse`. It parses PowerShell sources and reports syntax
errors. `validate_file_size.ps1` supplies the governed file list; it does not
measure file sizes or validate module ownership. The historical script names
do not establish an SRP check, and successful parsing proves syntax only.

Use the working rules in `AGENTS.md` to review cohesion, computational reuse,
and data ownership. Rust performance and memory requirements can justify
keeping or fusing operations across a conventional SRP boundary. Such a choice
must preserve correctness and the interface and safety contracts above; it
does not require a mandated comment or a structural exemption mechanism.

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
