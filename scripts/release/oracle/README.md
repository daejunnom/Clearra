# Oracle v0.8.0 inactive staging

This directory is the tracked, non-secret authority for staging the exact
v0.8.0 Oracle candidate. The ignored `src/admin/deploy/oracle` v0.7.5
bootstrap and invoker remain historical one-shots. Their release ID, archive
hashes, entry counts, active baseline, and temporary paths must not be reused
for v0.8.0.

The v0.8 path has six boundaries:

1. `create-local-layers-v080.sh` creates the private no-config overlay, the
   accepted CTK3 distribution, and the minimal production dependency layer
   from explicit allowlists. The hosted-Actions-only
   `create-actions-layers-v080.sh` creates only the latter two archives and
   never probes or consumes the ignored private overlay. Both builders verify
   an explicit accepted artifact directory bound to one source commit,
   canonical run ID, and run attempt; repo-local `packages/ctk3/dist` is never
   an input. They publish each archive once without overwriting an existing
   path.
2. `invoke-freeze-v080.ps1` and the root-owned
   `clearra-oracle-freeze-v080` verify the local bytes and pinned host. The
   wrapper accepts either one local overlay archive or one canonical,
   root-owned Oracle-only sealed overlay path plus its filename-bound SHA-256;
   those modes are mutually exclusive. It never returns or copies sealed
   private bytes to the runner. The root helper seals the assembled inputs
   against further deployment-account writes, then snapshots only
   hashes/sizes/service identity from the active release, assemble an
   independent candidate with the active private config copied internally, and
   return one canonical manifest. Neither private config nor settings bytes are
   returned, printed, or written to the local evidence directory.
3. `create-inactive-stage-v080.mjs` accepts that canonical, closed manifest and
   renders `clearra-oracle-inactive-stage-v080.template`. Every non-secret
   authority is baked into the generated one-shot before it reaches Oracle.
4. `invoke-inactive-stage-v080.ps1` verifies all local bytes, the pinned Oracle
   host key, the generated one-shot hash, every remote upload hash and mode,
   and the exact final attestation. In remote-overlay mode it also binds the
   sealed authority SHA-256 to `manifest.layers.overlay.sha256`, copies it only
   inside Oracle, and rechecks the copied owner, mode, size, canonical path,
   and digest. `-AuditOnly` performs only local typed/path/hash checks and
   process execution; it does not open SSH, create a remote directory, read the
   identity file, or claim that the sealed remote file exists.
5. The root-owned one-shot holds the existing release-deploy lock, independently
   validates all tar members and the assembled tree, stages only
   `/opt/clearra/releases/v0.8.0-<sha7>`, transactionally updates the canonical
   launcher and tree digester, and verifies that the active link, settings,
   service process, active tree, and active private config did not change.
6. `candidate-settings-v080.mjs` renders the exact non-secret 13-line candidate
   settings contract and exposes only its SHA-256 through the CLI.
   `invoke-release-deploy-v080.ps1` is the typed local boundary for rollback
   capture, candidate activation/proof, and exact prior restoration/proof. It
   reuses the pinned host-key authority and only checks that the approved
   identity path is a regular non-reparse leaf; it never opens or hashes that
   identity file.

Successful freezing removes its remote upload, root helper, and independent
candidate assembly after returning the manifest. Successful staging removes
the second remote upload directory, root input/stage directories, and root
bootstrap before returning its attestation. A failed run keeps its nonce-bound
upload for exact investigation, never activates the candidate, and the staging
one-shot restores a canonical tool transition that had begun. The final
candidate directory may remain only when it is already a fully validated,
immutable candidate with the expected tree digest.

## Frozen inputs

Create and record these inputs only after the final accepted commit exists.
The layer builder and freeze wrapper are the canonical producers; do not hand
assemble a substitute manifest:

- `source.tar.gz`: the commit-byte archive created by
  `scripts/release/create-exact-source-archive.mjs`. Never substitute a working
  tree archive or a Windows-extracted directory.
