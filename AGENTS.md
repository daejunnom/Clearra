# Clearra repository management rules

`config/clearra-management.v1.json` is the authority for generated paths,
toolchain versions, package managers, and Git convergence. Repository-provided
tools and automated agents must use `python -B _local/clearra_manage.py` for
storage, dependency, toolchain, and Git management operations.

Before adding or running a tool that writes files or starts another process,
register its source path, producer ID, output class, and lifecycle in the
manifest. Run it through `python -B _local/clearra_manage.py storage run
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

Never read, archive, print, or otherwise inspect `.env` files, keys, service
account files, API keys, or credential files. Report only that a prohibited
path blocked the operation.
