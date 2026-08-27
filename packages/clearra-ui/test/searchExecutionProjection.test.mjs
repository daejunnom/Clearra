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
        buildProbabilityCommand,
        buildProbabilityRequestForDesktop,
        buildProbabilityValidationCodes,
        createDefaultBuildProbabilityRequest,
        normalizeBuildProbabilityRequest
      } from './src/lib/workspace/buildProbabilityModel.ts';
      export {
        buildWorkspaceCommand,
        createDefaultWorkspaceRequest,
        normalizeWorkspaceRequest,
        workspaceRequestForDesktop,
        workspaceValidationCodes
      } from './src/lib/workspace/solverWorkspaceModel.ts';
      export {
        searchExecutionCommandArguments,
        searchExecutionDesktopFields
      } from './src/lib/workspace/searchExecutionModel.ts';
      export {
        createDefaultForwardSearchRequest,
        forwardSearchRequestForDesktop,
        spinCategoryOptions
      } from './src/lib/workspace/forwardSearchModel.ts';
      export {
        createDefaultSetupFinderRequest,
        setupFinderRequestForDesktop
      } from './src/lib/workspace/setupFinderModel.ts';
      export { buildDesktopAppRequest } from './src/lib/host/clearraDesktopHost.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const contractRows = readFileSync(
  new URL('../../../tests/fixtures/contracts/search_option_contract.tsv', import.meta.url),
  'utf8'
)
  .split(/\r?\n/u)
  .filter((line) => line && !line.startsWith('#'))
  .map((line) => {
    const columns = line.split('\t');
    assert.equal(columns.length, 13, `contract column count: ${line}`);
    return {
      family: columns[0],
      option: columns[1],
      kind: columns[2],
      valid: columns[3].split('|'),
      invalid: columns[4].split('|'),
      discordDefault: columns[5],
      nativeDefault: columns[6],
      disposition: columns[7],
      discordPath: columns[8],
      exposure: columns[9],
      lowering: columns[10],
      reason: columns[11],
      dependencies: columns[12]
    };
  });

function contractRow(family, option) {
  const row = contractRows.find((candidate) =>
    candidate.family === family && candidate.option === option
  );
  assert.ok(row, `missing shared contract row ${family}.${option}`);
  return row;
}

function commandTokens(command) {
  return command.split(/\s+/u);
}

function hasOption(tokens, option) {
  return tokens.includes(option);
}

function optionValue(tokens, option) {
  const index = tokens.indexOf(option);
  assert.notEqual(index, -1, `missing ${option}`);
  return tokens[index + 1];
}

const BUILD_TILING_RISKS = [
  {
    option: 'preserve-b2b',
    representatives: ['on'],
    apply: (request) => ({ ...request, preserveB2B: true })
  },
  {
    option: 'dependency-dag',
    representatives: ['on'],
    apply: (request) => ({ ...request, precomputeBuildDependencies: true })
  },
  {
    option: 'finesse',
    representatives: ['inputs'],
    apply: (request, value) => ({ ...request, finesse: value })
  },
  {
    option: 'rule',
    representatives: ['srs-plus', 'srs', 'srs-x', 'jstris-180'],
    apply: (request, value) => ({ ...request, rule: value })
  },
  {
    option: 'spin-profile',
    representatives: [
      't-spins',
      't-spins-plus',
      'all-spin',
      'all-spin-plus',
      'all-mini',
      'all-mini-plus'
    ],
    apply: (request, value) => ({ ...request, spinProfile: value })
  },
  {
    option: 'pattern-knowledge',
    representatives: ['both', 'oracle', 'visible-7'],
    apply: (request, value) => ({ ...request, patternKnowledge: value })
  }
];

const PC_TILING_RISKS = [
  {
    option: 'queue-knowledge',
    representatives: ['visible-7'],
    apply: (request, value) => ({ ...request, queueKnowledge: value })
  },
  {
    option: 'rule',
    representatives: ['srs-plus', 'srs', 'srs-x', 'jstris-180'],
    apply: (request, value) => ({ ...request, rule: value })
  },
  {
    option: 'spin-profile',
    representatives: [
      't-spins',
      't-spins-plus',
      'all-spin',
      'all-spin-plus',
      'all-mini',
      'all-mini-plus'
    ],
    apply: (request, value) => ({ ...request, spinProfile: value })
  },
  {
    option: 'preserve-b2b',
    representatives: ['on'],
    apply: (request) => ({ ...request, preserveB2B: true })
  },
  {
    option: 'initial-b2b',
    representatives: ['1', '65535'],
    apply: (request, value) => ({ ...request, initialB2B: Number(value) })
  },
  {
    option: 'solution-probabilities',
    representatives: ['on'],
    apply: (request) => ({ ...request, solutionProbabilities: true })
  },
  {
    option: 'tablebase',
    representatives: ['on'],
    apply: (request) => ({ ...request, tablebaseEnabled: true })
  },
  {
    option: 'dependency-dag',
    representatives: ['on'],
    apply: (request) => ({ ...request, precomputeBuildDependencies: true })
  }
];

