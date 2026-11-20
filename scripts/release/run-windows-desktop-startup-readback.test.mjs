// Pure fake-manager tests: no process, managed directory, or network is used.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { win32 } from 'node:path';
import test from 'node:test';
import {
  CANONICAL_ACCEPTANCE_EVIDENCE_FILE,
  CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA,
  CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES,
} from './canonical-acceptance-evidence.mjs';
import { sealCanonicalReport, verifyCanonicalReportHash } from './canonical-release-evidence.mjs';
import {
  TRUSTED_ORIGIN_FILE, parseWindowsDesktopStartupReadbackArgs, runWindowsDesktopStartupReadback,
} from './run-windows-desktop-startup-readback.mjs';

const SOURCE = 'd'.repeat(40);
const VERSION = '0.9.0';
const DIGEST = '3'.repeat(64);
const RUN = '23135124222';
const ATTEMPT = '2';
const NATIVE_RUN = '1795212345678901234-42';
const ROOT = 'D:\\repo\\Clearra';
const STAGE = win32.join(ROOT, '_local', 'artifacts', 'windows-desktop-runtime', `accepted-${RUN}-${ATTEMPT}`);
const GUI = `Clearra-GUI-v${VERSION}-windows-x86_64.exe`;
const BYTES = Buffer.from('MZ dummy Windows image (fixture only)');
const GUI_SHA = createHash('sha256').update(BYTES).digest('hex');
const WORKFLOW = '.github/workflows/release-cli.yml';
const REPO = 'daejunnom/Clearra';
const BASE = '/Clearra';
const SHELL = 'C:\\Program Files\\PowerShell\\7\\pwsh.exe';
const argv = (stage = STAGE, source = SOURCE) => ['--source-commit', source, '--evidence-dir', stage];

function inputs() {
  const jobs = CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.map((name, i) => ({
    name, job_id: String(1330 + i), status: 'passed',
  }));
  const acceptedEvidence = sealCanonicalReport({
    schema_id: CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA,
    repository: REPO, workflow_path: WORKFLOW, pages_base_path: BASE,
    run_id: RUN, run_attempt: ATTEMPT, release_version: VERSION, source_commit: SOURCE,
    status: 'passed', jobs,
    accepted_inputs: Object.fromEntries([
      'ctk3_manifest_sha256', 'pages_identity_sha256', 'wasm_build_receipt_sha256', 'gate_index_sha256',
    ].map((name) => [name, DIGEST])),
    final_source_fragments: {
      toolchains: { source_commit: SOURCE },
      canonical_gate: { source_commit: SOURCE, status: 'passed', readiness_open_count: 0 },
      surface_reports: ['desktop', 'discord', 'native', 'wasm'].map((surface) => ({
        surface, source_commit: SOURCE, status: 'passed', sha256: DIGEST,
      })),
      release_artifacts: [
        ['linux-cli', `Clearra-CLI-v${VERSION}-linux-x86_64`, 1, DIGEST],
        ['windows-cli', `Clearra-CLI-v${VERSION}-windows-x86_64.exe`, 1, DIGEST],
        ['windows-gui', GUI, BYTES.length, GUI_SHA],
      ].map(([role, name, size_bytes, sha256]) => ({ role, name, size_bytes, sha256, source_commit: SOURCE })),
    },
  });
  const origin = {
    repository: REPO, workflow_path: WORKFLOW, pages_base_path: BASE, run_id: RUN, run_attempt: ATTEMPT,
    release_version: VERSION, source_commit: SOURCE, artifact_name: `clearra-windows-gui-v${VERSION}-run-${RUN}-attempt-${ATTEMPT}`,
    producer_job_id: jobs.find(({ name }) => name === 'windows-gui').job_id,
    artifact_id: '66144591', artifact_archive_sha256: DIGEST, canonical_acceptance_sha256: acceptedEvidence.report_sha256,
  };
  return { acceptedEvidence, origin };
}

