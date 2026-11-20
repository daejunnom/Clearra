// Pure stage tests: no authenticated requests, files, manager or GUI.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { crc32, deflateRawSync } from 'node:zlib';
import {
  CANONICAL_ACCEPTANCE_EVIDENCE_FILE,
  CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA,
  CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES,
} from './canonical-acceptance-evidence.mjs';
import { canonicalJson, sealCanonicalReport } from './canonical-release-evidence.mjs';
import {
  parseWindowsDesktopStageArgs, stageWindowsDesktopStartupReadback,
} from './stage-windows-desktop-startup-readback.mjs';
import { TRUSTED_ORIGIN_FILE } from './run-windows-desktop-startup-readback.mjs';

const REPO = 'daejunnom/Clearra';
const SOURCE = '6'.repeat(40);
const RUN = '23812886621';
const ATTEMPT = '2';
const VERSION = '0.9.0';
const WORKFLOW = '.github/workflows/release-cli.yml';
const STEM = `/repos/${REPO}/actions`;
const ATTEMPT_PATH = `${STEM}/runs/${RUN}/attempts/${ATTEMPT}`;
const EXE = `Clearra-GUI-v${VERSION}-windows-x86_64.exe`;
const BYTES = Buffer.from('MZ fixture bytes, not a runnable Windows image', 'utf8');
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const SHA = digest(BYTES);
const BASE = '/Clearra';
const CLI = ['--source-commit', SOURCE, '--run-id', RUN, '--run-attempt', ATTEMPT, '--version', VERSION];
const DEST = `accepted-${RUN}-${ATTEMPT}`;
const GUI_ID = '776721';
const CANONICAL_ID = '776722';

function zip(name, payload, { method = 0, flags = 0, attributes = 0, declaredSize = payload.length } = {}) {
  const nameBytes = Buffer.from(name);
  const content = method === 8 ? deflateRawSync(payload) : payload;
  const crc = crc32(payload);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(flags, 6);
  local.writeUInt16LE(method, 8);
  local.writeUInt16LE(nameBytes.length, 26);
  const trailer = Buffer.alloc(flags & 8 ? 16 : 0);
  if (flags & 8) {
    trailer.writeUInt32LE(0x08074b50, 0);
    trailer.writeUInt32LE(crc, 4);
    trailer.writeUInt32LE(content.length, 8);
    trailer.writeUInt32LE(declaredSize, 12);
  } else {
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(content.length, 18);
    local.writeUInt32LE(declaredSize, 22);
  }
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(flags, 8);
  central.writeUInt16LE(method, 10);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(content.length, 20);
  central.writeUInt32LE(declaredSize, 24);
  central.writeUInt16LE(nameBytes.length, 28);
  central.writeUInt32LE(attributes, 38);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(central.length + nameBytes.length, 12);
  end.writeUInt32LE(local.length + nameBytes.length + content.length + trailer.length, 16);
  return Buffer.concat([local, nameBytes, content, trailer, central, nameBytes, end]);
}

