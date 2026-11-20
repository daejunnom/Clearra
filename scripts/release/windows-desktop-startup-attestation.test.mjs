import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import {
  CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA,
  CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES,
} from './canonical-acceptance-evidence.mjs';
import { canonicalSha256, sealCanonicalReport, verifyCanonicalReportHash } from './canonical-release-evidence.mjs';
import {
  sealAcceptedWindowsDesktopStartupAttestation,
  WINDOWS_DESKTOP_STARTUP_ATTESTATION_SCHEMA,
} from './windows-desktop-startup-attestation.mjs';

const S = 'a'.repeat(40);
const DIGEST = 'b'.repeat(64);
const VERSION = '0.9.0';
const REPOSITORY = 'daejunnom/Clearra';
const WORKFLOW = '.github/workflows/release-cli.yml';
const WORK_ROOT = 'D:/a/Clearra/Clearra';
const RUN = '23000101000';
const ATTEMPT = '2';
const MANAGER_RUN = '1795123412345678912-2310';
const NAME = `Clearra-GUI-v${VERSION}-windows-x86_64.exe`;
const PROBE = 'scripts/tools/verify-windows-desktop-runtime.ps1';
const ARTIFACT_PATH = `${WORK_ROOT}/_local/artifacts/windows-desktop-runtime/accepted/${NAME}`;
const BYTES = Buffer.from('MZ fixture bytes, not an executable', 'utf8');
const EXE_SHA = createHash('sha256').update(BYTES).digest('hex');
const PROOF_FLAGS = [
  'private_supervisor_job_verified', 'gui_process_owned', 'visible_clearra_window',
  'isolated_webview2_renderer_owned', 'owned_webview_tree_drained',
];

function fixture() {
  const jobs = CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.map((name, index) => ({
    name, job_id: String(9900 + index), status: 'passed',
  }));
  const artifacts = [
    { role: 'linux-cli', name: `Clearra-CLI-v${VERSION}-linux-x86_64`, sha256: DIGEST, size_bytes: 3, source_commit: S },
    { role: 'windows-cli', name: `Clearra-CLI-v${VERSION}-windows-x86_64.exe`, sha256: DIGEST, size_bytes: 8, source_commit: S },
    { role: 'windows-gui', name: NAME, sha256: EXE_SHA, size_bytes: BYTES.length, source_commit: S },
  ];
  const acceptedEvidence = sealCanonicalReport({
    schema_id: CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA, repository: REPOSITORY,
    release_version: VERSION, pages_base_path: '/Clearra', source_commit: S,
    run_id: RUN, run_attempt: ATTEMPT, workflow_path: WORKFLOW, status: 'passed', jobs,
    accepted_inputs: {
      ctk3_manifest_sha256: DIGEST, pages_identity_sha256: DIGEST,
      wasm_build_receipt_sha256: DIGEST, gate_index_sha256: DIGEST,
    },
    final_source_fragments: {
      toolchains: { source_commit: S },
      canonical_gate: { source_commit: S, status: 'passed', readiness_open_count: 0 },
      surface_reports: ['desktop', 'discord', 'native', 'wasm'].map((surface) => ({
        surface, source_commit: S, status: 'passed', sha256: DIGEST,
      })),
      release_artifacts: artifacts,
    },
  });
  const trustedActionsOrigin = {
    repository: REPOSITORY, workflow_path: WORKFLOW, source_commit: S,
    run_id: RUN, run_attempt: ATTEMPT, producer_job_id: jobs.find(({ name }) => name === 'windows-gui').job_id,
    artifact_id: '88299701',
    artifact_name: `clearra-windows-gui-v${VERSION}-run-${RUN}-attempt-${ATTEMPT}`,
    artifact_archive_sha256: DIGEST, canonical_acceptance_sha256: acceptedEvidence.report_sha256,
    release_version: VERSION, pages_base_path: '/Clearra',
  };
  const report = {
    schema: 'clearra.windows-desktop-startup-readback.v1', source_commit: S, artifact_version: VERSION,
    expected_artifact_sha256: EXE_SHA, artifact_sha256_before: EXE_SHA, artifact_sha256_after: EXE_SHA,
    artifact_bytes: BYTES.length, runtime_run_id: MANAGER_RUN, status: 'verified', reason: 'verified',
    gui_shutdown: 'graceful', ui_route_verified: false, release_authority: false,
    ...Object.fromEntries(PROOF_FLAGS.map((key) => [key, true])),
  };
  const startupOutput = JSON.stringify(report) + '\r\n';
  const supervisorReceipt = {
    schema_id: 'clearra.runtime-receipt.v2', stdout_tail: startupOutput, stderr_tail: '',
    outcome: {
      run_id: MANAGER_RUN, producer: 'windows-desktop-startup', profile: 'local-service',
      command: ['pwsh', '-NoProfile', '-NonInteractive', '-File', PROBE,
        '-Executable', ARTIFACT_PATH, '-Version', VERSION, '-SourceCommit', S, '-ExpectedSha256', EXE_SHA],
      return_code: 0, reason: 'normal', error_code: null, child_termination: { kind: 'exit', code: 0 },
      supervisor_termination_reason: null, duration_ms: 110, timeout_seconds: 75,
      descendant_processes_at_exit: 0, process_tree_stopped: true, automatic_retry: false,
    },
  };
  return {
    acceptedEvidence, trustedActionsOrigin, supervisorReceipt, startupOutput, executableBytes: Uint8Array.from(BYTES),
    invocation: { repository_root: WORK_ROOT, executable_path: ARTIFACT_PATH, powershell: 'pwsh', probe_timeout_seconds: 40 },
  };
}

