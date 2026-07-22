# Release Blocking Rules

Release-facing gates must fail when any release-blocking correctness or safety
condition regresses. The release path requires `-ExecutionSurface Trusted` and
is:

`ReleaseAcceptance -> NoProductDebt -> AdversarialCorrectness -> C sanitizer -> Rust exact tests -> ProductE2E -> WASM build/test -> Desktop build/test -> render golden`.

`GpuWorkerRelease` and `WorkerRelease` also include the release path and add
their stricter GPU/worker checks. Static checks own dependency/API/ABI/unsafe,
unsupported-capability, and product-debt surfaces. Runtime correctness is owned
by executed adversarial and product evidence; marker presence alone cannot make
a release pass.

## Blocking Conditions

The following conditions block release:

- `MITM PC backend marker found`
- `PackingBfsState owns queue/hold/full trace`
- `BuildUp memo key missing deleted_line_state`
- `BuildUp memo key missing hold_automaton_state`
- `ShapeKey drops TilingVariant`
- `TilingKey drops BuildVariant`
- `representative order used as coverage proof`
- `verify_first used as coverage proof`
- `target-frame support pruning applied globally`
- `PruneReason lacks proof evidence`
- `Unknown spin treated as False`
- `SpinProfile single-object regression`
- `Fin modeled as kick table`
- `postprocess changes PC coverage probability`
- `resource cap output marked complete`
- `runtime raw SVG rendering`
- `GUI subprocess shortcut`
- `WASM subprocess/process shortcut`
- `unconfirmed GPU result sources exact probability`
- `C memory double-release regression fail`
- `FFI pointer/count bound regression fail`
- `coverage probability invariant fail`
- `silent GPU fallback detected`
- `exact probability from unconfirmed GPU detected`
- `condition_summary field reintroduced`
- `renderer pixel/provenance golden regression`
- `custom piece silent fallback detected`
- `GUI subprocess detected`
- `raw SVG runtime rendering detected`

## Required Gates

The blocking rules are pinned by architecture validation:

- NoProductDebt blocks fixture solver fallback, non-product final responses,
  speculative ABI, dead legacy validation, hardcoded candidates, zero score
  matrices, and externally fabricated pruning proof.

- T3 coverage probability invariants block probability regressions.
- T5 security regression tests block C memory, FFI pointer/count, GPU trust,
  GUI subprocess, WASM shader, and raw SVG regressions.
- T6 MVP2 acceptance tests block `condition_summary` reintroduction and
  renderer pixel/provenance exactness regressions.
- T7 MVP3 acceptance tests block custom piece/custom bag silent fallback and
  generic cache identity regressions.
- U1/U4/U5/U3 architecture contracts block silent GPU fallback, renderer
  exactness shortcuts, asset import shortcuts, and GUI host boundary shortcuts.
- U Test / Acceptance / Release Gate pins architecture, data-structure,
  algorithm, pruning, probability, spin/score, PostProcess GPU, field/Fumen,
  replay, and security tests that prevent solution loss, false probability,
  silent fallback, unsafe FFI, and GUI/render boundary regressions.

T8 marker summary: release_blocking_rules_gate_release_acceptance.
U marker summary: test_acceptance_release_gate_blocks_solution_loss.
U marker summary: MITM PC backend marker found.
U marker summary: PackingBfsState owns queue/hold/full trace.
U marker summary: BuildUp memo key missing deleted_line_state.
U marker summary: BuildUp memo key missing hold_automaton_state.
U marker summary: ShapeKey drops TilingVariant.
U marker summary: TilingKey drops BuildVariant.
U marker summary: representative order used as coverage proof.
U marker summary: verify_first used as coverage proof.
U marker summary: target-frame support pruning applied globally.
U marker summary: PruneReason lacks proof evidence.
U marker summary: Unknown spin treated as False.
U marker summary: SpinProfile single-object regression.
U marker summary: Fin modeled as kick table.
U marker summary: postprocess changes PC coverage probability.
U marker summary: resource cap output marked complete.
U marker summary: runtime raw SVG rendering.
U marker summary: GUI subprocess shortcut.
U marker summary: WASM subprocess/process shortcut.
U marker summary: unconfirmed GPU result sources exact probability.
T8 marker summary: C memory double-release regression fail.
T8 marker summary: FFI pointer/count bound regression fail.
T8 marker summary: coverage probability invariant fail.
T8 marker summary: silent GPU fallback detected.
T8 marker summary: exact probability from unconfirmed GPU detected.
T8 marker summary: condition_summary field reintroduced.
T8 marker summary: renderer pixel/provenance golden regression.
T8 marker summary: custom piece silent fallback detected.
T8 marker summary: GUI subprocess detected.
T8 marker summary: raw SVG runtime rendering detected.
