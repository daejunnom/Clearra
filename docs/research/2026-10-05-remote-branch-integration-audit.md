# v0.8.1 resume: remote branch reconciliation and 24L boundary

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
