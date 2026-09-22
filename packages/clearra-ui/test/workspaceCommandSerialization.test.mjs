import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  buildWorkspaceCommand,
  buildWorkspaceCommandArguments,
  createDefaultWorkspaceRequest,
  normalizeWorkspaceInitialField,
  scenarioPieceWindow,
  workspaceValidationCodes,
  workspaceRequestForDesktop
} from '../src/lib/workspace/solverWorkspaceModel.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from '../src/lib/workspace/cliCommandModel.ts';
import {
  buildProbabilityCommandArguments,
  buildProbabilityRequestForDesktop,
  createDefaultBuildProbabilityRequest
} from '../src/lib/workspace/buildProbabilityModel.ts';
import {
  boundaryRecoveryArguments,
  boundaryRecoveryCommand,
  boundaryRecoveryDesktopRequest,
  createBoundaryRecoveryRequest,
  validateBoundaryRecoveryRequest
} from '../src/lib/workspace/boundaryRecoveryModel.ts';
import { validateBoundaryRecoveryPayload } from '../src/lib/workspace/boundaryRecoveryPayloadValidation.ts';

const canonicalGuiPcFullSolutionArguments = readFileSync(
  new URL('../../../tests/fixtures/contracts/gui_pc_full_solution_argv.tsv', import.meta.url),
  'utf8'
).trimEnd().split('\t');

test('boundary recovery keeps one fixed queue and independent bag B2B choices across browser and Desktop', () => {
  const request = {
    ...createBoundaryRecoveryRequest(),
    initialBoardMask: 0x3f0n,
    targetBoardMask: 0xc030n,
    height: 4,
    queue: 'IO',
    stageOneCount: 1,
    placements: 2,
    maxEarlyPlacements: 1,
    borrowSourcePosition: 2,
    borrowPlacementMask: 0x300c000n,
    holdEnabled: false,
    preserveB2BStageOne: true,
    preserveB2BStageTwo: false
  };
  assert.deepEqual(validateBoundaryRecoveryRequest(request), []);
  const arguments_ = boundaryRecoveryArguments(request);
  assert.deepEqual(arguments_.slice(0, 3), ['clearra', 'recovery', 'boundary']);
  assert.deepEqual(arguments_.slice(arguments_.indexOf('--queue'), arguments_.indexOf('--queue') + 2), ['--queue', 'IO']);
  assert.deepEqual(arguments_.slice(arguments_.indexOf('--borrow-source-position'), arguments_.indexOf('--borrow-source-position') + 2), ['--borrow-source-position', '2']);
  assert.equal(arguments_.includes('--preserve-b2b-stage-one'), true);
  assert.equal(arguments_.includes('--preserve-b2b-stage-two'), false);
  assert.deepEqual(boundaryRecoveryDesktopRequest(request, 'ko').arguments, arguments_);
  assert.deepEqual(tokenizeBrowserCommandForContract(boundaryRecoveryCommand(request)), arguments_);
});
test('boundary recovery exact roles serialize every source mask without a duplicate borrow mask', () => {
  const request = {
    ...createBoundaryRecoveryRequest(),
    queue: 'IO', height: 4, placements: 2, stageOneCount: 1,
    borrowSourcePosition: 2, placementRoleMasks: [0xfn, 0x300c000n]
  };
  assert.deepEqual(validateBoundaryRecoveryRequest(request), []);
  const args = boundaryRecoveryArguments(request);
  assert.equal(args.includes('--borrow-placement-mask'), false);
  assert.deepEqual(args.flatMap((value, index) => value === '--role-mask' ? [args[index + 1]] : []),
    ['1:0x000000000000000f', '2:0x000000000300c000']);
  assert.deepEqual(boundaryRecoveryDesktopRequest(request, 'ko').arguments, args);
  assert.deepEqual(tokenizeBrowserCommandForContract(boundaryRecoveryCommand(request)), args);
  assert.deepEqual(validateBoundaryRecoveryRequest({ ...request, placementRoleMasks: [0xfn] }), ['placement-roles']);
});