function assertRiskInventory(family, risks) {
  for (const risk of risks) {
    const row = contractRow(family, risk.option);
    for (const representative of risk.representatives) {
      assert.ok(
        row.valid.includes(representative),
        `${family}.${risk.option} representative ${representative} must come from the fixture`
      );
    }
  }
}

function buildTilingBase() {
  return {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 4,
    targetMask: 0xfn,
    queue: 'I',
    aggregation: 'tiling'
  };
}

function pcTilingBase() {
  return {
    ...production.createDefaultWorkspaceRequest(),
    lines: 4,
    boardMask: 0xfn,
    queue: 'IOTSZLJIO',
    scoreMode: 'tiling'
  };
}

function assertBuildTilingProjection(request, label) {
  const normalized = production.normalizeBuildProbabilityRequest(request);
  assert.equal(normalized.aggregation, 'tiling', label);
  assert.equal(normalized.rule, 'srs-plus', label);
  assert.equal(normalized.spinProfile, 't-spins', label);
  assert.equal(normalized.preserveB2B, false, label);
  assert.equal(normalized.precomputeBuildDependencies, false, label);
  assert.equal(normalized.finesse, 'off', label);
  assert.equal(normalized.patternKnowledge, 'both', label);

  const command = production.buildProbabilityCommand(request);
  const tokens = commandTokens(command);
  assert.ok(hasOption(tokens, '--tiling-only'), label);
  for (const option of [
    '--aggregate',
    '--rule',
    '--spin-profile',
    '--preserve-b2b',
    '--build-dependency-dag',
    '--no-build-dependency-dag',
    '--finesse',
    '--pattern-knowledge'
  ]) {
    assert.equal(hasOption(tokens, option), false, `${label}: leaked ${option}`);
  }
  assert.match(command, /--backend cpu --no-backend-fallback/u, label);

  const desktop = production.buildProbabilityRequestForDesktop(request, 'en');
  assert.equal(desktop.build_aggregation, 'tiling', label);
  assert.equal(desktop.rule, 'srs-plus', label);
  assert.equal(desktop.spin_profile, 't-spins', label);
  assert.equal(desktop.preserve_b2b, false, label);
  assert.equal(desktop.precompute_build_dependencies, false, label);
  assert.equal(desktop.finesse, 'off', label);
  assert.equal(desktop.pattern_knowledge, 'both', label);
  assert.equal(desktop.backend, 'cpu', label);
  assert.equal(desktop.allow_backend_fallback, false, label);
}

function assertPcTilingProjection(request, label) {
  const normalized = production.normalizeWorkspaceRequest(request);
  assert.equal(normalized.scoreMode, 'tiling', label);
  assert.equal(normalized.queueKnowledge, 'oracle', label);
  assert.equal(normalized.rule, 'srs-plus', label);
  assert.equal(normalized.scoreProfile, 'tetrio', label);
  assert.equal(normalized.spinProfile, 't-spins', label);
  assert.equal(normalized.preserveB2B, false, label);
  assert.equal(normalized.initialB2B, 0, label);
  assert.equal(normalized.solutionProbabilities, false, label);
  assert.equal(normalized.tablebaseEnabled, false, label);
  assert.equal(normalized.precomputeBuildDependencies, false, label);

  const command = production.buildWorkspaceCommand(request);
  const tokens = commandTokens(command);
  assert.deepEqual(tokens.slice(0, 3), ['clearra', 'pc', 'tiling'], label);
  assert.equal(hasOption(tokens, '--tiling-only'), false, label);
  for (const option of [
    '--count',
    '--rule',
    '--score',
    '--score-profile',
    '--spin-profile',
    '--preserve-b2b',
    '--initial-b2b',
    '--solution-probabilities',
    '--queue-knowledge',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag'
  ]) {
    assert.equal(hasOption(tokens, option), false, `${label}: leaked ${option}`);
  }

  const desktop = production.workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.score_mode, 'tiling', label);
  assert.equal(desktop.queue_knowledge, 'oracle', label);
  assert.equal(desktop.rule, 'srs-plus', label);
  assert.equal(desktop.score_profile, 'tetrio', label);
  assert.equal(desktop.spin_profile, 't-spins', label);
  assert.equal(desktop.preserve_b2b, false, label);
  assert.equal(desktop.initial_b2b, 0, label);
  assert.equal(desktop.solution_probabilities, false, label);
  assert.equal(desktop.precompute_build_dependencies, false, label);
}

function applyRisk(request, risk, representative) {
  return risk.apply(request, representative);
}

