# Clearra repository rules

`config/clearra-management.v1.json` governs only two things:

1. where Clearra-generated output may be written; and
2. memory, timeout, descendant-process, and WSL lifetime limits for risky runs.

The active manager is the Rust binary in `tools/clearra-manage`. The Python v1
implementation under `scripts/management/history/python-v1/` is historical
source and must not be used as an execution entrypoint.

## Work that does not require the manager

Ordinary read-only filesystem commands and source inspection are unrestricted.
This includes `Get-ChildItem`, `rg`, `git status`, `git diff`, and equivalent
tools. They do not create Clearra artifacts and must not be wrapped by the
manager.

Git is not a management-CLI domain. Prefer the Codex Git integration when it
is available; ordinary `git` and `gh` commands are also allowed. Normal branch
creation, fetch, add, commit, and non-force push do not need a receipt or a
registered process profile. Do not force-push, discard uncommitted work, or
delete an unreviewed branch/worktree unless the user explicitly authorizes it.
Promotion to `main` remains a normal exact-SHA fast-forward after the required
CI check; the Rust manager does not proxy or poll GitHub.

Cargo, pnpm, npm metadata queries, toolchain checks, package packing, and
publishing are not management-CLI domains. Use their native commands and the
committed lockfiles. Cargo defaults to `build/cargo/default` through
`.cargo/config.toml`, so direct Cargo commands retain the output-path contract.

Short-lived builds, formatting, linting, unit tests, source generators that
already write exclusively inside a declared root, and programs already inside
a finite CI/container memory boundary do not need `runtime run` merely because
they start a process.

## Generated output paths

Clearra-generated files must stay inside a repository or platform root declared
in `config/clearra-management.v1.json`. Use the prebuilt manager when a caller
accepts an output path or when the path is otherwise uncertain:

```text
clearra-manage storage verify --path <path>
clearra-manage storage audit
```

Path verification rejects traversal, symlink/junction escapes, and credential
paths. The local interactive override requires both
`--force-unmanaged-output` and `--force-reason`; its warning states that forcing
is not recommended. CI, release/deployment output, credential paths, and link
escapes must never use the override.

Do not write raw execution output directly into `docs/research/`. Only a
human-reviewed summary selected for version control belongs there. Never read,
archive, print, or inspect `.env` files, keys, service-account files, API keys,
or credential files.

## Memory and process-tree supervision

Use `clearra-manage runtime run` for work that can consume large or unbounded
memory, has a long-lived lease, launches a descendant tree that must die with
its owner, or runs a benchmark whose worker and memory identity must be kept:

```text
clearra-manage runtime run --producer <label> --profile <profile> \
  --timeout <seconds> -- <command>
```

The supervisor owns the complete tree, applies a Windows Job Object or Linux
process group boundary, enforces the declared hard memory/process/output/time
limits, and never retries OOM with changed resources. It may request
cooperative GC during host pressure, but records GC only when the child writes
the matching acknowledgement. If the small recovery reserve remains
unavailable, it fail-closes only the owned tree.

Do not silently reduce requested worker counts. Solver hot paths remain free of
supervisor code; containment belongs at the outer process boundary.

WSL execution is always a supervised case. `clearra-manage runtime wsl run`
owns only `Clearra-Build`, uses a fixed registered guest entrypoint, and
terminates only that distribution after the lease. Never call global
`wsl --shutdown`. Other distributions and `.wslconfig` are outside Clearra's
ownership.

## Fixed local ports

- `4194`: local product-test GUI
- `4195`: finite local benchmark/A/B GUI
- `8790`: Discord bot management surface through its local SSH forward

Do not substitute these ports or let one role adopt another role's listener.
