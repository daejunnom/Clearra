import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        DEFAULT_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority,
        createHostCapabilitySnapshot,
        isHostCapabilitySnapshot,
        normalizeRuntimeWarmupPolicy
      } from './src/lib/wasm/hostCapabilitySnapshot.ts';
      export { WasmTerminalWorkerController } from './src/lib/wasm/WasmTerminalWorkerController.ts';
      export {
        clearWasmTerminalResult,
        updateWasmCommandText
      } from './src/lib/wasm/wasmWorkerStore.ts';
      export {
        createDefaultWorkspaceRequest,
        normalizeWorkspaceRequest,
        updateWorkspaceDraft
      } from './src/lib/workspace/solverWorkspaceModel.ts';
      export {
        BUILD_PROBABILITY_PRIMARY_METRIC,
        createDefaultBuildProbabilityRequest,
        normalizeBuildProbabilityRequest,
        updateBuildProbabilityDraft
      } from './src/lib/workspace/buildProbabilityModel.ts';
      export { workspaceMessage } from './src/lib/workspace/workspaceI18n.ts';
      export {
        ClearraWasmTransferLimitError,
        assertWasmTransferWithinHostCap
      } from '../../apps/clearra-web/src/workers/clearraWasmRuntime.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const MIB = 1024 * 1024;

test('draft mode roundtrips preserve inactive PC and build selections', () => {
  const pcSelected = production.updateWorkspaceDraft(
    production.createDefaultWorkspaceRequest(),
    {
      scoreMode: 'summary',
      scoreProfile: 'guideline',
      spinProfile: 'all-mini-plus',
      preserveB2B: true,
      initialB2B: 9,
      queueKnowledge: 'visible-7',
      tablebaseEnabled: true,
      precomputeBuildDependencies: true
    }
  );
  const pcTilingDraft = production.updateWorkspaceDraft(pcSelected, {
    scoreMode: 'tiling'
  });
  const pcExecution = production.normalizeWorkspaceRequest(pcTilingDraft);
  assert.equal(pcExecution.queueKnowledge, 'oracle');
  assert.equal(pcExecution.tablebaseEnabled, false);
  assert.equal(pcTilingDraft.queueKnowledge, 'visible-7');
  assert.equal(pcTilingDraft.scoreProfile, 'guideline');
  assert.equal(pcTilingDraft.initialB2B, 9);
  const pcRestored = production.updateWorkspaceDraft(pcTilingDraft, {
    scoreMode: 'summary'
  });
  assert.equal(pcRestored.scoreProfile, 'guideline');
  assert.equal(pcRestored.spinProfile, 'all-mini-plus');
  assert.equal(pcRestored.preserveB2B, true);
  assert.equal(pcRestored.tablebaseEnabled, true);

  const buildSelected = production.updateBuildProbabilityDraft(
    production.createDefaultBuildProbabilityRequest(),
    {
      aggregation: 'spin',
      rule: 'jstris-180',
      spinProfile: 'all-spin-plus',
      preserveB2B: true,
      precomputeBuildDependencies: true,
      finesse: 'inputs',
      patternKnowledge: 'visible-7'
    }
  );
  const buildTilingDraft = production.updateBuildProbabilityDraft(buildSelected, {
    aggregation: 'tiling'
  });
  const buildExecution = production.normalizeBuildProbabilityRequest(buildTilingDraft);
  assert.equal(buildExecution.finesse, 'off');
  assert.equal(buildExecution.precomputeBuildDependencies, false);
  assert.equal(buildTilingDraft.finesse, 'inputs');
  assert.equal(buildTilingDraft.patternKnowledge, 'visible-7');
  const buildRestored = production.updateBuildProbabilityDraft(buildTilingDraft, {
    aggregation: 'spin'
  });
  assert.equal(buildRestored.rule, 'jstris-180');
  assert.equal(buildRestored.spinProfile, 'all-spin-plus');
  assert.equal(buildRestored.patternKnowledge, 'visible-7');
});