- `private-overlay-no-config.tar`: an explicit frozen allowlist below
  `apps/clearra-discord-bot/src/admin/`. It must not contain
  `apps/clearra-discord-bot/src/admin/config.mjs`, settings, `.env` files,
  credentials, or keys. Do not build it with a broad directory glob.
  GitHub-hosted runners must instead use an already sealed Oracle authority at
  `/opt/clearra/sealed-release-inputs/private-overlay-no-config-<sha256>.tar`.
  Every parent in that absolute path must be a root-owned, non-link directory
  with no group/other write authority. The source must be a nonempty root:root
  regular file at mode `0600` with exactly one link. The root helper opens it
  with `O_NOFOLLOW`, validates metadata and SHA-256 from that same descriptor,
  and copies it to a root-only `O_EXCL` temporary file. It then fsyncs and
  re-hashes the copy before tar validation; the temporary copy is removed on
  success and failure while the sealed original is never a cleanup target.
  Its filename SHA-256, configured SHA-256, live digest, and manifest digest
  must all agree. Creating this authority is a
  separate root bootstrap. Never store the overlay bytes in a GitHub Secret,
  workflow artifact, checkout, runner workspace, or log.
- `ctk3-dist.tar`: the canonical accepted CTK3 artifact, rooted only at
  `packages/ctk3/dist`. Its
  `clearra-accepted-ctk3.v2.json` binds the exact source commit, canonical
  workflow run ID, run attempt, sorted file set, sizes, and SHA-256 values.
  Rebuilding CTK3 locally or reading repo-local `packages/ctk3/dist` is not an
  authorized substitute.
- `node_modules.tar`: the frozen production dependency layer, rooted only at
  `node_modules`. Its only links must be
  `node_modules/ctk3 -> ../packages/ctk3` and
  `node_modules/@clearra/discord-bot -> ../../apps/clearra-discord-bot`.
- The non-secret active baseline observed by the root freeze immediately before
  staging preparation: exact
  active release path and tree SHA-256, settings byte length and SHA-256, and
  active `config.mjs` byte length and SHA-256. Do not read or record settings or
  config contents.
- The independently assembled candidate tree SHA-256 and final POSIX counts
  after the same normalization used by the one-shot: root-owned directories at
  `0755`, ordinary files at `0644`, the eight declared runtime helpers at
  `0755`, and exactly the two dependency links above.
- SHA-256 and byte length for the tracked v0.8 launcher and digester in this
  directory, plus the exact currently installed prior launcher/digester
  authority. Use `null` for `prior` only when the corresponding canonical path
  is known to be absent. A current tool that already equals the new tool is
  accepted without a transition.

The candidate tree digest is computed by the tracked root freeze with the tracked
`clearra-release-tree-digest.py` against a separate POSIX assembly that uses the
exact active private config bytes. It must not be learned from the staging run
that consumes it; the generated manifest is an independent frozen input to the
later stage.

## Canonical manifest

The manifest schema is `clearra.oracle.inactive-stage.v080.v1`. JSON key order,
two-space indentation, LF endings, and one final newline are part of the input
contract. The generator rejects duplicate/extra keys indirectly by requiring
the original bytes to equal `JSON.stringify(value, null, 2) + "\n"`.

