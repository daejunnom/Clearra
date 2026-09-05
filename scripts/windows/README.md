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
