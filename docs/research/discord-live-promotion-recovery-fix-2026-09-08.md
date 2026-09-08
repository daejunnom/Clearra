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
