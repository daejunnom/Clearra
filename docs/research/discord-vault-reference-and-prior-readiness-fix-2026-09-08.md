# Discord persistent Vault references and prior startup — 2026-09-08

## Confirmed failure, not a product search regression

At main `3571fd9754f75d197c16fe33f0324cc3a19cfcca`, canonical acceptance
`34199108210` and Pages publication `34201607382` succeeded. Discord primary
`34201495681/1` passed the zero-traffic smoke and warm direct-CLI/Discord
comparison, then failed Oracle activation. Recovery `34204897233` did not
produce verified recovery authority.

Oracle's candidate launcher reported `Discord token OCID is required.` The
candidate settings file deliberately contains exactly 13 non-secret runtime
settings, replacing the previous file. Discord/job Vault references existed only
in that previous settings surface, not in persistent systemd Environment entries.
This is a lost reference at the deployment boundary, not a missing Vault value.

Signed-in OCI console metadata confirmed all six Clearra secrets active. No
secret content, version payload, key file or credential file was opened. Effective
systemd metadata confirmed the existing admin, event and moderation-identity
references matched the console resource identifiers; only the Discord and job
references were absent. The existing moderation-identity reference already
pointed to the console's `clearra-runtime-identity-key`, so that mapping was not
inferred from the name. The telemetry transport reference has a separate owner
and was not changed.

## Minimal operator repair

The exact legacy prior was `/opt/clearra/releases/v0.7.4-701454b`. Its release tree
and settings digests matched the sealed prestage capture, but the release parent
directory was owned by 1001:1001 rather than root. The unchanged closed runtime
classifier rejected it as `active-filesystem-authority-invalid`.

After verifying the captured prior, current symlink, exact tree/settings digests
and old directory metadata, change **only that directory's ownership**, not its
children, release bytes, settings or current symlink. Starting that exact prior
then reached a stable active process with the exact immutable prior cwd. An
independent typed readback classified it as `prior / exact-prior-authority`.
This operator action is not a substitute for protected recovery evidence.

Add the two missing non-secret OCID references in the new root-owned systemd
drop-in `20-persistent-vault-references.conf`, without overwriting any existing
drop-in or modifying settings. Reload unit metadata without restarting the
service. Readback confirmed all five required reference names match console
metadata and the prior MainPID remains unchanged. No Vault access policy was
expanded, no secret value was created or rotated, and no candidate was promoted
by this operator repair.

## Source corrections

- Require all five persistent Vault references before candidate settings,
  symlink or process mutation. Report missing names only, never reference values
  or secret payloads. Keep the closed 13-line settings contract.
- A Type=simple service can report active before its launcher reaches the stable
  process cwd. Restore now waits for a bounded exact-prior MainPID/cwd readiness
  condition instead of a single immediate read. Failure still stops the service;
  foreign or unverified paths never pass.
- Include only closed allowlisted Oracle classification reasons and Cloud tag
  cleanup HTTP/stage reasons. Raw exceptions, response bodies and credentials
  are never echoed. No authority or tag-ownership guard is relaxed.
- Read-only Cloud service/revision planning still verified the exact owned
  candidate tag and unchanged prior 100% traffic. This does not prove the
  deployment identity's PATCH permission, and the cleanup fix must not be
  reported as a completed tag removal.

## Verification and retry boundary

The bounded release regression pool passed: **603 tests, 602 pass, one skip,
zero failures**. The focused restore fixture includes delayed process readiness
and closed failure cases. Native Rust product execution was not run on this host.

Dispatch protected recovery for original `34201495681/1`, then fresh exact-main
canonical acceptance and the matching Pages queue. Their existing protected
Discord continuation remains the production owner. Do not wait for new CI or
deployment results in this work session; dispatch is not a success claim.

Minimum-cover algorithm experiments, serial 4195 batches, external comparisons
and raw reports remain on the separate hotfix/local-only surfaces. They are not
part of this production correction, do not modify 4194 and grant no release
authority.
