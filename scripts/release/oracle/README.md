# Oracle v0.8.0 inactive staging

This directory is the tracked, non-secret authority for staging the exact
v0.8.0 Oracle candidate. The ignored `src/admin/deploy/oracle` v0.7.5
bootstrap and invoker remain historical one-shots. Their release ID, archive
hashes, entry counts, active baseline, and temporary paths must not be reused
for v0.8.0.

The v0.8 path has five boundaries:

1. `create-local-layers-v080.sh` creates the private no-config overlay, the
   current CTK3 distribution, and the minimal production dependency layer from
   explicit allowlists. It publishes each archive once without overwriting an
   existing path.
2. `invoke-freeze-v080.ps1` and the root-owned
   `clearra-oracle-freeze-v080` verify the local bytes and pinned host, seal the
   uploaded inputs against further deployment-account writes, snapshot only
   hashes/sizes/service identity from the active release, assemble an
   independent candidate with the active private config copied internally, and
   return one canonical manifest. Neither private config nor settings bytes are
   returned, printed, or written to the local evidence directory.
3. `create-inactive-stage-v080.mjs` accepts that canonical, closed manifest and
   renders `clearra-oracle-inactive-stage-v080.template`. Every non-secret
   authority is baked into the generated one-shot before it reaches Oracle.
4. `invoke-inactive-stage-v080.ps1` verifies all local bytes, the pinned Oracle
   host key, the generated one-shot hash, every remote upload hash and mode,
   and the exact final attestation. `-AuditOnly` performs only local reads and
   process execution; it does not open SSH, create a remote directory, or
   require an identity file.
5. The root-owned one-shot holds the existing release-deploy lock, independently
   validates all tar members and the assembled tree, stages only
   `/opt/clearra/releases/v0.8.0-<sha7>`, transactionally updates the canonical
   launcher and tree digester, and verifies that the active link, settings,
   service process, active tree, and active private config did not change.

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
- `ctk3-dist.tar`: the frozen CTK3 distribution, rooted only at
  `packages/ctk3/dist`.
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

Build the CTK3 distribution and create the three local layers in a new evidence
directory. The output directory must already exist and every output filename
must be absent:

```powershell
npm run build --workspace ctk3
if ($LASTEXITCODE -ne 0) { throw 'CTK3 production build failed' }

$repository = (Get-Location).Path
$evidenceDirectory = '<new-absolute-evidence-directory>'
$repositoryWsl = (& wsl.exe -e wslpath -a -- $repository).Trim()
$evidenceWsl = (& wsl.exe -e wslpath -a -- $evidenceDirectory).Trim()
& wsl.exe -e bash "$repositoryWsl/scripts/release/oracle/create-local-layers-v080.sh" `
  $repositoryWsl $evidenceWsl
if ($LASTEXITCODE -ne 0) { throw 'Oracle local layer freeze failed' }

$overlayArchive = Join-Path $evidenceDirectory 'private-overlay-no-config.tar'
$ctk3DistArchive = Join-Path $evidenceDirectory 'ctk3-dist.tar'
$dependenciesArchive = Join-Path $evidenceDirectory 'node_modules.tar'
$manifestPath = Join-Path $evidenceDirectory 'oracle-inactive-stage-v080.json'
```

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

## Regression commands

```powershell
node --test scripts/release/oracle-freeze-v080.test.mjs
node --test scripts/release/oracle-inactive-stage-v080.test.mjs
pwsh -NoProfile -File scripts/release/oracle/invoke-freeze-v080.test.ps1
pwsh -NoProfile -File scripts/release/oracle/invoke-inactive-stage-v080.test.ps1
$wslRepository = (wsl.exe -e wslpath -a -- (Get-Location).Path).Trim()
wsl.exe -e bash -n "$wslRepository/scripts/release/oracle/create-local-layers-v080.sh"
wsl.exe -e dash -n "$wslRepository/scripts/release/oracle/clearra-oracle-freeze-v080"
wsl.exe -e dash -n "$wslRepository/scripts/release/oracle/clearra-oracle-release-deploy-v080"
```
