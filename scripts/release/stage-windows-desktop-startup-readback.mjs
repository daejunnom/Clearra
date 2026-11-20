// Opt-in local preparation only. No GUI, workflow dispatch, or publication.
import { execFile, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { resolve, win32 } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs, promisify } from 'node:util';
import { crc32, inflateRawSync } from 'node:zlib';
import { requireOwnedFile } from '../benchmark/run-placement-coverage-proof.mjs';
import {
  CANONICAL_ACCEPTANCE_EVIDENCE_FILE,
  validateCanonicalAcceptanceEvidence,
} from './canonical-acceptance-evidence.mjs';
import { canonicalJson } from './canonical-release-evidence.mjs';
import { TRUSTED_ORIGIN_FILE } from './run-windows-desktop-startup-readback.mjs';
import { validateAcceptedWindowsDesktopStartupInput } from './windows-desktop-startup-attestation.mjs';
import {
  buildWindowsDesktopTrustedOrigin,
  selectWindowsDesktopTrustedOriginArtifacts,
} from './windows-desktop-trusted-origin.mjs';

const REPOSITORY = 'daejunnom/Clearra';
const HOST = 'github.com';
const STEM = `/repos/${REPOSITORY}/actions`;
const ROOT_PARTS = ['_local', 'artifacts', 'windows-desktop-runtime'];
const METADATA_LIMIT = 8 * 1024 * 1024;
const CANONICAL_ZIP_LIMIT = 2 * 1024 * 1024;
const CANONICAL_FILE_LIMIT = 1024 * 1024;
const GUI_ZIP_LIMIT = 128 * 1024 * 1024;
const GUI_FILE_LIMIT = 256 * 1024 * 1024;
const PAGE_LIMIT = 8;
const API_DEADLINE_MS = 180_000;
const MANAGER_ENV = new Set([
  'APPDATA', 'COMSPEC', 'HOMEDRIVE', 'HOMEPATH', 'LOCALAPPDATA', 'PATH', 'PROGRAMDATA',
  'PROGRAMFILES', 'SYSTEMDRIVE', 'SYSTEMROOT', 'TEMP', 'TMP', 'USERNAME', 'USERPROFILE', 'WINDIR',
]);
const SHA = /^[0-9a-f]{40}$/u;
const DIGEST = /^[0-9a-f]{64}$/u;
const ID = /^[1-9][0-9]{0,19}$/u;
const VERSION = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?$/u;
const HELP = `Usage: node scripts/release/stage-windows-desktop-startup-readback.mjs
  --source-commit <exact-accepted-SHA> --run-id <ID> --run-attempt <attempt>
  --version <expected-version> [--manager-profile release|debug]

Run under the native Clearra manager's bounded build-test profile. Requires
approved GitHub CLI authentication on github.com with read access to the run.
Reads only the chosen attempt and two existing artifacts. Verifies them
before creating a new managed accepted-<run>-<attempt> stage. Never updates
an existing stage, launches the GUI, builds, or publishes.`;

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function hasId(value) {
  return (typeof value === 'string' || Number.isSafeInteger(value)) && ID.test(String(value));
}

export function parseWindowsDesktopStageArgs(argv) {
  const { values, tokens } = parseArgs({
    args: argv, tokens: true, strict: true,
    options: {
      'source-commit': { type: 'string' }, 'run-id': { type: 'string' },
      'run-attempt': { type: 'string' }, version: { type: 'string' },
      'manager-profile': { type: 'string' }, help: { type: 'boolean' },
    },
  });
  requireThat(new Set(tokens.map((token) => token.name)).size === tokens.length, 'duplicate option');
  if (values.help) {
    requireThat(tokens.length === 1, 'help must be the only option');
    return { help: true };
  }
  const options = {
    sourceCommit: values['source-commit'], runId: values['run-id'], runAttempt: values['run-attempt'],
    version: values.version, managerProfile: values['manager-profile'] ?? 'release',
  };
  validateOptions(options);
  return options;
}

function validateOptions(options) {
  requireThat(typeof options.sourceCommit === 'string' && SHA.test(options.sourceCommit) &&
    typeof options.runId === 'string' && ID.test(options.runId) &&
    typeof options.runAttempt === 'string' && ID.test(options.runAttempt) &&
    typeof options.version === 'string' && VERSION.test(options.version) &&
    ['release', 'debug'].includes(options.managerProfile), 'exact source, run, attempt and version are required');
}

