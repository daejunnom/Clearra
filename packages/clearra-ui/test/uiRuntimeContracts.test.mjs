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
        wasmProductRetentionByteCap,
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
        projectWorkspacePublicFailure,
        workspacePublicFailureMessage
      } from './src/lib/workspace/workspacePublicFailure.ts';
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

test('score-minimals UI exposes the closed mode and pages every exact portfolio tie', () => {
  const controls = readFileSync(
    new URL('../src/lib/workspace/SearchControls.svelte', import.meta.url),
    'utf8'
  );
  const standalone = readFileSync(
    new URL('../src/lib/workspace/PcSolverStandalone.svelte', import.meta.url),
    'utf8'
  );
  const pager = readFileSync(
    new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url),
    'utf8'
  );
  const pagerModel = readFileSync(
    new URL('../src/lib/workspace/productResultPager.ts', import.meta.url),
    'utf8'
  );

  assert.match(controls, /<option value="score-minimals">/u);
  assert.match(standalone, /scoreMode: 'score-minimals'/u);
  assert.match(pager, /payload\?\.contract === 'pc\.score-minimals'/u);
  assert.match(pager, /Equality, membership, and ordering use score only/u);
  assert.match(pager, /async function nextOuterPage\(\)/u);
  assert.match(pager, /async function showMemberPage\(nextMemberPage/u);
  assert.match(pager, /loadNextPage/u);
  assert.match(pager, /loadMemberPage/u);
  assert.match(pager, /CoveragePortfolioPagerController/u);
  assert.match(pagerModel, /const MAX_RETAINED_PORTFOLIO_PAGES = 2/u);
  assert.match(pagerModel, /this\.pages\.slice\(1\)/u);
  assert.match(pagerModel, /validateCoveragePortfolioRuntimePage/u);
  assert.match(pager, /const MAX_RETAINED_MEMBER_PAGES = 3/u);
  assert.match(pager, /pruneMemberCache\(page\.alternative_index, nextMemberPage\)/u);
});

