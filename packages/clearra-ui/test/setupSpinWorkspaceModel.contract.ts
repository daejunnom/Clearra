import assert from 'node:assert/strict';

import {
  buildSetupFinderCommand,
  buildSetupFinderCommandArguments,
  buildSetupPathDetailCommand,
  buildSetupPathDetailCommandArguments,
  createDefaultSetupFinderRequest,
  setupFinderRequestForDesktop
} from '../src/lib/workspace/setupFinderModel';
import {
  buildSetupScoreCommand,
  buildSetupScoreCommandArguments,
  createDefaultSetupScoreRequest,
  setupScoreRequestForDesktop,
  setupScoreValidationCodes
} from '../src/lib/workspace/setupScoreModel';
import {
  EMPTY_SPIN_BOARD_MASK_V1,
  buildSpinStructureCommand,
  buildSpinStructureCommandArguments,
  createDefaultSpinStructureRequest,
  spinStructureRequestForDesktop,
  spinStructureValidationCodes,
  type SpinStructureMode
} from '../src/lib/workspace/spinStructureModel';

for (const [priority, route] of [
  ['all', 'joint'],
  ['build', 'build'],
  ['pc', 'pc']
] as const) {
  const request = { ...createDefaultSetupFinderRequest(), candidatePriority: priority };
  const command = buildSetupFinderCommand(request, 1);
  assert.match(command, new RegExp(`^clearra setup ${route} `, 'u'));
  assert.doesNotMatch(command, / --priority /u);
  const desktop = setupFinderRequestForDesktop(request, 'en', 1);
  assert.equal(desktop.app_request_model, 'clearra-cli/CommandRequest');
  assert.equal(desktop.command, 'cli');
  assert.deepEqual(desktop.arguments, buildSetupFinderCommandArguments(request, 1));
  const detail = buildSetupPathDetailCommand(
    request,
    { setupId: 'setup-1', conditionId: 'hold-empty' },
    1
  );
  assert.match(detail, /^clearra setup-finder /u);
  if (priority === 'all') assert.doesNotMatch(detail, / --priority /u);
  else assert.match(detail, new RegExp(` --priority ${priority} `, 'u'));
}

const spacedDetail = { setupId: 'setup id', conditionId: 'condition id' };
const setupDetailRequest = createDefaultSetupFinderRequest();
assert.match(
  buildSetupPathDetailCommand(setupDetailRequest, spacedDetail, 1),
  /--paths-for "setup id" --condition "condition id"$/u
);
assert.deepEqual(
  setupFinderRequestForDesktop(setupDetailRequest, 'ko', 1, spacedDetail).arguments,
  buildSetupPathDetailCommandArguments(setupDetailRequest, spacedDetail, 1)
);

const setupScore = {
  ...createDefaultSetupScoreRequest(),
  document: 'ctk3_test',
  workers: 1
};
assert.deepEqual(setupScoreValidationCodes(setupScore), []);
const setupScoreCommand = buildSetupScoreCommand(setupScore);
assert.match(setupScoreCommand, /^clearra setup score /u);
assert.match(setupScoreCommand, / --setup-queue I /u);
assert.match(setupScoreCommand, / --solution-queue OTSJ /u);
assert.match(setupScoreCommand, / --backend cpu --no-backend-fallback --workers 1$/u);
assert.doesNotMatch(setupScoreCommand, /attack|max-memory|gpu|fallback true/u);
const setupScoreDesktop = setupScoreRequestForDesktop(setupScore, 'ko');
assert.equal(setupScoreDesktop.app_request_model, 'clearra-cli/CommandRequest');
assert.equal(setupScoreDesktop.command, 'cli');
assert.equal(setupScoreDesktop.language, 'ko');
assert.deepEqual(setupScoreDesktop.arguments, buildSetupScoreCommandArguments(setupScore));
assert.equal(optionValue(setupScoreDesktop.arguments, '--setup-queue'), 'I');
assert.equal(optionValue(setupScoreDesktop.arguments, '--solution-queue'), 'OTSJ');
assert.equal(optionValue(setupScoreDesktop.arguments, '--backend'), 'cpu');
assert.equal(setupScoreDesktop.arguments.includes('--no-backend-fallback'), true);
assert.equal('max_memory_mib' in setupScoreDesktop, false);