test('one main-thread capability snapshot survives a lower worker hardware report', () => {
  const snapshot = production.createHostCapabilitySnapshot({
    snapshotId: 'main-eight',
    source: 'browser-main',
    reportedLogicalProcessors: 8,
    reportedDeviceMemoryGiB: 4
  });
  const authority = production.automaticWorkerAuthority(snapshot);
  assert.equal(production.isHostCapabilitySnapshot(snapshot), true);
  assert.equal(
    production.isHostCapabilitySnapshot({ ...snapshot, automaticWorkerCap: 8 }),
    false
  );
  assert.equal(authority.workersRequested, 7);
  assert.equal(authority.workersEffective, 7);

  const worker = new FakeWorker(4);
  const controller = new production.WasmTerminalWorkerController(() => worker, snapshot);
  controller.prewarm(
    authority.workersEffective,
    false,
    production.DEFAULT_RUNTIME_WARMUP_POLICY,
    authority
  );
  production.clearWasmTerminalResult();
  production.updateWasmCommandText('clearra pc --lines 2 --queue IOTSZ --workers 7');
  assert.equal(controller.run(), true);

  const prewarm = worker.messages.find((message) => message.type === 'prewarm_runtime');
  const run = worker.messages.find((message) => message.type === 'run_command_text');
  assert.equal(worker.hardwareConcurrency, 4);
  assert.equal(prewarm.hostCapabilitySnapshot.reportedLogicalProcessors, 8);
  assert.equal(prewarm.workerAuthority.workersEffective, 7);
  assert.equal(prewarm.workerAuthority.reason, 'reserved-main-thread');
  assert.equal(prewarm.workerAuthority, authority);
  assert.equal(run.workerAuthority.workersEffective, 7);
  assert.equal(run.workerAuthority.reason, 'reserved-main-thread');
  assert.equal(run.hostCapabilitySnapshot.snapshotId, 'main-eight');
  controller.dispose();

  const rootWorkerSource = readFileSync(
    new URL('../../../apps/clearra-web/src/workers/clearraWorker.ts', import.meta.url),
    'utf8'
  );
  const verifierSource = readFileSync(
    new URL('../../../apps/clearra-web/src/workers/clearraVerifierWorker.ts', import.meta.url),
    'utf8'
  );
  const modelSource = readFileSync(
    new URL('../src/lib/workspace/solverWorkspaceModel.ts', import.meta.url),
    'utf8'
  );
  const exportSource = readFileSync(
    new URL('../src/lib/workspace/solutionExportAsync.ts', import.meta.url),
    'utf8'
  );
  const ctkDrawerSource = readFileSync(
    new URL('../src/lib/workspace/CtkDrawerWorkspace.svelte', import.meta.url),
    'utf8'
  );
  assert.doesNotMatch(rootWorkerSource, /navigator\.hardwareConcurrency/u);
  assert.doesNotMatch(modelSource, /navigator\?*\.hardwareConcurrency/u);
  assert.doesNotMatch(exportSource, /hardwareConcurrency/u);
  assert.match(verifierSource, /request\.hostCapabilities/u);
  assert.match(exportSource, /automaticWorkerAuthority\(snapshot\)\.workersEffective/u);
  assert.match(ctkDrawerSource, /const documentWorkerCount = automaticWorkerAuthority/u);
  assert.match(ctkDrawerSource, /workers: documentWorkerCount/u);
});

test('CPU-only policies suppress GPU warmup in build and forward production paths', () => {
  const normalized = production.normalizeRuntimeWarmupPolicy({
    backend: 'cpu',
    cpuWarmup: true,
    gpuWarmup: true
  });
  assert.deepEqual(normalized, production.CPU_ONLY_RUNTIME_WARMUP_POLICY);

  for (const file of [
    'BuildProbabilityWorkspace.svelte',
    'ForwardSearchWorkspace.svelte',
    'SetupFinderWorkspace.svelte'
  ]) {
    const source = readFileSync(
      new URL(`../src/lib/workspace/${file}`, import.meta.url),
      'utf8'
    );
    assert.match(source, /CPU_ONLY_RUNTIME_WARMUP_POLICY/u, file);
  }
  const workerSource = readFileSync(
    new URL('../../../apps/clearra-web/src/workers/clearraWorker.ts', import.meta.url),
    'utf8'
  );
  assert.match(
    workerSource,
    /if \(normalizedWarmupPolicy\.gpuWarmup\) void startGpuWarmupAfterWasm/u
  );
});

