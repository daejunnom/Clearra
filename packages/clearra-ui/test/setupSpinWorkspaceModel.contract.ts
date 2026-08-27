import assert from 'node:assert/strict';

import {
  buildSetupFinderCommand,
  buildSetupPathDetailCommand,
  createDefaultSetupFinderRequest
} from '../src/lib/workspace/setupFinderModel';
import {
  buildSetupScoreCommand,
  createDefaultSetupScoreRequest,
  setupScoreRequestForDesktop,
  setupScoreValidationCodes
} from '../src/lib/workspace/setupScoreModel';
import {
  EMPTY_SPIN_BOARD_MASK_V1,
  buildSpinStructureCommand,
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
  const detail = buildSetupPathDetailCommand(
    request,
    { setupId: 'setup-1', conditionId: 'hold-empty' },
    1
  );
  assert.match(detail, /^clearra setup-finder /u);
  if (priority === 'all') assert.doesNotMatch(detail, / --priority /u);
  else assert.match(detail, new RegExp(` --priority ${priority} `, 'u'));
}

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
assert.equal(setupScoreDesktop.command, 'setup-score');
assert.equal(setupScoreDesktop.setup_queue, 'I');
assert.equal(setupScoreDesktop.setup_patterns, '');
assert.equal(setupScoreDesktop.solution_queue, 'OTSJ');
assert.equal(setupScoreDesktop.backend, 'cpu');
assert.equal(setupScoreDesktop.allow_backend_fallback, false);
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
assert.equal(patternedDesktop.setup_queue, '');
assert.equal(patternedDesktop.setup_patterns, 'P1');

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
  assert.equal(desktop.command, 'spin-structure');
  assert.equal(desktop.capability_id, `spin-structure.${mode}`);
  assert.equal(desktop.backend, 'cpu');
  assert.equal(desktop.allow_backend_fallback, false);
  assert.equal('queue' in desktop, false);
  assert.equal('patterns' in desktop, false);
  assert.equal('max_memory_mib' in desktop, false);
  if (mode === 'search') {
    assert.equal('objective' in desktop, false);
    assert.equal('max_patterns' in desktop, false);
  } else if (mode === 'cover') {
    assert.match(command, / --objective min-cover --max-patterns 100000 /u);
    assert.equal(desktop.objective, 'min-cover');
    assert.equal('final_piece' in desktop, false);
  } else {
    assert.match(command, / --final-piece T --max-patterns 100000 --no-dependency-report /u);
    assert.equal(desktop.final_piece, 'T');
    assert.equal(desktop.dependency_report, false);
    assert.equal('objective' in desktop, false);
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

console.log(JSON.stringify({ setup_ranked_routes: 3, setup_score_routes: 1, spin_structure_routes: 3 }));
