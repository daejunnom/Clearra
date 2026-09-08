# Discord live-promotion and recovery correction

## Observed failure

Deployment [34225230861](https://github.com/daejunnom/Clearra/actions/runs/34225230861)
passed acceptance/debt resolution, candidate preparation, Oracle freeze,
zero-traffic Cloud deployment, and warm CLI parity. Live Oracle verification
lost its SSH connection with exit 255 (`Broken pipe`). The original verifier
remained alive holding the release lock, so compensation returned exit 75.

The live candidate's process ID was `1568848`. The proof producer's
`^[2-9][0-9]*$` check rejected this valid PID: it excluded every leading `1`,
not just PID 1. Its operation check also admitted only legacy `path` while the
current product's actual operational logger emits `pc.path`. The bounded
journal inspection found READY but no completed operation for that process;
neither observation is a successful end-to-end proof.

Primary failure cleanup removed `candidate-965d994` even though Oracle restore
had failed, then ended with an unverified-cleanup error. Subsequent recovery
[34229289199](https://github.com/daejunnom/Clearra/actions/runs/34229289199)
could not classify the Oracle candidate: its configured tag URL no longer
served `/health`. Cloud traffic remained on the exact prior revision at 100%.
The strict Cloud non-traffic comparison is not relaxed by this correction.

## Changes

- Validate PID as a canonical safe positive integer greater than 1. Cover
  observed production PIDs and reject zero, PID 1, signs, leading zeros,
  fractions, exponent notation, trailing data, and unsafe integers.
- Admit only `path` and `pc.path` as equivalent proof-operation labels. Test
  against the actual product logger, retaining current-process, freshness,
  gateway/slash, success, release, and settings checks.
- Keep SSH alive every 15 seconds with four missed replies allowed. Emit
  non-secret proof-wait progress on stderr, preserving exact stdout
  attestations. Handle PIPE through the existing fail-closed transition cleanup.
- Defer candidate tag removal after an Oracle transition until the exact
  rollback marker exists. The protected recovery owner retains the endpoint
  needed to classify and restore that candidate. Test the actual PowerShell
  guard for all transition/rollback combinations.
- Restore settings with root-owned mode 0600, matching freeze and staging,
  instead of reintroducing 0644 during rollback. The executable rollback
  sandbox now asserts the requested installation mode.

OpenSSH's encrypted-channel liveness options are documented in
[ssh_config](https://man.openbsd.org/ssh_config#ServerAliveInterval).
These transport changes do not substitute for a real completed Discord path
request or expand runtime/IAM authority.

## Operational recovery and verification

The orphaned exact candidate verifier was terminated normally. Before manual
restoration, the original run's candidate-state bindings were verified against
its downloaded artifact, including the exact prior rollback capture. The
installed candidate release tree matched its sealed digest. Its existing
digest-guarded restore tool then restored `v0.7.4-701454b`; settings permissions
were returned to 0600. No secret value was displayed and no immutable release
source was patched in place.

Protected recovery
[34231149360](https://github.com/daejunnom/Clearra/actions/runs/34231149360)
subsequently succeeded, including terminal evidence verification and upload
(artifact `10057945078`). This clears the failed attempt's recovery debt; it
does not mean the new v0.8.0 candidate has been deployed.

Focused checks cover the producer/observer, executable rollback sandbox,
PowerShell SSH transport, workflow cleanup guard, and existing bounded Cloud
cleanup tests. The modified POSIX scripts also pass syntax checks. New source
must receive its own canonical acceptance before production promotion. Keep
the existing design-plan edits and hotfix/TB work separate from this release fix.

## Release regression harness follow-up

Canonical run
[34232117018](https://github.com/daejunnom/Clearra/actions/runs/34232117018)
failed before product builds or deployment: the PowerShell availability probe
hit its 10-second bound and the executable candidate-cleanup guard hit its
15-second bound. In the same four-worker pool, the successful inactive-stage
and prestage transport PowerShell fixtures took 21.0 and 23.3 seconds. This is
consistent with cold-start/runner contention, not evidence of a new IAM denial.
The failed assertions hid the process error behind a generic availability
message and `null !== 0`. Pages queue 34232120674 stopped on the upstream
acceptance failure; Discord run 34232220885 was skipped.

Use one test-only PowerShell subprocess contract: explicit noninteractive,
profile-free, shell-free arguments; closed stdin; hidden Windows processes;
a 60-second subprocess bound with forced termination; and error-code, signal,
and timeout diagnostics without printing invocation arguments. Apply it to
all four PowerShell-backed release test files, including formerly unbounded
calls. Expected script exit failures remain observable. Only a genuinely
missing local executable may be skipped; timeouts and all CI probe failures
remain failures. There are no automatic test retries or production timeout,
IAM, approval, rollback, or acceptance-policy changes.

The single bounded pool remains at up to four workers and retains every
existing regression. Its manifest adds the subprocess-contract regression.
On Windows with Node 24.16.0, PowerShell 7.6.5, and `CI=true`, all 612 tests
in 51 files passed in 21.9 seconds, with zero failures or skips. The fresh
canonical dispatch remains responsible for Ubuntu/Node 22 and full product
acceptance; local harness success is not production-deployment evidence.

## Delegated process-boundary validation follow-up

Canonical run
[34234316638](https://github.com/daejunnom/Clearra/actions/runs/34234316638)
passed metadata (including the corrected PowerShell tests), Rust, WASM,
Pages acceptance, the CLI/desktop builds, Discord tests, sanitizer, and the
other foundation leaves. NoProductDebt's static validation was the remaining
failure: it still required `spawnSync`, the full PowerShell argument prefix,
and `shell: false` inside the Oracle caller after those responsibilities had
moved into the shared process runner. Updating that static boundary was missed
in the preceding change. Pages queue 34234320455 consequently stopped and
Discord 34236957813 was skipped; no new runtime permission denial occurred.

The static check now reads the caller and shared runner as separate physical
owners. It requires the exact import and file/cwd delegation in the caller,
and the real subprocess implementation, noninteractive flags, shell-free
execution, closed stdin, hidden process, bounded timeout, and error propagation
in the runner. The original exact completion marker/count checks remain.
The process-contract regression is also required in the existing single test
manifest; no extra full test pool or product build was added.

The three CI errors were reproduced locally before the change. Afterwards,
the exact Release Identity Gate passed, as did six focused Node tests including
the executable Oracle transport regression. A new regression executes the
actual static boundary via its PowerShell AST, requires its single connection
to the release identity gate, and rejects 18 in-memory weakened caller/runner
variants without modifying repository files. This does not substitute for
fresh canonical acceptance or claim that production deployment is complete.

The full static command used by NoProductDebt also passed under Windows
PowerShell: eight tasks, zero errors, 31.3 seconds. It still reports 97
non-blocking module-size/cohesion warnings; those were not suppressed by this
fix. No unrelated Rust/WASM build or full product test rerun was performed
locally for this static-validator correction.
