# CI build reuse and Cloud Run baseline — 2026-09-09

Scope: build orchestration only. Preserve full release acceptance, current-run/source/attempt binding, existing product algorithms, and protected deployment approval. The v0.8.0–v1.0.0 roadmap and the minimum-algorithm plan are unchanged.

## Findings

- CTK3 already has one canonical build/test producer; Linux CLI, Rust, Discord and Pages download its sealed distribution. Keep this producer/consumer structure.
- WASM already has one verified producer; Pages consumes its sealed output without rebuilding Rust. Keep this boundary.
- All six Windows acceptance cache readers previously had no corresponding cache writer. Historical completed run `34234316638`, WASM job `102088189846`, reported a cache miss and spent **3m06s compiling wasm-bindgen-cli 0.2.126 itself**. This is historical timing evidence, not a new measurement of the currently running gate.
- The Linux product cache used only the dependency-lock key. An immutable cache entry could not advance to include compilation from subsequent source changes.
- Candidate Preflight had no build cache, and its direct WASM builder used `cargo-target-wasm` while canonical WASM uses `cargo-target`. Merely adding a cache reader without aligning that target would still miss compiled dependencies.
- Standalone Linux CLI and the current Cloud image compile the same `clearra-cli` release package with `wasm-cpu-runtime,webgpu-search`. They are not different solver implementations. However, a host Ubuntu binary was not proven compatible with the deployment's Debian Bookworm runtime.

## Implemented in this change

| Surface | Build/reuse policy |
| --- | --- |
| Canonical native acceptance | Rust shard is the single writer; foundation leaves are readers of the compatible native snapshot. |
| Canonical WASM | Separate WASM snapshot and one writer, including the version-checked wasm-bindgen executable. |
| Sanitizer | Separate C-only snapshot and one writer; never restores Cargo or wasm-bindgen payloads. |
| Candidate Preflight | Reads compatible native/WASM snapshots without any cache-writing or release authority; still builds/verifies its own exact source. |
| Linux CLI | One build in `rust:1.96-bookworm`, using a Bookworm/Rust-version/lock/source-specific cache; old Ubuntu cache entries cannot match. |
| Slim-runtime verification | Executes the already packaged Linux CLI in `node:22-bookworm-slim`; no Rust installation or second compilation. |

Each explicit Windows cache writer runs only after successful build/tests/sealing, only on an exact-key miss, and saves the restore action's **primary** key rather than its fallback key. Saving is optional and bounded to two minutes. This exception applies only to cache transport: compilation, tests, identity checks and artifact upload continue to fail closed. Cache restore never skips those checks.

Native and WASM writers use different immutable keys, preventing parallel producers from racing to publish different incomplete target trees under one key. Entries are disposable acceleration, not accepted product bytes or acceptance evidence. Initial runs and cache eviction still require cold builds; no percentage speedup is claimed without subsequent measurements.

## Parallel release shape

The existing six acceptance leaves remain split. Linux CLI depends only on metadata and the small CTK3 producer, **not** on WASM, Windows GUI or the acceptance fan-in. Its Bookworm build therefore overlaps those leaves on independent runners. The slim runtime probe follows the Linux build and precedes artifact upload. It checks rules, solver and finesse execution, all five compiled identity fields, the runtime OS/architecture and the unchanged binary digest.

Do not launch competing Cargo processes against a shared writable target merely to increase the job count: Cargo locking serializes them, while separate target trees duplicate dependency compilation. Cross-job caches are immutable snapshots, not concurrently mounted writable targets.

### Expected runtime performance

The Bookworm switch changes the build/runtime compatibility baseline, not the search algorithm, worker budget or CPU target policy. Both existing compile paths use the same feature pair, `--release`, Thin LTO and one codegen unit. `clearra-cli` maps `wasm-cpu-runtime` to `clearra-app/parallel`; this is a native Rust CLI, not an added WASM emulator. No existing `target-cpu=native` optimization is removed. There is therefore no source-based reason to predict a material compute slowdown solely from Bookworm. Compiler, allocator and host differences can still affect measured time; retain the warm CLI/Discord parity checks rather than claim a measured zero regression or split the builds without evidence.

## Original Cloud Run replacement assessment

**Historical scope of `e817ff5`:** production Cloud Build still compiled its CLI and CTK3 from the exact source archive. That change established the compatible Bookworm producer/probe only. The follow-up implementation below now connects accepted-product reuse for the next deployment; it does not change an already running deployment.

Original integration checklist, implemented by the follow-up below:

