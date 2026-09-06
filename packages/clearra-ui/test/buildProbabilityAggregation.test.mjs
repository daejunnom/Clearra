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
        buildCoveragePortfolioSummary,
        buildProbabilityAggregationAuthority,
        buildProbabilityCoverageAggregation
      }
        from './src/lib/workspace/buildProbabilityAggregation.ts';
      export {
        buildProbabilityCommandArguments,
        buildProbabilityRequestForDesktop,
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

test('CTK3 P7 build benchmark preserves the existing field and completes the 4L final field', () => {
  // Existing ctk3_w0kCQBjwwAMPPAD37g -> final ctk3_w0kCQBAANGI.
  // The CLI target is only newly built cells, not the whole final field.
  const existingMask = 0x3c0f03c0fn;
  const finalMask = 0xffffffffffn;
  const request = {
    ...production.createDefaultBuildProbabilityRequest(),
    existingMask, targetMask: finalMask & ~existingMask,
    height: 4, queue: 'P7', workers: 11,
    resultMode: 'all-solutions', aggregation: 'buildability'
  };
  assert.deepEqual(production.buildProbabilityValidationCodes(request), []);
  const argv = production.buildProbabilityCommandArguments(request);
  const option = (name) => argv[argv.indexOf(name) + 1];
  assert.equal(BigInt(option('--base-mask')), existingMask);
  assert.equal(BigInt(option('--target-mask')), 0xfc3f0fc3f0n);
  assert.equal(BigInt(option('--base-mask')) | BigInt(option('--target-mask')), finalMask);
  assert.equal(option('--patterns'), 'P7');
  assert.equal(option('--workers'), '11');
  assert.ok(argv.includes('--no-mirror'));
  assert.ok(!argv.includes('--cpu-warmup'));
});

test('Build minimum renders the real typed CLI payload without a legacy search report', () => {
  // Captured contract shape from one-I Build cover, including the nominal CTS1 identity.
  const payload = {
    contract: 'build.cover', result_kind: 'build-coverage-portfolio.v2',
    content: { payload_kind: 'build-coverage-portfolio-v2', payload: {
      contract: 'build-coverage-portfolio.v2', objective: 'min-cover',
      probability_basis: 'normalized-solution-pattern-bitset-or-union',
      source_candidate_count: '2', selected_candidate_count: '1', pattern_count: '1',
      required_pattern_count: '1', union_probability: '1',
      normalized_solution_set_hash: 'cts1:ea2c4fa12ddc1b01',
      canonical_first_candidate_id: 'ctk1|initial=0000000000000000|placements=I:000000000000000f',
      completeness: { source_universe_complete: true, coverage_rows_complete: true,
        probability_weights_complete: true, exact_minimum_proven: true, query_bound: true },
      page_source_available: true, page_source_identity_sha256: 'e'.repeat(64)
    } }
  };
  assert.deepEqual(production.buildCoveragePortfolioSummary(payload), {
    sourceCandidateCount: 2, selectedCandidateCount: 1, patternCount: 1,
    successfulPatternCount: 1, successProbability: '1'
  });
  const invalid = structuredClone(payload);
  invalid.content.payload.completeness.exact_minimum_proven = false;
  assert.equal(production.buildCoveragePortfolioSummary(invalid), null);
  assert.equal(production.buildCoveragePortfolioSummary(null), null);
  const source = readFileSync(new URL('../src/lib/workspace/BuildProbabilityResult.svelte', import.meta.url), 'utf8');
  assert.match(source, /buildCoveragePortfolioSummary\(productResultPayload\)/u);
  assert.match(source, /buildCoverSummary\?\.successProbability \?\? authorizedCoverage/u);
});

test('Build replay and score guard occupied rows but retain GUI height and extended minimum', () => {
  const base = { ...production.createDefaultBuildProbabilityRequest(), targetMask: 0xfn, queue: 'I' };
  for (const resultMode of ['complete-replay-paths', 'field-average-score', 'fixed-queue-maximum-score', 'highest-score-minimum-set']) {
    const request = { ...base, resultMode };
    assert.deepEqual(production.buildProbabilityValidationCodes(request), []);
    const argv = production.buildProbabilityCommandArguments(request);
    assert.equal(argv[argv.indexOf('--height') + 1], '8');
    const error = resultMode === 'complete-replay-paths' ? 'build_paths_height_invalid' : 'build_score_height_invalid';
    assert.deepEqual(production.buildProbabilityValidationCodes({ ...request, targetMask: 0xfn << 60n }), [error]);
    assert.deepEqual(production.buildProbabilityValidationCodes({ ...request, existingMask: 1n << 60n }), [error]);
  }
  const minimum = { ...base, resultMode: 'minimum-solutions', targetMask: 0xfn << 60n };
  assert.deepEqual(production.buildProbabilityValidationCodes(minimum), []);
  const argv = production.buildProbabilityCommandArguments(minimum);
  assert.equal(argv[argv.indexOf('--height') + 1], '8');
});

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
  const all = production.buildProbabilityCommandArguments({
    ...base,
    resultMode: 'all-solutions'
  });
  assert.equal(all[all.indexOf('--result-mode') + 1], 'all-solutions');

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

  const minimumWithImplicitStandardBag = production.buildProbabilityCommandArguments({
    ...base,
    queue: '   ',
    resultMode: 'minimum-solutions'
  });
  assert.equal(
    minimumWithImplicitStandardBag[minimumWithImplicitStandardBag.indexOf('--patterns') + 1],
    'P2'
  );
  assert.equal(minimumWithImplicitStandardBag.includes('--queue'), false);

  const minimumWithExplicitSourceWindow = production.buildProbabilityCommandArguments({
    ...base,
    queue: '',
    sourcePieces: 0xffff_ffff,
    resultMode: 'minimum-solutions'
  });
  assert.equal(
    minimumWithExplicitSourceWindow[minimumWithExplicitSourceWindow.indexOf('--patterns') + 1],
    'P2'
  );

  const tenPieceMinimum = production.buildProbabilityCommandArguments({
    ...base,
    targetMask: (1n << 40n) - 1n,
    queue: '',
    resultMode: 'minimum-solutions'
  });
  assert.equal(tenPieceMinimum[tenPieceMinimum.indexOf('--patterns') + 1], 'P7P4');

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

  for (const arguments_ of [all, path, score, minimum, fixed, scoreMinimum, failed]) {
    assert.equal(arguments_.includes('--cpu-warmup'), false);
  }
  assert.deepEqual(
    production.buildProbabilityRequestForDesktop(
      { ...base, resultMode: 'all-solutions' },
      'en'
    ).arguments,
    all
  );
  assert.deepEqual(
    production.buildProbabilityRequestForDesktop(
      { ...base, resultMode: 'minimum-solutions' },
      'ko'
    ).arguments,
    minimum
  );
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
