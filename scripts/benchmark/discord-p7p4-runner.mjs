// Local invocation of the unchanged product runner. Run only under
// clearra-manage runtime run --profile benchmark-search --timeout 900.
// Usage: node scripts/benchmark/discord-p7p4-runner.mjs /absolute/path/to/clearra N [path|path-2l|legacy-unique]
// Redirect stdout to an admitted _local/artifacts path. No server or client
// is created; the only spawned application is the supplied local Linux CLI.
import { spawn as nodeSpawn } from 'node:child_process';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { isAbsolute, resolve } from 'node:path';
import { availableParallelism } from 'node:os';
import { fileURLToPath } from 'node:url';

import { prepareClearraArguments } from '../../apps/clearra-discord-bot/src/clearra/command.mjs';
import { loadClearraJobServiceConfig } from '../../apps/clearra-discord-bot/src/job-service/config.mjs';
import { ClearraCommandRunner } from '../../apps/clearra-discord-bot/src/job-service/runner.mjs';

// These typed argv are checked against the real public slash lowering in the
// bounded contract tests. Keeping the fixture dependency-free lets the native
// diagnostic consume an already published CLI without installing JS encoders.
export function discordP7p4Fixture(mode = 'path') {
  if (!['path', 'path-2l', 'legacy-unique'].includes(mode)) {
    throw new Error('P7P4 diagnostic mode must be path, path-2l or legacy-unique.');
  }
  const legacy = mode === 'legacy-unique';
  const lines = mode === 'path-2l' ? 2 : 4;
  return Object.freeze({
    mode,
    capabilityId: legacy ? null : 'pc.path',
    objective: legacy ? 'unique' : 'all',
    countPolicy: legacy ? 'CountUnique' : 'CountAll',
    sourcePattern: 'P7P4',
    originalPatternCount: 4_233_600,
    originalSequenceLength: 11,
    lines,
    hold: 'empty',
    queueKnowledge: 'oracle',
    rule: 'srs-plus',
    arguments: Object.freeze(legacy ? [
      'pc', '--board-mask', '0', '--height', '4', '--pieces', '10',
      '--lines', '4', '--count', 'unique', '--patterns', 'P7P4',
      '--rule', 'srs-plus', '--hold', 'empty', '--queue-knowledge', 'oracle',
    ] : [
      'pc', 'path', '--lines', String(lines), '--board-mask', '0x0',
      '--height', String(lines), '--pieces', String(lines * 10 / 4),
      '--patterns', 'P7P4', '--hold', 'empty', '--rule', 'srs-plus',
    ]),
  });
}

export function nativePathReceipt(stdout) {
  try {
    const result = JSON.parse(stdout);
    const summary = result.summary ?? {};
    return {
      kind: result.kind,
      sourceCommit: result.runtime_identity?.source_commit,
      engineBuildId: result.runtime_identity?.engine_build_id,
      capabilityId: summary.capability_id,
      problemId: summary.problem_id,
      materializedPatternCount: summary.materialized_pattern_count,
      witnessCount: summary.witness_count,
      complete: summary.complete,
      pageContract: summary.page_contract,
      pageSourceAvailable: summary.page_source_available,
      pageSourceIdentitySha256: summary.page_source_identity_sha256,
      geometryCount: summary.geometry_count,
    };
  } catch {
    return null;
  }
}

