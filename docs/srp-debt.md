# SRP Policy

Clearra applies SRP at the cohesive module boundary. A module has one reason to change;
a function, method, assertion, or marker does not automatically need a
separate physical file.

## Cohesion Rules

- A public domain type or public contract may own a dedicated file when it
  forms a stable API boundary.
- An independent algorithm stage, backend, or validation policy owns a module
  because it has an independent change reason.
- Private helpers stay with their cohesive owner when they implement the same
  invariant or state transition.
- Constructors, accessors, and small functions that manipulate one parent state
  stay together. A one-function private helper cluster is file-level
  fragmentation, not SRP.
- Related state-transition code should remain readable as one module. Roughly
  100-400 lines for domain modules, 150-500 for orchestrators, 150-600 for
  algorithm stages, 200-700 for behavior tests, and 100-500 for validation
  tasks are useful review ranges, but size alone is not SRP debt.
- Large behavior may use a larger file or split into named submodules by actual
  responsibility. Line wrapping or arbitrary line limits are not design tools.
- Tests group domain behavior in one test module and keep individual cases as
  test functions. A file per test case is not required.
- Architecture validation groups checks by contract area. Marker-per-file validation is forbidden.
- Rust `include!` implementation trees, C source-includes, and generated
  `impl_001_methods` or `_functions` trees are not module boundaries.
  Method-per-file fragmentation is forbidden.
- Empty inherent `impl` shells, declaration-per-header `_api` trees, and
  include/comment-only C translation units are generated residue, not
  ownership boundaries.
- CMake source and test manifests remain cohesive lists. A directory of
  file-per-section manifest snippets is forbidden.
- Rust integration-test support lives below its owning test. A root-level
  `*_support.rs` file becomes an unintended Cargo test target.
- A file above 1,000 lines must carry an `SRP rationale:` that names its one
  behavior-level change reason. This is a permanent design explanation, not an
  exemption or deadline; temporary large-file exemption is forbidden.

## Behavior Module Boundaries

The high-risk test surfaces are grouped by observable behavior:

- BuildUp runner: native behavior, coverage source, objective reduction,
  execution retention, and replay trace. Memo-key safety remains independently
  owned by `buildup_memo_key_tests.rs`.
- JSON output: render, diagnostic/replay, PC score, PC trace/continuation, PC
  backend/result, and incomplete-resource contracts.
- GPU worker: descriptor, scheduling, trust/fallback, and memory lifetime.
- Spin target: recognition, Unknown policy, coverage probability, kick
  evidence, and replay execution.

The owner test files contain shared fixture construction and module declarations;
the behavior files contain executable cases. A case-per-file directory is not a
valid replacement for these modules.

Product E2E follows build, run, assertions, and report stages.
`product-e2e.ps1` owns parameter handling and stage composition only. Build and
binary resolution, marker/golden assertions, command/backend execution, and
report emission each have an
independent script module. The one-function report module is retained because
report emission is an independently reusable terminal stage, not a helper
fragment.

## Review Questions

Reviewers should ask:

1. Does the file have one coherent reason to change?
2. Do its private functions implement the same invariant or transition?
3. Would splitting improve ownership, or merely force readers through include
   order and filesystem hops?
4. Are tests grouped around observable domain behavior?
5. Does an architecture check validate behavior and boundaries rather than the
   mere presence of a string in a tiny file?
6. If a module exceeds 1,000 lines, does its permanent `SRP rationale:` explain
   a single change reason without a temporary exemption?

## Governed Surface

The gate covers source, tests, and scripts under `crates`, `core-c`, `scripts`,
`tools`, `apps`, `packages`, and `gui`. Generated and dependency directories are
excluded. Very large files produce a review warning, not an automatic failure.

The structural gate rejects known over-fragmentation forms while allowing
public domain modules and real dependency boundaries to remain separate.

## Consolidation Baseline

The July 2026 consolidation reduced the governed code surface from roughly
19,791 entries with a median size near 9 lines to 2,102 source files with a
65-line median. Files at or below 10 lines fell to 214, and files at or below
20 lines fell to 383. These numbers are a review baseline, not size limits.

The remaining short files are primarily public identifier/domain types,
module export boundaries, and tool configuration. The intentional
`core-c/tests/test_board64.c` implementation include remains an aggregate test
adapter; production C implementation includes are forbidden by the gate.
