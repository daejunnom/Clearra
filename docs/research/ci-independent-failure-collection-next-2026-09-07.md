# Independent CI failure collection follow-up

Integration update: the user subsequently requested including this follow-up in
the next v0.8.0 deployment retry. Its Candidate/Fast Fix changes are now in that
combined source change. The original separate-branch audit below remains the
record of scope and focused verification, not proof of a completed deployment.

This is a source/mock-tested follow-up on
`codex/ci-independent-failure-collection-next`, not a new acceptance run or a
production deployment. The canonical `Publish Product Release` collector is
owned by the preceding change and is not repeated here. Native builds, product
executables, Cloud requests, runtime ports, and deployment mutation were not
used to validate this branch.

## Audit scope: all nine other workflows

| Workflow | Independence and current action |
| --- | --- |
| `candidate-preflight.yml` | Existing Rust/WASM/CLI/UI leaves are independent siblings. Fixed CLI precompile metadata/smoke/fixture collection and postcompile startup/renderer/parity collection. The existing focused Rust collector already runs independent assertion failures and blocks repeated filters for the same failed compilation; preserve it and stop further selections on cancellation/shared runner errors. |
| `fast-fix-qualification.yml` | Conditional component jobs already fan out. Fixed Node-versus-TypeScript group collection in the shared focused runner, and changed topology fan-in to print every mismatched job before refusing any artifact consumption/ledger sealing. |
| `cloud-cli-diagnostic.yml` | Image source binding, immutable build, exact image verification, isolated Job execution, and log attestation are prerequisite-dependent. No safe independent test leaf is hidden behind an assertion failure at the workflow boundary. Existing failure diagnostics and ownership-checked cleanup remain; do not run evaluation without a verified image or adopt an unowned Job. |
| `discord-deploy.yml` | Source/acceptance/recovery-debt authority, immutable preparation, protected promotion, Pages-bound global sync, observation, and checkpoint sealing are dependent production transitions. Their success guards and compensation paths remain unchanged. This audit does not repair or bypass a live deployment authority failure. |
| `discord-deploy-recovery.yml` | Restore attempts and cleanup have their own compensation contract and terminal evidence authority. Existing `always()` and bounded retry behavior is recovery, not independent test continuation. Do not reinterpret intermediate recovery failures as passed validation or alter this topology in a CI feedback change. |
| `finalize-release-publication.yml` | A single finalization command consumes the exact successful completed tag run; its upload consumes verified finalization output. There is no independent test suite to continue. Failed provenance must continue to block sealing/upload. |
| `pages-rollback.yml` | Capture and restore are mutually exclusive modes. Authority, exact package acquisition/verification, sealing, and restore mutation require their producer inputs. Keep capture/restore consumers blocked after prerequisite failure; do not turn skipped modes into failures. |
| `pages.yml` | Exact acceptance, durable rollback capture, accepted build, upload, deploy, and public readback are dependent. Failed rollback/package/acceptance validation must block mutation; no diagnostic continuation is added to this deployment path. |
| `queue-pages-publication.yml` | A single authority-bound orchestrator waits for acceptance, captures a rollback snapshot, then requests Pages. Its ordering is a production prerequisite contract rather than an independent-test bottleneck. Keep failure/cancellation/main movement terminal. |

## Continuation boundaries

- Candidate CLI's three precompile checks all execute when Node setup succeeded
  and the run is not cancelled. Compilation still requires all three checks to
  pass. No source/metadata failure grants a usable product artifact.
- After the candidate binary compiles successfully, startup, typed renderer
  tests, and direct-versus-Discord parity are independent probes. A failure in
  one does not suppress the others. A failed/skipped compile or cancellation
  blocks all three, and every failing step keeps the job failed. The uploaded
  binary remains explicitly unqualified.
- The focused JavaScript runner validates the entire explicit selection before
  starting work. A normal nonzero Node test exit does not block the independent
  TypeScript group; both failures are included in the final error. Neither group
  consumes the other's output. A process signal, missing runner, or invalid exit
  status stops the sequence. No retry, shell execution, or file-selection
  broadening is added.
- Fast Fix topology validation examines carry-forward and all seven component
  selections, including invalid flags and missing/nonterminal job results. It
  reports all mismatches and exits nonzero before downloading/sealing evidence.
  Selected-success and unselected-skipped semantics remain mandatory. No-product
  mode cannot silently select a component.
- Continuation is not applied inside a shared-process TypeScript import after
  that import fails: subsequent modules have not been proven independent of its
  mutated process state. Nor does it override failed compilation, source
  authorization, artifact verification, or deployment prerequisites.

## Verification

Focused source/mock regression coverage includes normal multiple assertion
failures, later independent success, cancellation/signals, shared spawn errors,
fixed native filter compilation blocks, malformed qualification flags, complete
fan-in reporting, and workflow guards. No local Cargo invocation is used; native
regression commands in tests are mocked. Actual Actions runtime behavior and
native product execution require a future branch integration and trusted CI run.

The branch is rebased on the newly integrated main source before upload so the
preceding canonical collector is not duplicated in the next-commit diff. This
document is an implementation audit, not acceptance or publication authority.