const patternedScore = {
  ...setupScore,
  setupSourceKind: 'patterns' as const,
  setupSource: 'P1',
  solutionSourceKind: 'patterns' as const,
  solutionSource: 'P2'
};
assert.deepEqual(setupScoreValidationCodes(patternedScore), []);
assert.match(buildSetupScoreCommand(patternedScore), / --setup-patterns P1 /u);
const patternedDesktop = setupScoreRequestForDesktop(patternedScore, 'en');
assert.equal(patternedDesktop.arguments.includes('--setup-queue'), false);
assert.equal(optionValue(patternedDesktop.arguments, '--setup-patterns'), 'P1');

for (const mode of ['search', 'cover', 'guaranteed'] as const satisfies readonly SpinStructureMode[]) {
  const request = { ...createDefaultSpinStructureRequest(), mode, workers: 1 };
  assert.deepEqual(spinStructureValidationCodes(request), [], mode);
  const command = buildSpinStructureCommand(request);
  assert.match(command, new RegExp(`^clearra spin-structure ${mode} `, 'u'));
  assert.match(command, new RegExp(` --board-mask-v1 ${EMPTY_SPIN_BOARD_MASK_V1} `, 'u'));
  assert.match(command, / --pieces T /u);
  assert.match(command, / --workers 1$/u);
  assert.doesNotMatch(
    command,
    / --(?:queue|patterns|hold|no-hold|max-memory(?:-mib)?|gpu-device|tablebase|backend)(?: |$)/u
  );
  const desktop = spinStructureRequestForDesktop(request, 'en');
  assert.equal(desktop.app_request_model, 'clearra-cli/CommandRequest');
  assert.equal(desktop.command, 'cli');
  assert.deepEqual(desktop.arguments, buildSpinStructureCommandArguments(request));
  assert.equal(desktop.arguments.includes('--queue'), false);
  assert.equal(desktop.arguments.includes('--patterns'), false);
  assert.equal('max_memory_mib' in desktop, false);
  if (mode === 'search') {
    assert.equal(desktop.arguments.includes('--objective'), false);
    assert.equal(desktop.arguments.includes('--max-patterns'), false);
  } else if (mode === 'cover') {
    assert.match(command, / --objective min-cover --max-patterns 100000 /u);
    assert.equal(optionValue(desktop.arguments, '--objective'), 'min-cover');
    assert.equal(desktop.arguments.includes('--final-piece'), false);
  } else {
    assert.match(command, / --final-piece T --max-patterns 100000 --no-dependency-report /u);
    assert.equal(optionValue(desktop.arguments, '--final-piece'), 'T');
    assert.equal(desktop.arguments.includes('--no-dependency-report'), true);
    assert.equal(desktop.arguments.includes('--objective'), false);
  }
}

const exactZeroLine = { ...createDefaultSpinStructureRequest(), lines: '0' as const };
assert.deepEqual(spinStructureValidationCodes(exactZeroLine), []);
assert.match(buildSpinStructureCommand(exactZeroLine), / --lines 0 /u);

const invalidGuaranteed = {
  ...createDefaultSpinStructureRequest(),
  mode: 'guaranteed' as const,
  inventory: 'I',
  finalPiece: 'I' as const
};
assert.deepEqual(spinStructureValidationCodes(invalidGuaranteed), ['final_piece_invalid']);

function optionValue(arguments_: readonly string[], option: string): string | undefined {
  const index = arguments_.indexOf(option);
  return index < 0 ? undefined : arguments_[index + 1];
}

console.log(JSON.stringify({ setup_ranked_routes: 3, setup_score_routes: 1, spin_structure_routes: 3 }));