test('boundary recovery pattern mode preserves full-bag roles and finite budgets in both hosts', () => {
  const request = {
    ...createBoundaryRecoveryRequest(),
    queue: 'IJLOSTZIJLOSTZ', queuePattern: 'IJLOSTZP7',
    height: 8, stageOneCount: 7, placements: 14,
    borrowSourcePosition: 8, maxEarlyPlacements: 0,
    placementRoleMasks: Array.from({ length: 14 }, () => 0xfn),
    maxPatternEvaluations: 2, maxTotalStates: 1000
  };
  assert.deepEqual(validateBoundaryRecoveryRequest(request), []);
  const args = boundaryRecoveryArguments(request);
  assert.deepEqual(args.slice(args.indexOf('--queue-pattern'), args.indexOf('--queue-pattern') + 2),
    ['--queue-pattern', 'IJLOSTZP7']);
  assert.deepEqual(args.slice(args.indexOf('--max-total-states'), args.indexOf('--max-total-states') + 2),
    ['--max-total-states', '1000']);
  assert.deepEqual(boundaryRecoveryDesktopRequest(request, 'ko').arguments, args);
  assert.deepEqual(tokenizeBrowserCommandForContract(boundaryRecoveryCommand(request)), args);
  assert.deepEqual(validateBoundaryRecoveryRequest({ ...request, placementRoleMasks: [] }), ['pattern-roles']);
  const alternatives = { ...request, queuePattern: 'IJLOSTZIJLOSTZ;IJLOSTZIIIIIII' };
  assert.match(boundaryRecoveryCommand(alternatives), /--queue-pattern "IJLOSTZIJLOSTZ;IJLOSTZIIIIIII"/u);
  assert.deepEqual(tokenizeBrowserCommandForContract(boundaryRecoveryCommand(alternatives)),
    boundaryRecoveryArguments(alternatives));
});

test('weighted recovery payload keeps unresolved supply distinct from proven no-path supply', () => {
  const payload = {
    contract: 'boundary-recovery.v1', result_kind: 'boundary-recovery',
    content: { payload_kind: 'boundary-recovery', payload: {
      status: 'population-incomplete', knowledge_basis: 'full-pattern-universe',
      placement_role_scope: 'bag-piece-exact-lock-time', max_early_placements: 0,
      borrow_source_index: 0, borrow_placement_mask: '0x0',
      normal_states: 0, recovery_states: 0, stage_one_checkpoint_step: null,
      checkpoint_is_pc: null, borrowed_stage_two_count: 0, steps: [],
      population: {
        materialized_pattern_count: 2, total_possible_pattern_count: '2',
        evaluated_pattern_count: 1, state_count: 10, complete: false,
        normal_count: 0, pc_preserving_recovery_count: 0, non_pc_recovery_count: 0,
        no_path_count: 1, incomplete_count: 0, diagram_unavailable_count: 0,
        normal_probability: '0.00000000000000000',
        pc_preserving_recovery_probability: '0.00000000000000000',
        non_pc_recovery_probability: '0.00000000000000000',
        additional_recovery_probability: '0.00000000000000000',
        total_response_probability: '0.00000000000000000',
        no_path_probability: '0.50000000000000000',
        unknown_probability: '0.50000000000000000'
      }
    } }
  };
  assert.equal(validateBoundaryRecoveryPayload(payload), null);
  assert.equal(validateBoundaryRecoveryPayload({
    ...payload, content: { ...payload.content, payload: {
      ...payload.content.payload,
      population: { ...payload.content.payload.population, no_path_count: 0 }
    } }
  }), 'invalid boundary recovery payload');
});
const canonicalGuiBuildProbabilityB2bArguments = readFileSync(
  new URL('../../../tests/fixtures/contracts/gui_build_probability_b2b_argv.tsv', import.meta.url),
  'utf8'
).trimEnd().split(/\r?\n/u).map((line) => line.split('\t'));

