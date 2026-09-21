# Clearra repository management rules

`config/clearra-management.v1.json` is the authority for generated paths,
toolchain versions, package managers, and Git convergence. Repository-provided
tools and automated agents must use `python -B scripts/management/clearra_manage.py` for
storage, dependency, toolchain, and Git management operations.

Before adding or running a tool that writes files or starts another process,
register its source path, producer ID, output class, and lifecycle in the
manifest. Run it through `python -B scripts/management/clearra_manage.py storage run
--producer <id> -- <command>`. Do not write raw reports into `docs/research/`;
only a person-reviewed summary selected for version control belongs there.

Do not run raw `git pull`, destructive `git reset`, `git branch -D`, force
pushes, or branch/worktree deletion. Use the management entry point with the
`git inventory`, `git converge`, `git review`, `git upload`, `git promote`, and
`git finalize` subcommands; unresolved unique commits or dirty worktrees are
blockers. Record every selected or excluded item with `git review`; do not edit
the review JSON by hand. `git promote` performs one exact-SHA check lookup and
must not be wrapped in a polling loop.
Remote material may remain as a working copy only in the default checkout's
local `main` after exact-SHA CI, remote readback, local fast-forward, and an
independent checkout all agree.

pnpm is the workspace package manager. Do not run `npm install`, `npm ci`, or
general `npm exec`. npm is reserved for registry inspection and publication of
an already verified tarball. Installs must use the frozen pnpm lockfile. Use
`deps update` for a receipted lockfile update and `package pack` followed by
`package publish --apply` for publication; never publish a workspace directory.

Do not run `rustup update`, floating Rust toolchains, or `cargo install` into
the shared Cargo binary directory. The exact Rust toolchain is declared in
`rust-toolchain.toml`; Clearra-owned Cargo tools use the managed tool root.

Every new writer must add a producer and output class to the management
manifest before it writes. A path-policy warning may mention the local
`--force-unmanaged-output` escape hatch, but the warning must also state that
forcing is not recommended. CI, release, deployment, Git ref mutations,
credential paths, and link escapes never accept that override.

Every process execution point must also register a resource profile and the
complete tree-ownership, timeout, termination-grace, hard-memory, output-limit,
and no-OOM-retry contract in `config/clearra-management.v1.json`. Launch host
commands with `python -B scripts/management/clearra_manage.py runtime run --producer <id>
--profile <profile> -- <command>`; the compatible `storage run` command
delegates to the same supervisor. Do not add raw `spawn`, `Start-Process`,
`subprocess`, `std::process`, workflow/Docker launchers, or shell `exec` sites
without that registration.

The documented `storage audit|verify|clean`, `toolchain`, `deps`, `package`, and
`git` management subcommands enter their manifest-selected supervisor profile
automatically when called directly. `storage run` and `runtime run` are the
supervisor entrypoints themselves. If an outer managed command already owns the
tree, nested management commands inherit that boundary and do not create a
second supervisor.
The manifest automatically marks the `release-evidence` producer, applied
package publication, `main` promotion, and applied ruleset changes as release
contexts. Do not clear `CLEARRA_RELEASE` or bypass the finite cgroup/Job Object
requirement for those operations.

`scripts/management/clearra_runtime.py` is the only production source allowed to invoke
the WSL host executable. Do not invoke raw `wsl`, its `.exe` launcher, arbitrary
`bash -lc`, or the global WSL shutdown command. WSL work must use a registered
fixed guest entrypoint through `python -B scripts/management/clearra_manage.py runtime wsl
run --entry <id> -- <arguments>`. Clearra owns only the dedicated
`Clearra-Build` distribution, terminates only that distribution after each
lease, and treats `.wslconfig` as read-only. A new WSL entrypoint must declare
its profile and source requirement in the manifest before it runs.

Do not silently lower worker counts, enable normal-path `MemoryHigh` or RSS
sampling, or retry an OOM with different resources. Admission failure and OOM
must remain distinct typed failures, and the outer process boundary owns all
descendants without adding supervisor work to solver hot paths.

Never read, archive, print, or otherwise inspect `.env` files, keys, service
account files, API keys, or credential files. Report only that a prohibited
path blocked the operation.

Local port ownership is fixed: `4194` is the local product-test GUI, `4195` is
the finite A/B benchmark GUI, and `8790` is the Discord bot management surface
reached through its managed local SSH forward. Do not substitute one port for
another or let the benchmark helper adopt the product or management listener.