test('PC path and score-finder are reachable on both Web PC entry surfaces', () => {
  const controls = readFileSync(
    new URL('../src/lib/workspace/SearchControls.svelte', import.meta.url),
    'utf8'
  );
  const standalone = readFileSync(
    new URL('../src/lib/workspace/PcSolverStandalone.svelte', import.meta.url),
    'utf8'
  );
  const pager = readFileSync(
    new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url),
    'utf8'
  );

  assert.match(controls, /<option value="path">/u);
  assert.match(controls, /<option value="score-finder">/u);
  assert.match(standalone, /scoreMode: 'path'/u);
  assert.match(standalone, /scoreMode: 'score-finder'/u);
  assert.match(pager, /payload\.content\.payload_kind === 'pc-path-family'/u);
  assert.match(pager, /pathPageCount = lazyReplayPage\?\.geometry_count \?\? String\(pathCandidateGroups\.length\)/u);
  assert.match(pager, /pathCandidateGroup = pathCandidateGroups\[lazyReplayPage \? 0 : pathPageIndex\] \?\? null/u);
  assert.match(pager, /loadPcReplayPage\(/u);
  assert.match(pager, /collectPcReplayGeometryExportPages\(/u);
  assert.match(pager, /Every path can be copied; one representative replay is shown for each solution/u);
  assert.match(pager, /Inspect representative replay steps/u);
  assert.match(pager, /<PcPathReplayGif/u);
  assert.match(pager, /pathCandidateGroup\.distinctPatternCount/u);
  assert.match(pager, /pathCandidateGroup\.witnessCount/u);
  assert.match(pager, /pcPathCandidateGroupExportPages\([\s\S]*?pathCandidateGroup,[\s\S]*?targetLines,[\s\S]*?target_terminal_board_mask,[\s\S]*?mirrored_terminal_board_mask/u);
  assert.match(pager, /expectedTerminalBoardMask=\{buildPathFamily \? witness\.steps\.at\(-1\)\?\.board_after_line_clear_mask/u);
  assert.doesNotMatch(pager, /\{pathFamily\.problem_id\}|\{witness\.pattern_id\}|\{step\.line_clear_identity\}/u);
});

test('workspace failure projection keeps raw identifiers in developer evidence only', () => {
  const sentinel =
    'ctk1|initial=deadbeef candidate_id=81 pattern_id=7 problem_id=private trace_identity=raw';
  const projection = production.projectWorkspacePublicFailure({
    status: 'failed',
    responseStatus: 'execution-failed',
    error: sentinel,
    diagnostics: [{
      code: 'E_SCHEMA_IDENTITY_MISMATCH_candidate_id_81',
      severity: 'error',
      message: sentinel
    }]
  });

  assert.equal(projection.developerEvidence.error, sentinel);
  assert.equal(projection.developerEvidence.diagnostics[0].message, sentinel);
  assert.ok(projection.publicFailures.length > 0);
  const publicText = ['en', 'ko']
    .flatMap((language) => projection.publicFailures.map((failure) =>
      production.workspacePublicFailureMessage(language, failure)
    ))
    .join('\n');
  for (const privateToken of [
    'ctk1',
    'candidate_id',
    'pattern_id',
    'problem_id',
    'trace_identity',
    'E_SCHEMA_IDENTITY_MISMATCH'
  ]) {
    assert.doesNotMatch(publicText, new RegExp(privateToken, 'iu'));
  }

  const runtime = readFileSync(
    new URL('../src/lib/workspace/workspaceRuntime.ts', import.meta.url),
    'utf8'
  );
  assert.match(runtime, /projectWorkspacePublicFailure\(\{/u);
  assert.match(runtime, /publicFailures: failure\.publicFailures/u);
  assert.match(runtime, /developerDiagnostics: failure\.developerEvidence\.diagnostics/u);
  assert.match(runtime, /developerError: failure\.developerEvidence\.error/u);
  assert.doesNotMatch(
    runtime,
    /(?:publicFailures|developerDiagnostics|developerError):\s*state\.(?:diagnostics|error)/u
  );
});

test('generic runtime wrappers expose stable failure categories without private evidence', () => {
  for (const [message, expected] of [
    ['heartbeat lease expired candidate_id=81', 'worker-lease-expired'],
    ['RuntimeError: unreachable ctk1|private', 'runtime-trap'],
    ['memory access out of bounds pattern_id=7', 'runtime-trap'],
    ['whole-live budget exceeded problem_id=private', 'resource-limit'],
    ['Build replay does not terminate at the requested cleared field', 'result-invalid']
  ]) {
    const projected = production.projectWorkspacePublicFailure({
      status: 'failed', responseStatus: 'execution-failed',
      diagnostics: [{ code: 'generic-error', severity: 'error', message }]
    });
    assert.ok(projected.publicFailures.some((failure) => failure.code === expected));
    for (const language of ['en', 'ko']) {
      for (const failure of projected.publicFailures) {
        assert.doesNotMatch(production.workspacePublicFailureMessage(language, failure),
          /ctk1|candidate_id|pattern_id|problem_id/u);
      }
    }
  }
});

test('workspace shows one concrete replay failure and retains every wrapping diagnostic', () => {
  const diagnostics = [
    { code: 'I_PC_QUERY_MVP_SUPPORTED', severity: 'info', message: 'PC query is supported' },
    { code: 'E_APP_UNSUPPORTED', severity: 'error', message: 'requested execution runtime is unsupported' },
    { code: 'E_APP_EXECUTION_FAILED', severity: 'error', message: 'execution did not complete' },
    {
      code: 'E_APP_UNSUPPORTED', severity: 'error',
      message: 'requested execution runtime is unsupported: complete_replay_whole_live_limit_exceeded'
    }
  ];
  const projected = production.projectWorkspacePublicFailure({
    status: 'failed', responseStatus: 'unsupported', diagnostics
  });
  assert.deepEqual(projected.publicFailures, [{ code: 'resource-limit', severity: 'error' }]);
  assert.deepEqual(projected.developerEvidence.diagnostics, diagnostics);

  const unsupported = production.projectWorkspacePublicFailure({
    status: 'failed', responseStatus: 'unsupported', diagnostics: diagnostics.slice(0, 2)
  });
  assert.deepEqual(unsupported.publicFailures, [{ code: 'unsupported', severity: 'error' }]);

  const warning = production.projectWorkspacePublicFailure({
    status: 'failed', responseStatus: 'execution-failed',
    diagnostics: [{ code: 'W_MEMORY_BUDGET', severity: 'warning', message: 'unrelated warning' }]
  });
  assert.deepEqual(warning.publicFailures, [{ code: 'execution-failed', severity: 'error' }]);

  const supported = production.projectWorkspacePublicFailure({
    status: 'completed', responseStatus: 'success', diagnostics: diagnostics.slice(0, 1)
  });
  assert.deepEqual(supported.publicFailures, []);
  assert.deepEqual(supported.developerEvidence.diagnostics, diagnostics.slice(0, 1));
});

test('normal workspace failure DOM surfaces use the shared public notice only', () => {
  for (const file of [
    'ResultWorkspaceFrame.svelte',
    'ProductResultPager.svelte',
    'ProductFamilyResult.svelte',
    'BuildV2Result.svelte',
    'PcSolverResult.svelte',
    'SetupFinderResult.svelte',
    'OperationSequenceWorkspace.svelte',
    'SequenceDependenciesWorkspace.svelte',
    'DocumentUtilityWorkspace.svelte',
    'CtkDrawerWorkspace.svelte'
  ]) {
    const source = readFileSync(
      new URL(`../src/lib/workspace/${file}`, import.meta.url),
      'utf8'
    );
    assert.match(source, /WorkspaceFailureNotice/u, file);
    assert.doesNotMatch(
      source,
      /\b(?:view|runtimeView|renderRuntimeView)\.(?:error|diagnostics)\b|diagnostic\.(?:code|message)/u,
      file
    );
  }

  const setupWorkspace = readFileSync(
    new URL('../src/lib/workspace/SetupFinderWorkspace.svelte', import.meta.url),
    'utf8'
  );
  assert.match(setupWorkspace, /developerFailure: projection\.developerEvidence/u);
  assert.doesNotMatch(
    setupWorkspace,
    /diagnostics\.map\(\(diagnostic\) => diagnostic\.message\)|event\.message\s*\|\|\s*label/u
  );

  const pager = readFileSync(
    new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url),
    'utf8'
  );
  assert.match(pager, /projectWorkspacePublicFailure\(\{/u);
  assert.doesNotMatch(pager, />\{error\}</u);

  const replay = readFileSync(
    new URL('../src/lib/workspace/PcPathReplayGif.svelte', import.meta.url),
    'utf8'
  );
  assert.match(replay, /\{:else if renderError\}[\s\S]*?\{invalidLabel\}/u);
  assert.doesNotMatch(replay, /title=\{renderError\}|>\{renderError\}</u);
});

test('Build result UI separates ordinary families from exact portfolio paging', () => {
  const pager = readFileSync(
    new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url),
    'utf8'
  );

  assert.match(pager, /payload_kind === 'build-v2'/u);
  assert.match(pager, /payload_kind === 'build-coverage-portfolio-v2'/u);
  assert.match(pager, /payload_kind === 'build-setup-family-v1'/u);
  assert.match(pager, /loadInitialBuildPortfolioPage/u);
  const initialBuildLoaderStart = pager.indexOf('async function loadInitialBuildPortfolioPage');
  const initialBuildLoaderEnd = pager.indexOf('\n  function releaseHandle', initialBuildLoaderStart);
  const initialBuildLoader = pager.slice(initialBuildLoaderStart, initialBuildLoaderEnd);
  assert.ok(initialBuildLoaderStart >= 0 && initialBuildLoaderEnd > initialBuildLoaderStart);
  assert.match(initialBuildLoader, /await loadCoveragePortfolioExactPage\(/u);
  assert.match(initialBuildLoader, /isCurrent: \(\) => activeIdentity === payloadIdentity/u);
  assert.match(pager, /All optimal Build portfolios/u);
  assert.match(pager, /ordinary result family, not a portfolio tie/u);
  assert.match(pager, /Attack is informational/u);
  assert.match(pager, /Previous score evidence/u);
});

test('Build v2 workspace owns all surfaces without collapsing nominal document sources', () => {
  const workspace = readFileSync(
    new URL('../src/lib/workspace/BuildV2Workspace.svelte', import.meta.url),
    'utf8'
  );
  const source = readFileSync(
    new URL('../src/lib/workspace/BuildV2SourceEditor.svelte', import.meta.url),
    'utf8'
  );
  const controls = readFileSync(
    new URL('../src/lib/workspace/BuildV2Controls.svelte', import.meta.url),
    'utf8'
  );
  const webRoute = readFileSync(
    new URL('../../../apps/clearra-web/src/routes/+page.svelte', import.meta.url),
    'utf8'
  );
  const desktopRoute = readFileSync(
    new URL('../../../apps/clearra-desktop/src/routes/+page.svelte', import.meta.url),
    'utf8'
  );

  assert.match(workspace, /buildV2Command\(executionRequest\)/u);
  assert.match(workspace, /buildV2RequestForDesktop/u);
  assert.match(workspace, /loadNextDesktopProductPage/u);
  assert.match(workspace, /loadProductMemberPage/u);
  assert.match(source, /colored target document remains a producer input/u);
  assert.match(source, /never reinterpreted as a target/u);
  assert.match(controls, /Equality, selection, and ordering use score only/u);
  assert.match(controls, /Memory options remain unavailable/u);
  assert.match(webRoute, /selectedTool === 'build'/u);
  assert.match(desktopRoute, /selectedTool === 'build'/u);
});

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

test('root worker runtime imports stay outside the component-bearing WASM barrel', () => {
  const packageMetadata = JSON.parse(
    readFileSync(new URL('../package.json', import.meta.url), 'utf8')
  );
  const rootWorkerSource = readFileSync(
    new URL('../../../apps/clearra-web/src/workers/clearraWorker.ts', import.meta.url),
    'utf8'
  );
  const wasmBarrelSource = readFileSync(
    new URL('../src/lib/wasm/index.ts', import.meta.url),
    'utf8'
  );

  assert.equal(
    packageMetadata.exports['./wasm-host'],
    './src/lib/wasm/hostCapabilitySnapshot.ts'
  );
  assert.match(wasmBarrelSource, /WasmTerminalShell\.svelte/u);
  assert.match(
    rootWorkerSource,
    /import\s*\{[^;]*createHostCapabilitySnapshot[^;]*\}\s*from '@clearra\/ui\/wasm-host';/u
  );
  assert.doesNotMatch(
    rootWorkerSource,
    /import\s*\{[^;]*\}\s*from '@clearra\/ui\/wasm';/u
  );
  for (const file of [
    'clearraWorker.ts',
    'WasmJobRunner.ts',
    'ClearraProductJobRunner.ts',
    'DistributedWasmJobRunner.ts',
    'clearraWasmRuntime.ts'
  ]) {
    const source = readFileSync(
      new URL(`../../../apps/clearra-web/src/workers/${file}`, import.meta.url),
      'utf8'
    );
    const barrelImports = source.match(
      /import[^;]*from '@clearra\/ui\/wasm';/gu
    ) ?? [];
    for (const declaration of barrelImports) {
      assert.match(declaration, /^import\s+type\b/u, file);
    }
  }
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
    'clearra_wasm_output_release',
    'clearra_wasm_last_panic_len_exact'
  ]) {
    assert.match(runtimeSource, new RegExp(`\\b${accessor}\\b`, 'u'), accessor);
  }
});

test('product-retention budget is bounded independently of transfer and worker count', () => {
  for (const [memoryGiB, expectedMiB] of [[undefined, 64], [0.5, 32], [1, 64], [4, 256], [8, 512], [64, 512]]) {
    const snapshot = snapshotForMemory(memoryGiB);
    assert.equal(production.wasmProductRetentionByteCap(snapshot), expectedMiB * MIB);
    assert.equal(production.wasmProductRetentionByteCap({ ...snapshot, reportedLogicalProcessors: 128 }), expectedMiB * MIB);
  }
  assert.equal(snapshotForMemory(4).wasmTransferByteCap, 128 * MIB);
  assert.equal(production.wasmProductRetentionByteCap(snapshotForMemory(4)), 256 * MIB);
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