test('browser command text and Desktop argv preserve the same quoted queue field', () => {
  const queue = 'I O"\\T';
  const request = {
    ...createDefaultWorkspaceRequest(),
    queue
  };
  const expectedArguments = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'en');

  assert.equal(expectedArguments[expectedArguments.indexOf('--queue') + 1], queue);
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), expectedArguments);
  assert.deepEqual(desktopRequest.arguments, expectedArguments);
  assert.match(browserCommand, /--queue "I O\\"\\\\T"/u);
});

test('ordinary PC full solutions use one canonical argv envelope in browser and Desktop', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3fn,
    queue: '[I]',
    holdEnabled: false,
    scoreMode: 'off',
    backend: 'cpu',
    workers: 1
  };
  const arguments_ = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'en');

  assert.deepEqual(arguments_, canonicalGuiPcFullSolutionArguments);
  assert.equal(arguments_.includes('--cpu-warmup'), false,
    'asynchronously ready browser workers must not enter the native all-worker barrier');
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), arguments_);
  assert.deepEqual(desktopRequest.arguments, arguments_);
});

test('completed initial rows compact without shrinking the requested PC target frame', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    lines: 2,
    boardMask: 0x3ffffn,
    queue: 'IOT',
    holdEnabled: false,
    scoreMode: 'off',
    backend: 'cpu',
    workers: 1
  };
  const normalized = normalizeWorkspaceInitialField(request);

  assert.equal(normalized.clearedRows, 1);
  assert.equal(normalized.request.lines, 2);
  assert.equal(normalized.request.boardMask, 0xffn);
  assert.equal(scenarioPieceWindow(request), 3);
  assert.equal(workspaceValidationCodes(request, 'web').includes('scenario_not_tileable'), false);

  const arguments_ = buildWorkspaceCommandArguments(normalized.request);
  assert.equal(arguments_[arguments_.indexOf('--lines') + 1], '2');
  assert.equal(arguments_[arguments_.indexOf('--height') + 1], '2');
  assert.equal(arguments_[arguments_.indexOf('--pieces') + 1], '3');
  assert.equal(arguments_[arguments_.indexOf('--board-mask') + 1], '0x00000000000000ff');
  assert.deepEqual(
    tokenizeBrowserCommandForContract(buildWorkspaceCommand(normalized.request)),
    arguments_
  );
  assert.deepEqual(workspaceRequestForDesktop(normalized.request, 'en').arguments, arguments_);
});

test('minimum-cover GUI emits one canonical pc minimals command without a DTO count authority', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    lines: 2,
    boardMask: 0n,
    queue: 'IIOOO',
    holdEnabled: false,
    scoreMode: 'minimum-cover',
    backend: 'cpu',
    workers: 1
  };
  const arguments_ = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'ko');

  assert.deepEqual(arguments_.slice(0, 3), ['clearra', 'pc', 'minimals']);
  assert.equal(arguments_.includes('--count'), false);
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), arguments_);
  assert.deepEqual(desktopRequest.arguments, arguments_);
  assert.deepEqual(Object.keys(desktopRequest).sort(), [
    'app_request_model',
    'arguments',
    'command',
    'language'
  ]);
});

test('PC mandatory solutions stay bound to minimum-cover argv in browser and Desktop', () => {
  const key = 'ctk1|I:000003c0';
  const secondKey = 'ctk1|O:00000033';
  const base = {
    ...createDefaultWorkspaceRequest(),
    scoreMode: 'minimum-cover',
    pinnedSolutionKeys: [secondKey, key, secondKey]
  };
  const arguments_ = buildWorkspaceCommandArguments(base);
  assert.deepEqual(arguments_.slice(arguments_.indexOf('--pin-key')), [
    '--pin-key', secondKey, '--pin-key', key
  ]);
  assert.deepEqual(
    tokenizeBrowserCommandForContract(buildWorkspaceCommand(base)),
    arguments_
  );
  assert.deepEqual(workspaceRequestForDesktop(base, 'ko').arguments, arguments_);
  assert.equal(
    buildWorkspaceCommandArguments({ ...base, scoreMode: 'off' }).includes('--pin-key'),
    false
  );
});

