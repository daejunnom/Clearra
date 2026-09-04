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
        BUILD_SOURCE_PIECES_MAX,
        BUILD_SOURCE_PIECES_MIN,
        buildProbabilityCommand,
        buildProbabilityCommandArguments,
        buildProbabilityRequestForDesktop,
        buildProbabilityValidationCodes,
        createDefaultBuildProbabilityRequest,
        normalizeBuildProbabilityRequest
      } from './src/lib/workspace/buildProbabilityModel.ts';
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

function validBuildRequest(change = {}) {
  return {
    ...production.createDefaultBuildProbabilityRequest(),
    height: 1,
    targetMask: 0xfn,
    ...change
  };
}

function optionValue(command, option) {
  const tokens = Array.isArray(command) ? command : command.split(/\s+/u);
  const index = tokens.indexOf(option);
  return index === -1 ? null : tokens[index + 1];
}

test('build source-pieces omission preserves the native automatic default', () => {
  const request = validBuildRequest();
  assert.equal(request.sourcePieces, null);
  assert.equal(optionValue(production.buildProbabilityCommand(request), '--source-pieces'), null);

  const desktop = production.buildProbabilityRequestForDesktop(request, 'en');
  assert.equal(desktop.app_request_model, 'clearra-cli/CommandRequest');
  assert.equal(desktop.command, 'cli');
  assert.deepEqual(desktop.arguments, production.buildProbabilityCommandArguments(request));
  assert.equal(desktop.arguments.includes('--source-pieces'), false);
});

test('build source-pieces lowers exactly on command and typed desktop boundaries', () => {
  for (const sourcePieces of [1, 2, production.BUILD_SOURCE_PIECES_MAX]) {
    const request = validBuildRequest({ sourcePieces });
    assert.equal(
      optionValue(production.buildProbabilityCommand(request), '--source-pieces'),
      String(sourcePieces)
    );
    assert.equal(
      optionValue(
        production.buildProbabilityRequestForDesktop(request, 'ko').arguments,
        '--source-pieces'
      ),
      String(sourcePieces)
    );
  }
});

test('tiling preserves, validates, and lowers the active source window', () => {
  const draft = validBuildRequest({ aggregation: 'tiling', sourcePieces: 2 });
  assert.equal(draft.sourcePieces, 2);
  assert.equal(production.normalizeBuildProbabilityRequest(draft).sourcePieces, 2);
  assert.equal(optionValue(production.buildProbabilityCommand(draft), '--source-pieces'), '2');
  assert.equal(
    optionValue(
      production.buildProbabilityRequestForDesktop(draft, 'en').arguments,
      '--source-pieces'
    ),
    '2'
  );
  assert.equal(
    production
      .buildProbabilityValidationCodes({ ...draft, sourcePieces: 0 })
      .includes('source_pieces_invalid'),
    true
  );
});

test('build source-pieces validation follows the portable positive native usize range', () => {
  assert.equal(production.BUILD_SOURCE_PIECES_MIN, 1);
  assert.equal(production.BUILD_SOURCE_PIECES_MAX, 4_294_967_295);

  for (const sourcePieces of [null, 1, 2, production.BUILD_SOURCE_PIECES_MAX]) {
    assert.equal(
      production
        .buildProbabilityValidationCodes(validBuildRequest({ sourcePieces }))
        .includes('source_pieces_invalid'),
      false,
      String(sourcePieces)
    );
  }
  for (const sourcePieces of [0, -1, 1.5, production.BUILD_SOURCE_PIECES_MAX + 1, NaN]) {
    assert.equal(
      production
        .buildProbabilityValidationCodes(validBuildRequest({ sourcePieces }))
        .includes('source_pieces_invalid'),
      true,
      String(sourcePieces)
    );
  }
});