test('shared fixture defaults remain explicit beside the actual Web/Desktop projections', () => {
  assert.equal(contractRow('pc', 'lines').discordDefault, 'auto');
  assert.equal(contractRow('pc', 'lines').nativeDefault, '2');
  assert.equal(contractRow('pc', 'backend').discordDefault, 'auto');
  assert.equal(contractRow('pc', 'fallback').discordDefault, 'default');
  assert.equal(contractRow('build', 'backend').discordDefault, 'cpu');
  assert.equal(contractRow('build', 'fallback').discordDefault, 'deny');
  assert.equal(contractRow('build', 'aggregation').disposition, 'named');
  assert.equal(
    contractRow('build', 'aggregation').lowering,
    '--aggregate|--tiling-only'
  );

  const pc = {
    ...production.createDefaultWorkspaceRequest(),
    queue: 'IOTSZLJIO'
  };
  const pcCommand = production.buildWorkspaceCommand(pc);
  assert.equal(pc.lines, 4);
  assert.match(pcCommand, /--lines 4/u);
  assert.match(pcCommand, /--count unique/u);
  assert.match(pcCommand, /--backend auto --allow-backend-fallback/u);
  const pcDesktop = production.workspaceRequestForDesktop(pc, 'en');
  assert.equal(pcDesktop.lines, 4);
  assert.equal(pcDesktop.count_policy, 'unique');
  assert.equal(pcDesktop.backend, 'auto');
  assert.equal(pcDesktop.allow_backend_fallback, true);
  assert.equal(Number(optionValue(commandTokens(pcCommand), '--workers')), pcDesktop.workers);

  const buildRequest = {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 4,
    targetMask: 0xfn,
    queue: 'I'
  };
  const buildCommand = production.buildProbabilityCommand(buildRequest);
  assert.match(buildCommand, /--aggregate buildability/u);
  assert.match(buildCommand, /--backend cpu --no-backend-fallback/u);
  const buildDesktop = production.buildProbabilityRequestForDesktop(buildRequest, 'en');
  assert.equal(buildDesktop.build_aggregation, 'buildability');
  assert.equal(buildDesktop.backend, 'cpu');
  assert.equal(buildDesktop.allow_backend_fallback, false);
  assert.equal(
    Number(optionValue(commandTokens(buildCommand), '--workers')),
    buildDesktop.workers
  );
});

test('fixture backend by fallback Cartesian representatives preserve argv and typed parity', () => {
  const backends = contractRow('pc', 'backend').valid;
  const fallbackRepresentatives = contractRow('pc', 'fallback').valid;
  assert.deepEqual(backends, ['auto', 'cpu', 'gpu', 'hybrid']);
  assert.deepEqual(fallbackRepresentatives, ['default', 'allow', 'deny']);

  for (const backend of backends) {
    for (const fallback of fallbackRepresentatives) {
      const allowBackendFallback = fallback === 'default'
        ? backend === 'auto'
        : fallback === 'allow';
      const request = {
        backend,
        gpuDevice: backend === 'cpu' ? '3' : '2',
        workers: 2,
        useAllLogicalProcessors: false,
        allowBackendFallback,
        cpuWarmup: true,
        gpuWarmup: true
      };
      const tokens = production.searchExecutionCommandArguments(request);
      const desktop = production.searchExecutionDesktopFields(request);
      assert.deepEqual(tokens.slice(0, 2), ['--backend', backend], `${backend}/${fallback}`);
      assert.equal(
        hasOption(tokens, allowBackendFallback
          ? '--allow-backend-fallback'
          : '--no-backend-fallback'),
        true,
        `${backend}/${fallback}`
      );
      assert.equal(desktop.backend, backend, `${backend}/${fallback}`);
      assert.equal(desktop.allow_backend_fallback, allowBackendFallback, `${backend}/${fallback}`);
      assert.equal(Number(optionValue(tokens, '--workers')), desktop.workers, `${backend}/${fallback}`);
      assert.equal(desktop.gpu_device, backend === 'cpu' ? 'auto' : '2', `${backend}/${fallback}`);
      assert.equal(hasOption(tokens, '--gpu-warmup'), backend !== 'cpu', `${backend}/${fallback}`);
    }

    const pc = {
      ...production.createDefaultWorkspaceRequest(),
      queue: 'IOTSZLJIO',
      backend,
      gpuDevice: backend === 'cpu' ? '3' : '2'
    };
    const expectedAllow = backend === 'auto';
    const command = production.buildWorkspaceCommand(pc);
    const desktop = production.workspaceRequestForDesktop(pc, 'en');
    assert.equal(
      hasOption(commandTokens(command), expectedAllow
        ? '--allow-backend-fallback'
        : '--no-backend-fallback'),
      true,
      `PC ${backend}`
    );
    assert.equal(desktop.allow_backend_fallback, expectedAllow, `PC ${backend}`);
    assert.equal(desktop.gpu_device, backend === 'cpu' ? 'auto' : '2', `PC ${backend}`);
    assert.equal(
      Number(optionValue(commandTokens(command), '--workers')),
      desktop.workers,
      `PC ${backend}`
    );
  }
});

test('PC scenario validation preserves odd-height fields and enforces the documented B2B bound', () => {
  const oddScenario = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3n,
    queue: 'IO'
  };
  assert.equal(
    production.workspaceValidationCodes(oddScenario, 'web').includes('target_lines_invalid'),
    false,
    contractRow('pc', 'lines').dependencies
  );

  const maximum = { ...oddScenario, initialB2B: 0xffff };
  const overflow = { ...oddScenario, initialB2B: 0x10000 };
  assert.equal(
    production.workspaceValidationCodes(maximum, 'web').includes('initial_b2b_invalid'),
    false
  );
  assert.equal(
    production.workspaceValidationCodes(overflow, 'web').includes('initial_b2b_invalid'),
    true
  );
});