```json
{
  "schemaVersion": "clearra.oracle.inactive-stage.v080.v1",
  "sourceCommit": "<full-lowercase-40-character-commit>",
  "releaseId": "v0.8.0-<same-sha7>",
  "active": {
    "releasePath": "/opt/clearra/releases/<exact-active-release-id>",
    "treeSha256": "<active-tree-sha256>",
    "settingsSha256": "<active-settings-sha256>",
    "settingsSize": 0,
    "configSha256": "<active-config-sha256>",
    "configSize": 0
  },
  "candidate": {
    "treeSha256": "<candidate-tree-sha256>",
    "counts": {
      "directories": 0,
      "files0644": 0,
      "files0755": 8,
      "symlinks": 2
    }
  },
  "layers": {
    "source": {
      "sha256": "<source-archive-sha256>",
      "size": 0,
      "counts": { "files": 0, "directories": 0, "symlinks": 0 }
    },
    "overlay": {
      "sha256": "<overlay-archive-sha256>",
      "size": 0,
      "counts": { "files": 0, "directories": 0, "symlinks": 0 }
    },
    "ctk3Dist": {
      "sha256": "<ctk3-dist-archive-sha256>",
      "size": 0,
      "counts": { "files": 0, "directories": 0, "symlinks": 0 }
    },
    "dependencies": {
      "sha256": "<dependencies-archive-sha256>",
      "size": 0,
      "counts": { "files": 0, "directories": 0, "symlinks": 2 }
    }
  },
  "tools": {
    "launcher": {
      "sha256": "<tracked-launcher-sha256>",
      "size": 0,
      "prior": { "sha256": "<installed-launcher-sha256>", "size": 0 }
    },
    "digester": {
      "sha256": "<tracked-digester-sha256>",
      "size": 0,
      "prior": null
    }
  }
}
```

The zeros and angle-bracket values above are documentation placeholders and are
intentionally rejected. Build the real object in the displayed key order and
write it with a UTF-8 encoder without a byte-order mark.

## Freeze, audit, and stage

Create the exact public source archive from the accepted commit:

```powershell
$sourceCommit = '<full-lowercase-40-character-accepted-commit>'
$sourceArchive = '<absolute-output-directory>/source.tar.gz'
node scripts/release/create-exact-source-archive.mjs `
  --source-commit $sourceCommit `
  --output $sourceArchive
if ($LASTEXITCODE -ne 0) { throw 'exact source archive failed' }
```

Download the one accepted CTK3 artifact from the already resolved canonical
acceptance run, verify its embedded source/run/attempt authority, and create the
three local layers in a new evidence directory. Both directories must already
exist, the accepted directory must be empty before download, and every layer
output filename must be absent. Do not run a local CTK3 build:

```powershell
$repository = (Get-Location).Path
$acceptedRunId = '<canonical-successful-workflow-dispatch-run-id>'
$acceptedRunAttempt = '<exact-positive-run-attempt>'
$acceptedCtk3ArtifactName = "ctk3-accepted-$sourceCommit-run-$acceptedRunId-attempt-$acceptedRunAttempt"
$acceptedCtk3Directory = '<new-absolute-accepted-ctk3-directory>'
$evidenceDirectory = '<new-absolute-evidence-directory>'

gh run download $acceptedRunId `
  --name $acceptedCtk3ArtifactName `
  --dir $acceptedCtk3Directory
if ($LASTEXITCODE -ne 0) { throw 'accepted CTK3 artifact download failed' }
node scripts/tools/accepted-ctk3-dist.mjs `
  --verify $acceptedCtk3Directory `
  --expected-source-commit $sourceCommit `
  --expected-run-id $acceptedRunId `
  --expected-run-attempt $acceptedRunAttempt
if ($LASTEXITCODE -ne 0) { throw 'accepted CTK3 artifact authority failed' }

$repositoryWsl = (& wsl.exe -e wslpath -a -- $repository).Trim()
$acceptedCtk3Wsl = (& wsl.exe -e wslpath -a -- $acceptedCtk3Directory).Trim()
$evidenceWsl = (& wsl.exe -e wslpath -a -- $evidenceDirectory).Trim()
& wsl.exe -e bash "$repositoryWsl/scripts/release/oracle/create-local-layers-v080.sh" `
  $repositoryWsl `
  $acceptedCtk3Wsl `
  $sourceCommit `
  $acceptedRunId `
  $acceptedRunAttempt `
  $evidenceWsl
if ($LASTEXITCODE -ne 0) { throw 'Oracle local layer freeze failed' }

$overlayArchive = Join-Path $evidenceDirectory 'private-overlay-no-config.tar'
$ctk3DistArchive = Join-Path $evidenceDirectory 'ctk3-dist.tar'
$dependenciesArchive = Join-Path $evidenceDirectory 'node_modules.tar'
$manifestPath = Join-Path $evidenceDirectory 'oracle-inactive-stage-v080.json'
```