function setProbe(input, update, exitCode = 0) {
  const probe = { ...JSON.parse(input.startupOutput), ...update };
  input.startupOutput = JSON.stringify(probe) + '\r\n';
  input.supervisorReceipt.stdout_tail = input.startupOutput;
  Object.assign(input.supervisorReceipt.outcome, {
    return_code: exitCode, child_termination: { kind: 'exit', code: exitCode },
    reason: exitCode === 0 ? 'normal' : 'nonzero',
  });
}

function resignAcceptance(input, update) {
  const { report_sha256: unused, ...unsealed } = input.acceptedEvidence;
  const edited = structuredClone(unsealed);
  update(edited);
  input.acceptedEvidence = sealCanonicalReport(edited);
  input.trustedActionsOrigin.canonical_acceptance_sha256 = input.acceptedEvidence.report_sha256;
}

function setHeadless(input, reason) {
  setProbe(input, {
    status: 'unavailable', reason, gui_shutdown: 'not-started',
    ...Object.fromEntries(PROOF_FLAGS.map((key) => [key, false])),
  }, 3);
}

test('accepted side uses exact Windows GUI packaging identity and seals a narrow startup proof', () => {
  const input = fixture();
  const result = sealAcceptedWindowsDesktopStartupAttestation(input);
  assert.equal(result.schema_id, WINDOWS_DESKTOP_STARTUP_ATTESTATION_SCHEMA);
  assert.equal(result.artifact_scope, 'accepted');
  assert.equal(result.observation.status, 'verified');
  assert.equal(result.artifact.sha256, EXE_SHA);
  assert.equal(result.origin.artifact_archive_sha256, DIGEST);
  assert.equal(result.origin.canonical_acceptance_sha256, input.acceptedEvidence.report_sha256);
  assert.equal(result.supervisor.canonical_receipt_sha256, canonicalSha256(input.supervisorReceipt));
  assert.equal(result.supervisor.child_exit_code, 0);
  assert.equal(result.ui_route_verified, false);
  assert.equal(result.release_authority, false);
  assert.equal(verifyCanonicalReportHash(result), result.report_sha256);
  assert.ok(!JSON.stringify(result).includes(WORK_ROOT));
});