test('desktop request helper defaults fallback from the final backend instead of a global true', () => {
  for (const backend of ['auto', 'cpu', 'gpu', 'hybrid']) {
    const request = production.buildDesktopAppRequest({ backend });
    assert.equal(request.backend, backend);
    assert.equal(request.allow_backend_fallback, backend === 'auto', backend);
  }
  assert.equal(
    production.buildDesktopAppRequest({ backend: 'gpu', allow_backend_fallback: true })
      .allow_backend_fallback,
    true
  );
});

test('Desktop forward projection is command-discriminated and preserves normalized workers', () => {
  const stale = {
    ...production.createDefaultForwardSearchRequest('damage'),
    queue: 'TIO',
    initialCombo: 2,
    initialB2B: 3,
    damageAggregation: 'at-least',
    minimumDamage: 6,
    spinLines: '2+',
    spinCategory: 't'
  };
  const damage = JSON.parse(JSON.stringify(
    production.forwardSearchRequestForDesktop(stale, 'en', 11)
  ));
  assert.equal(damage.workers, 11);
  assert.equal(damage.initial_combo, 2);
  assert.equal(damage.initial_b2b, 3);
  assert.equal(damage.damage_aggregation, 'at-least');
  assert.equal(damage.minimum_damage, 6);
  for (const key of ['spin_lines', 'spin_category']) {
    assert.equal(Object.hasOwn(damage, key), false, `damage leaked ${key}`);
  }

  const spin = JSON.parse(JSON.stringify(
    production.forwardSearchRequestForDesktop(
      { ...stale, tool: 'spin-finder', queue: '[TIO]!' },
      'ko',
      12
    )
  ));
  assert.equal(spin.workers, 12);
  assert.equal(spin.spin_lines, '2+');
  assert.equal(spin.spin_category, 't');
  for (const key of [
    'initial_combo',
    'initial_b2b',
    'damage_aggregation',
    'minimum_damage'
  ]) {
    assert.equal(Object.hasOwn(spin, key), false, `spin-finder leaked ${key}`);
  }

  const ren = JSON.parse(JSON.stringify(
    production.forwardSearchRequestForDesktop(
      { ...stale, tool: 'ren', queue: 'TI', holdEnabled: false },
      'en',
      13
    )
  ));
  assert.equal(ren.command, 'ren');
  assert.equal(ren.queue, 'TI');
  assert.equal(ren.hold_enabled, false);
  assert.equal(ren.spin_profile, 'disabled');
  assert.equal(ren.preserve_b2b, false);
  assert.equal(ren.workers, 13);
  for (const key of [
    'initial_combo',
    'initial_b2b',
    'damage_aggregation',
    'minimum_damage',
    'spin_lines',
    'spin_category'
  ]) {
    assert.equal(Object.hasOwn(ren, key), false, `REN leaked ${key}`);
  }

  for (const command of ['setup', 'build-probability']) {
    const projected = JSON.parse(JSON.stringify(production.buildDesktopAppRequest({ command })));
    for (const key of [
      'initial_combo',
      'initial_b2b',
      'damage_aggregation',
      'minimum_damage',
      'spin_lines',
      'spin_category'
    ]) {
      assert.equal(Object.hasOwn(projected, key), false, `${command} leaked ${key}`);
    }
  }
});

test('forward spin category choices expose non-T spins for every all-piece profile', () => {
  for (const profile of ['all-spin', 'all-spin-plus', 'all-mini', 'all-mini-plus']) {
    assert.deepEqual(production.spinCategoryOptions(profile), ['any', 't', 'other'], profile);
  }
  for (const profile of ['t-spins', 't-spins-plus']) {
    assert.deepEqual(production.spinCategoryOptions(profile), ['any'], profile);
  }
});

test('Desktop setup search shares normalized workers while path detail keeps the host sentinel', () => {
  const request = {
    ...production.createDefaultSetupFinderRequest(),
    useAllLogicalProcessors: true
  };
  const search = production.setupFinderRequestForDesktop(request, 'en', 12);
  assert.equal(search.workers, 12);
  assert.equal(search.use_all_logical_processors, true);

  const detail = production.setupFinderRequestForDesktop(request, 'en', 12, {
    setupId: 'setup-1',
    conditionId: 'condition-1'
  });
  assert.equal(detail.workers, 0);
  assert.equal(detail.use_all_logical_processors, false);
});

