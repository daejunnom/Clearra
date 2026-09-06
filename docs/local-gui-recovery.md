# Local GUI recovery

The existing Windows "Clearra Local Services Watchdog" task starts at user
logon and checks port 4194 every 60 seconds. Occupied ports and running owned
startup processes are preserved. The launcher and child processes remain hidden.

Recovery starts Vite directly, bound to 127.0.0.1:4194 with strict port selection.
It does not run npm predev or compile WASM. The local-recovery serve mode accepts
the existing artifact even when its source fingerprint differs from the working
tree, with a warning. Manifest format and artifact size/hash checks still apply.
Normal builds and subsequently published artifact updates retain strict source
freshness verification. This mode is not evidence of a latest-source build.

The scheduled task runs after login, not before login. Source watchdog changes
must also be installed into the existing startup-v2 runtime to take effect.