test('accepted source, run, attempt, exact host job and canonical seal cannot be swapped', () => {
  for (const [field, value] of [
    ['source_commit', 'c'.repeat(40)], ['run_id', '23000101100'], ['run_attempt', '3'],
    ['producer_job_id', '1'], ['canonical_acceptance_sha256', '1'.repeat(64)],
    ['workflow_path', '.github/workflows/other.yml'], ['artifact_name', 'pre-main-candidate'],
  ]) {
    const input = fixture();
    input.trustedActionsOrigin[field] = value;
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, field);
  }
});

test('authority accepts no missing archive digest or structurally invalid IDs', () => {
  for (const [field, value] of [
    ['artifact_archive_sha256', null], ['artifact_id', '0'], ['run_id', 23000101000],
  ]) {
    const input = fixture();
    input.trustedActionsOrigin[field] = value;
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, field);
  }
});

test('canonical envelope cannot substitute or duplicate Windows GUI role/name', () => {
  for (const mutate of [
    (a) => { a[2].name = 'Clearra-clone.exe'; },
    (a) => { a[1] = { ...a[2] }; },
    (a) => { a[2].size_bytes += 1; },
  ]) {
    const input = fixture();
    resignAcceptance(input, (e) => { mutate(e.final_source_fragments.release_artifacts); });
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input));
  }
});

test('identically sized alternate executable bytes do not match canonical SHA', () => {
  const input = fixture();
  input.executableBytes[5] ^= 1;
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input));
});

test('supervisor must invoke the approved launcher, script, executable and source without extra flags', () => {
  for (const [slot, value] of [
    [0, 'powershell'], [3, '-Command'], [4, 'D:/test/verify-windows-desktop-runtime.ps1'],
    [6, ARTIFACT_PATH.replace('/accepted/', '/elsewhere/')], [8, '0.9.1'],
    [10, 'f'.repeat(40)], [12, DIGEST],
  ]) {
    const input = fixture();
    input.supervisorReceipt.outcome.command[slot] = value;
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, `command slot ${slot}`);
  }
  const input = fixture();
  input.supervisorReceipt.outcome.command.push('-Verbose');
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input));
});

test('explicit timeout and absolute script paths are allowed only for the approved invocation', () => {
  const input = fixture();
  input.invocation.probe_timeout_seconds = 39;
  input.supervisorReceipt.outcome.command[4] = `${WORK_ROOT}/${PROBE}`;
  input.supervisorReceipt.outcome.command.push('-TimeoutSeconds', '39');
  assert.equal(sealAcceptedWindowsDesktopStartupAttestation(input).observation.status, 'verified');
  input.supervisorReceipt.outcome.command[14] = '41';
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input));
});

test('managed stage does not allow same-name outside, prefix, stream, or traversal artifacts', () => {
  for (const path of [
    `D:/loose/${NAME}`, `${WORK_ROOT}/build-loose/${NAME}`,
    `${WORK_ROOT}/build/alias/../${NAME}`, `${ARTIFACT_PATH}:alternate`,
  ]) {
    const input = fixture();
    input.invocation.executable_path = path;
    input.supervisorReceipt.outcome.command[6] = path;
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, path);
  }
});

test('same JSON observation must appear in supervisor stdout; no extra lines, keys or duplicates', () => {
  const detached = fixture();
  detached.startupOutput = detached.startupOutput.replace('graceful', 'owned-root-forced');
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(detached));
  for (const mutate of [
    (line) => '{"status":"unavailable",' + line.slice(1),
    (line) => line + 'other evidence\r\n',
    (line) => line.replace('"gui_shutdown"', '"unexpected":false,"gui_shutdown"'),
  ]) {
    const input = fixture();
    input.startupOutput = mutate(input.startupOutput);
    input.supervisorReceipt.stdout_tail = input.startupOutput;
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input));
  }
});