// Not a general extractor. The producer uploads one ASCII-named regular
// member; use Node's audited raw deflate primitive with a hard output bound.
// Directory names/metadata are never used to choose filesystem destinations.
function readAuthenticatedSingleZip(archive, name, expectedSha, archiveLimit, fileLimit) {
  requireThat(Buffer.isBuffer(archive) && archive.length >= 22 && archive.length <= archiveLimit &&
    DIGEST.test(expectedSha), 'archive is outside the fixed byte budget');
  const observedSha = sha256(archive);
  requireThat(observedSha === expectedSha, 'raw ZIP differs from authenticated Actions digest');
  const eocd = archive.length - 22;
  requireThat(archive.readUInt32LE(eocd) === 0x06054b50 &&
    archive.readUInt16LE(eocd + 4) === 0 && archive.readUInt16LE(eocd + 6) === 0 &&
    archive.readUInt16LE(eocd + 8) === 1 && archive.readUInt16LE(eocd + 10) === 1 &&
    archive.readUInt16LE(eocd + 20) === 0, 'expected one closed ordinary ZIP entry');
  const offset = archive.readUInt32LE(eocd + 16);
  requireThat(offset >= 30 && offset + 46 <= eocd &&
    archive.readUInt32LE(eocd + 12) === eocd - offset &&
    archive.readUInt32LE(offset) === 0x02014b50, 'ZIP central entry is invalid');
  const flags = archive.readUInt16LE(offset + 8);
  const method = archive.readUInt16LE(offset + 10);
  const crc = archive.readUInt32LE(offset + 16);
  const compressed = archive.readUInt32LE(offset + 20);
  const size = archive.readUInt32LE(offset + 24);
  const nameSize = archive.readUInt16LE(offset + 28);
  const external = archive.readUInt32LE(offset + 38);
  const unixMode = (external >>> 16) & 0o170000;
  const memberName = Buffer.from(name, 'ascii');
  requireThat(nameSize === memberName.length &&
    offset + 46 + nameSize + archive.readUInt16LE(offset + 30) + archive.readUInt16LE(offset + 32) === eocd &&
    archive.subarray(offset + 46, offset + 46 + nameSize).equals(memberName) &&
    archive.readUInt16LE(offset + 34) === 0 && archive.readUInt32LE(offset + 42) === 0 &&
    (flags & ~0x0808) === 0 && [0, 8].includes(method) &&
    (unixMode === 0 || unixMode === 0o100000) && (external & 0x410) === 0 &&
    compressed > 0 && size > 0 && size <= fileLimit, 'ZIP member is not the expected bounded regular file');
  requireThat(archive.readUInt32LE(0) === 0x04034b50 &&
    archive.readUInt16LE(6) === flags && archive.readUInt16LE(8) === method &&
    archive.readUInt16LE(26) === nameSize &&
    archive.subarray(30, 30 + nameSize).equals(memberName), 'ZIP local entry does not match its directory');
  const start = 30 + nameSize + archive.readUInt16LE(28);
  const end = start + compressed;
  requireThat(end <= offset, 'ZIP member exceeds its closed local range');
  if (flags & 8) {
    const descriptor = offset - end === 16 && archive.readUInt32LE(end) === 0x08074b50 ? end + 4 : end;
    requireThat(offset - descriptor === 12 &&
      archive.readUInt32LE(descriptor) === crc &&
      archive.readUInt32LE(descriptor + 4) === compressed &&
      archive.readUInt32LE(descriptor + 8) === size, 'ZIP data descriptor is invalid');
  } else {
    requireThat(end === offset && archive.readUInt32LE(14) === crc &&
      archive.readUInt32LE(18) === compressed && archive.readUInt32LE(22) === size,
    'ZIP local size or checksum differs');
  }
  const data = archive.subarray(start, end);
  requireThat(method !== 0 || compressed === size, 'stored ZIP sizes differ');
  const payload = method === 0 ? data : inflateRawSync(data, { maxOutputLength: size });
  requireThat(payload.length === size && crc32(payload) === crc, 'ZIP member integrity is invalid');
  return { payload, observedSha };
}

async function readAllPages(io, endpoint, key) {
  const list = [];
  let total = null;
  for (let page = 1; page <= PAGE_LIMIT; page++) {
    const result = await io.getJson(`${endpoint}?per_page=100&page=${page}`);
    requireThat(Number.isSafeInteger(result?.total_count) && result.total_count >= 0 &&
      result.total_count <= PAGE_LIMIT * 100 && Array.isArray(result[key]) &&
      result[key].length <= 100 && (total === null || result.total_count === total),
    'invalid or unstable Actions catalogue page');
    total = result.total_count;
    list.push(...result[key]);
    if (list.length === total) return { total_count: total, [key]: list };
    requireThat(list.length < total && result[key].length === 100, 'incomplete or overfull Actions catalogue');
  }
  throw new Error('catalogue page limit reached');
}