async function main() {
  const executable = process.argv[2];
  const workers = Number(process.argv[3]);
  const fixture = discordP7p4Fixture(process.argv[4]);
  if (process.platform !== 'linux' || ![4, 5].includes(process.argv.length) || !isAbsolute(executable ?? '') ||
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

  const arguments_ = fixture.arguments;
  const preparedArguments = prepareClearraArguments(arguments_, {
    workers,
    useAllLogicalProcessors: true,
    logicalProcessors: workers,
    outputFormat: 'json',
    includeSolutionData: fixture.mode !== 'legacy-unique',
  });
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
    useAllLogicalProcessors: config.useAllLogicalProcessors,
    observedLogicalProcessors: config.processLogicalProcessors,
    maxOutputBytes: config.maxOutputBytes,
    maxArtifactBytes: config.maxArtifactBytes,
    deadlineMs: config.pcSearchTimeoutMs,
  };

  // Observation only: the production runner still controls argv, timers, output
  // accounting, signals, projection and cleanup. Path diagnostics retain at most
  // the existing 4-MiB pipe cap until close, then only a bounded native receipt.
  // Listeners never pause, resume, write to or kill the child.
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
    preparedArguments: null,
    expectedVcpus: null,
  };
  let nativeReceipt = null;
  function observedSpawn(file, childArguments, options) {
    const artifactIndex = childArguments.indexOf('--solution-output');
    const comparableArguments = artifactIndex < 0 ? childArguments : childArguments.slice(0, artifactIndex);
    assert.deepEqual(comparableArguments, preparedArguments, 'runner argv changed the requested fixture');
    assert.equal(options.env?.CLEARRA_EXPECTED_VCPUS, String(workers));
    observation.preparedArguments = [...childArguments];
    observation.expectedVcpus = options.env.CLEARRA_EXPECTED_VCPUS;
    let rawStdout = [];
    const child = nodeSpawn(file, childArguments, options);
    observation.spawnMs = elapsedMs();
    child.stdout?.on('data', chunk => {
      if (observation.firstStdoutMs === null) observation.firstStdoutMs = elapsedMs();
      observation.stdoutBytes += Buffer.isBuffer(chunk) ? chunk.length : Buffer.byteLength(chunk);
      if (fixture.mode !== 'legacy-unique') {
        if (observation.stdoutBytes <= config.maxOutputBytes && rawStdout !== null) rawStdout.push(Buffer.from(chunk));
        else rawStdout = null;
      }
    });
    child.stderr?.on('data', chunk => {
      observation.stderrBytes += Buffer.isBuffer(chunk) ? chunk.length : Buffer.byteLength(chunk);
    });
    child.on('close', (exitCode, signal) => {
      observation.closeMs = elapsedMs();
      observation.exitCode = exitCode;
      observation.signal = signal;
      if (fixture.mode !== 'legacy-unique' && rawStdout !== null) {
        nativeReceipt = nativePathReceipt(Buffer.concat(rawStdout).toString('utf8'));
      }
      rawStdout = null;
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
      schema: fixture.mode === 'legacy-unique' ? 'clearra.local-discord-runner.v1' : 'clearra.local-discord-runner.v2',
      diagnosticOnly: true,
      topologyScope: 'observed CI runner; not Cloud Run 8-vCPU performance or deployment evidence',
      nodeVersion: process.versions.node,
      adapterSourceCommit: process.env.CLEARRA_P7P4_ADAPTER_SOURCE ?? null,
      executable,
      arguments: arguments_,
      fixture,
      policy,
      nativeReceipt,
      observation: { ...observation, wallTimeMs },
      ...terminal,
      artifact: artifactMetadata,
    })}\n`);
    process.exitCode = result.exitCode === 0 && !result.signal && !result.failureCode ? 0 : 1;
  } catch (error) {
    const wallTimeMs = elapsedMs();
    process.stdout.write(`${JSON.stringify({
      schema: fixture.mode === 'legacy-unique' ? 'clearra.local-discord-runner.v1' : 'clearra.local-discord-runner.v2',
      diagnosticOnly: true,
      topologyScope: 'observed CI runner; not Cloud Run 8-vCPU performance or deployment evidence',
      nodeVersion: process.versions.node,
      adapterSourceCommit: process.env.CLEARRA_P7P4_ADAPTER_SOURCE ?? null,
      executable,
      arguments: arguments_,
      fixture,
      policy,
      nativeReceipt,
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
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
