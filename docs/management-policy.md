# Clearra management policy

`config/clearra-management.v1.json` is the policy source for two bounded
concerns: generated-output locations and runtime memory/process-tree safety.
The active implementation is the Rust crate in `tools/clearra-manage`.

The Python v1 implementation is retained under
`scripts/management/history/python-v1/` only as historical source. It is not an
active entrypoint, is not invoked by CI, and must not be used by product or
release scripts.

## Deliberate scope boundary

The manager validates:

- repository and Clearra-owned external output roots;
- traversal, symlink, junction, reparse-point, and credential-path escapes;
- hard memory, process-count, output-size, timeout, and descendant-tree limits;
- cooperative GC requests and acknowledgements under host pressure;
- scoped ownership and cleanup of each supervised WSL request.

The manager does not govern filesystem reads, Git, dependency management,
toolchain installation, package publication, or every ordinary child process.
`Get-ChildItem`, `rg`, `git`, `gh`, Cargo, pnpm, and similar native tools may be
used directly. This boundary avoids routing cheap read-only and source-control
work through a general Python policy layer.

## Build and output paths

Generated repository output is limited to the roots declared in the manifest:

| Root | Purpose |
| --- | --- |
| `build/` | Cargo targets, bundles, package/container staging, project tools |
| `coverage/` | coverage output |
| `_local/artifacts/` | test, benchmark, analysis, and raw research evidence |
| `_local/state/` | locks, checkpoints, and receipts |
| `_local/tmp/` | transaction-scoped temporary output |
| registered product paths | workspace package links and explicit web/desktop output |

Cargo uses `build/cargo/default` by default through `.cargo/config.toml`.
Specialized build owners may select another declared Clearra build/cache root.
This compatibility path is not yet covered by the managed capacity
catalog. Managed builds adopt the shared MMMM slot, catalog and
ownership limits documented in [the build system](build-system.md).
These storage bounds are separate from process-tree, credential and
WSL isolation rules; changing cache retention does not relax them.

Inspect or validate paths with the prebuilt executable:

```text
clearra-manage storage audit
clearra-manage storage verify --path <path>
```

On a local interactive terminal, an unmanaged path can be allowed for one call
with both `--force-unmanaged-output` and `--force-reason`. The manager records
the reason and prints that forcing is not recommended. Credential paths,
symlink/junction escapes, CI, release, and deployment contexts cannot use this
override.

## Runtime supervision

Use the supervisor for a run that can consume large or unbounded memory, owns a
long-lived service, creates a descendant tree that must end with its owner, or
performs a benchmark whose resource identity matters:

```text
clearra-manage runtime audit
clearra-manage runtime run --producer <label> --profile <profile> \
  --timeout <seconds> -- <command> [arguments]
```

Short builds, unit tests, formatting, linting, and processes already inside a
finite CI/container boundary do not require the runtime wrapper solely because
they launch another program.

The Windows implementation uses a Job Object with kill-on-close, hard aggregate
memory, and active-process limits. Linux uses an owned process group and
low-frequency aggregate `/proc` accounting. A Linux profile marked as requiring
hard containment starts only when the manager inherits a cgroup v2 with finite
`memory.max` and `pids.max`; otherwise it fails before spawning the child. The
supervisor checks host pressure at the low frequency declared in the manifest
and never changes worker count or retries an OOM with different resources.

At low host memory or when the owned tree approaches its emergency hard cap,
the supervisor writes a uniquely identified cooperative full-GC request. A
directly supervised Node root runs a small event-driven responder with exposed
V8 GC. Its completion is accepted only when the request, protocol, action, and
root PID match. A native process or a browser child with no responder does not
receive a false GC completion claim. A recovered episode is cleared and
rearmed. A sustained noncritical host warning does not end useful work; after
critical escalation or an owned-tree soft-limit event remains unresolved for
the recovery grace, only the Clearra-owned tree is terminated with a typed
fail-close reason. The hard cap still protects against sudden allocation spikes.

Start admission checks only the smaller critical physical reserve and, on
Windows, the critical commit reserve. `minimum_memory_mib` describes the
profile's intended working set; it is not reserved or compared with momentary
free memory at launch. Local profiles no longer have a fixed profile memory
maximum. Their emergency hard cap derives from total physical capacity on
Linux or total commit capacity on Windows, minus the configured reserve; it
never uses the start snapshot's available bytes. The no-swap WSL guest cgroup
is capped separately by host physical capacity. The Cloud Run job retains its
fixed 16 GiB platform cap. The manager samples current owned-tree memory rather than the
historical peak to decide whether a GC request actually recovered headroom.

Runtime receipts are written to the Clearra-owned platform state root and
include the sanitized command, profile, admission values, observed peak and
current owned-tree usage, pressure/GC state, responder availability, exit
reason, and tree-stop result. Secret values are redacted.

## WSL

All WSL work uses a fixed manifest entrypoint:

```text
clearra-manage runtime wsl verify [--distribution NAME]
clearra-manage runtime wsl run [--distribution NAME] --entry <registered-id> -- <arguments>
```

The existing default distribution is used when no distribution is specified.
The Rust manager runs a fixed guest entrypoint as the existing non-root user
and bounds the owned process tree in a per-run user service; it does not take
ownership of the distribution or require a preinstalled runtime marker.
Cancellation, timeout, and parent exit must drain only that request.
The manager never terminates a distribution, calls global `wsl --shutdown`,
or changes `.wslconfig`.

WSL's `MemoryMax` follows the host's total-capacity emergency ceiling rather
than free memory at session start. Native Cargo and C work has no managed heap
to collect, so a GC request cannot be counted as completed for that guest;
kernel reclaim and the cgroup hard boundary remain its safety mechanisms.

The guest still needs a working user service manager and the relevant pinned
toolchain. An unavailable containment or toolchain capability is reported
before running work; no privileged provisioning is attempted.

## Git and release promotion

Git operations use the Codex Git integration when available or ordinary
`git`/`gh` commands. The manager does not proxy branch, worktree, fetch, commit,
push, or GitHub API operations. Destructive ref changes and force pushes still
require explicit user authorization.

Promotion to `main` uses a normal fast-forward after the required check succeeds
for the exact candidate SHA. Verify the remote SHA after push and update the
default checkout with `--ff-only`. CI polling is a caller policy, not a manager
feature.

## Building the manager

The exact Rust toolchain remains pinned by `rust-toolchain.toml`:

```text
cargo build --locked -p clearra-manage --release
cargo test --locked -p clearra-manage
```

The binary is emitted under `build/cargo/default/release/`. PowerShell and Node
callers use the checked-in resolver helpers and may override the path with
`CLEARRA_MANAGE_BIN` for a verified prebuilt binary.