GitHub Actions uses the same accepted CTK3 identity but creates only the public
CTK3 and dependency layers:

```bash
bash scripts/release/oracle/create-actions-layers-v080.sh \
  "$GITHUB_WORKSPACE" \
  "$RUNNER_TEMP/accepted-ctk3" \
  "$CLEARRA_SOURCE_COMMIT" \
  "$ACCEPTED_RUN_ID" \
  "$ACCEPTED_RUN_ATTEMPT" \
  "$RUNNER_TEMP/oracle-layers"
```

The workflow supplies the private layer only as two non-secret references. The
archive path must be exactly derived from the digest; the archive itself stays
on Oracle:

```powershell
$remoteOverlayArchive = $env:ORACLE_PRIVATE_OVERLAY_ARCHIVE
$remoteOverlaySha256 = $env:ORACLE_PRIVATE_OVERLAY_SHA256

pwsh -NoProfile -File scripts/release/oracle/invoke-freeze-v080.ps1 `
  -SourceCommit $sourceCommit `
  -SourceArchive $sourceArchive `
  -RemoteOverlayArchive $remoteOverlayArchive `
  -RemoteOverlaySha256 $remoteOverlaySha256 `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -ManifestOutput $manifestPath `
  -AuditOnly
```

Removing `-AuditOnly` and adding `-IdentityFile <runner-temporary-key-path>`
performs the freeze. The wrapper passes only the canonical sealed path and
expected SHA-256 to the root helper. The sealed archive is absent from the
uid/gid 1001 upload inventory and neither the wrapper nor deployment account
opens, hashes, copies, or returns its bytes. The identity file must be created
from the separately protected GitHub Environment secret in the runner temporary
directory and removed in an `always()` cleanup step. A repository- or
organization-scoped SSH secret is forbidden; neither wrapper reads or hashes
the identity contents.

Audit the local freeze inputs without contacting Oracle. This verifies regular
non-reparse inputs, computes their size/hash authority, validates the pinned
host record, and checks the root helper syntax. Tar-member and active-baseline
validation occur only in the full root freeze, so `AuditOnly` does not claim a
candidate digest and does not create `$manifestPath`:

```powershell
pwsh -NoProfile -File scripts/release/oracle/invoke-freeze-v080.ps1 `
  -SourceCommit $sourceCommit `
  -SourceArchive $sourceArchive `
  -OverlayArchive $overlayArchive `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -ManifestOutput $manifestPath `
  -AuditOnly
```

Then perform the read-only Oracle freeze. This does not create a release under
`/opt/clearra/releases`, change `/opt/clearra/current`, restart the service, or
install canonical tools. It publishes the canonical manifest once:

```powershell
pwsh -NoProfile -File scripts/release/oracle/invoke-freeze-v080.ps1 `
  -SourceCommit $sourceCommit `
  -SourceArchive $sourceArchive `
  -OverlayArchive $overlayArchive `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -ManifestOutput $manifestPath `
  -IdentityFile '<approved-Oracle-identity-file>'
```

Audit all generated manifest bindings and, when an evidence copy is required,
create the exact root staging one-shot without overwriting an existing path:

```powershell
node scripts/release/oracle/create-inactive-stage-v080.mjs `
  --manifest $manifestPath `
  --audit
if ($LASTEXITCODE -ne 0) { throw 'Oracle stage manifest audit failed' }

$bootstrapEvidence = '<new-absolute-evidence-path>/clearra-oracle-inactive-stage-v080'
node scripts/release/oracle/create-inactive-stage-v080.mjs `
  --manifest $manifestPath `
  --output $bootstrapEvidence
if ($LASTEXITCODE -ne 0) { throw 'Oracle stage bootstrap generation failed' }
node scripts/release/oracle/create-inactive-stage-v080.mjs `
  --manifest $manifestPath `
  --check $bootstrapEvidence
if ($LASTEXITCODE -ne 0) { throw 'Oracle stage bootstrap check failed' }
```

Then audit all staging upload bytes without contacting Oracle:

