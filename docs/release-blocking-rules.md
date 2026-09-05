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

## Gate and Publication Separation

The full release path is preserved, but it is executed once at the frozen
predeployment boundary. A canonical `workflow_dispatch` run owns the exact-SHA
metadata tests, `ReleaseAcceptance`, product builds, packaged-product smokes,
and Discord contract evidence. A tag push is not a second acceptance run: all
full-test and product-build jobs are dispatch-only.

The workflow does not implement this speedup with background processes inside
one Windows workspace. Four isolated jobs own Foundation
(`NoProductDebt -> AdversarialCorrectness -> DesktopHost`), Sanitizer, Rust
(`RustExactTests -> ProductE2E -> RenderGolden`), and Pages (`WasmBuildTest`).
Each job seals its exact source/run/attempt, command, ordered stages, and
toolchains. A separate Linux fan-in accepts exactly one canonical report from
each job, rejects toolchain disagreement, and reconstructs the unchanged full
eight-stage gate. It also binds the deferred evidence ownership from
`NoProductDebt` and `AdversarialCorrectness` to the actual Rust, render, and
desktop owners before canonical acceptance evidence can be created.

The four jobs may restore the same prior canonical cache archive, but GitHub
extracts it into four physically independent runner filesystems. No shard owns
an `actions/cache` or `actions/cache/save` writer, so none can publish its
mutated Cargo, C, Pages, or desktop output back into another shard or append a
cache post-step to the acceptance tail. A miss runs cold and remains correct;
cache availability is an optimization rather than release authority.

Tag publication binds a successful canonical dispatch run selected by the
exact tag commit, downloads the Linux and Windows product artifacts from that
run ID, and then revalidates the same run by ID immediately before creating the
release. Publication blocks when current `main`, the annotated tag, run event,
run completion/conclusion, head SHA, workflow path, retained artifacts, or
release immutability differs from the accepted authority. It never rebuilds or
reruns tests to manufacture replacement evidence during publication.

Path-scoped developer tests are required for ordinary iteration and may cover
only the changed crate, workspace, C surface, or release helper. They provide
fast feedback but do not satisfy this release gate; see `docs/test-policy.md`
for the focused path map. Cross-boundary changes combine focused scopes, while
the full matrix remains reserved for canonical predeployment acceptance.

`Fast Fix Qualification` is a separate qualification-only boundary. Its
successful ledger has `status=qualified-not-deployed` and
`production_mutation=false`; it is not canonical acceptance or deployment
authority. It fails unless every selected component job passed, every
unselected component job was skipped, and every unchanged component carries an
ancestor accepted digest and deployment receipt hash. A changed component has
only a qualification receipt; its accepted and deployment fields remain null.
The latest accepted ledger is verified before impact classification, and its
source/workflow/run/attempt/report hash is sealed into the impact plan; the diff
starts at that ledger source rather than the last tag. Without a ledger, only a
tag-relative `full` result may request the canonical workflow. In v0.8, every
Desktop, CLI, Discord, PC4, shared, performance, unknown, and
release-infrastructure change is automatically promoted, leaving only
Pages-only and no-product qualification eligible. Production workflows must not consume fast-fix evidence
until a later adapter verifies the component-specific artifact and deployment
receipt schemas.

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