export async function stageWindowsDesktopStartupReadback(options, io) {
  validateOptions(options);
  requireThat(io.platform === 'win32' && io.architecture === 'x64', 'native Windows staging host required');
  const { sourceCommit, runId, runAttempt, version } = options;
  const expected = {
    repository: REPOSITORY, source_commit: sourceCommit, run_id: runId, run_attempt: runAttempt,
    release_version: version, pages_base_path: '/Clearra',
  };
  const directoryName = `accepted-${runId}-${runAttempt}`;
  // Reject a colliding or unmanaged destination before any API call.
  io.assertDestination(directoryName, options.managerProfile);
  const authenticated = await io.getJson('/user');
  requireThat(hasId(authenticated?.id) && typeof authenticated.login === 'string' && authenticated.login.length > 0,
    'approved GitHub CLI did not prove an authenticated user');
  const attemptEndpoint = `${STEM}/runs/${runId}/attempts/${runAttempt}`;
  const attempt = await io.getJson(attemptEndpoint);
  const jobs = await readAllPages(io, `${attemptEndpoint}/jobs`, 'jobs');
  const artifactCatalog = await readAllPages(io, `${STEM}/runs/${runId}/artifacts`, 'artifacts');
  const request = { expected, attempt, jobs, artifactCatalog };
  const selected = selectWindowsDesktopTrustedOriginArtifacts(request);
  const canonical = readAuthenticatedSingleZip(
    await io.getArchive(`${STEM}/artifacts/${selected.canonical.artifactId}/zip`, CANONICAL_ZIP_LIMIT),
    CANONICAL_ACCEPTANCE_EVIDENCE_FILE, selected.canonical.sha256, CANONICAL_ZIP_LIMIT, CANONICAL_FILE_LIMIT,
  );
  const jsonText = canonical.payload.toString('utf8');
  const acceptedEvidence = JSON.parse(jsonText);
  requireThat(canonical.payload.equals(Buffer.from(`${canonicalJson(acceptedEvidence)}\n`, 'utf8')),
    'authenticated canonical member is not exact canonical JSON');
  validateCanonicalAcceptanceEvidence(acceptedEvidence, {
    repository: REPOSITORY, sourceCommit, runId, runAttempt, version, basePath: expected.pages_base_path,
  });
  const executableName = `Clearra-GUI-v${version}-windows-x86_64.exe`;
  const guiEntries = acceptedEvidence.final_source_fragments.release_artifacts.filter((entry) => entry.role === 'windows-gui');
  requireThat(acceptedEvidence.jobs.find((job) => job.name === 'windows-gui')?.job_id === selected.producerJobId &&
    guiEntries.length === 1 && guiEntries[0].name === executableName && guiEntries[0].size_bytes <= GUI_FILE_LIMIT,
  'canonical seal differs from authenticated GUI job or expected binary');
  const gui = readAuthenticatedSingleZip(
    await io.getArchive(`${STEM}/artifacts/${selected.gui.artifactId}/zip`, GUI_ZIP_LIMIT),
    executableName, selected.gui.sha256, GUI_ZIP_LIMIT, guiEntries[0].size_bytes,
  );
  const trustedActionsOrigin = buildWindowsDesktopTrustedOrigin({
    ...request, acceptedEvidence,
    downloadedArchives: {
      canonical: { artifact_id: selected.canonical.artifactId, sha256: canonical.observedSha },
      gui: { artifact_id: selected.gui.artifactId, sha256: gui.observedSha },
    },
  });
  validateAcceptedWindowsDesktopStartupInput({ acceptedEvidence, trustedActionsOrigin, executableBytes: gui.payload });
  requireThat(gui.payload.length >= 2 && gui.payload.readUInt16LE(0) === 0x5a4d, 'GUI member is not a Windows image');
  const stage = io.commitDestination(directoryName, options.managerProfile, new Map([
    [CANONICAL_ACCEPTANCE_EVIDENCE_FILE, canonical.payload],
    [executableName, gui.payload],
    // Readback opens this first. Do not make it visible before the other files.
    [TRUSTED_ORIGIN_FILE, Buffer.from(`${canonicalJson(trustedActionsOrigin)}\n`, 'utf8')],
  ]));
  return Object.freeze({
    status: 'staged', stage, source_commit: sourceCommit, run_id: runId, run_attempt: runAttempt,
    artifact_sha256: guiEntries[0].sha256, startup_verified: false, release_authority: false,
  });
}

