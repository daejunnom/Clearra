# Clearra local-services watchdog

`install-clearra-local-services-watchdog.ps1` replaces the two legacy local
scheduled tasks with one `Clearra Local Services Watchdog` task. The task starts
through `wscript.exe`, and the launcher and the direct `node.exe`/`ssh.exe`
children are hidden. No command-shell process is used.

The single watchdog owns one named mutex and checks ports `4194` and `8790`
every 60 seconds. Any existing listener is authoritative and is preserved,
regardless of which process owns it. A process the watchdog has just started is
also retained while it is still warming up, preventing duplicate Vite or SSH
starts before a listener appears.

Install from an ordinary PowerShell session, supplying the local SSH key path
and destination without committing either value:

```powershell
& .\scripts\windows\install-clearra-local-services-watchdog.ps1 `
  -SshKeyPath '<local-key-path>' `
  -SshDestination '<user>@<host>'
```

Installation is idempotent. It atomically stages the runtime files under Local
AppData and registers one `IgnoreNew` task. If the same task is already
running, its definition is updated for the next safe start without stopping
that instance or its Vite/SSH children. A differently named idle legacy task is
removed; a running legacy task is only disabled for future triggers. Listener
PIDs already bound to ports `4194` and `8790` are checked before and after the
migration and must remain unchanged.

## One-off GUI experiments: 4195 only

Keep the installed watchdog and the user's long-lived GUI on 4194. Never start
another watchdog for an experiment, and never fall back to 4196 or an ephemeral
port. From the tooling checkout, run:

```powershell
node scripts/tools/run-gui-experiment.mjs --source-root '<absolute-experiment-worktree>' --lease-minutes 30
```

This helper refuses an occupied 4195 without adopting or terminating its owner.
The existing experimental listeners are a separate manual cleanup task; they
are not retroactively owned by this helper. A newly created server has a default
30-minute lease (explicitly configurable from 1 to 120 minutes), no automatic
restart, strict port binding, and no HMR refresh. Ctrl+C, parent exit/disconnect,
or lease expiry closes only that invocation's server. The child has an independent
lease to prevent an unresponsive parent from leaving it running indefinitely.
When starting the helper in the background on Windows use `Start-Process` with
`-WindowStyle Hidden`; its own child also uses `windowsHide` and no command shell.

The lease applies only to this finite experiment server, not CLI/GUI search time.
There is intentionally no HTTP-idle timeout: active browser-local WASM searches
need not send requests. Choose a sufficient explicit lease before a long audit,
and end the helper when that audit finishes. Ports 4194 and 8790 are never touched.