test('PC score mode transitions canonicalize inactive Desktop profiles', () => {
  const scored = {
    ...production.createDefaultWorkspaceRequest(),
    queue: 'IOTSZJL',
    scoreMode: 'summary',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    initialB2B: 9,
    preserveB2B: false
  };
  for (const scoreMode of ['off', 'minimum-cover', 'failed-queue']) {
    const request = { ...scored, scoreMode };
    const normalized = production.normalizeWorkspaceRequest(request);
    assert.equal(normalized.scoreProfile, 'tetrio', scoreMode);
    assert.equal(normalized.spinProfile, 't-spins', scoreMode);
    assert.equal(normalized.initialB2B, 0, scoreMode);
    const desktop = production.workspaceRequestForDesktop(request, 'en');
    assert.equal(desktop.score_profile, 'tetrio', scoreMode);
    assert.equal(desktop.spin_profile, 't-spins', scoreMode);
    assert.equal(desktop.initial_b2b, 0, scoreMode);
    const tokens = commandTokens(production.buildWorkspaceCommand(request));
    for (const option of ['--score-profile', '--spin-profile', '--initial-b2b']) {
      assert.equal(hasOption(tokens, option), false, `${scoreMode} leaked ${option}`);
    }
  }
});

test('PC minimum-cover and score GUI modes lower only to their canonical product commands', () => {
  const base = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3fn,
    queue: 'I',
    holdEnabled: true,
    tablebaseEnabled: true,
    precomputeBuildDependencies: true
  };
  const minimals = { ...base, scoreMode: 'minimum-cover' };
  const minimalsTokens = commandTokens(production.buildWorkspaceCommand(minimals));
  assert.deepEqual(minimalsTokens.slice(0, 3), ['clearra', 'pc', 'minimals']);
  for (const option of [
    '--objective',
    '--count',
    '--score',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag',
    '--max-memory-mib'
  ]) {
    assert.equal(hasOption(minimalsTokens, option), false, `pc.minimals leaked ${option}`);
  }
  const minimalsDesktop = production.workspaceRequestForDesktop(minimals, 'en');
  assert.equal(minimalsDesktop.score_mode, 'minimum-cover');
  assert.equal(minimalsDesktop.count_policy, 'unique');
  assert.equal(minimalsDesktop.tablebase_requested, false);
  assert.equal(minimalsDesktop.precompute_build_dependencies, false);

  const score = {
    ...base,
    scoreMode: 'summary',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    initialB2B: 7,
    backend: 'hybrid',
    gpuDevice: '2',
    workers: 8,
    useAllLogicalProcessors: true,
    preserveB2B: true,
    solutionProbabilities: true,
    maxPatterns: 7
  };
  const scoreTokens = commandTokens(production.buildWorkspaceCommand(score));
  assert.deepEqual(scoreTokens.slice(0, 3), ['clearra', 'pc', 'score']);
  assert.equal(optionValue(scoreTokens, '--score-profile'), 'guideline');
  assert.equal(optionValue(scoreTokens, '--spin-profile'), 'all-mini-plus');
  assert.equal(optionValue(scoreTokens, '--initial-b2b'), '7');
  for (const option of [
    '--objective',
    '--count',
    '--score',
    '--backend',
    '--workers',
    '--use-all-cpu-threads',
    '--gpu-device',
    '--allow-backend-fallback',
    '--no-backend-fallback',
    '--solution-probabilities',
    '--preserve-b2b',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag',
    '--max-patterns'
  ]) {
    assert.equal(hasOption(scoreTokens, option), false, `pc.score leaked ${option}`);
  }
  const scoreDesktop = production.workspaceRequestForDesktop(score, 'en');
  assert.equal(scoreDesktop.score_mode, 'summary');
  assert.equal(scoreDesktop.backend, 'cpu');
  assert.equal(scoreDesktop.workers, 1);
  assert.equal(scoreDesktop.use_all_logical_processors, false);
  assert.equal(scoreDesktop.allow_backend_fallback, false);
  assert.equal(scoreDesktop.preserve_b2b, false);
  assert.equal(scoreDesktop.solution_probabilities, false);
});

test('PC path GUI mode lowers to the complete ordinary replay-family contract', () => {
  const request = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3f0n,
    queue: 'I',
    holdEnabled: true,
    scoreMode: 'path',
    queueKnowledge: 'visible-7',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    initialB2B: 7,
    preserveB2B: true,
    solutionProbabilities: true,
    tablebaseEnabled: true,
    precomputeBuildDependencies: true
  };
  const normalized = production.normalizeWorkspaceRequest(request);
  assert.equal(normalized.queueKnowledge, 'oracle');
  assert.equal(normalized.scoreProfile, 'tetrio');
  assert.equal(normalized.spinProfile, 't-spins');
  assert.equal(normalized.initialB2B, 0);
  assert.equal(normalized.preserveB2B, false);
  assert.equal(normalized.solutionProbabilities, false);
  assert.equal(normalized.tablebaseEnabled, false);
  assert.equal(normalized.precomputeBuildDependencies, false);

  const tokens = commandTokens(production.buildWorkspaceCommand(request));
  assert.deepEqual(tokens.slice(0, 3), ['clearra', 'pc', 'path']);
  assert.equal(optionValue(tokens, '--queue'), 'I');
  assert.equal(optionValue(tokens, '--rule'), request.rule);
  for (const option of [
    '--objective',
    '--count',
    '--score',
    '--score-profile',
    '--spin-profile',
    '--initial-b2b',
    '--solution-probabilities',
    '--preserve-b2b',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag'
  ]) {
    assert.equal(hasOption(tokens, option), false, `pc.path leaked ${option}`);
  }

  const desktop = production.workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.score_mode, 'path');
  assert.equal(desktop.count_policy, 'all');
  assert.equal(desktop.queue_knowledge, 'oracle');
  assert.equal(desktop.score_profile, 'tetrio');
  assert.equal(desktop.spin_profile, 't-spins');
  assert.equal(desktop.initial_b2b, 0);
  assert.equal(desktop.preserve_b2b, false);
  assert.equal(desktop.solution_probabilities, false);
  assert.equal(desktop.tablebase_requested, false);
  assert.equal(desktop.precompute_build_dependencies, false);
});