1. Download the accepted Linux CLI and CTK3 from the exact canonical run and attempt, validate their bytes against the canonical evidence/CTK3 manifest, and retain the source identity checks. Never substitute the latest release tag or an unqualified candidate artifact.
2. Keep `exact-source.tar.gz` byte-for-byte source-only. Oracle recovery and the Cloud image authority depend on that boundary. Do not silently append generated CLI/CTK3 files to it.
3. Transfer accepted product inputs separately, with a closed manifest binding source, run, attempt, filenames and SHA256 values. Use immutable GCS generation/object bindings or an equivalently sealed build-input archive. Verify that manifest before Docker consumes the binary.
4. Extend the Cloud Build readback/image authority and prepared-state bindings to include those accepted inputs; retain builder-only credentials and the existing protected promotion/rollback identities.
5. Package the verified binary and CTK3 into the slim image without Cargo or CTK3 compilation. Keep container module-closure, startup/identity, warm CLI/Discord parity and candidate/rollback checks.
6. Keep the explicitly unqualified Cloud evaluation workflow distinct; its image cannot become canonical release authority.

This avoids adding a second Cloud-specific Rust build to the gate. An alternative was prebuilding the entire Cloud image as a parallel canonical job, but that would move Cloud identity/IAM and image publication into the gate and require a larger authority change. The follow-up uses the single compatible Linux CLI.

## Verification

- Local bounded release-regression pool: **52 files, 628 tests passed**; no Rust/WASM recompilation was needed for these orchestration changes.
- Windows PowerShell static acceptance: **8 tasks passed, 0 errors**, 29.99s. The 97 existing architecture advisories remain visible; they were not suppressed or treated as new runtime failures.
- Candidate/release workflow mutation tests reject shared native/WASM writer keys, failed-build cache publication, unbounded cache writes, cache-key drift, candidate cache writes, masked slim-runtime failures and unintended serialization behind WASM.
- Bookworm runtime identity/OS/architecture probe unit tests pass. This Windows machine has no Docker executable; actual Bookworm compilation and slim-image execution are required by the new Linux gate and are not claimed as locally executed.
- Existing plan-file SHA256 values were unchanged while preparing this change.
- After main is updated, the user-requested CI status snapshot is taken once. Do not poll to completion or treat an in-progress state as success.

