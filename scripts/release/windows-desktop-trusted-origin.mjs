// Pure origin binding for the local Windows startup consumer. The caller owns
// Actions authentication and constructing complete attempt-specific API inputs.
// ZIP hashes must be observed from the downloaded bytes, not copied from the
// API. The caller also owns safe extraction and reads acceptedEvidence from
// the canonical ZIP whose hash it observed. This module never performs I/O.
import {
  requireExactKeys, requirePlainObject, requireSha256, requireSourceCommit,
} from './canonical-release-evidence.mjs';
import { selectAcceptedWindowsDesktopStartupArtifact } from './windows-desktop-startup-attestation.mjs';

const WORKFLOW = '.github/workflows/release-cli.yml';
const EXPECTED_KEYS = ['repository', 'source_commit', 'run_id', 'run_attempt', 'release_version', 'pages_base_path'];
const GUI_JOB = 'windows-gui';
const CANONICAL_JOB = 'canonical-evidence';
const API_DIGEST = /^sha256:([0-9a-f]{64})$/u;

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

function positiveId(value, label) {
  const id = Number.isSafeInteger(value) ? String(value) : value;
  requireThat(typeof id === 'string' && /^[1-9][0-9]{0,19}$/u.test(id), `${label} is not a positive decimal ID`);
  return id;
}

function verifyAttempt(attempt, expected, runId, runAttempt) {
  requirePlainObject(attempt, 'Actions attempt');
  const repositoryId = positiveId(attempt.repository?.id, 'attempt repository ID');
  requireThat(positiveId(attempt.id, 'attempt run ID') === runId &&
    positiveId(attempt.run_attempt, 'attempt number') === runAttempt &&
    attempt.repository.full_name === expected.repository &&
    attempt.head_repository?.full_name === expected.repository &&
    positiveId(attempt.head_repository?.id, 'attempt head repository ID') === repositoryId &&
    attempt.head_branch === 'main' && attempt.head_sha === expected.source_commit &&
    attempt.path === WORKFLOW && attempt.event === 'workflow_dispatch' &&
    attempt.status === 'completed' && attempt.conclusion === 'success',
  'Actions attempt is not the exact successful canonical main run');
  return repositoryId;
}

function completeList(payload, key, label) {
  requirePlainObject(payload, label);
  const entries = payload[key];
  requireThat(Number.isSafeInteger(payload.total_count) &&
    Array.isArray(entries) && entries.length === payload.total_count,
  `${label} must contain the complete paginated list`);
  const ids = new Set();
  for (const entry of entries) {
    requirePlainObject(entry, `${label} entry`);
    const id = positiveId(entry.id, `${label} ID`);
    requireThat(!ids.has(id), `${label} repeats entity ID ${id}`);
    ids.add(id);
  }
  return entries;
}

function verifyJobs(payload, sourceCommit, runId, runAttempt) {
  const jobs = completeList(payload, 'jobs', 'attempt job catalog');
  for (const job of jobs) {
    requireThat(positiveId(job.run_id, 'job run ID') === runId &&
      positiveId(job.run_attempt, 'job attempt') === runAttempt && job.head_sha === sourceCommit,
    'attempt job catalog mixes sources, runs, or attempts');
  }
  function requireJob(name) {
    const matches = jobs.filter((job) => job.name === name);
    requireThat(matches.length === 1 &&
      matches[0].status === 'completed' && matches[0].conclusion === 'success',
    `no sole completed successful ${name} job in the chosen attempt`);
    return matches[0];
  }
  const producer = requireJob(GUI_JOB);
  requireJob(CANONICAL_JOB);
  // The successful exact-attempt jobs and both digest-bound uploads are the
  // evidence. Display titles of the steps that produced them may change.
  return positiveId(producer.id, 'Windows GUI producer job ID');
}