test('PC score-finder GUI mode owns one fixed queue and its fixed score policy', () => {
  const request = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3fn,
    queue: 'I',
    holdEnabled: true,
    queueKnowledge: 'visible-7',
    scoreMode: 'score-finder',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    initialB2B: 9,
    backend: 'hybrid',
    gpuDevice: '2',
    workers: 8,
    useAllLogicalProcessors: true,
    preserveB2B: true,
    solutionProbabilities: true,
    tablebaseEnabled: true,
    precomputeBuildDependencies: true,
    maxPatterns: 7
  };
  assert.deepEqual(production.workspaceValidationCodes(request, 'web'), []);
  const normalized = production.normalizeWorkspaceRequest(request);
  assert.equal(normalized.queueKnowledge, 'oracle');
  assert.equal(normalized.scoreProfile, 'jstris-ultra');
  assert.equal(normalized.spinProfile, 't-spins');
  assert.equal(normalized.initialB2B, 1);
  assert.equal(normalized.backend, 'cpu');
  assert.equal(normalized.gpuDevice, 'auto');
  assert.equal(normalized.workers, 1);
  assert.equal(normalized.useAllLogicalProcessors, false);
  assert.equal(normalized.preserveB2B, false);
  assert.equal(normalized.solutionProbabilities, false);
  assert.equal(normalized.tablebaseEnabled, false);
  assert.equal(normalized.precomputeBuildDependencies, false);
  assert.equal(normalized.maxPatterns, undefined);

  const tokens = commandTokens(production.buildWorkspaceCommand(request));
  assert.deepEqual(tokens.slice(0, 3), ['clearra', 'pc', 'score-finder']);
  assert.equal(optionValue(tokens, '--queue'), 'I');
  assert.equal(optionValue(tokens, '--rule'), request.rule);
  assert.equal(optionValue(tokens, '--initial-b2b'), '1');
  for (const option of [
    '--patterns',
    '--objective',
    '--count',
    '--score',
    '--score-profile',
    '--spin-profile',
    '--backend',
    '--workers',
    '--use-all-cpu-threads',
    '--gpu-device',
    '--allow-backend-fallback',
    '--no-backend-fallback',
    '--solution-probabilities',
    '--preserve-b2b',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag',
    '--max-patterns'
  ]) {
    assert.equal(hasOption(tokens, option), false, `pc.score-finder leaked ${option}`);
  }

  const desktop = production.workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.score_mode, 'score-finder');
  assert.equal(desktop.count_policy, 'all');
  assert.equal(desktop.queue, 'I');
  assert.equal(desktop.patterns, '');
  assert.equal(desktop.queue_knowledge, 'oracle');
  assert.equal(desktop.score_profile, 'jstris-ultra');
  assert.equal(desktop.spin_profile, 't-spins');
  assert.equal(desktop.initial_b2b, 1);
  assert.equal(desktop.backend, 'cpu');
  assert.equal(desktop.workers, 1);
  assert.equal(desktop.use_all_logical_processors, false);
  assert.equal(desktop.allow_backend_fallback, false);

  for (const queue of ['', 'P7', '[IOSZ]p2']) {
    const invalid = { ...request, queue };
    assert.ok(
      production
        .workspaceValidationCodes(invalid, 'web')
        .includes('pc-score-finder-fixed-queue-required'),
      queue || 'empty'
    );
  }
});