```powershell
pwsh -NoProfile -File scripts/release/oracle/invoke-inactive-stage-v080.ps1 `
  -ManifestPath $manifestPath `
  -SourceArchive $sourceArchive `
  -OverlayArchive $overlayArchive `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -AuditOnly
```

The hosted Actions form substitutes the two remote references for the local
overlay and is otherwise identical:

```powershell
pwsh -NoProfile -File scripts/release/oracle/invoke-inactive-stage-v080.ps1 `
  -ManifestPath $manifestPath `
  -SourceArchive $sourceArchive `
  -RemoteOverlayArchive $remoteOverlayArchive `
  -RemoteOverlaySha256 $remoteOverlaySha256 `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -AuditOnly
```

For the real inactive stage, replace `-AuditOnly` with the temporary
`-IdentityFile` path. The wrapper rejects a remote digest that differs from the
frozen manifest before contacting Oracle.

The audit must return exactly `oracle_inactive_stage_invoker=audit-ok` followed
by the source, release, candidate-tree, and generated-bootstrap identities. It
does not accept or inspect an identity file.

Run the same command without `-AuditOnly` and with the approved identity path
only after the local attestation has been recorded:

```powershell
pwsh -NoProfile -File scripts/release/oracle/invoke-inactive-stage-v080.ps1 `
  -ManifestPath $manifestPath `
  -SourceArchive $sourceArchive `
  -OverlayArchive $overlayArchive `
  -Ctk3DistArchive $ctk3DistArchive `
  -DependenciesArchive $dependenciesArchive `
  -IdentityFile '<approved-Oracle-identity-file>'
```

Never print, hash in logs, copy, or inspect the identity file. The wrapper passes
its path only to the local OpenSSH client, enforces the pinned ED25519 host key,
disables agents, forwarding, proxying, password and interactive authentication,
and permits only the closed non-secret remote token grammar.

The expected success result is:

```text
oracle_inactive_stage=ready
oracle_source_commit=<full-commit>
oracle_release_id=v0.8.0-<sha7>
oracle_release_sha256=<candidate-tree-sha256>
oracle_launcher_sha256=<tracked-launcher-sha256>
oracle_tree_digester_sha256=<tracked-digester-sha256>
oracle_stage_nonce=<64-lowercase-hex>
```

This result proves an inactive staged release and installed deployment tools.
It does not prove candidate activation, Discord behavior, Cloud Run traffic,
the observation window, tagging, or immutable release publication.

## Typed activation and rollback boundary

After Cloud Run has produced the exact tagged zero-traffic candidate URL, derive
the Oracle candidate settings authority from the canonical 13-line renderer.
The CLI writes only one lowercase SHA-256 line; it never writes the settings:

```powershell
$candidateUrl = '<exact-credential-free-HTTPS-candidate-origin>'
$oracleCandidateSettingsSha256 = (& node `
  scripts/release/oracle/candidate-settings-v080.mjs `
  --source-commit $sourceCommit `
  --candidate-url $candidateUrl `
  --hash-only).Trim()
if ($LASTEXITCODE -ne 0 -or
    $oracleCandidateSettingsSha256 -cnotmatch '^[0-9a-f]{64}$') {
  throw 'canonical Oracle candidate settings authority failed'
}
```

Use `invoke-release-deploy-v080.ps1` for all four remote operations. Its
operation-specific argument sets fail closed when a capture, candidate, or
restore field crosses into another operation. `-AuditOnly` validates the pinned
host authority, launcher syntax, typed values, canonical URLs/timestamp, proof
path, and exact remote argv without opening SSH or inspecting an identity file:

```powershell
$oracleWrapper = Join-Path (Get-Location) `
  'scripts/release/oracle/invoke-release-deploy-v080.ps1'
$oracleIdentityFile = '<approved-Oracle-identity-file>'
$oracleCommon = @{
  ScriptReleaseId = $oracleCandidateReleaseId
  ScriptReleaseSha256 = $oracleCandidateReleaseSha256
  DeploymentNonce = $deploymentNonce
}

& $oracleWrapper @oracleCommon `
  -Operation capture-rollback-authority `
  -PriorRevision $priorRevision `
  -PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind `
  -AuditOnly
if ($LASTEXITCODE -ne 0) { throw 'Oracle capture invocation audit failed' }

& $oracleWrapper @oracleCommon `
  -Operation verify-candidate `
  -Proof $oracleCandidateProofPath `
  -SourceCommit $sourceCommit `
  -CandidateUrl $candidateUrl `
  -CandidateRevision $candidateRevision `
  -OracleReleaseId $oracleCandidateReleaseId `
  -OracleReleaseSha256 $oracleCandidateReleaseSha256 `
  -OracleSettingsSha256 $oracleCandidateSettingsSha256 `
  -VerifiedAfter $deploymentVerifiedAfter `
  -AuditOnly
if ($LASTEXITCODE -ne 0) { throw 'Oracle candidate invocation audit failed' }
```

