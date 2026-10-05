# v0.8.1 resume: remote branch reconciliation and 24L boundary

## Current reconciliation, after the user's retirement and small-24L approval

The sections below this update preserve the earlier `810f1486` audit, not the
current deployment state. Its 195-entry inventory included the symbolic
`origin/HEAD`: there were **194 actual feature branches**. Its statement that
no branches were deleted applied before the subsequent explicit retirement
request.

The main integration candidate is now the linear branch
`codex/converge-main-product-fixes-20261005`, based on unchanged remote main
`b781f6b6ef1c9ce27e0430f87663a92508e1cb7d`. It contains:

- `bba41ef3`: the current v0.8.1 product tree from
  `codex/converge-v081-linear-20260928`, source
  `02b0b5869340ff74e749e092856cd11befc89055`, without reverting newer main
  recovery changes or regenerating qualified assets.
- `0fd2c192`: the final recovery multi-stage product fixes from
  `codex/converge-recovery-chain-20260929`, source
  `e2fe5f57f4da0c056a5b8c7a8d38f18e74efc42c`: independent stage inventories,
  initial symmetry, hold/exchange, row decoding, compiled host/worker payloads
  and production result presentation.
- The four-word PC input/compiler/identity boundary from `badc733e`, with the
  pinned formatter's changes and a new **one-placement** 24-line shared-engine
  smoke. The previous positive T cavities were only geometrically tileable;
  their positive reachability expectation was not independently established.
  The replacement keeps high initial and target bits in the fourth word and
  checks the real existing ILC/BuildUp path, without enumerating a large 24L
  PC. Full-height PC area and identity are covered separately by five small
  compiler tests. Neither this smoke nor the wider typed compiler claims that
  all public minimum/score/replay reducers have been extended to 24L.

The failed `badc733e` source jobs were inspected once. Rust formatting failed
in the new PC boundary; Recovery Build then attempted independent feedback
without preparing Node/pnpm dependencies after that failure. The candidate
corrects the format and runs the prerequisite setup with `!cancelled()`.
Failures remain failures; this does not use `continue-on-error` or grant
release authority. The v0.8.1, recovery, multi-stage and small PC24 source jobs
all explicitly admit the new linear branch and stay non-publishing.

Locally, the Node source/workflow contracts passed (22 tests), as did the four
isolated recovery-source tests, five extended PC compiler tests, and the
one-placement 24-line engine test. The Rust test bodies reported 0.00 seconds
for the latter two targets. **Their Windows supervised command receipts are
not successful receipts**: the current prebuilt manager reported
`E_CLEARRA_PROCESS_TREE_NOT_STOPPED` after the test child returned successfully;
final cleanup reported the tree stopped. This separate termination/accounting
observation is not hidden or treated as test/release acceptance.
The separate Pc-graph unit-test executable was blocked by Windows Application
Control (4551), including one manual retry with unchanged resources. It was
not executed locally and is left to the pinned Linux input-contract job; no
security setting, supervisor limit or WSL ownership was changed to bypass it.

| Retired remote feature branches | Count | Evidence required before deletion |
| --- | ---: | --- |
| Already ancestors of remote main | 31 | Exact ancestry and unchanged remote tip |
| Equivalent patches already in main | 53 | `git cherry` has no new patch and no unreviewed merge-only commit |
| Temporary CI/dispatch/source-transport branches | 28 | Tracked source review; final product source retained separately |
| Total | **112** | Every exact tip retained locally before the non-force deletion |

The remaining **82 original feature branches** have unconsumed product or
research history, or an unreviewed merge-only change. They are preserved, as
are all local worktrees and local-only PC24 integration commits. In particular,
the former `deploy-approval-flow` branch is not removed merely because its
non-merge patch is equivalent. v0.9.0 work remains frozen: deleting a redundant
TB branch does not delete its already-integrated source or qualify/activate TB.

Deleted exact tips remain reachable at
`refs/archive/clearra-remote-retired/2026-10-05/codex/<former-name>` in this local
repository. Restore only the selected former branch from its recorded tip
with an ordinary non-force push. No Git history, release/tag, CI run or
deployment artifact was deleted.

Main enforces linear history and the strict `Clearra management policy` check.
The previous merge-commit push was rejected with GH013; the candidate was
reconstructed as linear commits instead of changing or bypassing protection.
The current exact source must still receive its own required CI check before
fast-forwarding main. A source upload, local test body, or historical green run
does not count as main promotion, acceptance, publication, or readback. New
run upload identities may be confirmed once; ongoing CI polling stays disabled.

## Authority

This is a reviewed source-history summary, not a release receipt. The inventory
was taken after fetching and pruning `origin`, against integrated source
`810f148612468ea7f634fe786c07a1345244d4c5`. Later commits must qualify their own
exact source; a previous successful job does not qualify the new 24L code.

No secret, credential, private key, generated asset payload, or unrelated
application data was examined to produce this summary. No remote branch was
deleted or force-pushed. Unique work and local-only research remain recoverable.

## Inventory and disposition

All 195 remote feature refs were inspected for ancestry, non-equivalent patches
(`git cherry`), and changed tracked paths. This is a complete ref/history
inventory, not an assertion that every historical implementation is correct.

| Ref class | Count | Disposition |
| --- | ---: | --- |
| Ancestry already included in the integrated source | 34 | No additional merge required; retain refs pending explicit retirement |
| Different ancestry, no non-equivalent patches | 54 | Do not replay the same implementation under a second history |
| At least one non-equivalent patch | 107 | Classify by current source/owner; do not equate a unique commit with a missing product feature |

