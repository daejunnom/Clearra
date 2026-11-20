// Local diagnostic only. The caller must independently authenticate the
// Actions origin sidecar and archives before staging them. Nothing here
// acquires an artifact, requests an approval, rebuilds, or publishes.
import { spawnSync } from 'node:child_process';
import { closeSync, fstatSync, lstatSync, openSync, readSync, realpathSync } from 'node:fs';
import { resolve, win32 } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import {
  captureChild, readSupervisorReceipt, requireOwnedFile, stateReceiptDirectory,
} from '../benchmark/run-placement-coverage-proof.mjs';
import { CANONICAL_ACCEPTANCE_EVIDENCE_FILE } from './canonical-acceptance-evidence.mjs';
import {
  sealAcceptedWindowsDesktopStartupAttestation,
  selectAcceptedWindowsDesktopStartupArtifact,
  validateAcceptedWindowsDesktopStartupInput,
} from './windows-desktop-startup-attestation.mjs';

export const TRUSTED_ORIGIN_FILE = 'clearra-windows-desktop-trusted-origin.v1.json';
const DIAGNOSTIC_SCHEMA = 'clearra.windows-desktop-startup-local-consumer.v1';
const PROBE = 'scripts/tools/verify-windows-desktop-runtime.ps1';
const PROBE_TIMEOUT = 40;
const SUPERVISOR_TIMEOUT = '75';
const EXE_BUDGET = 512 * 1024 * 1024;
const METADATA_BUDGET = 1024 * 1024;
const SOURCE = /^[0-9a-f]{40}$/u;
const VERSION = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?$/u;
const BASE = ['_local', 'artifacts', 'windows-desktop-runtime'];
const TRACKED_ENTRYPOINTS = [
  PROBE, 'scripts/tools/windows-desktop-startup-proof.psm1',
  'scripts/release/run-windows-desktop-startup-readback.mjs',
  'scripts/release/windows-desktop-startup-attestation.mjs',
  'scripts/release/canonical-acceptance-evidence.mjs', 'scripts/release/canonical-release-evidence.mjs',
  'scripts/benchmark/run-placement-coverage-proof.mjs', 'config/clearra-management.v1.json',
];
const NATIVE_ENV_KEYS = [
  'APPDATA', 'COMSPEC', 'HOMEDRIVE', 'HOMEPATH', 'LOCALAPPDATA', 'NUMBER_OF_PROCESSORS',
  'PATH', 'PROCESSOR_ARCHITECTURE', 'PROGRAMDATA', 'PROGRAMFILES', 'PROGRAMFILES(X86)',
  'SYSTEMDRIVE', 'SYSTEMROOT', 'TEMP', 'TMP', 'USERDOMAIN', 'USERNAME', 'USERPROFILE', 'WINDIR',
];

const HELP = `Usage: node scripts/release/run-windows-desktop-startup-readback.mjs
  --source-commit <exact-accepted-SHA> --evidence-dir <absolute-managed-stage>
  [--manager-profile release|debug]

Use a clean tracked exact-source Windows checkout and a pre-existing native
manager. Stage the authenticated origin JSON, canonical acceptance JSON and
exact versioned GUI EXE under _local/artifacts/windows-desktop-runtime/
accepted-<run-id>-<attempt>. The caller must authenticate their Actions origin;
this command performs no download, rebuild, publish, or approval. The sole
managed startup attempt can return verified (0), unavailable (3), or
admission-contended (4); failures return 2.`;

function requireThat(value, message) {
  if (!value) throw new Error(message);
}