Record both audit attestations, then rerun each exact operation without
`-AuditOnly` and with `-IdentityFile $oracleIdentityFile`. The capture returns
one closed JSON object. Candidate success is exactly
`oracle_candidate=verified`; restore success is exactly
`oracle_rollback=verified`. The Cloud Run deployment runbook supplies the
captured prior release/settings/runtime fields to
`restore-prior-and-verify`. Never bypass this wrapper with a local `sudo`, a
free-form SSH command, or direct execution of a release helper.

For the 20-minute production observation, invoke the same typed wrapper with
`-Operation observe-candidate`. The approved runtime may supply the identity
path through `CLEARRA_ORACLE_IDENTITY_FILE` so it is never materialized in a
probe specification or report. The operation is read-only: it does not create
a proof file, change settings/current, or restart the service. Success is one
`clearra.oracle.candidate-observation.v1` JSON line binding the source,
candidate URL/revision, active release path/tree, settings digest, deployment
nonce, PID, systemd monotonic process start, boot ID, READY evidence, fresh
operation timestamp, observation timestamp, the exact `VerifiedAfter`, and
runtime identity. The journal reader chooses the qualifying successful `/path`
with the greatest canonical timestamp regardless of output order. Across the
window, require all identity/process/`VerifiedAfter` fields to remain unchanged.
The first sample accepts the candidate-verification operation as a baseline;
each later sample requires `freshOperationAt` to be newer than the preceding
remote `observedAt`, while every remote observation timestamp increases
strictly. The production authority and report both require an exact 1,200-second
interval, exactly two samples, sample 0 equal to `started_at`, and sample 1 equal
to `ended_at`. Perform one confirmed real `/path` strictly after the start
sample and before the end sample.

## Regression commands

```powershell
node --test scripts/release/oracle-freeze-v080.test.mjs
node --test scripts/release/oracle-inactive-stage-v080.test.mjs
node --test scripts/release/oracle/candidate-settings-v080.test.mjs
node --test scripts/release/oracle/create-actions-layers-v080.test.mjs
node --test scripts/tools/accepted-ctk3-dist.test.mjs
node --test apps/clearra-discord-bot/test/oracle-candidate-observation.test.mjs
pwsh -NoProfile -File scripts/release/oracle/invoke-freeze-v080.test.ps1
pwsh -NoProfile -File scripts/release/oracle/invoke-inactive-stage-v080.test.ps1
pwsh -NoProfile -File scripts/release/oracle/invoke-release-deploy-v080.test.ps1
$wslRepository = (wsl.exe -e wslpath -a -- (Get-Location).Path).Trim()
wsl.exe -e bash -n "$wslRepository/scripts/release/oracle/create-local-layers-v080.sh"
wsl.exe -e bash -n "$wslRepository/scripts/release/oracle/create-actions-layers-v080.sh"
wsl.exe -e dash -n "$wslRepository/scripts/release/oracle/clearra-oracle-freeze-v080"
wsl.exe -e dash -n "$wslRepository/scripts/release/oracle/clearra-oracle-release-deploy-v080"
```