The integrated branch is `codex/converge-pc24-integration-20261005`. It starts at
remote `main` `b781f6b6ef1c9ce27e0430f87663a92508e1cb7d` and merges the v0.8.1
product integration `codex/converge-v081-linear-20260928`, tip
`02b0b5869340ff74e749e092856cd11befc89055`. The merge retains the newer main
recovery repairs while bringing the signed accelerator admission and actual
CLI, Desktop, Web and Discord product connections together. Qualified packs
are reused unchanged; profiles are not regenerated or requalified here.

| Remaining ref family | Source finding | Action |
| --- | --- | --- |
| `codex/v081-main-convergence-20260928`, `codex/v081-selective-source-ci-20260927` | Older convergence trees omit newer recovery/desktop/signed-browser changes; unique ancestry alone would misclassify them as wholly unapplied | Keep history; consume current linear integration, not an older tree replacement |
| `codex/clearra-safe-integration-20260925`, pinned-boundary/candidate/release-integration variants | Large, overlapping predecessor integrations, not isolated patches | Preserve and review by behavior/owner; no blanket merge |
| `codex/converge-recovery-chain-20260929` | Eight non-equivalent commits: initial symmetry, ordered multi-stage supply/coverage, parser, witness row decoding, focused native/wire/browser tests | Genuine separate product integration candidate; remains open, not implicitly qualified by the v0.8.1 job |
| `recovery-chain-*`, `recovery-multistage-*`, import/tooling/dispatch/verify refs | Several versions of source proposals and CI-only transport wrappers for the same recovery work | Use the final reviewed source owner; do not merge temporary dispatch workflows into the product |
| `recovery-memo-*`, `recovery-physical-cache-*`, `recovery-sharing-*` | Local/source A/B proposals and bounded diagnostic jobs | Keep separate from product acceptance; no additional benchmark run in this task |
| `minimum-hotfix-*`, `min-cover-performance`, `v0.8.0-hotfix-minimum-algorithm-ab`, `pc-replay-dp-*` | Earlier minimum/replay experiments with different snapshots and local-only features | Do not overwrite current exact first-canonical/lazy-tie/reducer contracts |
| `release-build-architecture-ab`, `release-rust-shard-contention*`, `next-fast-deploy`, independent-failure/nonbuild-tail variants | Earlier build/deploy optimization proposals overlap later main workflow fixes | Inspect actual remaining workflow behavior before integrating; retain exact-source build/release authority |
| `codex/japanese-i18n-prep` | Its remaining unique commit changes old planning documents, not current Japanese product catalogs | Do not replace current plans with older shortened documents |
| `codex/legal-board-kick-index-20260907` | Local `_local/legal-board` kick-index experiment | Preserve as research; not a qualified replacement legal-board asset |
| `codex/converge-legal-board-cleanup-20260923` | Separate management-policy clarification | Keep distinct from solver/product changes; respect the current supplied AGENTS rules |
| `pc4-*`, `v0.9.0-*`, mixed `converge-v081-v090-*` | Tablebase discovery/transport/materializer/profile work and mixed-version integration | Preserve the unfinished v0.9.0 scope; no TB upgrade or domain widening in v0.8.1 |

Removing 34+54 refs is not required for source integration and would discard
useful branch-level provenance. Their retirement is separate from the reviewed
integration; the 107 unique-patch refs have not been silently deleted.

## Current non-publishing evidence

The first upload confirmation observed success for source `810f1486`:

- [v0.8.1 Selective Source](https://github.com/daejunnom/Clearra/actions/runs/37266411460)
- [Recovery Build Regression](https://github.com/daejunnom/Clearra/actions/runs/37266411466)
- [Clearra Management Policy](https://github.com/daejunnom/Clearra/actions/runs/37266411459)

These results were read once, not polled. They are source feedback and do not
constitute a canonical release acceptance, main promotion, deployment or
rollback readback. They do not cover the subsequently added four-word code.

## 24L source boundary and remaining product work

The four-word 10x24 board and extended ILC/BuildUp implementation already exist.
The previous gap is at the PC input, identity, execution-dispatch and reducer
boundaries, not the absence of a larger board type.

New boundary code preserves all four words, clears initial full rows across
word boundaries, binds target height and required piece count, and prevents
different high words from aliasing the same compact problem ID. The typed
compiler bridges to the existing extended execution field; it does not
replace the original field with a zero/low-word placeholder.

Local compiler tests have passed, including ordinary/finite ID agreement and
high-word alias rejection. Local full Core execution/formatting encountered
Windows Application Control error 4551. This is not a successful solver smoke
and not an assertion failure. The independent `PC 24L Source Boundary` workflow
therefore compiles the same source on Linux and executes finite 12L/24L ILC
fixtures without delaying the other v0.8.1 jobs.

Product activation remains open until input adapters, browser cooperative and
distributed execution, native worker execution, wide candidate identity,
minimum/score/replay/page/copy semantics and public capability limits agree.
Do not publish a 24L selector that only fails later in Geometry. Keep the
existing compact 1..6L path unchanged. Exact legal-board stays scoped to
qualified empty-origin 4L; PC4 Tablebase stays scoped to at most 4L and is not
silently activated by raising the ordinary PC target cap.
