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
      export { buildProbabilityAggregationAuthority }
        from './src/lib/workspace/buildProbabilityAggregation.ts';
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
  assert.match(resultSource, /aggregationAuthority\.state === 'rejected'/u);
  assert.match(resultSource, /effectiveAggregation === 'spin'/u);
});
