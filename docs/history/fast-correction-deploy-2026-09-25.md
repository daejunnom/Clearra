# Superseded Fast Correction design

Historical record only; current Fast Fix/component-ledger workflows are the execution authority. Do not use the following as current deployment instructions.

---

# Fast Correction Gate and Deploy

The fast correction path is a post-release path for a narrow correction whose
runtime ownership can be proven from Git history. It is not a user-selected
"performance unchanged" switch and it is not a substitute for canonical
acceptance when ownership is uncertain.

## Authority model

`Fast Correction Gate and Deploy` accepts only an exact accepted base SHA. The
base must be the commit of the latest reachable annotated production SemVer tag,
must have exactly one successful first-attempt canonical `Publish Product
Release` dispatch, and must be an ancestor of the exact current `main` candidate.

The workflow checks out the candidate without executing it. It then runs the
classifier, owner manifest, canonical-run resolver, and evidence sealer from the
accepted base checkout. The following authority bundle must be byte-identical
between base and candidate:

- `.github/workflows/fast-correction.yml`
- `scripts/release/fast-correction-authority.mjs`
- `scripts/release/fast-correction-evidence.mjs`
- `scripts/release/fast-correction-owners.v1.json`
- `scripts/release/fast-correction-plan.mjs`

A change to any bundle member therefore returns `full-required`; candidate code
cannot edit its own classification rules and immediately use the fast path.
The plan and final evidence bind the accepted tag/SHA/run, candidate SHA, raw
Git object/mode/status entries, diff hash, manifest hash, selected tests, builds,
deployments, skipped products, workflow run/attempt, artifact identities, and
public Pages identity where applicable. Both are retained for 90 days.

## Closed owner table

| Diff ownership | Fast decision | Tests | Build/deploy |
| --- | --- | --- | --- |
| Root project documentation and `docs/**` | `fast-eligible / documentation` | `git diff --check` | none |
| Listed release workflow YAML and the two closed workflow validators/runners | `fast-eligible / release-workflow` | bounded release regression pool and release workflow smoke validator | no product build; a changed `pages.yml` or `discord-deploy.yml` can qualify only its matching dual-authority follow-up |
| Web route/lib/static/config/test files owned only by `apps/clearra-web` | `fast-eligible / pages` | `@clearra/web` contracts | Pages WASM/Vite build and Pages deploy only |
| Fast authority bundle, core C, Rust/WASM crates, performance/benchmark paths, desktop, CLI, Discord runtime, web workers, tablebase, shared UI/CTK/schema, dependency manifests | `full-required` | none in the fast workflow | fresh canonical path required |
| Unknown, ambiguous, duplicate, rename/copy, type change, symlink, submodule, or unsupported mode transition | `full-required` | none in the fast workflow | fresh canonical path required |

Pages runtime source and a Pages/Discord control-workflow change cannot be mixed
in one fast candidate. That combination returns `control-runtime-mix` and
requires canonical acceptance. Release helper source under `scripts/release/**`
is not a workflow-only exception; it also requires the canonical path. This
keeps dual authority limited to YAML control-plane corrections such as quoting,
encoding, or action wiring while every executed helper remains accepted-base
code.

## Execution and deployment

Run the fast workflow from exact current `main` with `accepted_base_sha` set to
the latest production tag commit. Pages rollback inputs are necessary only when
the classifier selects Pages runtime source.

- Documentation: produces explicit no-deploy evidence.
- Release workflow validation only: runs the two closed release checks and
  produces no-product-deploy evidence.
- Pages source: installs the JavaScript workspace, tests `@clearra/web`, builds
  only Pages, validates/stamps the build with accepted-base authority code,
  verifies the durable rollback capture twice, deploys Pages, and requires exact
  public identity readback. CLI, desktop GUI, and Discord are neither tested nor
  built nor deployed.
- `pages.yml` workflow-only: dispatch `Publish GUI to GitHub Pages` from current
  `main` with the accepted product base SHA and exact successful fast run/attempt.
  The workflow validates the sealed dual authority, downloads the base canonical
  Pages artifact without rebuilding it, and records product source and workflow
  source separately in Pages deployment authority.
- `discord-deploy.yml` workflow-only: dispatch `Deploy Discord Production` from
  current `main` with the accepted product base SHA and exact successful fast
  run/attempt. The candidate workflow validates the same sealed dual authority,
  uses the base canonical run and product source, and retains a dedicated
  control-authority artifact. Existing `discord-path-confirmation` remains the
  only runtime promotion gate. Because product/catalog bytes are unchanged, the
  fast control path does not repeat global command sync or release checkpoint
  publication.

The workflow intentionally has no `actions: write` permission and never
auto-dispatches the canonical workflow. A `full-required` plan is uploaded as
blocked evidence and then makes the run fail. This avoids duplicate full runs
and prevents a classification result from granting its own privileged action.

## Operational sequence

1. Merge the initial fast-path authority only after the ordinary canonical full
   gate. Publish the annotated release tag; that tag becomes the first trusted
   classifier base.
2. For a later candidate, capture Pages rollback authority first if a Pages
   deployment may occur.
3. Dispatch `Fast Correction Gate and Deploy` once. Workflow reruns are
   forbidden; after a failure, use a fresh dispatch.
4. If the plan is `full-required`, stop and run the normal canonical release
   acceptance for the candidate. Do not reinterpret the result manually.
5. If a workflow-only plan contains a `pages` or `discord` control deployment,
   pass the exact fast run ID and attempt to only that matching follow-up
   workflow. The other product workflows remain untouched.

The normal annotated-tag publication contract and immutable three-asset release
remain canonical-only. Fast correction evidence cannot publish or replace a
release artifact and cannot bypass Discord Environment review.