test('PC score-minimals GUI mode binds score-only minimum cover without runtime overrides', () => {
  const request = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3fn,
    queue: 'I',
    holdEnabled: true,
    queueKnowledge: 'visible-7',
    scoreMode: 'score-minimals',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    initialB2B: 7,
    backend: 'hybrid',
    gpuDevice: '2',
    workers: 8,
    useAllLogicalProcessors: true,
    preserveB2B: true,
    solutionProbabilities: true,
    tablebaseEnabled: true,
    precomputeBuildDependencies: true,
    maxPatterns: 7
  };
  assert.deepEqual(
    production.workspaceValidationCodes(request, 'web'),
    ['visible-seven-minimum-cover-unsupported']
  );
  const normalized = production.normalizeWorkspaceRequest(request);
  assert.equal(normalized.queueKnowledge, 'oracle');
  assert.equal(normalized.preserveB2B, false);
  assert.equal(normalized.solutionProbabilities, false);
  assert.equal(normalized.backend, 'cpu');
  assert.equal(normalized.gpuDevice, 'auto');
  assert.equal(normalized.workers, 1);
  assert.equal(normalized.useAllLogicalProcessors, false);
  assert.equal(normalized.tablebaseEnabled, false);
  assert.equal(normalized.precomputeBuildDependencies, false);
  assert.equal(normalized.maxPatterns, undefined);

  const tokens = commandTokens(production.buildWorkspaceCommand(request));
  assert.deepEqual(tokens.slice(0, 3), ['clearra', 'pc', 'score-minimals']);
  assert.equal(optionValue(tokens, '--score-profile'), 'guideline');
  assert.equal(optionValue(tokens, '--spin-profile'), 'all-mini-plus');
  assert.equal(optionValue(tokens, '--initial-b2b'), '7');
  for (const option of [
    '--objective',
    '--count',
    '--score',
    '--backend',
    '--workers',
    '--use-all-cpu-threads',
    '--gpu-device',
    '--allow-backend-fallback',
    '--no-backend-fallback',
    '--solution-probabilities',
    '--preserve-b2b',
    '--tablebase',
    '--no-tablebase',
    '--build-dependency-dag',
    '--no-build-dependency-dag',
    '--max-patterns'
  ]) {
    assert.equal(hasOption(tokens, option), false, `pc.score-minimals leaked ${option}`);
  }

  const desktop = production.workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.score_mode, 'score-minimals');
  assert.equal(desktop.count_policy, 'all');
  assert.equal(desktop.queue_knowledge, 'oracle');
  assert.equal(desktop.backend, 'cpu');
  assert.equal(desktop.workers, 1);
  assert.equal(desktop.use_all_logical_processors, false);
  assert.equal(desktop.allow_backend_fallback, false);
  assert.equal(desktop.preserve_b2b, false);
  assert.equal(desktop.solution_probabilities, false);
  assert.equal(desktop.pattern_budget, 5040);
});

test('PC save GUI modes lower to distinct canonical products without fixed-boundary overrides', () => {
  const base = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 2,
    boardMask: 0xf3fcfn,
    queue: 'P7',
    holdEnabled: false,
    queueKnowledge: 'visible-7',
    scoreProfile: 'guideline',
    spinProfile: 'all-mini-plus',
    preserveB2B: true,
    initialB2B: 9,
    solutionProbabilities: true,
    tablebaseEnabled: true,
    precomputeBuildDependencies: true
  };
  for (const [scoreMode, subcommand] of [
    ['saves', 'saves'],
    ['best-save', 'best-save']
  ]) {
    const request = { ...base, scoreMode };
    const normalized = production.normalizeWorkspaceRequest(request);
    assert.equal(normalized.queueKnowledge, 'oracle', scoreMode);
    assert.equal(normalized.preserveB2B, false, scoreMode);
    assert.equal(normalized.solutionProbabilities, false, scoreMode);
    assert.equal(normalized.tablebaseEnabled, false, scoreMode);
    assert.equal(normalized.precomputeBuildDependencies, false, scoreMode);

    const tokens = commandTokens(production.buildWorkspaceCommand(request));
    assert.deepEqual(tokens.slice(0, 3), ['clearra', 'pc', subcommand], scoreMode);
    assert.equal(optionValue(tokens, '--patterns'), 'P7', scoreMode);
    for (const option of [
      '--queue',
      '--objective',
      '--count',
      '--solution-probabilities',
      '--queue-knowledge',
      '--preserve-b2b',
      '--tablebase',
      '--no-tablebase',
      '--build-dependency-dag',
      '--no-build-dependency-dag',
      '--max-memory-mib'
    ]) {
      assert.equal(hasOption(tokens, option), false, `${scoreMode} leaked ${option}`);
    }

    const desktop = production.workspaceRequestForDesktop(request, 'en');
    assert.equal(desktop.score_mode, scoreMode);
    assert.equal(desktop.count_policy, 'all');
    assert.equal(desktop.queue, '');
    assert.equal(desktop.patterns, 'P7');
    assert.equal(desktop.queue_knowledge, 'oracle');
    assert.equal(desktop.preserve_b2b, false);
    assert.equal(desktop.solution_probabilities, false);
    assert.equal(desktop.memory_budget_mb, 0);
    assert.equal(desktop.tablebase_requested, false);
    assert.equal(desktop.precompute_build_dependencies, false);
  }
});

test('build mode transitions canonicalize inactive spin and finesse policies', () => {
  const prior = {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 1,
    targetMask: 0xfn,
    queue: 'I',
    aggregation: 'spin',
    spinProfile: 'all-mini-plus',
    finesse: 'inputs',
    patternKnowledge: 'visible-7',
    preserveB2B: false
  };
  const transitioned = {
    ...prior,
    aggregation: 'buildability',
    finesse: 'off'
  };
  const normalized = production.normalizeBuildProbabilityRequest(transitioned);
  assert.equal(normalized.spinProfile, 't-spins');
  assert.equal(normalized.patternKnowledge, 'both');
  const desktop = production.buildProbabilityRequestForDesktop(transitioned, 'en');
  assert.equal(desktop.spin_profile, 't-spins');
  assert.equal(desktop.finesse, 'off');
  assert.equal(desktop.pattern_knowledge, 'both');
  const tokens = commandTokens(production.buildProbabilityCommand(transitioned));
  assert.equal(hasOption(tokens, '--spin-profile'), false);
  assert.equal(hasOption(tokens, '--finesse'), false);
  assert.equal(hasOption(tokens, '--pattern-knowledge'), false);
});