function fixture() {
  const trace = [];
  const repository = { id: 34202211, full_name: REPO };
  const attempt = {
    event: 'workflow_dispatch', repository, head_repository: { ...repository }, head_branch: 'main',
    head_sha: SOURCE, id: Number(RUN), run_attempt: Number(ATTEMPT), path: WORKFLOW,
    status: 'completed', conclusion: 'success',
  };
  const jobs = [...CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES, 'canonical-evidence'].map((name, index) => ({
    name, id: 112233446 + index, head_sha: SOURCE, run_id: Number(RUN), run_attempt: Number(ATTEMPT),
    status: 'completed', conclusion: 'success',
    steps: (name === 'windows-gui' ? ['Build standalone SvelteKit and Tauri GUI', 'Upload Windows GUI artifact'] :
      name === 'canonical-evidence' ? ['Create canonical acceptance evidence', 'Upload canonical acceptance evidence'] : [])
      .map((step) => ({ name: step, status: 'completed', conclusion: 'success' })),
  }));
  const evidence = sealCanonicalReport({
    schema_id: CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA, repository: REPO, workflow_path: WORKFLOW, pages_base_path: BASE,
    source_commit: SOURCE, run_id: RUN, run_attempt: ATTEMPT, release_version: VERSION, status: 'passed',
    jobs: jobs.slice(0, CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.length)
      .map(({ id, name }) => ({ job_id: String(id), name, status: 'passed' })),
    accepted_inputs: Object.fromEntries([
      'ctk3_manifest_sha256', 'pages_identity_sha256', 'wasm_build_receipt_sha256', 'gate_index_sha256',
    ].map((key) => [key, SHA])),
    final_source_fragments: {
      toolchains: { source_commit: SOURCE },
      canonical_gate: { source_commit: SOURCE, status: 'passed', readiness_open_count: 0 },
      surface_reports: ['desktop', 'discord', 'native', 'wasm'].map((surface) => ({
        surface, source_commit: SOURCE, status: 'passed', sha256: SHA,
      })),
      release_artifacts: [
        ['linux-cli', `Clearra-CLI-v${VERSION}-linux-x86_64`],
        ['windows-cli', `Clearra-CLI-v${VERSION}-windows-x86_64.exe`],
        ['windows-gui', EXE],
      ].map(([role, name]) => ({ role, name, sha256: SHA, size_bytes: BYTES.length, source_commit: SOURCE })),
    },
  });
  const canonicalBytes = Buffer.from(canonicalJson(evidence) + '\n');
  const data = {
    occupied: false, authenticated: { id: 581, login: 'operator-fixture' }, committed: null, paginated: false,
    guiZip: zip(EXE, BYTES, { method: 8, flags: 0x808 }),
    canonicalZip: zip(CANONICAL_ACCEPTANCE_EVIDENCE_FILE, canonicalBytes),
  };
  const artifacts = [
    { id: GUI_ID, name: `clearra-windows-gui-v${VERSION}-run-${RUN}-attempt-${ATTEMPT}`, digest: 'sha256:' + digest(data.guiZip) },
    { id: CANONICAL_ID, name: `canonical-acceptance-evidence-${SOURCE}-run-${RUN}-attempt-${ATTEMPT}`, digest: 'sha256:' + digest(data.canonicalZip) },
  ].map((entry) => ({ ...entry, expired: false, workflow_run: {
    id: Number(RUN), repository_id: repository.id, head_repository_id: repository.id, head_branch: 'main', head_sha: SOURCE,
  } }));
  const io = {
    platform: 'win32', architecture: 'x64',
    assertDestination(name, profile) {
      trace.push('destination');
      assert.equal(name, DEST);
      assert.equal(profile, 'release');
      if (data.occupied) throw new Error('stage exists');
    },
    async getJson(path) {
      trace.push(path);
      if (path === '/user') return data.authenticated;
      if (path === ATTEMPT_PATH) return attempt;
      if (path === ATTEMPT_PATH + '/jobs?per_page=100&page=1') return { total_count: jobs.length, jobs };
      if (path === `${STEM}/runs/${RUN}/artifacts?per_page=100&page=1`) {
        if (!data.paginated) return { total_count: artifacts.length, artifacts };
        return { total_count: 102, artifacts: Array.from({ length: 100 }, (_, i) => ({ id: String(100 + i) })) };
      }
      if (data.paginated && path === `${STEM}/runs/${RUN}/artifacts?per_page=100&page=2`) {
        return { total_count: 102, artifacts };
      }
      throw new Error('test did not authorize this path');
    },
    async getArchive(path, limit) {
      trace.push(path);
      if (path === `${STEM}/artifacts/${GUI_ID}/zip`) {
        assert.ok(limit <= 128 * 1024 * 1024);
        return data.guiZip;
      }
      assert.equal(path, `${STEM}/artifacts/${CANONICAL_ID}/zip`);
      assert.ok(limit <= 2 * 1024 * 1024);
      return data.canonicalZip;
    },
    commitDestination(name, profile, files) {
      trace.push('commit');
      assert.equal(name, DEST);
      assert.equal(profile, 'release');
      data.committed = files;
      return `D:/repo/_local/artifacts/windows-desktop-runtime/${name}`;
    },
  };
  return { io, trace, data, attempt, jobs, artifacts, evidence, canonicalBytes,
    run: () => stageWindowsDesktopStartupReadback(parseWindowsDesktopStageArgs(CLI), io) };
}

test('strict staging CLI cannot override repository, download endpoint or managed output', () => {
  assert.deepEqual(parseWindowsDesktopStageArgs(['--help']), { help: true });
  for (const args of [
    [...CLI, '--repository', 'other/project'], [...CLI, '--url', 'https://other.example'],
    [...CLI, '--output', 'C:/unmanaged'], [...CLI, '--run-id', RUN],
    [...CLI.slice(0, -1), 'bad/../../path'], ['--help', ...CLI],
  ]) assert.throws(() => parseWindowsDesktopStageArgs(args));
});

