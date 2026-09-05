import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        buildProbabilityAggregationAuthority,
        buildProbabilityCoverageAggregation
      }
        from './src/lib/workspace/buildProbabilityAggregation.ts';
      export {
        buildProbabilityCommandArguments,
        buildProbabilityValidationCodes,
        createDefaultBuildProbabilityRequest
      }
        from './src/lib/workspace/buildProbabilityModel.ts';
    `,
    loader: 'ts',
    resolveDir: packageRoot
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

function report(...values) {
  return {
    summary_fields: values.map((value) => ['build_probability_aggregation', value])
  };
}

function coverageReport({
  aggregation = 'buildability',
  availability = 'available',
  complete = true,
  sourceRowCount = 2,
  patternCount = 4,
  successfulPatternCount = 3,
  failedPatternCount = 1,
  successProbability = '0.75',
  failedProbability = '0.25',
  materializedProbabilityMass = '1'
} = {}) {
  const summary = {
    build_probability_aggregation: aggregation,
    coverage_aggregation_contract: 'pattern-coverage-aggregation.v1',
    coverage_aggregation_availability: availability,
    coverage_aggregation_complete: String(complete),
    coverage_aggregation_source_row_count: String(sourceRowCount),
    materialized_pattern_count: String(patternCount),
    covered_pattern_count: String(successfulPatternCount),
    failed_pattern_count: String(failedPatternCount),
    coverage_probability: successProbability,
    failed_coverage_probability: failedProbability,
    materialized_probability_mass: materializedProbabilityMass,
    coverage_probability_denominator: 'full-materialized-pattern-universe',
    probability_complete: String(complete)
  };
  return {
    summary_fields: Object.entries(summary),
    coverage_calculated: aggregation !== 'tiling',
    probability_calculated: aggregation !== 'tiling',
    materialized_pattern_count: patternCount,
    covered_pattern_count: successfulPatternCount,
    coverage_probability: successProbability,
    probability_complete: complete
  };
}

test('the request snapshot owns aggregation until a terminal report exists', () => {
  assert.deepEqual(
    production.buildProbabilityAggregationAuthority(null, 'spin'),
    {
      state: 'pending',
      requested: 'spin',
      reported: null,
      effective: 'spin',
      reason: null
    }
  );
});

test('terminal aggregation is authorized only by one exact matching typed field', () => {
  for (const aggregation of ['buildability', 'tiling', 'spin']) {
    assert.deepEqual(
      production.buildProbabilityAggregationAuthority(report(aggregation), aggregation),
      {
        state: 'authorized',
        requested: aggregation,
        reported: aggregation,
        effective: aggregation,
        reason: null
      },
      aggregation
    );
  }

  assert.equal(
    production.buildProbabilityAggregationAuthority({ summary_fields: [] }, 'spin').reason,
    'missing-or-duplicate-result-aggregation'
  );
  assert.equal(
    production.buildProbabilityAggregationAuthority(report('spin', 'spin'), 'spin').reason,
    'missing-or-duplicate-result-aggregation'
  );
  assert.equal(
    production.buildProbabilityAggregationAuthority(report('unknown'), 'spin').reason,
    'invalid-result-aggregation'
  );
});

test('an earlier generation cannot be relabelled by later aggregation controls', () => {
  const previousGeneration = report('buildability');
  assert.deepEqual(
    production.buildProbabilityAggregationAuthority(previousGeneration, 'spin'),
    {
      state: 'rejected',
      requested: 'spin',
      reported: 'buildability',
      effective: null,
      reason: 'request-result-aggregation-mismatch'
    }
  );

  const resultSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityResult.svelte', import.meta.url),
    'utf8'
  );
  const workspaceSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityWorkspace.svelte', import.meta.url),
    'utf8'
  );
  assert.match(workspaceSource, /resultAggregation = executionRequest\.aggregation/u);
  assert.match(workspaceSource, /resultMode = executionRequest\.resultMode/u);
  assert.match(workspaceSource, /\{resultMode\}/u);
  assert.match(resultSource, /aggregationAuthority\.state === 'rejected'/u);
  assert.match(resultSource, /effectiveAggregation === 'spin'/u);
});

test('Build result modes lower to CLI-owned products without changing engine aggregation', () => {
  const base = {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 4,
    existingMask: 0n,
    targetMask: 0xfn,
    queue: 'I',
    aggregation: 'spin',
    preserveB2B: true,
    finesse: 'inputs'
  };
  const path = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'complete-replay-paths'
  });
  assert.deepEqual(path.slice(0, 2), ['clearra', 'build-probability']);
  assert.equal(path[path.indexOf('--result-mode') + 1], 'complete-replay-paths');
  assert.equal(path[path.indexOf('--aggregate') + 1], 'buildability');
  assert.equal(path.includes('--finesse'), false);

  const score = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'field-average-score',
    scoreProfile: 'guideline',
    initialB2B: 7
  });
  assert.equal(score[score.indexOf('--result-mode') + 1], 'field-average-score');
  assert.equal(score[score.indexOf('--score-profile') + 1], 'guideline');
  assert.equal(score[score.indexOf('--initial-b2b') + 1], '7');

  const minimum = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'minimum-solutions'
  });
  assert.deepEqual(minimum.slice(0, 3), ['clearra', 'build', 'cover']);
  assert.equal(minimum[minimum.indexOf('--objective') + 1], 'min-cover');
  assert.equal(minimum.includes('--aggregate'), false);
  assert.equal(minimum.includes('--result-mode'), false);

  const fixed = production.buildProbabilityCommandArguments({
    ...base,
    queue: 'IOT',
    resultMode: 'fixed-queue-maximum-score',
    scoreProfile: 'jstris-ultra',
    initialB2B: 2
  });
  assert.equal(fixed[fixed.indexOf('--result-mode') + 1], 'fixed-queue-maximum-score');
  assert.equal(fixed[fixed.indexOf('--score-profile') + 1], 'jstris-ultra');

  const scoreMinimum = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'highest-score-minimum-set'
  });
  assert.equal(
    scoreMinimum[scoreMinimum.indexOf('--result-mode') + 1],
    'highest-score-minimum-set'
  );

  const failed = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'failed-queues',
    failedPatternLimit: 37
  });
  assert.equal(failed[failed.indexOf('--result-mode') + 1], 'failed-queues');
  assert.equal(failed[failed.indexOf('--failed-count') + 1], '37');

  for (const arguments_ of [path, score, minimum, fixed, scoreMinimum, failed]) {
    assert.equal(arguments_.includes('--cpu-warmup'), false);
  }
});

test('Build compatibility matrix validates only active result inputs', () => {
  const request = {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 4,
    targetMask: 0xfn,
    queue: 'P7',
    initialB2B: Number.NaN,
    failedPatternLimit: Number.NaN
  };
  assert.deepEqual(production.buildProbabilityValidationCodes(request), []);
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...request,
      resultMode: 'fixed-queue-maximum-score'
    }).sort(),
    ['fixed_queue_required', 'initial_b2b_invalid']
  );
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...request,
      resultMode: 'failed-queues'
    }),
    ['failed_pattern_limit_invalid']
  );

  const controls = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityControls.svelte', import.meta.url),
    'utf8'
  );
  for (const mode of [
    'all-solutions',
    'complete-replay-paths',
    'minimum-solutions',
    'field-average-score',
    'fixed-queue-maximum-score',
    'highest-score-minimum-set',
    'failed-queues'
  ]) assert.match(controls, new RegExp(`value="${mode}"`, 'u'));
  assert.match(
    controls,
    /request\.resultMode === 'all-solutions' \? request\.aggregation : 'buildability'/u
  );
  const pager = readFileSync(
    new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url),
    'utf8'
  );
  assert.match(pager, /solutionKeys=\{coverageSolutionKeys\}/u);
  assert.doesNotMatch(pager, /<code>\{(?:member|winner)\.normalized_solution_key\}<\/code>/u);
  assert.match(pager, /exportKeySource=\{coverageExportKeySource\}/u);
});

test('Build projects the shared PC coverage aggregation without reconstructing it', () => {
  for (const aggregation of ['buildability', 'spin']) {
    assert.deepEqual(
      production.buildProbabilityCoverageAggregation(
        coverageReport({ aggregation }),
        aggregation
      ),
      {
        state: 'authorized',
        sourceRowCount: 2,
        patternCount: 4,
        successfulPatternCount: 3,
        failedPatternCount: 1,
        successProbability: '0.75',
        failedProbability: '0.25',
        complete: true,
        reason: null
      },
      aggregation
    );
  }

  assert.deepEqual(
    production.buildProbabilityCoverageAggregation(
      coverageReport({
        availability: 'incomplete',
        complete: false,
        successProbability: '0.5',
        failedProbability: '0.5'
      }),
      'buildability'
    ),
    {
      state: 'authorized',
      sourceRowCount: 2,
      patternCount: 4,
      successfulPatternCount: 3,
      failedPatternCount: 1,
      successProbability: '0.5',
      failedProbability: '0.5',
      complete: false,
      reason: null
    }
  );
});

test('GUI aggregation argv and terminal coverage stay bound end to end', () => {
  const request = {
    ...production.createDefaultBuildProbabilityRequest(),
    aggregation: 'spin',
    height: 4,
    existingMask: 0n,
    targetMask: 0xfn,
    queue: 'I'
  };
  const arguments_ = production.buildProbabilityCommandArguments(request);
  const aggregateIndex = arguments_.indexOf('--aggregate');
  assert.notEqual(aggregateIndex, -1);
  assert.equal(arguments_[aggregateIndex + 1], 'spin');

  const terminal = coverageReport({ aggregation: 'spin' });
  const authority = production.buildProbabilityAggregationAuthority(
    terminal,
    request.aggregation
  );
  assert.equal(authority.state, 'authorized');
  assert.equal(
    production.buildProbabilityCoverageAggregation(
      terminal,
      authority.effective
    ).state,
    'authorized'
  );

  const changedAfterStart = { ...request, aggregation: 'buildability' };
  assert.equal(
    production.buildProbabilityAggregationAuthority(
      terminal,
      changedAfterStart.aggregation
    ).reason,
    'request-result-aggregation-mismatch'
  );
});

test('tiling keeps the shared coverage surface explicitly not calculated', () => {
  const tiling = coverageReport({
    aggregation: 'tiling',
    availability: 'not-calculated',
    complete: false,
    successfulPatternCount: 0,
    failedPatternCount: 'not-calculated',
    successProbability: 'not-calculated',
    failedProbability: 'not-calculated'
  });
  assert.deepEqual(
    production.buildProbabilityCoverageAggregation(tiling, 'tiling'),
    {
      state: 'not-calculated',
      sourceRowCount: 2,
      patternCount: 4,
      reason: null
    }
  );
});

test('Build rejects malformed or cross-contract coverage summaries as one unit', () => {
  const missing = coverageReport();
  missing.summary_fields = missing.summary_fields.filter(
    ([key]) => key !== 'failed_pattern_count'
  );
  assert.equal(
    production.buildProbabilityCoverageAggregation(missing, 'buildability').reason,
    'missing-or-duplicate-coverage-field'
  );

  const duplicate = coverageReport();
  duplicate.summary_fields.push(['coverage_probability', '0.75']);
  assert.equal(
    production.buildProbabilityCoverageAggregation(duplicate, 'buildability').reason,
    'missing-or-duplicate-coverage-field'
  );

  const wrongContract = coverageReport();
  wrongContract.summary_fields = wrongContract.summary_fields.map(([key, value]) =>
    key === 'coverage_aggregation_contract' ? [key, 'foreign-contract.v1'] : [key, value]
  );
  assert.equal(
    production.buildProbabilityCoverageAggregation(wrongContract, 'buildability').reason,
    'invalid-coverage-contract'
  );

  const wrongCount = coverageReport({ failedPatternCount: 2 });
  assert.equal(
    production.buildProbabilityCoverageAggregation(wrongCount, 'buildability').reason,
    'coverage-result-mismatch'
  );

  const wrongPartition = coverageReport({ failedProbability: '0.2' });
  assert.equal(
    production.buildProbabilityCoverageAggregation(wrongPartition, 'buildability').reason,
    'coverage-result-mismatch'
  );

  const resultSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityResult.svelte', import.meta.url),
    'utf8'
  );
  assert.match(resultSource, /authorizedCoverage\?\.successProbability/u);
  assert.match(resultSource, /authorizedCoverage\.failedPatternCount/u);
  assert.match(resultSource, /coverageAggregation\.state === 'rejected'/u);
});
