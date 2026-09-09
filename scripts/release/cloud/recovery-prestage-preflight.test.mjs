import assert from "node:assert/strict";
import test from "node:test";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { powerShellTestOptions, runPowerShellTest } from "../../tools/powershell-test-process.mjs";

const source = await readFile(new URL("../invoke-discord-runtime-recovery-v080.ps1", import.meta.url), "utf8");
const functions = [...source.matchAll(/^function [\w-]+ \{[\s\S]*?^\}/gm)].map((x) => x[0]).join("\n");
const prestage = source.slice(source.indexOf("if ($Stage -ceq 'prestage') {"), source.indexOf("\n$candidateState = Verify-LiveAuthority"));
const psOptions = powerShellTestOptions();
const setup = `
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
${functions}
$script:Calls = 0
$script:ChildExit = 0
$script:ChildArgs = @()
function node {
    $script:Calls += 1
    $script:ChildArgs = @($args)
    [Console]::Error.WriteLine('fixture-child-diagnostic')
    Write-Output 'fixture-success-stream-must-not-escape'
    $global:LASTEXITCODE = $script:ChildExit
}
$GcpProjectId = 'clearra-cloud'
$GcpRegion = 'asia-northeast1'
$ArtifactRoot = 'fixture-artifact'
$SourceCommit = 'a' * 40
$OriginalWorkflowRunId = '1'
$OriginalWorkflowRunAttempt = '1'
$prior = [pscustomobject]@{ prior_revision = 'clearra-current-job-prior' }
$intent = [pscustomobject]@{
    cloud_candidate_revision = 'clearra-current-job-candidate'
    cloud_candidate_tag = 'candidate-fixture'
    deployment_nonce = 'fixture-nonce'
}
$script:Service = [pscustomobject]@{ status = [pscustomobject]@{ traffic = @(
    [pscustomobject]@{ revisionName = $prior.prior_revision; percent = 100 },
    [pscustomobject]@{ revisionName = $intent.cloud_candidate_revision; tag = $intent.cloud_candidate_tag }
) } }
`;
function executePs(body) {
  const directory = mkdtempSync(join(tmpdir(), "clearra-preflight-behavior-"));
  try {
    const scriptPath = join(directory, "fixture.ps1");
    writeFileSync(scriptPath, setup + body, { encoding: "utf8", mode: 0o600 });
    // -Command can inherit a false $? from an intentionally caught exception.
    // -File measures script completion instead. Do not append exit 0 or reset
    // production error state: uncaught throws must still fail the process.
    return runPowerShellTest(["-File", scriptPath]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function runPs(body) {
  const result = executePs(body);
  assert.equal(result.status, 0, result.stderr || String(result.error));
  assert.doesNotMatch(result.stdout, /fixture-success-stream-must-not-escape/);
  return result;
}

test("prestage checks the Cloud cleanup preimage after prior authority and before the first Oracle call", () => {
  assert.ok(prestage.startsWith("if ($Stage -ceq 'prestage') {"));
  const prior = prestage.indexOf("prestage cleanup refuses Cloud traffic outside exact prior authority");
  const preflight = prestage.indexOf("Assert-PrestageCloudCleanupPreimage");
  const oracle = prestage.indexOf("scripts/release/oracle/invoke-freeze-v080.ps1");
  const seal = prestage.indexOf("Seal-ExactCandidateCloudResidue");
  assert.ok(prior >= 0 && preflight > prior && oracle > preflight && seal > oracle);
  assert.match(prestage, /--binding "cloud_cleanup_readback=/);
  assert.match(prestage, /--binding "cloud_candidate_residue_readback=/);
  assert.match(source, /if \(\$candidateTagEntryCount -eq 1\)[\s\S]*remove-recovery-candidate-tag\.mjs/);
});

test("tagless prior state needs no cleanup readback helper and emits no authority", psOptions, () => {
  runPs(`
$script:Service.status.traffic = @($script:Service.status.traffic[0])
$result = @(Assert-PrestageCloudCleanupPreimage -Service $script:Service -Intent $intent -PriorRevision $prior.prior_revision)
if ($script:Calls -ne 0 -or $result.Count -ne 0) { throw 'tagless preflight must not invoke the cleanup helper' }
`);
});

test("tagged prior state uses only read-only preimage mode and exact original intent arguments", psOptions, () => {
  const result = runPs(`
$output = @(Assert-PrestageCloudCleanupPreimage -Service $script:Service -Intent $intent -PriorRevision $prior.prior_revision)
if ($script:Calls -ne 1 -or $output.Count -ne 0) { throw 'preflight call or stdout differs' }
$expected = @('scripts/release/cloud/remove-recovery-candidate-tag.mjs', '--project', $GcpProjectId,
    '--region', $GcpRegion, '--intent', "$ArtifactRoot/prestage/intended-candidate-authority.json",
    '--prior-revision', $prior.prior_revision, '--source-commit', $SourceCommit,
    '--workflow-run-id', $OriginalWorkflowRunId, '--workflow-run-attempt', $OriginalWorkflowRunAttempt,
    '--deployment-nonce', $intent.deployment_nonce, '--validate-only')
if (($expected -join '|') -cne ($script:ChildArgs -join '|')) { throw 'preflight argument mismatch' }
`);
  assert.match(result.stderr, /fixture-child-diagnostic/);
});

test("an actAs denial from the actual cleanup helper remains the top-level cause", psOptions, () => {
  runPs(`
$script:ChildExit = 77
try {
    Invoke-NodeExact scripts/release/cloud/remove-recovery-candidate-tag.mjs --fixture-actual-cleanup
    throw 'unexpected success'
} catch {
    if ($_.Exception.Message -notmatch 'Cloud candidate-tag cleanup blocked: iam.serviceAccounts.actAs was denied') { throw }
    if ($_.Exception.Message -match 'tracked recovery validator failed') { throw 'root cause was lost' }
}
if ($script:Calls -ne 1) { throw 'denial was retried' }
`);
});

test("unrelated child failure is contextualized without echoing arbitrary arguments", psOptions, () => {
  const result = runPs(`
$script:ChildExit = 19
try { Invoke-NodeExact secret-fixture-path --token fixture-secret; throw 'unexpected success' }
catch {
    if ($_.Exception.Message -notmatch 'tracked recovery validator failed \\(helper=authority-validator exit_code=19\\)') { throw }
    if ($_.Exception.Message -match 'fixture-secret|secret-fixture-path') { throw 'argument leak' }
}
`);
  assert.doesNotMatch(result.stdout + result.stderr, /fixture-secret|secret-fixture-path/);
});

test("foreign prior and ambiguous candidate tags fail before helper invocation", psOptions, () => {
  runPs(`
try { Assert-PrestageCloudCleanupPreimage -Service $script:Service -Intent $intent -PriorRevision 'foreign'; throw 'unexpected success' }
catch { if ($_.Exception.Message -notmatch 'outside exact prior authority') { throw } }
$script:Service.status.traffic += $script:Service.status.traffic[1]
try { Assert-PrestageCloudCleanupPreimage -Service $script:Service -Intent $intent -PriorRevision $prior.prior_revision; throw 'unexpected success' }
catch { if ($_.Exception.Message -notmatch 'sealed candidate residue') { throw } }
if ($script:Calls -ne 0) { throw 'invalid authority reached helper' }
`);
});

test("actual prestage branch stops at a failed preimage read without an Oracle artifact or result", psOptions, async () => {
  const directory = await mkdtemp(join(tmpdir(), "clearra-denied-preflight-"));
  try {
    runPs(`
$EvidenceRoot = '${directory.replaceAll("'", "''")}'
$Stage = 'prestage'
$RestoreOnly = $false
$RemoteOverlaySha256 = 'b' * 64
$RemoteOverlayArchive = "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$RemoteOverlaySha256.tar"
$intent | Add-Member -NotePropertyName remote_overlay_archive -NotePropertyValue $RemoteOverlayArchive
$intent | Add-Member -NotePropertyName remote_overlay_sha256 -NotePropertyValue $RemoteOverlaySha256
function Get-ActiveCloudRevision {
    param([string] $OutputPath)
    $script:Service | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
    return 'clearra-current-job-prior'
}
$script:ChildExit = 19
try {
${prestage}
    throw 'unexpected success'
} catch {
    if ($_.Exception.Message -notmatch 'tracked recovery validator failed \\(helper=cloud-candidate-tag-cleanup exit_code=19\\)') { throw }
}
$files = @(Get-ChildItem -LiteralPath $EvidenceRoot)
if ($files.Count -ne 1 -or $files[0].Name -notlike 'cloud-prestage-before-*.json') { throw 'work continued after preimage read failure' }
if ($script:Calls -ne 1) { throw 'preimage read failure was not terminal for this invocation' }
`);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("uncaught actual-cleanup actAs denial still fails the PowerShell fixture process", psOptions, () => {
  const result = executePs(`
$script:ChildExit = 77
Invoke-NodeExact scripts/release/cloud/remove-recovery-candidate-tag.mjs --fixture-actual-cleanup
`);
  assert.equal(result.status, 1, result.stderr || String(result.error));
  assert.match(result.stderr, /Cloud candidate-tag cleanup blocked/);
  assert.doesNotMatch(result.stdout, /fixture-success-stream-must-not-escape/);
});

test("an assertion failure after an expected caught denial still fails the process", psOptions, () => {
  const result = executePs(`
$script:ChildExit = 77
try {
    Invoke-NodeExact scripts/release/cloud/remove-recovery-candidate-tag.mjs --fixture-actual-cleanup
    throw 'unexpected success'
} catch {
    if ($_.Exception.Message -notmatch 'iam.serviceAccounts.actAs was denied') { throw }
}
throw 'fixture-assertion-failed-after-caught-denial'
`);
  assert.equal(result.status, 1, result.stderr || String(result.error));
  assert.match(result.stderr, /fixture-assertion-failed-after-caught-denial/);
});
