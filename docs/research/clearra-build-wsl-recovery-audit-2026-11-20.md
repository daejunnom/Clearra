# Clearra-Build WSL recovery audit — 2026-11-20

## Findings

The current Rust manager **runs an already-provisioned dedicated
distribution; it does not install, restore, or register one**. This audit
read the integration source at `56c6cfe6` and inspected Windows
registration metadata only. The current user has an `Ubuntu` WSL2
registration and no `Clearra-Build` registration. The historical
install location, `${LOCALAPPDATA}/Clearra/wsl/Clearra-Build`, is also
absent. No existing owned installation at that location, checked-in
root filesystem, or rootfs provenance manifest was found. This does
not rule out an unrelated backup at an unregistered location. No
distribution was started, stopped, exported, imported or provisioned.

The [integration record](../v0.9.0-placement-dependency-rust-release-plan.md)
records Windows Application Control error **4551** starting the
`proc-macro2` build script before Clearra Core was checked. That is a
Windows execution-policy failure, distinct from an absent WSL runtime.
Neither establishes a Rust source-test result. Hosted Linux results and
future local WSL results are separately scoped evidence, not native
Windows clearance.

## Source ownership

| Boundary | Current source and meaning |
| --- | --- |
| Windows entrypoint | [Rust CLI](../../tools/clearra-manage/src/main.rs#L211) exposes `runtime wsl verify` and `runtime wsl run --entry`; it has no installation or recovery command. |
| Host admission and lifetime | [Rust WSL runner](../../tools/clearra-manage/src/wsl.rs#L16) takes the Clearra lease, only starts the configured dedicated distribution, checks limits, and only terminates that distribution. Source-taking entries currently stage a managed archive before the first WSL registration check implicit in launch. A registration-only preflight could reject a missing distribution before that staging. |
| Guest process and tool identity | [Guest session](../../scripts/runtime/clearra-wsl-session.sh) uses systemd cgroups, finite memory/task/runtime limits and a host-pipe-loss exit path. [Guest dispatcher](../../scripts/runtime/clearra-wsl-guest.sh) expects `/etc/clearra/runtime.json`, `clearra` as the runtime user, the policy toolchain digest and matching tool versions. This marker does not attest which root filesystem was installed. |
| Policy and storage | The [management policy](../../config/clearra-management.v1.json#L288) declares `${LOCALAPPDATA}/Clearra/wsl` as an explicit-lifecycle root. Its WSL block still names `Ubuntu` as `source_distribution`, but modern Rust uses that label only to validate that source and target names differ, not to provision or attest an image. |
| Retained guest provisioning | [Sanitize](../../scripts/runtime/clearra-wsl-sanitize.sh) expects a transaction-matching imported owner marker and a usable rustup from a clone. [Provision](../../scripts/runtime/clearra-wsl-provision.sh) expects that transaction's sanitized marker and rustup before installing tools and writing the runtime marker. Neither is a standalone, clean-image host installer. |
| Historical host provisioning | [Python v1](../../scripts/management/history/python-v1/clearra_runtime.py#L2726) exported a *stopped* Ubuntu, imported the dedicated clone, then ran sanitization and pinned-tool provisioning under transaction ownership. Commit `03eb0d3b` removed that active host route and moved Python v1 to history. [Current policy](../management-policy.md#L130) explicitly does not authorize executing it as a fallback. |

## Build and retirement order

| Surface | Established boundary | Work still required |
| --- | --- | --- |
| Host lease and archive | Exact `Clearra-Build` launch/cleanup identity, permanent OS lock across run and cleanup, and a staging guard acquired before archive creation. Present legacy leases fail closed; do not run the historical manager concurrently. | Bound the complete WSL list/terminate/readback phase and report cleanup separately from child exit. Stop before staging when a bounded registration/ownership preflight can disprove availability. |
| Guest source | Verify the exact staged archive bytes before extraction; failed staging must not replace the ext4 snapshot. Explicit sync emits one versioned receipt binding the dedicated distribution, published path, verified digest, and regular-file count. PowerShell consumes it only after a successful manager exit. | Bind one stable host checkout ID to one ext4 owned source slot. Keep digest as snapshot identity, not the checkout's lifetime key; do not remove historical digest directories without reader/owner evidence. |
| Native and WASM build owner | Native/C transactions select an experiment or product owner. The guest's direct WASM Cargo directory remains outside that transaction; the systemd boundary does not guarantee host-build-root environment inheritance. | Pass a validated host build-root and stable checkout identity before admitting the guest. Route WASM into the existing transaction rather than creating a second independent cache/catalog. Qualify ext4 versus Windows-mounted I/O as a separate decision. |
| Output and retirement | Benchmark output refuses existing data; retirement preflights all candidate trees under their leases before deleting any, refusing nested links and foreign owner records. Retention remains latest five products and one active experiment slot per admitted source/purpose. | Test the complete lifecycle on the dedicated real runtime; fixture tests establish individual behavior only. Do not use storage size as proof of ownership or safe disposal. |

## A safe recovery contract (not implemented)

The existing Ubuntu distribution and its lease take precedence. Do not
switch a managed build to it, stop it to pass the old provisioner's
precondition, copy/export its filesystem, or call global
`wsl --shutdown`. Sanitizing an imported clone *after* export does not
prevent copying workstation data in the first place. Build absence
must return a typed unavailable/preflight result, not choose another
distribution or rewrite the marker.

Provisioning, if separately authorized through the administrator route
defined by policy, should start with a clean WSL2 image whose pinned
origin, architecture, digest, and toolchain source can be checked
independently. Preflight the managed install directory and
registration without starting a distribution, then recheck ownership
and path identity under the dedicated lease before changing state.
Refuse foreign names, an unexplained pre-existing path, a mismatched
registration, and an active or uncertain owner. The host transaction
should record the exact image and destination before import; it
should not delete or replace an installation merely because its
marker is missing. Cleanup or
rollback may target only the exact installation owned by that
transaction, not Ubuntu or another user's assets.

The clean-image setup must provide WSL2 systemd and the existing
bounded session contract, create the non-root runtime user, and
install/verify the policy-pinned tools in the private guest roots. Issue
the guest marker only after successful verification; keep base-image
and host installation provenance in a separate host receipt. Test
registration and launch identity before running any worktree build.
After a dedicated runtime is actually established,
`clearra-manage runtime wsl verify` is a supervised toolchain/cgroup
smoke check; a subsequent registered build/test entrypoint is distinct
evidence tied to the tested sources. This audit performed neither step.
