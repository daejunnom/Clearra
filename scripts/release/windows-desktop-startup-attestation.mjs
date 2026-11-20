// Pure consumer: the caller must authenticate the Actions artifact origin and
// read the local manager receipt from its owned state directory. A digest binds
// those inputs; it is not a signature or permission to publish.
import { createHash } from 'node:crypto';
import { win32 } from 'node:path';
import { validateCanonicalAcceptanceEvidence } from './canonical-acceptance-evidence.mjs';
import {
  canonicalSha256,
  requireExactKeys,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
} from './canonical-release-evidence.mjs';

const WORKFLOW = '.github/workflows/release-cli.yml';
const PROBE = 'scripts/tools/verify-windows-desktop-runtime.ps1';
const PRODUCER = 'windows-desktop-startup';
const PROFILE = 'local-service';
const MANAGER_TIMEOUT = 75;
const PROBE_SCHEMA = 'clearra.windows-desktop-startup-readback.v1';
export const WINDOWS_DESKTOP_STARTUP_ATTESTATION_SCHEMA =
  'clearra.accepted-windows-desktop-startup-attestation.v1';

const ORIGIN_KEYS = [
  'repository', 'workflow_path', 'source_commit', 'run_id', 'run_attempt',
  'producer_job_id', 'artifact_id', 'artifact_name', 'artifact_archive_sha256',
  'canonical_acceptance_sha256', 'release_version', 'pages_base_path',
];
const OBSERVATION_PROOFS = [
  'private_supervisor_job_verified', 'gui_process_owned', 'visible_clearra_window',
  'isolated_webview2_renderer_owned', 'owned_webview_tree_drained',
];
const PROBE_KEYS = [
  'schema', 'source_commit', 'artifact_version', 'expected_artifact_sha256',
  'artifact_sha256_before', 'artifact_sha256_after', 'artifact_bytes', 'runtime_run_id',
  'status', 'reason', 'gui_shutdown', 'ui_route_verified', 'release_authority',
  ...OBSERVATION_PROOFS,
];
const NO_PROBE_WINDOW = new Set([
  'windows-x64-host-required', 'interactive-window-station-unavailable',
  'native-startup-probe-unavailable', 'private-supervisor-job-unverified',
]);
const OBSERVATION_UNAVAILABLE = new Set([
  'gui-process-not-job-owned', 'gui-window-not-observed',
  'isolated-webview2-renderer-not-observed', 'startup-readiness-not-stable',
  'owned-process-observation-unavailable',
]);
const PROBE_FAILURES = new Set([
  'required-supervisor-lease-missing', 'source-identity-mismatch', 'managed-root-contract-mismatch',
  'artifact-path-outside-managed-workspace', 'artifact-digest-mismatch', 'artifact-not-a-windows-image',
  'artifact-or-supervisor-input-unavailable', 'isolated-profile-creation-failed',
  'windows-gui-start-failed', 'gui-exited-before-readback', 'gui-exited-after-readback-before-close',
  'owned-gui-shutdown-unverified', 'owned-webview-tree-not-drained',
  'artifact-changed-during-probe', 'artifact-rehash-failed',
]);
const CLEAN_SHUTDOWN = new Set(['graceful', 'owned-root-forced']);
const SHUTDOWN_VALUES = new Set(['not-started', 'pending', 'already-exited', ...CLEAN_SHUTDOWN]);
const DISPOSITION_EXITS = new Map([['verified', 0], ['unavailable', 3], ['failed', 2]]);

function requireThat(value, message) {
  if (!value) throw new Error(message);
}