References: [GitHub cache behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching), [separate cache restore/save actions](https://github.com/actions/cache), [container jobs and sibling container actions](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/run-jobs-in-a-container).

## Post-push snapshot and deployment fix

Main build-orchestration commit: `e817ff5a2f8323ef6ae668a392bda3d1a2510acd`.

One usable status snapshot was obtained at **2026-09-09 00:31:38–42 KST**. The first raw run-list response was truncated before parsing and was retried once with only selected fields; there is no continued status polling.

- Canonical acceptance `34237996138` / source `21c0a7a`: success.
- Pages publication `34241279487` / source `21c0a7a`: success, including sealed public readback.
- New-main recovery regression check `34245191186`: success.
- Discord `34241118605`: Cloud Build and prepared-input sealing succeeded; promotion failed at **Download the exact prepared state**, before any protected runtime transition.
- Recovery `34245260647`: success, proven no Oracle/Cloud runtime mutation; no active recovery execution needed.

Failure log: the implicit artifact service returned `Failed to ListArtifacts ... (403) Forbidden`. The prepared artifact exists, is not expired, and is visible through the authenticated repository API (artifact `10063107978`). This does not prove why the intermediary denied its runtime session; do not invent an expiration or IAM diagnosis.

Fix: name all three same-run Discord handoff downloads explicitly with the existing `github.token`, current repository and current run ID, using the authenticated REST path already used by cross-run acceptance downloads. Source/run/attempt-bound artifact names, sealed byte checks, `actions: read`, protected approval and all promotion/recovery gates are unchanged. No PAT or broader permission is introduced. Fresh acceptance and Pages publication can then be requested for the new source; do not reuse old-source acceptance or wait for the new runs to complete.

Fix verification: 71 focused deployment/recovery/runtime tests passed, including four new handoff authority checks; the Windows PowerShell Release Identity Gate passed again. The earlier full orchestration pool remains 628 tests at the time it was run; it is not relabeled as a rerun of the subsequently added checks.

## Public-documentation preflight correction

Run `34245709188`, Discord contract job `102127364577`, reported 548 passing tests and one failure. The failure was the public-command non-discovery check on this note's former line 50, not an engine or Bookworm runtime failure. The note now describes the unqualified Cloud evaluation workflow without the disallowed wording.

The unchanged document policy has moved from the CTK3-dependent Discord suite into the dependency-free metadata regression pool. It still covers both public READMEs and every nested Markdown document, including research notes. Live slash/help/parser checks remain in the Discord suite. Each violation reports only the file, line and bounded matched text; all matching files are reported without dumping the entire documentation tree into Actions logs.

Validation: reproduced the old committed document's single violation at line 50; the corrected tree and policy/manifest regressions passed (8 tests); the local Discord built-only suite passed (657 tests); Release Identity Gate and release CLI workflow validation passed. No CTK3, Rust or WASM rebuild was performed for this document/test-orchestration correction.

## Slim-runtime response-envelope correction

Run `34246801516`, Linux CLI job `102131100147`, passed compilation and package smokes, then failed in the slim-runtime verifier. All three subprocesses exited successfully and their five-field identities passed; the final finesse check incorrectly read the root `mode` instead of `finesse_report.mode`. The unit fixture duplicated that incorrect flat shape, so it had not detected the mismatch. This failure is not evidence of an ABI incompatibility or solver regression.

The verifier now reads the public nested report, and its regression cases reject a missing/wrong report even if a misleading root `mode` is supplied. Identity mutations cover every probe and every identity field. The OS/architecture, binary-digest and no-recompilation checks remain unchanged.

Validation: the corrected fixture first reproduced three failures in the old verifier; all five focused cases passed after the fix. All three real probe commands also passed the corrected output verifier against the accepted Windows CLI artifact from run `34237996138`, source `21c0a7a76c929db9a8a68ae9e2f30c206b9a97b1`: the root mode was absent, `finesse_report.mode` was `search`, and the report was complete. This validates the shared CLI output contract, not a local Linux-container run. The bounded release regression pool passed **638 tests in 53 files** (24.01s); Release Identity Gate and release CLI workflow validation passed. No Rust/WASM rebuild was performed locally. Fresh acceptance must still pass the actual Bookworm runtime gate before publication.

## Accepted-product packaging follow-up (next deployment only)

The production workflow now downloads the Linux CLI from the **same exact canonical run and attempt** as CTK3. It verifies the binary against the canonical acceptance artifact hash/size/name and verifies CTK3's sealed files and the acceptance-bound manifest digest. A missing artifact or mismatch blocks preparation; there is no fallback compilation.

The Oracle `exact-source.tar.gz` remains byte-for-byte source-only. A separate `cloud-build-inputs.tar.gz` carries an `inputs/` directory containing a copy of that unchanged archive, the accepted CLI, accepted CTK3, canonical acceptance evidence and a closed source/run/attempt-bound manifest. This transport uses the existing source staging bucket and builder identity, without adding another upload permission or credential.

`cloudbuild-accepted-job-service.yaml` first verifies the externally supplied manifest hash and the inner source hash **before extraction**, then runs the full input verifier from that authenticated source. `Dockerfile.accepted-job-service` installs runtime npm dependencies with lifecycle scripts disabled, copies accepted products, runs the Bookworm CLI/identity/CTK3/module checks, and packages the image as the non-root Node user. It contains no Rust toolchain and no CTK3 build command. CLI bytes are checked again in the final image. The existing source-build Dockerfile/config remain separate for independent evaluations; production never selects them.

Cloud Build requests SHA256 source provenance. The new v2 image authority requires the fetched transport's base64 SHA256 to match the local archive, binds the resolved storage generation, accepted run/attempt, manifest and product hashes, and requires both packaging steps to succeed. The existing prepared-state hash transitively binds this v2 authority; protected promotion rechecks its inputs before any runtime mutation. Historical v1 image authority and prepared-state formats are unchanged, and recovery recognizes both old and new preparation step names.

Cloud Build itself remains necessary to assemble and publish the container image. The removed work is the second Rust compilation and second CTK3 compilation, not dependency installation, artifact verification, image upload, warm CLI/Discord checks or rollback protection. No measured deployment speedup is claimed before an actual run of this route.

Cloud-only packaging config changes are classified as managed-runtime changes rather than unnecessarily selecting the Oracle gateway. No workflow dispatch, Cloud Build submission, deployment cancellation or runtime mutation is part of preparing this follow-up.

Reference: [Cloud Build source provenance and requested SHA256 hashes](https://docs.cloud.google.com/build/docs/api/reference/rest/v1/projects.builds#SourceProvenance). For a single uploaded tarball, provenance describes that uploaded archive; the inner Git archive and individual accepted products are separately bound by the input manifest.

Follow-up validation: **664 release regressions in 55 files passed** (26.77s); **8 static architecture tasks passed**, with the same 97 existing advisories and no errors (40.25s). Both workflow/config YAML files parsed successfully, all ten custom Cloud substitutions are bound, and the actual bootstrap JavaScript parsed and executed locally against a fixture archive plus the real verifier. Mutation cases cover wrong run/attempt/source, changed accepted products, altered manifests, missing or mismatched fetched-source provenance, masked verification failure and source-build fallback.

The producer also verified real Linux CLI/CTK3/canonical-evidence artifacts from accepted run `34249090251`, source `01d2648ab04dbaaf3cf1374f5ad62c6fa0560084`, using a source archive produced by the existing exact-archive helper. This checks actual artifact formats and byte bindings without compiling or running the Linux executable on Windows. Docker is unavailable locally: no real container build, Cloud submission or deployment was performed, and an end-to-end packaging time is not yet measured.

The in-flight Discord release still requires remote main to equal its accepted source at checkpoint finalization. Keep this follow-up on `codex/reuse-accepted-cloud-inputs` until that release is no longer active, then merge before the next explicitly requested deployment; do not move main underneath the current release. This preparation neither cancels nor retries it.
