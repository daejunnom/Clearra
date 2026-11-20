# Rust cutover, duplicate ownership, and build capacity worklist

Baseline: integration branch at `4a11de7c` plus the explicitly preserved
working changes. This worklist classifies remaining work; it is not an
acceptance receipt or a replacement for the full relation roadmap.

## Rust transition inventory

| Plan | Current effective path | Remaining work and dependency |
| --- | --- | --- |
| R01, R07 | C ABI 23 remains public; no tracked external consumer is confirmed. C packing/build and independent oracle are distinct from data-only FFI imports. | Establish live link/symbol and ownership contracts. Keep the public ABI and independent oracle until scoped evidence allows retirement. |
| R02, H01 | Published CLI/Discord, Web and packaged Desktop select Rust CPU explicitly. Library/default and explicit C comparison routes differ. | Use one shipped App context constructor; retain explicit C reference and separate generic defaults until feature-by-feature parity. |
| R03 | Typed Build Probability/Cover V2 and legacy Build Cover are different contracts. Legacy C is the general solver; Rust legacy has a narrow proved-empty adapter. | Verify complete original ordinals on actual C/Rust Core requests, then connect complete positive Legacy Observed data at the App authority seam. Do not treat prefix qualification or finite fixtures as Observed parity. |
| R04, R06 | Extended 7–24L Rust exists; ordinary native multiworker advance can block for the whole solve. The 7L exact-admission seam is private. Compact canonical failed-queue has a scoped direct Rust implementation for a single CPU/Auto worker and complete Oracle coverage. | Run exact Core/App qualification for compact before treating it as verified. Cooperative/distributed compact and multiworker support remain open. Finish sliced parity before expanding 7L or activating products; keep variable depth, unsupported rules and request budgets explicit. |
| R05, H03 | Setup/score and typed retained Sc3 already have Rust paths; Sr3 one-worker and Sr2 input restart differ. Compact PC2 worker proof is private with no public issuance. | Compare each continuation contract, preserve checkpoint vs restart and cancellation, exercise real host apply/merge and independent physical witness before publishing PC2. |
| R08–R10 | Packaged CLI already links Rust; release ProductE2E and `clearra-pc-artifact` really execute C. | Qualify family-specific defaults, then split explicit C oracle inputs from Rust release evidence and change the dependent artifact identities rather than just flipping a Cargo flag. |
| R11–R12 | C sources, public ABI and qualification C oracle remain. | Retire only actual consumers proven unused or migrated after V03/H05/performance per scope, preserving the independent oracle, required data types and historical receipts. |
| H02, H04, H05 | CLI/Discord wrappers, Chromium and Desktop bridge have scoped coverage, not full deployed readback or a packaged Windows startup run. | Exact non-publishing x64 package/Job/WebView2 and live Discord/Pages readbacks. No verified pre-main package producer exists; build once and bind run/attempt/job/digests. |

## Duplicate ownership work that can proceed without a backend cutover

1. Web/Desktop workspace dispatch: share the fixed tool registry and
   component dispatch; keep worker factory, navigation, visibility and
   instrumentation host-owned.
2. Workspace timers and export/copy: share elapsed/cancellation plumbing
   only, not solver policy, Clipboard activation, memory caps, or formats.
3. Discord plain-value cloning/freezing and field normalization: share
   structure; keep each result schema and attack redaction scope distinct.
4. Rust terminal projection: consolidate ownership release and common
   public field projection; keep exact allocator limits, redacted audit,
   ordered duplicate fields and fail-closed result semantics.
5. Build ownership: one storage-policy authority for both shells; do not
   carry forward two policy implementations merely to keep launcher names.

## Build storage replacement

MMMM and RoveZero share managed defaults of **8 GiB per slot, 12 GiB for
the catalog, four completed plus one active, five-second monitoring**;
MMMM checks both byte caps while the child runs. Accelerate and Rove4k
provide ownership/path principles but no equivalent enforced byte cap.
Adopt the MMMM contract instead of Clearra's count-only newest-five and
per-digest directory rules. Limits measure logical file bytes and are
checked at admission, during managed execution, and before completion;
sampling is not a filesystem quota and cannot prevent every transient
overshoot. Fail closed on incomplete inventory. Only inactive,
ownership-proven regenerable outputs may be retired. Exported release
artifacts, reports, credentials, foreign/active leases and historical
WSL storage are not cleanup targets.

Windows Node/PowerShell launchers, CI, Cargo defaults and WSL/WASM must
ultimately reach the same capacity owner. The existing custody of
processes, WSL isolation, credentials and exact artifact acceptance is
not part of the storage rule being replaced. Do not provision or delete
a distro as part of the port. Keep bridge/reader lifetime and independent
ABI oracles intact while removing duplication.

The branch's managed Node and PowerShell launchers now share admission
and completion checks and the global catalog lock; Node samples during
execution. On Windows or under an existing supervisor, do not claim
descendant shutdown if it could not be observed. PowerShell direct calls
still lack a live monitor; bare Cargo, the guest's separate WASM target,
and GUI experiment direct-owner calls must be brought into scope. A full
catalog with readers of unknown state blocks a fresh slot instead of
discarding a completed output.

Implementation order: disjoint GUI/Discord/projection owners and Rust
family fixtures may proceed now; App route integration and Rust release
defaults depend on typed producer seams and parity; WSL/Cargo live
storage changes depend on a single admitted slot and cleanup authority.