function nativeIo() {
  requireThat(process.platform === 'win32' && process.arch === 'x64', 'native Windows staging host required');
  const root = realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
  const stageRoot = win32.join(root, ...ROOT_PARTS);
  const GH = (() => {
    for (const component of (process.env.PATH ?? '').split(';')) {
      const directory = component.replace(/^"([^"]+)"$/u, '$1');
      if (!/^[a-z]:[\\/]/iu.test(directory)) continue;
      try {
        const path = realpathSync(win32.join(directory, 'gh.exe'));
        if (!path.toLowerCase().startsWith(root.toLowerCase() + win32.sep) &&
          win32.basename(path).toLowerCase() === 'gh.exe' && lstatSync(path).isFile()) return path;
      } catch {
        // Looking up approved PATH tools does not repeat network work.
      }
    }
    throw new Error('approved native GitHub CLI not found on PATH');
  })();
  const deadline = performance.now() + API_DEADLINE_MS;
  const managerEnv = Object.fromEntries(Object.keys(process.env)
    .filter((key) => MANAGER_ENV.has(key.toUpperCase())).map((key) => [key, process.env[key]]));
  // gh reads its own approved authentication. Do not copy its credentials
  // into an explicit environment object or pass them to storage verification.
  process.env.GH_PROMPT_DISABLED = '1';
  process.env.GH_NO_UPDATE_NOTIFIER = '1';
  process.env.GH_NO_EXTENSION_UPDATE_NOTIFIER = '1';
  async function api(endpoint, cap) {
    requireThat(endpoint === '/user' || endpoint.startsWith(STEM + '/'), 'REST path outside the closed staging repository');
    const timeout = Math.min(60_000, Math.floor(deadline - performance.now()));
    requireThat(timeout > 0, 'staging API deadline expired');
    // gh handles its approved authentication without exposing a token here.
    const { stdout } = await promisify(execFile)(GH, [
      'api', '--hostname', HOST, '--method', 'GET', '-H', 'X-GitHub-Api-Version: 2026-03-10', endpoint,
    ], {
      cwd: root, encoding: 'buffer', maxBuffer: cap, windowsHide: true, timeout, shell: false,
    });
    requireThat(Buffer.isBuffer(stdout) && stdout.length <= cap, 'REST response exceeded its byte budget');
    return stdout;
  }
  function verify(manager, path, rootId) {
    const child = spawnSync(manager, ['--root', root, 'storage', 'verify', '--path', path], {
      cwd: root, stdio: ['ignore', 'pipe', 'pipe'], encoding: 'utf8', maxBuffer: CANONICAL_FILE_LIMIT,
      windowsHide: true, timeout: 10_000, shell: false, env: managerEnv,
    });
    requireThat(child.status === 0 && !child.error, 'native manager did not admit the proposed path');
    const receipt = JSON.parse(child.stdout);
    requireThat(receipt.schema_id === 'clearra.storage-verification.v2' && receipt.root_id === rootId &&
      win32.normalize(receipt.path).toLowerCase() === path.toLowerCase(), 'manager returned a foreign managed path');
  }
  function verifiedDestination(name, profile) {
    requireThat(/^accepted-[1-9][0-9]*-[1-9][0-9]*$/u.test(name), 'invalid stage name');
    const binary = win32.join(root, 'build', 'cargo', 'default', profile, 'clearra-manage.exe');
    const manager = requireOwnedFile(binary, true);
    const stage = win32.join(stageRoot, name);
    verify(manager, binary, 'build-publication');
    verify(manager, stageRoot, 'local-artifacts');
    verify(manager, stage, 'local-artifacts');
    try {
      lstatSync(stage);
      throw new Error('stage already exists; no overwrite or adoption');
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error;
    }
    return { manager, stage };
  }
  return {
    platform: process.platform, architecture: process.arch,
    assertDestination: verifiedDestination,
    getJson: async (endpoint) => JSON.parse((await api(endpoint, METADATA_LIMIT)).toString('utf8')),
    getArchive: api,
    commitDestination(name, profile, files) {
      const { manager, stage } = verifiedDestination(name, profile);
      mkdirSync(stageRoot, { recursive: true });
      verify(manager, stageRoot, 'local-artifacts');
      mkdirSync(stage); // exclusive, no recursive adoption and no replacement
      requireThat(realpathSync(stage) === stage && lstatSync(stage).isDirectory(), 'new stage path is not a real directory');
      for (const [filename, data] of files) {
        requireThat(win32.basename(filename) === filename, 'invalid staged member basename');
        const path = win32.join(stage, filename);
        verify(manager, path, 'local-artifacts');
        writeFileSync(path, data, { flag: 'wx' });
        requireOwnedFile(path);
      }
      // On interruption preserve the exact newly created slot for review. The
      // absent or invalid origin cannot be consumed as accepted readback.
      return stage;
    },
  };
}

async function main(argv) {
  try {
    const options = parseWindowsDesktopStageArgs(argv);
    if (options.help) {
      process.stdout.write(HELP + '\n');
      return;
    }
    const result = await stageWindowsDesktopStartupReadback(options, nativeIo());
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch {
    // Do not print GH stderr or its environment (may contain auth or URLs).
    process.stderr.write(JSON.stringify({ status: 'stage-failed', startup_verified: false, release_authority: false }) + '\n');
    process.exitCode = 2;
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  await main(process.argv.slice(2));
}