test('device-memory snapshot owns a conservative transfer cap and rejects before resize', () => {
  for (const value of [undefined, 0, Number.NaN, '4']) {
    assert.equal(snapshotForMemory(value).wasmTransferByteCap, 32 * MIB);
  }
  assert.equal(snapshotForMemory(0.5).wasmTransferByteCap, 16 * MIB);
  assert.equal(snapshotForMemory(1).wasmTransferByteCap, 32 * MIB);
  assert.equal(snapshotForMemory(4).wasmTransferByteCap, 128 * MIB);
  assert.equal(snapshotForMemory(8).wasmTransferByteCap, 128 * MIB);

  for (const cap of [16 * MIB, 32 * MIB, 128 * MIB]) {
    assert.doesNotThrow(() => production.assertWasmTransferWithinHostCap(cap, cap));
    assert.throws(
      () => production.assertWasmTransferWithinHostCap(cap + 1, cap),
      (error) =>
        error instanceof production.ClearraWasmTransferLimitError &&
        error.diagnosticCode === 'E_WASM_TRANSFER_HOST_LIMIT' &&
        error.requestedBytes === cap + 1 &&
        error.limitBytes === cap
    );
    assert.doesNotThrow(() => production.assertWasmTransferWithinHostCap(cap - 1, cap));
  }

  const runtimeSource = readFileSync(
    new URL('../../../apps/clearra-web/src/workers/clearraWasmRuntime.ts', import.meta.url),
    'utf8'
  );
  const guard = runtimeSource.indexOf(
    'assertWasmTransferWithinHostCap(input.byteLength, hostTransferByteCap)'
  );
  const resize = runtimeSource.indexOf('clearra_wasm_transfer_resize(input.byteLength)', guard);
  assert.ok(guard >= 0 && resize > guard, 'host cap rejects before WASM resize/copy');
  for (const accessor of [
    'clearra_wasm_distributed_worker_count_available',
    'clearra_wasm_distributed_worker_count_exact',
    'clearra_wasm_distributed_progress_available',
    'clearra_wasm_distributed_progress_geometry_nodes_exact',
    'clearra_wasm_distributed_progress_candidate_count_exact',
    'clearra_wasm_distributed_progress_candidate_family_count_exact',
    'clearra_wasm_distributed_progress_build_nodes_exact',
    'clearra_wasm_distributed_progress_coverage_checks_exact',
    'clearra_wasm_distributed_progress_pass_index_exact',
    'clearra_wasm_distributed_progress_pass_count_exact',
    'clearra_wasm_distributed_progress_layer_index_exact',
    'clearra_wasm_distributed_progress_layer_count_exact',
    'clearra_wasm_distributed_progress_layer_done_exact',
    'clearra_wasm_distributed_progress_layer_total_exact',
    'clearra_wasm_tiling_solution_count_available',
    'clearra_wasm_tiling_solution_count_exact',
    'clearra_wasm_distributed_verifier_last_candidate_count_available',
    'clearra_wasm_distributed_verifier_last_candidate_count_exact',
    'clearra_wasm_distributed_verifier_progress_available',
    'clearra_wasm_distributed_verifier_progress_candidate_count_exact',
    'clearra_wasm_distributed_verifier_progress_build_nodes_exact',
    'clearra_wasm_distributed_verifier_progress_coverage_checks_exact',
    'clearra_wasm_output_len_exact',
    'clearra_wasm_last_panic_len_exact'
  ]) {
    assert.match(runtimeSource, new RegExp(`\\b${accessor}\\b`, 'u'), accessor);
  }
});

test('build probability primary metric is explicitly oracle and distinct from finesse knowledge', () => {
  assert.deepEqual(production.BUILD_PROBABILITY_PRIMARY_METRIC, {
    id: 'full-future-oracle-build-probability',
    futureVisibility: 'full-future',
    queueKnowledge: 'oracle',
    distinctFrom: 'finesse-pattern-knowledge'
  });
  assert.equal(
    production.workspaceMessage('en', 'oracleBuildProbability'),
    'Full-future/oracle build probability'
  );
  assert.equal(
    production.workspaceMessage('ko', 'oracleBuildProbability'),
    '전체 미래/오라클 구축 확률'
  );
  const resultSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityResult.svelte', import.meta.url),
    'utf8'
  );
  const workspaceSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityWorkspace.svelte', import.meta.url),
    'utf8'
  );
  assert.match(resultSource, /data-metric-id=\{BUILD_PROBABILITY_PRIMARY_METRIC\.id\}/u);
  assert.match(resultSource, /label\('oracleBuildProbability'\)/u);
  assert.match(workspaceSource, /Exact full-future\/oracle build probability workspace/u);
});

function snapshotForMemory(reportedDeviceMemoryGiB) {
  return production.createHostCapabilitySnapshot({
    snapshotId: `memory-${String(reportedDeviceMemoryGiB)}`,
    source: 'host-provided',
    reportedLogicalProcessors: 4,
    reportedDeviceMemoryGiB
  });
}

class FakeWorker {
  constructor(hardwareConcurrency) {
    this.hardwareConcurrency = hardwareConcurrency;
  }

  messages = [];
  onmessage = null;
  onerror = null;
  onmessageerror = null;

  postMessage(message) {
    this.messages.push(message);
  }

  terminate() {}
}