function absoluteWindowsPath(value) {
  requireThat(typeof value === 'string' && /^[a-z]:[\\/]/iu.test(value) &&
    !/[:*?"<>|\u0000-\u001f]/u.test(value.slice(2)) &&
    value.slice(3).split(/[\\/]/u).every((part) => part && !/[. ]$/u.test(part)),
  'expected an absolute Windows path without aliases or traversal');
  return win32.normalize(value);
}

function pathKey(value) {
  return absoluteWindowsPath(value).toLowerCase();
}

function under(path, root) {
  return pathKey(path).startsWith(pathKey(root) + win32.sep);
}

export function parseWindowsDesktopStartupReadbackArgs(argv) {
  const { values, tokens } = parseArgs({
    args: argv,
    options: {
      'source-commit': { type: 'string' }, 'evidence-dir': { type: 'string' },
      'manager-profile': { type: 'string' }, help: { type: 'boolean' },
    },
    strict: true, tokens: true,
  });
  const names = tokens.map((token) => token.name);
  requireThat(new Set(names).size === names.length, 'duplicate option');
  if (values.help) {
    requireThat(names.length === 1, 'help must be the only option');
    return { help: true };
  }
  const sourceCommit = values['source-commit'];
  const managerProfile = values['manager-profile'] ?? 'release';
  requireThat(SOURCE.test(sourceCommit ?? '') && ['debug', 'release'].includes(managerProfile),
    'exact source commit and approved manager profile are required');
  return { sourceCommit, managerProfile, evidenceDirectory: absoluteWindowsPath(values['evidence-dir']) };
}

function nativeEnvironment(environment, sourceCommit) {
  const nativeKeys = new Map(Object.keys(environment).map((key) => [key.toUpperCase(), key]));
  const output = {};
  for (const key of NATIVE_ENV_KEYS) {
    const value = environment[nativeKeys.get(key)];
    if (typeof value === 'string') output[key] = value;
  }
  output.CLEARRA_SOURCE_COMMIT = sourceCommit;
  output.CLEARRA_ENGINE_BUILD_ID = sourceCommit;
  return output;
}

function outcome(status, reason, exitCode, fields = {}) {
  return {
    exitCode,
    report: { schema_id: DIAGNOSTIC_SCHEMA, status, reason, ui_route_verified: false,
      release_authority: false, ...fields },
  };
}

function admissionContended(execution) {
  return execution.signal === null && !execution.overflow && execution.code === 70 &&
    execution.stdout.length === 0 &&
    !/^clearra_runtime_receipt=/mu.test(execution.stderr) &&
    /^runtime: E_CLEARRA_RUNTIME_SLOT_BUSY(?::|\s|$)/mu.test(execution.stderr);
}

export async function runWindowsDesktopStartupReadback(options, io) {
  let reason = 'native-runtime-preflight-failed';
  let sourceCommit;
  try {
    if (io.platform !== 'win32' || io.architecture !== 'x64') {
      return outcome('unavailable', 'windows-x64-host-required', 3);
    }
    sourceCommit = options.sourceCommit;
    requireThat(SOURCE.test(sourceCommit ?? '') && ['debug', 'release'].includes(options.managerProfile),
      'invalid run options');
    requireThat(io.environment.CLEARRA_RUNTIME_SUPERVISED !== '1',
      'nested manager leases are not allowed');
    const root = absoluteWindowsPath(io.repositoryRoot);
    const env = nativeEnvironment(io.environment, sourceCommit);
    reason = 'git-checkout-not-exact-clean-tracked-source';
    io.verifyCheckout(root, sourceCommit, env);
    reason = 'input-not-in-named-managed-stage';
    const stage = absoluteWindowsPath(options.evidenceDirectory);
    const stageBase = win32.join(root, ...BASE);
    requireThat(pathKey(win32.dirname(stage)) === pathKey(stageBase) &&
      /^accepted-[1-9][0-9]{0,19}-[1-9][0-9]{0,19}$/u.test(win32.basename(stage)),
    'not a named accepted Windows GUI stage');
    const manager = win32.join(root, 'build', 'cargo', 'default', options.managerProfile, 'clearra-manage.exe');
    io.requireLocalFile(manager, true);
    io.verifyManagedPath(manager, root, manager, 'build-publication', env);
    io.verifyManagedPath(manager, root, stage, 'local-artifacts', env);
    io.requireDirectory(stage);
    const readStage = (name, maxBytes) => {
      const file = absoluteWindowsPath(win32.join(stage, name));
      requireThat(pathKey(win32.dirname(file)) === pathKey(stage), 'staged input is not a direct child');
      io.verifyManagedPath(manager, root, file, 'local-artifacts', env);
      io.requireLocalFile(file);
      return io.readBounded(file, maxBytes);
    };
    reason = 'accepted-artifact-preflight-failed';
    const trustedActionsOrigin = JSON.parse(readStage(TRUSTED_ORIGIN_FILE, METADATA_BUDGET).toString('utf8'));
    const acceptedEvidence = JSON.parse(readStage(CANONICAL_ACCEPTANCE_EVIDENCE_FILE, METADATA_BUDGET).toString('utf8'));
    const inputs = { trustedActionsOrigin, acceptedEvidence };
    const artifact = selectAcceptedWindowsDesktopStartupArtifact(inputs);
    const { source_commit, run_id, run_attempt, release_version } = trustedActionsOrigin;
    requireThat(source_commit === sourceCommit && VERSION.test(release_version) &&
      win32.basename(stage) === `accepted-${run_id}-${run_attempt}` &&
      artifact.size_bytes <= EXE_BUDGET, 'stage, source, version, or executable budget differs');
    const executable = win32.join(stage, artifact.name);
    validateAcceptedWindowsDesktopStartupInput({
      ...inputs, executableBytes: readStage(artifact.name, artifact.size_bytes),
    });
    reason = 'native-powershell-unavailable';
    const powershell = io.findPowerShell(root, env);
    requireThat(win32.basename(absoluteWindowsPath(powershell)).toLowerCase() === 'pwsh.exe' &&
      !under(powershell, root), 'PowerShell must come from the trusted native host PATH');
    const args = [
      '--root', root, 'runtime', 'run', '--producer', 'windows-desktop-startup',
      '--profile', 'local-service', '--timeout', SUPERVISOR_TIMEOUT, '--',
      powershell, '-NoProfile', '-NonInteractive', '-File', PROBE,
      '-Executable', executable, '-Version', release_version, '-SourceCommit', sourceCommit,
      '-ExpectedSha256', artifact.sha256, '-TimeoutSeconds', String(PROBE_TIMEOUT),
    ];
    // Last source verification before the sole GUI attempt.
    reason = 'git-checkout-not-exact-clean-tracked-source';
    io.verifyCheckout(root, sourceCommit, env);
    reason = 'native-supervisor-invocation-failed';
    const execution = await io.capture(manager, args, root, env);
    if (admissionContended(execution)) {
      return outcome('admission-contended', 'E_CLEARRA_RUNTIME_SLOT_BUSY', 4,
        { source_commit: sourceCommit });
    }
    reason = 'supervisor-receipt-not-bound';
    requireThat(execution.signal === null && !execution.overflow && Number.isInteger(execution.code),
      'supervisor invocation did not return bounded output');
    const supervisorReceipt = io.readReceipt(execution, env);
    requireThat(execution.code === supervisorReceipt.outcome?.return_code,
      'native driver exit differs from manager receipt');
    reason = 'git-checkout-not-exact-clean-tracked-source';
    io.verifyCheckout(root, sourceCommit, env);
    reason = 'startup-attestation-rejected';
    const sealed = sealAcceptedWindowsDesktopStartupAttestation({
      ...inputs, executableBytes: readStage(artifact.name, artifact.size_bytes),
      supervisorReceipt, startupOutput: execution.stdout,
      invocation: {
        repository_root: root, executable_path: executable, powershell,
        probe_timeout_seconds: PROBE_TIMEOUT,
      },
    });
    return { exitCode: sealed.supervisor.child_exit_code, report: sealed };
  } catch {
    return outcome('failed', reason, 2, SOURCE.test(sourceCommit ?? '') ? { source_commit: sourceCommit } : {});
  }
}

function captureFinite(command, args, cwd, env) {
  const child = spawnSync(command, args, {
    cwd, env, windowsHide: true, encoding: 'utf8', timeout: 15_000, maxBuffer: METADATA_BUDGET,
    stdio: ['ignore', 'pipe', 'pipe'], shell: false,
  });
  requireThat(child.status === 0 && !child.error, 'bounded native preflight did not succeed');
  return child.stdout;
}

function findNativeExecutable(name, root, env) {
  for (const entry of (env.PATH ?? '').split(';')) {
    const directory = entry.replace(/^"([^"]+)"$/u, '$1');
    // Empty/relative entries and a shadow executable from the repository
    // could otherwise win ordinary Windows executable lookup.
    if (!/^[a-z]:[\\/]/iu.test(directory)) continue;
    const candidate = win32.join(directory, name);
    if (under(candidate, root)) continue;
    try {
      const path = absoluteWindowsPath(realpathSync(candidate));
      if (!under(path, root) && win32.basename(path).toLowerCase() === name && lstatSync(path).isFile()) {
        return path;
      }
    } catch {
      // Continue a PATH lookup, not a retry of a managed invocation.
    }
  }
  throw new Error('native tool not found on the trusted host PATH');
}