test('queue-less Build minimum uses its finite standard bag as the sole source window', () => {
  const request = {
    ...createDefaultBuildProbabilityRequest(),
    height: 4,
    targetMask: 0xfn,
    sourcePieces: 0xffff_ffff,
    resultMode: 'minimum-solutions',
    workers: 1
  };
  const arguments_ = buildProbabilityCommandArguments(request);

  assert.deepEqual(arguments_.slice(0, 3), ['clearra', 'build', 'cover']);
  assert.equal(arguments_[arguments_.indexOf('--patterns') + 1], 'P2');
  assert.equal(arguments_.includes('--source-pieces'), false);
  assert.deepEqual(buildProbabilityRequestForDesktop(request, 'en').arguments, arguments_);
});

test('Build B2B emits the canonical serial and distributed WASM argv in browser and Desktop', () => {
  const base = {
    ...createDefaultBuildProbabilityRequest(),
    height: 4,
    existingMask: 0n,
    targetMask: 0xffffffffffn,
    queue: 'OTSZJLIOTI',
    holdEnabled: false,
    aggregation: 'buildability',
    preserveB2B: true,
    spinProfile: 't-spins'
  };

  for (const [index, workers] of [1, 2].entries()) {
    const request = { ...base, workers };
    const arguments_ = buildProbabilityCommandArguments(request);
    assert.deepEqual(arguments_, canonicalGuiBuildProbabilityB2bArguments[index]);
    assert.deepEqual(buildProbabilityRequestForDesktop(request, 'en').arguments, arguments_);
    assert.deepEqual(
      tokenizeBrowserCommandForContract(serializeCliCommandArguments(arguments_)),
      arguments_
    );
  }
});

test('browser command text and Desktop argv preserve literal process markers and C0 whitespace', () => {
  const comment = "literal | && ` $(x) > < ; &\tline\nquote\" slash\\ apostrophe'\u0007";
  const expectedArguments = [
    'clearra',
    'utility',
    'fumen',
    'text-to-fumen',
    '--format',
    'fumen',
    '--comment',
    comment
  ];
  const browserCommand = serializeCliCommandArguments(expectedArguments);
  const desktopRequest = cliCommandRequestForDesktop(expectedArguments, 'ko');

  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), expectedArguments);
  assert.deepEqual(desktopRequest.arguments, expectedArguments);
  assert.match(browserCommand, /--comment "/u);
  assert.throws(
    () => serializeCliCommandArguments([...expectedArguments, 'NUL\0value']),
    /must not contain NUL/u
  );
  assert.throws(
    () => cliCommandRequestForDesktop([...expectedArguments, 'NUL\0value'], 'en'),
    /must not contain NUL/u
  );
});

// Independent contract model for the browser WebCommandParser's closed quoted
// token grammar: only a quote or backslash may follow a quoted backslash.
function tokenizeBrowserCommandForContract(commandText) {
  const tokens = [];
  let token = '';
  let tokenStarted = false;
  let quoted = false;
  let escaped = false;
  for (const character of commandText) {
    if (quoted) {
      if (escaped) {
        assert.match(character, /["\\]/u);
        token += character;
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        quoted = false;
      } else {
        token += character;
      }
    } else if (character === '"') {
      quoted = true;
      tokenStarted = true;
    } else if (/\s/u.test(character)) {
      if (tokenStarted) {
        tokens.push(token);
        token = '';
        tokenStarted = false;
      }
    } else {
      token += character;
      tokenStarted = true;
    }
  }
  assert.equal(quoted || escaped, false);
  if (tokenStarted) tokens.push(token);
  return tokens;
}