function fixture() {
  const { acceptedEvidence, origin } = inputs();
  const trace = [];
  const verifiedPaths = new Set();
  const ownedFiles = new Set();
  const setup = {
    admission: false, unavailable: false, crash: false, staleCheckout: false,
    brokenReceipt: false, mutatedAfter: false, extraReceiptOnContention: false,
    denied: null, receipt: null, seenEnv: null, seenArgs: null, wrongCheckoutAt: null,
  };
  const io = {
    repositoryRoot: ROOT, architecture: 'x64', platform: 'win32',
    environment: { Path: 'C:\\trusted', LOCALAPPDATA: 'C:\\runtime', GITHUB_TOKEN: 'do-not-inherit' },
    verifyCheckout(root, sha) {
      trace.push('checkout');
      assert.equal(root, ROOT);
      if (setup.staleCheckout || setup.wrongCheckoutAt === trace.filter((x) => x === 'checkout').length) {
        throw new Error('wrong checkout');
      }
      assert.equal(sha, SOURCE);
    },
    requireLocalFile(path, allowHardlink = false) {
      trace.push('owned:' + win32.basename(path));
      if (win32.basename(path) === 'clearra-manage.exe') assert.equal(allowHardlink, true);
      else assert.equal(allowHardlink, false);
      ownedFiles.add(path);
    },
    requireDirectory(path) {
      trace.push('directory');
      assert.equal(path, STAGE);
      assert.ok(verifiedPaths.has(path));
    },
    verifyManagedPath(manager, root, path, rootId) {
      assert.equal(root, ROOT);
      assert.equal(win32.basename(manager), 'clearra-manage.exe');
      if (path === manager) assert.equal(rootId, 'build-publication');
      else assert.equal(rootId, 'local-artifacts');
      if (win32.basename(path) === setup.denied) throw new Error('not managed');
      trace.push('approved:' + win32.basename(path));
      verifiedPaths.add(path);
    },
    readBounded(path, limit) {
      const name = win32.basename(path);
      trace.push('read:' + name);
      assert.equal(win32.dirname(path), STAGE);
      assert.ok(verifiedPaths.has(path) && ownedFiles.has(path));
      if (name === TRUSTED_ORIGIN_FILE) return Buffer.from(JSON.stringify(origin));
      if (name === CANONICAL_ACCEPTANCE_EVIDENCE_FILE) return Buffer.from(JSON.stringify(acceptedEvidence));
      assert.equal(name, GUI);
      assert.equal(limit, BYTES.length);
      const bytes = Buffer.from(BYTES);
      if (setup.mutatedAfter && trace.includes('supervisor')) bytes[2] ^= 1;
      return bytes;
    },
    findPowerShell() { return SHELL; },
    async capture(manager, args, root, env) {
      trace.push('supervisor');
      setup.seenEnv = env;
      setup.seenArgs = args;
      assert.equal(root, ROOT);
      assert.equal(win32.basename(manager), 'clearra-manage.exe');
      if (setup.admission) {
        const marker = setup.extraReceiptOnContention ? 'clearra_runtime_receipt=fake\n' : '';
        return { code: 70, signal: null, overflow: false, stdout: '', stderr:
          marker + 'runtime: E_CLEARRA_RUNTIME_SLOT_BUSY: concurrency class local-service is occupied\n' };
      }
      if (setup.crash) return { code: null, signal: 'SIGTERM', overflow: false, stdout: '', stderr: '' };
      const code = setup.unavailable ? 3 : 0;
      const proof = !setup.unavailable;
      const stdout = JSON.stringify({
        schema: 'clearra.windows-desktop-startup-readback.v1',
        source_commit: SOURCE, artifact_version: VERSION, expected_artifact_sha256: GUI_SHA,
        artifact_sha256_before: GUI_SHA, artifact_sha256_after: GUI_SHA, artifact_bytes: BYTES.length,
        runtime_run_id: NATIVE_RUN, status: proof ? 'verified' : 'unavailable',
        reason: proof ? 'verified' : 'interactive-window-station-unavailable',
        gui_shutdown: proof ? 'graceful' : 'not-started', ui_route_verified: false, release_authority: false,
        private_supervisor_job_verified: proof, gui_process_owned: proof, visible_clearra_window: proof,
        isolated_webview2_renderer_owned: proof, owned_webview_tree_drained: proof,
      }) + '\r\n';
      setup.receipt = {
        schema_id: 'clearra.runtime-receipt.v2', stdout_tail: stdout, stderr_tail: '',
        outcome: {
          run_id: NATIVE_RUN, producer: 'windows-desktop-startup', profile: 'local-service', timeout_seconds: 75,
          duration_ms: 18, automatic_retry: false, supervisor_termination_reason: null, error_code: null,
          process_tree_stopped: true, descendant_processes_at_exit: 0, reason: code === 0 ? 'normal' : 'nonzero',
          return_code: setup.brokenReceipt ? 2 : code, child_termination: { kind: 'exit', code },
          command: args.slice(args.indexOf('--') + 1),
        },
      };
      return { code, signal: null, overflow: false, stdout, stderr: 'one emitted pointer' };
    },
    readReceipt(execution) {
      trace.push('receipt');
      assert.equal(execution.stderr, 'one emitted pointer');
      return setup.receipt;
    },
  };
  return { io, setup, trace, origin,
    run: (values = parseWindowsDesktopStartupReadbackArgs(argv())) => runWindowsDesktopStartupReadback(values, io) };
}

test('strict CLI forbids input/manager indirection and parses explicit managed stage', () => {
  assert.deepEqual(parseWindowsDesktopStartupReadbackArgs(['--help']), { help: true });
  assert.deepEqual(parseWindowsDesktopStartupReadbackArgs([...argv(), '--manager-profile', 'debug']), {
    sourceCommit: SOURCE, evidenceDirectory: STAGE, managerProfile: 'debug',
  });
  for (const args of [
    [...argv(), '--source-commit', SOURCE], ['--help', ...argv()], [...argv(), '--publish'],
    [...argv(), '--manager', 'D:/elsewhere/clearra-manage.exe'],
    argv('D:work\\relative'), argv(STAGE + '\\not\\..'), argv(STAGE + ':alternate'),
  ]) {
    assert.throws(() => parseWindowsDesktopStartupReadbackArgs(args));
  }
});

