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

test('extended Build option combinations preserve complete supply and active rules on web and desktop', () => {
  // The reported P7P7 field lives above the compact six-row boundary. Checking
  // its ingress and longer sources does not enumerate any of their queues.
  const field = {
    height: 10,
    existingMask: 0xc0383f3fc7n,
    targetMask: 0xc120fc1fcfe3fff3fc7c0c038n
  };
  let combinations = 0;
  for (const queue of ['P7P7', 'P7P7P7', 'ITOLSZJITOLSZJITOLSZJ']) {
    for (const rule of ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']) {
      for (const spinProfile of ['t-spins', 't-spins-plus', 'all-mini', 'all-mini-plus', 'all-spin', 'all-spin-plus']) {
        for (const preserveB2B of [false, true]) {
          for (const holdEnabled of [false, true]) {
            for (const workers of [1, 8]) {
              for (const aggregation of ['buildability', 'spin']) {
                const request = validBuildRequest({ ...field, queue, rule, spinProfile,
                  preserveB2B, holdEnabled, workers, aggregation });
                const context = `${queue}/${rule}/${spinProfile}/${preserveB2B}/${holdEnabled}/${workers}/${aggregation}`;
                assert.deepEqual(production.buildProbabilityValidationCodes(request), [], context);
                const args = production.buildProbabilityCommandArguments(request);
                assert.equal(optionValue(args, queue.startsWith('P') ? '--patterns' : '--queue'), queue, context);
                assert.equal(optionValue(args, '--height'), '10', context);
                assert.equal(BigInt(optionValue(args, '--base-mask')), field.existingMask, context);
                assert.equal(BigInt(optionValue(args, '--target-mask')), field.targetMask, context);
                assert.equal(optionValue(args, '--source-pieces'), null, 'the full supplied queue remains authoritative');
                assert.equal(optionValue(args, '--rule'), rule, context);
                assert.equal(optionValue(args, '--spin-profile'),
                  aggregation === 'spin' || preserveB2B ? spinProfile : null, context);
                assert.equal(args.includes('--preserve-b2b'), preserveB2B, context);
                assert.equal(args.includes('--hold'), holdEnabled, context);
                assert.equal(args.includes('--no-hold'), !holdEnabled, context);
                assert.equal(optionValue(args, '--workers'), String(workers), context);
                assert.deepEqual(production.buildProbabilityRequestForDesktop(request, 'ko').arguments, args, context);
                combinations += 1;
              }
            }
          }
        }
      }
    }
  }
  assert.equal(combinations, 1440);
});

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

test('build source validation rejects lengths that the native compiler cannot execute', () => {
  const twoPieceTarget = validBuildRequest({ targetMask: 0xffn });

  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...twoPieceTarget,
      queue: 'I'
    }),
    ['source_queue_too_short']
  );
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...twoPieceTarget,
      sourcePieces: 1
    }),
    ['source_pieces_too_short']
  );
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...twoPieceTarget,
      queue: 'IO',
      sourcePieces: 3
    }),
    ['source_pieces_exceeds_queue']
  );
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...twoPieceTarget,
      queue: 'IO',
      sourcePieces: 2
    }),
    []
  );

  // Relationship checks wait for a valid, tileable target so the user sees
  // the structural target error before any derived supply error.
  assert.deepEqual(
    production.buildProbabilityValidationCodes({
      ...twoPieceTarget,
      targetMask: 0x7n,
      queue: 'I',
      sourcePieces: 1
    }),
    ['build_target_not_tileable']
  );
});

test('build source-pieces control retains its bounded optional input', () => {
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
  for (const resultMode of ['complete-replay-paths', 'minimum-solutions', 'field-average-score',
    'fixed-queue-maximum-score', 'highest-score-minimum-set', 'failed-queues']) {
    const draft = validBuildRequest({ resultMode, solutionProbabilities: true });
    assert.equal(draft.solutionProbabilities, true, resultMode);
    assert.equal(production.normalizeBuildProbabilityRequest(draft).solutionProbabilities, false, resultMode);
    assert.equal(production.buildProbabilityCommandArguments(draft).includes('--solution-probabilities'), false, resultMode);
    assert.equal(production.buildProbabilityRequestForDesktop(draft, 'en')
      .arguments.includes('--solution-probabilities'), false, resultMode);
  }
});