function selectArtifact(catalog, name, repositoryId, runId, sourceCommit) {
  const matches = catalog.filter((entry) => entry.name === name);
  requireThat(matches.length === 1, `no unique artifact named ${name}`);
  const artifact = matches[0];
  const artifactId = positiveId(artifact.id, 'artifact ID');
  const run = artifact.workflow_run;
  requireThat(artifact.expired === false && run &&
    positiveId(run.id, 'artifact workflow run ID') === runId &&
    positiveId(run.repository_id, 'artifact repository ID') === repositoryId &&
    positiveId(run.head_repository_id, 'artifact head repository ID') === repositoryId &&
    run.head_sha === sourceCommit && run.head_branch === 'main',
  `artifact ${name} is expired or not owned by the exact source run`);
  const apiSha = typeof artifact.digest === 'string' && API_DIGEST.exec(artifact.digest);
  requireThat(apiSha, `API archive SHA-256 for ${name} is missing or invalid`);
  return Object.freeze({ artifactId, name, sha256: apiSha[1] });
}

// Validates authenticated API metadata BEFORE any archive is acquired. This
// selection alone carries no authority to stage an executable.
export function selectWindowsDesktopTrustedOriginArtifacts({ expected, attempt, jobs, artifactCatalog }) {
  requireExactKeys(expected, EXPECTED_KEYS, 'expected canonical acceptance identity');
  const sourceCommit = requireSourceCommit(expected.source_commit);
  const runId = positiveId(expected.run_id, 'expected run ID');
  const runAttempt = positiveId(expected.run_attempt, 'expected run attempt');
  const repositoryId = verifyAttempt(attempt, expected, runId, runAttempt);
  const producerJobId = verifyJobs(jobs, sourceCommit, runId, runAttempt);

  const catalog = completeList(artifactCatalog, 'artifacts', 'run artifact catalog');
  const guiName = `clearra-windows-gui-v${expected.release_version}-run-${runId}-attempt-${runAttempt}`;
  const canonicalName = `canonical-acceptance-evidence-${sourceCommit}-run-${runId}-attempt-${runAttempt}`;
  const gui = selectArtifact(catalog, guiName, repositoryId, runId, sourceCommit);
  const canonical = selectArtifact(catalog, canonicalName, repositoryId, runId, sourceCommit);
  requireThat(gui.sha256 !== canonical.sha256, 'GUI and canonical ZIP archives cannot be the same bytes');
  return Object.freeze({ gui, canonical, sourceCommit, runId, runAttempt, producerJobId });
}

function verifyArtifactObservation(selected, observed) {
  requireExactKeys(observed, ['artifact_id', 'sha256'], 'independent raw ZIP observation');
  requireThat(positiveId(observed.artifact_id, 'observed artifact ID') === selected.artifactId,
    `raw ZIP was obtained from a different artifact ID than ${selected.name}`);
  requireThat(requireSha256(observed.sha256, 'observed raw ZIP SHA-256') === selected.sha256,
    `API archive digest differs from the observed ZIP for ${selected.name}`);
}

export function buildWindowsDesktopTrustedOrigin({
  expected, attempt, jobs, artifactCatalog, acceptedEvidence, downloadedArchives,
}) {
  const { gui, canonical, sourceCommit, runId, runAttempt, producerJobId } =
    selectWindowsDesktopTrustedOriginArtifacts({ expected, attempt, jobs, artifactCatalog });
  requireExactKeys(downloadedArchives, ['gui', 'canonical'], 'raw ZIP observations');
  verifyArtifactObservation(gui, downloadedArchives.gui);
  verifyArtifactObservation(canonical, downloadedArchives.canonical);

  // Existing closed sidecar contract only; no extra authority or executor flag.
  const origin = Object.freeze({
    repository: expected.repository,
    workflow_path: WORKFLOW,
    source_commit: sourceCommit,
    run_id: runId,
    run_attempt: runAttempt,
    producer_job_id: producerJobId,
    artifact_id: gui.artifactId,
    artifact_name: gui.name,
    artifact_archive_sha256: gui.sha256,
    canonical_acceptance_sha256: acceptedEvidence?.report_sha256,
    release_version: expected.release_version,
    pages_base_path: expected.pages_base_path,
  });
  // Use the same canonical-envelope and unique GUI-role check as the future
  // consumer. The consumer additionally verifies the extracted EXE bytes.
  selectAcceptedWindowsDesktopStartupArtifact({ acceptedEvidence, trustedActionsOrigin: origin });
  return origin;
}