test('forged source, before/after digest, and incomplete verified ownership all fail closed', () => {
  for (const update of [
    { source_commit: '1'.repeat(40) }, { artifact_version: '0.8.4' }, { expected_artifact_sha256: DIGEST },
    { artifact_sha256_after: DIGEST }, { artifact_bytes: 1 }, { visible_clearra_window: false },
    { isolated_webview2_renderer_owned: false }, { private_supervisor_job_verified: false },
    { gui_process_owned: false }, { owned_webview_tree_drained: false }, { gui_shutdown: 'already-exited' },
    { ui_route_verified: true }, { release_authority: true }, { visible_clearra_window: 'true' },
  ]) {
    const input = fixture();
    setProbe(input, update);
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, JSON.stringify(update));
  }
});

test('timeout, inherited/unavailable Job facts, replacement run and automatic retry are not proof', () => {
  for (const update of [
    { supervisor_termination_reason: 'timeout', error_code: 'E_CLEARRA_TIMEOUT' },
    { process_tree_stopped: false }, { descendant_processes_at_exit: null },
    { descendant_processes_at_exit: 1 }, { run_id: MANAGER_RUN + '1' }, { profile: 'build-test' },
    { producer: 'other' }, { automatic_retry: true }, { timeout_seconds: 3600 },
    { child_termination: { kind: 'unavailable' } },
  ]) {
    const input = fixture();
    Object.assign(input.supervisorReceipt.outcome, update);
    assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(input), undefined, JSON.stringify(update));
  }
});

test('headless or unavailable host is sealed as unavailable, never GUI startup success', () => {
  for (const reason of [
    'interactive-window-station-unavailable', 'private-supervisor-job-unverified',
    'native-startup-probe-unavailable', 'windows-x64-host-required',
  ]) {
    const input = fixture();
    setHeadless(input, reason);
    if (reason === 'windows-x64-host-required') {
      setProbe(input, { artifact_sha256_before: null, artifact_sha256_after: null, artifact_bytes: null }, 3);
    }
    const result = sealAcceptedWindowsDesktopStartupAttestation(input);
    assert.equal(result.observation.status, 'unavailable');
    assert.equal(result.observation.reason, reason);
    assert.equal(result.observation.visible_clearra_window, false);
    assert.equal(result.supervisor.child_exit_code, 3);
    assert.equal(result.release_authority, false);
  }
  const partial = fixture();
  setProbe(partial, {
    status: 'unavailable', reason: 'isolated-webview2-renderer-not-observed',
    isolated_webview2_renderer_owned: false, owned_webview_tree_drained: false,
  }, 3);
  assert.equal(sealAcceptedWindowsDesktopStartupAttestation(partial).observation.status, 'unavailable');
});

test('a green supervisor or forged HWND cannot upgrade an unavailable probe; typed failure is preserved', () => {
  const wrongExit = fixture();
  setHeadless(wrongExit, 'interactive-window-station-unavailable');
  setProbe(wrongExit, {});
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(wrongExit));
  const wrongFlags = fixture();
  setHeadless(wrongFlags, 'private-supervisor-job-unverified');
  setProbe(wrongFlags, { visible_clearra_window: true }, 3);
  assert.throws(() => sealAcceptedWindowsDesktopStartupAttestation(wrongFlags));
  const failure = fixture();
  setProbe(failure, {
    status: 'failed', reason: 'gui-exited-before-readback', gui_shutdown: 'already-exited',
    private_supervisor_job_verified: true,
    gui_process_owned: false, visible_clearra_window: false,
    isolated_webview2_renderer_owned: false, owned_webview_tree_drained: false,
  }, 2);
  const result = sealAcceptedWindowsDesktopStartupAttestation(failure);
  assert.equal(result.observation.status, 'failed');
  assert.equal(result.supervisor.child_exit_code, 2);
});