test('exact accepted GUI is proven before one supervisor call and read back again afterward', async () => {
  const f = fixture();
  const result = await f.run();
  assert.equal(result.exitCode, 0);
  assert.equal(result.report.observation.status, 'verified');
  assert.equal(result.report.release_authority, false);
  assert.equal(result.report.supervisor.run_id, NATIVE_RUN);
  assert.equal(verifyCanonicalReportHash(result.report), result.report.report_sha256);
  assert.deepEqual(f.setup.seenArgs.slice(2, 10),
    ['runtime', 'run', '--producer', 'windows-desktop-startup', '--profile', 'local-service', '--timeout', '75']);
  assert.equal(f.setup.seenArgs[11], SHELL);
  assert.equal(f.setup.seenEnv.CLEARRA_SOURCE_COMMIT, SOURCE);
  assert.equal(f.setup.seenEnv.CLEARRA_ENGINE_BUILD_ID, SOURCE);
  assert.equal(Object.hasOwn(f.setup.seenEnv, 'GITHUB_TOKEN'), false);
  assert.deepEqual(f.trace.filter((event) => event === 'supervisor' || event === 'read:' + GUI),
    ['read:' + GUI, 'supervisor', 'read:' + GUI]);
});

test('outside or not-manager-verified staged files are rejected before metadata reads', async () => {
  const outside = fixture();
  const status = await outside.run(parseWindowsDesktopStartupReadbackArgs(argv('D:/other/' + win32.basename(STAGE))));
  assert.equal(status.report.reason, 'input-not-in-named-managed-stage');
  assert.deepEqual(outside.trace, ['checkout']);
  for (const denied of [win32.basename(STAGE), TRUSTED_ORIGIN_FILE]) {
    const f = fixture();
    f.setup.denied = denied;
    assert.equal((await f.run()).exitCode, 2);
    assert.ok(!f.trace.some((event) => event.startsWith('read:')));
    assert.ok(!f.trace.includes('supervisor'));
  }
});

test('unbound origin or wrong executable bytes stop before supervisor launch', async () => {
  const wrongRun = fixture();
  wrongRun.origin.run_attempt = '9';
  assert.equal((await wrongRun.run()).report.reason, 'accepted-artifact-preflight-failed');
  assert.ok(!wrongRun.trace.includes('read:' + GUI));
  const f = fixture();
  const readStage = f.io.readBounded;
  f.io.readBounded = (path, limit) => {
    const bytes = readStage(path, limit);
    if (win32.basename(path) === GUI) bytes[2] ^= 1;
    return bytes;
  };
  assert.equal((await f.run()).exitCode, 2);
  assert.ok(!f.trace.includes('supervisor'));
});

test('a stale or dirty source both before and during execution cannot seal an accepted readback', async () => {
  const stale = fixture();
  stale.setup.staleCheckout = true;
  assert.equal((await stale.run()).report.reason, 'git-checkout-not-exact-clean-tracked-source');
  assert.deepEqual(stale.trace, ['checkout']);
  const changed = fixture();
  changed.setup.wrongCheckoutAt = 3;
  const result = await changed.run();
  assert.equal(result.report.reason, 'git-checkout-not-exact-clean-tracked-source');
  assert.equal(changed.trace.filter((event) => event === 'supervisor').length, 1);
  assert.equal(Object.hasOwn(result.report, 'observation'), false);
});

test('runtime contention stops once with no receipt, nested lease, retry, or launcher adoption', async () => {
  const f = fixture();
  f.setup.admission = true;
  const result = await f.run();
  assert.equal(result.exitCode, 4);
  assert.equal(result.report.status, 'admission-contended');
  assert.equal(f.trace.filter((event) => event === 'supervisor').length, 1);
  assert.ok(!f.trace.includes('receipt'));
  const nested = fixture();
  nested.io.environment.CLEARRA_RUNTIME_SUPERVISED = '1';
  assert.equal((await nested.run()).exitCode, 2);
  assert.deepEqual(nested.trace, []);
});

test('headless native probe and incompatible host remain unavailable, not verified', async () => {
  const f = fixture();
  f.setup.unavailable = true;
  const result = await f.run();
  assert.equal(result.exitCode, 3);
  assert.equal(result.report.observation.status, 'unavailable');
  assert.equal(result.report.observation.visible_clearra_window, false);
  const noWindows = fixture();
  noWindows.io.platform = 'linux';
  assert.equal((await noWindows.run()).exitCode, 3);
  assert.deepEqual(noWindows.trace, []);
});

test('lost supervisor, mismatched child exit, forged contention and post-run substitution fail closed', async () => {
  for (const options of [
    { crash: true }, { brokenReceipt: true }, { admission: true, extraReceiptOnContention: true }, { mutatedAfter: true },
  ]) {
    const f = fixture();
    Object.assign(f.setup, options);
    const result = await f.run();
    assert.equal(result.exitCode, 2);
    assert.equal(result.report.status, 'failed');
    assert.equal(Object.hasOwn(result.report, 'observation'), false);
    assert.equal(f.trace.filter((event) => event === 'supervisor').length, 1);
  }
});