test('one explicit stage verifies authenticated attempt, both raw ZIPs and three committed leaves only', async () => {
  const f = fixture();
  f.data.paginated = true;
  const result = await f.run();
  assert.equal(result.status, 'staged');
  assert.equal(result.startup_verified, false);
  assert.equal(result.release_authority, false);
  assert.equal(result.artifact_sha256, SHA);
  assert.deepEqual(f.trace, [
    'destination', '/user', ATTEMPT_PATH, ATTEMPT_PATH + '/jobs?per_page=100&page=1',
    `${STEM}/runs/${RUN}/artifacts?per_page=100&page=1`, `${STEM}/runs/${RUN}/artifacts?per_page=100&page=2`,
    `${STEM}/artifacts/${CANONICAL_ID}/zip`, `${STEM}/artifacts/${GUI_ID}/zip`, 'commit',
  ]);
  assert.deepEqual([...f.data.committed.keys()], [CANONICAL_ACCEPTANCE_EVIDENCE_FILE, EXE, TRUSTED_ORIGIN_FILE]);
  assert.deepEqual(f.data.committed.get(EXE), BYTES);
  assert.deepEqual(f.data.committed.get(CANONICAL_ACCEPTANCE_EVIDENCE_FILE), f.canonicalBytes);
  const origin = JSON.parse(f.data.committed.get(TRUSTED_ORIGIN_FILE).toString('utf8'));
  assert.equal(origin.artifact_archive_sha256, digest(f.data.guiZip));
  assert.equal(origin.canonical_acceptance_sha256, f.evidence.report_sha256);
});

test('managed stage collision is rejected before authentication or acquisition', async () => {
  const f = fixture();
  f.data.occupied = true;
  await assert.rejects(f.run());
  assert.deepEqual(f.trace, ['destination']);
  assert.equal(f.data.committed, null);
});

test('missing approved authentication, failed attempt or wrong attempt jobs never reach ZIP acquisition', async () => {
  for (const change of [
    (f) => { f.data.authenticated = { message: 'not authenticated' }; },
    (f) => { f.attempt.conclusion = 'failure'; },
    (f) => { f.jobs[0].run_attempt--; },
    (f) => { f.artifacts[0].expired = true; },
  ]) {
    const f = fixture();
    change(f);
    await assert.rejects(f.run());
    assert.equal(f.data.committed, null);
    assert.ok(!f.trace.some((path) => path.endsWith('/zip')));
  }
});

test('wrong ZIP digest precedes member parsing and wrong canonical JSON blocks all GUI bytes', async () => {
  for (const changedCatalog of [false, true]) {
    const f = fixture();
    f.data.canonicalZip = zip(CANONICAL_ACCEPTANCE_EVIDENCE_FILE, Buffer.from('{}'));
    if (changedCatalog) f.artifacts[1].digest = 'sha256:' + digest(f.data.canonicalZip);
    await assert.rejects(f.run());
    assert.ok(!f.trace.includes(`${STEM}/artifacts/${GUI_ID}/zip`));
    assert.equal(f.data.committed, null);
  }
});

test('even authenticated ZIPs cannot stage paths, special files, unsupported compression or excess output', async () => {
  for (const newZip of [
    zip('../' + EXE, BYTES), zip(EXE, BYTES, { attributes: (0o120000 << 16) >>> 0 }),
    zip(EXE, BYTES, { attributes: 0x400 }), zip(EXE, BYTES, { flags: 1 }), zip(EXE, BYTES, { method: 12 }),
    zip(EXE, Buffer.alloc(4096, 'Z'), { method: 8, declaredSize: BYTES.length }),
    zip(EXE, BYTES, { method: 8, declaredSize: BYTES.length + 10 }),
    Buffer.concat([zip(EXE, BYTES), Buffer.from('trailing')]),
  ]) {
    const f = fixture();
    f.data.guiZip = newZip;
    f.artifacts[0].digest = 'sha256:' + digest(newZip);
    await assert.rejects(f.run());
    assert.equal(f.data.committed, null);
  }
});

test('valid alternate GUI ZIP bytes still cannot substitute the canonical executable', async () => {
  const f = fixture();
  const changed = Buffer.from(BYTES);
  changed[4] ^= 1;
  f.data.guiZip = zip(EXE, changed);
  f.artifacts[0].digest = 'sha256:' + digest(f.data.guiZip);
  await assert.rejects(f.run());
  assert.equal(f.data.committed, null);
});