test('empty PC and build sources normalize to the shared Standard7Bag projection', () => {
  const pc = {
    ...production.createDefaultWorkspaceRequest(),
    lines: 4,
    boardMask: 0n,
    queue: ''
  };
  assert.equal(production.workspaceValidationCodes(pc, 'web').includes('queue_invalid'), false);
  const pcTokens = commandTokens(production.buildWorkspaceCommand(pc));
  assert.equal(hasOption(pcTokens, '--queue'), false);
  assert.equal(hasOption(pcTokens, '--patterns'), false);
  const pcDesktop = production.workspaceRequestForDesktop(pc, 'en');
  assert.equal(pcDesktop.queue, '');
  assert.equal(pcDesktop.patterns, '');
  assert.equal(pcDesktop.tablebase_requested, false);

  const tablebase = production.workspaceRequestForDesktop(
    { ...pc, tablebaseEnabled: true },
    'en'
  );
  assert.equal(tablebase.tablebase_requested, true);

  const build = {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 1,
    existingMask: 0n,
    targetMask: 0xfn,
    queue: ''
  };
  assert.equal(production.buildProbabilityValidationCodes(build).includes('queue_invalid'), false);
  const buildTokens = commandTokens(production.buildProbabilityCommand(build));
  assert.equal(hasOption(buildTokens, '--queue'), false);
  assert.equal(hasOption(buildTokens, '--patterns'), false);
  const buildDesktop = production.buildProbabilityRequestForDesktop(build, 'en');
  assert.equal(buildDesktop.queue, '');
  assert.equal(buildDesktop.patterns, '');
});

test('build tiling canonicalizes fixture-derived singles and every ordered option-pair Cartesian value', () => {
  assertRiskInventory('build', BUILD_TILING_RISKS);
  let cases = 0;
  for (const risk of BUILD_TILING_RISKS) {
    for (const representative of risk.representatives) {
      assertBuildTilingProjection(
        applyRisk(buildTilingBase(), risk, representative),
        `single ${risk.option}=${representative}`
      );
      cases += 1;
    }
  }
  for (let left = 0; left < BUILD_TILING_RISKS.length; left += 1) {
    for (let right = left + 1; right < BUILD_TILING_RISKS.length; right += 1) {
      const leftRisk = BUILD_TILING_RISKS[left];
      const rightRisk = BUILD_TILING_RISKS[right];
      for (const leftValue of leftRisk.representatives) {
        for (const rightValue of rightRisk.representatives) {
          for (const [firstRisk, firstValue, secondRisk, secondValue] of [
            [leftRisk, leftValue, rightRisk, rightValue],
            [rightRisk, rightValue, leftRisk, leftValue]
          ]) {
            const request = applyRisk(
              applyRisk(buildTilingBase(), firstRisk, firstValue),
              secondRisk,
              secondValue
            );
            assertBuildTilingProjection(
              request,
              `${firstRisk.option}=${firstValue} then ${secondRisk.option}=${secondValue}`
            );
            cases += 1;
          }
        }
      }
    }
  }
  assert.ok(cases >= 100, `expected a substantive production matrix, got ${cases}`);
});

test('PC tiling canonicalizes fixture-derived high-risk singles and ordered option pairs', () => {
  assertRiskInventory('pc', PC_TILING_RISKS);
  let cases = 0;
  for (const risk of PC_TILING_RISKS) {
    for (const representative of risk.representatives) {
      assertPcTilingProjection(
        applyRisk(pcTilingBase(), risk, representative),
        `single ${risk.option}=${representative}`
      );
      cases += 1;
    }
  }
  for (let left = 0; left < PC_TILING_RISKS.length; left += 1) {
    for (let right = left + 1; right < PC_TILING_RISKS.length; right += 1) {
      const leftRisk = PC_TILING_RISKS[left];
      const rightRisk = PC_TILING_RISKS[right];
      for (const leftValue of leftRisk.representatives) {
        for (const rightValue of rightRisk.representatives) {
          for (const [firstRisk, firstValue, secondRisk, secondValue] of [
            [leftRisk, leftValue, rightRisk, rightValue],
            [rightRisk, rightValue, leftRisk, leftValue]
          ]) {
            const request = applyRisk(
              applyRisk(pcTilingBase(), firstRisk, firstValue),
              secondRisk,
              secondValue
            );
            assertPcTilingProjection(
              request,
              `${firstRisk.option}=${firstValue} then ${secondRisk.option}=${secondValue}`
            );
            cases += 1;
          }
        }
      }
    }
  }
  assert.ok(cases >= 100, `expected a substantive production matrix, got ${cases}`);
});