test('build source-pieces control explains the automatic dependency and result effect', () => {
  const controls = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityControls.svelte', import.meta.url),
    'utf8'
  );
  assert.match(controls, /type="number"/u);
  assert.match(controls, /min=\{BUILD_SOURCE_PIECES_MIN\}/u);
  assert.match(controls, /max=\{BUILD_SOURCE_PIECES_MAX\}/u);
  assert.match(controls, /request\.sourcePieces \?\? ''/u);
  const sourcePiecesInput = controls.match(/<input\s+type="number"[\s\S]*?\/>/u)?.[0];
  assert.ok(sourcePiecesInput);
  assert.doesNotMatch(sourcePiecesInput, /disabled=/u);

  assert.equal(production.workspaceMessage('en', 'sourcePiecesAutomatic'), 'Automatic (target/hold)');
  assert.match(production.workspaceMessage('en', 'sourcePiecesHelp'), /target piece count and hold state/u);
  assert.match(production.workspaceMessage('en', 'sourcePiecesHelp'), /every aggregation, including tiling/u);
  assert.doesNotMatch(production.workspaceMessage('en', 'sourcePiecesHelp'), /not sent/u);
  assert.equal(production.workspaceMessage('ko', 'sourcePiecesAutomatic'), '자동 (목표/홀드)');
  assert.match(production.workspaceMessage('ko', 'sourcePiecesHelp'), /목표 미노 수와 홀드 상태/u);
  assert.match(production.workspaceMessage('ko', 'sourcePiecesHelp'), /타일링을 포함한 모든 집계 방식/u);
  assert.doesNotMatch(production.workspaceMessage('ko', 'sourcePiecesHelp'), /전송하지 않습니다/u);
});

test('build solution probabilities lower only when enabled and remain unavailable for tiling', () => {
  const defaultRequest = validBuildRequest();
  assert.equal(defaultRequest.solutionProbabilities, false);
  assert.doesNotMatch(production.buildProbabilityCommand(defaultRequest), /--solution-probabilities/u);
  assert.equal(
    production
      .buildProbabilityRequestForDesktop(defaultRequest, 'en')
      .arguments.includes('--solution-probabilities'),
    false
  );

  const enabled = validBuildRequest({ solutionProbabilities: true });
  assert.equal(
    production
      .buildProbabilityCommand(enabled)
      .split(/\s+/u)
      .filter((token) => token === '--solution-probabilities').length,
    1
  );
  assert.equal(
    production
      .buildProbabilityRequestForDesktop(enabled, 'ko')
      .arguments.includes('--solution-probabilities'),
    true
  );

  const tilingDraft = validBuildRequest({
    aggregation: 'tiling',
    solutionProbabilities: true
  });
  assert.equal(tilingDraft.solutionProbabilities, true);
  assert.equal(
    production.normalizeBuildProbabilityRequest(tilingDraft).solutionProbabilities,
    false
  );
  assert.doesNotMatch(production.buildProbabilityCommand(tilingDraft), /--solution-probabilities/u);
  assert.equal(
    production
      .buildProbabilityRequestForDesktop(tilingDraft, 'en')
      .arguments.includes('--solution-probabilities'),
    false
  );
});

test('build probability controls expose and gate the solution-probability switch', () => {
  const controls = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityControls.svelte', import.meta.url),
    'utf8'
  );
  const checkedMarker = 'checked={request.solutionProbabilities}';
  const checkedIndex = controls.indexOf(checkedMarker);
  assert.notEqual(checkedIndex, -1);
  const inputStart = controls.lastIndexOf('<input', checkedIndex);
  const inputEnd = controls.indexOf('/>', checkedIndex);
  assert.notEqual(inputStart, -1);
  assert.notEqual(inputEnd, -1);
  const solutionProbabilitiesInput = controls.slice(inputStart, inputEnd + 2);
  assert.match(solutionProbabilitiesInput, /disabled=\{request\.aggregation === 'tiling'\}/u);
  assert.match(solutionProbabilitiesInput, /solutionProbabilities:/u);
  assert.match(controls, /label\('solutionProbabilities'\)/u);
});