function verifyCheckout(root, sourceCommit, env) {
  const git = findNativeExecutable('git.exe', root, env);
  const run = (...args) => captureFinite(git, ['--no-optional-locks', '-c', 'core.fsmonitor=false',
    '-c', 'core.untrackedCache=false', '-C', root, ...args], root, env).trim();
  requireThat(pathKey(run('rev-parse', '--show-toplevel')) === pathKey(root) &&
    run('rev-parse', '--verify', 'HEAD') === sourceCommit &&
    run('status', '--porcelain=v1', '--untracked-files=no', '--ignore-submodules=none') === '',
  'exact clean tracked checkout is required');
  run('ls-files', '--error-unmatch', '--', ...TRACKED_ENTRYPOINTS);
  const flags = run('ls-files', '-v', '--', ...TRACKED_ENTRYPOINTS).split(/\r?\n/u);
  requireThat(flags.length === TRACKED_ENTRYPOINTS.length && flags.every((line) => line.startsWith('H ')),
    'probe entry point is absent or hidden from normal worktree checks');
  for (const file of TRACKED_ENTRYPOINTS) requireOwnedFile(win32.join(root, file));
}

function nativeIo() {
  return {
    platform: process.platform, architecture: process.arch, environment: process.env,
    repositoryRoot: process.platform === 'win32' ? realpathSync(fileURLToPath(new URL('../..', import.meta.url))) : null,
    verifyCheckout,
    requireLocalFile: requireOwnedFile,
    requireDirectory(stage) {
      requireThat(realpathSync(stage) === stage && lstatSync(stage).isDirectory(), 'stage is not a real owned directory');
    },
    verifyManagedPath(manager, root, path, rootId, env) {
      const proof = JSON.parse(captureFinite(manager, ['--root', root, 'storage', 'verify', '--path', path], root, env));
      requireThat(proof.schema_id === 'clearra.storage-verification.v2' && proof.root_id === rootId &&
        pathKey(proof.path) === pathKey(path), 'native manager did not confirm the expected managed path');
    },
    readBounded(path, limit) {
      const identity = lstatSync(path);
      const { size, dev, ino } = identity;
      requireThat(identity.isFile() && identity.nlink === 1 && size >= 1 && size <= limit,
        'input is not a regular staged file within the finite read budget');
      const fd = openSync(path, 'r');
      try {
        const opened = fstatSync(fd);
        requireThat(opened.isFile() && opened.nlink === 1 && opened.size === size &&
          opened.dev === dev && opened.ino === ino && realpathSync(path) === path,
        'input identity changed before opening it');
        const bytes = Buffer.alloc(size);
        for (let offset = 0; offset < size;) {
          const count = readSync(fd, bytes, offset, size - offset, offset);
          requireThat(count > 0, 'input became truncated');
          offset += count;
        }
        requireThat(fstatSync(fd).size === size, 'input size changed while it was read');
        return bytes;
      } finally {
        closeSync(fd);
      }
    },
    findPowerShell: (root, env) => findNativeExecutable('pwsh.exe', root, env),
    capture: captureChild,
    readReceipt: (execution, env) => readSupervisorReceipt(execution, stateReceiptDirectory(env)),
  };
}

async function main(argv) {
  let result;
  try {
    const args = parseWindowsDesktopStartupReadbackArgs(argv);
    if (args.help) {
      process.stdout.write(HELP + '\n');
      return;
    }
    result = await runWindowsDesktopStartupReadback(args, nativeIo());
  } catch {
    result = outcome('failed', 'invalid-local-consumer-invocation', 2);
  }
  process.stdout.write(JSON.stringify(result.report) + '\n');
  process.exitCode = result.exitCode;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  await main(process.argv.slice(2));
}