function requireDecimal(value, label) {
  requireThat(typeof value === 'string' && /^[1-9][0-9]{0,19}$/u.test(value),
    `${label} must be a positive decimal string`);
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function checkOrigin(origin, evidence) {
  requireExactKeys(origin, ORIGIN_KEYS, 'trusted Actions origin');
  for (const key of ['run_id', 'run_attempt', 'producer_job_id', 'artifact_id']) {
    requireDecimal(origin[key], `trusted Actions origin ${key}`);
  }
  requireSourceCommit(origin.source_commit);
  requireSha256(origin.artifact_archive_sha256, 'trusted Actions archive SHA-256');
  requireSha256(origin.canonical_acceptance_sha256, 'trusted canonical evidence SHA-256');
  requireThat(origin.workflow_path === WORKFLOW, 'origin must be the canonical release workflow');
  requireThat(origin.artifact_name ===
    `clearra-windows-gui-v${origin.release_version}-run-${origin.run_id}-attempt-${origin.run_attempt}`,
  'origin must select the exact Windows GUI artifact name and attempt');
  validateCanonicalAcceptanceEvidence(evidence, {
    sourceCommit: origin.source_commit, runId: origin.run_id, runAttempt: origin.run_attempt,
    repository: origin.repository, version: origin.release_version, basePath: origin.pages_base_path,
  });
  requireThat(evidence.report_sha256 === origin.canonical_acceptance_sha256,
    'canonical acceptance seal differs from the trusted origin');
  const producerJob = evidence.jobs.filter(({ name }) => name === 'windows-gui');
  requireThat(producerJob.length === 1 && producerJob[0].job_id === origin.producer_job_id,
    'Windows GUI producer job differs from the trusted origin');
}

function selectExecutable(evidence, version) {
  const entries = evidence.final_source_fragments.release_artifacts;
  const expected = new Map([
    ['linux-cli', `Clearra-CLI-v${version}-linux-x86_64`],
    ['windows-cli', `Clearra-CLI-v${version}-windows-x86_64.exe`],
    ['windows-gui', `Clearra-GUI-v${version}-windows-x86_64.exe`],
  ]);
  for (const [role, name] of expected) {
    const matches = entries.filter((entry) => entry.role === role);
    requireThat(matches.length === 1 && matches[0].name === name,
      `canonical acceptance must contain a unique versioned ${role} artifact`);
  }
  const entry = entries.find(({ role }) => role === 'windows-gui');
  return Object.freeze({
    role: 'windows-gui', name: entry.name, size_bytes: entry.size_bytes, sha256: entry.sha256,
  });
}

// Only selects trusted metadata. Call validateAcceptedWindowsDesktopStartupInput
// with the actual staged bytes before any startup attempt.
export function selectAcceptedWindowsDesktopStartupArtifact({ acceptedEvidence, trustedActionsOrigin }) {
  checkOrigin(trustedActionsOrigin, acceptedEvidence);
  return selectExecutable(acceptedEvidence, trustedActionsOrigin.release_version);
}

export function validateAcceptedWindowsDesktopStartupInput({
  acceptedEvidence, trustedActionsOrigin, executableBytes,
}) {
  const artifact = selectAcceptedWindowsDesktopStartupArtifact({ acceptedEvidence, trustedActionsOrigin });
  requireThat(executableBytes instanceof Uint8Array && executableBytes.length === artifact.size_bytes &&
    sha256(executableBytes) === artifact.sha256,
  'downloaded Windows GUI executable size or SHA-256 differs from acceptance');
  return artifact;
}

function absoluteWindowsPath(value, label) {
  // Reject alternate names and traversal instead of silently normalizing them
  // into the expected file or allowing a drive-relative/UNC substitute.
  requireThat(typeof value === 'string' && /^[a-z]:[\\/]/iu.test(value) &&
    !/[:*?"<>|\u0000-\u001f]/u.test(value.slice(2)), `${label} must be an absolute Windows drive path`);
  const parts = value.slice(3).split(/[\\/]/u);
  requireThat(parts.every((part) => part && !/[. ]$/u.test(part)),
    `${label} has a path alias or traversal`);
  return win32.normalize(value);
}

function pathKey(value) {
  return value.toLowerCase();
}

function checkInvocation(invocation, command, origin, artifact) {
  requireExactKeys(invocation, ['repository_root', 'executable_path', 'powershell', 'probe_timeout_seconds'],
    'expected probe invocation');
  const repositoryRoot = absoluteWindowsPath(invocation.repository_root, 'repository root');
  const executable = absoluteWindowsPath(invocation.executable_path, 'expected executable');
  const allowedRoots = ['build', '_local/artifacts'].map((root) =>
    pathKey(win32.join(repositoryRoot, root) + win32.sep));
  requireThat(allowedRoots.some((root) => pathKey(executable).startsWith(root)) &&
    win32.basename(executable) === artifact.name,
  'expected executable is not the versioned GUI file in a managed workspace root');
  const shell = invocation.powershell;
  requireThat(typeof shell === 'string' &&
    (/^pwsh(?:\.exe)?$/iu.test(shell) ||
      /^pwsh\.exe$/iu.test(win32.basename(absoluteWindowsPath(shell, 'expected PowerShell')))),
  'expected launcher must be PowerShell 7');
  const timeout = invocation.probe_timeout_seconds;
  requireThat(Number.isSafeInteger(timeout) && timeout >= 5 && timeout <= 45,
    'expected observation deadline is outside the finite probe budget');
  requireThat(Array.isArray(command) && (command.length === 13 || command.length === 15) &&
    command.every((arg) => typeof arg === 'string') &&
    command[0] === shell && command[1] === '-NoProfile' &&
    command[2] === '-NonInteractive' && command[3] === '-File',
  'supervisor command is not the exact direct PowerShell probe');
  const expectedProbe = pathKey(win32.join(repositoryRoot, PROBE));
  const seenProbe = /^scripts[\\/]tools[\\/]verify-windows-desktop-runtime\.ps1$/u.test(command[4])
    ? expectedProbe : pathKey(absoluteWindowsPath(command[4], 'observed probe path'));
  requireThat(seenProbe === expectedProbe, 'supervisor command invoked a different probe script');
  requireThat(command[5] === '-Executable' &&
    pathKey(absoluteWindowsPath(command[6], 'observed executable')) === pathKey(executable) &&
    command[7] === '-Version' && command[8] === origin.release_version &&
    command[9] === '-SourceCommit' && command[10] === origin.source_commit &&
    command[11] === '-ExpectedSha256' && command[12] === artifact.sha256 &&
    (command.length === 13 ? timeout === 40 :
      command[13] === '-TimeoutSeconds' && command[14] === String(timeout)),
  'supervisor command executable, source, digest, or observation deadline differs');
}

function singleCompactJsonLine(output, label) {
  requireThat(typeof output === 'string' && output.length <= 16 * 1024,
    `${label} must be a bounded JSON string`);
  const body = output.replace(/\r?\n$/u, '');
  requireThat(body.length > 0 && !/[\r\n]/u.test(body), `${label} must contain one JSON line`);
  let parsed;
  try {
    parsed = JSON.parse(body);
  } catch {
    throw new Error(`${label} must contain valid JSON`);
  }
  // The PowerShell producer uses ConvertTo-Json -Compress and emits only
  // typed ASCII scalars. A duplicate member or extra line is not that output.
  requireThat(JSON.stringify(parsed) === body, `${label} is not one compact JSON observation`);
  return { body, parsed };
}

function checkProbe(output, receipt, origin, artifact, invocation) {
  const { body, parsed: probe } = singleCompactJsonLine(output, 'raw startup readback');
  requireExactKeys(probe, PROBE_KEYS, 'raw startup readback');
  requireThat(receipt?.schema_id === 'clearra.runtime-receipt.v2' &&
    singleCompactJsonLine(receipt.stdout_tail, 'supervisor stdout tail').body === body,
  'raw startup readback is not the supervisor stdout observation');
  const outcome = receipt.outcome;
  requireThat(outcome?.producer === PRODUCER && outcome.profile === PROFILE &&
    /^[0-9]{1,24}-[1-9][0-9]{0,9}$/u.test(outcome.run_id ?? '') &&
    outcome.run_id === probe.runtime_run_id && outcome.timeout_seconds === MANAGER_TIMEOUT &&
    Number.isSafeInteger(outcome.duration_ms) && outcome.duration_ms >= 0 &&
    outcome.automatic_retry === false && outcome.supervisor_termination_reason === null &&
    outcome.error_code === null && outcome.process_tree_stopped === true &&
    outcome.descendant_processes_at_exit === 0 && outcome.child_termination?.kind === 'exit',
  'supervisor did not prove the finite unforced owned invocation and stopped tree');
  checkInvocation(invocation, outcome.command, origin, artifact);
  requireThat(probe.schema === PROBE_SCHEMA && probe.source_commit === origin.source_commit &&
    probe.artifact_version === origin.release_version && probe.expected_artifact_sha256 === artifact.sha256,
  'startup readback source, version, or expected digest differs from the chosen artifact');
  requireThat(OBSERVATION_PROOFS.every((key) => typeof probe[key] === 'boolean') &&
    probe.ui_route_verified === false && probe.release_authority === false &&
    SHUTDOWN_VALUES.has(probe.gui_shutdown),
  'startup readback contains invalid ownership or authority fields');
  requireThat((!probe.visible_clearra_window && !probe.isolated_webview2_renderer_owned) ||
    probe.gui_process_owned, 'window or renderer cannot outlive GUI ownership evidence');
  requireThat(!probe.gui_process_owned || probe.private_supervisor_job_verified,
    'GUI ownership requires a private supervisor job');

  const exitCode = DISPOSITION_EXITS.get(probe.status);
  requireThat(Number.isInteger(exitCode) && outcome.return_code === exitCode &&
    outcome.child_termination.code === exitCode &&
    outcome.reason === (exitCode === 0 ? 'normal' : 'nonzero'),
  'startup disposition disagrees with the supervisor child exit');
  const artifactUntouched = probe.artifact_sha256_before === null &&
    probe.artifact_sha256_after === null && probe.artifact_bytes === null;
  const artifactMatched = probe.artifact_sha256_before === artifact.sha256 &&
    probe.artifact_sha256_after === artifact.sha256 && probe.artifact_bytes === artifact.size_bytes;
  if (probe.status === 'verified') {
    requireThat(probe.reason === 'verified' && artifactMatched &&
      OBSERVATION_PROOFS.every((key) => probe[key]) && CLEAN_SHUTDOWN.has(probe.gui_shutdown),
    'verified startup requires matching before/after bytes and owned stable window, renderer and shutdown');
  } else if (probe.status === 'unavailable') {
    requireThat(!probe.owned_webview_tree_drained &&
      (NO_PROBE_WINDOW.has(probe.reason) || OBSERVATION_UNAVAILABLE.has(probe.reason)),
    'unavailable observation has an invalid startup reason or shutdown proof');
    if (NO_PROBE_WINDOW.has(probe.reason)) {
      const beforeFileRead = probe.reason === 'windows-x64-host-required';
      requireThat((beforeFileRead ? artifactUntouched : artifactMatched) &&
        OBSERVATION_PROOFS.every((key) => !probe[key]) && probe.gui_shutdown === 'not-started',
      'host-unavailable readback cannot contain a window, renderer or GUI launch');
    } else {
      requireThat(artifactMatched && probe.private_supervisor_job_verified &&
        (CLEAN_SHUTDOWN.has(probe.gui_shutdown) || probe.gui_shutdown === 'already-exited') &&
        (probe.reason !== 'gui-window-not-observed' || !probe.visible_clearra_window) &&
        (probe.reason !== 'isolated-webview2-renderer-not-observed' ||
          !probe.isolated_webview2_renderer_owned) &&
        (probe.reason !== 'gui-process-not-job-owned' ||
          (!probe.gui_process_owned && !probe.visible_clearra_window && !probe.isolated_webview2_renderer_owned)),
      'unavailable owned-process observation is inconsistent');
    }
  } else {
    requireThat(PROBE_FAILURES.has(probe.reason) &&
      (artifactMatched || (artifactUntouched && OBSERVATION_PROOFS.every((key) => !probe[key]) &&
        probe.gui_shutdown === 'not-started')),
      'failed observation has an unbound reason or changed executable bytes');
  }
  return { outcome, probe, stdoutSha256: sha256(Buffer.from(body, 'utf8')), exitCode };
}

// No filesystem reads, downloads, process launches, or production assertions.
// Trusted origin and supervisor receipts must be collected independently by the caller.
export function sealAcceptedWindowsDesktopStartupAttestation({
  acceptedEvidence, trustedActionsOrigin, executableBytes, startupOutput, supervisorReceipt, invocation,
}) {
  const artifact = validateAcceptedWindowsDesktopStartupInput({ acceptedEvidence, trustedActionsOrigin, executableBytes });
  const { outcome, probe, stdoutSha256, exitCode } = checkProbe(
    startupOutput, supervisorReceipt, trustedActionsOrigin, artifact, invocation,
  );
  return sealCanonicalReport({
    schema_id: WINDOWS_DESKTOP_STARTUP_ATTESTATION_SCHEMA,
    authority: 'non-publishing-startup-observation',
    artifact_scope: 'accepted',
    source_commit: trustedActionsOrigin.source_commit,
    release_version: trustedActionsOrigin.release_version,
    origin: Object.freeze({ ...trustedActionsOrigin }),
    artifact,
    supervisor: Object.freeze({
      run_id: outcome.run_id, producer: PRODUCER, profile: PROFILE, timeout_seconds: MANAGER_TIMEOUT,
      child_exit_code: exitCode, reason: outcome.reason, process_tree_stopped: true,
      canonical_receipt_sha256: canonicalSha256(supervisorReceipt),
      observed_stdout_sha256: stdoutSha256,
    }),
    observation: Object.freeze({
      status: probe.status,
      reason: probe.reason,
      gui_shutdown: probe.gui_shutdown,
      artifact_sha256_before: probe.artifact_sha256_before,
      artifact_sha256_after: probe.artifact_sha256_after,
      ...Object.fromEntries(OBSERVATION_PROOFS.map((key) => [key, probe[key]])),
    }),
    ui_route_verified: false,
    release_authority: false,
  });
}
