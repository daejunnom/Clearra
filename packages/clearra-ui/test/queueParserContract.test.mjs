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
        parseBrowserQueueInput,
        workspaceValidationCodes,
        createDefaultWorkspaceRequest
      } from './src/lib/workspace/solverWorkspaceModel.ts';
      export { workspaceMessage } from './src/lib/workspace/workspaceI18n.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const corpus = readFileSync(
  new URL('../../../tests/fixtures/contracts/queue_parser_contract.tsv', import.meta.url),
  'utf8'
)
  .split(/\r?\n/u)
  .filter((line) => line && !line.startsWith('#'))
  .map((line) => {
    const [input, valid, canonical, kind, sequenceLength, error] = line.split('\t');
    assert.notEqual(error, undefined, `contract column count: ${line}`);
    return {
      input,
      valid: valid === 'true',
      canonical: canonical === '-' ? null : canonical,
      kind,
      sequenceLength: sequenceLength === '-' ? null : Number(sequenceLength),
      error: error === '-' ? null : error
    };
  });

test('browser production queue parser matches the shared TS/Rust corpus', () => {
  for (const row of corpus) {
    const parsed = production.parseBrowserQueueInput(row.input);
    assert.equal(Boolean(parsed), row.valid, row.input);
    assert.equal(parsed?.source ?? null, row.canonical, row.input);
    assert.equal(parsed?.kind ?? row.kind, row.kind, row.input);
    assert.equal(parsed?.sequenceLength ?? null, row.sequenceLength, row.input);
    assert.equal(parsed ? null : 'invalid', row.error, row.input);
  }
});

test('visible-seven minimum cover is rejected before workspace execution', () => {
  const request = {
    ...production.createDefaultWorkspaceRequest(),
    queue: 'P4',
    lines: 2,
    queueKnowledge: 'visible-7',
    scoreMode: 'minimum-cover'
  };
  const codes = production.workspaceValidationCodes(request, 'web');

  assert.ok(codes.includes('visible-seven-minimum-cover-unsupported'));
  assert.match(
    production.workspaceMessage('en', 'visible-seven-minimum-cover-unsupported'),
    /seven visible pieces/i
  );
  assert.match(
    production.workspaceMessage('ko', 'visible-seven-minimum-cover-unsupported'),
    /앞 7개/u
  );
  const controls = readFileSync(
    new URL('../src/lib/workspace/SearchControls.svelte', import.meta.url),
    'utf8'
  );
  const workspace = readFileSync(
    new URL('../src/lib/workspace/SolverWorkspace.svelte', import.meta.url),
    'utf8'
  );
  assert.match(controls, /\{#each validationCodes as code\}/u);
  assert.match(workspace, /if \(active \|\| validationCodes\.length\) return;/u);
});
