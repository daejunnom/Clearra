# Clearra management policy

`config/clearra-management.v1.json` is the single policy source for generated
paths, toolchain versions, package authority, and lossless Git convergence.
`python -B _local/clearra_manage.py` is the common entry point. A new writer or
process launcher must register its source path, producer, output class, and
lifecycle before CI accepts it.

## Storage and toolchains

Managed repository output is limited to these roots:

| Root | Purpose | Lifetime |
| --- | --- | --- |
| `build/<producer>/<profile>` | Cargo targets, project tools, package and container staging, publication inputs | reproducible until receipted cleanup |
| `coverage/<producer>` | coverage output | ephemeral until receipted cleanup |
| `_local/artifacts/<class>/<run-id>` | test, benchmark, analysis, and raw research evidence | retained evidence |
| `_local/state/<producer>/<run-id>` | locks, checkpoints, and transaction receipts | transaction lifetime |
| `_local/tmp/<producer>/<run-id>` | atomic-write and process temporary files | process lifetime |
| `docs/research` | reviewed summaries selected for Git | source controlled |

The product build owner also has a registered Clearra-only external cache at
the platform `Clearra/build` root. npm, pnpm, Cargo, and rustup keep their normal
shared user stores. The management command records identity and byte deltas but
never cleans another project's entries.

Use these checks before a managed operation:

```text
python -B _local/clearra_manage.py storage audit
python -B _local/clearra_manage.py storage verify
python -B _local/clearra_manage.py toolchain check
python -B _local/clearra_manage.py deps verify
```

Run a registered producer with:

```text
python -B _local/clearra_manage.py storage run --producer cargo -- cargo check --workspace --locked
```

The local TTY-only unmanaged-output override is intentionally unavailable to
CI, release, deployment, Git ref changes, credential paths, and link or mount
escapes. Its warning states that forcing is not recommended.

## Dependency and package changes

pnpm is the sole workspace installer. Frozen installs use:

```text
python -B _local/clearra_manage.py deps install
```

Only the dependency-update command may mutate lockfiles. It starts from a clean
worktree, records before and after package graphs and authority-file hashes,
and rejects changes outside the selected manager's files:

```text
python -B _local/clearra_manage.py deps update --manager pnpm -- ctk3 --latest
python -B _local/clearra_manage.py deps update --manager cargo -- -p package-name --precise 1.2.3
```

Publishing is a two-step exact-tarball operation. `pack` runs pnpm, rejects
lifecycle changes to tracked source, checks every tar member, and seals the
package name, version, content list, source SHA, tree, and tarball digest.
`publish` defaults to validation only. `--apply` verifies the same source and
tarball again, checks the exact npm version and registry identity, then invokes
`npm publish <exact-tarball> --provenance --ignore-scripts` once.

```text
python -B _local/clearra_manage.py package pack --package ctk3
python -B _local/clearra_manage.py package publish --receipt <pack-receipt>
python -B _local/clearra_manage.py package publish --receipt <pack-receipt> --apply
```

## Lossless Git convergence

Run the phases explicitly. None of these commands polls CI.

1. `git inventory --fetch` fetches all branches and tags without pruning,
   removes shallow history, checks object closure, and records all refs,
   worktrees, and dirty states.
2. `git converge` creates safety refs, a verified bundle, uncommitted patches,
   a source archive, and a review ledger. It makes no branch decision.
3. `git review` records each inclusion or exclusion with an immutable decision
   receipt. A dirty worktree can be selected only after `--record-worktree`
   proves that its archived state reconstructs an exact candidate tree.
4. `git converge --apply` replays selected linear history on the current
   `codex/converge-*` branch. A conflict records stage blobs and worktree
   hashes, preserves a safety ref, aborts, and restores the initial candidate.
5. Push the candidate normally. The push starts the required candidate CI; do
   not poll it.
6. `git promote` later reads required checks once for the exact candidate SHA.
   Only a successful closed check set proceeds. It verifies the GitHub ruleset
   and maintainer set, uses a normal fast-forward `candidate:main` push, reads
   remote main back, advances the clean default local main, and retains an
   independently validated checkout.
7. `git finalize` is a dry run. After reviewing its complete removal plan,
   `git finalize --apply` removes only reviewed or proven-equivalent branches,
   clean worktrees, the retained verification checkout, and safety state. It
   finishes only when remote main and the default local main are the sole
   working copies with the same commit and tree.

Examples for review decisions:

```text
python -B _local/clearra_manage.py git review --safety-receipt <receipt> --candidate <branch> --decide-ref refs/heads/topic --decision selected --reason "required source change"
python -B _local/clearra_manage.py git review --safety-receipt <receipt> --candidate <branch> --decide-worktree <absolute-path> --decision excluded --reason "generated local experiment"
```

Raw `git pull`, destructive reset, force push, pre-verification deletion,
prune, and GC are outside this contract.
