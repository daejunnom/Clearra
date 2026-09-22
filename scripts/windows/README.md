# Clearra local-services watchdog

`install-clearra-local-services-watchdog.ps1` replaces the two legacy local
scheduled tasks with one `Clearra Local Services Watchdog` task. The task starts
through `wscript.exe` and the Clearra `local-service` runtime supervisor. The
watchdog and its direct `node.exe`/`ssh.exe` descendants share one Job Object,
and all windows remain hidden. No command-shell process is used.

The single watchdog owns one named mutex and checks the local product-test GUI
on `4194` and the Discord bot management surface on `8790` every 60 seconds.
The latter is reached through the managed local SSH forward. Any existing
listener is authoritative and is preserved,
regardless of which process owns it. A process the watchdog has just started is
also retained while it is still warming up, preventing duplicate Vite or SSH
starts before a listener appears.

Install from an ordinary PowerShell session, supplying the local SSH key path
and destination without committing either value:

```powershell
. .\scripts\lib\clearra-manage-command.ps1
$manager = Get-ClearraManageExecutable -RepositoryRoot $PWD
& $manager --root $PWD runtime run `
  --producer local-watchdog --profile control --timeout 300 -- `
  powershell.exe -NoLogo -NoProfile -NonInteractive -File `
  .\scripts\windows\install-clearra-local-services-watchdog.ps1 `
  -SshKeyPath '<local-key-path>' `
  -SshDestination '<user>@<host>'
```

Installation is idempotent. It atomically stages the runtime files under
`%LOCALAPPDATA%\Clearra\state\local-services-v2`, writes its bounded log under
`%LOCALAPPDATA%\Clearra\logs`, and registers one `IgnoreNew` task. If the same task is already
running, its definition is updated for the next safe start without stopping
that instance or its Vite/SSH children. A differently named idle legacy task is
removed; a running legacy task is only disabled for future triggers. Listener
PIDs already bound to ports `4194` and `8790` are checked before and after the
migration and must remain unchanged.

## Finite A/B benchmark GUI: 4195 only

Keep the installed watchdog, the product-test GUI on 4194, and the Discord bot
management listener on 8790. Use 4195 only for the finite local A/B benchmark;
never fall back to 4196 or an ephemeral port. From the tooling checkout, run:

```powershell
. .\scripts\lib\clearra-manage-command.ps1
$manager = Get-ClearraManageExecutable -RepositoryRoot $PWD
& $manager --root $PWD runtime run `
  --producer browser --profile local-service --timeout 1800 -- `
  node scripts/tools/run-gui-experiment.mjs `
  --source-root '<absolute-benchmark-worktree>' --lease-minutes 30
```

This helper refuses an occupied 4195 without adopting or terminating its owner.
The existing experimental listeners are a separate manual cleanup task; they
are not retroactively owned by this helper. A newly created server has a default
30-minute lease (explicitly configurable from 1 to 120 minutes), no automatic
restart, strict port binding, and no HMR refresh. Ctrl+C, parent exit/disconnect,
or lease expiry closes only that invocation's server. The child has an independent
lease, while the outer Job Object closes the full tree if the runtime supervisor
is lost. Start the managed command in a hidden terminal when running it in the
background; its child also uses `windowsHide` and no command shell.

The lease applies only to this finite experiment server, not CLI/GUI search time.
There is intentionally no HTTP-idle timeout: active browser-local WASM searches
need not send requests. Choose a sufficient explicit lease before a long audit,
and end the helper when that audit finishes. The 4194 product-test and 8790
Discord management listeners are never touched.
