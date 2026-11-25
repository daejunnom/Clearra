// Local invocation of the unchanged product runner. Run only under
// clearra-manage runtime run --profile benchmark-search --timeout 900.
// Usage: node scripts/benchmark/discord-p7p4-runner.mjs /absolute/path/to/clearra N
// Redirect stdout to an admitted _local/artifacts path. No server or client
// is created; the only spawned application is the supplied local Linux CLI.
import { spawn as nodeSpawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { isAbsolute } from 'node:path';
import { availableParallelism } from 'node:os';

import { loadClearraJobServiceConfig } from '../../apps/clearra-discord-bot/src/job-service/config.mjs';
import { ClearraCommandRunner } from '../../apps/clearra-discord-bot/src/job-service/runner.mjs';

const executable = process.argv[2];
const workers = Number(process.argv[3]);
if (process.platform !== 'linux' || process.argv.length !== 4 || !isAbsolute(executable ?? '') ||
    !Number.isSafeInteger(workers) || workers < 1 || workers > availableParallelism()) {
  throw new Error('Use Linux, an absolute executable path and the explicit observed CPU width.');
}
if (process.versions.node !== '22.23.2') {
  throw new Error('Use the pinned Node 22.23.2 executable for this production comparison.');
}

// The product runner uses os.tmpdir(). Keep its native CTK3 files inside the
// verified output root; its normal cleanup still owns each per-job directory
// and file. The caller verifies an optional platform-root destination first.
const temporaryRoot = process.env.CLEARRA_P7P4_TEMP_ROOT;
if (!isAbsolute(temporaryRoot ?? '')) throw new Error('Artifact temporary root must be absolute.');
await mkdir(temporaryRoot, { recursive: true });
process.env.TMPDIR = temporaryRoot;

const config = loadClearraJobServiceConfig({
  CLEARRA_EXECUTABLE: executable,
  CLEARRA_EXPECTED_VCPUS: String(workers),
  CLEARRA_SEARCH_WORKERS_PER_SESSION: String(workers),
  CLEARRA_USE_ALL_LOGICAL_PROCESSORS: '1',
  CLEARRA_MAX_CONCURRENT_JOBS: '1',
  CLEARRA_MAX_OUTPUT_BYTES: String(4 * 1024 * 1024),
  CLEARRA_MAX_GIF_BYTES: String(24 * 1024 * 1024),
  CLEARRA_MAX_CTK3_FILE_BYTES: String(24 * 1024 * 1024),
  CLEARRA_SEARCH_TIMEOUT_MS: String(14 * 60_000),
  CLEARRA_PC_SEARCH_TIMEOUT_MS: String(14 * 60_000),
  CLEARRA_JOB_TERMINATION_GRACE_MS: '2000',
  // These loader settings do not start an HTTP service.
  CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: '1',
  CLEARRA_LISTEN_HOST: '127.0.0.1',
});

const arguments_ = [
  'pc', '--board-mask', '0', '--height', '4', '--pieces', '10',
  '--lines', '4', '--count', 'unique', '--patterns', 'P7P4',
  '--rule', 'srs-plus', '--hold', 'empty', '--queue-knowledge', 'oracle',
];
const request = {
  arguments: arguments_,
  timeoutClass: 'pc_reverse',
  deadlineUnixMs: Date.now() + 14 * 60_000,
  maxOutputBytes: config.maxOutputBytes,
  maxArtifactBytes: config.maxArtifactBytes,
};
const policy = {
  workers: config.searchWorkersPerSession,
  expectedVcpus: config.expectedVcpus,
  observedLogicalProcessors: config.processLogicalProcessors,
  maxOutputBytes: config.maxOutputBytes,
  maxArtifactBytes: config.maxArtifactBytes,
  deadlineMs: config.pcSearchTimeoutMs,
};

// Observation only: the production runner still controls argv, timers, output
// accounting, signals, projection and cleanup. These listeners retain counters,
// not output chunks, and never pause, resume, write to or kill the child.
const executionStartedNs = process.hrtime.bigint();
const elapsedMs = () => Number(process.hrtime.bigint() - executionStartedNs) / 1e6;
const observation = {
  spawnMs: null,
  firstStdoutMs: null,
  stdoutBytes: 0,
  stderrBytes: 0,
  closeMs: null,
  exitCode: null,
  signal: null,
};
function observedSpawn(...arguments_) {
  const child = nodeSpawn(...arguments_);
  observation.spawnMs = elapsedMs();
  child.stdout?.on('data', chunk => {
    if (observation.firstStdoutMs === null) observation.firstStdoutMs = elapsedMs();
    observation.stdoutBytes += Buffer.isBuffer(chunk) ? chunk.length : Buffer.byteLength(chunk);
  });
  child.stderr?.on('data', chunk => {
    observation.stderrBytes += Buffer.isBuffer(chunk) ? chunk.length : Buffer.byteLength(chunk);
  });
  child.on('close', (exitCode, signal) => {
    observation.closeMs = elapsedMs();
    observation.exitCode = exitCode;
    observation.signal = signal;
  });
  return child;
}

try {
  const result = await new ClearraCommandRunner(config, { spawn: observedSpawn }).execute(request);
  const wallTimeMs = elapsedMs();
  const { artifact, ...terminal } = result;
  // Preserve product stdout and existing stageDurationsMs verbatim. The runner
  // has already read and validated the complete native artifact. Retain its
  // receipt here without copying its up-to-24-MiB source into this report.
  const artifactMetadata = artifact ? {
    contract: artifact.contract,
    filename: artifact.filename,
    byteLength: artifact.byteLength,
    sha256: artifact.sha256,
    solutionCount: artifact.solutionCount,
    complete: artifact.complete,
  } : null;
  process.stdout.write(`${JSON.stringify({
    schema: 'clearra.local-discord-runner.v1',
    diagnosticOnly: true,
    topologyScope: 'observed CI runner; not Cloud Run 8-vCPU performance or deployment evidence',
    nodeVersion: process.versions.node,
    adapterSourceCommit: process.env.CLEARRA_P7P4_ADAPTER_SOURCE ?? null,
    executable,
    arguments: arguments_,
    policy,
    observation: { ...observation, wallTimeMs },
    ...terminal,
    artifact: artifactMetadata,
  })}\n`);
  process.exitCode = result.exitCode === 0 && !result.signal && !result.failureCode ? 0 : 1;
} catch (error) {
  const wallTimeMs = elapsedMs();
  process.stdout.write(`${JSON.stringify({
    schema: 'clearra.local-discord-runner.v1',
    diagnosticOnly: true,
    topologyScope: 'observed CI runner; not Cloud Run 8-vCPU performance or deployment evidence',
    nodeVersion: process.versions.node,
    adapterSourceCommit: process.env.CLEARRA_P7P4_ADAPTER_SOURCE ?? null,
    executable,
    arguments: arguments_,
    policy,
    observation: { ...observation, wallTimeMs },
    error: {
      name: error?.name ?? 'Error',
      message: error instanceof Error ? error.message : String(error),
      failureCode: error?.failureCode ?? null,
      failureStage: error?.failureStage ?? null,
    },
    stageDurationsMs: error?.stageDurationsMs ?? {},
  })}\n`);
  process.exitCode = 1;
}
